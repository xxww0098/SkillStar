pub mod discover;
pub mod sync;

use crate::env_refs::{normalize_server, validate_server};
use crate::error::{McpHubError, McpHubResult};
use crate::model::{
    ConflictStrategy, FoundServer, LibraryDelta, LibraryFile, LibrarySnapshot, McpServer,
};
use crate::targets::{McpPathRoots, canonical_agent_id, detect_agent_targets};
use chrono::Utc;
use ss_core::infra::paths::{mcp_data_dir, mcp_library_path, mcp_lock_path};
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

static IN_PROCESS_LOCK: Mutex<()> = Mutex::new(());

pub struct LibraryLockGuard {
    _mem: MutexGuard<'static, ()>,
    _file: File,
}

/// Acquire both in-process serialization and cross-process advisory lock at `runtime/locks/mcp/library.lock`.
pub fn acquire_library_lock() -> McpHubResult<LibraryLockGuard> {
    let mem = IN_PROCESS_LOCK
        .lock()
        .map_err(|_| McpHubError::Lock("in-process MCP library mutex poisoned".into()))?;
    let lock_path = mcp_lock_path();
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    file.lock()
        .map_err(|err| McpHubError::Lock(format!("failed locking {}: {err}", lock_path.display())))?;
    Ok(LibraryLockGuard { _mem: mem, _file: file })
}

/// Read `data/mcp/library.json` without creating missing directories or files.
pub fn read_library_unlocked() -> McpHubResult<LibraryFile> {
    let (lib, _) = read_library_unlocked_inner()?;
    Ok(lib)
}

