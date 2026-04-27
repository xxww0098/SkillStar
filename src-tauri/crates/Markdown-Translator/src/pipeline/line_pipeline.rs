//! Line-based translation pipeline following rockbenben's approach.
//!
//! Key insight: the current byte-range replacement in `mapper::apply()` breaks
//! when the LLM returns translations of different lengths than the originals.
//! The byte offsets of subsequent segments become invalid after each replacement.
//!
//! rockbenben's approach:
//! 1. Replace non-translatable elements with placeholders
//! 2. Split each line into translatable fragments (with source positions)
//! 3. Translate ALL fragments in ONE batch
//! 4. Map translations back by fragment position
//! 5. Restore placeholders
//!
//! This avoids byte-range replacement entirely — translations are applied
//! by fragment/index position, not by computed offsets.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, warn};

use crate::error::{Error, Result};
use crate::parser::protector;
use crate::provider::LlmProvider;
use crate::types::{DocumentContext, NodeType, PipelineMode, Segment};

/// A translatable fragment extracted from a line of text.
/// Tracks the original text and its position within the source line.
#[derive(Debug, Clone)]
pub struct TextFragment {
    /// Unique ID for this fragment across all segments.
    pub fragment_id: String,
    /// The actual text content to translate.
    pub text: String,
    /// Position of this fragment within its source line (character index).
    pub start_offset: usize,
    /// End position (exclusive) within the source line.
    pub end_offset: usize,
    /// The segment this fragment belongs to.
    pub segment_id: String,
    /// Line number within the segment (0 = first line).
    pub line_index: usize,
}

/// Metadata for a segment's line range in the source document.
#[derive(Debug, Clone)]
pub struct SegmentLineMeta {
    pub segment_id: String,
    pub line_start: usize,
    pub line_end: usize,
    pub node_type: NodeType,
}

/// The result of extracting fragments from a document's worth of segments.
#[derive(Debug, Clone)]
pub struct ExtractedFragments {
    /// All translatable fragments in document order.
    pub fragments: Vec<TextFragment>,
    /// Map from segment_id to its line metadata.
    pub segment_metas: HashMap<String, SegmentLineMeta>,
    /// Original lines grouped by line number.
    pub lines: Vec<String>,
}

/// Result of translating a batch of fragments.
#[derive(Debug, Clone)]
pub struct FragmentTranslationBatch {
    pub translations: Vec<(String, String)>, // (fragment_id, translated_text)
}

/// The line-based translation pipeline.
pub struct LinePipeline {
    provider: Arc<dyn LlmProvider>,
}

impl LinePipeline {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Run the full line-based translation pipeline on a document's segments.
    pub async fn translate_document(
        &self,
        segments: &[Segment],
        context: &DocumentContext,
        target_lang: &str,
        mode: PipelineMode,
    ) -> Result<Vec<String>> {
        if segments.is_empty() {
            return Ok(vec![]);
        }

        let extracted = self.extract_fragments(segments);
        info!(
            segments = segments.len(),
            lines = extracted.lines.len(),
            fragments = extracted.fragments.len(),
            "LinePipeline: extracted"
        );

        if extracted.fragments.is_empty() {
            return Ok(extracted.lines);
        }

        let batch = self
            .translate_batch(&extracted, context, target_lang, mode)
            .await?;
        let translated_lines = self.apply_translations(&extracted, &batch);
        let restored_lines = self.restore_placeholders(segments, &translated_lines);

        Ok(restored_lines)
    }

    /// Extract all translatable fragments from segments, grouping by line.
    pub fn extract_fragments(&self, segments: &[Segment]) -> ExtractedFragments {
        extract_fragments_impl(segments)
    }

    /// Translate all fragments in a single batch call.
    async fn translate_batch(
        &self,
        extracted: &ExtractedFragments,
        context: &DocumentContext,
        target_lang: &str,
        _mode: PipelineMode,
    ) -> Result<FragmentTranslationBatch> {
        if extracted.fragments.is_empty() {
            return Ok(FragmentTranslationBatch {
                translations: vec![],
            });
        }

        let (system, user) = self.build_batch_prompt(extracted, context, target_lang);

        let response = self
            .provider
            .chat_text(&system, &user, "line_translate")
            .await
            .map_err(|e| Error::Pipeline(format!("batch translation failed: {e}")))?;

        let translations = self.parse_batch_response(&response, &extracted.fragments)?;
        Ok(FragmentTranslationBatch { translations })
    }

