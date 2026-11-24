use crate::edit::jsonc::parse_jsonc;
use crate::env_refs::normalize_imported_env_syntax;
use crate::error::{McpHubError, McpHubResult};
use crate::formats::json_formats::decode_json_entry;
use crate::model::{McpFormat, McpServer, Transport};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Adaptive result produced by parsing pasted JSON/JSONC snippets or CLI command lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedPasteResult {
    pub servers: Vec<McpServer>,
    pub detected_kind: String,
}

/// Parse arbitrary MCP configuration pastes:
/// - Full agent JSON/JSONC objects (`mcpServers`, `mcp.servers`, `mcp`, `context_servers`)
/// - Single named entry `{ "name": { "command": ... } }`
/// - Bare single server object `{ "command": ..., "args": [...] }` / `{ "url": ... }`
/// - CLI commands (`claude mcp add ...`, `npx ...`, `uvx ...`, `KEY=VAL cmd args...`, URLs)
pub fn parse_pasted_config(raw: &str) -> McpHubResult<ParsedPasteResult> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(McpHubError::InvalidServer(
            "paste input is empty".to_string(),
        ));
    }

    if trimmed.starts_with('{') {
        if let Ok(val) = parse_jsonc(trimmed)
            && let Some(obj) = val.as_object()
        {
            return parse_json_paste_object(obj);
        }
    }

    // Try URL alone.
    if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !trimmed.contains(char::is_whitespace)
    {
        let derived = derive_name_from_url(trimmed);
        return Ok(ParsedPasteResult {
            servers: vec![McpServer {
                name: derived,
                transport: Transport::Http,
                url: trimmed.to_string(),
                ..Default::default()
            }],
            detected_kind: "url".to_string(),
        });
    }

    // Shell command paste (`npx ...`, `claude mcp add ...`, `FOO=1 uvx ...`).
    parse_cli_paste(trimmed)
}

fn parse_json_paste_object(obj: &Map<String, Value>) -> McpHubResult<ParsedPasteResult> {
    for (wrapper, fmt) in [
        ("mcpServers", McpFormat::Claude),
        ("context_servers", McpFormat::Zed),
        ("mcp_servers", McpFormat::Claude),
    ] {
        if let Some(inner) = obj.get(wrapper).and_then(|v| v.as_object()) {
            let servers = decode_named_map(inner, fmt);
            if !servers.is_empty() {
                return Ok(ParsedPasteResult {
                    servers,
                    detected_kind: format!("json:{wrapper}"),
                });
            }
        }
    }
    if let Some(mcp_obj) = obj.get("mcp").and_then(|v| v.as_object()) {
        if let Some(servers_obj) = mcp_obj.get("servers").and_then(|v| v.as_object()) {
            let servers = decode_named_map(servers_obj, McpFormat::ZCode);
            if !servers.is_empty() {
                return Ok(ParsedPasteResult {
                    servers,
                    detected_kind: "json:mcp.servers".to_string(),
                });
            }
        }
        let servers = decode_named_map(mcp_obj, McpFormat::OpenCode);
        if !servers.is_empty() {
            return Ok(ParsedPasteResult {
                servers,
                detected_kind: "json:mcp".to_string(),
            });
        }
    }

    // Single unnamed server object: `{ "command": ... }` or `{ "url": ... }`.
    if obj.contains_key("command") || obj.contains_key("url") || obj.contains_key("serverUrl") {
        if let Some(mut srv) = decode_json_entry(McpFormat::Claude, "", obj) {
            srv.name = if srv.transport == Transport::Stdio {
                derive_name_from_cmd(&srv.command, &srv.args)
            } else {
                derive_name_from_url(&srv.url)
            };
            return Ok(ParsedPasteResult {
                servers: vec![srv],
                detected_kind: "json:single".to_string(),
            });
        }
    }

    // Map of `{ "server-a": { ... }, "server-b": { ... } }`.
    let named = decode_named_map(obj, McpFormat::Claude);
    if !named.is_empty() {
        return Ok(ParsedPasteResult {
            servers: named,
            detected_kind: "json:map".to_string(),
        });
    }

    Err(McpHubError::InvalidServer(
        "JSON object did not contain recognizable MCP server entries".to_string(),
    ))
}

fn decode_named_map(map: &Map<String, Value>, fmt: McpFormat) -> Vec<McpServer> {
    let mut out = Vec::new();
    for (name, val) in map {
        if let Some(entry_obj) = val.as_object() {
            if let Some(srv) = decode_json_entry(fmt, name, entry_obj) {
                out.push(srv);
            } else if let Some(srv) = decode_json_entry(McpFormat::OpenCode, name, entry_obj) {
                out.push(srv);
            }
        }
    }
    out
}

