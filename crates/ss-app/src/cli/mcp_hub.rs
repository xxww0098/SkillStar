use super::{McpBackupsCommand, McpCliCommand};
use anyhow::{Context, Result, bail};
use ss_mcp_hub::library::{
    get_server, get_snapshot, import_found_server, remove_server, save_server, sync_all,
};
use ss_mcp_hub::model::{ConflictStrategy, McpServer, Transport};
use ss_mcp_hub::probe::check_server;
use ss_mcp_hub::store::query_store;
use ss_mcp_hub::targets::{McpPathRoots, canonical_agent_id, detect_agent_targets};
use std::collections::BTreeMap;

#[path = "mcp_hub_extra.rs"]
mod extra;

pub use extra::*;

pub fn cmd_mcp_hub(command: McpCliCommand) {
    if let Err(err) = dispatch(command) {
        eprintln!("✗ {err}");
        std::process::exit(1);
    }
}

pub fn dispatch(command: McpCliCommand) -> Result<()> {
    match command {
        McpCliCommand::Approve { .. } => Ok(()),
        McpCliCommand::List { json } => cmd_mcp_list(json),
        McpCliCommand::Agents { installed, json } => cmd_mcp_agents(installed, json),
        McpCliCommand::Store {
            query,
            online,
            limit,
            json,
        } => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(cmd_mcp_store(query.as_deref(), online, limit, json)),
        McpCliCommand::Add {
            name,
            transport,
            command,
            args,
            url,
            envs,
            headers,
            agent,
            replace,
            json,
        } => cmd_mcp_add(
            &name,
            &transport,
            command.as_deref(),
            &args,
            url.as_deref(),
            &envs,
            &headers,
            &agent,
            replace,
            json,
        ),
        McpCliCommand::Import {
            agent,
            name,
            all,
            replace,
            json,
        } => cmd_mcp_import(agent.as_deref(), name.as_deref(), all, replace, json),
        McpCliCommand::Remove { name, json } => cmd_mcp_remove(&name, json),
        McpCliCommand::Sync { json } => cmd_mcp_sync(json),
        McpCliCommand::Check { name, json } => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(cmd_mcp_check(&name, json)),
        McpCliCommand::StoreInstall {
            item,
            name,
            inputs,
            agent,
            online,
            json,
        } => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(cmd_mcp_store_install(
                &item,
                name.as_deref(),
                &inputs,
                &agent,
                online,
                json,
            )),
        McpCliCommand::Pause { name, json } => cmd_mcp_pause(&name, json),
        McpCliCommand::Resume { name, json } => cmd_mcp_resume(&name, json),
        McpCliCommand::RemoveFound { agent, name, json } => {
            cmd_mcp_remove_found(&agent, &name, json)
        }
        McpCliCommand::Paste {
            raw,
            agent,
            replace,
            json,
        } => cmd_mcp_paste(&raw, &agent, replace, json),
        McpCliCommand::Snippets {
            project_root,
            agent,
            json,
        } => cmd_mcp_snippets(project_root.as_deref(), &agent, json),
        McpCliCommand::Tools {
            name,
            disable,
            enable,
            enable_all,
            json,
        } => cmd_mcp_tools(&name, &disable, &enable, enable_all, json),
        McpCliCommand::Rename {
            old_name,
            new_name,
            replace,
            json,
        } => cmd_mcp_rename(&old_name, &new_name, replace, json),
        McpCliCommand::Backups { command } => match command {
            McpBackupsCommand::List { limit, json } => cmd_mcp_backups_list(limit, json),
            McpBackupsCommand::Restore {
                backup_id,
                target_path,
                json,
            } => cmd_mcp_backups_restore(&backup_id, target_path.as_deref(), json),
        },
    }
}

