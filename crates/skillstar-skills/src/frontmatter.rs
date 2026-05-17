//! Minimal YAML frontmatter parser/renderer for SKILL.md files.
//!
//! Replaces the dependency on `markdown_translator::parser::frontmatter`
//! after the translation feature was removed.

use std::collections::HashMap;
use serde_yaml::Value;

/// Result of splitting a Markdown document at its YAML frontmatter boundary.
#[derive(Debug)]
pub struct SplitResult {
    /// Parsed frontmatter key-value pairs (empty if no frontmatter present).
    pub data: HashMap<String, Value>,
    /// The Markdown body after the frontmatter block.
    pub body: String,
    /// Number of lines consumed by the frontmatter block (including delimiters).
    pub line_count: usize,
}

/// Split a Markdown document into YAML frontmatter and body.
///
/// Expects the standard `---` delimited frontmatter at the start of the file.
/// If no valid frontmatter is found, returns empty data and the full content as body.
pub fn split_front_matter(content: &str) -> SplitResult {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return SplitResult {
            data: HashMap::new(),
            body: content.to_string(),
            line_count: 0,
        };
    }

    // Find the closing `---` delimiter (skip the opening line)
    let after_opening = match trimmed.find('\n') {
        Some(pos) => pos + 1,
        None => {
            return SplitResult {
                data: HashMap::new(),
                body: content.to_string(),
                line_count: 0,
            };
        }
    };

    let rest = &trimmed[after_opening..];
    let closing_pos = rest.find("\n---");

    let (yaml_str, body_start) = match closing_pos {
        Some(pos) => {
            let yaml = &rest[..pos];
            // Skip past the closing `---` line
            let after_closing = pos + 4; // "\n---".len()
            let body_offset = after_opening + after_closing;
            // Skip the newline after closing delimiter if present
            let body = if trimmed[body_offset..].starts_with('\n') {
                &trimmed[body_offset + 1..]
            } else {
                &trimmed[body_offset..]
            };
            (yaml, body)
        }
        None => {
            return SplitResult {
                data: HashMap::new(),
                body: content.to_string(),
                line_count: 0,
            };
        }
    };

    let data: HashMap<String, Value> = serde_yaml::from_str(yaml_str).unwrap_or_default();
    let line_count = yaml_str.lines().count() + 2; // +2 for the two `---` lines

    SplitResult {
        data,
        body: body_start.to_string(),
        line_count,
    }
}

/// Render a Markdown document with optional YAML frontmatter.
///
/// If `front_matter` is `None` or empty, returns the body unchanged.
pub fn render_with_front_matter(front_matter: Option<&HashMap<String, Value>>, body: &str) -> String {
    match front_matter {
        Some(data) if !data.is_empty() => {
            let yaml = serde_yaml::to_string(data).unwrap_or_default();
            format!("---\n{}---\n{}", yaml, body)
        }
        _ => body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_no_frontmatter() {
        let content = "# Hello\n\nBody text\n";
        let result = split_front_matter(content);
        assert!(result.data.is_empty());
        assert_eq!(result.body, content);
        assert_eq!(result.line_count, 0);
    }

    #[test]
    fn split_with_frontmatter() {
        let content = "---\ntitle: Test\n---\n# Hello\n\nBody\n";
        let result = split_front_matter(content);
        assert_eq!(
            result.data.get("title").and_then(Value::as_str),
            Some("Test")
        );
        assert_eq!(result.body, "# Hello\n\nBody\n");
        assert!(result.line_count > 0);
    }

    #[test]
    fn render_empty_frontmatter_returns_body() {
        let body = "# Hello\n";
        let rendered = render_with_front_matter(None, body);
        assert_eq!(rendered, body);
    }

    #[test]
    fn render_with_data() {
        let mut data = HashMap::new();
        data.insert("title".to_string(), Value::String("Test".to_string()));
        let body = "# Hello\n";
        let rendered = render_with_front_matter(Some(&data), body);
        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains("title: Test"));
        assert!(rendered.ends_with("---\n# Hello\n"));
    }
}
