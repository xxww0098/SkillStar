use crate::edit::file::{read_lossless, write_lossless_with_backup, TextPreservation};
use crate::edit::toml_lossless::{parse_toml, patch_toml_entry};
use crate::edit::yaml_lossless::{
    dsh_extract_servers, dsh_patch_server, parse_yaml, patch_yaml_section_entry,
};
use crate::env_refs::{normalize_imported_env_syntax, single_env_var_ref};
use crate::error::{McpHubError, McpHubResult};
use crate::formats::FileSyncOutcome;
use crate::model::{McpFormat, McpServer, Transport};
use crate::targets::BuiltInTargetSpec;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

static GOOSE_ENV_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9_]*$").unwrap());

// ── TOML (Codex & Grok) ─────────────────────────────────────────────────────

pub fn read_toml_servers(spec: &BuiltInTargetSpec, path: &Path) -> McpHubResult<Vec<McpServer>> {
    let Some((_, text, _, _)) = read_lossless(path)? else {
        return Ok(Vec::new());
    };
    let doc = parse_toml(&text)?;
    let Some(mcp_table) = doc.get("mcp_servers").and_then(|v| v.as_table()) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for (name, val) in mcp_table {
        let Some(tbl) = val.as_table() else {
            continue;
        };
        if let Some(srv) = decode_toml_entry(spec.format, name, tbl) {
            out.push(srv);
        }
    }
    Ok(out)
}

fn decode_toml_entry(
    format: McpFormat,
    name: &str,
    tbl: &toml::map::Map<String, toml::Value>,
) -> Option<McpServer> {
    if let Some(url_raw) = tbl.get("url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        let mut headers = BTreeMap::new();
        if let Some(hdr) = tbl.get("http_headers").or_else(|| tbl.get("headers")).and_then(|v| v.as_table()) {
            for (k, v) in hdr {
                if let Some(s) = v.as_str() {
                    headers.insert(k.clone(), s.to_string());
                }
            }
        }
        if let Some(env_hdr) = tbl.get("env_http_headers").and_then(|v| v.as_table()) {
            for (k, v) in env_hdr {
                if let Some(var_name) = v.as_str() {
                    headers.insert(k.clone(), format!("${{{var_name}}}"));
                }
            }
        }
        if let Some(bearer) = tbl.get("bearer_token_env_var").and_then(|v| v.as_str())
            && !bearer.is_empty()
        {
            headers.insert("Authorization".into(), format!("Bearer ${{{bearer}}}"));
        }
        let disabled_tools = tbl
            .get("disabled_tools")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let startup_timeout_sec = if format == McpFormat::Codex {
            tbl.get("startup_timeout_sec").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        } else {
            None
        };
        return Some(McpServer {
            name: name.to_string(),
            transport: Transport::Http,
            url: normalize_imported_env_syntax(url_raw),
            headers,
            disabled_tools,
            startup_timeout_sec,
            ..Default::default()
        });
    }

    let command = tbl.get("command").and_then(|v| v.as_str())?.to_string();
    let args = tbl
        .get("args")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let mut env = BTreeMap::new();
    if let Some(env_tbl) = tbl.get("env").and_then(|v| v.as_table()) {
        for (k, v) in env_tbl {
            if let Some(s) = v.as_str() {
                env.insert(k.clone(), s.to_string());
            }
        }
    }
    if let Some(env_vars) = tbl.get("env_vars").and_then(|v| v.as_array()) {
        for var in env_vars {
            if let Some(vname) = var.as_str() {
                env.entry(vname.to_string())
                    .or_insert_with(|| format!("${{{vname}}}"));
            }
        }
    }
    let disabled_tools = tbl
        .get("disabled_tools")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let startup_timeout_sec = if format == McpFormat::Codex {
        tbl.get("startup_timeout_sec").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
    } else {
        None
    };

    Some(McpServer {
        name: name.to_string(),
        transport: Transport::Stdio,
        command,
        args,
        env,
        disabled_tools,
        startup_timeout_sec,
        ..Default::default()
    })
}

