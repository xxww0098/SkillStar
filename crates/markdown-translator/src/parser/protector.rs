use regex::Regex;
use std::sync::LazyLock;

use crate::types::{ProtectedSpan, Segment, SpanType};

/// Regex patterns for Markdown control syntax that must be preserved during translation.
static RE_HEADING_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(#{1,6}\s+)").unwrap());
static RE_BLOCKQUOTE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^((?:>\s?)+)").unwrap());
static RE_LIST_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(\s*(?:[-+*]|\d+\.)\s+)").unwrap());
static RE_INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`\n]+`").unwrap());
static RE_HTML_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"</?[a-zA-Z][^>\n]*?>").unwrap());
static RE_LINK_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!?\[[^\]]*]\()([^)]+)(\))").unwrap());
static RE_BARE_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://[^\s)>]+").unwrap());
static RE_LEADING_PLACEHOLDERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\{\{[A-Z_0-9]+\}\})+").unwrap());

/// Replaces Markdown control syntax in a segment with placeholders,
/// returning a modified segment with populated `protected_spans`.
pub fn protect(segment: &Segment) -> Segment {
    let mut text = segment.source_text.clone();
    let mut spans: Vec<ProtectedSpan> = Vec::new();

    // Helper: store a match and return its placeholder.
    let mut store = |value: &str, span_type: SpanType| -> String {
        let placeholder = format!(
            "{{{{{ty}_{i}}}}}",
            ty = span_type_tag(&span_type),
            i = spans.len()
        );
        spans.push(ProtectedSpan {
            placeholder: placeholder.clone(),
            original: value.to_owned(),
            span_type,
        });
        placeholder
    };

    // Order matters: heading/blockquote/list prefixes first, then inline elements.
    text = RE_HEADING_PREFIX
        .replace_all(&text, |caps: &regex::Captures| {
            store(&caps[1], SpanType::Md)
        })
        .into_owned();
    text = RE_BLOCKQUOTE_PREFIX
        .replace_all(&text, |caps: &regex::Captures| {
            store(&caps[1], SpanType::Md)
        })
        .into_owned();
    text = RE_LIST_PREFIX
        .replace_all(&text, |caps: &regex::Captures| {
            store(&caps[1], SpanType::Md)
        })
        .into_owned();
    text = RE_INLINE_CODE
        .replace_all(&text, |caps: &regex::Captures| {
            store(&caps[0], SpanType::Code)
        })
        .into_owned();
    text = RE_HTML_TAG
        .replace_all(&text, |caps: &regex::Captures| {
            store(&caps[0], SpanType::Html)
        })
        .into_owned();
    // Links: protect the URL part only, keep the bracket structure.
    text = RE_LINK_URL
        .replace_all(&text, |caps: &regex::Captures| {
            let url_placeholder = store(&caps[2], SpanType::Url);
            format!("{}{}{}", &caps[1], url_placeholder, &caps[3])
        })
        .into_owned();
    text = RE_BARE_URL
        .replace_all(&text, |caps: &regex::Captures| {
            store(&caps[0], SpanType::Url)
        })
        .into_owned();

    Segment {
        source_text: text,
        protected_spans: spans,
        ..segment.clone()
    }
}

/// Restores all placeholders in translated text back to their original content.
pub fn restore(translated_text: &str, segment: &Segment) -> String {
    let mut result = translated_text.to_owned();

    // Ensure leading placeholders from the source are at the front of the result,
    // even if the LLM reordered them.
    if let Some(leading) = RE_LEADING_PLACEHOLDERS.find(&segment.source_text) {
        let prefix = leading.as_str();
        // Strip any leading placeholders the LLM might have placed.
        if let Some(m) = RE_LEADING_PLACEHOLDERS.find(&result) {
            result = result[m.end()..].to_owned();
        }
        // Remove duplicate occurrences of the prefix in the body.
        result = result.replace(prefix, "");
        result = format!("{prefix}{result}");
    }

    // Sort protected spans by the position they appear in the source text
    // to maintain correct restoration order (prevents "##" being placed after "`code`")
    let mut sorted_spans = segment.protected_spans.clone();
    sorted_spans.sort_by_key(|span| {
        segment
            .source_text
            .find(&span.placeholder)
            .unwrap_or(usize::MAX)
    });

    // Replace each placeholder with its original text.
    for span in &sorted_spans {
        result = result.replace(&span.placeholder, &span.original);
    }

    result
}

fn span_type_tag(span_type: &SpanType) -> &'static str {
    match span_type {
        SpanType::Md => "MD",
        SpanType::Code => "CODE",
        SpanType::Html => "HTML",
        SpanType::Url => "URL",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeType;

    fn make_segment(text: &str) -> Segment {
        Segment {
            segment_id: "test-0".into(),
            node_type: NodeType::Paragraph,
            source_text: text.into(),
            context_path: vec![],
            byte_range: 0..text.len(),
            line_start: 0,
            line_end: 1,
            protected_spans: vec![],
            metadata: Default::default(),
        }
    }

    #[test]
    fn protect_heading() {
        let seg = make_segment("## Hello World");
        let protected = protect(&seg);
        assert!(protected.source_text.contains("{{MD_0}}"));
        assert_eq!(protected.protected_spans[0].original, "## ");
    }

    #[test]
    fn protect_inline_code() {
        let seg = make_segment("Use `cargo build` to compile");
        let protected = protect(&seg);
        assert!(protected.source_text.contains("{{CODE_"));
        assert_eq!(
            protected
                .protected_spans
                .iter()
                .find(|s| s.span_type == SpanType::Code)
                .unwrap()
                .original,
            "`cargo build`"
        );
    }

    #[test]
    fn protect_url_in_link() {
        let seg = make_segment("[click here](https://example.com)");
        let protected = protect(&seg);
        assert!(protected.source_text.contains("{{URL_"));
        assert!(!protected.source_text.contains("https://example.com"));
    }

    #[test]
    fn restore_roundtrip() {
        let seg = make_segment("## Use `code` at [link](https://x.com)");
        let protected = protect(&seg);
        let restored = restore(&protected.source_text, &protected);
        assert_eq!(restored, "## Use `code` at [link](https://x.com)");
    }
}
