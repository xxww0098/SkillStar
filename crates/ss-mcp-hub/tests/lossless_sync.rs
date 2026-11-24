use ss_core::infra::paths::mcp_backups_dir;
use std::sync::Mutex;

static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());
use ss_mcp_hub::edit::file::{read_lossless, write_lossless_with_backup};
use ss_mcp_hub::edit::jsonc::{parse_jsonc, patch_json_path};
use ss_mcp_hub::edit::toml_lossless::patch_toml_entry;
use ss_mcp_hub::formats::read_entries_from_target;
use ss_mcp_hub::library::{
    get_snapshot, import_found_server, remove_found_server, remove_server, save_server,
    set_server_paused, sync_all, toggle_server_agent,
};
use ss_mcp_hub::model::{
    ConflictStrategy, McpFormat, McpServer, StoreInstallRequest, Transport,
};
use ss_mcp_hub::probe::check_server;
use ss_mcp_hub::store::{featured::featured_servers, install_store_server, query_store};
use ss_mcp_hub::targets::{
    McpPathRoots, build_project_snippets, builtin_targets, canonical_agent_id, project_targets,
    resolve_target_spec,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use tempfile::TempDir;

struct SandboxEnv {
    _lock: std::sync::MutexGuard<'static, ()>,
    data_dir: TempDir,
    home_dir: TempDir,
}

impl SandboxEnv {
    fn enter() -> Self {
        let lock = TEST_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let data_dir = TempDir::new().unwrap();
        let home_dir = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", data_dir.path());
            std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", home_dir.path());
        }
        Self {
            _lock: lock,
            data_dir,
            home_dir,
        }
    }

    fn home(&self) -> &std::path::Path {
        self.home_dir.path()
    }
}

impl Drop for SandboxEnv {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
            std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME");
        }
    }
}

fn stdio_server(name: &str, agents: &[&str]) -> McpServer {
    let mut env = BTreeMap::new();
    env.insert("MY_API_TOKEN".into(), "${MY_API_TOKEN}".into());
    env.insert("LOG_LEVEL".into(), "info".into());
    McpServer {
        name: name.into(),
        transport: Transport::Stdio,
        command: "npx".into(),
        args: vec!["-y".into(), "@modelcontextprotocol/server-memory".into()],
        env,
        agents: agents.iter().map(|s| (*s).into()).collect(),
        ..Default::default()
    }
}

#[test]
fn canonical_agent_count_is_35_and_agy_converges_into_antigravity() {
    let sb = SandboxEnv::enter();
    let roots = McpPathRoots::from_env();
    assert!(roots.sandboxed);
    assert_eq!(roots.home_dir, sb.home());

    let targets = builtin_targets(&roots);
    assert_eq!(targets.len(), 35);

    let ids: BTreeSet<&str> = targets.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids.len(), 35);
    assert!(!ids.contains("agy"));
    assert!(!ids.contains("claude-magpie-3p"));
    assert!(!ids.contains("gemini"));
    assert!(!ids.contains("gemini-cli"));
    assert!(!ids.contains("windsurf"));
    assert!(!ids.contains("roo-code"));
    assert!(ids.contains("antigravity"));
    assert!(ids.contains("liveagent"));
    assert!(ids.contains("kiro-cli"));
    assert_eq!(canonical_agent_id("agy"), "antigravity");
    assert!(resolve_target_spec(&roots, "gemini").is_err());
    assert!(resolve_target_spec(&roots, "gemini-cli").is_err());

    let proj = project_targets();
    assert_eq!(proj.len(), 10);
    assert!(!proj.iter().any(|p| p.agent_id == "gemini"));
}

