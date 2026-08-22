//! The preset chips: curated marketplace rows merged over the built-in catalog.
//!
//! Two cross-domain jobs live here. The mapping (§C.1 of the audit) turns one
//! curated [`McpRegistryServer`] into an [`McpPreset`]; the orchestration around
//! it decides which rows qualify and folds them over
//! [`skillstar_models::mcp::get_mcp_presets`]. Both used to sit in the Tauri
//! command adapter, which `docs/boundaries.md` says holds no domain logic.
//!
//! The two sources are complementary, never alternatives: the built-in catalog
//! is the floor that survives a missing or corrupt snapshot DB, so a failed
//! curated read degrades to "no curated additions", never to "no presets".

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use skillstar_marketplace::McpRegistryServer;
use skillstar_models::mcp::{McpPreset, get_mcp_presets, merge_mcp_presets};
use tracing::warn;

use super::draft::registry_to_entry;

/// Every preset chip the UI offers, curated rows first.
///
/// Reads the local snapshot; no network. See [`list_mcp_presets_with`] for the
/// rule this applies to whatever the read returns.
pub fn list_mcp_presets() -> Vec<McpPreset> {
    list_mcp_presets_with(load_curated_servers)
}

/// [`list_mcp_presets`] over an injected curated-row read.
///
/// Only rows the marketplace explicitly flags `recommended` become chips — the
/// full curated catalog stays behind the marketplace browser. A read that fails
/// is logged and treated as an empty promotion list, which is what keeps the
/// built-in catalog reaching the UI when the snapshot DB is gone or unreadable.
pub fn list_mcp_presets_with(
    load_curated: impl FnOnce() -> Result<Vec<McpRegistryServer>>,
) -> Vec<McpPreset> {
    let curated = match load_curated() {
        Ok(servers) => servers
            .iter()
            .filter(|server| server.recommended)
            .map(curated_server_to_preset)
            .collect(),
        Err(err) => {
            warn!(target: "mcp", error = %err, "curated MCP presets unavailable; serving the built-in catalog only");
            Vec::new()
        }
    };
    merge_mcp_presets(curated, get_mcp_presets())
}

/// Open the snapshot DB and read its curated MCP rows.
///
/// The snapshot *runtime* (db path, data root, installed-skill loaders) is host
/// bootstrap and stays with the host — `src-tauri/src/core/marketplace_snapshot`
/// for the GUI, `cli::mod` for the CLI. This only opens what they configured.
fn load_curated_servers() -> Result<Vec<McpRegistryServer>> {
    skillstar_marketplace::snapshot::initialize()
        .context("initialize marketplace snapshot for MCP presets")?;
    skillstar_marketplace::mcp_snapshot::list_curated_mcp_servers()
        .context("load curated MCP rows from the marketplace DB")
}

/// Map one curated registry row into a preset chip.
pub fn curated_server_to_preset(server: &McpRegistryServer) -> McpPreset {
    let draft = registry_to_entry(server);

    // Union across packages rather than only the selected one: the chip lists
    // what the user will need to supply, and a preset is offered before any
    // runtime shape has been chosen.
    let mut required_env = BTreeSet::new();
    for package in &server.packages {
        for key in &package.required_env {
            required_env.insert(key.clone());
        }
    }

    let mut tags = draft.tags;
    if server.recommended && !tags.iter().any(|tag| tag == "recommended") {
        tags.push("recommended".to_string());
    }
    if let Some(source) = &server.source
        && !tags.iter().any(|tag| tag == source)
    {
        tags.push(source.clone());
    }

    McpPreset {
        id: server.id.clone(),
        name: draft.name,
        description: server.description.clone(),
        homepage: server.repo_url.clone(),
        transport: draft.transport,
        command: draft.command,
        args: draft.args,
        env: draft.env,
        url: draft.url,
        headers: draft.headers,
        tags,
        required_env: required_env.into_iter().collect(),
        // A curated preset's id *is* its catalog row id, so the chip can hand
        // this straight to the install wizard.
        catalog_id: Some(server.id.clone()),
    }
}
