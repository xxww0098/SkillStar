//! XML-based segment batching for LLM translation.
//!
//! Encodes translatable nodes as `<seg id="N">text</seg>` so the LLM only needs
//! to "preserve the tags exactly" — much more robust than free-form JSON output.
//! Parsing uses `quick-xml` for forgiving entity / CDATA handling.

use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::ast::TranslatableNode;

/// Speed-first defaults for current OpenAI-compatible providers.
///
/// The live large-file test showed per-request overhead dominates tiny batches,
/// while very large XML batches can stall slower providers. These defaults sit
/// between the earlier 1500/8 setting and the too-heavy 3000/16 experiment.
pub const DEFAULT_MAX_CHARS_PER_BATCH: usize = 2200;
pub const DEFAULT_MAX_ITEMS_PER_BATCH: usize = 12;

#[derive(Debug, Clone)]
pub struct XmlBatch {
    pub xml: String,
    pub ids: Vec<usize>,
    /// Total input chars in this batch — used to size `max_tokens` for the LLM
    /// call (avoids the over-allocation latency penalty noted in chat_completion_capped).
    pub input_chars: usize,
}

#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub max_chars_per_batch: usize,
    pub max_items_per_batch: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_chars_per_batch: DEFAULT_MAX_CHARS_PER_BATCH,
            max_items_per_batch: DEFAULT_MAX_ITEMS_PER_BATCH,
        }
    }
}

pub fn pack(nodes: &[TranslatableNode], config: &BatchConfig) -> Vec<XmlBatch> {
    let mut batches = Vec::new();
    let mut current = XmlBatch {
        xml: String::new(),
        ids: Vec::new(),
        input_chars: 0,
    };

    for node in nodes {
        let escaped = escape_xml(&node.text);
        let seg = format!(r#"<seg id="{}">{}</seg>"#, node.id, escaped);

        let would_exceed_items =
            !current.ids.is_empty() && current.ids.len() >= config.max_items_per_batch;
        let would_exceed_chars =
            !current.ids.is_empty() && current.xml.len() + seg.len() > config.max_chars_per_batch;

        if would_exceed_items || would_exceed_chars {
            batches.push(std::mem::replace(
                &mut current,
                XmlBatch {
                    xml: String::new(),
                    ids: Vec::new(),
                    input_chars: 0,
                },
            ));
        }

        current.xml.push_str(&seg);
        current.ids.push(node.id);
        current.input_chars += node.text.chars().count();
    }

    if !current.ids.is_empty() {
        batches.push(current);
    }
    batches
}

#[derive(Debug)]
pub enum XmlParseError {
    MissingSegment { id: usize },
    MalformedXml(String),
}

impl std::fmt::Display for XmlParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSegment { id } => {
                write!(f, "translation response missing <seg id=\"{id}\">")
            }
            Self::MalformedXml(msg) => write!(f, "translation response malformed: {msg}"),
        }
    }
}

impl std::error::Error for XmlParseError {}

/// Parse `<seg id="N">translated text</seg>` from a (possibly noisy) LLM response.
///
/// Strategy:
///   1. Strip ```xml fences if present.
///   2. Walk `<seg>` start events, read text until matching end.
///   3. Validate every `expected_ids` is present.
///
/// Forgiving on:
///   - Leading/trailing prose around segments (it's ignored)
///   - Markdown code fences wrapping the XML
///   - Whitespace between segments
pub fn parse(
    response: &str,
    expected_ids: &[usize],
) -> Result<HashMap<usize, String>, XmlParseError> {
    let xml = strip_fences(response);
    let mut reader = Reader::from_reader(Cursor::new(xml.as_bytes()));
    let cfg = reader.config_mut();
    cfg.check_end_names = false;
    cfg.check_comments = false;
    cfg.trim_text(false);

    let mut result: HashMap<usize, String> = HashMap::new();
    let mut buf = Vec::new();

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"seg" => {
                let id = match extract_id(&e) {
                    Some(id) => id,
                    None => continue,
                };
                let text = read_seg_text(&mut reader)?;
                result.insert(id, text);
            }
            Ok(Event::Empty(e)) if e.name().as_ref() == b"seg" => {
                if let Some(id) = extract_id(&e) {
                    result.insert(id, String::new());
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                // Soft-fail: report what's parsed so far, let caller decide
                // whether MissingSegment forces a retry.
                return check_complete(result, expected_ids).map_err(|e| match e {
                    XmlParseError::MissingSegment { id } => XmlParseError::MalformedXml(format!(
                        "parser error at byte {}: {} (also missing id={})",
                        reader.error_position(),
                        err,
                        id
                    )),
                    other => other,
                });
            }
            _ => {}
        }
    }

    check_complete(result, expected_ids)
}

