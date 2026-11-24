use crate::error::McpHubResult;
use crate::formats::{read_entries_from_target, remove_server_from_path};
use crate::model::{FoundServer, LibraryFile, McpServer, SyncWarning};
use crate::targets::{McpPathRoots, builtin_targets, resolve_target_spec};
use std::collections::BTreeSet;

/// Scan installed/existing agent configuration files for unmanaged MCP servers,
/// filtering out entries owned by SkillStar's ledger and client-bundled internals (e.g. Codex runtimes).
pub fn discover_found_servers(
    roots: &McpPathRoots,
    lib: &LibraryFile,
) -> (Vec<FoundServer>, Vec<SyncWarning>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_paths: BTreeSet<std::path::PathBuf> = BTreeSet::new();

    for spec in builtin_targets(roots) {
        if !spec.primary_path.exists() && !spec.also_paths.iter().any(|p| p.exists()) {
            continue;
        }
        if !seen_paths.insert(spec.primary_path.clone()) && spec.also_paths.is_empty() {
            // Multiple agents sharing the exact same file path report unmanaged servers once on the first canonical owner.
            continue;
        }
        let ledger = lib.applied.get(&spec.id);
        let desired_for_agent: BTreeSet<&str> = lib
            .servers
            .iter()
            .filter(|s| !s.paused && s.agents.iter().any(|a| a == &spec.id))
            .map(|s| s.name.as_str())
            .collect();

        let paths = std::iter::once(&spec.primary_path).chain(spec.also_paths.iter());
        let mut seen_names_for_agent: BTreeSet<String> = BTreeSet::new();

        for path in paths {
            if !path.exists() {
                continue;
            }
            match read_entries_from_target(&spec, path) {
                Ok(entries) => {
                    for srv in entries {
                        if is_app_owned_internal(&spec.id, &srv) {
                            continue;
                        }
                        if ledger.is_some_and(|l| l.mcp.contains(&srv.name))
                            || desired_for_agent.contains(srv.name.as_str())
                        {
                            continue;
                        }
                        if seen_names_for_agent.insert(srv.name.clone()) {
                            found.push(FoundServer {
                                agent_id: spec.id.clone(),
                                agent_name: spec.name.clone(),
                                config_path: path.to_string_lossy().to_string(),
                                format: spec.format,
                                server: srv,
                            });
                        }
                    }
                }
                Err(err) => warnings.push(SyncWarning {
                    agent_id: spec.id.clone(),
                    path: path.display().to_string(),
                    code: "discover_parse_failed".to_string(),
                    message: err.to_string(),
                }),
            }
        }
    }

    (found, warnings)
}

/// Remove an unmanaged MCP entry from an agent's config files with rolling backup protection.
pub fn remove_unmanaged_server(
    roots: &McpPathRoots,
    agent_id: &str,
    server_name: &str,
) -> McpHubResult<Vec<SyncWarning>> {
    let spec = resolve_target_spec(roots, agent_id)?;
    let mut warnings = Vec::new();
    let mut paths = vec![spec.primary_path.clone()];
    paths.extend(spec.also_paths.iter().cloned());
    for p in paths {
        if !p.exists() {
            continue;
        }
        let outcome = remove_server_from_path(&spec, &p, server_name)?;
        if let Some(w) = outcome.warning {
            warnings.push(w);
        }
    }
    Ok(warnings)
}

/// Filter out runtime servers bundled internally by agents (such as Codex CUA node/runtime bridges).
fn is_app_owned_internal(agent_id: &str, srv: &McpServer) -> bool {
    if agent_id == "codex" {
        let cmd = srv.command.replace('\\', "/");
        if cmd.contains("/cua_node/") || cmd.contains("/openai/codex/runtimes/") {
            return true;
        }
        if srv
            .args
            .iter()
            .any(|a| {
                let norm = a.replace('\\', "/");
                norm.contains("/cua_node/") || norm.contains("/openai/codex/runtimes/")
            })
        {
            return true;
        }
    }
    false
}
