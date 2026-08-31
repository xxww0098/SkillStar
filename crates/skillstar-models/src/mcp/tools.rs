//! Per-tool config paths, installed detection, and live config readers/writers.

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

use crate::tool_sync::{
    create_rolling_backup, resolve_opencode_config_path, resolve_zcode_config_path,
    sync_config_dir, sync_home_dir,
};

/// ZCode desktop loads MCP from `~/.zcode/cli/config.json` (`mcp.servers`), not `v2/config.json`.
pub fn resolve_zcode_cli_mcp_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".zcode").join("cli").join("config.json"))
}

use super::*;

/// `~/.claude.json` — where Claude Code reads user-scope MCP servers.
pub fn resolve_claude_json_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".claude.json"))
}

/// Claude Desktop Chat's config file:
/// - macOS `~/Library/Application Support/Claude/claude_desktop_config.json`
/// - Windows `%APPDATA%\Claude\claude_desktop_config.json`
/// - Linux `~/.config/Claude/claude_desktop_config.json`
///
/// Anchored on [`sync_config_dir`] rather than [`sync_home_dir`] because those
/// three paths are exactly what an OS config dir resolves to — hand-rolling the
/// per-OS split here would be a second, divergent copy of that logic.
/// `sync_config_dir` funnels through the same `SKILLSTAR_TOOL_SYNC_HOME`
/// sandbox check as `sync_home_dir`, so tests never touch the real one.
pub(crate) fn resolve_legacy_claude_desktop_config_path() -> Result<PathBuf> {
    Ok(sync_config_dir()?
        .join("Claude")
        .join("claude_desktop_config.json"))
}

/// Public Claude Desktop Chat target (`mcpServers.<name>`).
///
/// Same file as [`resolve_legacy_claude_desktop_config_path`] by design: the
/// legacy id removes an old projection, the public
/// [`CLAUDE_DESKTOP_CHAT_TOOL_ID`] maintains a live one. See
/// [`LEGACY_CLAUDE_DESKTOP_TOOL_ID`] for how the two are kept from fighting.
pub fn resolve_claude_desktop_chat_config_path() -> Result<PathBuf> {
    resolve_legacy_claude_desktop_config_path()
}

/// Claude Desktop Chat install probe (registry `installed` column).
///
/// The config *directory* is what the app creates on first launch, so it is
/// the reliable signal; the desktop-app probe additionally catches an install
/// that has never been run. `home` is unused because this target's config lives
/// under the OS config dir, not the home dir.
pub(crate) fn installed_claude_desktop_chat(_home: &Path) -> bool {
    sync_config_dir()
        .map(|dir| dir.join("Claude").exists())
        .unwrap_or(false)
        || skillstar_core::infra::path_env::desktop_app_installed("Claude")
}

/// Legacy Gemini CLI MCP config (`~/.gemini/settings.json`). Cleanup-only —
/// Gemini is no longer a public MCP target.
pub(crate) fn resolve_legacy_gemini_settings_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".gemini").join("settings.json"))
}

/// `~/.grok/config.toml`
pub fn resolve_grok_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".grok").join("config.toml"))
}

/// `~/.kiro/settings/mcp.json` — Kiro's user-scope MCP servers (top-level `mcpServers`).
pub fn resolve_kiro_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".kiro").join("settings").join("mcp.json"))
}

/// `~/.cursor/mcp.json` — Cursor's user-scope MCP servers (top-level `mcpServers`).
pub fn resolve_cursor_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".cursor").join("mcp.json"))
}

/// `~/.copilot/mcp-config.json` — the home-anchored, portable VS Code / Copilot
/// MCP config (top-level **`servers`**, not `mcpServers`).
///
/// VS Code reads MCP servers from several places; this is the only one that is
/// user-scope and platform-independent, which is what a projection target needs.
/// The workspace `.vscode/mcp.json` and the per-profile file behind
/// `MCP: Open User Configuration` are deliberately not written: the first is
/// per-project (SkillStar has no project context here) and the second lives
/// under a profile directory whose location SkillStar has no verified resolver
/// for.
pub fn resolve_vscode_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".copilot").join("mcp-config.json"))
}

