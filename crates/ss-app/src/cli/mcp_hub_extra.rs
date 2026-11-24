use anyhow::{Context, Result, bail};
use ss_mcp_hub::library::{
    get_server, get_snapshot, remove_found_server, save_server, save_servers_batch,
    set_server_paused,
};
use ss_mcp_hub::model::{ConflictStrategy, StoreInstallRequest};
use ss_mcp_hub::store::query_store;
use ss_mcp_hub::targets::{McpPathRoots, canonical_agent_id, detect_agent_targets};
use ss_mcp_hub::{
    build_project_snippets, install_store_server, list_backups, parse_pasted_config,
    restore_backup,
};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

fn print_warnings(warnings: &[ss_mcp_hub::model::SyncWarning]) {
    for w in warnings {
        eprintln!("Warning [{}]: {}", w.agent_id, w.message);
    }
}

/// Search and install an MCP server from the built-in store.
#[allow(clippy::too_many_arguments)]
pub async fn cmd_mcp_store_install(
    item_id_or_query: &str,
    name: Option<&str>,
    inputs: &[String],
    agents: &[String],
    online: bool,
    json: bool,
) -> Result<()> {
    let trimmed = item_id_or_query.trim();
    if trimmed.is_empty() {
        bail!("store item id or query must not be empty");
    }

    let resp = query_store(trimmed, online, 30).await?;
    let item = resp
        .items
        .iter()
        .find(|i| i.id == trimmed || i.name == trimmed)
        .cloned()
        .or_else(|| {
            if resp.items.len() == 1 {
                resp.items.first().cloned()
            } else {
                None
            }
        });
    let Some(item) = item else {
        if resp.items.is_empty() {
            bail!("no store item matched '{trimmed}'");
        }
        let candidates: Vec<String> = resp.items.iter().take(5).map(|i| i.id.clone()).collect();
        bail!(
            "multiple store items matched '{trimmed}'; specify exact id ({})",
            candidates.join(", ")
        );
    };

    let mut values = BTreeMap::new();
    for pair in inputs {
        let (k, v) = pair
            .split_once('=')
            .with_context(|| format!("invalid --input '{pair}', expected KEY=VALUE"))?;
        values.insert(k.trim().to_string(), v.to_string());
    }

    let resolved_agents: Vec<String> = if agents.is_empty() {
        let roots = McpPathRoots::from_env();
        detect_agent_targets(&roots)
            .into_iter()
            .filter(|a| a.installed)
            .map(|a| a.id)
            .collect()
    } else {
        agents
            .iter()
            .map(|a| canonical_agent_id(a).to_string())
            .collect()
    };

    let item_id = item.id.clone();
    let snap = install_store_server(StoreInstallRequest {
        item,
        name: name.map(str::to_string),
        values,
        agents: resolved_agents,
    })?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Installed MCP store server '{item_id}' into library.");
        print_warnings(&snap.warnings);
    }
    Ok(())
}

/// Pause a managed MCP server (withdraws it from synced agent configs without deleting it).
pub fn cmd_mcp_pause(name: &str, json: bool) -> Result<()> {
    let snap = set_server_paused(name, true)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Paused MCP server '{name}' and withdrew it from agent configs.");
        print_warnings(&snap.warnings);
    }
    Ok(())
}

/// Resume a paused MCP server and re-sync it to configured agents.
pub fn cmd_mcp_resume(name: &str, json: bool) -> Result<()> {
    let snap = set_server_paused(name, false)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Resumed MCP server '{name}' and synced configured agents.");
        print_warnings(&snap.warnings);
    }
    Ok(())
}

/// Remove an unmanaged discovered MCP server directly from an agent's config file.
pub fn cmd_mcp_remove_found(agent: &str, name: &str, json: bool) -> Result<()> {
    let canon = canonical_agent_id(agent);
    let snap = remove_found_server(canon, name)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Removed unmanaged MCP server '{name}' from agent '{canon}'.");
        print_warnings(&snap.warnings);
    }
    Ok(())
}

/// Parse pasted JSON/TOML/YAML/CLI MCP config text (`-` reads stdin) and save to library.
pub fn cmd_mcp_paste(raw: &str, agents: &[String], replace: bool, json: bool) -> Result<()> {
    let input = if raw == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        raw.to_string()
    };
    let parsed = parse_pasted_config(&input)?;
    if parsed.servers.is_empty() {
        bail!(
            "no valid MCP server entries found in pasted input (detected: {})",
            parsed.detected_kind
        );
    }

    let resolved_agents: Vec<String> = agents
        .iter()
        .map(|a| canonical_agent_id(a).to_string())
        .collect();
    let mut servers = parsed.servers;
    if !resolved_agents.is_empty() {
        for srv in &mut servers {
            srv.agents = resolved_agents.clone();
        }
    }

    let count = servers.len();
    let conflict = if replace {
        ConflictStrategy::Replace
    } else {
        ConflictStrategy::Rename
    };
    let delta = save_servers_batch(servers, conflict)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&delta)?);
    } else {
        println!("Imported {count} MCP server(s) from pasted config.");
        print_warnings(&delta.warnings);
    }
    Ok(())
}

