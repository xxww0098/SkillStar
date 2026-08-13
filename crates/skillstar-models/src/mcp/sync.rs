//! Projecting servers into / removing servers from each tool's live config.
//!
//! ## Failure semantics
//!
//! Every write here goes through [`guarded_write`], which makes a **single
//! tool** atomic: back up, write, and on failure put the file back exactly as
//! it was. A tool whose write failed is therefore never left half-updated.
//!
//! A **batch** across tools is deliberately *not* transactional. When tool 3
//! fails, tools 1 and 2 keep their (correct) new config rather than being
//! reverted for an unrelated tool's problem — reverting would take a working
//! config away from a tool that had nothing wrong with it. What callers get
//! instead is a full account: [`mcp_sync_consistency`] turns the result vector
//! into applied / rolled-back / drifted lists, so partial success is reported
//! rather than silently absorbed.
//!
//! **Delete is the exception.** `delete_server_and_sync` is all-or-nothing: if
//! any target fails to drop the server, the removals that already landed are
//! put back before the error propagates, so the store and the live configs
//! never disagree about whether a server exists.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

use super::*;

/// Outcome of one backed-up write to a tool's config file.
struct GuardedWrite {
    /// The rolling backup taken before the write, when there was a file to
    /// back up. Reported to the UI and used as the undo handle.
    backup: Option<PathBuf>,
    /// What the write itself returned.
    outcome: Result<()>,
    /// The config is back to its pre-attempt bytes (either restored, or the
    /// write never got far enough to change anything).
    rolled_back: bool,
    /// The undo failed — this file may be half-written.
    rollback_error: Option<String>,
}

impl GuardedWrite {
    /// A failure that happened before anything on disk could change.
    fn untouched(error: anyhow::Error) -> Self {
        Self {
            backup: None,
            outcome: Err(error),
            rolled_back: true,
            rollback_error: None,
        }
    }

    /// Fold this attempt into the caller's result shape.
    fn apply_to(self, result: &mut McpSyncResult) {
        result.backup_path = self
            .backup
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        match self.outcome {
            Ok(()) => result.success = true,
            Err(e) => {
                result.error = Some(e.to_string());
                result.rolled_back = self.rolled_back;
                result.rollback_error = self.rollback_error;
            }
        }
    }
}

/// Back up `path`, run `write`, and undo the write if it fails.
///
/// The undo is what makes a single tool atomic. Without it a writer that
/// failed partway through (or a `spec.upsert` that rejected the file *after*
/// creating its parent directory) would leave the user's config in a state
/// neither the store nor the user asked for, with the backup sitting unused
/// beside it — which is exactly what the audit flagged.
fn guarded_write(path: &Path, write: impl FnOnce(&Path) -> Result<()>) -> GuardedWrite {
    let backup = match backup_if_exists(path) {
        Ok(backup) => backup,
        // The backup failed, so nothing was written: the file is untouched.
        Err(e) => return GuardedWrite::untouched(e),
    };
    match write(path) {
        Ok(()) => GuardedWrite {
            backup,
            outcome: Ok(()),
            rolled_back: false,
            rollback_error: None,
        },
        Err(e) => {
            let (rolled_back, rollback_error) = match restore_from_backup(path, backup.as_deref()) {
                Ok(()) => (true, None),
                Err(restore_err) => (false, Some(restore_err.to_string())),
            };
            GuardedWrite {
                backup,
                outcome: Err(e),
                rolled_back,
                rollback_error,
            }
        }
    }
}

/// Project a single server into one tool's live config.
///
/// When `force` is false and the tool is not installed, the write is skipped
/// and a `skipped: true` result is returned. A failed write is rolled back —
/// see the module docs for what that does and does not guarantee.
pub fn sync_server_to_tool(entry: &McpServerEntry, tool_id: &str, force: bool) -> McpSyncResult {
    let mut result = McpSyncResult::pending(tool_id, &entry.id);
    result.config_path = resolve_mcp_config_path(tool_id)
        .ok()
        .map(|p| p.to_string_lossy().to_string());

    if !is_supported_tool(tool_id) {
        result.error = Some(format!("Unsupported public MCP target '{tool_id}'"));
        result.rolled_back = true;
        return result;
    }

    if !force && !tool_installed(tool_id) {
        result.success = true;
        result.skipped = true;
        return result;
    }

    sync_server_to_tool_inner(entry, tool_id).apply_to(&mut result);
    result
}

