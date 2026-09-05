//! Parse pasted MCP snippets into store drafts.
//!
//! Complementary to [`super::import_from_tool`]: that path reads a live agent
//! config from disk; this path accepts anything the user (or a
//! `skillstar://mcp` deep link) dropped on the command center. It never
//! writes the store. The UI must still run the existing create-form or
//! marketplace install-confirm path.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use url::Url;

use super::{
    JsonReadDialect, McpServerEntry, blank_entry, entry_from_json_spec,
    entry_from_json_spec_dialect,
};

/// What kind of paste the parser recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "McpPasteKind.ts")]
pub enum McpPasteKind {
    JsonServers,
    Url,
    Command,
    DeepLink,
    Catalog,
    Empty,
    Unknown,
}

/// Drafts (and optional catalog id) parsed from a paste. Never an install.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpPasteParse.ts")]
pub struct McpPasteParse {
    pub kind: McpPasteKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drafts: Vec<McpServerEntry>,
    /// Marketplace catalog row id when the paste/deep-link names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl McpPasteParse {
    fn empty() -> Self {
        Self {
            kind: McpPasteKind::Empty,
            drafts: Vec::new(),
            catalog_id: None,
            warnings: Vec::new(),
            error: None,
        }
    }

    fn unknown(error: impl Into<String>) -> Self {
        Self {
            kind: McpPasteKind::Unknown,
            drafts: Vec::new(),
            catalog_id: None,
            warnings: Vec::new(),
            error: Some(error.into()),
        }
    }
}

/// Parse a pasted snippet or `skillstar://mcp` URL into drafts.
///
/// This function has no I/O and never installs anything.
pub fn parse_pasted_mcp(text: &str) -> McpPasteParse {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return McpPasteParse::empty();
    }

    if let Some(parsed) = parse_deep_link(trimmed) {
        return parsed;
    }
    if looks_like_deep_link_query(trimmed) {
        return parse_query_pairs(trimmed, McpPasteKind::DeepLink);
    }
    if let Some(parsed) = parse_json_blob(trimmed) {
        return parsed;
    }
    if let Some(parsed) = parse_url(trimmed) {
        return parsed;
    }
    if let Some(parsed) = parse_command_line(trimmed) {
        return parsed;
    }

    McpPasteParse::unknown("could not parse as MCP JSON, URL, command, or skillstar://mcp link")
}

fn parse_deep_link(text: &str) -> Option<McpPasteParse> {
    if !text
        .get(..12)
        .is_some_and(|s| s.eq_ignore_ascii_case("skillstar://"))
    {
        return None;
    }
    let rest = text.split_once("://")?.1.trim_start_matches('/');
    if rest.len() < 3 || !rest[..3].eq_ignore_ascii_case("mcp") {
        return None;
    }
    let query = rest.split_once('?')?.1;
    Some(parse_query_pairs(query, McpPasteKind::DeepLink))
}

fn looks_like_deep_link_query(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (lower.contains("url=")
        || lower.contains("catalog=")
        || lower.contains("config=")
        || lower.contains("command="))
        && !text.contains(' ')
        && !text.starts_with('{')
        && !text.starts_with('[')
}

