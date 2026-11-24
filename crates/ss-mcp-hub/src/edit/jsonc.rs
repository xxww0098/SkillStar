use crate::error::{McpHubError, McpHubResult};
use serde_json::{Map, Value};

#[derive(Debug, Clone)]
struct ObjectEntry {
    key: String,
    key_start: usize,
    val_start: usize,
    val_end: usize,
    comma_pos: Option<usize>,
}

/// Strip `//` line comments, `/* ... */` block comments, and trailing commas outside string literals.
pub fn strip_jsonc_to_json(input: &str) -> String {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut out = Vec::with_capacity(len);
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'"' => {
                let start = i;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(len);
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                out.extend_from_slice(&bytes[start..i]);
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] == b'\n' {
                        out.push(b'\n');
                    } else {
                        out.push(b' ');
                    }
                    i += 1;
                }
                i = (i + 2).min(len);
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }

    // Second pass outside strings to eliminate trailing commas `,` before `}` or `]`.
    let mut clean = Vec::with_capacity(out.len());
    let olen = out.len();
    let mut j = 0;
    while j < olen {
        match out[j] {
            b'"' => {
                let start = j;
                j += 1;
                while j < olen {
                    if out[j] == b'\\' {
                        j = (j + 2).min(olen);
                    } else if out[j] == b'"' {
                        j += 1;
                        break;
                    } else {
                        j += 1;
                    }
                }
                clean.extend_from_slice(&out[start..j]);
            }
            b',' => {
                let mut k = j + 1;
                while k < olen && out[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < olen && (out[k] == b'}' || out[k] == b']') {
                    j += 1;
                    continue;
                }
                clean.push(b',');
                j += 1;
            }
            b => {
                clean.push(b);
                j += 1;
            }
        }
    }

    String::from_utf8_lossy(&clean).into_owned()
}

/// Parse JSON or JSONC into a `serde_json::Value`. Blank documents parse as `{}`.
pub fn parse_jsonc(input: &str) -> McpHubResult<Value> {
    let stripped = strip_jsonc_to_json(input);
    if stripped.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(&stripped).map_err(|err| McpHubError::ConfigParse {
        path: "<jsonc>".to_string(),
        detail: err.to_string(),
    })
}

/// Navigate a nested object path inside parsed JSON without mutating anything.
pub fn get_json_path<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path {
        cur = cur.as_object()?.get(*seg)?;
    }
    Some(cur)
}

fn detect_indent_unit(src: &str) -> String {
    for line in src.lines() {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.is_empty() || trimmed == "{" || trimmed == "}" {
            continue;
        }
        let ws_len = line.len() - trimmed.len();
        if ws_len == 0 {
            continue;
        }
        let ws = &line[..ws_len];
        if ws.starts_with('\t') {
            return "\t".to_string();
        }
        if ws.chars().all(|c| c == ' ') && ws.len() <= 8 {
            return ws.to_string();
        }
    }
    "  ".to_string()
}

fn skip_ws_and_comments(bytes: &[u8], mut i: usize) -> usize {
    let len = bytes.len();
    while i < len {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
        } else if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(len);
        } else {
            break;
        }
    }
    i
}

fn scan_string_end(bytes: &[u8], start_quote: usize) -> McpHubResult<(usize, String)> {
    let len = bytes.len();
    if start_quote >= len || bytes[start_quote] != b'"' {
        return Err(McpHubError::ConfigParse {
            path: "<jsonc>".to_string(),
            detail: format!("expected '\"' at offset {start_quote}"),
        });
    }
    let mut i = start_quote + 1;
    while i < len {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == b'"' {
            let raw = std::str::from_utf8(&bytes[start_quote..=i]).map_err(|err| {
                McpHubError::ConfigParse {
                    path: "<jsonc>".to_string(),
                    detail: err.to_string(),
                }
            })?;
            let decoded: String =
                serde_json::from_str(raw).map_err(|err| McpHubError::ConfigParse {
                    path: "<jsonc>".to_string(),
                    detail: err.to_string(),
                })?;
            return Ok((i + 1, decoded));
        } else {
            i += 1;
        }
    }
    Err(McpHubError::ConfigParse {
        path: "<jsonc>".to_string(),
        detail: "unterminated JSON string".to_string(),
    })
}