fn sync_server_to_tool_inner(entry: &McpServerEntry, tool_id: &str) -> GuardedWrite {
    // Validation and path resolution run before any IO, so a failure here
    // leaves the config file untouched by construction.
    if let Err(e) = validate_entry(entry) {
        return GuardedWrite::untouched(e);
    }
    let Some(spec) = mcp_tool_spec(tool_id) else {
        return GuardedWrite::untouched(anyhow::anyhow!("Unsupported tool '{tool_id}'"));
    };
    let path = match (spec.resolve_config_path)() {
        Ok(path) => path,
        Err(e) => return GuardedWrite::untouched(e),
    };
    guarded_write(&path, |path| (spec.upsert)(path, entry))
}

/// Remove a server (by name) from one tool's live config.
pub fn remove_server_from_tool(name: &str, tool_id: &str) -> McpSyncResult {
    let mut result = McpSyncResult::pending(tool_id, name);
    result.config_path = resolve_mcp_config_path(tool_id)
        .ok()
        .map(|p| p.to_string_lossy().to_string());
    remove_server_from_tool_inner(name, tool_id).apply_to(&mut result);
    result
}

fn remove_server_from_tool_inner(name: &str, tool_id: &str) -> GuardedWrite {
    let path = match resolve_mcp_config_path(tool_id) {
        Ok(path) => path,
        Err(e) => return GuardedWrite::untouched(e),
    };
    if !path.exists() {
        return GuardedWrite {
            backup: None,
            outcome: Ok(()),
            rolled_back: false,
            rollback_error: None,
        };
    }
    // Hidden legacy projections are not registry rows; they only support the
    // removal used by cleanup tombstones.
    if tool_id == LEGACY_CLAUDE_DESKTOP_TOOL_ID {
        return guarded_write(&path, |path| json_mcpservers_remove_strict(path, name));
    }
    if tool_id == LEGACY_GEMINI_TOOL_ID {
        return guarded_write(&path, |path| json_mcpservers_remove(path, name));
    }
    let Some(spec) = mcp_tool_spec(tool_id) else {
        return GuardedWrite::untouched(anyhow::anyhow!("Unsupported tool '{tool_id}'"));
    };
    guarded_write(&path, |path| (spec.remove)(path, name))
}

/// Project a server to every public Agent target. Entries created by older
/// SkillStar releases may additionally carry hidden legacy keys (`claude-desktop`,
/// `gemini`); those cleanup tombstones remove the old projection instead of
/// updating it.
pub fn sync_server_all_tools(entry: &mut McpServerEntry, force: bool) -> Vec<McpSyncResult> {
    let mut results = sync_server_public_tools(entry, force);

    if let Some(cleanup) = cleanup_legacy_desktop_chat(entry) {
        results.push(cleanup);
    }
    if let Some(cleanup) = cleanup_legacy_gemini(entry) {
        results.push(cleanup);
    }
    results
}

/// Project a server only to the public Agent targets.
///
/// Each tool is written independently and atomically; the batch is not. Pass
/// the returned vector to [`mcp_sync_consistency`] to learn whether every tool
/// actually received the projection.
pub fn sync_server_public_tools(entry: &McpServerEntry, force: bool) -> Vec<McpSyncResult> {
    MCP_TOOL_IDS
        .iter()
        .map(|&tool_id| {
            let enabled = entry.enabled.get(tool_id).copied().unwrap_or(false);
            if enabled {
                sync_server_to_tool(entry, tool_id, force)
            } else {
                remove_server_from_tool(&entry.name, tool_id)
            }
        })
        .collect()
}