/// `~/.codeium/windsurf/mcp_config.json` — Windsurf's MCP config
/// (top-level `mcpServers`, remote endpoints under `serverUrl`).
pub fn resolve_windsurf_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home
        .join(".codeium")
        .join("windsurf")
        .join("mcp_config.json"))
}

/// `~/.cline/mcp.json` — Cline CLI's MCP config (top-level `mcpServers`).
///
/// The VS Code extension keeps its own `cline_mcp_settings.json` under the
/// editor's globalStorage; that path is extension-id and profile dependent, so
/// only the CLI config is targeted.
pub fn resolve_cline_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".cline").join("mcp.json"))
}

/// `~/.gemini/settings.json` — Gemini CLI's settings file (top-level
/// `mcpServers`).
///
/// Same file as [`resolve_legacy_gemini_settings_path`] by design: the legacy
/// id removes an old projection, the public `gemini-cli` id maintains a live
/// one. See [`LEGACY_GEMINI_TOOL_ID`] for how the two are kept from fighting.
pub fn resolve_gemini_cli_config_path() -> Result<PathBuf> {
    resolve_legacy_gemini_settings_path()
}

/// `~/.config/zed/settings.json` — Zed's settings file (top-level
/// **`context_servers`**).
///
/// Zed uses `~/.config` on every platform, so this is a home-relative join
/// rather than an OS config-dir lookup.
pub fn resolve_zed_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".config").join("zed").join("settings.json"))
}

/// Released Maka Desktop / CLI MCP config:
/// - macOS `~/Library/Application Support/Maka/workspaces/default/mcp.json`
/// - Windows `%APPDATA%\Maka\workspaces\default\mcp.json`
/// - Linux `~/.config/Maka/workspaces/default/mcp.json`
///
/// Anchored on [`sync_config_dir`] rather than [`sync_home_dir`] because those
/// three paths are exactly what an OS config dir resolves to. The development
/// isolation profile `Maka Dev` is intentionally not written.
pub fn resolve_maka_config_path() -> Result<PathBuf> {
    Ok(sync_config_dir()?
        .join("Maka")
        .join("workspaces")
        .join("default")
        .join("mcp.json"))
}

/// Maka install probe (registry `installed` column).
///
/// Skills live under `~/.maka`; MCP lives under the OS config dir's `Maka`
/// profile. Either, plus the CLI binary or the Desktop app, is enough.
pub(crate) fn installed_maka(home: &Path) -> bool {
    home.join(".maka").exists()
        || sync_config_dir()
            .map(|dir| dir.join("Maka").exists())
            .unwrap_or(false)
        || skillstar_core::infra::path_env::binary_on_enriched_path("maka")
        || skillstar_core::infra::path_env::desktop_app_installed("Maka")
}

/// Resolve the live MCP config file for a tool.
///
/// Registry-driven; hidden legacy ids (`claude-desktop`, `gemini`) resolve
/// separately so old projections stay cleanable without becoming public targets.
pub fn resolve_mcp_config_path(tool_id: &str) -> Result<PathBuf> {
    if tool_id == LEGACY_CLAUDE_DESKTOP_TOOL_ID {
        return resolve_legacy_claude_desktop_config_path();
    }
    if tool_id == LEGACY_GEMINI_TOOL_ID {
        return resolve_legacy_gemini_settings_path();
    }
    match mcp_tool_spec(tool_id) {
        Some(spec) => (spec.resolve_config_path)(),
        None => bail!("Unsupported tool '{tool_id}'"),
    }
}

/// Best-effort "is this tool installed?" probe used to skip pointless writes.
///
/// Registry-driven; unknown ids report not-installed.
pub fn tool_installed(tool_id: &str) -> bool {
    let Ok(home) = sync_home_dir() else {
        return false;
    };
    match mcp_tool_spec(tool_id) {
        Some(spec) => (spec.installed)(&home),
        None => false,
    }
}

/// Claude Code install probe (registry `installed` column): any of binary /
/// desktop app / config dir / user-scope MCP config counts.
pub(crate) fn installed_claude_code(home: &Path) -> bool {
    claude_code_installed_from_signals(
        skillstar_core::infra::path_env::binary_on_enriched_path("claude"),
        skillstar_core::infra::path_env::desktop_app_installed("Claude"),
        home.join(".claude").exists(),
        home.join(".claude.json").exists(),
    )
}