fn scan_value_end(bytes: &[u8], start: usize) -> McpHubResult<usize> {
    let len = bytes.len();
    if start >= len {
        return Err(McpHubError::ConfigParse {
            path: "<jsonc>".to_string(),
            detail: "unexpected EOF waiting for JSON value".to_string(),
        });
    }
    match bytes[start] {
        b'"' => Ok(scan_string_end(bytes, start)?.0),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = start;
            while i < len {
                match bytes[i] {
                    b'"' => {
                        i = scan_string_end(bytes, i)?.0;
                    }
                    b'/' if i + 1 < len && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*') => {
                        i = skip_ws_and_comments(bytes, i);
                    }
                    b'{' | b'[' => {
                        depth += 1;
                        i += 1;
                    }
                    b'}' | b']' => {
                        depth = depth.saturating_sub(1);
                        i += 1;
                        if depth == 0 {
                            return Ok(i);
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            Err(McpHubError::ConfigParse {
                path: "<jsonc>".to_string(),
                detail: "unterminated object or array".to_string(),
            })
        }
        _ => {
            let mut i = start;
            while i < len {
                let b = bytes[i];
                if b == b',' || b == b'}' || b == b']' || b.is_ascii_whitespace() {
                    break;
                }
                if i + 1 < len && b == b'/' && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*') {
                    break;
                }
                i += 1;
            }
            Ok(i)
        }
    }
}

fn scan_object_entries(
    bytes: &[u8],
    obj_start: usize,
    obj_end: usize,
) -> McpHubResult<(Vec<ObjectEntry>, bool)> {
    let mut entries = Vec::new();
    let mut i = skip_ws_and_comments(bytes, obj_start + 1);
    let mut trailing_comma = false;
    while i < obj_end {
        trailing_comma = false;
        if bytes[i] == b'}' {
            break;
        }
        if bytes[i] != b'"' {
            return Err(McpHubError::ConfigParse {
                path: "<jsonc>".to_string(),
                detail: format!("expected object key at offset {i}"),
            });
        }
        let key_start = i;
        let (after_key, key) = scan_string_end(bytes, key_start)?;
        let colon = skip_ws_and_comments(bytes, after_key);
        if colon >= obj_end || bytes[colon] != b':' {
            return Err(McpHubError::ConfigParse {
                path: "<jsonc>".to_string(),
                detail: format!("expected ':' after key '{key}'"),
            });
        }
        let val_start = skip_ws_and_comments(bytes, colon + 1);
        let val_end = scan_value_end(bytes, val_start)?;
        let after_val = skip_ws_and_comments(bytes, val_end);
        let comma_pos = if after_val < obj_end && bytes[after_val] == b',' {
            trailing_comma = true;
            i = skip_ws_and_comments(bytes, after_val + 1);
            Some(after_val)
        } else {
            i = after_val;
            None
        };
        entries.push(ObjectEntry {
            key,
            key_start,
            val_start,
            val_end,
            comma_pos,
        });
    }
    Ok((entries, trailing_comma))
}

fn find_root_object_span(bytes: &[u8]) -> McpHubResult<(usize, usize)> {
    let start = skip_ws_and_comments(bytes, 0);
    if start >= bytes.len() || bytes[start] != b'{' {
        return Err(McpHubError::ConfigParse {
            path: "<jsonc>".to_string(),
            detail: "root JSON value is not an object".to_string(),
        });
    }
    let end_plus_one = scan_value_end(bytes, start)?;
    Ok((start, end_plus_one - 1))
}

fn line_indent_at(src: &str, offset: usize) -> String {
    let prefix = &src[..offset.min(src.len())];
    let line_start = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let line = &src[line_start..offset.min(src.len())];
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn format_value_with_indent(value: &Value, indent_unit: &str, base_indent: &str) -> McpHubResult<String> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent_unit.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(value, &mut ser).map_err(|err| McpHubError::ConfigEdit {
        path: "<jsonc>".to_string(),
        detail: err.to_string(),
    })?;
    let pretty = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = pretty.split('\n');
    let Some(first) = lines.next() else {
        return Ok(String::new());
    };
    let mut out = String::from(first);
    for line in lines {
        out.push('\n');
        out.push_str(base_indent);
        out.push_str(line);
    }
    Ok(out)
}

fn wrap_nested_value(subpath: &[&str], key: &str, val: &Value) -> Value {
    let mut inner = Map::new();
    inner.insert(key.to_string(), val.clone());
    let mut acc = Value::Object(inner);
    for seg in subpath.iter().rev() {
        let mut m = Map::new();
        m.insert((*seg).to_string(), acc);
        acc = Value::Object(m);
    }
    acc
}

/// Losslessly set (`Some(val)`) or delete (`None`) `entry_key` inside nested object `path`.
/// Preserves existing comments, trailing commas, key order, and indentation.
pub fn patch_json_path(
    src: &str,
    path: &[&str],
    entry_key: &str,
    new_val: Option<&Value>,
) -> McpHubResult<String> {
    let indent_unit = detect_indent_unit(src);
    if src.trim().is_empty() {
        let Some(val) = new_val else {
            return Ok(src.to_string());
        };
        let doc = wrap_nested_value(path, entry_key, val);
        let mut rendered = format_value_with_indent(&doc, &indent_unit, "")?;
        rendered.push('\n');
        return Ok(rendered);
    }

    let bytes = src.as_bytes();
    let (mut obj_start, mut obj_end) = find_root_object_span(bytes)?;

    for (depth, seg) in path.iter().enumerate() {
        let (entries, _) = scan_object_entries(bytes, obj_start, obj_end)?;
        if let Some(found) = entries.into_iter().find(|e| e.key == *seg) {
            if bytes.get(found.val_start) != Some(&b'{') {
                if new_val.is_none() {
                    return Ok(src.to_string());
                }
                let val = new_val.unwrap();
                let wrapped = wrap_nested_value(&path[depth + 1..], entry_key, val);
                let base_indent = line_indent_at(src, found.key_start);
                let rendered = format_value_with_indent(&wrapped, &indent_unit, &base_indent)?;
                let mut out = String::with_capacity(src.len() + rendered.len());
                out.push_str(&src[..found.val_start]);
                out.push_str(&rendered);
                out.push_str(&src[found.val_end..]);
                return Ok(out);
            }
            obj_start = found.val_start;
            obj_end = found.val_end - 1;
        } else {
            let Some(val) = new_val else {
                return Ok(src.to_string());
            };
            let wrapped = wrap_nested_value(&path[depth + 1..], entry_key, val);
            return insert_object_entry(src, obj_start, obj_end, seg, &wrapped, &indent_unit);
        }
    }

    let (entries, trailing_comma) = scan_object_entries(bytes, obj_start, obj_end)?;
    if let Some(idx) = entries.iter().position(|e| e.key == entry_key) {
        if let Some(val) = new_val {
            let base_indent = line_indent_at(src, entries[idx].key_start);
            let rendered = format_value_with_indent(val, &indent_unit, &base_indent)?;
            let mut out = String::with_capacity(src.len() + rendered.len());
            out.push_str(&src[..entries[idx].val_start]);
            out.push_str(&rendered);
            out.push_str(&src[entries[idx].val_end..]);
            Ok(out)
        } else {
            delete_object_entry(src, obj_start, obj_end, &entries, idx, trailing_comma)
        }
    } else if let Some(val) = new_val {
        insert_object_entry(src, obj_start, obj_end, entry_key, val, &indent_unit)
    } else {
        Ok(src.to_string())
    }
}

fn insert_object_entry(
    src: &str,
    obj_start: usize,
    obj_end: usize,
    key: &str,
    val: &Value,
    indent_unit: &str,
) -> McpHubResult<String> {
    let bytes = src.as_bytes();
    let (entries, trailing_comma) = scan_object_entries(bytes, obj_start, obj_end)?;
    let encoded_key = serde_json::to_string(key).map_err(|err| McpHubError::ConfigEdit {
        path: "<jsonc>".to_string(),
        detail: err.to_string(),
    })?;

    if let Some(last) = entries.last() {
        let entry_indent = line_indent_at(src, last.key_start);
        let child_indent = if entry_indent.is_empty() {
            format!("{}{}", line_indent_at(src, obj_start), indent_unit)
        } else {
            entry_indent
        };
        let rendered_val = format_value_with_indent(val, indent_unit, &child_indent)?;
        let insert_comma_prefix = if last.comma_pos.is_some() { "" } else { "," };
        let after_last = last.comma_pos.map(|p| p + 1).unwrap_or(last.val_end);
        let insert_after_comment = same_line_comment_end(src, after_last, obj_end);
        let trailing = if trailing_comma { "," } else { "" };

        // Place new entry after any same-line comment on the previous last entry so
        // inline `// comment` stays attached to that entry instead of shifting onto ours.
        let mut out = String::with_capacity(src.len() + rendered_val.len() + 32);
        out.push_str(&src[..after_last]);
        if !insert_comma_prefix.is_empty() {
            out.push_str(insert_comma_prefix);
        }
        out.push_str(&src[after_last..insert_after_comment]);
        out.push('\n');
        out.push_str(&child_indent);
        out.push_str(&encoded_key);
        out.push_str(": ");
        out.push_str(&rendered_val);
        out.push_str(trailing);
        out.push_str(&src[insert_after_comment..]);
        Ok(out)
    } else {
        let parent_indent = line_indent_at(src, obj_start);
        let child_indent = format!("{parent_indent}{indent_unit}");
        let rendered_val = format_value_with_indent(val, indent_unit, &child_indent)?;
        let inside = &src[obj_start + 1..obj_end];
        let mut out = String::with_capacity(src.len() + rendered_val.len() + 32);
        out.push_str(&src[..=obj_start]);
        if inside.trim().is_empty() {
            out.push('\n');
            out.push_str(&child_indent);
            out.push_str(&encoded_key);
            out.push_str(": ");
            out.push_str(&rendered_val);
            out.push('\n');
            out.push_str(&parent_indent);
            out.push_str(&src[obj_end..]);
        } else {
            // Preserve leading comments inside an otherwise empty object.
            let insert_at = skip_ws_and_comments(bytes, obj_start + 1).min(obj_end);
            out.clear();
            out.push_str(&src[..insert_at]);
            if !src[..insert_at].ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&child_indent);
            out.push_str(&encoded_key);
            out.push_str(": ");
            out.push_str(&rendered_val);
            out.push('\n');
            out.push_str(&parent_indent);
            out.push_str(&src[obj_end..]);
        }
        Ok(out)
    }
}