#[test]
fn jsonc_editor_preserves_comments_trailing_commas_custom_fields_and_siblings() {
    let sb = SandboxEnv::enter();
    let cursor_path = sb.home().join(".cursor/mcp.json");
    fs::create_dir_all(cursor_path.parent().unwrap()).unwrap();
    let initial = r#"{
  // User top comment
  "otherSetting": true,
  "mcpServers": {
    /* keep user custom entry */
    "user-custom": {
      "command": "echo",
      "args": ["hello"], // inline comment
    },
    "managed": {
      "command": "old-cmd",
      "timeout": 90,
      "autoApprove": ["ping"]
    }
  }
}
"#;
    fs::write(&cursor_path, initial).unwrap();

    // Update managed entry (also changing transport to http to prove stale owned keys disappear while `timeout`/`autoApprove` survive!)
    let srv = McpServer {
        name: "managed".into(),
        transport: Transport::Http,
        url: "https://example.com/mcp".into(),
        headers: BTreeMap::from([("Authorization".into(), "Bearer ${TOKEN}".into())]),
        agents: vec!["cursor".into()],
        ..Default::default()
    };
    let snap = save_server(srv, None, ConflictStrategy::Replace).unwrap();
    assert!(snap.warnings.is_empty(), "warnings: {:?}", snap.warnings);

    let after = fs::read_to_string(&cursor_path).unwrap();
    assert!(after.contains("// User top comment"), "{after}");
    assert!(after.contains("/* keep user custom entry */"), "{after}");
    assert!(after.contains("// inline comment"), "{after}");
    assert!(after.contains("\"otherSetting\": true"), "{after}");
    assert!(after.contains("\"timeout\": 90"), "{after}");
    assert!(after.contains("\"autoApprove\""), "{after}");
    assert!(after.contains("${env:TOKEN}"), "cursor should use ${{env:VAR}}: {after}");

    let parsed = parse_jsonc(&after).unwrap();
    let m_obj = parsed["mcpServers"]["managed"].as_object().unwrap();
    assert!(m_obj.get("command").is_none(), "stale stdio key removed");
    assert_eq!(m_obj["url"], "https://example.com/mcp");

    // Re-syncing identical state produces zero extra backups and zero edits
    let backup_dir = mcp_backups_dir();
    let count_before = fs::read_dir(&backup_dir).unwrap().count();
    sync_all().unwrap();
    let count_after = fs::read_dir(&backup_dir).unwrap().count();
    assert_eq!(count_before, count_after, "no-op sync must not create backup");

    // Removing managed server deletes ONLY `managed` while leaving `user-custom` + comments + valid JSONC intact
    remove_server("managed").unwrap();
    let final_str = fs::read_to_string(&cursor_path).unwrap();
    assert!(final_str.contains("// User top comment"));
    assert!(final_str.contains("\"user-custom\""));
    assert!(!final_str.contains("\"managed\""));
    let final_parsed = parse_jsonc(&final_str).unwrap();
    assert!(final_parsed["mcpServers"]["user-custom"].is_object());
}

/// BOM and CRLF survive a read-modify-write of a plain file on every platform.
#[test]
fn bom_crlf_preserved_across_writes() {
    let _sb = SandboxEnv::enter();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("mcp.json");

    let mut initial_bytes = Vec::from(b"\xEF\xBB\xBF".as_slice());
    initial_bytes.extend_from_slice(b"{\r\n  \"mcpServers\": {}\r\n}\r\n");
    fs::write(&path, &initial_bytes).unwrap();

    let (_, text, pres, _) = read_lossless(&path).unwrap().unwrap();
    assert!(pres.has_bom);
    assert!(pres.crlf);
    let patched = patch_json_path(
        &text,
        &["mcpServers"],
        "demo",
        Some(&serde_json::json!({ "command": "npx" })),
    )
    .unwrap();
    assert!(write_lossless_with_backup(&path, Some(&initial_bytes), &patched, &pres).unwrap());

    assert_preserved_encoding(&fs::read(&path).unwrap());
}

/// Symlinks are a unix property of the lossless editor: the write goes to the
/// link's target and the link stays a link. Windows needs a privilege to create
/// one, so the plain-file case above is what runs there.
#[cfg(unix)]
#[test]
fn bom_crlf_and_symlink_preserved_across_writes() {
    let _sb = SandboxEnv::enter();
    let dir = TempDir::new().unwrap();
    let real_file = dir.path().join("real_mcp.json");
    let link_path = dir.path().join("symlink_mcp.json");

    let mut initial_bytes = Vec::from(b"\xEF\xBB\xBF".as_slice());
    initial_bytes.extend_from_slice(b"{\r\n  \"mcpServers\": {}\r\n}\r\n");
    fs::write(&real_file, &initial_bytes).unwrap();
    std::os::unix::fs::symlink(&real_file, &link_path).unwrap();

    let (_, text, pres, resolved) = read_lossless(&link_path).unwrap().unwrap();
    assert!(pres.has_bom);
    assert!(pres.crlf);
    assert_eq!(
        fs::canonicalize(&resolved).unwrap(),
        fs::canonicalize(&real_file).unwrap()
    );

    let patched = patch_json_path(
        &text,
        &["mcpServers"],
        "demo",
        Some(&serde_json::json!({ "command": "npx" })),
    )
    .unwrap();
    let changed = write_lossless_with_backup(&link_path, Some(&initial_bytes), &patched, &pres).unwrap();
    assert!(changed);
    assert!(
        fs::symlink_metadata(&link_path)
            .unwrap()
            .file_type()
            .is_symlink(),
        "symlink must stay a symlink"
    );
    assert_preserved_encoding(&fs::read(&real_file).unwrap());
}