fn check_complete(
    result: HashMap<usize, String>,
    expected_ids: &[usize],
) -> Result<HashMap<usize, String>, XmlParseError> {
    for &id in expected_ids {
        if !result.contains_key(&id) {
            return Err(XmlParseError::MissingSegment { id });
        }
    }
    Ok(result)
}

fn extract_id(event: &BytesStart) -> Option<usize> {
    for attr in event.attributes().flatten() {
        if attr.key.as_ref() == b"id" {
            return std::str::from_utf8(attr.value.as_ref())
                .ok()
                .and_then(|s| s.parse::<usize>().ok());
        }
    }
    None
}

fn read_seg_text(reader: &mut Reader<Cursor<&[u8]>>) -> Result<String, XmlParseError> {
    let mut text = String::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                let unescaped = e
                    .unescape()
                    .map_err(|e| XmlParseError::MalformedXml(e.to_string()))?;
                text.push_str(&unescaped);
            }
            Ok(Event::CData(e)) => {
                text.push_str(
                    std::str::from_utf8(e.as_ref())
                        .map_err(|e| XmlParseError::MalformedXml(e.to_string()))?,
                );
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"seg" => break,
            Ok(Event::Eof) => {
                return Err(XmlParseError::MalformedXml(
                    "unexpected EOF inside <seg>".to_string(),
                ));
            }
            Err(err) => return Err(XmlParseError::MalformedXml(err.to_string())),
            _ => {}
        }
    }
    Ok(text)
}

fn strip_fences(s: &str) -> String {
    let trimmed = s.trim();
    let no_xml_fence = trimmed
        .strip_prefix("```xml")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let no_trailing = no_xml_fence.strip_suffix("```").unwrap_or(no_xml_fence);
    no_trailing.trim().to_string()
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_into_single_batch_for_small_input() {
        let nodes = vec![
            TranslatableNode {
                id: 1,
                text: "Hello".into(),
            },
            TranslatableNode {
                id: 2,
                text: "World".into(),
            },
        ];
        let batches = pack(&nodes, &BatchConfig::default());
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].ids, vec![1, 2]);
        assert!(batches[0].xml.contains(r#"<seg id="1">Hello</seg>"#));
        assert!(batches[0].xml.contains(r#"<seg id="2">World</seg>"#));
    }

    #[test]
    fn splits_when_exceeding_items_limit() {
        let nodes: Vec<_> = (1..=5)
            .map(|id| TranslatableNode {
                id,
                text: format!("t{id}"),
            })
            .collect();
        let batches = pack(
            &nodes,
            &BatchConfig {
                max_chars_per_batch: 10_000,
                max_items_per_batch: 2,
            },
        );
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].ids.len(), 2);
        assert_eq!(batches[1].ids.len(), 2);
        assert_eq!(batches[2].ids.len(), 1);
    }

    #[test]
    fn escapes_xml_special_chars() {
        let nodes = vec![TranslatableNode {
            id: 1,
            text: r#"a & <b> "c""#.into(),
        }];
        let batches = pack(&nodes, &BatchConfig::default());
        assert!(
            batches[0]
                .xml
                .contains(r#"<seg id="1">a &amp; &lt;b&gt; &quot;c&quot;</seg>"#)
        );
    }

    #[test]
    fn parses_clean_response() {
        let xml = r#"<seg id="1">你好</seg><seg id="2">世界</seg>"#;
        let parsed = parse(xml, &[1, 2]).expect("parse ok");
        assert_eq!(parsed[&1], "你好");
        assert_eq!(parsed[&2], "世界");
    }

    #[test]
    fn parses_through_code_fences() {
        let xml = "```xml\n<seg id=\"1\">译文</seg>\n```";
        let parsed = parse(xml, &[1]).expect("parse ok");
        assert_eq!(parsed[&1], "译文");
    }

    #[test]
    fn parses_with_prose_around_xml() {
        let xml = r#"Here's the translation:

<seg id="1">第一段</seg>
<seg id="2">第二段</seg>

That's all."#;
        let parsed = parse(xml, &[1, 2]).expect("parse ok");
        assert_eq!(parsed[&1], "第一段");
        assert_eq!(parsed[&2], "第二段");
    }

    #[test]
    fn parses_cdata() {
        let xml = r#"<seg id="1"><![CDATA[<not a tag>]]></seg>"#;
        let parsed = parse(xml, &[1]).expect("parse ok");
        assert_eq!(parsed[&1], "<not a tag>");
    }

    #[test]
    fn parse_unescapes_entities() {
        let xml = r#"<seg id="1">a &amp; b &lt;c&gt;</seg>"#;
        let parsed = parse(xml, &[1]).expect("parse ok");
        assert_eq!(parsed[&1], "a & b <c>");
    }

    #[test]
    fn parse_reports_missing_segment() {
        let xml = r#"<seg id="1">only one</seg>"#;
        let err = parse(xml, &[1, 2]).unwrap_err();
        assert!(matches!(err, XmlParseError::MissingSegment { id: 2 }));
    }
}
