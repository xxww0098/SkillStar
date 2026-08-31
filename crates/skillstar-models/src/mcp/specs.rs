//! Per-tool wire-format spec generation (canonical JSON, OpenCode, Codex TOML).

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

use super::*;

/// How a client's JSON wire format labels the transport, and under which key
/// it expects a remote server's endpoint.
///
/// Every difference in §5.2 of the MCP design research that a *writer* has to
/// respect is one variant here, so a new target is a dialect selection rather
/// than another hand-rolled `match entry.transport`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonDialect {
    /// `type: "stdio" | "http" | "sse"` + `url` — Claude Code, Cursor, Kiro,
    /// ZCode, VS Code.
    ///
    /// Writing `type` on every remote entry is not cosmetic: Claude Code
    /// rejects a `url` entry that has no `type` outright ("has a `url` but no
    /// `type`") and skips the server (research §5.3 #1, P0-3).
    Typed,
    /// `type: "streamableHttp" | "sse"` — Cline's camelCase spelling, which
    /// matches neither the spec's `streamable-http` nor VS Code's `http`
    /// (research §5.3 #8). stdio entries carry no `type` at all.
    ClineCamel,
    /// No `type` key; the endpoint lives under `serverUrl` — Windsurf, the
    /// single most-often-mis-written format in the matrix (research §5.3 #6).
    ServerUrlNoType,
    /// No `type` key; SSE goes under `url` and Streamable HTTP under
    /// `httpUrl` — Gemini CLI distinguishes transports purely by which key is
    /// present (research §5.3 #7).
    GeminiUrlKeys,
    /// No `type` key; `command` means local, `url` means remote — Zed and
    /// Claude Desktop Chat (research §5.1).
    ///
    /// Note this is the *opposite* rule from [`Self::Typed`], which Claude
    /// **Code** uses: there a `url` without a `type` is a hard configuration
    /// error. The two Claude surfaces do not share a dialect, so neither may
    /// borrow the other's spec builder.
    PlainNoType,
}

impl JsonDialect {
    /// The `type` value to write for a remote entry, or `None` when the
    /// format has no `type` key at all.
    fn remote_type(self, transport: &str) -> Option<&'static str> {
        match self {
            Self::Typed => Some(if transport == "sse" { "sse" } else { "http" }),
            Self::ClineCamel => Some(if transport == "sse" {
                "sse"
            } else {
                "streamableHttp"
            }),
            Self::ServerUrlNoType | Self::GeminiUrlKeys | Self::PlainNoType => None,
        }
    }

    /// The key a remote entry's endpoint goes under.
    fn url_key(self, transport: &str) -> &'static str {
        match self {
            Self::ServerUrlNoType => "serverUrl",
            // Gemini CLI reads `url` as SSE and `httpUrl` as Streamable HTTP.
            Self::GeminiUrlKeys if transport != "sse" => "httpUrl",
            _ => "url",
        }
    }

    /// Whether stdio entries carry `type: "stdio"`.
    fn stdio_type(self) -> Option<&'static str> {
        match self {
            Self::Typed => Some("stdio"),
            Self::ClineCamel | Self::ServerUrlNoType | Self::GeminiUrlKeys | Self::PlainNoType => {
                None
            }
        }
    }
}

/// Build a JSON server value in the given [`JsonDialect`].
///
/// Shared by every JSON target: stdio writes `command`/`args`/`env`/`cwd`,
/// remote writes the dialect's URL key plus `headers`. Tool-specific extras
/// (approval lists, timeouts) are layered on by the per-tool builders below.
pub(crate) fn json_spec(entry: &McpServerEntry, dialect: JsonDialect) -> Value {
    let mut obj = Map::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            if let Some(kind) = dialect.remote_type(&entry.transport) {
                obj.insert("type".into(), json!(kind));
            }
            if let Some(url) = &entry.url {
                obj.insert(dialect.url_key(&entry.transport).into(), json!(url));
            }
            if !entry.headers.is_empty() {
                obj.insert("headers".into(), json!(string_map(&entry.headers)));
            }
        }
        _ => {
            if let Some(kind) = dialect.stdio_type() {
                obj.insert("type".into(), json!(kind));
            }
            if let Some(cmd) = &entry.command {
                obj.insert("command".into(), json!(cmd));
            }
            if !entry.args.is_empty() {
                obj.insert("args".into(), json!(entry.args));
            }
            if !entry.env.is_empty() {
                obj.insert("env".into(), json!(string_map(&entry.env)));
            }
            if let Some(cwd) = &entry.cwd {
                obj.insert("cwd".into(), json!(cwd));
            }
        }
    }
    Value::Object(obj)
}

