use crate::error::McpHubResult;
use crate::formats::{remove_server_from_path, sync_external_tools_off, write_server_to_path};
use crate::model::{AppliedAgentLedger, LibraryFile, McpServer, SyncWarning};
use crate::targets::{BuiltInTargetSpec, McpPathRoots, builtin_targets};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Synchronize all desired servers & disabled tool rules across every affected agent file
/// while strictly leaving untouched any agent that has neither desired servers nor prior ledger entries.
pub fn sync_library_with_roots(
    roots: &McpPathRoots,
    lib: &mut LibraryFile,
) -> McpHubResult<Vec<SyncWarning>> {
    let specs: BTreeMap<String, BuiltInTargetSpec> = builtin_targets(roots)
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();

    // Map agent_id -> list of active desired servers (skip paused).
    let mut desired_by_agent: BTreeMap<String, Vec<&McpServer>> = BTreeMap::new();
    for srv in &lib.servers {
        if srv.paused {
            continue;
        }
        for agent_id in &srv.agents {
            desired_by_agent
                .entry(agent_id.clone())
                .or_default()
                .push(srv);
        }
    }

    // Visit ONLY agents present in desired OR present in applied ledger!
    let mut impacted_agents: BTreeSet<String> = desired_by_agent.keys().cloned().collect();
    impacted_agents.extend(lib.applied.keys().cloned());

    let mut warnings = Vec::new();
    let mut next_applied: BTreeMap<String, AppliedAgentLedger> = BTreeMap::new();

    for agent_id in impacted_agents {
        let Some(spec) = specs.get(&agent_id) else {
            // Preserve unknown ledger entries instead of dropping blindly.
            if let Some(prev) = lib.applied.get(&agent_id) {
                next_applied.insert(agent_id, prev.clone());
            }
            continue;
        };

        let prev_ledger = lib.applied.get(&agent_id).cloned().unwrap_or_default();
        let desired_list = desired_by_agent
            .get(&agent_id)
            .cloned()
            .unwrap_or_default();
        let desired_names: BTreeSet<String> =
            desired_list.iter().map(|s| s.name.clone()).collect();

        let mut agent_paths: Vec<PathBuf> = vec![spec.primary_path.clone()];
        agent_paths.extend(spec.also_paths.iter().cloned());

        let mut retained_mcp: BTreeSet<String> = BTreeSet::new();
        let mut next_tools_off: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        // 1. Withdraw entries previously written by SkillStar that are no longer desired (deleted, unticked, or paused).
        for old_name in &prev_ledger.mcp {
            if desired_names.contains(old_name) {
                continue;
            }
            if withdraw_server_from_agent(spec, &agent_paths, old_name, &mut warnings) {
                retained_mcp.insert(old_name.clone());
            }
        }

        // 2. Write or update desired servers onto primary + also paths cleanly.
        for srv in &desired_list {
            let prev_srv_tools = prev_ledger
                .tools_off
                .get(&srv.name)
                .cloned()
                .unwrap_or_default();
            let mut primary_succeeded = false;
            let mut incompatible = false;
            for (p_idx, path) in agent_paths.iter().enumerate() {
                match write_server_to_path(spec, path, srv, &prev_srv_tools) {
                    Ok(outcome) => {
                        if let Some(w) = outcome.warning {
                            let code = w.code.clone();
                            warnings.push(w);
                            if p_idx == 0 && (code == "unsupported_remote" || code == "unsupported_sse") {
                                incompatible = true;
                                break;
                            }
                        } else if p_idx == 0 {
                            primary_succeeded = true;
                            if !outcome.applied_tools_off.is_empty() {
                                next_tools_off.insert(srv.name.clone(), outcome.applied_tools_off);
                            }
                        }
                    }
                    Err(err) => {
                        warnings.push(SyncWarning {
                            agent_id: agent_id.clone(),
                            path: path.display().to_string(),
                            code: "write_failed".to_string(),
                            message: err.to_string(),
                        });
                    }
                }
            }
            if incompatible {
                // If this server was previously written to this agent (e.g. stdio -> http/sse),
                // withdraw the stale entry from disk so it never becomes an unmanaged ghost.
                if prev_ledger.mcp.contains(&srv.name)
                    && withdraw_server_from_agent(spec, &agent_paths, &srv.name, &mut warnings)
                {
                    retained_mcp.insert(srv.name.clone());
                }
            } else if primary_succeeded {
                retained_mcp.insert(srv.name.clone());
            }
        }

        // 3. External tool-disable sync (`claude` permissions.deny / OpenCode-family `tools.*`).
        if matches!(spec.id.as_str(), "claude" | "opencode" | "deveco" | "mimocode") {
            let mut desired_external: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for srv in &desired_list {
                if !srv.disabled_tools.is_empty() && retained_mcp.contains(&srv.name) {
                    desired_external.insert(srv.name.clone(), srv.disabled_tools.clone());
                }
            }
            match sync_external_tools_off(spec, &desired_external, &prev_ledger.tools_off) {
                Ok(updated_ext_tools) => {
                    for (srv, set) in updated_ext_tools {
                        next_tools_off.insert(srv, set);
                    }
                }
                Err(err) => warnings.push(SyncWarning {
                    agent_id: agent_id.clone(),
                    path: spec
                        .claude_settings_path
                        .as_ref()
                        .unwrap_or(&spec.primary_path)
                        .display()
                        .to_string(),
                    code: "tools_off_sync_failed".to_string(),
                    message: err.to_string(),
                }),
            }
        }

        if !retained_mcp.is_empty() || !next_tools_off.is_empty() {
            next_applied.insert(
                agent_id,
                AppliedAgentLedger {
                    mcp: retained_mcp,
                    tools_off: next_tools_off,
                },
            );
        }
    }

    lib.applied = next_applied;
    Ok(warnings)
}

fn withdraw_server_from_agent(
    spec: &crate::targets::BuiltInTargetSpec,
    agent_paths: &[PathBuf],
    server_name: &str,
    warnings: &mut Vec<SyncWarning>,
) -> bool {
    let mut still_present = false;
    for path in agent_paths {
        match remove_server_from_path(spec, path, server_name) {
            Ok(outcome) => {
                if let Some(w) = outcome.warning {
                    if w.code == "foreign_zed_extension" {
                        still_present = true;
                    }
                    warnings.push(w);
                }
            }
            Err(err) => {
                still_present = true;
                warnings.push(SyncWarning {
                    agent_id: spec.id.clone(),
                    path: path.display().to_string(),
                    code: "remove_failed".to_string(),
                    message: err.to_string(),
                });
            }
        }
    }
    still_present
}