/// OpenCode install probe (registry `installed` column): config dir or an
/// existing opencode.json.
pub(crate) fn installed_opencode(home: &Path) -> bool {
    home.join(".config").join("opencode").exists()
        || resolve_opencode_config_path()
            .map(|p| p.exists())
            .unwrap_or(false)
}

pub(crate) fn claude_code_installed_from_signals(
    binary_found: bool,
    desktop_code_found: bool,
    config_dir_found: bool,
    mcp_config_found: bool,
) -> bool {
    binary_found || desktop_code_found || config_dir_found || mcp_config_found
}

// ---------------------------------------------------------------------------
// Per-format live-server counters (registry `count_live` column)
// ---------------------------------------------------------------------------

/// Count entries in a top-level JSON map under `root_key`.
pub(crate) fn count_json_named_map(content: &str, root_key: &str) -> usize {
    serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|v| v.get(root_key).and_then(|m| m.as_object()).map(|m| m.len()))
        .unwrap_or(0)
}

/// Count entries in a top-level `mcpServers` JSON map (Claude Code, Kiro,
/// Cursor, Windsurf, Cline, Gemini CLI).
pub(crate) fn count_json_mcpservers(content: &str) -> usize {
    count_json_named_map(content, MCP_SERVERS_KEY)
}

/// Count entries in VS Code's top-level `servers` map.
pub(crate) fn count_vscode_servers(content: &str) -> usize {
    count_json_named_map(content, VSCODE_SERVERS_KEY)
}

/// Count entries in Zed's top-level `context_servers` map.
pub(crate) fn count_zed_context_servers(content: &str) -> usize {
    count_json_named_map(content, ZED_SERVERS_KEY)
}

/// Count entries in a TOML `mcp_servers` table (Codex, Grok).
pub(crate) fn count_toml_mcp_servers(content: &str) -> usize {
    toml::from_str::<toml::Table>(content)
        .ok()
        .and_then(|t| {
            t.get("mcp_servers")
                .and_then(|v| v.as_table())
                .map(|m| m.len())
        })
        .unwrap_or(0)
}

/// Count entries in an OpenCode-style `mcp` JSON map.
pub(crate) fn count_opencode_mcp(content: &str) -> usize {
    serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|v| v.get("mcp").and_then(|m| m.as_object()).map(|m| m.len()))
        .unwrap_or(0)
}

/// Count entries in a ZCode CLI `mcp.servers` JSON map.
pub(crate) fn count_zcode_cli_servers(content: &str) -> usize {
    serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|v| {
            v.get("mcp")
                .and_then(|m| m.get("servers"))
                .and_then(|s| s.as_object())
                .map(|m| m.len())
        })
        .unwrap_or(0)
}