/// Canonical "community" mcpServers value (base shape shared by Claude Code,
/// Kiro, Cursor, ZCode, and VS Code). stdio keeps `type`; http/sse carry
/// `type` + `url` and optional `headers`. Does **not** include any
/// tool-specific approval/exposure/timeout fields — callers layer those on top
/// per tool (see [`claude_code_spec`] and [`kiro_spec`]).
pub(crate) fn canonical_spec(entry: &McpServerEntry) -> Value {
    json_spec(entry, JsonDialect::Typed)
}

/// Claude Code value (`~/.claude.json` `mcpServers.<name>`): canonical shape
/// only. Claude Code has no verified native per-server auto-approve,
/// disabled-tools, or timeout field, so none of the approval/exposure config
/// is projected here.
pub(crate) fn claude_code_spec(entry: &McpServerEntry) -> Value {
    canonical_spec(entry)
}

/// Claude Desktop Chat value (`claude_desktop_config.json`
/// `mcpServers.<name>`).
///
/// **Not** [`canonical_spec`], despite the shared `mcpServers` root key: the
/// documented Claude Desktop format lists no `type` field and identifies a
/// local server by its `command` (research §5.1). Reusing Claude Code's
/// type-carrying builder here would write a key this client does not read —
/// the exact inverse of the P0-3 bug on the Code side.
///
/// Claude Desktop has no documented per-server auto-approve, disabled-tools or
/// timeout field, so none of the approval/exposure config is projected.
pub(crate) fn claude_desktop_chat_spec(entry: &McpServerEntry) -> Value {
    json_spec(entry, JsonDialect::PlainNoType)
}

/// Cursor value (`~/.cursor/mcp.json` `mcpServers.<name>`): canonical shape
/// only. Cursor has no verified native per-server auto-approve,
/// disabled-tools, or timeout field, so none of the approval/exposure config
/// is projected here.
pub(crate) fn cursor_spec(entry: &McpServerEntry) -> Value {
    canonical_spec(entry)
}

/// Kiro value (`~/.kiro/settings/mcp.json` `mcpServers.<name>`): canonical
/// shape plus `autoApprove` and `disabledTools` — the exact fields documented
/// at kiro.dev/docs/cli/mcp/configuration. `timeout_ms` has no documented Kiro
/// field and is not projected.
pub(crate) fn kiro_spec(entry: &McpServerEntry) -> Value {
    let mut obj = match canonical_spec(entry) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if entry.auto_approve_all {
        obj.insert("autoApprove".into(), json!(["*"]));
    } else if !entry.auto_approve_tools.is_empty() {
        obj.insert("autoApprove".into(), json!(entry.auto_approve_tools));
    }
    if !entry.disabled_tools.is_empty() {
        obj.insert("disabledTools".into(), json!(entry.disabled_tools));
    }
    Value::Object(obj)
}

/// OpenCode value: stdio→`local` (command array, `environment`), http/sse→`remote`.
/// Also carries `timeout` (ms) — OpenCode's own documented field. OpenCode has
/// no per-server auto-approve/disabled-tools field (tool exposure is
/// controlled globally via the `tools`/`agent` glob config, out of scope for a
/// single server entry), so those are not projected.
pub(crate) fn opencode_spec(entry: &McpServerEntry) -> Value {
    let mut obj = Map::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            obj.insert("type".into(), json!("remote"));
            if let Some(url) = &entry.url {
                obj.insert("url".into(), json!(url));
            }
            if !entry.headers.is_empty() {
                obj.insert("headers".into(), json!(string_map(&entry.headers)));
            }
            obj.insert("enabled".into(), json!(true));
        }
        _ => {
            obj.insert("type".into(), json!("local"));
            let mut command_arr: Vec<Value> = Vec::new();
            command_arr.push(json!(entry.command.clone().unwrap_or_default()));
            for a in &entry.args {
                command_arr.push(json!(a));
            }
            obj.insert("command".into(), Value::Array(command_arr));
            if !entry.env.is_empty() {
                obj.insert("environment".into(), json!(string_map(&entry.env)));
            }
            obj.insert("enabled".into(), json!(true));
        }
    }
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        obj.insert("timeout".into(), json!(ms));
    }
    Value::Object(obj)
}

