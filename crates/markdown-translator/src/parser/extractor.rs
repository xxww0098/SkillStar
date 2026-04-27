use std::collections::HashMap;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::parser::protector;
use crate::types::{NodeType, Segment};

/// Keys in front matter that should be extracted for translation.
const FRONTMATTER_TRANSLATABLE_KEYS: &[&str] = &["title", "description"];

/// Extracts translatable segments from a Markdown body text using `pulldown-cmark`.
///
/// Each segment contains the full text of a translatable block (paragraph, heading,
/// blockquote, table cell, or **tight list item**) along with its byte range within the body.
pub fn extract(
    body: &str,
    body_line_offset: usize,
    front_matter: &HashMap<String, serde_yaml::Value>,
) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();

    let opts =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_HEADING_ATTRIBUTES;
    let parser = Parser::new_ext(body, opts);

    // Track the state of the current block.
    let mut current_block: Option<BlockState> = None;
    // Tight list items have no wrapping `Paragraph` in pulldown-cmark; track them here.
    let mut tight_list_item: Option<BlockState> = None;
    let mut in_code_block = false;

    for (event, range) in parser.into_offset_iter() {
        match event {
            // ── Block openers ────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {
                tight_list_item = None;
                current_block = Some(BlockState {
                    node_type: NodeType::Paragraph,
                    byte_start: range.start,
                    byte_end: range.end,
                });
            }
            Event::Start(Tag::Heading { level, .. }) => {
                tight_list_item = None;
                let depth = heading_level_to_depth(level);
                heading_stack.truncate(depth.saturating_sub(1));
                current_block = Some(BlockState {
                    node_type: NodeType::Heading,
                    byte_start: range.start,
                    byte_end: range.end,
                });
            }
            Event::Start(Tag::BlockQuote(_)) => {
                tight_list_item = None;
                current_block = Some(BlockState {
                    node_type: NodeType::Blockquote,
                    byte_start: range.start,
                    byte_end: range.end,
                });
            }
            Event::Start(Tag::Table(_)) => {
                tight_list_item = None;
            }
            Event::Start(Tag::List(_)) => {
                // Nested / following list inside an item cancels tight-only capture.
                tight_list_item = None;
            }
            Event::Start(Tag::TableCell) => {
                tight_list_item = None;
                current_block = Some(BlockState {
                    node_type: NodeType::TableCell,
                    byte_start: range.start,
                    byte_end: range.end,
                });
            }
            Event::Start(Tag::Item) => {
                tight_list_item = Some(BlockState {
                    node_type: NodeType::Paragraph,
                    byte_start: range.start,
                    byte_end: range.end,
                });
            }

            // ── Code blocks: skip entirely ───────────────────────────
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                current_block = None;
                tight_list_item = None;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
            }

            // ── Block closers → emit segment ────────────────────────
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::BlockQuote(_) | TagEnd::TableCell,
            ) => {
                if let Some(block) = current_block.take() {
                    emit_block_from_range(
                        body,
                        body_line_offset,
                        &mut heading_stack,
                        &mut segments,
                        block,
                        range.end,
                    );
                }
            }
            Event::End(TagEnd::Item) => {
                if let Some(block) = tight_list_item.take() {
                    emit_block_from_range(
                        body,
                        body_line_offset,
                        &mut heading_stack,
                        &mut segments,
                        block,
                        range.end,
                    );
                }
            }

            // ── Text inside blocks: expand the block range ──────────
            Event::Text(_) | Event::Code(_) | Event::SoftBreak | Event::HardBreak => {
                if !in_code_block {
                    if let Some(ref mut block) = current_block {
                        block.byte_end = range.end;
                    }
                    if let Some(ref mut block) = tight_list_item {
                        block.byte_end = range.end;
                    }
                }
            }

            _ => {}
        }
    }

    // ── Front matter segments ────────────────────────────────────────────
    let title = front_matter
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let context_path: Vec<String> = if title.is_empty() {
        vec![]
    } else {
        vec![title.to_owned()]
    };

    for &key in FRONTMATTER_TRANSLATABLE_KEYS {
        if let Some(value) = front_matter.get(key).and_then(|v| v.as_str()) {
            if value.trim().is_empty() {
                continue;
            }
            let fm_segment = Segment {
                segment_id: format!("fm-{key}"),
                node_type: NodeType::FrontMatter,
                source_text: value.to_owned(),
                context_path: context_path.clone(),
                byte_range: 0..0, // Front matter is handled separately.
                line_start: 0,
                line_end: body_line_offset,
                protected_spans: vec![],
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "front_matter_key".to_owned(),
                        serde_json::Value::String(key.to_owned()),
                    );
                    m
                },
            };
            // Insert front matter segments at the beginning.
            segments.insert(0, protector::protect(&fm_segment));
        }
    }

    segments
}

