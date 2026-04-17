use std::collections::HashMap;

/// Parsed front matter block.
#[derive(Debug, Clone)]
pub struct FrontMatterSplit {
    pub raw: String,
    pub data: HashMap<String, serde_yaml::Value>,
    pub body: String,
    /// Number of lines consumed by the front matter block (including delimiters).
    pub line_count: usize,
}

/// Split a Markdown document into front matter and body.
///
/// Expects YAML front matter delimited by `---` lines at the very beginning of the file.
pub fn split_front_matter(text: &str) -> FrontMatterSplit {
    let lines: Vec<&str> = text.lines().collect();

    if lines.len() < 3 || lines[0].trim() != "---" {
        return FrontMatterSplit {
            raw: String::new(),
            data: HashMap::new(),
            body: text.to_owned(),
            line_count: 0,
        };
    }

    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            let raw_lines = &lines[1..i];
            let raw = raw_lines.join("\n");
            let data: HashMap<String, serde_yaml::Value> =
                serde_yaml::from_str(&raw).unwrap_or_default();

            // Body starts after the closing `---` line.
            let body_start_byte = text
                .lines()
                .take(i + 1)
                .map(|l| l.len() + 1) // +1 for the newline
                .sum::<usize>();
            let body = if body_start_byte <= text.len() {
                &text[body_start_byte..]
            } else {
                ""
            };

            return FrontMatterSplit {
                raw,
                data,
                body: body.to_owned(),
                line_count: i + 1,
            };
        }
    }

    // No closing `---` found.
    FrontMatterSplit {
        raw: String::new(),
        data: HashMap::new(),
        body: text.to_owned(),
        line_count: 0,
    }
}

/// Serialize front matter data back to a YAML string (without delimiters).
pub fn dump_front_matter(data: &HashMap<String, serde_yaml::Value>) -> String {
    serde_yaml::to_string(data).unwrap_or_default().trim().to_owned()
}

/// Reconstruct the full document text from optional front matter and body.
pub fn render_with_front_matter(
    front_matter: Option<&HashMap<String, serde_yaml::Value>>,
    body: &str,
) -> String {
    match front_matter {
        Some(data) if !data.is_empty() => {
            let yaml = dump_front_matter(data);
            format!("---\n{yaml}\n---\n{body}")
        }
        _ => body.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_no_front_matter() {
        let text = "# Hello\n\nWorld";
        let result = split_front_matter(text);
        assert_eq!(result.line_count, 0);
        assert_eq!(result.body, text);
        assert!(result.data.is_empty());
    }

    #[test]
    fn split_with_front_matter() {
        let text = "---\ntitle: Test\ndescription: A test\n---\n# Hello\n\nWorld\n";
        let result = split_front_matter(text);
        assert_eq!(result.line_count, 4);
        assert_eq!(result.data.get("title").unwrap(), "Test");
        assert!(result.body.starts_with("# Hello"));
    }

    #[test]
    fn dump_roundtrip() {
        let mut data = HashMap::new();
        data.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Hello".to_owned()),
        );
        let yaml = dump_front_matter(&data);
        assert!(yaml.contains("title: Hello"));
    }

    #[test]
    fn render_with_fm() {
        let mut data = HashMap::new();
        data.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Test".to_owned()),
        );
        let body = "# Heading\n\nContent\n";
        let rendered = render_with_front_matter(Some(&data), body);
        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains("title: Test"));
        assert!(rendered.ends_with(body));
    }
}
