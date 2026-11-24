use crate::error::{McpHubError, McpHubResult};

/// Parse a TOML document into a dynamic `toml::Value`. Blank input yields an empty table.
pub fn parse_toml(input: &str) -> McpHubResult<toml::Value> {
    if input.trim().is_empty() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    toml::from_str(input).map_err(|err| McpHubError::ConfigParse {
        path: "<toml>".to_string(),
        detail: err.to_string(),
    })
}

/// Format a TOML key segment using bare identifier when safe, or quoted string otherwise.
pub fn format_toml_key(segment: &str) -> String {
    if !segment.is_empty()
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        segment.to_string()
    } else {
        toml::Value::String(segment.to_string()).to_string()
    }
}

/// Parse a TOML table header line (`[a.b."c"]` or `[[a.b]]`) outside comments, returning `(is_array, segments)`.
fn parse_header_line(trimmed_line: &str) -> Option<(bool, Vec<String>)> {
    let trimmed = trimmed_line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let is_array = trimmed.starts_with("[[");
    let open_len = if is_array { 2 } else { 1 };
    let rest = &trimmed[open_len..];
    // Walk to matching closing bracket respecting quotes.
    let mut in_dq = false;
    let mut in_sq = false;
    let mut escape = false;
    let mut close_idx = None;
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_dq {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_dq = false;
            }
        } else if in_sq {
            if b == b'\'' {
                in_sq = false;
            }
        } else {
            match b {
                b'"' => in_dq = true,
                b'\'' => in_sq = true,
                b']' => {
                    if is_array {
                        if i + 1 < bytes.len() && bytes[i + 1] == b']' {
                            close_idx = Some(i);
                            break;
                        }
                    } else {
                        close_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    let close = close_idx?;
    let tail_after = rest[close + open_len..].trim();
    if !tail_after.is_empty() && !tail_after.starts_with('#') {
        return None;
    }
    let inside = &rest[..close];
    parse_dotted_key_segments(inside).map(|segs| (is_array, segs))
}

fn parse_dotted_key_segments(inside: &str) -> Option<Vec<String>> {
    let mut segs = Vec::new();
    let bytes = inside.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let seg = match bytes[i] {
            b'"' => {
                let start = i;
                i += 1;
                let mut esc = false;
                while i < bytes.len() {
                    if esc {
                        esc = false;
                    } else if bytes[i] == b'\\' {
                        esc = true;
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                let decoded: String = serde_json::from_str(&inside[start..i]).ok()?;
                decoded
            }
            b'\'' => {
                let start = i + 1;
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                if i >= bytes.len() {
                    return None;
                }
                let s = inside[start..i].to_string();
                i += 1;
                s
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b' ' && bytes[i] != b'\t'
                {
                    i += 1;
                }
                inside[start..i].to_string()
            }
        };
        segs.push(seg);
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i < bytes.len() {
            if bytes[i] == b'.' {
                i += 1;
            } else {
                return None;
            }
        }
    }
    if segs.is_empty() { None } else { Some(segs) }
}

#[derive(Debug, Clone)]
struct TomlSectionSpan {
    start_line: usize,
    end_line: usize,
    is_array: bool,
    header_path: Vec<String>,
}

fn scan_toml_sections(lines: &[&str]) -> Vec<TomlSectionSpan> {
    let mut spans = Vec::new();
    let mut multiline_dq = false;
    let mut multiline_sq = false;

    for (idx, line) in lines.iter().enumerate() {
        // Track multiline strings roughly so `[foo]` inside `"""..."""` never looks like a table header.
        let mut chars = line.chars().peekable();
        if !multiline_dq && !multiline_sq {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('#')
                && let Some((is_array, path)) = parse_header_line(trimmed)
            {
                spans.push(TomlSectionSpan {
                    start_line: idx,
                    end_line: lines.len(),
                    is_array,
                    header_path: path,
                });
            }
        }
        let mut in_dq = false;
        let mut in_sq = false;
        let mut esc = false;
        while let Some(c) = chars.next() {
            if multiline_dq {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' && chars.peek() == Some(&'"') {
                    let _ = chars.next();
                    if chars.peek() == Some(&'"') {
                        let _ = chars.next();
                        multiline_dq = false;
                    }
                }
                continue;
            }
            if multiline_sq {
                if c == '\'' && chars.peek() == Some(&'\'') {
                    let _ = chars.next();
                    if chars.peek() == Some(&'\'') {
                        let _ = chars.next();
                        multiline_sq = false;
                    }
                }
                continue;
            }
            if in_dq {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    in_dq = false;
                }
            } else if in_sq {
                if c == '\'' {
                    in_sq = false;
                }
            } else {
                match c {
                    '#' => break,
                    '"' => {
                        if chars.peek() == Some(&'"') {
                            let mut clone = chars.clone();
                            clone.next();
                            if clone.peek() == Some(&'"') {
                                chars.next();
                                chars.next();
                                multiline_dq = true;
                            } else {
                                chars.next();
                            }
                        } else {
                            in_dq = true;
                        }
                    }
                    '\'' => {
                        if chars.peek() == Some(&'\'') {
                            let mut clone = chars.clone();
                            clone.next();
                            if clone.peek() == Some(&'\'') {
                                chars.next();
                                chars.next();
                                multiline_sq = true;
                            } else {
                                chars.next();
                            }
                        } else {
                            in_sq = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    for i in 0..spans.len() {
        if i + 1 < spans.len() {
            spans[i].end_line = spans[i + 1].start_line;
        }
    }
    spans
}

fn render_toml_table_block(
    prefix: &str,
    key: &str,
    table: &toml::map::Map<String, toml::Value>,
) -> McpHubResult<String> {
    let mut wrapper = toml::map::Map::new();
    let mut sub = toml::map::Map::new();
    sub.insert(key.to_string(), toml::Value::Table(table.clone()));
    wrapper.insert(prefix.to_string(), toml::Value::Table(sub));
    let mut serialized = toml::to_string_pretty(&toml::Value::Table(wrapper)).map_err(|err| {
        McpHubError::ConfigEdit {
            path: "<toml>".to_string(),
            detail: err.to_string(),
        }
    })?;
    // `toml::to_string_pretty` skips an empty wrapper header if subtable had zero scalar keys and only sub-subtables,
    // OR outputs `[prefix.key]` plus subtables cleanly. Ensure trimmed trailing newlines match single blank separator.
    serialized = serialized.trim().to_string();
    if serialized.is_empty() {
        serialized = format!("[{}.{}]", prefix, format_toml_key(key));
    }
    Ok(serialized)
}

/// Replace, insert, or delete `[<prefix>.<key>]` (+ any subtables `[<prefix>.<key>.*]`) inside `src`.
/// Unrelated tables (like Codex `[projects.*]`), header comments, and key ordering stay untouched.
pub fn patch_toml_entry(
    src: &str,
    prefix: &str,
    key: &str,
    table: Option<&toml::map::Map<String, toml::Value>>,
) -> McpHubResult<String> {
    let normalized = src.replace("\r\n", "\n");
    let trailing_nl = normalized.ends_with('\n') || normalized.is_empty();
    let lines: Vec<&str> = if normalized.is_empty() {
        Vec::new()
    } else {
        normalized.trim_end_matches('\n').split('\n').collect()
    };

    let spans = scan_toml_sections(&lines);
    let mut keep_ranges = vec![true; lines.len()];
    let mut insert_line: Option<usize> = None;
    let mut last_prefix_end: Option<usize> = None;
    let mut bare_prefix_range: Option<(usize, usize)> = None;

    for span in &spans {
        if span.is_array {
            continue;
        }
        if span.header_path.len() == 1 && span.header_path[0] == prefix {
            bare_prefix_range = Some((span.start_line, span.end_line));
            last_prefix_end = Some(span.end_line);
        } else if span.header_path.len() >= 2 && span.header_path[0] == prefix {
            last_prefix_end = Some(span.end_line);
            if span.header_path[1] == key {
                if insert_line.is_none() {
                    insert_line = Some(span.start_line);
                }
                for flag in &mut keep_ranges[span.start_line..span.end_line] {
                    *flag = false;
                }
            }
        }
    }

    // Remove top-level dotted keys (`mcp_servers.<key>.*`) before the first `[table]` header
    // and dotted keys inside bare `[mcp_servers]` (`<key>.*`) to avoid duplicate-key errors.
    let root_end = spans.first().map(|s| s.start_line).unwrap_or(lines.len());
    mark_dotted_key_removals(&lines, 0, root_end, Some(prefix), key, &mut keep_ranges);
    if let Some((b_start, b_end)) = bare_prefix_range {
        mark_dotted_key_removals(&lines, b_start + 1, b_end, None, key, &mut keep_ranges);
    }

    let rendered_block = match table {
        Some(t) => Some(render_toml_table_block(prefix, key, t)?),
        None => None,
    };

    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len() + 16);
    let mut inserted = false;

    for (idx, line) in lines.iter().enumerate() {
        if Some(idx) == insert_line && !inserted {
            if let Some(block) = &rendered_block {
                for b_line in block.lines() {
                    result_lines.push(b_line.to_string());
                }
                result_lines.push(String::new());
            }
            inserted = true;
        }
        if keep_ranges[idx] {
            result_lines.push((*line).to_string());
        }
        if !inserted && insert_line.is_none() && last_prefix_end == Some(idx + 1)
            && let Some(block) = &rendered_block
        {
            if result_lines.last().is_some_and(|l| !l.is_empty()) {
                result_lines.push(String::new());
            }
            for b_line in block.lines() {
                result_lines.push(b_line.to_string());
            }
            inserted = true;
        }
    }

    if !inserted && let Some(block) = &rendered_block {
        while result_lines.last().is_some_and(|l| l.is_empty()) {
            result_lines.pop();
        }
        if !result_lines.is_empty() {
            result_lines.push(String::new());
        }
        for b_line in block.lines() {
            result_lines.push(b_line.to_string());
        }
    }

    // Trim excess consecutive trailing blank lines.
    while result_lines.last().is_some_and(|l| l.is_empty()) {
        result_lines.pop();
    }

    let mut out = result_lines.join("\n");
    if trailing_nl && !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

fn mark_dotted_key_removals(
    lines: &[&str],
    start_line: usize,
    end_line: usize,
    expected_prefix: Option<&str>,
    key: &str,
    keep_ranges: &mut [bool],
) {
    let mut idx = start_line;
    while idx < end_line {
        let end_of_stmt = find_toml_assignment_end_line(lines, idx, end_line);
        let trimmed = lines[idx].trim_start();
        if !trimmed.starts_with('#')
            && let Some((lhs, _)) = trimmed.split_once('=')
            && let Some(segs) = parse_dotted_key_segments(lhs.trim())
        {
            let matched = match expected_prefix {
                Some(pref) => {
                    segs.len() >= 2 && segs[0] == pref && segs[1] == key
                }
                None => segs.first().map(String::as_str) == Some(key),
            };
            if matched {
                for flag in &mut keep_ranges[idx..=end_of_stmt] {
                    *flag = false;
                }
            }
        }
        idx = end_of_stmt + 1;
    }
}

fn find_toml_assignment_end_line(lines: &[&str], start_idx: usize, limit: usize) -> usize {
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut multiline_dq = false;
    let mut multiline_sq = false;

    for (idx, line) in lines.iter().enumerate().take(limit).skip(start_idx) {
        let mut chars = line.chars().peekable();
        let mut in_dq = false;
        let mut in_sq = false;
        let mut esc = false;
        while let Some(c) = chars.next() {
            if multiline_dq {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' && chars.peek() == Some(&'"') {
                    let _ = chars.next();
                    if chars.peek() == Some(&'"') {
                        let _ = chars.next();
                        multiline_dq = false;
                    }
                }
                continue;
            }
            if multiline_sq {
                if c == '\'' && chars.peek() == Some(&'\'') {
                    let _ = chars.next();
                    if chars.peek() == Some(&'\'') {
                        let _ = chars.next();
                        multiline_sq = false;
                    }
                }
                continue;
            }
            if in_dq {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    in_dq = false;
                }
            } else if in_sq {
                if c == '\'' {
                    in_sq = false;
                }
            } else {
                match c {
                    '#' => break,
                    '"' => {
                        if chars.peek() == Some(&'"') {
                            let mut clone = chars.clone();
                            clone.next();
                            if clone.peek() == Some(&'"') {
                                chars.next();
                                chars.next();
                                multiline_dq = true;
                            } else {
                                chars.next();
                            }
                        } else {
                            in_dq = true;
                        }
                    }
                    '\'' => {
                        if chars.peek() == Some(&'\'') {
                            let mut clone = chars.clone();
                            clone.next();
                            if clone.peek() == Some(&'\'') {
                                chars.next();
                                chars.next();
                                multiline_sq = true;
                            } else {
                                chars.next();
                            }
                        } else {
                            in_sq = true;
                        }
                    }
                    '[' => bracket_depth += 1,
                    ']' => bracket_depth = bracket_depth.saturating_sub(1),
                    '{' => brace_depth += 1,
                    '}' => brace_depth = brace_depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
        if bracket_depth == 0 && brace_depth == 0 && !multiline_dq && !multiline_sq {
            return idx;
        }
    }
    limit.saturating_sub(1).max(start_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_top_level_dotted_keys_when_patching_mcp_server() {
        let src = r#"model = "gpt-5"
mcp_servers.alpha.command = "old"
mcp_servers.alpha.args = [
  "-y",
  "old-pkg",
]
mcp_servers.beta.command = "keep"
"#;
        let mut tbl = toml::map::Map::new();
        tbl.insert("command".into(), toml::Value::String("npx".into()));
        let patched = patch_toml_entry(src, "mcp_servers", "alpha", Some(&tbl)).unwrap();
        assert!(!patched.contains("mcp_servers.alpha."), "{patched}");
        assert!(patched.contains("mcp_servers.beta.command = \"keep\""), "{patched}");
        assert!(patched.contains("[mcp_servers.alpha]"), "{patched}");
        let doc = parse_toml(&patched).unwrap();
        assert_eq!(doc["mcp_servers"]["alpha"]["command"].as_str(), Some("npx"));
        assert_eq!(doc["mcp_servers"]["beta"]["command"].as_str(), Some("keep"));

        let removed = patch_toml_entry(&patched, "mcp_servers", "alpha", None).unwrap();
        let doc2 = parse_toml(&removed).unwrap();
        assert!(doc2["mcp_servers"].get("alpha").is_none());
        assert_eq!(doc2["mcp_servers"]["beta"]["command"].as_str(), Some("keep"));
    }
}
