use crate::edit::file::{read_lossless, write_lossless_with_backup, TextPreservation};
use crate::edit::jsonc::{get_json_path, parse_jsonc, patch_json_path};
use crate::env_refs::{normalize_imported_env_syntax, render_env_syntax, EnvRefStyle};
use crate::error::McpHubResult;
use crate::formats::json_extra::{
    read_liveagent_servers, remove_liveagent_server, write_liveagent_server,
};
use crate::formats::FileSyncOutcome;
use crate::model::{McpFormat, McpServer, SyncWarning, Transport};
use crate::targets::BuiltInTargetSpec;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn container_path(format: McpFormat, existing_root: Option<&Value>) -> &'static [&'static str] {
    match format {
        McpFormat::OpenCode => {
            if let Some(root) = existing_root
                && get_json_path(root, &["mcp", "servers"]).is_some_and(|v| v.is_object())
            {
                &["mcp", "servers"]
            } else {
                &["mcp"]
            }
        }
        McpFormat::Crush => &["mcp"],
        McpFormat::ZCode => &["mcp", "servers"],
        McpFormat::Zed => &["context_servers"],
        McpFormat::Pi => &["mcpServers"],
        _ => &["mcpServers"],
    }
}

fn format_env_style(format: McpFormat, no_cmd_refs: bool) -> (EnvRefStyle, EnvRefStyle) {
    if no_cmd_refs {
        return (EnvRefStyle::Plain, EnvRefStyle::Plain);
    }
    match format {
        McpFormat::Cursor => (EnvRefStyle::DollarEnvColon, EnvRefStyle::DollarEnvColon),
        McpFormat::OpenCode => (EnvRefStyle::BraceEnvColon, EnvRefStyle::BraceEnvColon),
        McpFormat::Antigravity
        | McpFormat::Kimi
        | McpFormat::Alma
        | McpFormat::Devin
        | McpFormat::Cline
        | McpFormat::Zed
        | McpFormat::LiveAgent => (EnvRefStyle::Plain, EnvRefStyle::Plain),
        _ => (EnvRefStyle::DollarBrace, EnvRefStyle::DollarBrace),
    }
}

fn owned_keys(format: McpFormat) -> &'static [&'static str] {
    match format {
        McpFormat::Claude | McpFormat::Pi => &[
            "type", "command", "args", "env", "url", "headers",
        ],
        McpFormat::Desktop => &["command", "args", "env"],
        McpFormat::Antigravity => &["command", "args", "env", "serverUrl", "headers"],
        McpFormat::Cursor => &["command", "args", "env", "url", "headers"],
        McpFormat::OpenCode => &["type", "command", "environment", "url", "headers", "enabled"],
        McpFormat::PiNative => &["command", "args", "env", "url", "headers", "auth"],
        McpFormat::Omp => &["type", "command", "args", "env", "url", "headers"],
        McpFormat::Copilot => &["type", "command", "args", "env", "url", "headers", "tools"],
        McpFormat::Crush => &["type", "command", "args", "env", "url", "headers"],
        McpFormat::ZCode => &["type", "command", "args", "env", "url", "headers"],
        McpFormat::Kimi => &["transport", "command", "args", "env", "url", "headers"],
        McpFormat::Alma => &["command", "args", "env", "url", "headers", "transport"],
        McpFormat::Devin => &["transport", "command", "args", "env", "url", "headers"],
        McpFormat::Cline => &["transport", "disabled"],
        McpFormat::CommandCode => &["transport", "command", "args", "env", "url", "headers", "enabled"],
        McpFormat::Zed => &["command", "args", "env", "url", "headers"],
        McpFormat::Kiro => &["command", "args", "env", "url", "headers"],
        McpFormat::Codex | McpFormat::Grok | McpFormat::Goose | McpFormat::Hermes
        | McpFormat::Dsh | McpFormat::LiveAgent => &[],
    }
}

fn is_foreign_zed_entry(format: McpFormat, obj: &Map<String, Value>) -> bool {
    format == McpFormat::Zed && !obj.contains_key("command") && !obj.contains_key("url")
}

fn merge_tools_list(
    existing: Option<&Value>,
    prev_ledger: &BTreeSet<String>,
    desired_tools: &[String],
) -> (Vec<String>, BTreeSet<String>) {
    let mut final_list: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    if let Some(arr) = existing.and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(name) = item.as_str()
                && !prev_ledger.contains(name)
                && seen.insert(name.to_string())
            {
                final_list.push(name.to_string());
            }
        }
    }

    let mut new_ledger = BTreeSet::new();
    for tool in desired_tools {
        let trimmed = tool.trim();
        if trimmed.is_empty() {
            continue;
        }
        new_ledger.insert(trimmed.to_string());
        if seen.insert(trimmed.to_string()) {
            final_list.push(trimmed.to_string());
        }
    }
    (final_list, new_ledger)
}