/// Count MCP servers currently present in a tool's live config file.
fn count_live_servers(spec: &McpToolSpec) -> usize {
    let path = match (spec.resolve_config_path)() {
        Ok(p) => p,
        Err(_) => return 0,
    };
    if !path.exists() {
        return 0;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    (spec.count_live)(&content)
}

/// Status of every supported tool's MCP target.
pub fn tool_statuses() -> Vec<McpToolStatus> {
    mcp_tool_specs()
        .iter()
        .map(|spec| {
            let config_path = (spec.resolve_config_path)()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            McpToolStatus {
                tool_id: spec.id.to_string(),
                label: spec.label.to_string(),
                config_path,
                installed: tool_installed(spec.id),
                server_count: count_live_servers(spec),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Live config writers
// ---------------------------------------------------------------------------

pub(crate) fn backup_if_exists(path: &Path) -> Result<Option<PathBuf>> {
    if path.exists() {
        Ok(Some(create_rolling_backup(path)?))
    } else {
        Ok(None)
    }
}

/// Undo a failed write to `path` using the backup taken just before it.
///
/// The two cases are not symmetric, which is why this is one function rather
/// than a bare copy:
///
/// - `backup = Some(..)` — the file existed. Copy the backup back, restoring
///   the exact bytes the user had (including everything in the file that has
///   nothing to do with MCP).
/// - `backup = None` — there was no file. Anything at `path` now was created by
///   the attempt being undone, so removing it is what "back to before" means.
///   A file the writer never got as far as creating is already correct.
///
/// Returning `Ok(())` means the config is byte-for-byte back to its
/// pre-attempt state. An `Err` is the one case a caller must escalate: the
/// write failed *and* the undo failed, so the file may be half-written.
pub(crate) fn restore_from_backup(path: &Path, backup: Option<&Path>) -> Result<()> {
    match backup {
        Some(backup) => {
            std::fs::copy(backup, path).with_context(|| {
                format!(
                    "Failed to restore {} from its backup {}",
                    path.display(),
                    backup.display()
                )
            })?;
            Ok(())
        }
        None => {
            if !path.exists() {
                return Ok(());
            }
            std::fs::remove_file(path).with_context(|| {
                format!(
                    "Failed to remove the partially written {} left by a failed sync",
                    path.display()
                )
            })?;
            Ok(())
        }
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    Ok(())
}

/// Read a JSON config file as an object map — **fail-closed**.
///
/// A missing (or empty) file is legitimate: there is nothing to merge into, so
/// an empty map is returned and the caller writes a fresh config. Anything else
/// — unreadable bytes, invalid JSON, or a non-object root — is an error.
///
/// This distinction is the whole point of the helper. Every writer below
/// re-serializes the *entire* file, and these files carry far more than
/// SkillStar's MCP block (`~/.claude.json` holds most of Claude Code's user
/// settings). Falling back to an empty map on a parse failure would replace the
/// user's whole config with `{"mcpServers": …}` on the next toggle.
fn read_json_object_strict(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let content = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read {}. Refusing to rewrite it — check the file's permissions, then retry.",
            path.display()
        )
    })?;
    let content = content.trim_start_matches('\u{FEFF}');
    if content.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_str(content).with_context(|| {
        format!(
            "Invalid JSON in {}. Refusing to overwrite it — fix or move the file, then retry.",
            path.display()
        )
    })?;
    value.as_object().cloned().with_context(|| {
        format!(
            "Expected a JSON object at the root of {}. Refusing to overwrite the existing value.",
            path.display()
        )
    })
}

/// Read a TOML config file as a table — **fail-closed**, mirroring
/// [`read_json_object_strict`].
fn read_toml_table_strict(path: &Path) -> Result<toml::Table> {
    if !path.exists() {
        return Ok(toml::Table::new());
    }
    let content = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read {}. Refusing to rewrite it — check the file's permissions, then retry.",
            path.display()
        )
    })?;
    let content = content.trim_start_matches('\u{FEFF}');
    toml::from_str::<toml::Table>(content).with_context(|| {
        format!(
            "Invalid TOML in {}. Refusing to overwrite it — fix or move the file, then retry.",
            path.display()
        )
    })
}

/// Top-level key holding the server map in the community JSON format.
pub(crate) const MCP_SERVERS_KEY: &str = "mcpServers";
/// VS Code's top-level key — `servers`, not `mcpServers` (research §5.3 #11).
pub(crate) const VSCODE_SERVERS_KEY: &str = "servers";
/// Zed's top-level key — `context_servers` (research §5.3 #9).
pub(crate) const ZED_SERVERS_KEY: &str = "context_servers";