/// Generate project-level MCP configuration snippets for active library servers.
pub fn cmd_mcp_snippets(
    project_root: Option<&Path>,
    agents: &[String],
    json: bool,
) -> Result<()> {
    let snap = get_snapshot()?;
    let filter: Vec<String> = agents
        .iter()
        .map(|a| canonical_agent_id(a).to_string())
        .collect();
    let snippets = build_project_snippets(project_root, &snap.servers, &filter);
    if json {
        println!("{}", serde_json::to_string_pretty(&snippets)?);
        return Ok(());
    }
    if snippets.is_empty() {
        println!("No project MCP snippets available (no active servers or matching agent).");
        return Ok(());
    }
    for s in snippets {
        println!(
            "=== {} ({}) [{}] ===\n{}\n",
            s.agent_name,
            s.rel_path,
            s.format.as_str(),
            s.content.trim_end()
        );
    }
    Ok(())
}

/// View or update `disabled_tools` on a managed MCP server.
pub fn cmd_mcp_tools(
    name: &str,
    disable: &[String],
    enable: &[String],
    enable_all: bool,
    json: bool,
) -> Result<()> {
    let mut srv = get_server(name)?;
    if disable.is_empty() && enable.is_empty() && !enable_all {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "name": srv.name,
                    "disabledTools": srv.disabled_tools,
                }))?
            );
        } else if srv.disabled_tools.is_empty() {
            println!("MCP server '{name}' has no disabled tools.");
        } else {
            println!(
                "Disabled tools on '{name}': {}",
                srv.disabled_tools.join(", ")
            );
        }
        return Ok(());
    }

    if enable_all {
        srv.disabled_tools.clear();
    }
    for tool in enable {
        let t = tool.trim();
        if !t.is_empty() {
            srv.disabled_tools.retain(|existing| existing != t);
        }
    }
    for tool in disable {
        let t = tool.trim();
        if !t.is_empty() && !srv.disabled_tools.iter().any(|existing| existing == t) {
            srv.disabled_tools.push(t.to_string());
        }
    }

    let snap = save_server(srv, Some(name), ConflictStrategy::Replace)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Updated disabled tools for MCP server '{name}'.");
        print_warnings(&snap.warnings);
    }
    Ok(())
}

/// Rename a managed MCP server and re-sync configured agents.
pub fn cmd_mcp_rename(old_name: &str, new_name: &str, replace: bool, json: bool) -> Result<()> {
    let trimmed_new = new_name.trim();
    if trimmed_new.is_empty() {
        bail!("new server name must not be empty");
    }
    let mut srv = get_server(old_name)?;
    srv.name = trimmed_new.to_string();
    let conflict = if replace {
        ConflictStrategy::Replace
    } else {
        ConflictStrategy::Rename
    };
    let snap = save_server(srv, Some(old_name), conflict)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Renamed MCP server '{old_name}' -> '{trimmed_new}'.");
        print_warnings(&snap.warnings);
    }
    Ok(())
}

/// List rolling MCP config backups newest first.
pub fn cmd_mcp_backups_list(limit: Option<usize>, json: bool) -> Result<()> {
    let entries = list_backups(limit)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }
    if entries.is_empty() {
        println!("No MCP config backups recorded.");
        return Ok(());
    }
    for entry in entries {
        let state = if entry.existed { "snapshot" } else { "absent" };
        let target = entry.target_path.as_deref().unwrap_or("-");
        println!(
            "{}  [{} | {} bytes]  {}  -> {}",
            entry.id, state, entry.size_bytes, entry.created_at, target
        );
    }
    Ok(())
}

