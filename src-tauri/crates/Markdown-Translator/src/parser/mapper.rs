use std::collections::HashMap;

use crate::parser::frontmatter::render_with_front_matter;
use crate::types::{NodeType, ParsedDocument, TranslationResult};

/// Applies translation results back into the document body, producing the final text.
///
/// Body segments are replaced by byte range in **reverse order** to preserve offsets.
/// Front matter keys are updated in-place.
pub fn apply(doc: &ParsedDocument, translations: &[TranslationResult]) -> String {
    let translation_map: HashMap<&str, &str> = translations
        .iter()
        .map(|t| (t.segment_id.as_str(), t.translated_text.as_str()))
        .collect();

    // ── Body replacement (reverse order to preserve byte offsets) ────────
    let mut body = doc.body_text.clone();

    // Collect body segments that have translations, sorted by byte_range.start descending.
    let mut body_replacements: Vec<_> = doc
        .segments
        .iter()
        .filter(|s| {
            s.node_type != NodeType::FrontMatter
                && translation_map.contains_key(s.segment_id.as_str())
        })
        .collect();
    body_replacements.sort_by(|a, b| b.byte_range.start.cmp(&a.byte_range.start));

    for segment in body_replacements {
        if let Some(&translated) = translation_map.get(segment.segment_id.as_str()) {
            let start = segment.byte_range.start;
            let end = segment.byte_range.end;
            if start <= body.len() && end <= body.len() {
                body.replace_range(start..end, translated);
            }
        }
    }

    // ── Front matter replacement ────────────────────────────────────────
    let mut fm_data = doc
        .front_matter
        .as_ref()
        .map(|fm| fm.data.clone())
        .unwrap_or_default();
    let mut fm_changed = false;

    for segment in &doc.segments {
        if segment.node_type != NodeType::FrontMatter {
            continue;
        }
        if let Some(&translated) = translation_map.get(segment.segment_id.as_str()) {
            if let Some(key) = segment
                .metadata
                .get("front_matter_key")
                .and_then(|v| v.as_str())
            {
                fm_data.insert(
                    key.to_owned(),
                    serde_yaml::Value::String(translated.to_owned()),
                );
                fm_changed = true;
            }
        }
    }

    let fm_ref = if fm_changed || doc.front_matter.is_some() {
        Some(&fm_data)
    } else {
        None
    };

    render_with_front_matter(fm_ref.filter(|d| !d.is_empty()), &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FrontMatter, Segment};

    #[test]
    fn body_replacement_basic() {
        let body = "Hello world\n\nSecond paragraph\n";
        let doc = ParsedDocument {
            source_path: "test.md".into(),
            source_text: body.to_owned(),
            body_text: body.to_owned(),
            target_lang: "chinese".into(),
            front_matter: None,
            body_line_offset: 0,
            segments: vec![Segment {
                segment_id: "body-0".into(),
                node_type: NodeType::Paragraph,
                source_text: "Hello world".into(),
                context_path: vec![],
                byte_range: 0..11,
                line_start: 0,
                line_end: 1,
                protected_spans: vec![],
                metadata: Default::default(),
            }],
        };
        let translations = vec![TranslationResult {
            segment_id: "body-0".into(),
            translated_text: "你好世界".into(),
            notes: vec![],
            applied_terms: Default::default(),
            confidence: 1.0,
        }];
        let result = apply(&doc, &translations);
        assert!(result.contains("你好世界"));
        assert!(result.contains("Second paragraph"));
    }

    #[test]
    fn front_matter_replacement() {
        let body = "# Heading\n";
        let mut fm_data = HashMap::new();
        fm_data.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Original Title".to_owned()),
        );
        let doc = ParsedDocument {
            source_path: "test.md".into(),
            source_text: format!("---\ntitle: Original Title\n---\n{body}"),
            body_text: body.to_owned(),
            target_lang: "chinese".into(),
            front_matter: Some(FrontMatter {
                raw: "title: Original Title".into(),
                data: fm_data,
            }),
            body_line_offset: 3,
            segments: vec![Segment {
                segment_id: "fm-title".into(),
                node_type: NodeType::FrontMatter,
                source_text: "Original Title".into(),
                context_path: vec![],
                byte_range: 0..0,
                line_start: 0,
                line_end: 3,
                protected_spans: vec![],
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "front_matter_key".to_owned(),
                        serde_json::Value::String("title".to_owned()),
                    );
                    m
                },
            }],
        };
        let translations = vec![TranslationResult {
            segment_id: "fm-title".into(),
            translated_text: "翻译标题".into(),
            notes: vec![],
            applied_terms: Default::default(),
            confidence: 1.0,
        }];
        let result = apply(&doc, &translations);
        assert!(result.contains("翻译标题"));
    }
}