fn build_toml_table(
    format: McpFormat,
    server: &McpServer,
    existing: Option<&toml::map::Map<String, toml::Value>>,
    prev_tools_off: &BTreeSet<String>,
) -> (toml::map::Map<String, toml::Value>, BTreeSet<String>) {
    let mut out = existing.cloned().unwrap_or_default();
    let owned = [
        "command",
        "args",
        "env",
        "env_vars",
        "url",
        "headers",
        "http_headers",
        "env_http_headers",
        "bearer_token_env_var",
        "startup_timeout_sec",
    ];
    for key in owned {
        out.remove(key);
    }

    if format == McpFormat::Codex {
        match server.transport {
            Transport::Stdio => {
                out.insert("command".into(), toml::Value::String(server.command.clone()));
                if !server.args.is_empty() {
                    out.insert(
                        "args".into(),
                        toml::Value::Array(server.args.iter().cloned().map(toml::Value::String).collect()),
                    );
                }
                let mut literal_env = toml::map::Map::new();
                let mut env_vars = Vec::new();
                for (k, v) in &server.env {
                    if let Some(var_name) = single_env_var_ref(v)
                        && var_name == *k
                    {
                        env_vars.push(toml::Value::String(var_name));
                    } else {
                        literal_env.insert(k.clone(), toml::Value::String(v.clone()));
                    }
                }
                if !literal_env.is_empty() {
                    out.insert("env".into(), toml::Value::Table(literal_env));
                }
                if !env_vars.is_empty() {
                    out.insert("env_vars".into(), toml::Value::Array(env_vars));
                }
            }
            Transport::Http | Transport::Sse => {
                out.insert("url".into(), toml::Value::String(server.url.clone()));
                let mut http_hdrs = toml::map::Map::new();
                let mut env_hdrs = toml::map::Map::new();
                for (k, v) in &server.headers {
                    if k.eq_ignore_ascii_case("authorization")
                        && let Some(rest) = v.strip_prefix("Bearer ").map(str::trim)
                        && let Some(var_name) = single_env_var_ref(rest)
                    {
                        out.insert("bearer_token_env_var".into(), toml::Value::String(var_name));
                        continue;
                    }
                    if let Some(var_name) = single_env_var_ref(v) {
                        env_hdrs.insert(k.clone(), toml::Value::String(var_name));
                    } else {
                        http_hdrs.insert(k.clone(), toml::Value::String(v.clone()));
                    }
                }
                if !http_hdrs.is_empty() {
                    out.insert("http_headers".into(), toml::Value::Table(http_hdrs));
                }
                if !env_hdrs.is_empty() {
                    out.insert("env_http_headers".into(), toml::Value::Table(env_hdrs));
                }
            }
        }
        if let Some(sec) = server.startup_timeout_sec {
            out.insert("startup_timeout_sec".into(), toml::Value::Float(sec));
        }

        // Preserve user's disabled_tools outside our ledger.
        let mut final_tools = Vec::new();
        let mut seen = BTreeSet::new();
        if let Some(arr) = existing.and_then(|t| t.get("disabled_tools")).and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str()
                    && !prev_tools_off.contains(s)
                    && seen.insert(s.to_string())
                {
                    final_tools.push(s.to_string());
                }
            }
        }
        let mut new_ledger = BTreeSet::new();
        for tool in &server.disabled_tools {
            let t = tool.trim();
            if !t.is_empty() {
                new_ledger.insert(t.to_string());
                if seen.insert(t.to_string()) {
                    final_tools.push(t.to_string());
                }
            }
        }
        if final_tools.is_empty() {
            out.remove("disabled_tools");
        } else {
            out.insert(
                "disabled_tools".into(),
                toml::Value::Array(final_tools.into_iter().map(toml::Value::String).collect()),
            );
        }
        return (out, new_ledger);
    }

    // Grok format (`no-sse`): clean TOML `[mcp_servers.<name>]`
    match server.transport {
        Transport::Stdio => {
            out.insert("command".into(), toml::Value::String(server.command.clone()));
            if !server.args.is_empty() {
                out.insert(
                    "args".into(),
                    toml::Value::Array(server.args.iter().cloned().map(toml::Value::String).collect()),
                );
            }
            if !server.env.is_empty() {
                let env_tbl = server
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
                    .collect();
                out.insert("env".into(), toml::Value::Table(env_tbl));
            }
        }
        Transport::Http | Transport::Sse => {
            out.insert("url".into(), toml::Value::String(server.url.clone()));
            if !server.headers.is_empty() {
                let hdr_tbl = server
                    .headers
                    .iter()
                    .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
                    .collect();
                out.insert("headers".into(), toml::Value::Table(hdr_tbl));
            }
        }
    }
    (out, BTreeSet::new())
}