fn heading_level_to_depth(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

struct BlockState {
    node_type: NodeType,
    byte_start: usize,
    byte_end: usize,
}

fn emit_block_from_range(
    body: &str,
    body_line_offset: usize,
    heading_stack: &mut Vec<String>,
    segments: &mut Vec<Segment>,
    block: BlockState,
    end: usize,
) {
    let block_text = &body[block.byte_start..end];
    let trimmed = block_text.trim();
    if trimmed.is_empty() {
        return;
    }
    let line_start = body_line_offset + body[..block.byte_start].lines().count();
    let line_end = body_line_offset + body[..end].lines().count();

    let segment = Segment {
        segment_id: format!("body-{}", segments.len()),
        node_type: block.node_type,
        source_text: block_text.to_owned(),
        context_path: heading_stack.clone(),
        byte_range: block.byte_start..end,
        line_start,
        line_end,
        protected_spans: vec![],
        metadata: HashMap::new(),
    };

    if segment.node_type == NodeType::Heading {
        heading_stack.push(trimmed.trim_start_matches('#').trim().to_owned());
    }

    segments.push(protector::protect(&segment));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_paragraphs() {
        let body = "Hello world\n\nSecond paragraph\n";
        let segments = extract(body, 0, &HashMap::new());
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].node_type, NodeType::Paragraph);
        assert_eq!(segments[1].node_type, NodeType::Paragraph);
    }

    #[test]
    fn extract_headings() {
        let body = "# Title\n\nSome text\n\n## Sub\n\nMore text\n";
        let segments = extract(body, 0, &HashMap::new());
        // Should have: heading(Title), paragraph(Some text), heading(Sub), paragraph(More text)
        assert!(segments.len() >= 3);
        let heading = segments.iter().find(|s| s.node_type == NodeType::Heading);
        assert!(heading.is_some());
    }

    #[test]
    fn skip_code_blocks() {
        let body = "Before\n\n```rust\nlet x = 1;\n```\n\nAfter\n";
        let segments = extract(body, 0, &HashMap::new());
        // Code block content should not appear in any segment.
        for seg in &segments {
            assert!(
                !seg.source_text.contains("let x = 1"),
                "Code block content leaked into segment: {}",
                seg.source_text
            );
        }
    }

    #[test]
    fn context_path_tracking() {
        let body = "# A\n\n## B\n\nContent\n";
        let segments = extract(body, 0, &HashMap::new());
        let content_seg = segments
            .iter()
            .find(|s| s.source_text.contains("Content"))
            .unwrap();
        assert!(
            content_seg.context_path.len() >= 1,
            "context_path should have heading entries"
        );
    }

    #[test]
    fn front_matter_extraction() {
        let mut fm = HashMap::new();
        fm.insert(
            "title".to_owned(),
            serde_yaml::Value::String("My Doc".to_owned()),
        );
        fm.insert(
            "description".to_owned(),
            serde_yaml::Value::String("A doc".to_owned()),
        );
        let body = "# Hello\n";
        let segments = extract(body, 4, &fm);
        let fm_segs: Vec<_> = segments
            .iter()
            .filter(|s| s.node_type == NodeType::FrontMatter)
            .collect();
        assert_eq!(fm_segs.len(), 2);
    }

    /// Tight list items omit a `Paragraph` wrapper in pulldown-cmark; they must still become segments.
    #[test]
    fn extract_tight_list_items() {
        let body = "## When to use\n\n- First bullet line\n- Second bullet line\n";
        let segments = extract(body, 0, &HashMap::new());
        let bullets: Vec<_> = segments
            .iter()
            .filter(|s| {
                s.source_text.contains("First bullet") || s.source_text.contains("Second bullet")
            })
            .collect();
        assert_eq!(
            bullets.len(),
            2,
            "expected two list-item segments, got: {:?}",
            segments.iter().map(|s| &s.source_text).collect::<Vec<_>>()
        );
    }

    /// Loose list items use inner paragraphs; ensure we do not duplicate segments.
    #[test]
    fn extract_loose_list_items_no_duplicate() {
        let body = "- Loose one\n\n- Loose two\n";
        let segments = extract(body, 0, &HashMap::new());
        let loose: Vec<_> = segments
            .iter()
            .filter(|s| s.source_text.contains("Loose"))
            .collect();
        assert_eq!(loose.len(), 2, "segments: {:?}", segments);
    }
}