    fn build_batch_prompt(
        &self,
        extracted: &ExtractedFragments,
        context: &DocumentContext,
        target_lang: &str,
    ) -> (String, String) {
        let system = format!(
            r#"You are a precise text translation specialist.

TRANSLATION RULES:
- Translate text fragments to {target_lang} accurately and concisely.
- Preserve all placeholder markers EXACTLY as-is ({{PH1}}, {{CODE_0}}, etc.).
- Do NOT add explanations, notes, or any commentary.
- Return ONLY the translation for each fragment, one per line.
- Keep the fragment_id prefix exactly as provided.

OUTPUT FORMAT:
For each fragment, return a single line:
FRAGMENT|fragment_id|translated_text"#,
        );

        let mut user = String::new();
        user.push_str("=== DOCUMENT CONTEXT ===\n\n");
        if !context.title.is_empty() {
            user.push_str(&format!("Title: {}\n\n", truncate(&context.title, 120)));
        }
        if !context.audience.is_empty() {
            user.push_str(&format!("Target audience: {}\n\n", &context.audience));
        }
        user.push_str("=== FRAGMENTS TO TRANSLATE ===\n\n");
        user.push_str(&format!(
            "Translate the following {} fragments to {}:\n\n",
            extracted.fragments.len(),
            target_lang
        ));

        for frag in &extracted.fragments {
            user.push_str(&format!(
                "FRAGMENT|{}|{}\n",
                frag.fragment_id,
                escape_pipe(frag.text.as_str())
            ));
        }

        user.push_str("\n=== END OF FRAGMENTS ===\n");
        user.push_str("Return each translated fragment on a new line using FRAGMENT|fragment_id|translated_text format.");

        (system, user)
    }

    fn parse_batch_response(
        &self,
        response: &str,
        fragments: &[TextFragment],
    ) -> Result<Vec<(String, String)>> {
        let mut translations = Vec::new();
        let frag_ids: Vec<&str> = fragments.iter().map(|f| f.fragment_id.as_str()).collect();

        for line in response.lines() {
            let line = line.trim();
            if !line.starts_with("FRAGMENT|") {
                continue;
            }

            // Parse: FRAGMENT|fragment_id|translated_text
            if let Some(second_pipe) = line[9..].find('|') {
                let frag_id = &line[9..9 + second_pipe];
                let translated = &line[9 + second_pipe + 1..];

                if !frag_ids.contains(&frag_id) {
                    warn!(
                        fragment_id = frag_id,
                        "unknown fragment ID in response, skipping"
                    );
                    continue;
                }

                translations.push((frag_id.to_string(), translated.to_string()));
            }
        }

        if translations.len() != fragments.len() {
            return Err(Error::Pipeline(format!(
                "expected {} translations, got {}",
                fragments.len(),
                translations.len()
            )));
        }

        Ok(translations)
    }

    /// Apply translated fragments back to lines to produce translated lines.
    fn apply_translations(
        &self,
        extracted: &ExtractedFragments,
        batch: &FragmentTranslationBatch,
    ) -> Vec<String> {
        let frag_map: HashMap<&str, &str> = batch
            .translations
            .iter()
            .map(|(id, text)| (id.as_str(), text.as_str()))
            .collect();

        let mut frag_by_line: HashMap<(String, usize), Vec<&TextFragment>> = HashMap::new();
        for frag in &extracted.fragments {
            frag_by_line
                .entry((frag.segment_id.clone(), frag.line_index))
                .or_default()
                .push(frag);
        }

        let mut translated_lines = extracted.lines.clone();

        for (i, line) in translated_lines.iter_mut().enumerate() {
            let (segment_id, line_idx) = self.find_segment_line(i, &extracted.segment_metas);

            if let Some(fragments) = frag_by_line.get(&(segment_id.clone(), line_idx)) {
                if !fragments.is_empty() {
                    let mut new_line = line.clone();
                    let mut sorted_frags: Vec<_> = fragments.iter().collect();
                    sorted_frags.sort_by(|a, b| b.start_offset.cmp(&a.start_offset));

                    for frag in sorted_frags {
                        if let Some(&translated) = frag_map.get(frag.fragment_id.as_str()) {
                            new_line.replace_range(frag.start_offset..frag.end_offset, translated);
                        }
                    }
                    *line = new_line;
                }
            }
        }

        translated_lines
    }

