//! Projecting servers into / removing servers from each tool's live config.

use anyhow::{Result, bail};
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
    let path = resolve_mcp_config_path(tool_id)?;
    let backup = backup_if_exists(&path)?;
    match tool_id {
        "claude-code" | "gemini" => {
            json_mcpservers_upsert(&path, &entry.name, canonical_spec(entry))?
        }
        "claude-desktop" => {
            json_mcpservers_upsert(&path, &entry.name, claude_desktop_spec(entry)?)?
        }
        "opencode" => opencode_upsert(&path, &entry.name, opencode_spec(entry))?,
        "zcode" => {
            zcode_cli_upsert(&path, &entry.name, zcode_cli_spec(entry))?;
            let _ = zcode_v2_opencode_mcp_remove(&entry.name);
        }
        "codex" => codex_upsert(&path, &entry.name, codex_toml_table(entry))?,
        "grok" => codex_upsert(&path, &entry.name, grok_toml_table(entry))?,
        _ => bail!("Unsupported tool '{tool_id}'"),
    }
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
    match tool_id {
        "claude-code" | "claude-desktop" | "gemini" => json_mcpservers_remove(&path, name)?,
        "opencode" => opencode_remove(&path, name)?,
        "zcode" => {
            zcode_cli_remove(&path, name)?;
            let _ = zcode_v2_opencode_mcp_remove(name);
        }
        "codex" | "grok" => codex_remove(&path, name)?,
        _ => bail!("Unsupported tool '{tool_id}'"),
    }
    Ok(backup)
}

/// Project a server to all tools per its `enabled` map: enabled tools get an
/// upsert, disabled tools get a removal. Returns one result per tool touched.
pub fn sync_server_all_tools(entry: &McpServerEntry, force: bool) -> Vec<McpSyncResult> {
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

/// Re-project every server in the store to every tool (full reconciliation).
pub fn sync_all(store: &McpStore, force: bool) -> Vec<McpSyncResult> {
    store
        .servers
        .iter()
        .flat_map(|s| sync_server_all_tools(s, force))
        .collect()
}