pub fn write_toml_server(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server: &McpServer,
    prev_tools_off: &BTreeSet<String>,
) -> McpHubResult<FileSyncOutcome> {
    let existing = read_lossless(path)?;
    let (raw_opt, text, pres) = match &existing {
        Some((raw, txt, pr, _)) => (Some(raw.as_slice()), txt.clone(), *pr),
        None => (None, String::new(), TextPreservation::default()),
    };
    let doc = parse_toml(&text)?;
    let old_tbl = doc
        .get("mcp_servers")
        .and_then(|v| v.as_table())
        .and_then(|m| m.get(&server.name))
        .and_then(|v| v.as_table());
    let (new_tbl, applied_tools) = build_toml_table(spec.format, server, old_tbl, prev_tools_off);
    let patched = patch_toml_entry(&text, "mcp_servers", &server.name, Some(&new_tbl))?;
    let _ = write_lossless_with_backup(path, raw_opt, &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: applied_tools,
    })
}

pub fn remove_toml_server(
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
    let patched = patch_toml_entry(&text, "mcp_servers", server_name, None)?;
    let _ = write_lossless_with_backup(path, Some(&raw), &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: BTreeSet::new(),
    })
}

pub fn encode_codex_project_snippet(servers: &[&McpServer]) -> String {
    let mut out = String::new();
    for srv in servers {
        if srv.transport == Transport::Sse {
            continue;
        }
        let (tbl, _) = build_toml_table(McpFormat::Codex, srv, None, &BTreeSet::new());
        if let Ok(chunk) = patch_toml_entry(&out, "mcp_servers", &srv.name, Some(&tbl)) {
            out = chunk;
        }
    }
    if out.is_empty() {
        "[mcp_servers]\n".to_string()
    } else {
        out
    }
}

// ── YAML (Goose, Hermes, DSH) ───────────────────────────────────────────────

pub fn read_yaml_servers(spec: &BuiltInTargetSpec, path: &Path) -> McpHubResult<Vec<McpServer>> {
    let Some((_, text, _, _)) = read_lossless(path)? else {
        return Ok(Vec::new());
    };
    if spec.format == McpFormat::Dsh {
        return Ok(dsh_extract_servers(&text));
    }

    let doc = parse_yaml(&text)?;
    let section_key = if spec.format == McpFormat::Goose {
        "extensions"
    } else {
        "mcp_servers"
    };
    let Some(section) = doc
        .get(serde_yaml::Value::String(section_key.into()))
        .and_then(|v| v.as_mapping())
    else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for (k, v) in section {
        let Some(name) = k.as_str() else {
            continue;
        };
        let Some(map) = v.as_mapping() else {
            continue;
        };
        if spec.format == McpFormat::Goose {
            if let Some(srv) = decode_goose_entry(name, map) {
                out.push(srv);
            }
        } else if let Some(srv) = decode_hermes_entry(name, map) {
            out.push(srv);
        }
    }
    Ok(out)
}

