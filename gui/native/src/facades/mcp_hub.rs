//! External MCP management & built-in store facade (`McpHub` in TSX).
//!
//! File-touching and subprocess/network calls run on tokio's blocking pool or async
//! futures (`ss_mcp_hub::probe::check_server`, `ss_mcp_hub::store::query_store`).

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use ss_mcp_hub::{
    ConflictStrategy, McpServer, StoreInstallRequest, batch_remove_servers as batch_drop_servers,
    batch_set_paused as batch_toggle_paused, build_project_snippets,
    check_server as probe_mcp_server, get_server, get_snapshot, import_found_server,
    install_store_server_delta as install_store_delta, list_backups as list_hub_backups,
    parse_pasted_config, query_store as search_store, read_library_unlocked_wrapper,
    remove_found_server as drop_found_server, remove_server_delta,
    restore_backup as restore_hub_backup, save_server_delta as persist_server_delta, save_servers_batch,
    set_server_agents_delta as assign_agents_delta, set_server_paused_delta as toggle_paused_delta,
    sync_all as reconcile_all, toggle_server_agent_delta,
};
use std::path::PathBuf;

fn parse_conflict(raw: Option<&str>) -> ConflictStrategy {
    match raw.map(str::trim).unwrap_or("rename") {
        "replace" => ConflictStrategy::Replace,
        "keep" => ConflictStrategy::Keep,
        _ => ConflictStrategy::Rename,
    }
}

/// External MCP management & built-in store facade exposed to TSX as `McpHub`.
#[module]
pub struct McpHub;

#[module]
impl McpHub {
    /// Load managed external MCP servers, live 35-agent states, and unmanaged discovered entries.
    pub async fn snapshot() -> Result<JsonValue, String> {
        blocking(|| {
            let snap = get_snapshot().map_err(|err| err.to_string())?;
            serde_json::to_value(&snap).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("snapshot task failed: {join}"))?
    }

    /// Create or update one MCP server (with optional rename via `original_name`) and sync agents.
    pub async fn save_server(
        server: JsonValue,
        original_name: Option<String>,
        conflict: Option<String>,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let parsed: McpServer =
                serde_json::from_value(server).map_err(|err| format!("invalid server payload: {err}"))?;
            let strategy = parse_conflict(conflict.as_deref());
            let delta = persist_server_delta(parsed, original_name.as_deref(), strategy)
                .map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("save_server task failed: {join}"))?
    }