pub fn cmd_mcp_list(json: bool) -> Result<()> {
    let snap = get_snapshot()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }
    if snap.servers.is_empty() {
        println!("No managed external MCP servers configured.");
    } else {
        for srv in &snap.servers {
            let status = if srv.paused { "paused" } else { "active" };
            let target = match srv.transport {
                Transport::Stdio => format!("{} {}", srv.command, srv.args.join(" ")),
                Transport::Http | Transport::Sse => srv.url.clone(),
            };
            let agents = if srv.agents.is_empty() {
                "-".to_string()
            } else {
                srv.agents.join(", ")
            };
            println!(
                "{} [{} / {}] -> {} (agents: {})",
                srv.name,
                srv.transport.as_str(),
                status,
                target.trim(),
                agents
            );
        }
    }
    if !snap.found.is_empty() {
        println!("\nDiscovered unmanaged MCP servers ({}):", snap.found.len());
        for f in &snap.found {
            println!("  {}:{} ({})", f.agent_id, f.server.name, f.config_path);
        }
    }
    Ok(())
}

pub fn cmd_mcp_agents(installed_only: bool, json: bool) -> Result<()> {
    let roots = McpPathRoots::from_env();
    let mut agents = detect_agent_targets(&roots);
    if installed_only {
        agents.retain(|a| a.installed);
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&agents)?);
        return Ok(());
    }
    for agent in agents {
        let mark = if agent.installed { "installed" } else { "absent" };
        println!(
            "{:<14} {:<18} [{}] ({}) -> {}",
            agent.id,
            agent.name,
            mark,
            agent.format.as_str(),
            agent.config_path
        );
    }
    Ok(())
}

pub async fn cmd_mcp_store(
    query: Option<&str>,
    online: bool,
    limit: usize,
    json: bool,
) -> Result<()> {
    let resp = query_store(query.unwrap_or(""), online, limit).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    if let Some(err) = &resp.online_error {
        eprintln!("Warning: online MCP Registry query failed, showing offline results: {err}");
    }
    for item in resp.items {
        let tag = if item.featured { "featured" } else { item.source.as_str() };
        println!(
            "{:<24} [{} / {}] {}",
            item.name,
            item.transport.as_str(),
            tag,
            item.description
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_mcp_add(
    name: &str,
    transport: &str,
    command: Option<&str>,
    args: &[String],
    url: Option<&str>,
    envs: &[String],
    headers: &[String],
    agents: &[String],
    replace: bool,
    json: bool,
) -> Result<()> {
    let parsed_transport = match transport.to_lowercase().as_str() {
        "stdio" => Transport::Stdio,
        "http" => Transport::Http,
        "sse" => Transport::Sse,
        other => bail!("unsupported transport '{other}'; expected stdio, http, or sse"),
    };

    let mut env_map = BTreeMap::new();
    for pair in envs {
        let (k, v) = pair
            .split_once('=')
            .with_context(|| format!("invalid --env '{pair}', expected KEY=VALUE"))?;
        env_map.insert(k.trim().to_string(), v.to_string());
    }

    let mut hdr_map = BTreeMap::new();
    for pair in headers {
        let (k, v) = pair
            .split_once(':')
            .or_else(|| pair.split_once('='))
            .with_context(|| format!("invalid --header '{pair}', expected KEY:VALUE"))?;
        hdr_map.insert(k.trim().to_string(), v.trim().to_string());
    }

    let existing = if replace { get_server(name).ok() } else { None };
    let server = McpServer {
        name: name.to_string(),
        transport: parsed_transport,
        command: command.unwrap_or("").to_string(),
        args: args.to_vec(),
        env: env_map,
        url: url.unwrap_or("").to_string(),
        headers: hdr_map,
        disabled_tools: existing
            .as_ref()
            .map(|s| s.disabled_tools.clone())
            .unwrap_or_default(),
        startup_timeout_sec: existing.as_ref().and_then(|s| s.startup_timeout_sec),
        paused: existing.as_ref().map(|s| s.paused).unwrap_or(false),
        market_id: existing.as_ref().and_then(|s| s.market_id.clone()),
        agents: agents
            .iter()
            .map(|a| canonical_agent_id(a).to_string())
            .collect(),
        ..Default::default()
    };

    let strategy = if replace {
        ConflictStrategy::Replace
    } else {
        ConflictStrategy::Rename
    };
    let edit_of = existing.as_ref().map(|s| s.name.as_str());
    let snap = save_server(server, edit_of, strategy)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Saved MCP server '{name}' and synced configured agents.");
        for w in snap.warnings {
            eprintln!("Warning [{}]: {}", w.agent_id, w.message);
        }
    }
    Ok(())
}

pub fn cmd_mcp_import(
    agent: Option<&str>,
    name: Option<&str>,
    all: bool,
    replace: bool,
    json: bool,
) -> Result<()> {
    let conflict = if replace {
        ConflictStrategy::Replace
    } else {
        ConflictStrategy::Rename
    };
    let initial = get_snapshot()?;
    let target_agent = agent.map(|a| canonical_agent_id(a).to_string());

    let matches: Vec<_> = initial
        .found
        .into_iter()
        .filter(|f| target_agent.as_ref().is_none_or(|id| id == &f.agent_id))
        .filter(|f| name.is_none_or(|n| n == f.server.name))
        .collect();

    if matches.is_empty() {
        bail!("no matching unmanaged MCP servers found");
    }
    if !all && matches.len() > 1 && name.is_none() {
        bail!(
            "found {} unmanaged MCP servers; specify --name or pass --all",
            matches.len()
        );
    }

    let mut latest = None;
    for item in matches {
        latest = Some(import_found_server(&item.agent_id, &item.server.name, conflict)?);
    }
    let snap = latest.context("nothing imported")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Imported unmanaged MCP server(s) losslessly.");
    }
    Ok(())
}

pub fn cmd_mcp_remove(name: &str, json: bool) -> Result<()> {
    let snap = remove_server(name)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Removed MCP server '{name}' from library and synced agents.");
    }
    Ok(())
}