fn decode_goose_entry(name: &str, map: &serde_yaml::Mapping) -> Option<McpServer> {
    let typ = map
        .get(serde_yaml::Value::String("type".into()))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if typ == "builtin" {
        return None;
    }
    let env_keys: Vec<String> = map
        .get(serde_yaml::Value::String("env_keys".into()))
        .and_then(|v| v.as_sequence())
        .map(|s| s.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    if typ == "streamable_http" || map.contains_key(serde_yaml::Value::String("uri".into())) {
        let uri = map
            .get(serde_yaml::Value::String("uri".into()))
            .and_then(|v| v.as_str())?
            .to_string();
        let headers = yaml_str_map(map.get(serde_yaml::Value::String("headers".into())));
        return Some(McpServer {
            name: name.to_string(),
            transport: Transport::Http,
            url: uri,
            headers,
            ..Default::default()
        });
    }
    let cmd = map
        .get(serde_yaml::Value::String("cmd".into()))
        .and_then(|v| v.as_str())?
        .to_string();
    let args = map
        .get(serde_yaml::Value::String("args".into()))
        .and_then(|v| v.as_sequence())
        .map(|s| s.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let mut env = yaml_str_map(map.get(serde_yaml::Value::String("envs".into())));
    for k in env_keys {
        env.entry(k.clone()).or_insert_with(|| format!("${{{k}}}"));
    }
    Some(McpServer {
        name: name.to_string(),
        transport: Transport::Stdio,
        command: cmd,
        args,
        env,
        ..Default::default()
    })
}

fn decode_hermes_entry(name: &str, map: &serde_yaml::Mapping) -> Option<McpServer> {
    if let Some(url) = map
        .get(serde_yaml::Value::String("url".into()))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let headers = yaml_str_map(map.get(serde_yaml::Value::String("headers".into())));
        let transport = match map
            .get(serde_yaml::Value::String("transport".into()))
            .and_then(|v| v.as_str())
        {
            Some("sse") => Transport::Sse,
            _ => Transport::Http,
        };
        return Some(McpServer {
            name: name.to_string(),
            transport,
            url: url.to_string(),
            headers,
            ..Default::default()
        });
    }
    let cmd = map
        .get(serde_yaml::Value::String("command".into()))
        .and_then(|v| v.as_str())?
        .to_string();
    let args = map
        .get(serde_yaml::Value::String("args".into()))
        .and_then(|v| v.as_sequence())
        .map(|s| s.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let env = yaml_str_map(map.get(serde_yaml::Value::String("env".into())));
    let disabled_tools = map
        .get(serde_yaml::Value::String("tools".into()))
        .and_then(|v| v.as_mapping())
        .and_then(|m| m.get(serde_yaml::Value::String("exclude".into())))
        .and_then(|v| v.as_sequence())
        .map(|s| s.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    Some(McpServer {
        name: name.to_string(),
        transport: Transport::Stdio,
        command: cmd,
        args,
        env,
        disabled_tools,
        ..Default::default()
    })
}

fn yaml_str_map(val: Option<&serde_yaml::Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(map) = val.and_then(|v| v.as_mapping()) else {
        return out;
    };
    for (k, v) in map {
        if let (Some(ks), Some(vs)) = (k.as_str(), v.as_str()) {
            out.insert(ks.to_string(), vs.to_string());
        }
    }
    out
}

pub fn write_yaml_server(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server: &McpServer,
    prev_tools_off: &BTreeSet<String>,
) -> McpHubResult<FileSyncOutcome> {
    let existing = read_lossless(path)?;
    let (raw_opt, text, pres) = match &existing {
        Some((raw, txt, pr, _)) => (Some(raw.as_slice()), txt.clone(), *pr),
        None => (None, String::new(), TextPreservation::default()),
    };

    if spec.format == McpFormat::Dsh {
        let patched = dsh_patch_server(&text, &server.name, Some(server))?;
        let _ = write_lossless_with_backup(path, raw_opt, &patched, &pres)?;
        return Ok(FileSyncOutcome {
            warning: None,
            applied_tools_off: BTreeSet::new(),
        });
    }

    let doc = parse_yaml(&text)?;
    let section_key = if spec.format == McpFormat::Goose {
        "extensions"
    } else {
        "mcp_servers"
    };
    let old_map = doc
        .get(serde_yaml::Value::String(section_key.into()))
        .and_then(|v| v.as_mapping())
        .and_then(|m| m.get(serde_yaml::Value::String(server.name.clone())))
        .and_then(|v| v.as_mapping());

    let (new_val, applied_tools) = if spec.format == McpFormat::Goose {
        (build_goose_yaml_entry(server, old_map)?, BTreeSet::new())
    } else {
        build_hermes_yaml_entry(server, old_map, prev_tools_off)
    };

    let patched = patch_yaml_section_entry(&text, section_key, &server.name, Some(&new_val))?;
    let _ = write_lossless_with_backup(path, raw_opt, &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: applied_tools,
    })
}

fn build_goose_yaml_entry(
    server: &McpServer,
    existing: Option<&serde_yaml::Mapping>,
) -> McpHubResult<serde_yaml::Value> {
    let mut map = existing.cloned().unwrap_or_default();
    for k in ["type", "cmd", "args", "envs", "uri", "headers", "env_keys"] {
        map.remove(serde_yaml::Value::String(k.into()));
    }
    map.insert("name".into(), server.name.clone().into());
    map.insert("enabled".into(), true.into());
    if !map.contains_key(serde_yaml::Value::String("timeout".into())) {
        map.insert("timeout".into(), 300.into());
    }

    let mut env_keys = Vec::new();
    match server.transport {
        Transport::Stdio => {
            map.insert("type".into(), "stdio".into());
            map.insert("cmd".into(), server.command.clone().into());
            if !server.args.is_empty() {
                map.insert(
                    "args".into(),
                    serde_yaml::Value::Sequence(
                        server.args.iter().cloned().map(Into::into).collect(),
                    ),
                );
            }
            let mut envs = serde_yaml::Mapping::new();
            for (k, v) in &server.env {
                if let Some(var_name) = single_env_var_ref(v)
                    && var_name == *k
                    && GOOSE_ENV_NAME.is_match(k)
                {
                    env_keys.push(serde_yaml::Value::String(var_name));
                } else {
                    envs.insert(k.clone().into(), v.clone().into());
                }
            }
            if !envs.is_empty() {
                map.insert("envs".into(), serde_yaml::Value::Mapping(envs));
            }
        }
        Transport::Http => {
            map.insert("type".into(), "streamable_http".into());
            map.insert("uri".into(), server.url.clone().into());
            let mut hdrs = serde_yaml::Mapping::new();
            for (k, v) in &server.headers {
                hdrs.insert(k.clone().into(), v.clone().into());
            }
            if !hdrs.is_empty() {
                map.insert("headers".into(), serde_yaml::Value::Mapping(hdrs));
            }
        }
        Transport::Sse => {
            return Err(McpHubError::UnsupportedForAgent {
                agent: "goose".into(),
                reason: "Goose does not support SSE MCP servers".into(),
            });
        }
    }
    if !env_keys.is_empty() {
        map.insert("env_keys".into(), serde_yaml::Value::Sequence(env_keys));
    }
    Ok(serde_yaml::Value::Mapping(map))
}

fn build_hermes_yaml_entry(
    server: &McpServer,
    existing: Option<&serde_yaml::Mapping>,
    prev_tools_off: &BTreeSet<String>,
) -> (serde_yaml::Value, BTreeSet<String>) {
    let mut map = existing.cloned().unwrap_or_default();
    for k in ["command", "args", "env", "url", "headers", "transport"] {
        map.remove(serde_yaml::Value::String(k.into()));
    }
    match server.transport {
        Transport::Stdio => {
            map.insert("command".into(), server.command.clone().into());
            if !server.args.is_empty() {
                map.insert(
                    "args".into(),
                    serde_yaml::Value::Sequence(
                        server.args.iter().cloned().map(Into::into).collect(),
                    ),
                );
            }
            if !server.env.is_empty() {
                let mut env_m = serde_yaml::Mapping::new();
                for (k, v) in &server.env {
                    env_m.insert(k.clone().into(), v.clone().into());
                }
                map.insert("env".into(), serde_yaml::Value::Mapping(env_m));
            }
        }
        Transport::Http | Transport::Sse => {
            map.insert("url".into(), server.url.clone().into());
            if server.transport == Transport::Sse {
                map.insert("transport".into(), "sse".into());
            }
            if !server.headers.is_empty() {
                let mut hdr_m = serde_yaml::Mapping::new();
                for (k, v) in &server.headers {
                    hdr_m.insert(k.clone().into(), v.clone().into());
                }
                map.insert("headers".into(), serde_yaml::Value::Mapping(hdr_m));
            }
        }
    }

    // Tools exclusion: `tools.exclude`
    let mut final_tools = Vec::new();
    let mut seen = BTreeSet::new();
    let mut tools_map = map
        .get(serde_yaml::Value::String("tools".into()))
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();
    if let Some(arr) = tools_map
        .get(serde_yaml::Value::String("exclude".into()))
        .and_then(|v| v.as_sequence())
    {
        for item in arr {
            if let Some(s) = item.as_str()
                && !prev_tools_off.contains(s)
                && seen.insert(s.to_string())
            {
                final_tools.push(s.to_string());
            }
        }
    }
    let mut new_ledger = BTreeSet::new();
    for tool in &server.disabled_tools {
        let t = tool.trim();
        if !t.is_empty() {
            new_ledger.insert(t.to_string());
            if seen.insert(t.to_string()) {
                final_tools.push(t.to_string());
            }
        }
    }
    if final_tools.is_empty() {
        tools_map.remove(serde_yaml::Value::String("exclude".into()));
        if tools_map.is_empty() {
            map.remove(serde_yaml::Value::String("tools".into()));
        } else {
            map.insert("tools".into(), serde_yaml::Value::Mapping(tools_map));
        }
    } else {
        tools_map.insert(
            "exclude".into(),
            serde_yaml::Value::Sequence(final_tools.into_iter().map(Into::into).collect()),
        );
        map.insert("tools".into(), serde_yaml::Value::Mapping(tools_map));
    }

    (serde_yaml::Value::Mapping(map), new_ledger)
}

pub fn remove_yaml_server(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server_name: &str,
) -> McpHubResult<FileSyncOutcome> {
    let Some((raw, text, pres, _)) = read_lossless(path)? else {
        return Ok(FileSyncOutcome {
            warning: None,
            applied_tools_off: BTreeSet::new(),
        });
    };
    let patched = if spec.format == McpFormat::Dsh {
        dsh_patch_server(&text, server_name, None)?
    } else {
        let section_key = if spec.format == McpFormat::Goose {
            "extensions"
        } else {
            "mcp_servers"
        };
        patch_yaml_section_entry(&text, section_key, server_name, None)?
    };
    let _ = write_lossless_with_backup(path, Some(&raw), &patched, &pres)?;
    Ok(FileSyncOutcome {
        warning: None,
        applied_tools_off: BTreeSet::new(),
    })
}