pub(super) fn build_json_entry(
    spec: &BuiltInTargetSpec,
    server: &McpServer,
    existing: Option<&Map<String, Value>>,
    prev_tools_off: &BTreeSet<String>,
) -> McpHubResult<(Map<String, Value>, BTreeSet<String>)> {
    let format = spec.format;
    let mut out = existing.cloned().unwrap_or_default();
    for k in owned_keys(format) {
        out.remove(*k);
    }

    let (cmd_style, env_style) = format_env_style(format, spec.no_cmd_refs);
    let rendered_cmd = render_env_syntax(&server.command, cmd_style);
    let rendered_args: Vec<Value> = server
        .args
        .iter()
        .map(|a| Value::String(render_env_syntax(a, cmd_style)))
        .collect();
    let rendered_env = |env: &BTreeMap<String, String>| -> Map<String, Value> {
        env.iter()
            .map(|(k, v)| (k.clone(), Value::String(render_env_syntax(v, env_style))))
            .collect()
    };
    let rendered_headers = |hdrs: &BTreeMap<String, String>| -> Map<String, Value> {
        hdrs.iter()
            .map(|(k, v)| (k.clone(), Value::String(render_env_syntax(v, env_style))))
            .collect()
    };

    if format == McpFormat::Cline {
        let mut tr = Map::new();
        match server.transport {
            Transport::Stdio => {
                tr.insert("type".into(), Value::String("stdio".into()));
                tr.insert("command".into(), Value::String(rendered_cmd));
                if !rendered_args.is_empty() {
                    tr.insert("args".into(), Value::Array(rendered_args));
                }
                if !server.env.is_empty() {
                    tr.insert("env".into(), Value::Object(rendered_env(&server.env)));
                }
            }
            Transport::Http => {
                tr.insert("type".into(), Value::String("streamableHttp".into()));
                tr.insert("url".into(), Value::String(render_env_syntax(&server.url, env_style)));
                if !server.headers.is_empty() {
                    tr.insert("headers".into(), Value::Object(rendered_headers(&server.headers)));
                }
            }
            Transport::Sse => {
                tr.insert("type".into(), Value::String("sse".into()));
                tr.insert("url".into(), Value::String(render_env_syntax(&server.url, env_style)));
                if !server.headers.is_empty() {
                    tr.insert("headers".into(), Value::Object(rendered_headers(&server.headers)));
                }
            }
        }
        out.insert("transport".into(), Value::Object(tr));
        return Ok((out, BTreeSet::new()));
    }

    if format == McpFormat::OpenCode {
        match server.transport {
            Transport::Stdio => {
                out.insert("type".into(), Value::String("local".into()));
                let mut cmd_arr = vec![Value::String(rendered_cmd)];
                cmd_arr.extend(rendered_args);
                out.insert("command".into(), Value::Array(cmd_arr));
                if !server.env.is_empty() {
                    out.insert("environment".into(), Value::Object(rendered_env(&server.env)));
                }
            }
            Transport::Http | Transport::Sse => {
                out.insert("type".into(), Value::String("remote".into()));
                out.insert("url".into(), Value::String(render_env_syntax(&server.url, env_style)));
                if !server.headers.is_empty() {
                    out.insert("headers".into(), Value::Object(rendered_headers(&server.headers)));
                }
            }
        }
        out.insert("enabled".into(), Value::Bool(true));
        return Ok((out, BTreeSet::new()));
    }

    match server.transport {
        Transport::Stdio => {
            if matches!(format, McpFormat::Omp | McpFormat::Crush | McpFormat::ZCode) {
                out.insert("type".into(), Value::String("stdio".into()));
            } else if format == McpFormat::Copilot {
                out.insert("type".into(), Value::String("local".into()));
                out.insert("tools".into(), Value::Array(vec![Value::String("*".into())]));
            } else if matches!(format, McpFormat::Kimi | McpFormat::Devin) {
                out.insert("transport".into(), Value::String("stdio".into()));
            } else if format == McpFormat::CommandCode {
                out.insert("transport".into(), Value::String("stdio".into()));
                out.insert("enabled".into(), Value::Bool(true));
            }
            out.insert("command".into(), Value::String(rendered_cmd));
            if !rendered_args.is_empty() {
                out.insert("args".into(), Value::Array(rendered_args));
            }
            if !server.env.is_empty() {
                out.insert("env".into(), Value::Object(rendered_env(&server.env)));
            }
        }
        Transport::Http | Transport::Sse => {
            let url_rendered = Value::String(render_env_syntax(&server.url, env_style));
            match format {
                McpFormat::Antigravity => {
                    out.insert("serverUrl".into(), url_rendered);
                }
                McpFormat::Claude
                | McpFormat::Pi
                | McpFormat::Omp
                | McpFormat::Crush
                | McpFormat::ZCode => {
                    out.insert("type".into(), Value::String(server.transport.as_str().into()));
                    out.insert("url".into(), url_rendered);
                }
                McpFormat::Copilot => {
                    out.insert("type".into(), Value::String(server.transport.as_str().into()));
                    out.insert("url".into(), url_rendered);
                    out.insert("tools".into(), Value::Array(vec![Value::String("*".into())]));
                }
                McpFormat::Kimi | McpFormat::Devin => {
                    out.insert(
                        "transport".into(),
                        Value::String(server.transport.as_str().into()),
                    );
                    out.insert("url".into(), url_rendered);
                }
                McpFormat::Alma => {
                    if server.transport == Transport::Sse {
                        out.insert("transport".into(), Value::String("sse".into()));
                    }
                    out.insert("url".into(), url_rendered);
                }
                McpFormat::CommandCode => {
                    out.insert("transport".into(), Value::String("http".into()));
                    out.insert("url".into(), url_rendered);
                    out.insert("enabled".into(), Value::Bool(true));
                }
                _ => {
                    out.insert("url".into(), url_rendered);
                }
            }
            if !server.headers.is_empty() {
                out.insert("headers".into(), Value::Object(rendered_headers(&server.headers)));
            }
        }
    }

    let mut applied_tools = BTreeSet::new();
    if spec.supports_tools_off && format == McpFormat::Crush {
        let key = "disabled_tools";
        let (merged, ledger) = merge_tools_list(out.get(key), prev_tools_off, &server.disabled_tools);
        applied_tools = ledger;
        if merged.is_empty() {
            out.remove(key);
        } else {
            out.insert(key.into(), Value::Array(merged.into_iter().map(Value::String).collect()));
        }
    }

    Ok((out, applied_tools))
}