fn parse_query_pairs(query: &str, fallback_kind: McpPasteKind) -> McpPasteParse {
    let mut catalog_id = None;
    let mut url = None;
    let mut command = None;
    let mut config = None;
    let mut name = None;

    if let Some(raw) = take_raw_json_query_value(query, "config") {
        config = Some(raw.to_string());
    }

    for (key, value) in Url::parse(&format!("skillstar://mcp?{query}"))
        .ok()
        .map(|u| u.query_pairs().into_owned().collect::<Vec<_>>())
        .unwrap_or_else(|| fallback_query_pairs(query))
    {
        match key.as_str() {
            "catalog" if catalog_id.is_none() => {
                let value = value.trim();
                if !value.is_empty() {
                    catalog_id = Some(value.to_string());
                }
            }
            "url" if url.is_none() => {
                let value = value.trim();
                if !value.is_empty() {
                    url = Some(value.to_string());
                }
            }
            "command" if command.is_none() => {
                let value = value.trim();
                if !value.is_empty() {
                    command = Some(value.to_string());
                }
            }
            "config" if config.is_none() => {
                let value = value.trim();
                if !value.is_empty() {
                    config = Some(value.to_string());
                }
            }
            "name" if name.is_none() => {
                let value = value.trim();
                if !value.is_empty() {
                    name = Some(value.to_string());
                }
            }
            _ => {}
        }
    }

    if let Some(catalog) = catalog_id {
        let mut parsed = McpPasteParse {
            kind: McpPasteKind::Catalog,
            drafts: Vec::new(),
            catalog_id: Some(catalog),
            warnings: Vec::new(),
            error: None,
        };
        if url.is_some() || command.is_some() || config.is_some() {
            parsed
                .warnings
                .push("catalog takes precedence; other deep-link fields were ignored".into());
        }
        return parsed;
    }

    if let Some(config) = config {
        let mut parsed = parse_json_blob(&config)
            .unwrap_or_else(|| McpPasteParse::unknown("deep-link config= was not valid MCP JSON"));
        parsed.kind = McpPasteKind::DeepLink;
        apply_name_override(&mut parsed, name.as_deref());
        return parsed;
    }

    if let Some(url) = url {
        let mut parsed = parse_url(&url).unwrap_or_else(|| {
            let mut entry = blank_entry(&name_from_url(&url), "http");
            entry.url = Some(url);
            McpPasteParse {
                kind: McpPasteKind::Url,
                drafts: vec![entry],
                catalog_id: None,
                warnings: Vec::new(),
                error: None,
            }
        });
        parsed.kind = fallback_kind;
        apply_name_override(&mut parsed, name.as_deref());
        return parsed;
    }

    if let Some(command) = command {
        let mut parsed = parse_command_line(&command).unwrap_or_else(|| {
            McpPasteParse::unknown("deep-link command= was not a single launch command")
        });
        if parsed.kind != McpPasteKind::Unknown {
            parsed.kind = fallback_kind;
        }
        apply_name_override(&mut parsed, name.as_deref());
        return parsed;
    }

    McpPasteParse::unknown("skillstar://mcp link had no url, catalog, config, or command")
}

fn take_raw_json_query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}=");
    let idx = query.find(&prefix)?;
    let rest = query[idx + prefix.len()..].trim_start();
    if rest.starts_with('{') || rest.starts_with('[') {
        Some(rest)
    } else {
        None
    }
}

fn fallback_query_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

fn apply_name_override(parsed: &mut McpPasteParse, name: Option<&str>) {
    let Some(name) = name.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    if parsed.drafts.len() == 1 {
        parsed.drafts[0].name = name.to_string();
    }
}

fn parse_json_blob(text: &str) -> Option<McpPasteParse> {
    let slice = extract_json_slice(text)?;
    let value: Value = serde_json::from_str(slice).ok()?;
    let mut warnings = Vec::new();
    let drafts = drafts_from_json(&value, &mut warnings);
    if drafts.is_empty() {
        return Some(McpPasteParse {
            kind: McpPasteKind::Unknown,
            drafts: Vec::new(),
            catalog_id: None,
            warnings,
            error: Some("JSON did not contain any MCP server specs".into()),
        });
    }
    Some(McpPasteParse {
        kind: McpPasteKind::JsonServers,
        drafts,
        catalog_id: None,
        warnings,
        error: None,
    })
}

fn extract_json_slice(text: &str) -> Option<&str> {
    let mut text = text.trim();
    if let Some(rest) = text.strip_prefix("```json") {
        text = rest;
    } else if let Some(rest) = text.strip_prefix("```JSON") {
        text = rest;
    } else if let Some(rest) = text.strip_prefix("```") {
        text = rest;
    }
    text = text.trim();
    if let Some(rest) = text.strip_suffix("```") {
        text = rest.trim();
    }
    let start = text.find(['{', '['])?;
    let open = text.as_bytes()[start];
    let close = if open == b'{' { '}' } else { ']' };
    let end = text.rfind(close)?;
    if end >= start {
        Some(&text[start..=end])
    } else {
        None
    }
}

