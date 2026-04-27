use crate::config::TranslatorConfig;
use crate::types::{DocumentContext, NodeType, Segment, SegmentBundle};

/// Line-based semantic shard with explicit line range metadata.
#[derive(Debug, Clone)]
pub struct SemanticShard {
    pub shard_id: String,
    pub segments: Vec<Segment>,
    pub line_start: usize,
    pub line_end: usize,
    pub summary_before: String,
    pub summary_after: String,
}

impl SemanticShard {
    /// Returns the number of lines covered by this shard (inclusive range).
    pub fn line_count(&self) -> usize {
        self.line_end
            .saturating_sub(self.line_start)
            .saturating_add(1)
    }

    /// Returns true if this shard's line range overlaps with another.
    pub fn overlaps(&self, other: &SemanticShard) -> bool {
        self.line_start < other.line_end && other.line_start < self.line_end
    }
}

/// Builds line-based semantic shards from segments.
///
/// Strategy:
/// - Each shard is bounded by line range (line_start..line_end)
/// - Shard breaks prefer semantic boundaries: headings, blockquotes, table cells
/// - Size limits (chars, segments) are soft constraints - we break at semantic
///   boundaries even if slightly over limit, but never start a shard over limit
/// - Shards track their line range for incremental/integrity validation
pub fn build_semantic_shards(
    segments: &[Segment],
    context: &DocumentContext,
    config: &TranslatorConfig,
) -> Vec<SemanticShard> {
    let max_chars = config.segmentation.max_bundle_chars;
    let max_segments = config.segmentation.max_bundle_segments;

    let mut shards: Vec<SemanticShard> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    let mut current_chars: usize = 0;
    let mut current_line_start: Option<usize> = None;
    let mut current_line_end: usize = 0;

    for segment in segments {
        let seg_size = estimate_translatable_size(segment);
        let seg_line_start = segment.line_start;
        let seg_line_end = segment.line_end;

        let would_be_oversized = current_chars + seg_size > max_chars;
        let at_segment_limit = current.len() >= max_segments;

        // Determine if this segment is a semantic break point.
        // Semantic breaks are preferred cut points even if slightly oversized.
        let is_semantic_break = matches!(
            segment.node_type,
            NodeType::Heading | NodeType::Blockquote | NodeType::TableCell
        );

        let should_flush = !current.is_empty()
            && (at_segment_limit
                || (would_be_oversized && (is_semantic_break || current_chars > max_chars / 2)));

        if should_flush {
            // Only flush if we're at a semantic boundary OR current is > half full.
            // If we haven't reached half the size limit and not at semantic boundary, keep adding.
            if is_semantic_break || current_chars > max_chars / 2 {
                flush_shard(
                    &mut shards,
                    &mut current,
                    &mut current_chars,
                    current_line_start.take(),
                    current_line_end,
                    context,
                );
            }
        }

        // Start tracking line range if this is the first segment in shard
        if current.is_empty() {
            current_line_start = Some(seg_line_start);
        }
        current_line_end = seg_line_end;

        current.push(segment.clone());
        current_chars += seg_size;
    }

    // Flush remaining segments
    if !current.is_empty() {
        flush_shard(
            &mut shards,
            &mut current,
            &mut current_chars,
            current_line_start.take(),
            current_line_end,
            context,
        );
    }

    shards
}

/// Flushes the current accumulated segments into a SemanticShard.
fn flush_shard(
    shards: &mut Vec<SemanticShard>,
    current: &mut Vec<Segment>,
    current_chars: &mut usize,
    line_start: Option<usize>,
    line_end: usize,
    context: &DocumentContext,
) {
    if current.is_empty() {
        return;
    }

    let summary_before = if let Some(prev) = shards.last() {
        prev.summary_after.clone()
    } else {
        truncate(&context.abstract_text, 200)
    };

    let summary_after = build_shard_summary(current);

    shards.push(SemanticShard {
        shard_id: format!("shard-{}", shards.len()),
        segments: std::mem::take(current),
        line_start: line_start.unwrap_or(0),
        line_end,
        summary_before,
        summary_after,
    });

    *current_chars = 0;
}

/// Builds bundles (SegmentBundle) from semantic shards for backward compatibility.
pub fn build_bundles_from_shards(
    shards: Vec<SemanticShard>,
    context: &DocumentContext,
) -> Vec<SegmentBundle> {
    shards
        .into_iter()
        .map(|shard| SegmentBundle {
            bundle_id: shard.shard_id,
            segments: shard.segments,
            summary_before: shard.summary_before,
            summary_after: shard.summary_after,
            style_instructions: context.style_guide.clone(),
        })
        .collect()
}

/// Estimates the translatable content size of a segment.
/// Excludes protected spans (code, URLs, etc.) since they don't need translation.
fn estimate_translatable_size(segment: &Segment) -> usize {
    let mut size = segment.source_text.len();

    // Subtract protected span lengths to get actual translatable content
    for span in &segment.protected_spans {
        size = size.saturating_sub(span.original.len());
    }

    // Add overhead for the segment_id and JSON structure (rough estimate)
    size + segment.segment_id.len() + 20
}

fn build_shard_summary(segments: &[Segment]) -> String {
    let joined: String = segments
        .iter()
        .map(|s| s.source_text.replace('\n', " "))
        .collect::<Vec<_>>()
        .join(" ");
    truncate(&joined, 240)
}

fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Builds bundles (SegmentBundle) from segments - backward-compatible entry point.
/// Internally uses semantic shard building with line-based boundaries.
pub fn build_bundles(
    segments: &[Segment],
    context: &DocumentContext,
    config: &TranslatorConfig,
) -> Vec<SegmentBundle> {
    let shards = build_semantic_shards(segments, context, config);
    build_bundles_from_shards(shards, context)
}

/// Extracts line mappings from source text for integrity verification.
/// Returns a vector of (line_number, line_text) tuples.
pub fn extract_line_mappings(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect()
}

/// Merges overlapping or adjacent shards into a single shard.
pub fn merge_shards(shards: Vec<SemanticShard>) -> Vec<SemanticShard> {
    if shards.is_empty() {
        return shards;
    }

    let mut merged: Vec<SemanticShard> = Vec::new();
    let mut current = shards[0].clone();

    for next in shards.into_iter().skip(1) {
        // Check if current and next are adjacent or overlapping
        if current.line_end >= next.line_start {
            // Merge them
            current.line_end = current.line_end.max(next.line_end);
            current.segments.extend(next.segments);
            current.summary_after = build_shard_summary(&current.segments);
        } else {
            merged.push(current);
            current = next;
        }
    }
    merged.push(current);

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_context() -> DocumentContext {
        DocumentContext {
            title: "Test".into(),
            abstract_text: "Abstract text".into(),
            section_summaries: HashMap::new(),
            style_guide: vec!["Tone: technical".into()],
            audience: "developers".into(),
        }
    }

    fn make_segment_with_lines(text: &str, line_start: usize, line_end: usize) -> Segment {
        Segment {
            segment_id: format!("seg-{}", text.len()),
            node_type: NodeType::Paragraph,
            source_text: text.to_owned(),
            context_path: vec![],
            byte_range: 0..text.len(),
            line_start,
            line_end,
            protected_spans: vec![],
            metadata: HashMap::new(),
        }
    }

    fn make_heading_segment(text: &str, level: u8, line_start: usize, line_end: usize) -> Segment {
        Segment {
            segment_id: format!("h{}-{}", level, text.len()),
            node_type: NodeType::Heading,
            source_text: text.to_owned(),
            context_path: vec![],
            byte_range: 0..text.len(),
            line_start,
            line_end,
            protected_spans: vec![],
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn single_shard() {
        let config = TranslatorConfig::default();
        let ctx = make_context();
        let segments = vec![
            make_segment_with_lines("Hello", 1, 1),
            make_segment_with_lines("World", 2, 2),
        ];
        let shards = build_semantic_shards(&segments, &ctx, &config);
        assert_eq!(shards.len(), 1);
        assert_eq!(shards[0].segments.len(), 2);
        assert_eq!(shards[0].line_start, 1);
        assert_eq!(shards[0].line_end, 2);
    }

    #[test]
    fn splits_at_semantic_boundary() {
        let mut config = TranslatorConfig::default();
        config.segmentation.max_bundle_chars = 20;
        let ctx = make_context();
        let segments = vec![
            make_segment_with_lines("This is a long paragraph that exceeds the limit.", 1, 1),
            make_heading_segment("## Heading", 2, 2, 2),
            make_segment_with_lines("Another paragraph.", 3, 3),
        ];
        let shards = build_semantic_shards(&segments, &ctx, &config);
        // Should split: first segment alone (heading is semantic break)
        assert!(shards.len() >= 2);
        assert_eq!(shards[0].line_end, 1);
    }

    #[test]
    fn line_count() {
        let config = TranslatorConfig::default();
        let ctx = make_context();
        let segments = vec![
            make_segment_with_lines("Line 1", 1, 2),
            make_segment_with_lines("Line 2", 3, 3),
        ];
        let shards = build_semantic_shards(&segments, &ctx, &config);
        assert_eq!(shards[0].line_count(), 3);
    }

    #[test]
    fn shard_overlap_detection() {
        let shard1 = SemanticShard {
            shard_id: "s1".into(),
            segments: vec![],
            line_start: 1,
            line_end: 5,
            summary_before: "".into(),
            summary_after: "".into(),
        };
        let shard2 = SemanticShard {
            shard_id: "s2".into(),
            segments: vec![],
            line_start: 4,
            line_end: 8,
            summary_before: "".into(),
            summary_after: "".into(),
        };
        assert!(shard1.overlaps(&shard2));
    }

    #[test]
    fn merge_adjacent_shards() {
        let shards = vec![
            SemanticShard {
                shard_id: "s1".into(),
                segments: vec![],
                line_start: 1,
                line_end: 3,
                summary_before: "a".into(),
                summary_after: "b".into(),
            },
            SemanticShard {
                shard_id: "s2".into(),
                segments: vec![],
                line_start: 3,
                line_end: 5,
                summary_before: "b".into(),
                summary_after: "c".into(),
            },
        ];
        let merged = merge_shards(shards);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].line_start, 1);
        assert_eq!(merged[0].line_end, 5);
    }

    #[test]
    fn line_mappings_extraction() {
        let text = "Line 1\nLine 2\nLine 3";
        let mappings = extract_line_mappings(text);
        assert_eq!(mappings.len(), 3);
        assert_eq!(mappings[0], (1, "Line 1"));
        assert_eq!(mappings[1], (2, "Line 2")); // enumerate is 0-indexed, we add 1
    }
}