/// Remove a server name from every public Agent target.
pub fn remove_server_from_public_tools(name: &str) -> Vec<McpSyncResult> {
    MCP_TOOL_IDS
        .iter()
        .map(|&tool_id| remove_server_from_tool(name, tool_id))
        .collect()
}

/// Remove the old Desktop Chat projection when this entry carries migration
/// evidence. Returning `None` for new entries keeps the tombstone fully out of
/// the way of entries that never had one.
///
/// **Subsumption.** The public [`CLAUDE_DESKTOP_CHAT_TOOL_ID`] target writes
/// the very same `claude_desktop_config.json` → `mcpServers.<name>` key this
/// tombstone deletes. When an entry carries both, deleting would erase the
/// projection the public target legitimately owns — and because
/// [`sync_server_all_tools`] runs the public pass first, it would erase a key
/// written moments earlier in the same call. So the tombstone is consumed
/// without touching the file: its job (stop SkillStar from leaving an
/// unmanaged entry behind) is already done by the live target that now
/// maintains it.
pub fn cleanup_legacy_desktop_chat(entry: &mut McpServerEntry) -> Option<McpSyncResult> {
    if entry.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID) != Some(&true) {
        return None;
    }
    if entry.enabled.get(CLAUDE_DESKTOP_CHAT_TOOL_ID) == Some(&true) {
        mark_legacy_desktop_chat_clean(entry);
        return Some(subsumed_tombstone(
            LEGACY_CLAUDE_DESKTOP_TOOL_ID,
            &entry.name,
        ));
    }
    let result = remove_server_from_tool(&entry.name, LEGACY_CLAUDE_DESKTOP_TOOL_ID);
    if result.success {
        mark_legacy_desktop_chat_clean(entry);
    }
    Some(result)
}

/// A cleanup tombstone consumed without touching the file, because a public
/// target now owns the same config key. Reported as a deliberate no-op
/// (`success` + `skipped`) so batch consistency does not read it as a failure.
fn subsumed_tombstone(tool_id: &str, name: &str) -> McpSyncResult {
    let mut subsumed = McpSyncResult::pending(tool_id, name);
    subsumed.config_path = resolve_mcp_config_path(tool_id)
        .ok()
        .map(|p| p.to_string_lossy().to_string());
    subsumed.success = true;
    subsumed.skipped = true;
    subsumed
}

/// Consume a successful legacy cleanup tombstone. The false value preserves
/// old-store round-trip semantics without authorizing future deletions.
pub fn mark_legacy_desktop_chat_clean(entry: &mut McpServerEntry) {
    if entry.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID) == Some(&true) {
        entry
            .enabled
            .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), false);
    }
}

/// Remove the old Gemini CLI projection when this entry still has `enabled.gemini`.
///
/// **Subsumption.** The public `gemini-cli` target writes the very same
/// `~/.gemini/settings.json` → `mcpServers.<name>` key this tombstone deletes.
/// When an entry carries both, deleting would erase the projection the public
/// target legitimately owns — and because `sync_server_all_tools` runs the
/// public pass first, it would erase a key written moments earlier in the same
/// call. So the tombstone is consumed without touching the file: its job (stop
/// SkillStar from leaving an unmanaged entry behind) is already done by the
/// live target that now maintains it.
pub fn cleanup_legacy_gemini(entry: &mut McpServerEntry) -> Option<McpSyncResult> {
    if entry.enabled.get(LEGACY_GEMINI_TOOL_ID) != Some(&true) {
        return None;
    }
    if entry.enabled.get(GEMINI_CLI_TOOL_ID) == Some(&true) {
        mark_legacy_gemini_clean(entry);
        return Some(subsumed_tombstone(LEGACY_GEMINI_TOOL_ID, &entry.name));
    }
    let result = remove_server_from_tool(&entry.name, LEGACY_GEMINI_TOOL_ID);
    if result.success {
        mark_legacy_gemini_clean(entry);
    }
    Some(result)
}

