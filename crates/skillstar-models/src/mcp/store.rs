//! Unified MCP store: path, read/write IO, validation, and pure CRUD on `McpStore`.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::tool_sync::create_rolling_backup;

use super::*;

// ---------------------------------------------------------------------------
// Store path + IO
// ---------------------------------------------------------------------------

/// `~/.skillstar/config/mcp_servers.json`
pub fn mcp_store_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("mcp_servers.json")
}

/// Suffix of the quarantine copy taken when the store cannot be parsed.
const CORRUPT_SUFFIX: &str = ".corrupt.";

/// Read the store — **fail-closed**.
///
/// A missing file is a legitimate empty store. A file that exists but cannot be
/// read or parsed is *never* downgraded to an empty store: [`write_mcp_store`]
/// replaces the whole file, so an empty default here would erase every server
/// the user owns on the next click. Unparseable content is copied aside as
/// `mcp_servers.json.corrupt.<epoch_ms>` and the error is propagated so the UI
/// can show it.
pub fn read_mcp_store(path: &Path) -> Result<McpStore> {
    if !path.exists() {
        return Ok(McpStore::default());
    }
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read the MCP store {}. Refusing to continue with an empty store — check the file's permissions, then retry.",
            path.display()
        )
    })?;
    let text = text.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<McpStore>(text) {
        Ok(store) => apply_store_version_gate(store, path),
        Err(e) => {
            let saved = quarantine_corrupt_store(path)
                .inspect_err(|copy_err| {
                    tracing::warn!(
                        "Failed to quarantine the malformed MCP store {}: {copy_err}",
                        path.display()
                    );
                })
                .ok();
            match saved {
                Some(copy) => bail!(
                    "The MCP store {} is not valid JSON ({e}). Your original file was copied to {} — repair or restore it, then retry. SkillStar will not continue with an empty store, so nothing is overwritten.",
                    path.display(),
                    copy.display()
                ),
                None => bail!(
                    "The MCP store {} is not valid JSON ({e}). Repair or move the file, then retry. SkillStar will not continue with an empty store, so nothing is overwritten.",
                    path.display()
                ),
            }
        }
    }
}

/// Enforce [`MCP_STORE_VERSION`] on a store that parsed cleanly.
///
/// Parsing is not enough to prove a file is safe to work with. `serde` will
/// happily read a file written by a *newer* SkillStar, silently dropping every
/// field this build does not know about — and the next
/// [`write_mcp_store`] would then persist that lossy copy over the user's real
/// data. So the gate runs in both directions:
///
/// - **newer than this build → refuse.** Fail closed, exactly like a malformed
///   file: better a clear error telling the user to upgrade than an
///   irreversible downgrade of their store.
/// - **older than this build → upgrade and persist immediately.** Writing the
///   upgraded file back at read time (rather than waiting for the user's next
///   edit) means the on-disk version never lags the running code, so a later
///   downgrade hits the refuse branch above instead of a version that lies.
fn apply_store_version_gate(mut store: McpStore, path: &Path) -> Result<McpStore> {
    if store.version > MCP_STORE_VERSION {
        bail!(
            "The MCP store {} was written by a newer version of SkillStar (schema v{}, this build reads v{MCP_STORE_VERSION}). Refusing to open it — an older build would silently drop the newer fields on the next save. Update SkillStar, or move the file aside to start fresh.",
            path.display(),
            store.version
        );
    }
    if store.version < MCP_STORE_VERSION {
        let from = store.version;
        store.version = MCP_STORE_VERSION;
        write_mcp_store(&store, path).with_context(|| {
            format!(
                "Failed to persist the MCP store {} after upgrading it from schema v{from} to v{MCP_STORE_VERSION}",
                path.display()
            )
        })?;
        tracing::info!(
            "Upgraded the MCP store {} from schema v{from} to v{MCP_STORE_VERSION}",
            path.display()
        );
    }
    Ok(store)
}

/// Copy an unparseable store aside as `<file>.corrupt.<epoch_ms>` so the data
/// stays recoverable by hand.
///
/// Repeated reads of the same broken file reuse the existing copy — every MCP
/// screen visit calls this path, and one snapshot per visit would bury the user
/// in identical files.
fn quarantine_corrupt_store(path: &Path) -> Result<PathBuf> {
    let original = std::fs::read(path)
        .with_context(|| format!("Failed to re-read {} for quarantine", path.display()))?;
    if let Some(existing) = existing_corrupt_copy(path, &original) {
        return Ok(existing);
    }
    let copy = PathBuf::from(format!(
        "{}{CORRUPT_SUFFIX}{}",
        path.to_string_lossy(),
        now_ms()
    ));
    std::fs::write(&copy, &original)
        .with_context(|| format!("Failed to write {}", copy.display()))?;
    Ok(copy)
}

/// Find an already-quarantined sibling holding exactly the same bytes.
fn existing_corrupt_copy(path: &Path, original: &[u8]) -> Option<PathBuf> {
    let parent = path.parent()?;
    let prefix = format!("{}{CORRUPT_SUFFIX}", path.file_name()?.to_str()?);
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(&prefix) {
            continue;
        }
        if std::fs::read(entry.path()).is_ok_and(|bytes| bytes == original) {
            return Some(entry.path());
        }
    }
    None
}