fn read_library_unlocked_inner() -> McpHubResult<(LibraryFile, bool)> {
    let path = mcp_library_path();
    match fs::read_to_string(&path) {
        Ok(raw) => {
            if raw.trim().is_empty() {
                return Ok((
                    LibraryFile {
                        version: 1,
                        ..Default::default()
                    },
                    false,
                ));
            }
            let mut lib: LibraryFile =
                serde_json::from_str(&raw).map_err(|err| McpHubError::ConfigParse {
                    path: path.display().to_string(),
                    detail: err.to_string(),
                })?;
            let changed = canonicalize_library_agents(&mut lib);
            Ok((lib, changed))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok((
            LibraryFile {
                version: 1,
                ..Default::default()
            },
            false,
        )),
        Err(err) => Err(McpHubError::Io(err)),
    }
}

fn is_removed_mcp_agent(id: &str) -> bool {
    id.eq_ignore_ascii_case("gemini") || id.eq_ignore_ascii_case("gemini-cli")
}

fn canonicalize_library_agents(lib: &mut LibraryFile) -> bool {
    let mut changed = false;
    for srv in &mut lib.servers {
        let mut next: Vec<String> = srv
            .agents
            .iter()
            .map(|id| canonical_agent_id(id).to_string())
            .filter(|id| !is_removed_mcp_agent(id))
            .collect();
        next.sort();
        next.dedup();
        if next != srv.agents {
            srv.agents = next;
            changed = true;
        }
    }
    // Migrate legacy `agy` applied key into `antigravity` in-memory if present.
    if let Some(legacy) = lib.applied.remove("agy") {
        changed = true;
        let entry = lib.applied.entry("antigravity".to_string()).or_default();
        entry.mcp.extend(legacy.mcp);
        for (srv, tools) in legacy.tools_off {
            entry.tools_off.entry(srv).or_default().extend(tools);
        }
    }
    // Purge retired Gemini CLI ledger entries (D-122) without touching ~/.gemini/settings.json.
    if lib.applied.remove("gemini").is_some() {
        changed = true;
    }
    if lib.applied.remove("gemini-cli").is_some() {
        changed = true;
    }
    changed
}

/// Persist `library.json` atomically inside `data/mcp/`.
pub fn write_library_unlocked(lib: &LibraryFile) -> McpHubResult<()> {
    let dir = mcp_data_dir();
    fs::create_dir_all(&dir)?;
    let path = mcp_library_path();
    let encoded = serde_json::to_string_pretty(lib).map_err(|err| McpHubError::ConfigEdit {
        path: path.display().to_string(),
        detail: err.to_string(),
    })?;
    write_atomic_json(&path, encoded.as_bytes())
}

fn write_atomic_json(path: &Path, bytes: &[u8]) -> McpHubResult<()> {
    let parent = path.parent().ok_or_else(|| McpHubError::ConfigEdit {
        path: path.display().to_string(),
        detail: "missing parent dir".into(),
    })?;
    let tmp = parent.join(format!(".library.{}.tmp", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Return current library servers, live 35 agent states, and discovered unmanaged servers (read-only).
pub fn get_snapshot() -> McpHubResult<LibrarySnapshot> {
    let _guard = acquire_library_lock()?;
    let (lib, changed) = read_library_unlocked_inner()?;
    if changed {
        write_library_unlocked(&lib)?;
    }
    let roots = McpPathRoots::from_env();
    let agents = detect_agent_targets(&roots);
    let (found, warnings) = discover::discover_found_servers(&roots, &lib);
    Ok(LibrarySnapshot {
        servers: lib.servers,
        agents,
        found,
        warnings,
    })
}

fn finish_delta(roots: &McpPathRoots, mut lib: LibraryFile) -> McpHubResult<LibraryDelta> {
    let warnings = sync::sync_library_with_roots(roots, &mut lib)?;
    write_library_unlocked(&lib)?;
    Ok(LibraryDelta {
        servers: lib.servers,
        warnings,
    })
}

fn finish_snapshot(roots: &McpPathRoots, mut lib: LibraryFile) -> McpHubResult<LibrarySnapshot> {
    let warnings = sync::sync_library_with_roots(roots, &mut lib)?;
    write_library_unlocked(&lib)?;
    let agents = detect_agent_targets(roots);
    let (found, mut disc_warnings) = discover::discover_found_servers(roots, &lib);
    let mut all_warnings = warnings;
    all_warnings.append(&mut disc_warnings);
    Ok(LibrarySnapshot {
        servers: lib.servers,
        agents,
        found,
        warnings: all_warnings,
    })
}

fn prepare_server_for_upsert(mut server: McpServer, now: &str) -> McpHubResult<McpServer> {
    server.agents = server
        .agents
        .iter()
        .map(|id| canonical_agent_id(id).to_string())
        .collect();
    server = normalize_server(server);
    validate_server(&server)?;
    server.updated_at = Some(now.to_string());
    Ok(server)
}

fn upsert_server_in_lib(
    lib: &mut LibraryFile,
    mut server: McpServer,
    original_name: Option<&str>,
    conflict: ConflictStrategy,
) -> McpHubResult<()> {
    let edit_idx = original_name
        .and_then(|orig| lib.servers.iter().position(|s| s.name == orig));
    let colliding_idx = lib
        .servers
        .iter()
        .position(|s| s.name == server.name);

    match (edit_idx, colliding_idx) {
        (Some(idx), Some(col_idx)) if idx != col_idx => match conflict {
            ConflictStrategy::Replace => {
                lib.servers[col_idx] = server;
                lib.servers.remove(idx);
            }
            ConflictStrategy::Keep => {
                return Err(McpHubError::AlreadyExists(server.name));
            }
            ConflictStrategy::Rename => {
                server.name = pick_unique_name(&lib.servers, &server.name);
                lib.servers[idx] = server;
            }
        },
        (Some(idx), _) => {
            lib.servers[idx] = server;
        }
        (None, Some(col_idx)) => match conflict {
            ConflictStrategy::Replace => {
                lib.servers[col_idx] = server;
            }
            ConflictStrategy::Keep => {
                return Err(McpHubError::AlreadyExists(server.name));
            }
            ConflictStrategy::Rename => {
                server.name = pick_unique_name(&lib.servers, &server.name);
                lib.servers.push(server);
            }
        },
        (None, None) => {
            lib.servers.push(server);
        }
    }
    Ok(())
}

fn validate_agent_ids(roots: &McpPathRoots, agents: &[String]) -> McpHubResult<()> {
    for id in agents {
        let _ = crate::targets::resolve_target_spec(roots, id)?;
    }
    Ok(())
}

fn save_server_core(
    server: McpServer,
    original_name: Option<&str>,
    conflict: ConflictStrategy,
) -> McpHubResult<(McpPathRoots, LibraryFile)> {
    let now = Utc::now().to_rfc3339();
    let server = prepare_server_for_upsert(server, &now)?;
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();
    validate_agent_ids(&roots, &server.agents)?;
    upsert_server_in_lib(&mut lib, server, original_name, conflict)?;
    Ok((roots, lib))
}

/// Save a new server or update an existing server by name (or rename via `original_name`),
/// then losslessly sync impacted agent configuration files.
pub fn save_server(
    server: McpServer,
    original_name: Option<&str>,
    conflict: ConflictStrategy,
) -> McpHubResult<LibrarySnapshot> {
    let (roots, lib) = save_server_core(server, original_name, conflict)?;
    finish_snapshot(&roots, lib)
}

/// Save a new or updated server and return only `{ servers, warnings }`.
pub fn save_server_delta(
    server: McpServer,
    original_name: Option<&str>,
    conflict: ConflictStrategy,
) -> McpHubResult<LibraryDelta> {
    let (roots, lib) = save_server_core(server, original_name, conflict)?;
    finish_delta(&roots, lib)
}

/// Save multiple servers in one lock acquisition and one agent-config reconciliation pass.
pub fn save_servers_batch(
    servers: Vec<McpServer>,
    conflict: ConflictStrategy,
) -> McpHubResult<LibraryDelta> {
    let now = Utc::now().to_rfc3339();
    let mut prepared = Vec::with_capacity(servers.len());
    for srv in servers {
        prepared.push(prepare_server_for_upsert(srv, &now)?);
    }
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();
    for srv in &prepared {
        validate_agent_ids(&roots, &srv.agents)?;
    }
    for srv in prepared {
        upsert_server_in_lib(&mut lib, srv, None, conflict)?;
    }
    finish_delta(&roots, lib)
}

fn toggle_server_agent_core(
    server_name: &str,
    agent_id: &str,
    enabled: bool,
) -> McpHubResult<(McpPathRoots, LibraryFile)> {
    let canon = canonical_agent_id(agent_id).to_string();
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();
    let _ = crate::targets::resolve_target_spec(&roots, &canon)?;

    let srv = lib
        .servers
        .iter_mut()
        .find(|s| s.name == server_name)
        .ok_or_else(|| McpHubError::NotFound(server_name.to_string()))?;

    if enabled {
        if !srv.agents.contains(&canon) {
            srv.agents.push(canon);
            srv.agents.sort();
        }
    } else {
        srv.agents.retain(|a| a != &canon);
    }
    srv.updated_at = Some(Utc::now().to_string());
    Ok((roots, lib))
}

/// Toggle one agent on or off for a managed server and sync immediately.
pub fn toggle_server_agent(
    server_name: &str,
    agent_id: &str,
    enabled: bool,
) -> McpHubResult<LibrarySnapshot> {
    let (roots, lib) = toggle_server_agent_core(server_name, agent_id, enabled)?;
    finish_snapshot(&roots, lib)
}

/// Toggle one agent on or off and return only `{ servers, warnings }`.
pub fn toggle_server_agent_delta(
    server_name: &str,
    agent_id: &str,
    enabled: bool,
) -> McpHubResult<LibraryDelta> {
    let (roots, lib) = toggle_server_agent_core(server_name, agent_id, enabled)?;
    finish_delta(&roots, lib)
}

fn set_server_agents_core(
    server_name: &str,
    agent_ids: Vec<String>,
) -> McpHubResult<(McpPathRoots, LibraryFile)> {
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();

    let mut clean_agents = Vec::new();
    for raw in agent_ids {
        let canon = canonical_agent_id(&raw).to_string();
        let _ = crate::targets::resolve_target_spec(&roots, &canon)?;
        if !clean_agents.contains(&canon) {
            clean_agents.push(canon);
        }
    }
    clean_agents.sort();

    let srv = lib
        .servers
        .iter_mut()
        .find(|s| s.name == server_name)
        .ok_or_else(|| McpHubError::NotFound(server_name.to_string()))?;
    srv.agents = clean_agents;
    srv.updated_at = Some(Utc::now().to_string());
    Ok((roots, lib))
}

/// Replace the agent target set for one server and return only `{ servers, warnings }`.
pub fn set_server_agents_delta(
    server_name: &str,
    agent_ids: Vec<String>,
) -> McpHubResult<LibraryDelta> {
    let (roots, lib) = set_server_agents_core(server_name, agent_ids)?;
    finish_delta(&roots, lib)
}

fn set_server_paused_core(
    server_names: &[String],
    paused: bool,
) -> McpHubResult<(McpPathRoots, LibraryFile)> {
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();
    let now = Utc::now().to_string();

    for server_name in server_names {
        let srv = lib
            .servers
            .iter_mut()
            .find(|s| s.name == *server_name)
            .ok_or_else(|| McpHubError::NotFound(server_name.clone()))?;
        srv.paused = paused;
        srv.updated_at = Some(now.clone());
    }
    Ok((roots, lib))
}

/// Pause or resume a server cleanly (withdrawing/restoring only ledger-owned agent entries).
pub fn set_server_paused(server_name: &str, paused: bool) -> McpHubResult<LibrarySnapshot> {
    let (roots, lib) = set_server_paused_core(&[server_name.to_string()], paused)?;
    finish_snapshot(&roots, lib)
}

/// Pause or resume one server and return only `{ servers, warnings }`.
pub fn set_server_paused_delta(server_name: &str, paused: bool) -> McpHubResult<LibraryDelta> {
    let (roots, lib) = set_server_paused_core(&[server_name.to_string()], paused)?;
    finish_delta(&roots, lib)
}

/// Pause or resume multiple servers in one lock acquisition and one reconciliation pass.
pub fn batch_set_paused(server_names: &[String], paused: bool) -> McpHubResult<LibraryDelta> {
    let (roots, lib) = set_server_paused_core(server_names, paused)?;
    finish_delta(&roots, lib)
}

fn remove_servers_core(server_names: &[String]) -> McpHubResult<(McpPathRoots, LibraryFile)> {
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    for server_name in server_names {
        let before = lib.servers.len();
        lib.servers.retain(|s| s.name != *server_name);
        if lib.servers.len() == before {
            return Err(McpHubError::NotFound(server_name.clone()));
        }
    }
    let roots = McpPathRoots::from_env();
    Ok((roots, lib))
}

/// Delete a server from the library and withdraw only the ledger-tracked entries written by SkillStar.
pub fn remove_server(server_name: &str) -> McpHubResult<LibrarySnapshot> {
    let (roots, lib) = remove_servers_core(&[server_name.to_string()])?;
    finish_snapshot(&roots, lib)
}

/// Delete one server and return only `{ servers, warnings }`.
pub fn remove_server_delta(server_name: &str) -> McpHubResult<LibraryDelta> {
    let (roots, lib) = remove_servers_core(&[server_name.to_string()])?;
    finish_delta(&roots, lib)
}

/// Delete multiple servers in one lock acquisition and one reconciliation pass.
pub fn batch_remove_servers(server_names: &[String]) -> McpHubResult<LibraryDelta> {
    let (roots, lib) = remove_servers_core(server_names)?;
    finish_delta(&roots, lib)
}

/// Re-run full lossless reconciliation of all library servers against every affected agent file.
pub fn sync_all() -> McpHubResult<LibrarySnapshot> {
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();
    let warnings = sync::sync_library_with_roots(&roots, &mut lib)?;
    write_library_unlocked(&lib)?;
    let agents = detect_agent_targets(&roots);
    let (found, mut disc_warnings) = discover::discover_found_servers(&roots, &lib);
    let mut all_warnings = warnings;
    all_warnings.append(&mut disc_warnings);
    Ok(LibrarySnapshot {
        servers: lib.servers,
        agents,
        found,
        warnings: all_warnings,
    })
}

/// Adopt an unmanaged server from an agent config into `library.json` (recording ledger ownership),
/// with optional conflict strategy when a same-named server already exists.
pub fn import_found_server(
    agent_id: &str,
    server_name: &str,
    conflict: ConflictStrategy,
) -> McpHubResult<LibrarySnapshot> {
    let canon = canonical_agent_id(agent_id).to_string();
    let _guard = acquire_library_lock()?;
    let mut lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();
    let (found_list, _) = discover::discover_found_servers(&roots, &lib);
    let target: FoundServer = found_list
        .into_iter()
        .find(|f| f.agent_id == canon && f.server.name == server_name)
        .ok_or_else(|| McpHubError::NotFound(format!("{canon}:{server_name}")))?;

    let mut candidate = target.server;
    candidate.agents = vec![canon.clone()];
    candidate.updated_at = Some(Utc::now().to_string());

    if let Some(existing_idx) = lib.servers.iter().position(|s| s.name == candidate.name) {
        match conflict {
            ConflictStrategy::Keep => {
                // Keep library definition, but enroll this agent into it!
                if !lib.servers[existing_idx].agents.contains(&canon) {
                    lib.servers[existing_idx].agents.push(canon.clone());
                    lib.servers[existing_idx].agents.sort();
                }
            }
            ConflictStrategy::Replace => {
                let mut merged_agents = lib.servers[existing_idx].agents.clone();
                if !merged_agents.contains(&canon) {
                    merged_agents.push(canon.clone());
                    merged_agents.sort();
                }
                candidate.agents = merged_agents;
                lib.servers[existing_idx] = candidate;
            }
            ConflictStrategy::Rename => {
                if same_server_spec(&lib.servers[existing_idx], &candidate) {
                    if !lib.servers[existing_idx].agents.contains(&canon) {
                        lib.servers[existing_idx].agents.push(canon.clone());
                        lib.servers[existing_idx].agents.sort();
                    }
                } else {
                    candidate.name = pick_unique_name(&lib.servers, &candidate.name);
                    lib.servers.push(candidate);
                }
            }
        }
    } else {
        lib.servers.push(candidate);
    }

    let warnings = sync::sync_library_with_roots(&roots, &mut lib)?;
    write_library_unlocked(&lib)?;
    let agents = detect_agent_targets(&roots);
    let (found, mut disc_warnings) = discover::discover_found_servers(&roots, &lib);
    let mut all_warnings = warnings;
    all_warnings.append(&mut disc_warnings);
    Ok(LibrarySnapshot {
        servers: lib.servers,
        agents,
        found,
        warnings: all_warnings,
    })
}

/// Remove an unmanaged server entry directly from an agent's config file (backing up first).
pub fn remove_found_server(agent_id: &str, server_name: &str) -> McpHubResult<LibrarySnapshot> {
    let canon = canonical_agent_id(agent_id).to_string();
    let _guard = acquire_library_lock()?;
    let lib = read_library_unlocked()?;
    let roots = McpPathRoots::from_env();
    let warnings = discover::remove_unmanaged_server(&roots, &canon, server_name)?;
    let agents = detect_agent_targets(&roots);
    let (found, mut disc_warnings) = discover::discover_found_servers(&roots, &lib);
    let mut all_warnings = warnings;
    all_warnings.append(&mut disc_warnings);
    Ok(LibrarySnapshot {
        servers: lib.servers,
        agents,
        found,
        warnings: all_warnings,
    })
}

/// Return one server by name (read-only).
pub fn get_server(server_name: &str) -> McpHubResult<McpServer> {
    let _guard = acquire_library_lock()?;
    let lib = read_library_unlocked()?;
    lib.servers
        .into_iter()
        .find(|s| s.name == server_name)
        .ok_or_else(|| McpHubError::NotFound(server_name.to_string()))
}

fn same_server_spec(a: &McpServer, b: &McpServer) -> bool {
    a.transport == b.transport
        && a.command == b.command
        && a.args == b.args
        && a.url == b.url
}

fn pick_unique_name(existing: &[McpServer], base: &str) -> String {
    if !existing.iter().any(|s| s.name == base) {
        return base.to_string();
    }
    for n in 2..1000 {
        let cand = format!("{base}-{n}");
        if !existing.iter().any(|s| s.name == cand) {
            return cand;
        }
    }
    format!("{base}-{}", Utc::now().timestamp())
}