pub fn cmd_mcp_sync(json: bool) -> Result<()> {
    let snap = sync_all()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap)?);
    } else {
        println!("Synchronized {} MCP server(s).", snap.servers.len());
        for w in snap.warnings {
            eprintln!("Warning [{}]: {}", w.agent_id, w.message);
        }
    }
    Ok(())
}

pub async fn cmd_mcp_check(name: &str, json: bool) -> Result<()> {
    let srv = get_server(name)?;
    let res = check_server(&srv).await;
    if json {
        println!("{}", serde_json::to_string_pretty(&res)?);
        return Ok(());
    }
    if res.ok {
        println!(
            "OK: {} ({} {}, {} tools, {}ms)",
            name,
            if res.server_name.is_empty() {
                "unknown"
            } else {
                &res.server_name
            },
            res.server_version,
            res.tools.len(),
            res.duration_ms
        );
        for tool in res.tools {
            println!("  - {}: {}", tool.name, tool.description);
        }
        Ok(())
    } else {
        bail!(
            "MCP server '{}' check failed: {}",
            name,
            res.error.unwrap_or_else(|| "unknown error".into())
        )
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::{Cli, Commands, McpBackupsCommand, McpCliCommand};

    #[test]
    fn parses_all_new_mcp_hub_subcommands() {
        let cases: &[&[&str]] = &[
            &[
                "skillstar",
                "mcp",
                "store-install",
                "github",
                "--name",
                "gh",
                "--input",
                "TOKEN=abc",
                "--agent",
                "codex,claude",
                "--online",
                "--json",
            ],
            &["skillstar", "mcp", "pause", "gh", "--json"],
            &["skillstar", "mcp", "resume", "gh", "--json"],
            &["skillstar", "mcp", "remove-found", "codex", "legacy", "--json"],
            &[
                "skillstar",
                "mcp",
                "paste",
                "-",
                "--agent",
                "codex",
                "--replace",
                "--json",
            ],
            &[
                "skillstar",
                "mcp",
                "snippets",
                "--project",
                "/tmp/repo",
                "--agent",
                "claude",
                "--json",
            ],
            &[
                "skillstar",
                "mcp",
                "tools",
                "gh",
                "--disable",
                "a,b",
                "--enable",
                "c",
                "--enable-all",
                "--json",
            ],
            &[
                "skillstar",
                "mcp",
                "rename",
                "old",
                "new",
                "--replace",
                "--json",
            ],
            &["skillstar", "mcp", "backups", "list", "--limit", "5", "--json"],
            &[
                "skillstar",
                "mcp",
                "backups",
                "restore",
                "bak-1",
                "--target",
                "/tmp/cfg.json",
                "--json",
            ],
        ];

        for argv in cases {
            let cli = Cli::try_parse_from(*argv)
                .unwrap_or_else(|err| panic!("failed to parse {:?}: {err}", argv));
            match cli.command {
                Commands::Mcp { command } => match command {
                    McpCliCommand::StoreInstall {
                        item,
                        name,
                        inputs,
                        agent,
                        online,
                        json,
                    } => {
                        assert_eq!(item, "github");
                        assert_eq!(name.as_deref(), Some("gh"));
                        assert_eq!(inputs, vec!["TOKEN=abc"]);
                        assert_eq!(agent, vec!["codex", "claude"]);
                        assert!(online && json);
                    }
                    McpCliCommand::Pause { name, json } => {
                        assert_eq!(name, "gh");
                        assert!(json);
                    }
                    McpCliCommand::Resume { name, json } => {
                        assert_eq!(name, "gh");
                        assert!(json);
                    }
                    McpCliCommand::RemoveFound { agent, name, json } => {
                        assert_eq!(agent, "codex");
                        assert_eq!(name, "legacy");
                        assert!(json);
                    }
                    McpCliCommand::Paste {
                        raw,
                        agent,
                        replace,
                        json,
                    } => {
                        assert_eq!(raw, "-");
                        assert_eq!(agent, vec!["codex"]);
                        assert!(replace && json);
                    }
                    McpCliCommand::Snippets {
                        project_root,
                        agent,
                        json,
                    } => {
                        assert_eq!(
                            project_root.as_deref(),
                            Some(std::path::Path::new("/tmp/repo"))
                        );
                        assert_eq!(agent, vec!["claude"]);
                        assert!(json);
                    }
                    McpCliCommand::Tools {
                        name,
                        disable,
                        enable,
                        enable_all,
                        json,
                    } => {
                        assert_eq!(name, "gh");
                        assert_eq!(disable, vec!["a", "b"]);
                        assert_eq!(enable, vec!["c"]);
                        assert!(enable_all && json);
                    }
                    McpCliCommand::Rename {
                        old_name,
                        new_name,
                        replace,
                        json,
                    } => {
                        assert_eq!(old_name, "old");
                        assert_eq!(new_name, "new");
                        assert!(replace && json);
                    }
                    McpCliCommand::Backups { command } => match command {
                        McpBackupsCommand::List { limit, json } => {
                            assert_eq!(limit, Some(5));
                            assert!(json);
                        }
                        McpBackupsCommand::Restore {
                            backup_id,
                            target_path,
                            json,
                        } => {
                            assert_eq!(backup_id, "bak-1");
                            assert_eq!(
                                target_path.as_deref(),
                                Some(std::path::Path::new("/tmp/cfg.json"))
                            );
                            assert!(json);
                        }
                    },
                    other => panic!("unexpected variant parsed from {:?}: {}", argv, match other {
                        McpCliCommand::Approve { .. } => "approve",
                        McpCliCommand::List { .. } => "list",
                        McpCliCommand::Agents { .. } => "agents",
                        McpCliCommand::Store { .. } => "store",
                        McpCliCommand::Add { .. } => "add",
                        McpCliCommand::Import { .. } => "import",
                        McpCliCommand::Remove { .. } => "remove",
                        McpCliCommand::Sync { .. } => "sync",
                        McpCliCommand::Check { .. } => "check",
                        _ => "other",
                    }),
                },
                _ => panic!("expected mcp command for {:?}", argv),
            }
        }
    }
}