/// Write the store atomically (temp file + rename), backing up any file it
/// replaces.
///
/// The rolling backup is the second half of the fail-closed contract in
/// [`read_mcp_store`]: a write that turns out to be wrong (a bad migration, a
/// schema change) stays recoverable from `mcp_servers.json.bak.<epoch_ms>`.
pub fn write_mcp_store(store: &McpStore, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    if path.exists() {
        create_rolling_backup(path).with_context(|| {
            format!(
                "Failed to back up the MCP store {} before overwriting it",
                path.display()
            )
        })?;
    }
    let json = serde_json::to_string_pretty(store).context("Failed to serialize McpStore")?;
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json.as_bytes())
        .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate an entry's transport-specific required fields.
///
/// This is the *invariant* half of validation and runs on every sync, so it
/// deliberately accepts anything an older SkillStar was willing to store.
/// New input goes through [`validate_entry_input`], which adds the stricter
/// name / url / env / header rules — see `validate.rs` for why the two are
/// kept apart.
pub fn validate_entry(entry: &McpServerEntry) -> Result<()> {
    if entry.name.trim().is_empty() {
        bail!("MCP server name must not be empty");
    }
    match entry.transport.as_str() {
        "stdio" => {
            if entry
                .command
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                bail!("stdio MCP server '{}' requires a command", entry.name);
            }
        }
        "http" | "sse" => {
            if entry.url.as_deref().map(str::trim).unwrap_or("").is_empty() {
                bail!(
                    "{} MCP server '{}' requires a url",
                    entry.transport,
                    entry.name
                );
            }
        }
        other => bail!("Unknown MCP transport '{other}' (expected stdio|http|sse)"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Store CRUD (pure — operate on &mut McpStore)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Store CRUD (pure — operate on &mut McpStore)
// ---------------------------------------------------------------------------

/// Create a new server: assigns a fresh UUID, timestamps, and sort index.
pub fn create_server(store: &mut McpStore, mut entry: McpServerEntry) -> Result<McpServerEntry> {
    validate_entry_input(&entry)?;
    if store.servers.iter().any(|s| s.name == entry.name) {
        bail!("An MCP server named '{}' already exists", entry.name);
    }
    entry.id = Uuid::new_v4().to_string();
    let now = now_ms();
    entry.created_at = Some(now);
    entry.updated_at = Some(now);
    entry.sort_index = store
        .servers
        .iter()
        .map(|s| s.sort_index)
        .max()
        .map_or(0, |m| m + 1);
    // Drop enable flags for unknown tools.
    entry.enabled.retain(|k, _| is_supported_tool(k));
    store.servers.push(entry.clone());
    Ok(entry)
}

/// Apply a partial patch to an existing server.
pub fn update_server(
    store: &mut McpStore,
    id: &str,
    patch: McpServerPatch,
) -> Result<McpServerEntry> {
    // Guard against renaming onto another server's name.
    if let Some(new_name) = patch.name.as_ref()
        && store
            .servers
            .iter()
            .any(|s| s.id != id && &s.name == new_name)
    {
        bail!("An MCP server named '{}' already exists", new_name);
    }
    let server = store
        .servers
        .iter_mut()
        .find(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    let previous_name = server.name.clone();

    if let Some(v) = patch.name {
        server.name = v;
    }
    if let Some(v) = patch.transport {
        server.transport = v;
    }
    if let Some(v) = patch.command {
        server.command = Some(v);
    }
    if let Some(v) = patch.args {
        server.args = v;
    }
    if let Some(v) = patch.env {
        server.env = v;
    }
    if let Some(v) = patch.cwd {
        server.cwd = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.url {
        server.url = Some(v);
    }
    if let Some(v) = patch.headers {
        server.headers = v;
    }
    if let Some(v) = patch.description {
        server.description = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.homepage {
        server.homepage = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.tags {
        server.tags = v;
    }
    if let Some(v) = patch.auto_approve_all {
        server.auto_approve_all = v;
    }
    if let Some(v) = patch.auto_approve_tools {
        server.auto_approve_tools = v;
    }
    if let Some(v) = patch.disabled_tools {
        server.disabled_tools = v;
    }
    if let Some(v) = patch.timeout_ms {
        server.timeout_ms = v;
    }
    server.updated_at = Some(now_ms());

    let updated = server.clone();
    // Editing is an input path, so the strict policy applies to the result —
    // except for a name the edit did not touch, which stays grandfathered
    // (see `validate_entry_edit`).
    validate_entry_edit(&updated, Some(&previous_name))?;
    Ok(updated)
}

/// Remove a server from the store. Returns the removed entry.
pub fn delete_server(store: &mut McpStore, id: &str) -> Result<McpServerEntry> {
    let idx = store
        .servers
        .iter()
        .position(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    Ok(store.servers.remove(idx))
}

/// Set the enabled flag for a server on a tool. Returns the updated entry.
pub fn set_tool_enabled(
    store: &mut McpStore,
    id: &str,
    tool_id: &str,
    enabled: bool,
) -> Result<McpServerEntry> {
    if !is_supported_tool(tool_id) {
        bail!("Unsupported tool '{tool_id}'");
    }
    let server = store
        .servers
        .iter_mut()
        .find(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    server.enabled.insert(tool_id.to_string(), enabled);
    server.updated_at = Some(now_ms());
    Ok(server.clone())
}