/// Consume a successful Gemini cleanup tombstone.
pub fn mark_legacy_gemini_clean(entry: &mut McpServerEntry) {
    if entry.enabled.get(LEGACY_GEMINI_TOOL_ID) == Some(&true) {
        entry.enabled.insert(LEGACY_GEMINI_TOOL_ID.into(), false);
    }
}

fn cleanup_legacy_desktop_chat_or_bail(
    entry: &mut McpServerEntry,
) -> Result<Option<McpSyncResult>> {
    let result = cleanup_legacy_desktop_chat(entry);
    if let Some(cleanup) = &result
        && !cleanup.success
    {
        bail!(
            "Failed to clean legacy Claude Desktop Chat MCP '{}': {}",
            entry.name,
            cleanup.error.as_deref().unwrap_or("unknown error")
        );
    }
    Ok(result)
}

fn cleanup_legacy_gemini_or_bail(entry: &mut McpServerEntry) -> Result<Option<McpSyncResult>> {
    let result = cleanup_legacy_gemini(entry);
    if let Some(cleanup) = &result
        && !cleanup.success
    {
        bail!(
            "Failed to clean legacy Gemini CLI MCP '{}': {}",
            entry.name,
            cleanup.error.as_deref().unwrap_or("unknown error")
        );
    }
    Ok(result)
}

fn ensure_cleanup_succeeded(results: &[McpSyncResult], action: &str) -> Result<()> {
    if let Some(failed) = results
        .iter()
        .find(|result| !result.success && !result.skipped)
    {
        bail!(
            "{action} failed for '{}': {}",
            failed.tool_id,
            failed.error.as_deref().unwrap_or("unknown error")
        );
    }
    Ok(())
}

/// Put back every removal in `results` that actually landed.
///
/// Used when a multi-tool removal fails partway: without this, the tools that
/// were already cleaned stay cleaned while the store keeps the server (delete)
/// or its old name (rename), and the two disagree with no way to tell.
/// Each restored entry flips to `success: false` + `rolled_back: true` so the
/// returned vector describes the machine's real end state rather than the
/// intent that was abandoned.
fn undo_successful_removals(results: &mut [McpSyncResult]) {
    for result in results.iter_mut() {
        if !result.success || result.skipped {
            continue;
        }
        let Some(config_path) = result.config_path.clone() else {
            continue;
        };
        let backup = result.backup_path.clone();
        match restore_from_backup(Path::new(&config_path), backup.as_deref().map(Path::new)) {
            Ok(()) => {
                result.success = false;
                result.rolled_back = true;
            }
            Err(e) => {
                result.success = false;
                result.rolled_back = false;
                result.rollback_error = Some(e.to_string());
            }
        }
    }
}

/// Remove a name from every public target, undoing the whole batch if any
/// target fails. Returns the (possibly rolled-back) results plus the verdict.
fn remove_from_public_tools_atomically(
    name: &str,
    action: &str,
) -> (Vec<McpSyncResult>, Result<()>) {
    let mut results = remove_server_from_public_tools(name);
    match ensure_cleanup_succeeded(&results, action) {
        Ok(()) => (results, Ok(())),
        Err(e) => {
            undo_successful_removals(&mut results);
            (results, Err(e))
        }
    }
}

