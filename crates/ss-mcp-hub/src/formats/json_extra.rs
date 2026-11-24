use crate::edit::file::{read_lossless, write_lossless_with_backup, TextPreservation};
use crate::edit::jsonc::{parse_jsonc, patch_json_path};
use crate::env_refs::{render_env_syntax, EnvRefStyle};
use crate::error::McpHubResult;
use crate::formats::json_formats::{build_json_entry, decode_json_entry};
use crate::formats::FileSyncOutcome;
use crate::model::{McpFormat, McpServer, Transport};
use crate::targets::BuiltInTargetSpec;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

// ── LiveAgent (`{ servers: [{ id, enabled, transport, command, args, env, url, headers }], selected }`) ──

fn load_liveagent_seed(
    spec: &BuiltInTargetSpec,
    primary_path: &Path,
) -> McpHubResult<(Option<Vec<u8>>, String, TextPreservation, Value)> {
    if let Some((raw, text, pres, _)) = read_lossless(primary_path)? {
        let val = parse_jsonc(&text)?;
        return Ok((Some(raw), text, pres, val));
    }
    if let Some(seed_path) = &spec.seed_from_path
        && let Some((_, seed_text, _, _)) = read_lossless(seed_path)?
    {
        let seed_doc = parse_jsonc(&seed_text)?;
        if let Some(mcp_val) = seed_doc.get("mcp").filter(|v| v.is_object()) {
            return Ok((None, String::new(), TextPreservation::default(), mcp_val.clone()));
        }
    }
    Ok((None, String::new(), TextPreservation::default(), Value::Object(Map::new())))
}

pub(super) fn read_liveagent_servers(
    spec: &BuiltInTargetSpec,
    path: &Path,
) -> McpHubResult<Vec<McpServer>> {
    let (_, _, _, doc) = load_liveagent_seed(spec, path)?;
    let Some(arr) = doc.get("servers").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in arr {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(id) = obj.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
            continue;
        };
        if let Some(srv) = decode_json_entry(McpFormat::LiveAgent, id, obj) {
            out.push(srv);
        }
    }
    Ok(out)
}

pub(super) fn write_liveagent_server(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server: &McpServer,
) -> McpHubResult<FileSyncOutcome> {
    let (raw_opt, text, pres, doc) = load_liveagent_seed(spec, path)?;
    let mut servers_arr = doc
        .get("servers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let existing_idx = servers_arr
        .iter()
        .position(|v| v.get("id").and_then(|id| id.as_str()) == Some(server.name.as_str()));
    let existing_map = existing_idx
        .and_then(|idx| servers_arr.get(idx))
        .and_then(|v| v.as_object())
        .cloned();
    let mut entry = existing_map.unwrap_or_default();
    entry.insert("id".into(), Value::String(server.name.clone()));
    entry.insert("enabled".into(), Value::Bool(true));
    if server.transport == Transport::Stdio {
        entry.insert("transport".into(), Value::String("stdio".into()));
        entry.insert(
            "command".into(),
            Value::String(render_env_syntax(&server.command, EnvRefStyle::Plain)),
        );
        if server.args.is_empty() {
            entry.remove("args");
        } else {
            entry.insert(
                "args".into(),
                Value::Array(
                    server
                        .args
                        .iter()
                        .map(|a| Value::String(render_env_syntax(a, EnvRefStyle::Plain)))
                        .collect(),
                ),
            );
        }
        if server.env.is_empty() {
            entry.remove("env");
        } else {
            entry.insert(
                "env".into(),
                Value::Object(
                    server
                        .env
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(render_env_syntax(v, EnvRefStyle::Plain))))
                        .collect(),
                ),
            );
        }
        entry.remove("url");
        entry.remove("headers");
    } else {
        entry.insert("transport".into(), Value::String("http".into()));
        entry.insert(
            "url".into(),
            Value::String(render_env_syntax(&server.url, EnvRefStyle::Plain)),
        );
        if server.headers.is_empty() {
            entry.remove("headers");
        } else {
            entry.insert(
                "headers".into(),
                Value::Object(
                    server
                        .headers
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(render_env_syntax(v, EnvRefStyle::Plain))))
                        .collect(),
                ),
            );
        }
        entry.remove("command");
        entry.remove("args");
        entry.remove("env");
    }

    if let Some(idx) = existing_idx {
        servers_arr[idx] = Value::Object(entry);
    } else {
        servers_arr.push(Value::Object(entry));
    }

    let base_text = if text.is_empty() && !doc.as_object().is_none_or(|m| m.is_empty()) {
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    } else {
        text
    };
    let patched = patch_json_path(&base_text, &[], "servers", Some(&Value::Array(servers_arr)))?;
    let _ = write_lossless_with_backup(path, raw_opt.as_deref(), &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: BTreeSet::new(),
    })
}

pub(super) fn remove_liveagent_server(
    _spec: &BuiltInTargetSpec,
    path: &Path,
    server_name: &str,
) -> McpHubResult<FileSyncOutcome> {
    let Some((raw, text, pres, _)) = read_lossless(path)? else {
        return Ok(FileSyncOutcome {
            warning: None,
            applied_tools_off: BTreeSet::new(),
        });
    };
    let doc = parse_jsonc(&text)?;
    let Some(mut servers_arr) = doc.get("servers").and_then(|v| v.as_array()).cloned() else {
        return Ok(FileSyncOutcome {
            warning: None,
            applied_tools_off: BTreeSet::new(),
        });
    };
    let before = servers_arr.len();
    servers_arr.retain(|v| v.get("id").and_then(|id| id.as_str()) != Some(server_name));
    if servers_arr.len() == before {
        return Ok(FileSyncOutcome {
            warning: None,
            applied_tools_off: BTreeSet::new(),
        });
    }
    let patched = patch_json_path(&text, &[], "servers", Some(&Value::Array(servers_arr)))?;
    let _ = write_lossless_with_backup(path, Some(&raw), &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: BTreeSet::new(),
    })
}

pub fn encode_json_project_snippet(
    format: McpFormat,
    no_cmd_refs: bool,
    servers: &[&McpServer],
) -> String {
    let dummy = BuiltInTargetSpec {
        id: format.as_str().to_string(),
        name: format.as_str().to_string(),
        primary_path: std::path::PathBuf::new(),
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format,
        no_cmd_refs,
        installed: true,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    };
    let mut entries = Map::new();
    for srv in servers {
        if !format.supports_remote() && srv.transport.is_remote() {
            continue;
        }
        if !format.supports_sse() && srv.transport == Transport::Sse {
            continue;
        }
        if let Ok((entry, _)) = build_json_entry(&dummy, srv, None, &BTreeSet::new()) {
            entries.insert(srv.name.clone(), Value::Object(entry));
        }
    }
    let root_doc = match format {
        McpFormat::OpenCode => {
            let mut m = Map::new();
            m.insert("mcp".into(), Value::Object(entries));
            Value::Object(m)
        }
        McpFormat::ZCode => {
            let mut mcp = Map::new();
            mcp.insert("servers".into(), Value::Object(entries));
            let mut m = Map::new();
            m.insert("mcp".into(), Value::Object(mcp));
            Value::Object(m)
        }
        _ => {
            let mut m = Map::new();
            m.insert("mcpServers".into(), Value::Object(entries));
            Value::Object(m)
        }
    };
    serde_json::to_string_pretty(&root_doc).unwrap_or_else(|_| "{}".into())
}