/// Decode all MCP servers found in a JSON/JSONC agent file.
pub fn read_json_servers(spec: &BuiltInTargetSpec, path: &Path) -> McpHubResult<Vec<McpServer>> {
    if spec.format == McpFormat::LiveAgent {
        return read_liveagent_servers(spec, path);
    }

    let Some((_, text, _, _)) = read_lossless(path)? else {
        return Ok(Vec::new());
    };
    let root = parse_jsonc(&text)?;
    let path_segs = container_path(spec.format, Some(&root));
    let Some(container) = get_json_path(&root, path_segs).and_then(|v| v.as_object()) else {
        return Ok(Vec::new());
    };

    let claude_deny = if spec.id == "claude" {
        read_claude_settings_deny_entries(spec.claude_settings_path.as_deref())
    } else {
        Vec::new()
    };
    let opencode_tools = if spec.format == McpFormat::OpenCode {
        root.get("tools").and_then(|v| v.as_object())
    } else {
        None
    };

    let mut servers = Vec::new();
    for (name, val) in container {
        let Some(obj) = val.as_object() else {
            continue;
        };
        if is_foreign_zed_entry(spec.format, obj) {
            continue;
        }
        if let Some(mut srv) = decode_json_entry(spec.format, name, obj) {
            if !claude_deny.is_empty() {
                let prefix = format!("mcp__{}__", srv.name);
                for entry in &claude_deny {
                    if let Some(tool) = entry.strip_prefix(&prefix)
                        && !tool.is_empty()
                    {
                        srv.disabled_tools.push(tool.to_string());
                    }
                }
            }
            if let Some(tools_map) = opencode_tools {
                let prefix = format!("{}_", srv.name);
                for (k, v) in tools_map {
                    if v.as_bool() == Some(false)
                        && let Some(tool) = k.strip_prefix(&prefix)
                        && !tool.is_empty()
                    {
                        srv.disabled_tools.push(tool.to_string());
                    }
                }
            }
            if !srv.disabled_tools.is_empty() {
                srv.disabled_tools.sort();
                srv.disabled_tools.dedup();
            }
            servers.push(srv);
        }
    }
    Ok(servers)
}

