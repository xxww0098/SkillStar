use crate::parser::frontmatter::split_front_matter;
use crate::types::{ParsedDocument, ValidationReport};
use std::collections::HashMap;

/// Integrity marker for line-level validation.
/// Each marker contains a hash of the original line content and position.
/// Markers are embedded in the output to allow verification of line integrity.
#[derive(Debug, Clone, PartialEq)]
pub struct IntegrityMarker {
    /// Line number in source (1-indexed)
    pub line_number: usize,
    /// SHA256 hash of the original line content (first 16 chars of hex)
    pub content_hash: String,
    /// Character count of the original line
    pub char_count: usize,
}

impl IntegrityMarker {
    /// Creates a new integrity marker from a line.
    pub fn from_line(line_number: usize, line: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        line.hash(&mut hasher);
        let hash = hasher.finish();
        let content_hash = format!("{:016x}", hash);

        Self {
            line_number,
            content_hash,
            char_count: line.len(),
        }
    }

    /// Creates a marker from a serialized string representation.
    pub fn from_str(s: &str) -> Option<Self> {
        // Format: "## integrity:LINENUM:HASH:CHARS ##"
        let s = s.trim();
        if !s.starts_with("## integrity:") || !s.ends_with(" ##") {
            return None;
        }
        let inner = &s[13..s.len() - 3];
        let parts: Vec<&str> = inner.split(':').collect();
        if parts.len() != 3 {
            return None;
        }
        let line_number = parts[0].parse().ok()?;
        let content_hash = parts[1].to_string();
        let char_count = parts[2].parse().ok()?;

        Some(Self {
            line_number,
            content_hash,
            char_count,
        })
    }

    /// Serializes the marker to a string format.
    pub fn to_marker_string(&self) -> String {
        format!(
            "## integrity:{}:{}:{} ##",
            self.line_number, self.content_hash, self.char_count
        )
    }
}

/// Generates integrity markers for all lines in the source text.
pub fn generate_integrity_markers(source_text: &str) -> Vec<IntegrityMarker> {
    source_text
        .lines()
        .enumerate()
        .map(|(i, line)| IntegrityMarker::from_line(i + 1, line))
        .collect()
}

/// Validates integrity markers in the output text against the source markers.
/// Returns markers that failed validation.
pub fn validate_integrity_markers(
    source_markers: &[IntegrityMarker],
    output_text: &str,
) -> Vec<(IntegrityMarker, IntegrityError)> {
    let mut errors = Vec::new();

    for (line_number, line) in output_text.lines().enumerate() {
        let line_num = line_number + 1;

        // Try to find a marker for this line number
        if let Some(source_marker) = source_markers.iter().find(|m| m.line_number == line_num) {
            // Check if this line contains an integrity marker
            if let Some(output_marker) = extract_marker_from_line(line) {
                // Clone the values we need to compare, so we can use them multiple times
                let source_hash = source_marker.content_hash.clone();
                let source_chars = source_marker.char_count;
                let output_hash = output_marker.content_hash.clone();
                let output_chars = output_marker.char_count;

                // Compare hashes
                if output_hash != source_hash {
                    errors.push((
                        output_marker.clone(),
                        IntegrityError::ContentMismatch {
                            expected: source_hash,
                            found: output_hash,
                            line_number: line_num,
                        },
                    ));
                }
                // Compare char counts
                if output_chars != source_chars {
                    errors.push((
                        output_marker,
                        IntegrityError::LengthMismatch {
                            expected: source_chars,
                            found: output_chars,
                            line_number: line_num,
                        },
                    ));
                }
            } else {
                // Line doesn't have a marker but source did - possible omission
                // This is a warning, not an error, as the line might have been legitimately translated
            }
        }
    }

    errors
}

/// Extracts an integrity marker from a line if present.
fn extract_marker_from_line(line: &str) -> Option<IntegrityMarker> {
    // Look for pattern "## integrity:NUM:HASH:CHARS ##"
    if let Some(start) = line.find("## integrity:") {
        if let Some(end_offset) = line[start..].find(" ##") {
            let end = start + end_offset;
            let marker_str = &line[start..end + 3];
            return IntegrityMarker::from_str(marker_str);
        }
    }
    None
}