fn drafts_from_json(value: &Value, warnings: &mut Vec<String>) -> Vec<McpServerEntry> {
    if let Some(map) = named_server_map(value) {
        return entries_from_map(map, warnings);
    }
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("server-{}", i + 1));
                match entry_from_pasted_spec(&name, item) {
                    Some(entry) => Some(entry),
                    None => {
                        warnings.push(format!("skipped JSON array item {name}: not a server spec"));
                        None
                    }
                }
            })
            .collect(),
        Value::Object(obj) => {
            if obj.contains_key("command") || obj.contains_key("url") || obj.contains_key("type") {
                let name = obj
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("imported");
                match entry_from_pasted_spec(name, value) {
                    Some(entry) => vec![entry],
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn named_server_map(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value
        .get("mcpServers")
        .and_then(Value::as_object)
        .or_else(|| value.get("servers").and_then(Value::as_object))
        .or_else(|| {
            value
                .get("mcp")
                .and_then(|mcp| mcp.get("servers"))
                .and_then(Value::as_object)
        })
        .or_else(|| {
            let mcp = value.get("mcp")?.as_object()?;
            if mcp.values().any(|v| v.is_object()) {
                Some(mcp)
            } else {
                None
            }
        })
}

fn entries_from_map(
    map: &serde_json::Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Vec<McpServerEntry> {
    let mut drafts = Vec::new();
    for (name, spec) in map {
        match entry_from_pasted_spec(name, spec) {
            Some(entry) => drafts.push(entry),
            None => warnings.push(format!("skipped '{name}': not a server spec")),
        }
    }
    drafts
}

/// Paste is more permissive than a live-config round-trip: Cursor/Claude often
/// omit `type` and only set `url`. Try the typed dialect first, then the
/// no-type remote spellings.
fn entry_from_pasted_spec(name: &str, spec: &Value) -> Option<McpServerEntry> {
    entry_from_json_spec(name, spec)
        .or_else(|| entry_from_json_spec_dialect(name, spec, JsonReadDialect::PlainNoType))
        .or_else(|| entry_from_json_spec_dialect(name, spec, JsonReadDialect::ServerUrlNoType))
        .or_else(|| entry_from_json_spec_dialect(name, spec, JsonReadDialect::GeminiUrlKeys))
}

fn parse_url(text: &str) -> Option<McpPasteParse> {
    let text = text.trim();
    if text.contains(char::is_whitespace) {
        return None;
    }
    let url = Url::parse(text).ok()?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }
    url.host_str()?;
    let mut entry = blank_entry(&name_from_url(text), "http");
    entry.url = Some(text.to_string());
    Some(McpPasteParse {
        kind: McpPasteKind::Url,
        drafts: vec![entry],
        catalog_id: None,
        warnings: Vec::new(),
        error: None,
    })
}

fn parse_command_line(text: &str) -> Option<McpPasteParse> {
    let line = if text.contains('\n') {
        text.lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))?
    } else {
        text.trim()
    };
    let tokens = match tokenize_command_line(line) {
        Ok(tokens) if !tokens.is_empty() => tokens,
        Ok(_) => return None,
        Err(error) => {
            return Some(McpPasteParse::unknown(error));
        }
    };
    if !looks_like_command(&tokens) {
        return None;
    }
    let mut entry = blank_entry(&name_from_tokens(&tokens), "stdio");
    entry.command = Some(tokens[0].clone());
    entry.args = tokens[1..].to_vec();
    Some(McpPasteParse {
        kind: McpPasteKind::Command,
        drafts: vec![entry],
        catalog_id: None,
        warnings: Vec::new(),
        error: None,
    })
}

fn looks_like_command(tokens: &[String]) -> bool {
    looks_like_launcher(&tokens[0]) || tokens[0].contains('/') || tokens[0].starts_with('.')
}

fn looks_like_launcher(command: &str) -> bool {
    matches!(
        command,
        "npx"
            | "bunx"
            | "pnpm"
            | "npm"
            | "yarn"
            | "uvx"
            | "uv"
            | "docker"
            | "podman"
            | "python"
            | "python3"
            | "node"
            | "deno"
            | "cargo"
            | "cua-driver"
    )
}

fn tokenize_command_line(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut chars = input.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
            }
            (None, '|' | ';' | '<' | '>') => {
                return Err("paste is not a single command".into());
            }
            (None, '&') if chars.peek() == Some(&'&') => {
                return Err("paste is not a single command".into());
            }
            (None, '$') if chars.peek() == Some(&'(') => {
                return Err("paste must not expand a subshell".into());
            }
            (None, '`') => return Err("paste must not expand a subshell".into()),
            (_, c) => buf.push(c),
        }
    }
    if quote.is_some() {
        return Err("unclosed quote in command".into());
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    Ok(tokens)
}