/// Restore a rolling MCP config backup by id (and optional explicit target path override).
pub fn cmd_mcp_backups_restore(
    backup_id: &str,
    target_path: Option<&Path>,
    json: bool,
) -> Result<()> {
    let res = restore_backup(backup_id, target_path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&res)?);
    } else if res.removed_new_file {
        println!(
            "Restored absent pre-state from backup '{}': removed {}",
            res.id, res.target_path
        );
    } else {
        println!(
            "Restored {} byte(s) to {} from backup '{}'.",
            res.restored_bytes, res.target_path, res.id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss_mcp_hub::library::{get_server, save_server};
    use ss_mcp_hub::model::{ConflictStrategy, McpServer, Transport};
    use tempfile::TempDir;

    struct CliSandbox {
        _lock: tokio::sync::MutexGuard<'static, ()>,
        _data_dir: TempDir,
        home_dir: TempDir,
        prev_data_dir: Option<std::ffi::OsString>,
        prev_tool_home: Option<std::ffi::OsString>,
    }

    impl CliSandbox {
        fn enter() -> Self {
            let lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
            let prev_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
            let prev_tool_home = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            let data_dir = TempDir::new().unwrap();
            let home_dir = TempDir::new().unwrap();
            unsafe {
                std::env::set_var("SKILLSTAR_DATA_DIR", data_dir.path());
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", home_dir.path());
            }
            Self {
                _lock: lock,
                _data_dir: data_dir,
                home_dir,
                prev_data_dir,
                prev_tool_home,
            }
        }
    }

    impl Drop for CliSandbox {
        fn drop(&mut self) {
            unsafe {
                match self.prev_data_dir.take() {
                    Some(v) => std::env::set_var("SKILLSTAR_DATA_DIR", v),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
                match self.prev_tool_home.take() {
                    Some(v) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", v),
                    None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                }
            }
        }
    }

    #[test]
    fn cmd_mcp_add_replace_preserves_metadata_fields() {
        let sb = CliSandbox::enter();
        std::fs::write(sb.home_dir.path().join(".claude.json"), "{}").unwrap();

        let original = McpServer {
            name: "alpha".into(),
            transport: Transport::Stdio,
            command: "npx".into(),
            args: vec!["-y".into(), "old-pkg".into()],
            disabled_tools: vec!["rm_file".into()],
            startup_timeout_sec: Some(25.0),
            paused: true,
            market_id: Some("featured:alpha".into()),
            agents: vec!["claude".into()],
            ..Default::default()
        };
        save_server(original, None, ConflictStrategy::Replace).unwrap();

        super::super::cmd_mcp_add(
            "alpha",
            "stdio",
            Some("uvx"),
            &["new-pkg".to_string()],
            None,
            &["FOO=bar".to_string()],
            &[],
            &["claude".to_string()],
            true,
            true,
        )
        .unwrap();

        let updated = get_server("alpha").unwrap();
        assert_eq!(updated.command, "uvx");
        assert_eq!(updated.args, vec!["new-pkg"]);
        assert_eq!(updated.disabled_tools, vec!["rm_file"]);
        assert_eq!(updated.startup_timeout_sec, Some(25.0));
        assert!(updated.paused);
        assert_eq!(updated.market_id.as_deref(), Some("featured:alpha"));
    }

    #[test]
    fn cli_pause_resume_tools_rename_paste_snippets_and_backups_work() {
        let sb = CliSandbox::enter();
        let claude_path = sb.home_dir.path().join(".claude.json");
        std::fs::write(
            &claude_path,
            r#"{"mcpServers":{"unmanaged":{"command":"echo","args":["hi"]}}}"#,
        )
        .unwrap();

        // remove-found clears unmanaged entry
        cmd_mcp_remove_found("claude", "unmanaged", true).unwrap();
        let disk_after_rm = std::fs::read_to_string(&claude_path).unwrap();
        assert!(!disk_after_rm.contains("unmanaged"), "{disk_after_rm}");

        // paste imports server and syncs to claude
        cmd_mcp_paste(
            r#"{"mcpServers":{"demo":{"command":"echo","args":["ok"]}}}"#,
            &["claude".to_string()],
            true,
            true,
        )
        .unwrap();
        assert!(get_server("demo").is_ok());

        // tools disables and enables
        cmd_mcp_tools(
            "demo",
            &["bad_tool".to_string(), "other_tool".to_string()],
            &[],
            false,
            true,
        )
        .unwrap();
        assert_eq!(
            get_server("demo").unwrap().disabled_tools,
            vec!["bad_tool", "other_tool"]
        );
        cmd_mcp_tools("demo", &[], &["bad_tool".to_string()], false, true).unwrap();
        assert_eq!(get_server("demo").unwrap().disabled_tools, vec!["other_tool"]);
        cmd_mcp_tools("demo", &[], &[], true, true).unwrap();
        assert!(get_server("demo").unwrap().disabled_tools.is_empty());

        // pause & resume & snippets
        cmd_mcp_pause("demo", true).unwrap();
        assert!(get_server("demo").unwrap().paused);
        cmd_mcp_resume("demo", true).unwrap();
        assert!(!get_server("demo").unwrap().paused);
        cmd_mcp_snippets(None, &["claude".to_string()], true).unwrap();

        // rename
        cmd_mcp_rename("demo", "demo-v2", true, true).unwrap();
        assert!(get_server("demo").is_err());
        assert!(get_server("demo-v2").is_ok());

        // backups list & restore
        let backups = list_backups(None).unwrap();
        assert!(!backups.is_empty());
        cmd_mcp_backups_list(Some(5), true).unwrap();
        cmd_mcp_backups_restore(&backups[0].id, None, true).unwrap();
    }

    #[tokio::test]
    async fn cli_store_install_validates_required_inputs() {
        let _sb = CliSandbox::enter();
        let err = cmd_mcp_store_install("github", Some("my-gh"), &[], &[], false, true)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("Authorization") || err.to_string().contains("missing required"),
            "unexpected error: {err}"
        );

        cmd_mcp_store_install(
            "github",
            Some("my-gh"),
            &["Authorization=ghp_abc".to_string()],
            &[],
            false,
            true,
        )
        .await
        .unwrap();
        let saved = get_server("my-gh").unwrap();
        assert_eq!(
            saved.headers.get("Authorization").map(String::as_str),
            Some("Bearer ghp_abc")
        );
    }
}