fn delete_object_entry(
    src: &str,
    obj_start: usize,
    obj_end: usize,
    entries: &[ObjectEntry],
    idx: usize,
    trailing_comma: bool,
) -> McpHubResult<String> {
    let entry = &entries[idx];
    let bytes = src.as_bytes();
    let is_last = idx + 1 == entries.len();
    let line_start = src[..entry.key_start]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let own_line_prefix_blank = src[line_start..entry.key_start]
        .chars()
        .all(|c| c == ' ' || c == '\t')
        && line_start > obj_start;

    let cut_start = if own_line_prefix_blank {
        line_start
    } else {
        entry.key_start
    };
    let mut cut_end = entry.comma_pos.map(|p| p + 1).unwrap_or(entry.val_end);

    if own_line_prefix_blank {
        // Consume rest-of-line horizontal whitespace + single newline when the line has no inline comment.
        let mut cursor = cut_end;
        while cursor < obj_end && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
            cursor += 1;
        }
        if cursor < obj_end && bytes[cursor] == b'\n' {
            cut_end = cursor + 1;
        }
    } else if !is_last {
        while cut_end < obj_end && bytes[cut_end] == b' ' {
            cut_end += 1;
        }
    }

    let mut out = String::with_capacity(src.len());
    if is_last && !trailing_comma && idx > 0 {
        if let Some(prev_comma) = entries[idx - 1].comma_pos {
            // Strip previous trailing comma so strict JSON stays valid when removing the last item.
            out.push_str(&src[..prev_comma]);
            out.push_str(&src[prev_comma + 1..cut_start]);
            out.push_str(&src[cut_end..]);
            return Ok(out);
        }
    }

    out.push_str(&src[..cut_start]);
    out.push_str(&src[cut_end..]);
    Ok(out)
}