fn assert_preserved_encoding(disk_raw: &[u8]) {
    assert!(disk_raw.starts_with(b"\xEF\xBB\xBF"), "BOM preserved");
    let disk_str = String::from_utf8_lossy(&disk_raw[3..]);
    assert!(disk_str.contains("\r\n"), "CRLF preserved");
    assert!(!disk_str.replace("\r\n", "").contains('\n'), "no stray LF");
}

#[test]
fn toml_codex_lossless_preserves_projects_comments_and_ignores_bundled_runtimes() {
    let sb = SandboxEnv::enter();
    let codex_path = sb.home().join(".codex/config.toml");
    fs::create_dir_all(codex_path.parent().unwrap()).unwrap();
    let initial = r#"# Codex top comment
model = "gpt-5"

[projects."/Users/me/repo"]
trust_level = "trusted"

[mcp_servers.cua]
command = "/Applications/Codex.app/Contents/Resources/cua_node/bin/node"

[mcp_servers.alpha]
command = "old"
startup_timeout_sec = 45.0
custom_codex_note = "keep me"
"#;
    fs::write(&codex_path, initial).unwrap();

    let mut srv = McpServer {
        name: "alpha".into(),
        transport: Transport::Http,
        url: "https://mcp.example.com".into(),
        headers: BTreeMap::from([
            ("Authorization".into(), "Bearer ${AUTH_TOKEN}".into()),
            ("X-Env-Header".into(), "${HEADER_ENV}".into()),
            ("X-Static".into(), "plain".into()),
        ]),
        disabled_tools: vec!["dangerous_tool".into()],
        agents: vec!["codex".into()],
        ..Default::default()
    };
    save_server(srv.clone(), None, ConflictStrategy::Replace).unwrap();

    let after = fs::read_to_string(&codex_path).unwrap();
    assert!(after.contains("# Codex top comment"), "{after}");
    assert!(after.contains("[projects.\"/Users/me/repo\"]"), "{after}");
    assert!(after.contains("trust_level = \"trusted\""), "{after}");
    assert!(after.contains("custom_codex_note = \"keep me\""), "{after}");
    assert!(!after.contains("startup_timeout_sec"), "{after}");
    assert!(after.contains("bearer_token_env_var = \"AUTH_TOKEN\""), "{after}");
    assert!(after.contains("X-Env-Header = \"HEADER_ENV\""), "{after}");
    assert!(after.contains("disabled_tools = [\"dangerous_tool\"]"), "{after}");

    // Writing an explicit startup_timeout_sec sets it, and clearing it removes it again.
    srv.startup_timeout_sec = Some(60.0);
    save_server(srv.clone(), Some("alpha"), ConflictStrategy::Replace).unwrap();
    let with_timeout = fs::read_to_string(&codex_path).unwrap();
    assert!(with_timeout.contains("startup_timeout_sec = 60"), "{with_timeout}");

    srv.startup_timeout_sec = None;
    save_server(srv, Some("alpha"), ConflictStrategy::Replace).unwrap();
    let cleared_timeout = fs::read_to_string(&codex_path).unwrap();
    assert!(!cleared_timeout.contains("startup_timeout_sec"), "{cleared_timeout}");
    assert!(cleared_timeout.contains("custom_codex_note = \"keep me\""), "{cleared_timeout}");

    // Bundled `cua_node` server must not show up as unmanaged foundServer
    let snap = get_snapshot().unwrap();
    assert!(!snap.found.iter().any(|f| f.server.name == "cua"));

    // Untoggling removes ONLY alpha while leaving [projects.*] and `cua` intact
    toggle_server_agent("alpha", "codex", false).unwrap();
    let trimmed = fs::read_to_string(&codex_path).unwrap();
    assert!(trimmed.contains("[projects.\"/Users/me/repo\"]"));
    assert!(trimmed.contains("[mcp_servers.cua]"));
    assert!(!trimmed.contains("[mcp_servers.alpha]"));

    let _ = patch_toml_entry;
}