fn parse_cli_paste(raw: &str) -> McpHubResult<ParsedPasteResult> {
    // Collapse backslash line continuations before tokenizing.
    let joined = raw.replace("\\\r\n", " ").replace("\\\n", " ");
    let tokens = split_shell_words(&joined)?;
    if tokens.is_empty() {
        return Err(McpHubError::InvalidServer("empty command paste".into()));
    }

    // Support `claude mcp add [--transport http|sse|stdio] [-e K=V] [--header "K: V"] <name> <url_or_cmd...>`
    if tokens.len() >= 4
        && (tokens[0] == "claude" || tokens[0].ends_with("/claude"))
        && tokens[1] == "mcp"
        && tokens[2] == "add"
    {
        return parse_claude_mcp_add(&tokens[3..]);
    }

    // Generic `KEY=VAL command arg1 arg2`
    let mut env = BTreeMap::new();
    let mut idx = 0;
    while idx < tokens.len() {
        if let Some((k, v)) = tokens[idx].split_once('=')
            && !k.is_empty()
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            env.insert(k.to_string(), normalize_imported_env_syntax(v));
            idx += 1;
            continue;
        }
        break;
    }
    if idx >= tokens.len() {
        return Err(McpHubError::InvalidServer(
            "expected a command after environment assignments".to_string(),
        ));
    }
    let command = tokens[idx].clone();
    let args: Vec<String> = tokens[idx + 1..].to_vec();
    let name = derive_name_from_cmd(&command, &args);
    Ok(ParsedPasteResult {
        servers: vec![McpServer {
            name,
            transport: Transport::Stdio,
            command,
            args,
            env,
            ..Default::default()
        }],
        detected_kind: "cli:command".to_string(),
    })
}

fn parse_claude_mcp_add(tokens: &[String]) -> McpHubResult<ParsedPasteResult> {
    let mut transport = Transport::Stdio;
    let mut env = BTreeMap::new();
    let mut headers = BTreeMap::new();
    let mut positional: Vec<String> = Vec::new();
    let mut double_dash_rest: Option<Vec<String>> = None;

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if tok == "--" {
            double_dash_rest = Some(tokens[i + 1..].to_vec());
            break;
        }
        // Once `<name>` and `<commandOrUrl>` have been seen, preserve every subsequent
        // token (including `-y`, `--port`, etc.) verbatim for the wrapped command.
        if positional.len() >= 2 {
            positional.push(tok.clone());
            i += 1;
            continue;
        }
        if (tok == "--transport" || tok == "-t") && i + 1 < tokens.len() {
            transport = parse_claude_transport(&tokens[i + 1]);
            i += 2;
            continue;
        }
        if let Some(val) = tok.strip_prefix("--transport=") {
            transport = parse_claude_transport(val);
            i += 1;
            continue;
        }
        if (tok == "--env" || tok == "-e") && i + 1 < tokens.len() {
            if let Some((k, v)) = tokens[i + 1].split_once('=') {
                env.insert(k.to_string(), v.to_string());
            }
            i += 2;
            continue;
        }
        if let Some(val) = tok.strip_prefix("--env=") {
            if let Some((k, v)) = val.split_once('=') {
                env.insert(k.to_string(), v.to_string());
            }
            i += 1;
            continue;
        }
        if (tok == "--header" || tok == "-H") && i + 1 < tokens.len() {
            if let Some((k, v)) = tokens[i + 1].split_once(':') {
                headers.insert(k.trim().to_string(), v.trim().to_string());
            }
            i += 2;
            continue;
        }
        if let Some(val) = tok.strip_prefix("--header=") {
            if let Some((k, v)) = val.split_once(':') {
                headers.insert(k.trim().to_string(), v.trim().to_string());
            }
            i += 1;
            continue;
        }
        if (tok == "--scope" || tok == "-s" || tok == "--client-id" || tok == "--client-secret" || tok == "--callback-port")
            && i + 1 < tokens.len()
        {
            i += 2;
            continue;
        }
        if tok.starts_with("--scope=")
            || tok.starts_with("--client-id=")
            || tok.starts_with("--client-secret=")
            || tok.starts_with("--callback-port=")
        {
            i += 1;
            continue;
        }
        positional.push(tok.clone());
        i += 1;
    }

    let name = positional
        .first()
        .cloned()
        .ok_or_else(|| McpHubError::InvalidServer("missing server name in `claude mcp add`".into()))?;
    let mut rest: Vec<String> = positional.into_iter().skip(1).collect();
    if let Some(after) = double_dash_rest {
        rest.extend(after);
    }

    if transport.is_remote()
        || (rest.len() == 1
            && (rest[0].starts_with("http://") || rest[0].starts_with("https://")))
    {
        let url = rest.first().cloned().unwrap_or_default();
        if transport == Transport::Stdio {
            transport = Transport::Http;
        }
        return Ok(ParsedPasteResult {
            servers: vec![McpServer {
                name,
                transport,
                url,
                headers,
                ..Default::default()
            }],
            detected_kind: "cli:claude-mcp-add".to_string(),
        });
    }

    let command = rest.first().cloned().unwrap_or_default();
    let args = rest.into_iter().skip(1).collect();
    Ok(ParsedPasteResult {
        servers: vec![McpServer {
            name,
            transport: Transport::Stdio,
            command,
            args,
            env,
            ..Default::default()
        }],
        detected_kind: "cli:claude-mcp-add".to_string(),
    })
}