fn same_line_comment_end(src: &str, start: usize, obj_end: usize) -> usize {
    let bytes = src.as_bytes();
    let mut i = start;
    let mut comment_end = None;
    while i < obj_end {
        if bytes[i] == b' ' || bytes[i] == b'\t' {
            i += 1;
        } else if i + 1 < obj_end && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            i += 2;
            while i < obj_end && bytes[i] != b'\n' {
                i += 1;
            }
            comment_end = Some(i);
            break;
        } else if i + 1 < obj_end && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let mut j = i + 2;
            let mut closed_same_line = false;
            while j + 1 < obj_end {
                if bytes[j] == b'\n' {
                    break;
                }
                if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                    i = j + 2;
                    comment_end = Some(i);
                    closed_same_line = true;
                    break;
                }
                j += 1;
            }
            if !closed_same_line {
                break;
            }
        } else {
            break;
        }
    }
    comment_end.unwrap_or(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_preserves_inline_comment_on_last_entry_without_trailing_comma() {
        let src = "{\n  \"mcpServers\": {\n    \"first\": { \"command\": \"echo\" } // keep on first\n  }\n}\n";
        let next = patch_json_path(
            src,
            &["mcpServers"],
            "second",
            Some(&serde_json::json!({ "command": "npx" })),
        )
        .unwrap();
        let first_line = next
            .lines()
            .find(|l| l.contains("\"first\""))
            .expect("first line");
        let second_line = next
            .lines()
            .find(|l| l.contains("\"second\""))
            .expect("second line");
        assert!(first_line.contains("// keep on first"), "{next}");
        assert!(first_line.contains("}, // keep on first"), "{next}");
        assert!(!second_line.contains("// keep on first"), "{next}");
        assert!(parse_jsonc(&next).is_ok(), "{next}");
    }
}