#[test]
fn zed_foreign_extension_entry_is_never_overwritten_or_deleted() {
    let sb = SandboxEnv::enter();
    let zed_dir = if cfg!(windows) {
        sb.home().join("AppData/Roaming/Zed")
    } else {
        sb.home().join(".config/zed")
    };
    fs::create_dir_all(&zed_dir).unwrap();
    let zed_path = zed_dir.join("settings.json");
    let zed_initial = r#"{
  // Zed UI theme
  "theme": "One Dark",
  "context_servers": {
    "ext-postgres": {
      "enabled": true,
      "settings": { "database_url": "postgres://localhost/db" }
    }
  }
}
"#;
    fs::write(&zed_path, zed_initial).unwrap();

    let conflicting = McpServer {
        name: "ext-postgres".into(),
        transport: Transport::Stdio,
        command: "npx".into(),
        agents: vec!["zed".into()],
        ..Default::default()
    };
    let snap = save_server(conflicting, None, ConflictStrategy::Replace).unwrap();
    assert!(
        snap.warnings
            .iter()
            .any(|w| w.code == "foreign_zed_extension" && w.agent_id == "zed"),
        "expected foreign_zed_extension warning: {:?}",
        snap.warnings
    );

    // Confirm extension entry stays byte-equivalent to before!
    let disk = fs::read_to_string(&zed_path).unwrap();
    assert_eq!(disk, zed_initial);
}