fn read_claude_settings_deny_entries(settings_path: Option<&Path>) -> Vec<String> {
    let Some(path) = settings_path else {
        return Vec::new();
    };
    let Ok(Some((_, text, _, _))) = read_lossless(path) else {
        return Vec::new();
    };
    let Ok(root) = parse_jsonc(&text) else {
        return Vec::new();
    };
    extract_str_list(get_json_path(&root, &["permissions", "deny"]))
}

pub fn decode_json_entry(
    format: McpFormat,
    name: &str,
    obj: &Map<String, Value>,
) -> Option<McpServer> {
    if format == McpFormat::Cline
        && let Some(tr) = obj.get("transport").and_then(|v| v.as_object())
    {
        let mut srv = decode_json_entry(McpFormat::Claude, name, tr)?;
        if obj.get("disabled").and_then(|v| v.as_bool()) == Some(true) {
            srv.paused = true;
        }
        return Some(srv);
    }

    if format == McpFormat::OpenCode {
        let paused = obj.get("enabled").and_then(|v| v.as_bool()) == Some(false);
        let typ = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if typ == "remote" || obj.contains_key("url") {
            let url = obj.get("url").and_then(|v| v.as_str())?.to_string();
            let headers = extract_str_map(obj.get("headers"));
            return Some(McpServer {
                name: name.to_string(),
                transport: Transport::Http,
                url: normalize_imported_env_syntax(&url),
                headers,
                paused,
                ..Default::default()
            });
        }
        if let Some(cmd_arr) = obj.get("command").and_then(|v| v.as_array())
            && let Some(first) = cmd_arr.first().and_then(|v| v.as_str())
        {
            let args = cmd_arr[1..]
                .iter()
                .filter_map(|v| v.as_str().map(normalize_imported_env_syntax))
                .collect();
            let env = extract_str_map(obj.get("environment").or_else(|| obj.get("env")));
            return Some(McpServer {
                name: name.to_string(),
                transport: Transport::Stdio,
                command: normalize_imported_env_syntax(first),
                args,
                env,
                paused,
                ..Default::default()
            });
        }
    }

    let url = obj
        .get("serverUrl")
        .or_else(|| obj.get("url"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let cmd_str = obj
        .get("command")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(url_raw) = url
        && cmd_str.is_none()
    {
        let explicit = obj
            .get("type")
            .or_else(|| obj.get("transport"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let transport = if explicit.eq_ignore_ascii_case("sse") {
            Transport::Sse
        } else {
            Transport::Http
        };
        let mut headers = extract_str_map(obj.get("headers"));
        if let Some(token) = obj.get("auth").and_then(|v| v.as_str())
            && !token.is_empty()
        {
            headers
                .entry("Authorization".into())
                .or_insert_with(|| format!("Bearer {}", normalize_imported_env_syntax(token)));
        }
        let disabled_tools = extract_str_list(obj.get("disabled_tools"));
        return Some(McpServer {
            name: name.to_string(),
            transport,
            url: normalize_imported_env_syntax(url_raw),
            headers,
            disabled_tools,
            ..Default::default()
        });
    }

    let cmd = cmd_str?;
    let args = obj
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(normalize_imported_env_syntax))
                .collect()
        })
        .unwrap_or_default();
    let env = extract_str_map(obj.get("env"));
    let disabled_tools = extract_str_list(obj.get("disabled_tools"));
    Some(McpServer {
        name: name.to_string(),
        transport: Transport::Stdio,
        command: normalize_imported_env_syntax(cmd),
        args,
        env,
        disabled_tools,
        ..Default::default()
    })
}

fn extract_str_map(val: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(obj) = val.and_then(|v| v.as_object()) else {
        return out;
    };
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            out.insert(k.clone(), normalize_imported_env_syntax(s));
        }
    }
    out
}