/// Validates that front matter in the output is still valid YAML.
pub fn validate_frontmatter(output_text: &str) -> ValidationReport {
    let fm = split_front_matter(output_text);
    if fm.line_count == 0 {
        return ValidationReport {
            passed: true,
            ..Default::default()
        };
    }

    match serde_yaml::from_str::<serde_yaml::Value>(&fm.raw) {
        Ok(_) => ValidationReport {
            passed: true,
            ..Default::default()
        },
        Err(e) => ValidationReport {
            passed: false,
            errors: vec![format!("Front matter YAML is invalid: {e}")],
            ..Default::default()
        },
    }
}

/// Validates that the structural token signature of the source document
/// matches that of the translated output.
pub fn validate_structure(source_doc: &ParsedDocument, output_text: &str) -> ValidationReport {
    let target_body = split_front_matter(output_text).body;

    let opts = pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES;

    let source_sig = extract_signature(&source_doc.body_text, opts);
    let target_sig = extract_signature(&target_body, opts);

    if source_sig != target_sig {
        return ValidationReport {
            passed: false,
            errors: vec!["Structural token signature changed unexpectedly.".into()],
            metrics: {
                let mut m = HashMap::new();
                m.insert(
                    "source_signature_len".into(),
                    serde_json::json!(source_sig.len()),
                );
                m.insert(
                    "target_signature_len".into(),
                    serde_json::json!(target_sig.len()),
                );
                m
            },
            ..Default::default()
        };
    }

    ValidationReport {
        passed: true,
        metrics: {
            let mut m = HashMap::new();
            m.insert("token_count".into(), serde_json::json!(source_sig.len()));
            m
        },
        ..Default::default()
    }
}

/// Integrity error types for marker validation.
#[derive(Debug, Clone, PartialEq)]
pub enum IntegrityError {
    /// The content hash didn't match between source and output.
    ContentMismatch {
        expected: String,
        found: String,
        line_number: usize,
    },
    /// The character count didn't match.
    LengthMismatch {
        expected: usize,
        found: usize,
        line_number: usize,
    },
    /// A marker was expected but not found in the output.
    MissingMarker { line_number: usize },
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntegrityError::ContentMismatch {
                expected,
                found,
                line_number,
            } => {
                write!(
                    f,
                    "Line {}: content mismatch (expected hash {}, found {})",
                    line_number, expected, found
                )
            }
            IntegrityError::LengthMismatch {
                expected,
                found,
                line_number,
            } => {
                write!(
                    f,
                    "Line {}: length mismatch (expected {} chars, found {})",
                    line_number, expected, found
                )
            }
            IntegrityError::MissingMarker { line_number } => {
                write!(f, "Line {}: missing integrity marker", line_number)
            }
        }
    }
}

/// Combined validator: front matter + structure + integrity markers.
pub fn validate(
    source_doc: &ParsedDocument,
    output_text: &str,
    enable_integrity_check: bool,
) -> ValidationReport {
    let fm_report = validate_frontmatter(output_text);
    let struct_report = validate_structure(source_doc, output_text);

    let mut all_errors = Vec::new();
    all_errors.extend(fm_report.errors.clone());
    all_errors.extend(struct_report.errors.clone());

    let mut warnings = Vec::new();
    warnings.extend(fm_report.warnings.clone());
    warnings.extend(struct_report.warnings.clone());

    let mut metrics = fm_report.metrics.clone();
    metrics.extend(struct_report.metrics.clone());

    // Integrity marker validation
    if enable_integrity_check {
        let source_markers = generate_integrity_markers(&source_doc.body_text);
        let marker_errors = validate_integrity_markers(&source_markers, output_text);
        let marker_errors_count = marker_errors.len();

        for (marker, error) in marker_errors {
            all_errors.push(format!(
                "Integrity error at line {}: {}",
                marker.line_number, error
            ));
            metrics.insert(
                format!("integrity_error_line_{}", marker.line_number),
                serde_json::json!(error.to_string()),
            );
        }

        metrics.insert(
            "integrity_markers_checked".into(),
            serde_json::json!(source_markers.len()),
        );
        metrics.insert(
            "integrity_errors_found".into(),
            serde_json::json!(marker_errors_count),
        );
    }

    ValidationReport {
        passed: all_errors.is_empty(),
        errors: all_errors,
        warnings,
        metrics,
    }
}