#[test]
fn pause_discovery_tools_off_and_35_format_roundtrips() {
    let sb = SandboxEnv::enter();

    // 1. Pre-seed unmanaged Claude Desktop server + Codex server + unrelated Cursor file untouched check
    let claude_path = sb.home().join(".claude.json");
    fs::write(&claude_path, r#"{ "mcpServers": { "legacy-tool": { "command": "uvx", "args": ["mcp-server-git"] } } }"#).unwrap();

    // Create unrelated user Deny in Claude settings
    let settings_path = sb.home().join(".claude/settings.json");
    fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
    fs::write(&settings_path, r#"{ "permissions": { "deny": ["Bash(rm -rf:*)"] } }"#).unwrap();

    let snap = get_snapshot().unwrap();
    assert!(snap.found.iter().any(|f| f.agent_id == "claude" && f.server.name == "legacy-tool"));

    // Import into library
    let snap = import_found_server("claude", "legacy-tool", ConflictStrategy::Replace).unwrap();
    assert!(snap.found.is_empty());

    // Update disabled tools and pause/resume
    let mut updated = snap.servers[0].clone();
    updated.disabled_tools = vec!["dangerous_exec".into()];
    updated.agents = vec!["claude".into(), "opencode".into()];
    save_server(updated, None, ConflictStrategy::Replace).unwrap();

    let settings_txt = fs::read_to_string(&settings_path).unwrap();
    assert!(settings_txt.contains("Bash(rm -rf:*)"), "user deny kept");
    assert!(settings_txt.contains("mcp__legacy-tool__dangerous_exec"));

    // Paused removes server from claude.json without losing library entry
    set_server_paused("legacy-tool", true).unwrap();
    let claude_paused = fs::read_to_string(&claude_path).unwrap();
    assert!(!claude_paused.contains("\"legacy-tool\""));
    assert_eq!(get_snapshot().unwrap().servers.len(), 1);

    // Resume restores it
    set_server_paused("legacy-tool", false).unwrap();
    let claude_resumed = fs::read_to_string(&claude_path).unwrap();
    assert!(claude_resumed.contains("\"legacy-tool\""));

    // Unmanaged remove-found removes entry with backup
    let ghost_path = sb.home().join(".kimi-code/mcp.json");
    fs::create_dir_all(ghost_path.parent().unwrap()).unwrap();
    fs::write(&ghost_path, r#"{ "mcpServers": { "ghost": { "command": "echo" } } }"#).unwrap();
    let snap = remove_found_server("kimi", "ghost").unwrap();
    assert!(!snap.found.iter().any(|f| f.server.name == "ghost"));
    assert!(!fs::read_to_string(&ghost_path).unwrap().contains("\"ghost\""));

    // Test every single one of the 35 built-in agents handles sync/read cleanly (including DSH patch & LiveAgent seed!)
    let liveagent_dir = sb.home().join(".liveagent");
    fs::create_dir_all(&liveagent_dir).unwrap();
    fs::write(
        liveagent_dir.join("config.json"),
        r#"{ "mcp": { "servers": [ { "id": "seeded", "enabled": true, "transport": "stdio", "command": "echo" } ], "selected": "seeded" } }"#,
    )
    .unwrap();

    let dsh_dir = sb.home().join(".dsh");
    fs::create_dir_all(dsh_dir.join("profiles/work")).unwrap();
    fs::write(
        dsh_dir.join("config.yaml"),
        "---\n- insert:\n    - id: user-plugin\n      name: \"user/other\"\n",
    )
    .unwrap();
    fs::write(dsh_dir.join("profiles/work/cordis.patch.yml"), "---\n").unwrap();

    let goose_path = if cfg!(windows) {
        sb.home().join("AppData/Roaming/Block/goose/config/config.yaml")
    } else {
        sb.home().join(".config/goose/config.yaml")
    };
    fs::create_dir_all(goose_path.parent().unwrap()).unwrap();
    fs::write(&goose_path, "# goose config\nGOOSE_MODEL: gpt-5\n").unwrap();

    let roots = McpPathRoots::from_env();
    let all_ids: Vec<String> = builtin_targets(&roots).into_iter().map(|t| t.id).collect();
    let universal = stdio_server("mem", &all_ids.iter().map(String::as_str).collect::<Vec<_>>());
    let snap_all = save_server(universal, None, ConflictStrategy::Replace).unwrap();
    assert!(
        snap_all.warnings.is_empty(),
        "stdio server must sync cleanly to all 35 agents: {:?}",
        snap_all.warnings
    );

    // Verify Goose preserved header comment + GOOSE_MODEL
    let goose_after = fs::read_to_string(&goose_path).unwrap();
    assert!(goose_after.contains("# goose config"), "{goose_after}");
    assert!(goose_after.contains("GOOSE_MODEL: gpt-5"), "{goose_after}");
    assert!(goose_after.contains("env_keys:"), "{goose_after}");

    // Verify DSH preserved user insert & wrote profile patch with !!js
    let dsh_after = fs::read_to_string(dsh_dir.join("config.yaml")).unwrap();
    assert!(dsh_after.contains("user/other"), "{dsh_after}");
    assert!(dsh_after.contains("- insert: # skillstar"), "{dsh_after}");
    assert!(dsh_after.contains("!!js process.env.MY_API_TOKEN"), "{dsh_after}");
    assert!(fs::read_to_string(dsh_dir.join("profiles/work/cordis.patch.yml"))
        .unwrap()
        .contains("- insert: # skillstar"));

    // Verify LiveAgent preserved seeded entry alongside newly added `mem`
    let live_spec = resolve_target_spec(&roots, "liveagent").unwrap();
    let live_entries = read_entries_from_target(&live_spec, &live_spec.primary_path).unwrap();
    assert!(live_entries.iter().any(|s| s.name == "seeded"));
    assert!(live_entries.iter().any(|s| s.name == "mem"));

    // Verify agy alias toggle routes cleanly to `antigravity`
    toggle_server_agent("mem", "agy", false).unwrap();
    let agy_spec = resolve_target_spec(&roots, "antigravity").unwrap();
    let agy_entries = read_entries_from_target(&agy_spec, &agy_spec.primary_path).unwrap();
    assert!(!agy_entries.iter().any(|s| s.name == "mem"));

    // Verify SSE rejection warning on no-sse formats (e.g. DSH & Codex)
    let sse_srv = McpServer {
        name: "events".into(),
        transport: Transport::Sse,
        url: "https://events.example.com/sse".into(),
        agents: vec!["claude".into(), "codex".into(), "dsh".into()],
        ..Default::default()
    };
    let snap_sse = save_server(sse_srv, None, ConflictStrategy::Replace).unwrap();
    assert!(snap_sse.warnings.iter().any(|w| w.agent_id == "codex" && w.code == "unsupported_sse"));
    assert!(snap_sse.warnings.iter().any(|w| w.agent_id == "dsh" && w.code == "unsupported_sse"));

    // Project snippet preview generation covers all 10 targets
    let snippets = build_project_snippets(None, &snap_sse.servers, &[]);
    assert_eq!(snippets.len(), 10);
    assert!(snippets.iter().any(|s| s.agent_id == "codex" && s.content.contains("[mcp_servers.mem]")));
    assert!(!snippets.iter().any(|s| s.agent_id == "gemini"));

    let _ = McpFormat::Claude;
}

#[test]
fn gemini_cli_is_ignored_on_disk_and_purged_from_legacy_library() {
    let sb = SandboxEnv::enter();
    let gemini_settings = sb.home().join(".gemini/settings.json");
    fs::create_dir_all(gemini_settings.parent().unwrap()).unwrap();
    let original_gemini = r#"{ "mcpServers": { "gemini-only": { "httpUrl": "https://example.com/mcp" } } }"#;
    fs::write(&gemini_settings, original_gemini).unwrap();

    // Legacy library.json referencing `gemini` and `gemini-cli` must be cleaned without touching ~/.gemini/settings.json.
    let lib_path = ss_core::infra::paths::mcp_library_path();
    fs::create_dir_all(lib_path.parent().unwrap()).unwrap();
    fs::write(
        &lib_path,
        r#"{
  "version": 1,
  "servers": [
    {
      "name": "demo",
      "transport": "stdio",
      "command": "npx",
      "agents": ["claude", "gemini", "gemini-cli"]
    }
  ],
  "applied": {
    "gemini": { "mcp": ["demo"] },
    "gemini-cli": { "mcp": ["demo"] }
  }
}"#,
    )
    .unwrap();

    let snap = get_snapshot().unwrap();
    assert!(!snap.found.iter().any(|f| f.agent_id == "gemini" || f.server.name == "gemini-only"));
    assert_eq!(snap.servers[0].agents, vec!["claude".to_string()]);
    assert_eq!(fs::read_to_string(&gemini_settings).unwrap(), original_gemini);

    let persisted = fs::read_to_string(&lib_path).unwrap();
    assert!(!persisted.contains("gemini"), "legacy gemini ledger must be purged: {persisted}");

    let rejected = save_server(
        stdio_server("bad", &["gemini"]),
        None,
        ConflictStrategy::Replace,
    );
    assert!(rejected.is_err());
}

#[tokio::test]
async fn paste_store_and_stdio_probe_work_end_to_end() {
    let sb = SandboxEnv::enter();
    let _ = sb.data_dir.path();

    // 1. Curated featured list & offline snapshot pass schema/uniqueness/template invariants
    let featured = featured_servers();
    assert!(!featured.is_empty());
    let mut featured_ids = BTreeSet::new();
    let mut featured_names = BTreeSet::new();
    for item in &featured {
        assert!(!item.id.trim().is_empty());
        assert!(!item.id.starts_with("magpie-"));
        assert!(featured_ids.insert(item.id.clone()), "duplicate featured id: {}", item.id);
        assert!(featured_names.insert(item.name.clone()), "duplicate featured name: {}", item.name);
        ss_mcp_hub::env_refs::validate_server(&item.template)
            .unwrap_or_else(|e| panic!("invalid featured template '{}': {e}", item.id));
    }

    let snapshot_items = ss_mcp_hub::store::snapshot::search_snapshot("", 500);
    assert!(!snapshot_items.is_empty());
    for item in &snapshot_items {
        ss_mcp_hub::env_refs::validate_server(&item.template)
            .unwrap_or_else(|e| panic!("invalid snapshot template '{}': {e}", item.id));
    }

    // 2. Offline store query searches both featured and embedded market_servers.json snapshot
    let res = query_store("playwright", false, 20).await.unwrap();
    assert!(!res.online_used);
    assert!(res.items.iter().any(|i| i.id == "playwright"));

    let snap_res = query_store("browser-use", false, 20).await.unwrap();
    assert!(snap_res.items.iter().any(|i| i.id.contains("browser-use")));

    // Install store item with header input formatting (`Bearer {}`)
    let gh_item = featured.iter().find(|i| i.id == "github").unwrap().clone();
    let mut values = BTreeMap::new();
    values.insert("Authorization".into(), "ghp_test123".into());
    let snap = install_store_server(StoreInstallRequest {
        item: gh_item,
        name: Some("my-github".into()),
        values,
        agents: vec!["claude".into()],
    })
    .unwrap();
    let installed = snap.servers.iter().find(|s| s.name == "my-github").unwrap();
    assert_eq!(
        installed.headers.get("Authorization").map(String::as_str),
        Some("Bearer ghp_test123")
    );

    // 3. Adaptive paste parsing (CLI & JSONC)
    let cli = ss_mcp_hub::parse_pasted_config(
        "claude mcp add -t http -H \"Authorization: Bearer x\" linear https://mcp.linear.app/mcp",
    )
    .unwrap();
    assert_eq!(cli.servers[0].name, "linear");
    assert_eq!(cli.servers[0].transport, Transport::Http);

    // 4. Live stdio probe over piped subprocess if `python3` or `sh` is present
    #[cfg(unix)]
    if which::which("sh").is_ok() {
        let mock_srv = McpServer {
            name: "mock-stdio".into(),
            transport: Transport::Stdio,
            command: "sh".into(),
            args: vec![
                "-c".into(),
                concat!(
                    "while IFS= read -r line; do ",
                    "case \"$line\" in ",
                    "*\\\"initialize\\\"*) printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2024-11-05\",\"serverInfo\":{\"name\":\"mock-srv\",\"version\":\"0.1.0\"}}}' ;; ",
                    "*\\\"tools/list\\\"*) printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"tools\":[{\"name\":\"ping\",\"description\":\"Ping pong\"}]}}' ; break ;; ",
                    "esac; ",
                    "done"
                )
                .into(),
            ],
            ..Default::default()
        };
        let chk = check_server(&mock_srv).await;
        assert!(chk.ok, "probe failed: {:?}", chk.error);
        assert_eq!(chk.server_name, "mock-srv");
        assert_eq!(chk.tools.len(), 1);
        assert_eq!(chk.tools[0].name, "ping");
    }
}

#[test]
fn transport_change_withdraws_incompatible_agent_without_leaving_ghost_entry() {
    let sb = SandboxEnv::enter();
    let roots = McpPathRoots::from_env();
    // Pick a `no-sse` target inside sandbox (e.g. codex, which supports http & stdio, not sse)
    let codex_spec = resolve_target_spec(&roots, "codex").unwrap();
    fs::create_dir_all(codex_spec.primary_path.parent().unwrap()).unwrap();

    let srv = stdio_server("switch-test", &["codex"]);
    save_server(srv, None, ConflictStrategy::Replace).unwrap();
    let before_entries = read_entries_from_target(&codex_spec, &codex_spec.primary_path).unwrap();
    assert!(before_entries.iter().any(|s| s.name == "switch-test"));

    // Switching transport to SSE makes `codex` unsupported (`no-sse`):
    // sync must withdraw old stdio entry from config.toml instead of leaving a ghost entry.
    let updated = McpServer {
        name: "switch-test".into(),
        transport: Transport::Sse,
        url: "https://sse.example.com/events".into(),
        agents: vec!["codex".into()],
        ..Default::default()
    };
    let snap = save_server(updated, Some("switch-test"), ConflictStrategy::Replace).unwrap();
    assert!(
        snap.warnings
            .iter()
            .any(|w| w.agent_id == "codex" && w.code == "unsupported_sse"),
        "expected unsupported_sse warning: {:?}",
        snap.warnings
    );

    let after_entries = read_entries_from_target(&codex_spec, &codex_spec.primary_path).unwrap();
    assert!(
        !after_entries.iter().any(|s| s.name == "switch-test"),
        "stale stdio entry remained on disk: {after_entries:?}"
    );
    assert!(
        !snap
            .found
            .iter()
            .any(|f| f.agent_id == "codex" && f.server.name == "switch-test"),
        "ghost unmanaged entry appeared in found: {:?}",
        snap.found
    );
    let _ = sb;
}

#[test]
fn deveco_and_mimocode_support_tools_off_sync_and_cleanup() {
    let _sb = SandboxEnv::enter();
    let roots = McpPathRoots::from_env();
    let deveco = resolve_target_spec(&roots, "deveco").unwrap();
    let mimocode = resolve_target_spec(&roots, "mimocode").unwrap();
    assert!(deveco.supports_tools_off);
    assert!(mimocode.supports_tools_off);

    fs::create_dir_all(deveco.primary_path.parent().unwrap()).unwrap();
    fs::create_dir_all(mimocode.primary_path.parent().unwrap()).unwrap();

    let mut srv = stdio_server("tool-filter-srv", &["deveco", "mimocode"]);
    srv.disabled_tools = vec!["dangerous_exec".into()];
    save_server(srv.clone(), None, ConflictStrategy::Replace).unwrap();

    for spec in [&deveco, &mimocode] {
        let raw = fs::read_to_string(&spec.primary_path).unwrap();
        let parsed = parse_jsonc(&raw).unwrap();
        assert_eq!(
            parsed["tools"]["tool-filter-srv_dangerous_exec"],
            serde_json::json!(false),
            "expected disabled tool flag in {}: {raw}",
            spec.id
        );
    }

    // Clearing disabled_tools cleans up OpenCode-style `tools` keys
    srv.disabled_tools.clear();
    save_server(srv, Some("tool-filter-srv"), ConflictStrategy::Replace).unwrap();
    for spec in [&deveco, &mimocode] {
        let raw = fs::read_to_string(&spec.primary_path).unwrap();
        let parsed = parse_jsonc(&raw).unwrap();
        assert!(
            parsed["tools"]
                .as_object()
                .map(|m| !m.contains_key("tool-filter-srv_dangerous_exec"))
                .unwrap_or(true),
            "stale disabled tool flag remained in {}: {raw}",
            spec.id
        );
    }
}

#[test]
fn backups_list_and_restore_handles_absent_file_and_existing_content() {
    let sb = SandboxEnv::enter();
    let claude_path = sb.home().join(".claude.json");
    assert!(!claude_path.exists());

    // First write creates `.absent.bak` before creating `.claude.json`
    let srv1 = stdio_server("first-srv", &["claude"]);
    save_server(srv1, None, ConflictStrategy::Replace).unwrap();
    assert!(claude_path.exists());

    let backups = ss_mcp_hub::list_backups(None).unwrap();
    assert!(!backups.is_empty(), "expected at least one backup entry");
    let absent_bak = backups
        .iter()
        .find(|b| {
            !b.existed
                && b.target_path
                    .as_deref()
                    .map(|p| std::path::Path::new(p) == claude_path)
                    .unwrap_or(false)
        })
        .expect("expected absent marker backup for newly created .claude.json");

    // Second write backs up the prior `.claude.json` content
    let first_disk = fs::read_to_string(&claude_path).unwrap();
    let srv2 = stdio_server("second-srv", &["claude"]);
    save_server(srv2, None, ConflictStrategy::Replace).unwrap();
    assert!(fs::read_to_string(&claude_path).unwrap().contains("second-srv"));

    let backups2 = ss_mcp_hub::list_backups(None).unwrap();
    let canon_claude = fs::canonicalize(&claude_path).unwrap();
    let latest_non_absent = backups2
        .iter()
        .find(|b| {
            b.existed
                && b.target_path
                    .as_deref()
                    .map(|p| {
                        let pb = std::path::PathBuf::from(p);
                        fs::canonicalize(&pb).unwrap_or(pb) == canon_claude
                    })
                    .unwrap_or(false)
        })
        .unwrap_or_else(|| {
            panic!("expected content backup before second write; backups2={backups2:#?}")
        });

    let restored_prev = ss_mcp_hub::restore_backup(&latest_non_absent.id, None).unwrap();
    assert!(!restored_prev.removed_new_file);
    assert_eq!(fs::read_to_string(&claude_path).unwrap(), first_disk);

    // Restoring the `.absent.bak` removes `.claude.json` back to its pre-creation state
    let restored_absent = ss_mcp_hub::restore_backup(&absent_bak.id, None).unwrap();
    assert!(restored_absent.removed_new_file);
    assert!(!claude_path.exists());
}

#[test]
fn skillstar_zed_config_dir_env_override_is_respected() {
    let sb = SandboxEnv::enter();
    let custom_zed = sb.home().join("custom-zed-dir");
    fs::create_dir_all(&custom_zed).unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_ZED_CONFIG_DIR", &custom_zed);
    }
    let roots = McpPathRoots::from_env();
    let zed = resolve_target_spec(&roots, "zed").unwrap();
    unsafe {
        std::env::remove_var("SKILLSTAR_ZED_CONFIG_DIR");
    }
    assert_eq!(zed.primary_path, custom_zed.join("settings.json"));
}