/// ZCode desktop agent MCP (`~/.zcode/cli/config.json` → `mcp.servers.<name>`).
/// Uses the same community stdio / http shape as Claude Code (`command` + `args` + `env`),
/// not the OpenCode `local`/`remote` form under `v2/config.json`.
pub(crate) fn zcode_cli_spec(entry: &McpServerEntry) -> Value {
    canonical_spec(entry)
}

/// VS Code value (`~/.copilot/mcp-config.json` → `servers.<name>`).
///
/// Same `type`/`url` vocabulary as Claude Code; what differs is the *root*
/// key (`servers`, not `mcpServers` — research §5.3 #11), which the writer in
/// `tools.rs` owns. `inputs` / `sandbox` are VS Code features SkillStar does
/// not author, and merge semantics leave any the user wrote alone.
pub(crate) fn vscode_spec(entry: &McpServerEntry) -> Value {
    canonical_spec(entry)
}

/// Windsurf value (`~/.codeium/windsurf/mcp_config.json` → `mcpServers.<name>`).
///
/// The documented format lists no `type` key and puts a remote endpoint under
/// `serverUrl`. Writing `url` here is the single most common mistake against
/// this format (research §5.3 #6), so the dialect — not this function — is
/// what guarantees the right key.
pub(crate) fn windsurf_spec(entry: &McpServerEntry) -> Value {
    json_spec(entry, JsonDialect::ServerUrlNoType)
}

/// Cline value (`~/.cline/mcp.json` → `mcpServers.<name>`): camelCase
/// `streamableHttp` plus Cline's own `autoApprove` and `timeout` fields.
///
/// `disabled` is deliberately never written. Cline stores the user's own
/// enable/disable toggle in that key; SkillStar removes an entry it should not
/// project rather than flipping a flag the user owns.
pub(crate) fn cline_spec(entry: &McpServerEntry) -> Value {
    let mut obj = match json_spec(entry, JsonDialect::ClineCamel) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if entry.auto_approve_all {
        obj.insert("autoApprove".into(), json!(["*"]));
    } else if !entry.auto_approve_tools.is_empty() {
        obj.insert("autoApprove".into(), json!(entry.auto_approve_tools));
    }
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        obj.insert("timeout".into(), json!(ms));
    }
    Value::Object(obj)
}

/// Gemini CLI value (`~/.gemini/settings.json` → `mcpServers.<name>`).
///
/// Transport is encoded by *which URL key is present* rather than a `type`
/// field: `url` = SSE, `httpUrl` = Streamable HTTP. Also carries the two
/// documented Gemini-only fields SkillStar has data for — `excludeTools`
/// (Gemini's own name for the disabled-tools blacklist, which takes precedence
/// over `includeTools`) and `timeout` in milliseconds.
///
/// `trust: true` would skip every confirmation prompt. It is intentionally not
/// projected from `auto_approve_all`: the field is server-wide and irreversible
/// from the agent's side, and blanket-trusting a freshly installed server is
/// exactly the posture the MCP security guidance argues against.
pub(crate) fn gemini_cli_spec(entry: &McpServerEntry) -> Value {
    let mut obj = match json_spec(entry, JsonDialect::GeminiUrlKeys) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if !entry.disabled_tools.is_empty() {
        obj.insert("excludeTools".into(), json!(entry.disabled_tools));
    }
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        obj.insert("timeout".into(), json!(ms));
    }
    Value::Object(obj)
}

/// Zed value (`~/.config/zed/settings.json` → `context_servers.<name>`).
///
/// No `type` key: a `command` makes it local, a `url` makes it remote. The
/// unusual part is the root key (`context_servers`, not `mcpServers` —
/// research §5.3 #9), owned by the writer in `tools.rs`.
pub(crate) fn zed_spec(entry: &McpServerEntry) -> Value {
    json_spec(entry, JsonDialect::PlainNoType)
}