fn extract_str_list(val: Option<&Value>) -> Vec<String> {
    val.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn write_json_server(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server: &McpServer,
    prev_tools_off: &BTreeSet<String>,
) -> McpHubResult<FileSyncOutcome> {
    if spec.format == McpFormat::LiveAgent {
        return write_liveagent_server(spec, path, server);
    }

    let existing = read_lossless(path)?;
    let (raw_opt, text, pres) = match &existing {
        Some((raw, txt, pr, _)) => (Some(raw.as_slice()), txt.clone(), *pr),
        None => (None, String::new(), TextPreservation::default()),
    };

    let root = parse_jsonc(&text)?;
    let cpath = container_path(spec.format, Some(&root));
    let old_entry = get_json_path(&root, cpath)
        .and_then(|v| v.as_object())
        .and_then(|m| m.get(&server.name))
        .and_then(|v| v.as_object());

    if let Some(old_obj) = old_entry
        && is_foreign_zed_entry(spec.format, old_obj)
    {
        return Ok(FileSyncOutcome {
            warning: Some(SyncWarning {
                agent_id: spec.id.clone(),
                path: path.display().to_string(),
                code: "foreign_zed_extension".to_string(),
                message: format!(
                    "Preserved Zed extension context_servers.{} without overwriting",
                    server.name
                ),
            }),
            applied_tools_off: BTreeSet::new(),
        });
    }

    let (merged_obj, applied_tools) = build_json_entry(spec, server, old_entry, prev_tools_off)?;
    let val = Value::Object(merged_obj);
    let patched = patch_json_path(&text, cpath, &server.name, Some(&val))?;
    let _ = write_lossless_with_backup(path, raw_opt, &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: applied_tools,
    })
}

pub fn remove_json_server(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server_name: &str,
) -> McpHubResult<FileSyncOutcome> {
    if spec.format == McpFormat::LiveAgent {
        return remove_liveagent_server(spec, path, server_name);
    }
    let Some((raw, text, pres, _)) = read_lossless(path)? else {
        return Ok(FileSyncOutcome {
            warning: None,
            applied_tools_off: BTreeSet::new(),
        });
    };
    let root = parse_jsonc(&text)?;
    let cpath = container_path(spec.format, Some(&root));
    let existing = get_json_path(&root, cpath)
        .and_then(|v| v.as_object())
        .and_then(|m| m.get(server_name))
        .and_then(|v| v.as_object());
    let Some(old_obj) = existing else {
        return Ok(FileSyncOutcome {
            warning: None,
            applied_tools_off: BTreeSet::new(),
        });
    };
    if is_foreign_zed_entry(spec.format, old_obj) {
        return Ok(FileSyncOutcome {
            warning: Some(SyncWarning {
                agent_id: spec.id.clone(),
                path: path.display().to_string(),
                code: "foreign_zed_extension".to_string(),
                message: format!(
                    "Refused to remove Zed extension context_servers.{server_name}"
                ),
            }),
            applied_tools_off: BTreeSet::new(),
        });
    }
    let patched = patch_json_path(&text, cpath, server_name, None)?;
    let _ = write_lossless_with_backup(path, Some(&raw), &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: BTreeSet::new(),
    })
}

pub fn sync_claude_settings_deny(
    settings_path: &Path,
    desired_by_server: &BTreeMap<String, Vec<String>>,
    prev_ledger: &BTreeMap<String, BTreeSet<String>>,
) -> McpHubResult<BTreeMap<String, BTreeSet<String>>> {
    let prev_tokens: BTreeSet<String> = prev_ledger
        .iter()
        .flat_map(|(srv, tools)| tools.iter().map(move |t| format!("mcp__{srv}__{t}")))
        .collect();
    let mut new_ledger: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut desired_tokens = Vec::new();
    for (srv, tools) in desired_by_server {
        let mut set = BTreeSet::new();
        for tool in tools {
            let trimmed = tool.trim();
            if !trimmed.is_empty() {
                set.insert(trimmed.to_string());
                desired_tokens.push(format!("mcp__{srv}__{trimmed}"));
            }
        }
        if !set.is_empty() {
            new_ledger.insert(srv.clone(), set);
        }
    }

    let existing = read_lossless(settings_path)?;
    if existing.is_none() && desired_tokens.is_empty() && prev_tokens.is_empty() {
        return Ok(BTreeMap::new());
    }
    let (raw_opt, text, pres) = match &existing {
        Some((raw, txt, pr, _)) => (Some(raw.as_slice()), txt.clone(), *pr),
        None => (None, String::new(), TextPreservation::default()),
    };
    let root = parse_jsonc(&text)?;
    let current_deny = get_json_path(&root, &["permissions", "deny"])
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut next_deny: Vec<Value> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for val in current_deny {
        if let Some(s) = val.as_str() {
            if !prev_tokens.contains(s) && seen.insert(s.to_string()) {
                next_deny.push(Value::String(s.to_string()));
            }
        } else {
            next_deny.push(val);
        }
    }
    for tok in desired_tokens {
        if seen.insert(tok.clone()) {
            next_deny.push(Value::String(tok));
        }
    }

    let patched = patch_json_path(
        &text,
        &["permissions"],
        "deny",
        Some(&Value::Array(next_deny)),
    )?;
    let _ = write_lossless_with_backup(settings_path, raw_opt, &patched, &pres)?;
    Ok(new_ledger)
}

