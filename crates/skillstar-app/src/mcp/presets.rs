//! Curated marketplace row → `McpPreset` chip.
//!
//! The second cross-domain mapping the audit (§C.1) found in the command layer.
//! It is a thin wrapper over [`super::draft::registry_to_entry`]: a preset is a
//! draft plus the card metadata the chip shows and the `required_env` list the
//! form highlights.

use std::collections::BTreeSet;

use skillstar_marketplace::McpRegistryServer;
use skillstar_models::mcp::McpPreset;

use super::draft::registry_to_entry;

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
    }
}