fn name_from_url(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .map(|host| {
            let host = host.strip_prefix("www.").unwrap_or(&host);
            let host = host.strip_prefix("mcp.").unwrap_or(host);
            sanitize_name(&host.replace('.', "-"))
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "imported".into())
}

fn name_from_tokens(tokens: &[String]) -> String {
    if let Some(cmd) = tokens.first() {
        let base = cmd.trim_end_matches('/').rsplit('/').next().unwrap_or(cmd);
        if base == "cua-driver" {
            return "cua-driver".into();
        }
    }
    tokens
        .iter()
        .rev()
        .find(|t| {
            !t.starts_with('-') && !t.contains('=') && t.chars().any(|c| c.is_ascii_alphabetic())
        })
        .map(|t| {
            let t = t.trim_end_matches('/');
            let base = t.rsplit('/').next().unwrap_or(t);
            let base = base.strip_prefix('@').unwrap_or(base);
            sanitize_name(base)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "imported".into())
}

fn sanitize_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if c == '.' || c == '@' {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_paste_is_empty() {
        let parsed = parse_pasted_mcp("  \n ");
        assert_eq!(parsed.kind, McpPasteKind::Empty);
        assert!(parsed.drafts.is_empty());
    }

    #[test]
    fn parses_streamable_http_url() {
        let parsed = parse_pasted_mcp("https://mcp.example.com/mcp");
        assert_eq!(parsed.kind, McpPasteKind::Url);
        assert_eq!(parsed.drafts.len(), 1);
        assert_eq!(parsed.drafts[0].transport, "http");
        assert_eq!(
            parsed.drafts[0].url.as_deref(),
            Some("https://mcp.example.com/mcp")
        );
        assert_eq!(parsed.drafts[0].name, "example-com");
    }

    #[test]
    fn parses_npx_command_with_quoted_args() {
        let parsed = parse_pasted_mcp("npx -y @modelcontextprotocol/server-github");
        assert_eq!(parsed.kind, McpPasteKind::Command);
        let draft = &parsed.drafts[0];
        assert_eq!(draft.command.as_deref(), Some("npx"));
        assert_eq!(draft.args, ["-y", "@modelcontextprotocol/server-github"]);
        assert_eq!(draft.name, "server-github");
        assert_eq!(draft.transport, "stdio");
    }

    #[test]
    fn parses_uvx_and_docker() {
        let uvx = parse_pasted_mcp("uvx mcp-server-git");
        assert_eq!(uvx.drafts[0].command.as_deref(), Some("uvx"));
        assert_eq!(uvx.drafts[0].name, "mcp-server-git");

        let docker = parse_pasted_mcp("docker run -i --rm mcp/git");
        assert_eq!(docker.drafts[0].command.as_deref(), Some("docker"));
        assert_eq!(docker.drafts[0].args, ["run", "-i", "--rm", "mcp/git"]);
        assert_eq!(docker.drafts[0].name, "git");
    }

    #[test]
    fn rejects_piped_or_compound_commands() {
        let parsed = parse_pasted_mcp("npx -y foo | sh");
        assert_eq!(parsed.kind, McpPasteKind::Unknown);
        assert!(parsed.error.unwrap().contains("single command"));
    }

    #[test]
    fn parses_community_mcp_servers_json() {
        let parsed = parse_pasted_mcp(
            r#"{
              "mcpServers": {
                "github": {
                  "command": "npx",
                  "args": ["-y", "@modelcontextprotocol/server-github"],
                  "env": { "GITHUB_TOKEN": "secret" }
                },
                "remote": { "url": "https://example.com/mcp" }
              }
            }"#,
        );
        assert_eq!(parsed.kind, McpPasteKind::JsonServers);
        assert_eq!(parsed.drafts.len(), 2);
        let github = parsed.drafts.iter().find(|d| d.name == "github").unwrap();
        assert_eq!(github.command.as_deref(), Some("npx"));
        assert_eq!(
            github.env.get("GITHUB_TOKEN").map(String::as_str),
            Some("secret")
        );
        let remote = parsed.drafts.iter().find(|d| d.name == "remote").unwrap();
        assert_eq!(remote.transport, "http");
        assert_eq!(remote.url.as_deref(), Some("https://example.com/mcp"));
    }

    #[test]
    fn parses_fenced_vscode_servers_json() {
        let parsed = parse_pasted_mcp(
            "```json\n{\"servers\":{\"fs\":{\"command\":\"npx\",\"args\":[\"-y\",\"server-fs\"]}}}\n```",
        );
        assert_eq!(parsed.kind, McpPasteKind::JsonServers);
        assert_eq!(parsed.drafts[0].name, "fs");
        assert_eq!(parsed.drafts[0].command.as_deref(), Some("npx"));
    }

    #[test]
    fn deep_link_catalog_wins_and_does_not_install() {
        let parsed = parse_pasted_mcp(
            "skillstar://mcp?catalog=io.github.modelcontextprotocol/server-github&url=https://evil.example/mcp",
        );
        assert_eq!(parsed.kind, McpPasteKind::Catalog);
        assert_eq!(
            parsed.catalog_id.as_deref(),
            Some("io.github.modelcontextprotocol/server-github")
        );
        assert!(parsed.drafts.is_empty());
        assert!(!parsed.warnings.is_empty());
    }

    #[test]
    fn deep_link_url_seeds_http_draft() {
        let parsed = parse_pasted_mcp("skillstar://mcp?url=https://mcp.example.com/mcp&name=docs");
        assert_eq!(parsed.kind, McpPasteKind::DeepLink);
        assert_eq!(parsed.drafts[0].name, "docs");
        assert_eq!(
            parsed.drafts[0].url.as_deref(),
            Some("https://mcp.example.com/mcp")
        );
    }

    #[test]
    fn deep_link_config_json_is_still_a_draft() {
        let parsed = parse_pasted_mcp(
            r#"skillstar://mcp?config={"mcpServers":{"ctx":{"command":"npx","args":["-y","ctx7"]}}}"#,
        );
        assert_eq!(parsed.kind, McpPasteKind::DeepLink);
        assert_eq!(parsed.drafts[0].name, "ctx");
        assert_eq!(parsed.drafts[0].command.as_deref(), Some("npx"));
    }

    #[test]
    fn raw_query_string_is_accepted() {
        let parsed = parse_pasted_mcp("catalog=io.github.foo/bar");
        assert_eq!(parsed.kind, McpPasteKind::Catalog);
        assert_eq!(parsed.catalog_id.as_deref(), Some("io.github.foo/bar"));
    }

    #[test]
    fn unknown_prose_is_not_a_command() {
        let parsed = parse_pasted_mcp("please install github mcp for me");
        assert_eq!(parsed.kind, McpPasteKind::Unknown);
        assert!(parsed.drafts.is_empty());
    }

    #[test]
    fn never_writes_an_id_so_create_path_assigns_one() {
        let parsed = parse_pasted_mcp("https://example.com/mcp");
        assert!(parsed.drafts[0].id.is_empty());
    }

    #[test]
    fn parses_cua_driver_command() {
        let parsed = parse_pasted_mcp("cua-driver mcp");
        assert_eq!(parsed.kind, McpPasteKind::Command);
        assert_eq!(parsed.drafts.len(), 1);
        let draft = &parsed.drafts[0];
        assert_eq!(draft.name, "cua-driver");
        assert_eq!(draft.command.as_deref(), Some("cua-driver"));
        assert_eq!(draft.args, ["mcp"]);
        assert_eq!(draft.transport, "stdio");
    }
}