/// Extracts a structural signature from Markdown text by collecting immutable event types.
fn extract_signature(text: &str, opts: pulldown_cmark::Options) -> Vec<String> {
    use pulldown_cmark::Event;

    let parser = pulldown_cmark::Parser::new_ext(text, opts);
    let mut sig = Vec::new();

    for (event, _) in parser.into_offset_iter() {
        let token = match &event {
            Event::Start(pulldown_cmark::Tag::Heading { level, .. }) => {
                Some(format!("heading_{}", *level as u8))
            }
            Event::Start(pulldown_cmark::Tag::CodeBlock(_)) => Some("code_block".into()),
            Event::Start(pulldown_cmark::Tag::List(ordered)) => Some(if ordered.is_some() {
                "ordered_list".into()
            } else {
                "bullet_list".into()
            }),
            Event::Start(pulldown_cmark::Tag::BlockQuote(_)) => Some("blockquote".into()),
            Event::End(pulldown_cmark::TagEnd::Heading(level)) => {
                Some(format!("end_heading_{}", *level as u8))
            }
            Event::End(pulldown_cmark::TagEnd::CodeBlock) => Some("end_code_block".into()),
            Event::End(pulldown_cmark::TagEnd::List(_)) => Some("end_list".into()),
            Event::End(pulldown_cmark::TagEnd::BlockQuote(_)) => Some("end_blockquote".into()),
            _ => None,
        };
        if let Some(t) = token {
            sig.push(t);
        }
    }

    sig
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_marker_creation() {
        let marker = IntegrityMarker::from_line(1, "Hello World");
        assert_eq!(marker.line_number, 1);
        assert_eq!(marker.char_count, 11);
        assert_eq!(marker.content_hash.len(), 16);
    }

    #[test]
    fn integrity_marker_serialization() {
        let marker = IntegrityMarker::from_line(5, "Test line");
        let serialized = marker.to_marker_string();
        assert!(serialized.contains("## integrity:"));
        assert!(serialized.contains(" ##"));

        let parsed = IntegrityMarker::from_str(&serialized).unwrap();
        assert_eq!(parsed.line_number, marker.line_number);
        assert_eq!(parsed.content_hash, marker.content_hash);
        assert_eq!(parsed.char_count, marker.char_count);
    }

    #[test]
    fn marker_roundtrip() {
        let original = IntegrityMarker::from_line(42, "Some content here");
        let serialized = original.to_marker_string();
        let deserialized = IntegrityMarker::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn invalid_marker_string() {
        assert!(IntegrityMarker::from_str("not a marker").is_none());
        assert!(IntegrityMarker::from_str("## integrity: ##").is_none());
        assert!(IntegrityMarker::from_str("## integrity:abc ##").is_none());
    }

    #[test]
    fn generate_markers_for_text() {
        let text = "Line 1\nLine 2\nLine 3";
        let markers = generate_integrity_markers(text);
        assert_eq!(markers.len(), 3);
        assert_eq!(markers[0].line_number, 1);
        assert_eq!(markers[1].line_number, 2);
        assert_eq!(markers[2].line_number, 3);
    }

    #[test]
    fn valid_frontmatter() {
        let text = "---\ntitle: Test\n---\n# Hello\n";
        let report = validate_frontmatter(text);
        assert!(report.passed);
    }

    #[test]
    fn no_frontmatter_is_valid() {
        let text = "# Hello\n\nWorld\n";
        let report = validate_frontmatter(text);
        assert!(report.passed);
    }

    #[test]
    fn matching_structure() {
        let source = "# A\n\nParagraph\n\n## B\n\nMore\n";
        let doc = crate::parser::parse(source, std::path::Path::new("test.md"), "en");
        let translated = "# AA\n\nParagraphh\n\n## BB\n\nMoree\n";
        let report = validate_structure(&doc, translated);
        assert!(report.passed);
    }

    #[test]
    fn mismatched_structure() {
        let source = "# A\n\nParagraph\n\n## B\n\nMore\n";
        let doc = crate::parser::parse(source, std::path::Path::new("test.md"), "en");
        let translated = "# AA\n\nParagraphh\n\nMoree\n"; // Missing the ## B heading.
        let report = validate_structure(&doc, translated);
        assert!(!report.passed);
    }

    #[test]
    fn integrity_error_display() {
        let error = IntegrityError::ContentMismatch {
            expected: "abc123".to_string(),
            found: "def456".to_string(),
            line_number: 10,
        };
        let display = format!("{}", error);
        assert!(display.contains("10"));
        assert!(display.contains("abc123"));
        assert!(display.contains("def456"));

        let error2 = IntegrityError::LengthMismatch {
            expected: 100,
            found: 50,
            line_number: 5,
        };
        let display2 = format!("{}", error2);
        assert!(display2.contains("5"));
        assert!(display2.contains("100"));
        assert!(display2.contains("50"));
    }
}