/// Apply an edit and reconcile all public targets plus any pending legacy
/// cleanup. A rename cannot commit if cleaning the old Desktop Chat name fails.
pub fn update_server_and_sync(
    store: &mut McpStore,
    id: &str,
    patch: McpServerPatch,
    force: bool,
) -> Result<(McpServerEntry, Vec<McpSyncResult>)> {
    let mut next = store.clone();
    let mut old_entry = next
        .servers
        .iter()
        .find(|server| server.id == id)
        .cloned()
        .with_context(|| format!("MCP server '{id}' not found"))?;
    let preview = update_server(&mut next, id, patch)?;
    let renamed = old_entry.name != preview.name;
    let legacy_desktop_rename = if renamed {
        cleanup_legacy_desktop_chat_or_bail(&mut old_entry)?
    } else {
        None
    };
    let legacy_gemini_rename = if renamed {
        cleanup_legacy_gemini_or_bail(&mut old_entry)?
    } else {
        None
    };

    if legacy_desktop_rename.is_some() || legacy_gemini_rename.is_some() {
        let stored = next
            .servers
            .iter_mut()
            .find(|server| server.id == id)
            .with_context(|| format!("MCP server '{id}' not found"))?;
        if legacy_desktop_rename.is_some() {
            mark_legacy_desktop_chat_clean(stored);
        }
        if legacy_gemini_rename.is_some() {
            mark_legacy_gemini_clean(stored);
        }
    }

    let mut results = legacy_desktop_rename
        .into_iter()
        .chain(legacy_gemini_rename)
        .collect::<Vec<_>>();
    if renamed {
        // A half-applied rename is the worst outcome available: the store
        // would hold the new name while some tools still carry the old key,
        // with nothing left pointing at the orphan. Undo and refuse instead.
        let (old_name_cleanup, verdict) =
            remove_from_public_tools_atomically(&old_entry.name, "Removing renamed MCP server");
        results.extend(old_name_cleanup);
        verdict?;
    }
    let updated = {
        let stored = next
            .servers
            .iter_mut()
            .find(|server| server.id == id)
            .with_context(|| format!("MCP server '{id}' not found"))?;
        results.extend(sync_server_all_tools(stored, force));
        stored.clone()
    };
    *store = next;
    Ok((updated, results))
}

/// Delete an entry only after any pending legacy cleanup succeeds.
pub fn delete_server_and_sync(
    store: &mut McpStore,
    id: &str,
) -> Result<(McpServerEntry, Vec<McpSyncResult>)> {
    let mut next = store.clone();
    let mut existing = next
        .servers
        .iter()
        .find(|server| server.id == id)
        .cloned()
        .with_context(|| format!("MCP server '{id}' not found"))?;
    let legacy_desktop = cleanup_legacy_desktop_chat_or_bail(&mut existing)?;
    let legacy_gemini = cleanup_legacy_gemini_or_bail(&mut existing)?;
    let removed = delete_server(&mut next, id)?;
    // All-or-nothing: the store only drops the server once every target has
    // dropped it too. A partial removal is undone so the machine never ends up
    // with a server the store no longer knows how to clean up.
    let (mut results, verdict) =
        remove_from_public_tools_atomically(&removed.name, "Removing deleted MCP server");
    results.extend(legacy_desktop);
    results.extend(legacy_gemini);
    verdict?;
    *store = next;
    Ok((removed, results))
}

/// Toggle one public target while consuming any pending legacy cleanup first.
pub fn set_tool_enabled_and_sync(
    store: &mut McpStore,
    id: &str,
    tool_id: &str,
    enabled: bool,
    force: bool,
) -> Result<McpSyncResult> {
    let mut next = store.clone();
    set_tool_enabled(&mut next, id, tool_id, enabled)?;
    let entry = next
        .servers
        .iter_mut()
        .find(|server| server.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    cleanup_legacy_desktop_chat_or_bail(entry)?;
    cleanup_legacy_gemini_or_bail(entry)?;
    let result = if enabled {
        sync_server_to_tool(entry, tool_id, force)
    } else {
        remove_server_from_tool(&entry.name, tool_id)
    };
    *store = next;
    Ok(result)
}

/// Reconcile one stored entry by id, consuming a successful cleanup tombstone.
pub fn sync_server_by_id(
    store: &mut McpStore,
    id: &str,
    force: bool,
) -> Result<Vec<McpSyncResult>> {
    let entry = store
        .servers
        .iter_mut()
        .find(|server| server.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    Ok(sync_server_all_tools(entry, force))
}

/// Re-project every server in the store to every tool (full reconciliation).
pub fn sync_all(store: &mut McpStore, force: bool) -> Vec<McpSyncResult> {
    store
        .servers
        .iter_mut()
        .flat_map(|s| sync_server_all_tools(s, force))
        .collect()
}