    fn find_segment_line(
        &self,
        doc_line: usize,
        metas: &HashMap<String, SegmentLineMeta>,
    ) -> (String, usize) {
        for meta in metas.values() {
            if doc_line >= meta.line_start && doc_line <= meta.line_end {
                return (meta.segment_id.clone(), doc_line - meta.line_start);
            }
        }
        (String::new(), 0)
    }

    /// Restore protected placeholders in translated lines using segment metadata.
    fn restore_placeholders(&self, segments: &[Segment], lines: &[String]) -> Vec<String> {
        let seg_by_id: HashMap<&str, &Segment> = segments
            .iter()
            .map(|s| (s.segment_id.as_str(), s))
            .collect();

        let mut seg_ranges: HashMap<&str, (usize, usize)> = HashMap::new();
        let mut current_line = 0usize;
        for seg in segments {
            let line_count = seg.source_text.lines().count();
            seg_ranges.insert(
                seg.segment_id.as_str(),
                (current_line, current_line + line_count - 1),
            );
            current_line += line_count;
        }

        let mut result = lines.to_vec();

        for (seg_id, seg) in &seg_by_id {
            if let Some(&(start, end)) = seg_ranges.get(seg_id) {
                if start >= result.len() {
                    continue;
                }
                let end = end.min(result.len() - 1);

                let seg_lines: Vec<String> = result[start..=end].to_vec();
                let joined = seg_lines.join("\n");
                let restored = protector::restore(&joined, seg);
                let restored_lines: Vec<&str> = restored.lines().collect();

                for (i, line_idx) in (start..=end).enumerate() {
                    if i < restored_lines.len() {
                        result[line_idx] = restored_lines[i].to_string();
                    }
                }
            }
        }

        result
    }
}

// ============================================================================
// Free functions (usable without LinePipeline instance)
// ============================================================================

/// Extract all translatable fragments from segments (free function version).
fn extract_fragments_impl(segments: &[Segment]) -> ExtractedFragments {
    let mut all_lines: Vec<String> = Vec::new();
    let mut all_fragments: Vec<TextFragment> = Vec::new();
    let mut segment_metas: HashMap<String, SegmentLineMeta> = HashMap::new();
    let mut global_line_offset = 0usize;

    for seg in segments {
        let seg_lines: Vec<&str> = seg.source_text.lines().collect();
        let num_lines = seg_lines.len();

        let protected_spans = &seg.protected_spans;
        let mut counter = 0usize;

        for (line_idx, line) in seg_lines.iter().enumerate() {
            let line_str = line.to_string();
            all_lines.push(line_str.clone());

            let line_frags = extract_line_fragments_impl(
                line_str.as_str(),
                &seg.segment_id,
                line_idx,
                &mut counter,
                protected_spans,
            );

            for mut frag in line_frags {
                frag.fragment_id = format!(
                    "{}-{}-{}",
                    seg.segment_id,
                    line_idx,
                    frag.text.chars().take(20).collect::<String>()
                );
                frag.segment_id = seg.segment_id.clone();
                frag.line_index = line_idx;
                all_fragments.push(frag);
            }
        }

        segment_metas.insert(
            seg.segment_id.clone(),
            SegmentLineMeta {
                segment_id: seg.segment_id.clone(),
                line_start: global_line_offset,
                line_end: global_line_offset + num_lines.saturating_sub(1),
                node_type: seg.node_type.clone(),
            },
        );

        global_line_offset += num_lines;
    }

    ExtractedFragments {
        fragments: all_fragments,
        segment_metas,
        lines: all_lines,
    }
}

