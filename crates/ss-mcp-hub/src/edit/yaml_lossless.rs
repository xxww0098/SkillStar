use crate::env_refs::to_dsh_env_expr;
use crate::error::{McpHubError, McpHubResult};
use crate::model::{McpServer, Transport};
use regex::Regex;
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;
use std::sync::LazyLock;

const DSH_PLUGIN_NAME: &str = "@deepseek-ai/dsh-mcp-client";
const DSH_INSERT_MARK: &str = "- insert: # skillstar";

static DSH_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{1,32}$").unwrap());

/// Parse a YAML file into `serde_yaml::Value`. Blank documents map to an empty Mapping.
pub fn parse_yaml(input: &str) -> McpHubResult<Value> {
    if input.trim().is_empty() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    serde_yaml::from_str(input).map_err(|err| McpHubError::ConfigParse {
        path: "<yaml>".to_string(),
        detail: err.to_string(),
    })
}

/// Top-level line block editor for YAML configs (e.g. Goose `extensions`, Hermes `mcp_servers`).
/// Unrelated top-level sections, document markers (`---`), and surrounding comments stay byte-intact.
pub fn patch_yaml_section_entry(
    src: &str,
    section_key: &str,
    entry_key: &str,
    entry_val: Option<&Value>,
) -> McpHubResult<String> {
    let mut parsed = parse_yaml(src)?;
    let root_map = parsed
        .as_mapping_mut()
        .ok_or_else(|| McpHubError::ConfigParse {
            path: "<yaml>".to_string(),
            detail: "expected top-level YAML mapping".to_string(),
        })?;
    let sec_k = Value::String(section_key.to_string());
    let ent_k = Value::String(entry_key.to_string());

    if let Some(val) = entry_val {
        let mut sec_map = root_map
            .get(&sec_k)
            .and_then(|v| v.as_mapping())
            .cloned()
            .unwrap_or_default();
        sec_map.insert(ent_k, val.clone());
        root_map.insert(sec_k.clone(), Value::Mapping(sec_map));
    } else if let Some(sec_map) = root_map.get_mut(&sec_k).and_then(|v| v.as_mapping_mut()) {
        sec_map.remove(&ent_k);
        if sec_map.is_empty() {
            // Keep explicit empty section or remove if absent cleanly — preserve sibling behavior cleanly.
            root_map.insert(sec_k.clone(), Value::Mapping(Mapping::new()));
        }
    } else {
        return Ok(src.to_string());
    }

    let rendered_section = match root_map.get(&sec_k) {
        Some(section_val) => {
            let mut single = Mapping::new();
            single.insert(sec_k, section_val.clone());
            let yaml = serde_yaml::to_string(&Value::Mapping(single)).map_err(|err| {
                McpHubError::ConfigEdit {
                    path: "<yaml>".to_string(),
                    detail: err.to_string(),
                }
            })?;
            yaml.strip_prefix("---\n")
                .unwrap_or(&yaml)
                .trim_end_matches('\n')
                .to_string()
        }
        None => String::new(),
    };

    let normalized = src.replace("\r\n", "\n");
    if normalized.trim().is_empty() {
        return Ok(if rendered_section.is_empty() {
            String::new()
        } else {
            format!("{rendered_section}\n")
        });
    }

    let lines: Vec<&str> = normalized.trim_end_matches('\n').split('\n').collect();
    let (sec_start, sec_end) = find_top_level_yaml_block(&lines, section_key);

    let mut out_lines: Vec<String> = Vec::with_capacity(lines.len() + 16);
    if let Some((start, end)) = sec_start.zip(sec_end) {
        for line in &lines[..start] {
            out_lines.push((*line).to_string());
        }
        if !rendered_section.is_empty() {
            for r_line in rendered_section.lines() {
                out_lines.push(r_line.to_string());
            }
        }
        for line in &lines[end..] {
            out_lines.push((*line).to_string());
        }
    } else {
        for line in &lines {
            out_lines.push((*line).to_string());
        }
        if !rendered_section.is_empty() {
            if out_lines.last().is_some_and(|l| !l.trim().is_empty()) {
                out_lines.push(String::new());
            }
            for r_line in rendered_section.lines() {
                out_lines.push(r_line.to_string());
            }
        }
    }

    let mut out = out_lines.join("\n").trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

fn find_top_level_yaml_block(lines: &[&str], section_key: &str) -> (Option<usize>, Option<usize>) {
    let quoted = format!("\"{section_key}\":");
    let squoted = format!("'{section_key}':");
    let bare = format!("{section_key}:");

    let mut start = None;
    let mut end = None;
    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with(' ') || line.starts_with('\t') || line.starts_with('#') {
            continue;
        }
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." || trimmed.is_empty() {
            continue;
        }
        if start.is_none() {
            if line.starts_with(&bare) || line.starts_with(&quoted) || line.starts_with(&squoted) {
                start = Some(idx);
            }
        } else {
            end = Some(idx);
            break;
        }
    }
    let Some(s) = start else {
        return (None, None);
    };
    let mut e = end.unwrap_or(lines.len());
    // Exclude trailing blank lines and top-level `#` comments before the next section or EOF
    // so comments between top-level sections remain untouched outside the replaced block.
    while e > s + 1 && (lines[e - 1].trim().is_empty() || lines[e - 1].starts_with('#')) {
        e -= 1;
    }
    (Some(s), Some(e))
}