/// Upsert `<root_key>.<name>` in a JSON config file.
///
/// The root key is a parameter because the community format's `mcpServers` is
/// not universal: VS Code uses `servers` and Zed uses `context_servers`, and
/// writing the wrong one produces a syntactically valid file the client
/// silently ignores.
pub(crate) fn json_named_map_upsert(
    path: &Path,
    root_key: &str,
    name: &str,
    spec: Value,
) -> Result<()> {
    let mut root = read_json_object_strict(path)?;
    let servers = root
        .entry(root_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    // Only ever create the key when it is missing. A hand-written non-object
    // server map is refused rather than silently replaced — same rule as
    // `ensure_mcp_servers_map` applies to ZCode's `mcp` key.
    let map = servers.as_object_mut().with_context(|| {
        format!(
            "Expected `{root_key}` to be a JSON object in {}. Refusing to overwrite the existing value.",
            path.display()
        )
    })?;
    map.insert(name.to_string(), spec);
    write_json_pretty(path, &Value::Object(root))
}

/// Remove `<root_key>.<name>` from a JSON config file.
///
/// Fail-closed: a file that exists but cannot be read or parsed is left
/// byte-for-byte intact instead of being replaced by a freshly serialized `{}`.
pub(crate) fn json_named_map_remove(path: &Path, root_key: &str, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_object_strict(path)?;
    if let Some(servers) = root.get_mut(root_key) {
        let servers = servers.as_object_mut().with_context(|| {
            format!(
                "Expected `{root_key}` to be a JSON object in {}. Refusing to overwrite the existing value.",
                path.display()
            )
        })?;
        servers.remove(name);
    }
    write_json_pretty(path, &Value::Object(root))
}

/// Upsert `mcpServers.<name>` in a JSON config file.
pub(crate) fn json_mcpservers_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    json_named_map_upsert(path, MCP_SERVERS_KEY, name, spec)
}

/// Maka's current on-disk schema. v1 (missing `version`) is still readable
/// and is upgraded on write, matching Maka's own store.
const MAKA_MCP_CONFIG_VERSION: u64 = 2;

fn maka_version_is_supported(path: &Path, root: &Map<String, Value>) -> Result<()> {
    match root.get("version") {
        None => Ok(()),
        Some(Value::Number(n))
            if n.as_u64() == Some(1) || n.as_u64() == Some(MAKA_MCP_CONFIG_VERSION) =>
        {
            Ok(())
        }
        Some(other) => bail!(
            "Unsupported MCP config version {other} in {}. Refusing to overwrite it — this file uses a newer Maka schema than SkillStar can write.",
            path.display()
        ),
    }
}

/// Upsert `mcpServers.<name>` in Maka's `mcp.json`, keeping `version: 2`.
pub(crate) fn maka_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    let mut root = read_json_object_strict(path)?;
    maka_version_is_supported(path, &root)?;
    root.insert("version".into(), json!(MAKA_MCP_CONFIG_VERSION));
    let servers = root
        .entry(MCP_SERVERS_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let map = servers.as_object_mut().with_context(|| {
        format!(
            "Expected `{MCP_SERVERS_KEY}` to be a JSON object in {}. Refusing to overwrite the existing value.",
            path.display()
        )
    })?;
    map.insert(name.to_string(), spec);
    write_json_pretty(path, &Value::Object(root))
}

/// Remove `mcpServers.<name>` from Maka's `mcp.json` without touching `version`.
pub(crate) fn maka_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let root = read_json_object_strict(path)?;
    maka_version_is_supported(path, &root)?;
    json_named_map_remove(path, MCP_SERVERS_KEY, name)
}

/// Remove `mcpServers.<name>` from a JSON config file.
pub(crate) fn json_mcpservers_remove(path: &Path, name: &str) -> Result<()> {
    json_named_map_remove(path, MCP_SERVERS_KEY, name)
}

/// Legacy Desktop Chat cleanup entry point. Removal is now uniformly
/// fail-closed, so this is exactly [`json_mcpservers_remove`]; the name is kept
/// so the tombstone call sites still read as "never touch a broken file".
pub(crate) fn json_mcpservers_remove_strict(path: &Path, name: &str) -> Result<()> {
    json_mcpservers_remove(path, name)
}

fn write_json_pretty(path: &Path, value: &Value) -> Result<()> {
    ensure_parent(path)?;
    let out = serde_json::to_string_pretty(value).context("Failed to serialize JSON config")?;
    std::fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Upsert `mcp.<name>` in opencode.json (preserves `$schema`).
pub(crate) fn opencode_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    let mut root = read_json_object_strict(path)?;
    root.entry("$schema".to_string())
        .or_insert_with(|| json!("https://opencode.ai/config.json"));
    let mcp = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let map = mcp.as_object_mut().with_context(|| {
        format!(
            "Expected `mcp` to be a JSON object in {}. Refusing to overwrite the existing value.",
            path.display()
        )
    })?;
    map.insert(name.to_string(), spec);
    write_json_pretty(path, &Value::Object(root))
}

