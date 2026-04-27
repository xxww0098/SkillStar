use crate::types::{DocumentContext, SegmentBundle, TranslationResult};

/// Build (system_prompt, user_prompt) for translation.
/// Uses pure text format with integrity markers for robust parsing.
pub fn build_translator_prompts(
    bundle: &SegmentBundle,
    context: &DocumentContext,
    target_lang: &str,
) -> (String, String) {
    let system_prompt = format!(
        r#"You are a Markdown translation specialist.

TRANSLATION RULES:
- Translate human-readable text to {target_lang}.
- Keep placeholders like {{PH1}}, {{PH2}} EXACTLY as-is.
- Keep URLs, code blocks, and inline code EXACTLY as-is.
- Preserve Markdown syntax: headings (#), lists (- *), blockquotes (>), tables.
- Preserve emphasis markers (* and _) — do NOT modify them.
- Preserve YAML front matter keys and structure.
- Do NOT add explanations, apologies, or extra commentary.
- Do NOT rephrase or improve — translate faithfully in original order.

OUTPUT FORMAT:
Return translations using integrity markers:

[[BEGIN segment_id]]
translated text here
[[END segment_id]]

Process ALL segments. Keep segment_ids exactly as provided."#
    );

    let user_prompt = build_translation_user_prompt(bundle, context, target_lang);
    (system_prompt, user_prompt)
}

fn build_translation_user_prompt(
    bundle: &SegmentBundle,
    context: &DocumentContext,
    target_lang: &str,
) -> String {
    let mut prompt = String::new();

    // Document context header
    prompt.push_str("=== DOCUMENT CONTEXT ===\n\n");
    if !context.title.is_empty() {
        prompt.push_str(&format!("Title: {}\n\n", truncate(&context.title, 120)));
    }
    if !context.style_guide.is_empty() {
        prompt.push_str("Style guidelines:\n");
        for (i, guide) in context.style_guide.iter().take(3).enumerate() {
            prompt.push_str(&format!("  {}. {}\n", i + 1, truncate(guide, 200)));
        }
        prompt.push('\n');
    }
    if !context.audience.is_empty() {
        prompt.push_str(&format!("Target audience: {}\n\n", &context.audience));
    }

    // Segment list
    prompt.push_str("=== SEGMENTS TO TRANSLATE ===\n\n");
    prompt.push_str(&format!(
        "Translate the following {} segments to {}:\n\n",
        bundle.segments.len(),
        target_lang
    ));

    for segment in &bundle.segments {
        let ctx_path = if segment.context_path.len() > 2 {
            segment.context_path[segment.context_path.len() - 2..].join(" / ")
        } else {
            segment.context_path.join(" / ")
        };

        prompt.push_str(&format!("--- Segment: {} ---\n", segment.segment_id));
        if !ctx_path.is_empty() {
            prompt.push_str(&format!("Context: {}\n", ctx_path));
        }
        prompt.push_str(&format!("Type: {}\n", segment.node_type.as_str()));
        if !segment.protected_spans.is_empty() {
            let placeholders: Vec<_> = segment
                .protected_spans
                .iter()
                .map(|s| s.placeholder.as_str())
                .collect();
            prompt.push_str(&format!("Protected: {}\n", placeholders.join(", ")));
        }
        prompt.push_str(&format!(
            "[[BEGIN {}]]\n{}\n[[END {}]]\n\n",
            segment.segment_id, segment.source_text, segment.segment_id
        ));
    }

    prompt.push_str("=== END OF SEGMENTS ===\n");
    prompt
        .push_str("Now return ALL translations using the [[BEGIN id]]...[[END id]] format above.");

    prompt
}

/// Build (system_prompt, user_prompt) for review.
pub fn build_reviewer_prompts(
    bundle: &SegmentBundle,
    _context: &DocumentContext,
    translations: &[TranslationResult],
) -> (String, String) {
    let system_prompt = r#"You are a translation reviewer.

REVIEW RULES:
- Review translations for accuracy, fluency, and consistency.
- Keep placeholders like {{PH1}} EXACTLY as-is.
- Preserve Markdown syntax and emphasis markers.
- Do NOT add explanations — return corrected translations only.

OUTPUT FORMAT:
Return translations using integrity markers:

[[BEGIN segment_id]]
reviewed/corrected text here
[[END segment_id]]

Only include segments that need changes. Others will be kept as-is."#
        .to_owned();

    let mut prompt = String::new();
    prompt.push_str("=== SEGMENTS TO REVIEW ===\n\n");
    prompt.push_str(&format!(
        "Review {} segments for accuracy and fluency:\n\n",
        bundle.segments.len()
    ));

    for (seg, tr) in bundle.segments.iter().zip(translations.iter()) {
        prompt.push_str(&format!("--- Segment: {} ---\n", seg.segment_id));
        prompt.push_str(&format!("Source:\n{}\n\n", seg.source_text));
        prompt.push_str(&format!("Current translation:\n{}\n\n", tr.translated_text));
        prompt.push_str(&format!(
            "[[BEGIN {}]]\n{}\n[[END {}]]\n\n",
            seg.segment_id, tr.translated_text, seg.segment_id
        ));
    }

    prompt.push_str("=== END OF REVIEW ===\n");
    prompt.push_str("Return corrected translations using [[BEGIN id]]...[[END id]] format.");

    (system_prompt, prompt)
}

/// Build (system_prompt, user_prompt) for format guard.
pub fn build_guard_prompts(
    bundle: &SegmentBundle,
    translations: &[TranslationResult],
) -> (String, String) {
    let system_prompt = r#"You are a Markdown format repair specialist.

REPAIR RULES:
- Fix only Markdown formatting issues (headings, lists, emphasis, tables).
- Keep placeholders like {{PH1}} EXACTLY as-is.
- Keep translated content accurate — only fix format, not translation.
- Make minimal edits — do not re-translate.

OUTPUT FORMAT:
Return translations using integrity markers:

[[BEGIN segment_id]]
format-corrected text here
[[END segment_id]]

Only include segments with format issues."#
        .to_owned();

    let mut prompt = String::new();
    prompt.push_str("=== SEGMENTS TO REPAIR ===\n\n");
    prompt.push_str(&format!(
        "Fix Markdown format issues in {} segments:\n\n",
        translations.len()
    ));

    for (seg, tr) in bundle.segments.iter().zip(translations.iter()) {
        prompt.push_str(&format!("--- Segment: {} ---\n", seg.segment_id));
        prompt.push_str(&format!("Source:\n{}\n\n", seg.source_text));
        prompt.push_str(&format!("Translation:\n{}\n\n", tr.translated_text));
        prompt.push_str(&format!(
            "[[BEGIN {}]]\n{}\n[[END {}]]\n\n",
            seg.segment_id, tr.translated_text, seg.segment_id
        ));
    }

    prompt.push_str("=== END OF REPAIRS ===\n");
    prompt.push_str("Return format-corrected translations using [[BEGIN id]]...[[END id]] format.");

    (system_prompt, prompt)
}

/// Extract translations from a response that uses integrity markers.
/// Returns None if parsing fails.
pub fn parse_marked_translations(response: &str) -> Option<Vec<(String, String)>> {
    let mut results = Vec::new();
    let mut remaining = response;

    loop {
        // Find next [[BEGIN id]]
        let begin_pos = remaining.find("[[BEGIN ")?;
        let end_of_begin = remaining[begin_pos..].find("]]")?;
        let id_start = begin_pos + 8; // skip "[[BEGIN "
        let id_end = id_start + end_of_begin - 8;
        let segment_id = remaining[id_start..id_end].to_string();

        // Find corresponding [[END id]]
        let after_begin = &remaining[begin_pos + end_of_begin + 2..];
        let end_marker = format!("[[END {}]]", segment_id);
        let end_pos = after_begin.find(&end_marker)?;

        let translated_text = after_begin[..end_pos].trim().to_string();
        results.push((segment_id, translated_text));

        // Move to remaining text after this block
        remaining = &after_begin[end_pos + end_marker.len()..];

        // Stop if no more complete blocks
        if !remaining.contains("[[BEGIN ") {
            break;
        }
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        s.chars().take(max_len).collect::<String>() + "..."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_marked_translations() {
        let response = r#"
Some preamble text.

[[BEGIN seg_001]]
Hello, world!
[[END seg_001]]

[[BEGIN seg_002]]
Goodbye, world!
[[END seg_002]]

Some footer text.
"#;

        let results = parse_marked_translations(response);
        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "seg_001");
        assert_eq!(results[0].1, "Hello, world!");
        assert_eq!(results[1].0, "seg_002");
        assert_eq!(results[1].1, "Goodbye, world!");
    }

    #[test]
    fn test_parse_marked_translations_empty() {
        let response = "No markers here.";
        let results = parse_marked_translations(response);
        assert!(results.is_none());
    }
}