/// Maka value (`<config>/Maka/workspaces/default/mcp.json` → `mcpServers.<name>`).
///
/// No `type` key: a `command` is stdio, a `url` is remote. Remote transport is
/// Maka's own `transport` field (`streamable-http` / `sse`), not a `type`.
/// `enabled` is projected as `true` because SkillStar removes an entry it
/// should not project rather than flipping Maka's own toggle. The wrapper
/// `version: 2` is owned by the writer in `tools.rs`.
pub(crate) fn maka_spec(entry: &McpServerEntry) -> Value {
    let mut obj = match json_spec(entry, JsonDialect::PlainNoType) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    obj.insert("enabled".into(), json!(true));
    match entry.transport.as_str() {
        "http" => {
            obj.insert("transport".into(), json!("streamable-http"));
        }
        "sse" => {
            obj.insert("transport".into(), json!("sse"));
        }
        _ => {}
    }
    Value::Object(obj)
}

/// Grok `[mcp_servers.<name>]` TOML table (`~/.grok/config.toml`).
pub(crate) fn grok_toml_table(entry: &McpServerEntry) -> toml::Table {
    let mut t = toml::Table::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            if let Some(url) = &entry.url {
                t.insert("url".into(), toml::Value::String(url.clone()));
            }
            if !entry.headers.is_empty() {
                t.insert(
                    "headers".into(),
                    toml::Value::Table(toml_string_table(&entry.headers)),
                );
            }
        }
        _ => {
            if let Some(cmd) = &entry.command {
                t.insert("command".into(), toml::Value::String(cmd.clone()));
            }
            if !entry.args.is_empty() {
                let arr: Vec<toml::Value> = entry
                    .args
                    .iter()
                    .map(|a| toml::Value::String(a.clone()))
                    .collect();
                t.insert("args".into(), toml::Value::Array(arr));
            }
            if let Some(cwd) = &entry.cwd {
                t.insert("cwd".into(), toml::Value::String(cwd.clone()));
            }
            if !entry.env.is_empty() {
                t.insert(
                    "env".into(),
                    toml::Value::Table(toml_string_table(&entry.env)),
                );
            }
        }
    }
    t
}

/// Codex `[mcp_servers.<name>]` TOML table. Also carries `disabled_tools`
/// (blacklist) and `tool_timeout_sec` — both documented Codex fields.
/// `auto_approve_*` has no Codex per-server equivalent (approval is
/// controlled by the separate `[apps.<name>]` connector config) and is not
/// projected.
pub(crate) fn codex_toml_table(entry: &McpServerEntry) -> toml::Table {
    let mut t = toml::Table::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            t.insert("type".into(), toml::Value::String(entry.transport.clone()));
            if let Some(url) = &entry.url {
                t.insert("url".into(), toml::Value::String(url.clone()));
            }
            if !entry.headers.is_empty() {
                t.insert(
                    "http_headers".into(),
                    toml::Value::Table(toml_string_table(&entry.headers)),
                );
            }
        }
        _ => {
            t.insert("type".into(), toml::Value::String("stdio".into()));
            if let Some(cmd) = &entry.command {
                t.insert("command".into(), toml::Value::String(cmd.clone()));
            }
            if !entry.args.is_empty() {
                let arr: Vec<toml::Value> = entry
                    .args
                    .iter()
                    .map(|a| toml::Value::String(a.clone()))
                    .collect();
                t.insert("args".into(), toml::Value::Array(arr));
            }
            if let Some(cwd) = &entry.cwd {
                t.insert("cwd".into(), toml::Value::String(cwd.clone()));
            }
            if !entry.env.is_empty() {
                t.insert(
                    "env".into(),
                    toml::Value::Table(toml_string_table(&entry.env)),
                );
            }
        }
    }
    if !entry.disabled_tools.is_empty() {
        let arr: Vec<toml::Value> = entry
            .disabled_tools
            .iter()
            .map(|d| toml::Value::String(d.clone()))
            .collect();
        t.insert("disabled_tools".into(), toml::Value::Array(arr));
    }
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        let sec = (ms / 1000).max(1) as i64;
        t.insert("tool_timeout_sec".into(), toml::Value::Integer(sec));
    }
    t
}

fn string_map(m: &BTreeMap<String, String>) -> Map<String, Value> {
    m.iter().map(|(k, v)| (k.clone(), json!(v))).collect()
}

fn toml_string_table(m: &BTreeMap<String, String>) -> toml::Table {
    m.iter()
        .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
        .collect()
}