/// Extract translatable fragments from a single line (free function version).
fn extract_line_fragments_impl(
    line: &str,
    segment_id: &str,
    line_index: usize,
    counter: &mut usize,
    protected_spans: &[crate::types::ProtectedSpan],
) -> Vec<TextFragment> {
    use crate::types::SpanType;

    let mut fragments = Vec::new();
    let line = line.trim_end_matches('\r');

    let protected_ranges: Vec<(usize, usize, &str)> = protected_spans
        .iter()
        .filter(|s| s.span_type != SpanType::Md)
        .filter_map(|s| {
            line.find(&s.placeholder)
                .map(|start| (start, start + s.placeholder.len(), s.original.as_str()))
        })
        .collect();

    let is_first_line = line_index == 0;
    let prefix_len = if is_first_line {
        detect_prefix_len(line)
    } else {
        0
    };

    if prefix_len > 0 {
        let content = &line[prefix_len..];
        if !content.trim().is_empty() {
            let translatable = isolate_translatable_impl(content, &protected_ranges, prefix_len);
            for (text, start, end) in translatable {
                if !text.trim().is_empty() {
                    *counter += 1;
                    fragments.push(TextFragment {
                        fragment_id: format!("{}-{}-{}", segment_id, line_index, *counter),
                        text,
                        start_offset: start,
                        end_offset: end,
                        segment_id: segment_id.to_string(),
                        line_index,
                    });
                }
            }
        }
    } else {
        let translatable = isolate_translatable_impl(line, &protected_ranges, 0);
        for (text, start, end) in translatable {
            if !text.trim().is_empty() {
                *counter += 1;
                fragments.push(TextFragment {
                    fragment_id: format!("{}-{}-{}", segment_id, line_index, *counter),
                    text,
                    start_offset: start,
                    end_offset: end,
                    segment_id: segment_id.to_string(),
                    line_index,
                });
            }
        }
    }

    fragments
}

/// Detect prefix length for a markdown line (heading, list, blockquote).
fn detect_prefix_len(line: &str) -> usize {
    // Count consecutive hash characters at start
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if hashes > 0 {
        // Must be followed by a space to be a heading
        let after_hashes = &line[hashes..];
        if after_hashes.starts_with(' ') {
            return hashes + 1; // include the trailing space
        }
        return 0; // like "##not a heading" (no space)
    }

    // Blockquote: > followed by space
    if line.starts_with("> ") {
        return 2;
    }
    // Unordered list: - /*/+ followed by space
    if let Some(rest) = line
        .strip_prefix('-')
        .or_else(|| line.strip_prefix('*'))
        .or_else(|| line.strip_prefix('+'))
    {
        if rest.starts_with(' ') || rest.is_empty() {
            return 1 + rest.len() - rest.trim_start_matches(' ').len();
        }
    }
    // Ordered list: digit(s) followed by . and space
    let stripped = line.trim_start();
    if !stripped.is_empty() && stripped.chars().next().unwrap().is_ascii_digit() {
        if let Some(rest) = stripped.strip_prefix(|c: char| c.is_ascii_digit()) {
            if rest.starts_with(". ") {
                return line.len() - stripped.len() + rest.len();
            }
        }
    }
    0
}

/// Isolate translatable content excluding protected ranges (free function version).
fn isolate_translatable_impl(
    text: &str,
    protected_ranges: &[(usize, usize, &str)],
    base_offset: usize,
) -> Vec<(String, usize, usize)> {
    if protected_ranges.is_empty() {
        if text.trim().is_empty() {
            return vec![];
        }
        return vec![(text.to_string(), base_offset, base_offset + text.len())];
    }

    let mut result = Vec::new();
    let mut search_from = 0usize;

    let mut sorted = protected_ranges.to_vec();
    sorted.sort_by_key(|r| r.0);

    for &(start, end, _) in &sorted {
        if start > search_from {
            let chunk = &text[search_from..start];
            if !chunk.trim().is_empty() {
                result.push((
                    chunk.to_string(),
                    base_offset + search_from,
                    base_offset + start,
                ));
            }
        }
        search_from = end;
    }

    if search_from < text.len() {
        let chunk = &text[search_from..];
        if !chunk.trim().is_empty() {
            result.push((
                chunk.to_string(),
                base_offset + search_from,
                base_offset + text.len(),
            ));
        }
    }

    result
}