/// Validate server name constraint enforced by DeepSeek Harness (`@deepseek-ai/dsh-mcp-client`).
pub fn validate_dsh_name(name: &str) -> McpHubResult<()> {
    if DSH_NAME_RE.is_match(name) {
        Ok(())
    } else {
        Err(McpHubError::UnsupportedForAgent {
            agent: "dsh".to_string(),
            reason: "DeepSeek Harness takes a server name of at most 32 letters, digits, - and _"
                .to_string(),
        })
    }
}

/// Parse existing DeepSeek Harness `- insert:` patch rows representing `@deepseek-ai/dsh-mcp-client`.
pub fn dsh_extract_servers(src: &str) -> Vec<McpServer> {
    let sanitized = neutralize_yaml_js_tags(src);
    let Ok(val) = parse_yaml(&sanitized) else {
        return Vec::new();
    };
    let Some(seq) = val.as_sequence() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in seq {
        let Some(map) = item.as_mapping() else {
            continue;
        };
        let Some(insert_rows) = map
            .get(Value::String("insert".to_string()))
            .and_then(|v| v.as_sequence())
        else {
            continue;
        };
        for row in insert_rows {
            let Some(rmap) = row.as_mapping() else {
                continue;
            };
            let is_mcp_client = rmap
                .get(Value::String("name".to_string()))
                .and_then(|v| v.as_str())
                == Some(DSH_PLUGIN_NAME);
            if !is_mcp_client {
                continue;
            }
            let Some(cfg) = rmap
                .get(Value::String("config".to_string()))
                .and_then(|v| v.as_mapping())
            else {
                continue;
            };
            let Some(server_name) = cfg
                .get(Value::String("serverName".to_string()))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let transport = match cfg
                .get(Value::String("transport".to_string()))
                .and_then(|v| v.as_str())
                .unwrap_or("stdio")
            {
                "http" => Transport::Http,
                "sse" => Transport::Sse,
                _ => Transport::Stdio,
            };
            let command = cfg
                .get(Value::String("command".to_string()))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = cfg
                .get(Value::String("args".to_string()))
                .and_then(|v| v.as_sequence())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let env = extract_yaml_string_map(cfg.get(Value::String("env".to_string())));
            let url = cfg
                .get(Value::String("url".to_string()))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let headers = extract_yaml_string_map(cfg.get(Value::String("headers".to_string())));
            out.push(McpServer {
                name: server_name.to_string(),
                transport,
                command,
                args,
                env,
                url,
                headers,
                ..Default::default()
            });
        }
    }
    out
}

fn extract_yaml_string_map(val: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(map) = val.and_then(|v| v.as_mapping()) else {
        return out;
    };
    for (k, v) in map {
        if let Some(ks) = k.as_str() {
            if let Some(vs) = v.as_str() {
                // Restore `!!js process.env.VAR` markers back to canonical `${VAR}`.
                if let Some(rest) = vs.strip_prefix("__SKILLSTAR_JS__:process.env.") {
                    out.insert(ks.to_string(), format!("${{{rest}}}"));
                } else {
                    out.insert(ks.to_string(), vs.to_string());
                }
            }
        }
    }
    out
}