pub fn sync_opencode_tools_map(
    config_path: &Path,
    desired_by_server: &BTreeMap<String, Vec<String>>,
    prev_ledger: &BTreeMap<String, BTreeSet<String>>,
) -> McpHubResult<BTreeMap<String, BTreeSet<String>>> {
    let prev_keys: BTreeSet<String> = prev_ledger
        .iter()
        .flat_map(|(srv, tools)| tools.iter().map(move |t| format!("{srv}_{t}")))
        .collect();
    let mut new_ledger: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut desired_keys = BTreeSet::new();
    for (srv, tools) in desired_by_server {
        let mut set = BTreeSet::new();
        for tool in tools {
            let trimmed = tool.trim();
            if !trimmed.is_empty() {
                set.insert(trimmed.to_string());
                desired_keys.insert(format!("{srv}_{trimmed}"));
            }
        }
        if !set.is_empty() {
            new_ledger.insert(srv.clone(), set);
        }
    }
    if desired_keys.is_empty() && prev_keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let Some((raw, mut text, pres, _)) = read_lossless(config_path)? else {
        return Ok(BTreeMap::new());
    };
    for old_k in prev_keys.difference(&desired_keys) {
        text = patch_json_path(&text, &["tools"], old_k, None)?;
    }
    for k in &desired_keys {
        text = patch_json_path(&text, &["tools"], k, Some(&Value::Bool(false)))?;
    }
    let _ = write_lossless_with_backup(config_path, Some(&raw), &text, &pres)?;
    Ok(new_ledger)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targets::{McpPathRoots, resolve_target_spec};
    use tempfile::TempDir;

    #[test]
    fn read_json_servers_decodes_claude_opencode_disabled_tools_and_paused_states() {
        let dir = TempDir::new().unwrap();
        let roots = McpPathRoots::from_sandbox(dir.path().to_path_buf());

        // Claude with permissions.deny
        let claude = resolve_target_spec(&roots, "claude").unwrap();
        std::fs::create_dir_all(claude.claude_settings_path.as_ref().unwrap().parent().unwrap()).unwrap();
        std::fs::write(
            &claude.primary_path,
            r#"{ "mcpServers": { "fs": { "command": "npx", "args": ["-y", "fs"] } } }"#,
        )
        .unwrap();
        std::fs::write(
            claude.claude_settings_path.as_ref().unwrap(),
            r#"{ "permissions": { "deny": ["mcp__fs__write_file", "Bash(rm:*)"] } }"#,
        )
        .unwrap();
        let claude_srvs = read_json_servers(&claude, &claude.primary_path).unwrap();
        assert_eq!(claude_srvs[0].disabled_tools, vec!["write_file"]);

        // OpenCode with tools.<srv>_<tool> = false and enabled: false
        let opencode = resolve_target_spec(&roots, "opencode").unwrap();
        std::fs::create_dir_all(opencode.primary_path.parent().unwrap()).unwrap();
        std::fs::write(
            &opencode.primary_path,
            r#"{ "mcp": { "mem": { "type": "local", "command": ["npx", "mem"], "enabled": false } }, "tools": { "mem_drop_db": false, "other_tool": true } }"#,
        )
        .unwrap();
        let oc_srvs = read_json_servers(&opencode, &opencode.primary_path).unwrap();
        assert!(oc_srvs[0].paused);
        assert_eq!(oc_srvs[0].disabled_tools, vec!["drop_db"]);

        // Cline with disabled: true
        let cline = resolve_target_spec(&roots, "cline").unwrap();
        std::fs::create_dir_all(cline.primary_path.parent().unwrap()).unwrap();
        std::fs::write(
            &cline.primary_path,
            r#"{ "mcpServers": { "off": { "disabled": true, "transport": { "type": "stdio", "command": "uvx", "args": ["tool"] } } } }"#,
        )
        .unwrap();
        let cline_srvs = read_json_servers(&cline, &cline.primary_path).unwrap();
        assert!(cline_srvs[0].paused);
        assert_eq!(cline_srvs[0].command, "uvx");
    }
}