fn escape_pipe(s: &str) -> String {
    s.replace('|', "{{PIPE}}")
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        s.chars().take(max_len).collect::<String>() + "..."
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_segment(text: &str, segment_id: &str, node_type: NodeType) -> Segment {
        Segment {
            segment_id: segment_id.into(),
            node_type,
            source_text: text.into(),
            context_path: vec![],
            byte_range: 0..text.len(),
            line_start: 0,
            line_end: text.lines().count().saturating_sub(1),
            protected_spans: vec![],
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn detect_heading_prefix() {
        // Use position of first space as prefix length
        assert_eq!(detect_prefix_len("## Hello World"), 3); // "## "
        assert_eq!(detect_prefix_len("# Title"), 2); // "# "
        assert_eq!(detect_prefix_len("> A quote"), 2); // "> "
        assert_eq!(detect_prefix_len("- item"), 2); // "- "
        assert_eq!(detect_prefix_len("Just text"), 0);
    }

    #[test]
    fn isolate_translatable_basic() {
        let result = isolate_translatable_impl("Hello world", &[], 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "Hello world");

        // {{CODE_0}} is 10 chars (positions 6-16)
        let result = isolate_translatable_impl("Hello {{CODE_0}} world", &[(6, 16, "`code`")], 0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "Hello ");
        assert_eq!(result[1].0, " world");
    }

    #[test]
    fn isolate_translatable_multiple_protected() {
        // "world" at positions 12-17 (5 chars)
        let protected = &[(0, 5, "Hello"), (12, 17, "world")];
        let result = isolate_translatable_impl("Hello cruel world", protected, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, " cruel ");
    }

    #[test]
    fn extract_fragments_impl_single_segment() {
        let seg = make_segment(
            "## Hello World\n\nThis is a paragraph.",
            "seg-1",
            NodeType::Paragraph,
        );
        let extracted = extract_fragments_impl(&[seg]);

        // 3 lines: "## Hello World", "", "This is a paragraph."
        assert_eq!(extracted.lines.len(), 3);
        // Should have fragments from translatable content
        assert!(!extracted.fragments.is_empty());
    }

    #[test]
    fn extract_fragments_impl_preserves_heading_structure() {
        let seg = make_segment("## Hello World", "seg-1", NodeType::Heading);
        let extracted = extract_fragments_impl(&[seg]);

        // 1 line
        assert_eq!(extracted.lines.len(), 1);
        // Should have one fragment for "Hello World"
        let heading_frags: Vec<_> = extracted
            .fragments
            .iter()
            .filter(|f| f.segment_id == "seg-1")
            .collect();
        assert!(!heading_frags.is_empty());
    }

    #[test]
    fn segment_line_meta_mapping() {
        let seg1 = make_segment("Line 1\nLine 2\nLine 3", "s1", NodeType::Paragraph);
        let seg2 = make_segment("Line A\nLine B", "s2", NodeType::Paragraph);

        let extracted = extract_fragments_impl(&[seg1.clone(), seg2.clone()]);

        // 3 + 2 = 5 lines
        assert_eq!(extracted.lines.len(), 5);

        let meta_s1 = extracted.segment_metas.get("s1").unwrap();
        assert_eq!(meta_s1.line_start, 0);
        assert_eq!(meta_s1.line_end, 2);

        let meta_s2 = extracted.segment_metas.get("s2").unwrap();
        assert_eq!(meta_s2.line_start, 3);
        assert_eq!(meta_s2.line_end, 4);
    }

    #[test]
    fn truncate_function() {
        assert_eq!(truncate("short", 10), "short");
        // take max_len chars + "..." = max_len + 3 total
        assert_eq!(truncate("this is long", 7), "this is...");
        // For CJK: take max_len chars, append "..."
        assert_eq!(truncate("日本語テスト", 4), "日本語テ...");
    }

    #[test]
    fn escape_pipe_helper() {
        assert_eq!(escape_pipe("a|b|c"), "a{{PIPE}}b{{PIPE}}c");
        assert_eq!(escape_pipe("no pipes"), "no pipes");
    }

    #[test]
    fn fragment_translation_batch_empty() {
        let batch = FragmentTranslationBatch {
            translations: vec![],
        };
        assert!(batch.translations.is_empty());
    }
}
