use std::collections::HashMap;

use crate::config::TranslatorConfig;
use crate::types::{DocumentContext, NodeType, ParsedDocument, Segment};

/// Builds document-level context that is injected into LLM prompts.
pub fn build(
    doc: &ParsedDocument,
    segments: &[Segment],
    config: &TranslatorConfig,
) -> DocumentContext {
    // Title: prefer front matter, then first heading context path.
    let title = doc
        .front_matter
        .as_ref()
        .and_then(|fm| fm.data.get("title"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| {
            segments
                .iter()
                .find(|s| !s.context_path.is_empty())
                .map(|s| s.context_path.join(" / "))
                .unwrap_or_default()
        });

    // Abstract: first paragraph's text.
    let abstract_text = segments
        .iter()
        .find(|s| s.node_type == NodeType::Paragraph)
        .map(|s| s.source_text.clone())
        .unwrap_or_default();

    // Section summaries: first paragraph under each heading path.
    let mut section_summaries: HashMap<String, String> = HashMap::new();
    for seg in segments {
        if seg.context_path.is_empty() || seg.node_type != NodeType::Paragraph {
            continue;
        }
        let path = seg.context_path.join(" / ");
        section_summaries.entry(path).or_insert_with(|| {
            seg.source_text.chars().take(200).collect()
        });
    }

    // Style guide assembly.
    let mut style_guide = vec![
        format!("Tone: {}", config.style.tone),
        format!("Audience: {}", config.style.audience),
    ];
    style_guide.extend(config.style.instructions.iter().cloned());
    if !config.style.preserve_terms.is_empty() {
        style_guide.push(format!(
            "Preserve these terms: {}",
            config.style.preserve_terms.join(", ")
        ));
    }

    DocumentContext {
        title,
        abstract_text,
        section_summaries,
        style_guide,
        audience: config.style.audience.clone(),
    }
}
