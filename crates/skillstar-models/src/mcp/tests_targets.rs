//! Per-target wire-format tests: the field differences that make an entry work
//! in one client and be silently ignored in another.
//!
//! Split out of `tests.rs` (already ~570 lines) so neither file approaches the
//! repo's file-size threshold.

use super::*;
use serde_json::Value;

fn stdio(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "stdio");
    e.command = Some("npx".into());
    e.args = vec!["-y".into(), "example-mcp".into()];
    e.env.insert("API_KEY".into(), "secret".into());
    e
}

fn http(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "http");
    e.url = Some("https://example.com/mcp".into());
    e.headers
        .insert("Authorization".into(), "Bearer xxx".into());
    e
}

fn sse(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "sse");
    e.url = Some("https://example.com/sse".into());
    e
}

// ---------------------------------------------------------------------------
// P0-3: a `url` without a `type` is a configuration error in Claude Code
// ---------------------------------------------------------------------------

/// Does a **remote** entry in this target's documented format carry a `type`?
///
/// Every registry row must appear below. That is the point of the table: a new
/// target cannot be added without someone deciding, in writing, whether its
/// format is type-carrying — which is exactly the decision that produces
/// silently-ignored entries when it is made by accident.
fn remote_format_has_type_key(tool_id: &str) -> bool {
    match tool_id {
        // Documented `type: http | sse` (or the TOML equivalent).
        "claude-code" | "cursor" | "kiro" | "zcode" | "vscode" | "codex" => true,
        // Cline has a `type` for remotes, spelled in camelCase.
        "cline" => true,
        // OpenCode's own `remote` vocabulary lives under `type`.
        "opencode" => true,
        // No `type` key in the documented format: transport is implied by
        // which keys are present. `claude-desktop-chat` sits here and
        // `claude-code` above on purpose — the two Claude surfaces document
        // opposite rules, and writing Code's `type` into Chat's file would
        // hand that client a key it does not read. Antigravity rejects
        // `type: stdio`. Hermes YAML has no type key either.
        "grok"
        | "hermes"
        | "windsurf"
        | "gemini-cli"
        | "antigravity"
        | "zed"
        | "claude-desktop-chat" => false,
        other => panic!(
            "tool '{other}' is in the registry but not in the wire-type policy table — decide whether a remote entry in its format carries a `type` key and record it here"
        ),
    }
}

/// The `type` value a **stdio** entry carries in this target's format, if any.
///
/// Split from the remote table because the two are genuinely independent:
/// Cline documents `type` values only for remote transports, so its stdio
/// entries carry none even though the format has the key.
fn stdio_type_token(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "claude-code" | "cursor" | "kiro" | "zcode" | "vscode" | "codex" => Some("stdio"),
        // OpenCode's stdio spelling is its own word.
        "opencode" => Some("local"),
        // Cline's documented `type` values are `streamableHttp` and `sse`
        // only — a local server is identified by having a `command`.
        "cline" => None,
        "grok"
        | "hermes"
        | "windsurf"
        | "gemini-cli"
        | "antigravity"
        | "zed"
        | "claude-desktop-chat" => None,
        other => {
            panic!("tool '{other}' is in the registry but not in the stdio wire-type policy table")
        }
    }
}