    /// Create/import multiple MCP servers in one lock acquisition and one reconciliation pass.
    pub async fn save_servers(
        servers: Vec<JsonValue>,
        conflict: Option<String>,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let parsed = servers
                .into_iter()
                .map(|raw| {
                    serde_json::from_value::<McpServer>(raw)
                        .map_err(|err| format!("invalid server payload: {err}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let strategy = parse_conflict(conflict.as_deref());
            let delta = save_servers_batch(parsed, strategy).map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("save_servers task failed: {join}"))?
    }

    /// Toggle one canonical MCP agent (or alias such as `agy`) on/off for a server and sync.
    pub async fn toggle_server_agent(
        name: String,
        agent_id: String,
        enabled: bool,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let delta = toggle_server_agent_delta(&name, &agent_id, enabled)
                .map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("toggle_server_agent task failed: {join}"))?
    }

    /// Replace the full selected agent list for one managed MCP server and sync.
    pub async fn set_server_agents(
        name: String,
        agent_ids: Vec<String>,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let delta = assign_agents_delta(&name, agent_ids).map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("set_server_agents task failed: {join}"))?
    }

    /// Pause or resume a server cleanly (withdrawing/restoring only ledger-owned agent entries).
    pub async fn set_server_paused(name: String, paused: bool) -> Result<JsonValue, String> {
        blocking(move || {
            let delta = toggle_paused_delta(&name, paused).map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("set_server_paused task failed: {join}"))?
    }

    /// Pause or resume multiple servers in one lock acquisition and one reconciliation pass.
    pub async fn batch_set_paused(
        names: Vec<String>,
        paused: bool,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let delta = batch_toggle_paused(&names, paused).map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("batch_set_paused task failed: {join}"))?
    }

    /// Remove a server from `library.json` and withdraw ledger-tracked entries from configured agents.
    pub async fn remove_server(name: String) -> Result<JsonValue, String> {
        blocking(move || {
            let delta = remove_server_delta(&name).map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("remove_server task failed: {join}"))?
    }

    /// Remove multiple servers in one lock acquisition and one reconciliation pass.
    pub async fn batch_remove_servers(names: Vec<String>) -> Result<JsonValue, String> {
        blocking(move || {
            let delta = batch_drop_servers(&names).map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("batch_remove_servers task failed: {join}"))?
    }

    /// Losslessly adopt an unmanaged MCP entry discovered in an agent config file.
    pub async fn import_found_server(
        agent_id: String,
        name: String,
        conflict: Option<String>,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let strategy = parse_conflict(conflict.as_deref());
            let snap = import_found_server(&agent_id, &name, strategy)
                .map_err(|err| err.to_string())?;
            serde_json::to_value(&snap).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("import_found_server task failed: {join}"))?
    }

    /// Delete an unmanaged entry from an agent's config file (with rolling backup).
    pub async fn remove_found_server(agent_id: String, name: String) -> Result<JsonValue, String> {
        blocking(move || {
            let snap = drop_found_server(&agent_id, &name).map_err(|err| err.to_string())?;
            serde_json::to_value(&snap).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("remove_found_server task failed: {join}"))?
    }

    /// Reconcile all managed servers across affected agent configuration files.
    pub async fn sync_all() -> Result<JsonValue, String> {
        blocking(|| {
            let snap = reconcile_all().map_err(|err| err.to_string())?;
            serde_json::to_value(&snap).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("sync_all task failed: {join}"))?
    }

    /// Parse pasted JSON/JSONC, single entry, or CLI command (`claude mcp add`, `npx`, `uvx`).
    pub async fn parse_paste(raw: String) -> Result<JsonValue, String> {
        blocking(move || {
            let parsed = parse_pasted_config(&raw).map_err(|err| err.to_string())?;
            serde_json::to_value(&parsed).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("parse_paste task failed: {join}"))?
    }

    /// Generate project-level MCP configuration previews across the 10 supported project targets.
    pub async fn project_snippets(
        agents: Vec<String>,
        project_root: Option<String>,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let lib = read_library_unlocked_wrapper().map_err(|err| err.to_string())?;
            let root_path = project_root
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            let snippets = build_project_snippets(root_path.as_deref(), &lib.servers, &agents);
            serde_json::to_value(&snippets).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("project_snippets task failed: {join}"))?
    }

    /// Probe live stdio/HTTP/SSE connectivity and enumerate `tools/list` for a stored server.
    pub async fn check_server(name: String) -> Result<JsonValue, String> {
        let srv = blocking(move || get_server(&name).map_err(|err| err.to_string()))
            .await
            .map_err(|join| format!("load server task failed: {join}"))??;
        let result = probe_mcp_server(&srv).await;
        serde_json::to_value(&result).map_err(|err| err.to_string())
    }

    /// Query the built-in MCP store (29 curated external servers + offline snapshot + optional online Registry).
    pub async fn query_store(
        query: String,
        online: bool,
        limit: u32,
    ) -> Result<JsonValue, String> {
        let resp = search_store(&query, online, limit as usize)
            .await
            .map_err(|err| err.to_string())?;
        serde_json::to_value(&resp).map_err(|err| err.to_string())
    }

    /// Instantiate a store server template with user inputs (`env` / `header` / `arg`) and sync agents.
    pub async fn install_store_server(request: JsonValue) -> Result<JsonValue, String> {
        blocking(move || {
            let req: StoreInstallRequest = serde_json::from_value(request)
                .map_err(|err| format!("invalid store install payload: {err}"))?;
            let delta = install_store_delta(req).map_err(|err| err.to_string())?;
            serde_json::to_value(&delta).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("install_store_server task failed: {join}"))?
    }

    /// List rolling MCP config backups newest first.
    pub async fn list_backups(limit: Option<u32>) -> Result<JsonValue, String> {
        blocking(move || {
            let entries =
                list_hub_backups(limit.map(|n| n as usize)).map_err(|err| err.to_string())?;
            serde_json::to_value(&entries).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("list_backups task failed: {join}"))?
    }

    /// Restore a rolling MCP config backup by id (and optional target path override).
    pub async fn restore_backup(
        backup_id: String,
        target_path: Option<String>,
    ) -> Result<JsonValue, String> {
        blocking(move || {
            let target_pb = target_path
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            let res = restore_hub_backup(&backup_id, target_pb.as_deref())
                .map_err(|err| err.to_string())?;
            serde_json::to_value(&res).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("restore_backup task failed: {join}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn snapshot_save_toggle_pause_and_store_roundtrip_in_sandbox() {
        let _lock = crate::facades::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let data_dir = TempDir::new().unwrap();
        let home_dir = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", data_dir.path());
            std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", home_dir.path());
        }

        let snap = McpHub::snapshot().await.unwrap();
        assert_eq!(snap["agents"].as_array().unwrap().len(), 35);
        assert!(snap["servers"].as_array().unwrap().is_empty());

        let saved = McpHub::save_server(
            serde_json::json!({
                "name": "demo-mem",
                "transport": "stdio",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-memory"],
                "agents": ["claude"]
            }),
            None,
            Some("replace".into()),
        )
        .await
        .unwrap();
        assert_eq!(saved["servers"].as_array().unwrap().len(), 1);
        assert!(home_dir.path().join(".claude.json").exists());

        let paused = McpHub::set_server_paused("demo-mem".into(), true)
            .await
            .unwrap();
        assert_eq!(paused["servers"][0]["paused"], JsonValue::Bool(true));

        let store = McpHub::query_store("playwright".into(), false, 10)
            .await
            .unwrap();
        assert!(!store["items"].as_array().unwrap().is_empty());

        let backups = McpHub::list_backups(Some(10)).await.unwrap();
        let entries = backups.as_array().unwrap();
        assert!(!entries.is_empty());
        let first_id = entries[0]["id"].as_str().unwrap().to_string();
        let restored = McpHub::restore_backup(first_id, None).await.unwrap();
        assert!(restored["targetPath"].as_str().is_some());

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
            std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME");
        }
    }
}