pub(crate) fn split_shell_words(input: &str) -> McpHubResult<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_sq = false;
    let mut in_dq = false;
    let mut esc = false;
    let mut has_token = false;

    for c in input.chars() {
        if esc {
            cur.push(c);
            esc = false;
            has_token = true;
            continue;
        }
        if in_sq {
            if c == '\'' {
                in_sq = false;
            } else {
                cur.push(c);
            }
            has_token = true;
            continue;
        }
        if in_dq {
            if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_dq = false;
            } else {
                cur.push(c);
            }
            has_token = true;
            continue;
        }
        match c {
            '\\' => {
                esc = true;
                has_token = true;
            }
            '\'' => {
                in_sq = true;
                has_token = true;
            }
            '"' => {
                in_dq = true;
                has_token = true;
            }
            c if c.is_whitespace() => {
                if has_token {
                    words.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            _ => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if in_sq || in_dq {
        return Err(McpHubError::InvalidServer(
            "unterminated quote in command paste".to_string(),
        ));
    }
    if has_token {
        words.push(cur);
    }
    Ok(words)
}

pub(crate) fn derive_name_from_cmd(cmd: &str, args: &[String]) -> String {
    let runner = std::path::Path::new(cmd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(cmd);
    if matches!(runner, "npx" | "bunx" | "uvx" | "pnpm" | "pnpx") {
        for arg in args {
            if arg.starts_with('-') {
                continue;
            }
            let base = arg.rsplit('/').next().unwrap_or(arg);
            let no_ver = base.split('@').next().filter(|s| !s.is_empty()).unwrap_or(base);
            let clean = sanitize_slug(no_ver);
            if !clean.is_empty() {
                return clean;
            }
        }
    }
    let clean = sanitize_slug(runner);
    if clean.is_empty() {
        "mcp-server".into()
    } else {
        clean
    }
}

fn derive_name_from_url(raw_url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(raw_url)
        && let Some(host) = parsed.host_str()
    {
        let clean = sanitize_slug(
            host.strip_prefix("mcp.")
                .or_else(|| host.strip_prefix("api."))
                .unwrap_or(host)
                .split('.')
                .next()
                .unwrap_or("mcp-server"),
        );
        if !clean.is_empty() {
            return clean;
        }
    }
    "mcp-server".into()
}

fn parse_claude_transport(raw: &str) -> Transport {
    match raw.to_lowercase().as_str() {
        "http" => Transport::Http,
        "sse" => Transport::Sse,
        _ => Transport::Stdio,
    }
}

pub(crate) fn sanitize_slug(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_mcp_add_preserves_wrapped_command_flags_without_double_dash() {
        let parsed = parse_pasted_config(
            "claude mcp add -s user -e API_KEY=secret mem npx -y @modelcontextprotocol/server-memory --port 3000",
        )
        .unwrap();
        assert_eq!(parsed.servers.len(), 1);
        let srv = &parsed.servers[0];
        assert_eq!(srv.name, "mem");
        assert_eq!(srv.command, "npx");
        assert_eq!(
            srv.args,
            vec!["-y", "@modelcontextprotocol/server-memory", "--port", "3000"]
        );
        assert_eq!(srv.env.get("API_KEY").map(String::as_str), Some("secret"));
    }
}
