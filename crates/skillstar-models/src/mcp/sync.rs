//! Projecting servers into / removing servers from each tool's live config.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

use super::*;

/// Project a single server into one tool's live config.
///
/// When `force` is false and the tool is not installed, the write is skipped
/// and a `skipped: true` result is returned.
pub fn sync_server_to_tool(entry: &McpServerEntry, tool_id: &str, force: bool) -> McpSyncResult {
    let mut result = McpSyncResult {
        tool_id: tool_id.to_string(),
        server_id: entry.id.clone(),
        success: false,
        skipped: false,
        config_path: resolve_mcp_config_path(tool_id)
            .ok()
            .map(|p| p.to_string_lossy().to_string()),
        backup_path: None,
        error: None,
    };

    if !is_supported_tool(tool_id) {
        result.error = Some(format!("Unsupported public MCP target '{tool_id}'"));
        return result;
    }

    if !force && !tool_installed(tool_id) {
        result.success = true;
        result.skipped = true;
        return result;
    }

    match sync_server_to_tool_inner(entry, tool_id) {
        Ok(backup) => {
            result.success = true;
            result.backup_path = backup.map(|p| p.to_string_lossy().to_string());
        }
        Err(e) => result.error = Some(e.to_string()),
    }
    result
}

fn sync_server_to_tool_inner(entry: &McpServerEntry, tool_id: &str) -> Result<Option<PathBuf>> {
    validate_entry(entry)?;
    let spec =
        mcp_tool_spec(tool_id).ok_or_else(|| anyhow::anyhow!("Unsupported tool '{tool_id}'"))?;
    let path = (spec.resolve_config_path)()?;
    let backup = backup_if_exists(&path)?;
    (spec.upsert)(&path, entry)?;
    Ok(backup)
}

/// Remove a server (by name) from one tool's live config.
pub fn remove_server_from_tool(name: &str, tool_id: &str) -> McpSyncResult {
    let mut result = McpSyncResult {
        tool_id: tool_id.to_string(),
        server_id: name.to_string(),
        success: false,
        skipped: false,
        config_path: resolve_mcp_config_path(tool_id)
            .ok()
            .map(|p| p.to_string_lossy().to_string()),
        backup_path: None,
        error: None,
    };
    match remove_server_from_tool_inner(name, tool_id) {
        Ok(backup) => {
            result.success = true;
            result.backup_path = backup.map(|p| p.to_string_lossy().to_string());
        }
        Err(e) => result.error = Some(e.to_string()),
    }
    result
}

fn remove_server_from_tool_inner(name: &str, tool_id: &str) -> Result<Option<PathBuf>> {
    let path = resolve_mcp_config_path(tool_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let backup = backup_if_exists(&path)?;
    // Hidden legacy projections are not registry rows; they only support the
    // removal used by cleanup tombstones.
    if tool_id == LEGACY_CLAUDE_DESKTOP_TOOL_ID {
        json_mcpservers_remove_strict(&path, name)?;
        return Ok(backup);
    }
    if tool_id == LEGACY_GEMINI_TOOL_ID {
        json_mcpservers_remove(&path, name)?;
        return Ok(backup);
    }
    let spec =
        mcp_tool_spec(tool_id).ok_or_else(|| anyhow::anyhow!("Unsupported tool '{tool_id}'"))?;
    (spec.remove)(&path, name)?;
    Ok(backup)
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
/// evidence. Returning `None` for new entries keeps the separate product fully
/// outside the public Claude Code flow.
pub fn cleanup_legacy_desktop_chat(entry: &mut McpServerEntry) -> Option<McpSyncResult> {
    if entry.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID) != Some(&true) {
        return None;
    }
    let result = remove_server_from_tool(&entry.name, LEGACY_CLAUDE_DESKTOP_TOOL_ID);
    if result.success {
        mark_legacy_desktop_chat_clean(entry);
    }
    Some(result)
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
pub fn cleanup_legacy_gemini(entry: &mut McpServerEntry) -> Option<McpSyncResult> {
    if entry.enabled.get(LEGACY_GEMINI_TOOL_ID) != Some(&true) {
        return None;
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
        let old_name_cleanup = remove_server_from_public_tools(&old_entry.name);
        ensure_cleanup_succeeded(&old_name_cleanup, "Removing renamed MCP server")?;
        results.extend(old_name_cleanup);
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
    let mut results = remove_server_from_public_tools(&removed.name);
    ensure_cleanup_succeeded(&results, "Removing deleted MCP server")?;
    results.extend(legacy_desktop);
    results.extend(legacy_gemini);
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