/// Read a target's projected value for `entry` back out of its own writer.
///
/// Goes through the real `upsert` + `read_servers` pair rather than calling a
/// spec builder directly, so the root key (`mcpServers` vs `servers` vs
/// `context_servers`) is covered too.
fn project(spec: &McpToolSpec, entry: &McpServerEntry, dir: &TempDir) -> Value {
    let path = dir.path().join(format!("{}.cfg", spec.id));
    (spec.upsert)(&path, entry).unwrap_or_else(|e| panic!("{}: upsert failed: {e}", spec.id));
    let content = std::fs::read_to_string(&path).unwrap();
    if spec.id == "codex" || spec.id == "grok" {
        let table: toml::Table = toml::from_str(&content).unwrap();
        return serde_json::to_value(
            table
                .get("mcp_servers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get(&entry.name))
                .unwrap_or_else(|| panic!("{}: server missing from TOML", spec.id)),
        )
        .unwrap();
    }
    if spec.id == "hermes" {
        let yaml: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
        let entry = yaml
            .get("mcp_servers")
            .and_then(|m| m.get(&entry.name))
            .unwrap_or_else(|| panic!("{}: server missing from YAML {content}", spec.id));
        return serde_json::to_value(entry).unwrap();
    }
    let root: Value = serde_json::from_str(&content).unwrap();
    for key in [MCP_SERVERS_KEY, VSCODE_SERVERS_KEY, ZED_SERVERS_KEY, "mcp"] {
        if let Some(found) = root
            .get(key)
            .and_then(|m| m.get("servers").or(Some(m)))
            .and_then(|m| m.get(&entry.name))
        {
            return found.clone();
        }
    }
    panic!("{}: server missing from {content}", spec.id);
}

/// Claude Code refuses an entry that has a `url` but no `type` — it reports a
/// configuration error and skips the server entirely. Any target whose format
/// carries a `type` must therefore always write one alongside a URL, and the
/// ones that do not must not invent a key their client will not read.
#[test]
fn remote_entries_carry_a_type_exactly_where_the_format_has_one() {
    let dir = TempDir::new("wire-type");
    for spec in mcp_tool_specs() {
        for entry in [http("remote-http"), sse("remote-sse")] {
            let value = project(spec, &entry, &dir);
            let has_url = value.get("url").is_some()
                || value.get("serverUrl").is_some()
                || value.get("httpUrl").is_some();
            assert!(has_url, "{}: {value} carries no endpoint", spec.id);
            assert_eq!(
                value.get("type").is_some(),
                remote_format_has_type_key(spec.id),
                "{} projected {value} for transport {}",
                spec.id,
                entry.transport
            );
        }
    }
}

/// The same rule for stdio: a type-carrying format must say `stdio`, not leave
/// it to be guessed.
#[test]
fn stdio_entries_carry_a_type_exactly_where_the_format_has_one() {
    let dir = TempDir::new("wire-type-stdio");
    for spec in mcp_tool_specs() {
        let value = project(spec, &stdio("local"), &dir);
        assert_eq!(
            value.get("type").and_then(Value::as_str),
            stdio_type_token(spec.id),
            "{} projected {value}",
            spec.id
        );
    }
}

// ---------------------------------------------------------------------------
// Per-client field differences
// ---------------------------------------------------------------------------

/// Windsurf reads a remote endpoint from `serverUrl`. Writing `url` produces a
/// file Windsurf parses without complaint and then ignores.
#[test]
fn windsurf_writes_serverurl_rather_than_url() {
    let value = windsurf_spec(&http("remote"));
    assert_eq!(value["serverUrl"], "https://example.com/mcp");
    assert!(value.get("url").is_none(), "{value}");
    assert!(value.get("type").is_none(), "{value}");
    assert_eq!(value["headers"]["Authorization"], "Bearer xxx");
}

/// Cline's Streamable HTTP token is camelCase — neither the spec's
/// `streamable-http` nor VS Code's `http`.
#[test]
fn cline_writes_camelcase_streamablehttp() {
    assert_eq!(cline_spec(&http("remote"))["type"], "streamableHttp");
    assert_eq!(cline_spec(&sse("remote"))["type"], "sse");
    // stdio has no `type` in Cline's documented format.
    assert!(cline_spec(&stdio("local")).get("type").is_none());
}

/// Cline's own approval and timeout fields are projected; `disabled` is not,
/// because that key holds the user's own toggle.
#[test]
fn cline_projects_approval_and_timeout_but_never_disabled() {
    let mut entry = stdio("local");
    entry.auto_approve_all = true;
    entry.timeout_ms = Some(30_000);
    let value = cline_spec(&entry);
    assert_eq!(value["autoApprove"], serde_json::json!(["*"]));
    assert_eq!(value["timeout"], 30_000);
    assert!(value.get("disabled").is_none(), "{value}");
}

/// Gemini CLI has no `type`: `httpUrl` means Streamable HTTP and `url` means
/// SSE. Copying a config from anywhere else gets this wrong.
#[test]
fn gemini_cli_splits_transports_across_two_url_keys() {
    let over_http = gemini_cli_spec(&http("remote"));
    assert_eq!(over_http["httpUrl"], "https://example.com/mcp");
    assert!(over_http.get("url").is_none(), "{over_http}");
    assert!(over_http.get("type").is_none(), "{over_http}");

    let over_sse = gemini_cli_spec(&sse("remote"));
    assert_eq!(over_sse["url"], "https://example.com/sse");
    assert!(over_sse.get("httpUrl").is_none(), "{over_sse}");
}

/// Gemini spells the disabled-tools blacklist `excludeTools`, and never gets
/// `trust: true` handed to it from SkillStar's auto-approve flag.
#[test]
fn gemini_cli_maps_disabled_tools_to_excludetools_and_never_sets_trust() {
    let mut entry = stdio("local");
    entry.disabled_tools = vec!["dangerous".into()];
    entry.auto_approve_all = true;
    let value = gemini_cli_spec(&entry);
    assert_eq!(value["excludeTools"], serde_json::json!(["dangerous"]));
    assert!(value.get("disabledTools").is_none(), "{value}");
    assert!(value.get("trust").is_none(), "{value}");
}

/// VS Code's root key is `servers`; Zed's is `context_servers`. Both differ
/// from the community `mcpServers` every other JSON target uses.
#[test]
fn vscode_and_zed_use_their_own_root_keys() {
    let dir = TempDir::new("root-keys");

    let vscode = dir.path().join("vscode.json");
    json_named_map_upsert(
        &vscode,
        VSCODE_SERVERS_KEY,
        "srv",
        vscode_spec(&stdio("srv")),
    )
    .unwrap();
    let root: Value = serde_json::from_str(&std::fs::read_to_string(&vscode).unwrap()).unwrap();
    assert!(root.get("servers").is_some(), "{root}");
    assert!(root.get("mcpServers").is_none(), "{root}");

    let zed = dir.path().join("zed.json");
    json_named_map_upsert(&zed, ZED_SERVERS_KEY, "srv", zed_spec(&stdio("srv"))).unwrap();
    let root: Value = serde_json::from_str(&std::fs::read_to_string(&zed).unwrap()).unwrap();
    assert!(root.get("context_servers").is_some(), "{root}");
    assert!(root.get("mcpServers").is_none(), "{root}");
}

/// Antigravity rejects `type: "stdio"` and hides the server. It also must not
/// inherit Gemini CLI's `httpUrl` key — different product, different file.
#[test]
fn antigravity_omits_the_type_key_that_the_ide_rejects() {
    let local = antigravity_spec(&stdio("local"));
    assert_eq!(local["command"], "npx");
    assert!(local.get("type").is_none(), "{local}");

    let remote = antigravity_spec(&http("remote"));
    assert_eq!(remote["url"], "https://example.com/mcp");
    assert!(remote.get("type").is_none(), "{remote}");
    assert!(remote.get("httpUrl").is_none(), "{remote}");
}

/// Claude Desktop Chat documents no `type` key at all — the inverse of Claude
/// Code, which *rejects* a `url` entry that has none. The two surfaces share
/// the `mcpServers` root key and nothing else, so they must not share a spec
/// builder.
#[test]
fn claude_desktop_chat_omits_the_type_key_claude_code_requires() {
    let local = claude_desktop_chat_spec(&stdio("local"));
    assert_eq!(local["command"], "npx");
    assert_eq!(local["args"][1], "example-mcp");
    assert_eq!(local["env"]["API_KEY"], "secret");
    assert!(local.get("type").is_none(), "{local}");

    let remote = claude_desktop_chat_spec(&http("remote"));
    assert_eq!(remote["url"], "https://example.com/mcp");
    assert_eq!(remote["headers"]["Authorization"], "Bearer xxx");
    assert!(remote.get("type").is_none(), "{remote}");
    // Neither Windsurf's `serverUrl` nor Gemini's `httpUrl`.
    assert!(remote.get("serverUrl").is_none(), "{remote}");
    assert!(remote.get("httpUrl").is_none(), "{remote}");

    // Claude Code, on the same entry, must still write its `type`.
    assert_eq!(claude_code_spec(&http("remote"))["type"], "http");
}

/// Claude Desktop Chat's projection lands under the community `mcpServers`
/// key, and leaves every unrelated key in that file alone — the file also
/// holds the user's app settings.
#[test]
fn claude_desktop_chat_writes_mcpservers_and_preserves_the_rest() {
    let dir = TempDir::new("desktop-chat-root-key");
    let path = dir.path().join("claude_desktop_config.json");
    std::fs::write(
        &path,
        r#"{"theme":"dark","mcpServers":{"user-owned":{"command":"keep"}}}"#,
    )
    .unwrap();

    let spec = mcp_tool_spec(CLAUDE_DESKTOP_CHAT_TOOL_ID).expect("public target");
    (spec.upsert)(&path, &stdio("managed")).unwrap();

    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(root["theme"], "dark");
    assert_eq!(root["mcpServers"]["user-owned"]["command"], "keep");
    assert_eq!(root["mcpServers"]["managed"]["command"], "npx");
    assert!(root.get("servers").is_none(), "{root}");
    assert!(root.get("context_servers").is_none(), "{root}");
}

/// Zed decides local vs remote from which key is present, with no `type`.
#[test]
fn zed_omits_type_and_signals_remote_with_url() {
    let remote = zed_spec(&http("remote"));
    assert_eq!(remote["url"], "https://example.com/mcp");
    assert!(remote.get("type").is_none(), "{remote}");

    let local = zed_spec(&stdio("local"));
    assert_eq!(local["command"], "npx");
    assert!(local.get("type").is_none(), "{local}");
}

// ---------------------------------------------------------------------------
// Round trips
// ---------------------------------------------------------------------------

/// Can this target's format express the given transport well enough to get it
/// back on read?
///
/// Every exception here is a property of the *client's documented format*, not
/// of SkillStar's writer, and each is deliberately named rather than folded
/// into a blanket skip.
fn round_trip_preserves(tool_id: &str, transport: &str) -> bool {
    match (tool_id, transport) {
        // Grok's TOML carries no transport marker at all, so a remote entry is
        // indistinguishable from a malformed local one; the shared TOML reader
        // requires a `command` and drops it. Pre-existing.
        ("grok", "http" | "sse") => false,
        // OpenCode collapses both remote transports into one `remote` form, so
        // an http entry returns as sse (audit B.7-b, tracked separately).
        ("opencode", "http") => false,
        // Windsurf, Zed, Claude Desktop Chat, Antigravity and Hermes have no
        // `type` key: a URL is a URL. SSE is written faithfully but reads
        // back as http, because the file genuinely does not record which one
        // it was.
        ("windsurf" | "zed" | "claude-desktop-chat" | "antigravity" | "hermes", "sse") => false,
        _ => true,
    }
}

/// Writing an entry and importing it back must not change what it means.
///
/// This is the check that catches a dialect implemented on only one side —
/// the failure mode the audit found for OpenCode, where an `http` server came
/// back as `sse` and every other target then inherited the wrong transport.
#[test]
fn every_target_round_trips_transport_and_endpoint() {
    let dir = TempDir::new("round-trip");
    for spec in mcp_tool_specs() {
        for original in [stdio("rt"), http("rt"), sse("rt")] {
            if !round_trip_preserves(spec.id, &original.transport) {
                continue;
            }
            let path = dir
                .path()
                .join(format!("{}-{}.cfg", spec.id, original.transport));
            (spec.upsert)(&path, &original).unwrap();
            let content = std::fs::read_to_string(&path).unwrap();
            let read = (spec.read_servers)(&content)
                .unwrap_or_else(|e| panic!("{}: read_servers failed: {e}", spec.id));
            let found = read
                .iter()
                .find(|e| e.name == original.name)
                .unwrap_or_else(|| panic!("{}: entry missing after round trip", spec.id));

            assert_eq!(
                found.transport, original.transport,
                "{} changed the transport of a {} entry",
                spec.id, original.transport
            );
            match original.transport.as_str() {
                "http" | "sse" => assert_eq!(
                    found.url, original.url,
                    "{} lost the endpoint of a {} entry",
                    spec.id, original.transport
                ),
                _ => assert_eq!(
                    found.command, original.command,
                    "{} lost the command of a stdio entry",
                    spec.id
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared test helpers
// ---------------------------------------------------------------------------

/// Serializes tests that write into the process-wide sandbox home.
///
/// Most tests here use explicit temp paths, but a few have to go through the
/// real resolvers (that *is* what they are testing). Those all land in one
/// `sync_home_dir()` sandbox shared by the whole test process, so a `force`
/// sync in one test can rewrite the file another test is asserting on. Any
/// test that touches a resolver-owned path takes this lock.
pub(crate) static SANDBOX_HOME_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ---------------------------------------------------------------------------
// Temp-dir helper
// ---------------------------------------------------------------------------

/// A throwaway directory owned by one test. Every path here is explicit, so no
/// test in this module resolves `$HOME`.
pub(crate) struct TempDir {
    dir: std::path::PathBuf,
}

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ss-mcp-{tag}-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.dir
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}