fn neutralize_yaml_js_tags(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        if let Some(idx) = line.find("!!js ") {
            let (before, after) = line.split_at(idx);
            let expr = after.trim_start_matches("!!js ").trim();
            let encoded = serde_json::to_string(&format!("__SKILLSTAR_JS__:{expr}"))
                .unwrap_or_else(|_| "\"\"".to_string());
            out.push_str(before);
            out.push_str(&encoded);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Losslessly upsert or remove a `- insert: # skillstar` patch item in a DSH patch file,
/// preserving `---`, header comments, and non-SkillStar user patch entries.
pub fn dsh_patch_server(
    src: &str,
    server_name: &str,
    server: Option<&McpServer>,
) -> McpHubResult<String> {
    if server.is_some() {
        validate_dsh_name(server_name)?;
        if let Some(s) = server
            && s.transport == Transport::Sse
        {
            return Err(McpHubError::UnsupportedForAgent {
                agent: "dsh".to_string(),
                reason: "DeepSeek Harness does not support SSE MCP servers".to_string(),
            });
        }
    }

    if !src.trim().is_empty() {
        let sanitized = neutralize_yaml_js_tags(src);
        let parsed = parse_yaml(&sanitized)?;
        if !matches!(parsed, Value::Sequence(_) | Value::Null) {
            return Err(McpHubError::ConfigParse {
                path: "<dsh-yaml>".to_string(),
                detail: "expected top-level YAML sequence for DSH patch config".to_string(),
            });
        }
    }

    let normalized = src.replace("\r\n", "\n");
    let (head, mut items) = split_dsh_items(&normalized);

    let target_id = format!("skillstar-mcp-{server_name}");
    items.retain(|item| !is_owned_dsh_insert(item, &target_id, server_name));

    if let Some(srv) = server {
        items.push(render_dsh_insert_block(srv));
    }

    let mut out_parts = Vec::new();
    if !head.is_empty() {
        out_parts.push(head.join("\n"));
    }
    for item in items {
        out_parts.push(item.join("\n"));
    }
    let mut result = out_parts.join("\n");
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

fn split_dsh_items(src: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut head = Vec::new();
    let mut items: Vec<Vec<String>> = Vec::new();
    for line in src.trim_end_matches('\n').split('\n') {
        if src.trim().is_empty() {
            break;
        }
        if line.starts_with("- ") || line == "-" {
            items.push(vec![line.to_string()]);
        } else if let Some(last) = items.last_mut() {
            last.push(line.to_string());
        } else {
            head.push(line.to_string());
        }
    }
    (head, items)
}

fn is_owned_dsh_insert(item_lines: &[String], target_id: &str, server_name: &str) -> bool {
    let Some(first) = item_lines.first() else {
        return false;
    };
    let is_marked = first.contains("# skillstar") || first.contains("# magpie");
    if !is_marked {
        return false;
    }
    let joined = item_lines.join("\n");
    joined.contains(&format!("id: {target_id}"))
        || joined.contains(&format!("serverName: {server_name}"))
        || joined.contains(&format!("serverName: \"{server_name}\""))
}

fn render_dsh_insert_block(server: &McpServer) -> Vec<String> {
    let mut lines = vec![
        DSH_INSERT_MARK.to_string(),
        format!("    - id: skillstar-mcp-{}", server.name),
        format!("      name: \"{DSH_PLUGIN_NAME}\""),
        "      config:".to_string(),
        format!("        serverName: {}", yaml_scalar(&server.name)),
    ];
    match server.transport {
        Transport::Stdio => {
            lines.push("        transport: stdio".to_string());
            lines.push(format!("        command: {}", yaml_scalar(&server.command)));
            if !server.args.is_empty() {
                lines.push("        args:".to_string());
                for arg in &server.args {
                    lines.push(format!("          - {}", yaml_scalar(arg)));
                }
            }
            if !server.env.is_empty() {
                lines.push("        env:".to_string());
                for (k, v) in &server.env {
                    lines.push(format!("          {}: {}", yaml_scalar(k), yaml_env_or_scalar(v)));
                }
            }
        }
        Transport::Http | Transport::Sse => {
            lines.push("        transport: http".to_string());
            lines.push(format!("        url: {}", yaml_scalar(&server.url)));
            if !server.headers.is_empty() {
                lines.push("        headers:".to_string());
                for (k, v) in &server.headers {
                    lines.push(format!("          {}: {}", yaml_scalar(k), yaml_env_or_scalar(v)));
                }
            }
        }
    }
    lines
}

fn yaml_scalar(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

fn yaml_env_or_scalar(s: &str) -> String {
    if let Some(js) = to_dsh_env_expr(s) {
        format!("!!js {js}")
    } else {
        yaml_scalar(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_yaml_preserves_inter_section_comments_and_dsh_checks_sequence() {
        let yaml_src = "extensions:\n  old:\n    cmd: echo\n\n# User model block\nGOOSE_MODEL: gpt-5\n";
        let mut entry = Mapping::new();
        entry.insert("cmd".into(), "npx".into());
        let patched = patch_yaml_section_entry(yaml_src, "extensions", "new", Some(&Value::Mapping(entry))).unwrap();
        assert!(patched.contains("# User model block"), "{patched}");
        assert!(patched.contains("GOOSE_MODEL: gpt-5"), "{patched}");

        let bad_dsh = "some_mapping:\n  key: value\n";
        let srv = McpServer {
            name: "demo".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            ..Default::default()
        };
        let err = dsh_patch_server(bad_dsh, "demo", Some(&srv)).unwrap_err();
        assert!(err.to_string().contains("sequence"), "{err}");
    }
}