pub(crate) fn opencode_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_object_strict(path)?;
    if let Some(mcp) = root.get_mut("mcp") {
        let map = mcp.as_object_mut().with_context(|| {
            format!(
                "Expected `mcp` to be a JSON object in {}. Refusing to overwrite the existing value.",
                path.display()
            )
        })?;
        map.remove(name);
    }
    write_json_pretty(path, &Value::Object(root))
}

/// Upsert `[mcp_servers.<name>]` in Codex config.toml.
pub(crate) fn codex_upsert(path: &Path, name: &str, table: toml::Table) -> Result<()> {
    let mut root = read_toml_table_strict(path)?;
    let mcp_servers = root
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let map = mcp_servers.as_table_mut().with_context(|| {
        format!(
            "Expected `mcp_servers` to be a TOML table in {}. Refusing to overwrite the existing value.",
            path.display()
        )
    })?;
    map.insert(name.to_string(), toml::Value::Table(table));
    write_toml_pretty(path, &root)
}

pub(crate) fn codex_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_toml_table_strict(path)?;
    if let Some(mcp_servers) = root.get_mut("mcp_servers") {
        let map = mcp_servers.as_table_mut().with_context(|| {
            format!(
                "Expected `mcp_servers` to be a TOML table in {}. Refusing to overwrite the existing value.",
                path.display()
            )
        })?;
        map.remove(name);
        if map.is_empty() {
            root.remove("mcp_servers");
        }
    }
    write_toml_pretty(path, &root)
}
fn ensure_mcp_servers_map(root: &mut Map<String, Value>) -> Result<Map<String, Value>> {
    let mcp_val = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    // Only ever create the entry when it's missing. If the user wrote a
    // non-object `mcp` (e.g. `"mcp": true` or an array), refuse instead of
    // panicking mid-write and clobbering their hand-edited value.
    let kind = match mcp_val {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        _ => "scalar",
    };
    let mcp_obj = mcp_val
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("`mcp` field must be a JSON object, but existing value is {kind}; refusing to overwrite the user's hand-edited config"))?;
    mcp_obj
        .entry("servers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    Ok(mcp_obj
        .get("servers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default())
}

fn write_mcp_servers_map(root: &mut Map<String, Value>, servers: Map<String, Value>) {
    let mcp_val = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(mcp_obj) = mcp_val.as_object_mut() {
        mcp_obj.insert("servers".to_string(), Value::Object(servers));
    }
}

/// Upsert `mcp.servers.<name>` in `~/.zcode/cli/config.json`.
pub(crate) fn zcode_cli_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    let mut root = read_json_object_strict(path)?;
    let mut servers = ensure_mcp_servers_map(&mut root)?;
    servers.insert(name.to_string(), spec);
    write_mcp_servers_map(&mut root, servers);
    write_json_pretty(path, &Value::Object(root))
}

pub(crate) fn zcode_cli_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_object_strict(path)?;
    if let Some(mcp) = root.get_mut("mcp") {
        let mcp = mcp.as_object_mut().with_context(|| {
            format!(
                "Expected `mcp` to be a JSON object in {}. Refusing to overwrite the existing value.",
                path.display()
            )
        })?;
        if let Some(servers) = mcp.get_mut("servers") {
            let servers = servers.as_object_mut().with_context(|| {
                format!(
                    "Expected `mcp.servers` to be a JSON object in {}. Refusing to overwrite the existing value.",
                    path.display()
                )
            })?;
            servers.remove(name);
        }
    }
    write_json_pretty(path, &Value::Object(root))
}

/// Best-effort: drop a stale OpenCode-style entry from `~/.zcode/v2/config.json` `mcp`.
pub(crate) fn zcode_v2_opencode_mcp_remove(name: &str) -> Result<()> {
    let path = resolve_zcode_config_path()?;
    if !path.exists() {
        return Ok(());
    }
    opencode_remove(&path, name)
}

fn write_toml_pretty(path: &Path, table: &toml::Table) -> Result<()> {
    ensure_parent(path)?;
    let out = toml::to_string_pretty(table).context("Failed to serialize TOML config")?;
    std::fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}
