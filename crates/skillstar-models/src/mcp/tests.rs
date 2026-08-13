//! Unit tests for MCP server management (pure: in-memory specs/entries + temp-dir IO).

use super::*;

/// Every test below that resolves a real config path shares one sandbox home
/// with the rest of the test process, so they all take the same lock — see
/// `tests_targets::SANDBOX_HOME_TEST_LOCK`.
use super::tests_targets::SANDBOX_HOME_TEST_LOCK as LEGACY_DESKTOP_CONFIG_TEST_LOCK;

fn reset_legacy_desktop_config() -> std::path::PathBuf {
    let path = resolve_legacy_claude_desktop_config_path().unwrap();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::remove_file(&path).ok();
    path
}

fn stdio(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "stdio");
    e.command = Some("npx".into());
    e.args = vec![
        "-y".into(),
        "@modelcontextprotocol/server-filesystem".into(),
    ];
    e.env.insert("HOME".into(), "/Users/test".into());
    e
}

fn http(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "http");
    e.url = Some("https://example.com/mcp".into());
    e.headers
        .insert("Authorization".into(), "Bearer xxx".into());
    e
}

#[test]
fn canonical_stdio_has_type_command_args_env() {
    let v = claude_code_spec(&stdio("fs"));
    assert_eq!(v["type"], "stdio");
    assert_eq!(v["command"], "npx");
    assert_eq!(v["args"][0], "-y");
    assert_eq!(v["env"]["HOME"], "/Users/test");
}

/// Claude has exactly two public targets, one per *surface*, and the legacy
/// tombstone ids are neither of them.
///
/// The Code surface (`~/.claude.json`, served by `claude-code` for both the
/// CLI and Desktop Code) and the Chat surface (`claude_desktop_config.json`)
/// read different files in different wire formats, so one target cannot cover
/// both. `claude-desktop` stays absent because a public target and a
/// once-only cleanup tombstone must never share an id.
#[test]
fn supported_tool_ids_have_one_target_per_claude_surface() {
    assert!(MCP_TOOL_IDS.contains(&"claude-code"));
    assert!(MCP_TOOL_IDS.contains(&CLAUDE_DESKTOP_CHAT_TOOL_ID));
    assert!(!MCP_TOOL_IDS.contains(&LEGACY_CLAUDE_DESKTOP_TOOL_ID));
    assert!(!MCP_TOOL_IDS.contains(&LEGACY_GEMINI_TOOL_ID));
    assert_eq!(
        MCP_TOOL_IDS
            .iter()
            .filter(|tool_id| tool_id.starts_with("claude"))
            .copied()
            .collect::<Vec<_>>(),
        vec!["claude-code", CLAUDE_DESKTOP_CHAT_TOOL_ID]
    );
    // Two ids, two files — the Chat target must not be aimed at Code's config.
    assert_ne!(
        resolve_mcp_config_path("claude-code").unwrap(),
        resolve_mcp_config_path(CLAUDE_DESKTOP_CHAT_TOOL_ID).unwrap()
    );
}

#[test]
fn claude_code_mcp_installation_accepts_each_shared_surface() {
    assert!(claude_code_installed_from_signals(
        true, false, false, false
    ));
    assert!(claude_code_installed_from_signals(
        false, true, false, false
    ));
    assert!(claude_code_installed_from_signals(
        false, false, true, false
    ));
    assert!(claude_code_installed_from_signals(
        false, false, false, true
    ));
    assert!(!claude_code_installed_from_signals(
        false, false, false, false
    ));
}

#[test]
fn opencode_stdio_becomes_local_command_array() {
    let v = opencode_spec(&stdio("fs"));
    assert_eq!(v["type"], "local");
    assert_eq!(v["command"][0], "npx");
    assert_eq!(v["command"][1], "-y");
    assert_eq!(v["environment"]["HOME"], "/Users/test");
    assert_eq!(v["enabled"], true);
}

#[test]
fn opencode_http_becomes_remote() {
    let v = opencode_spec(&http("r"));
    assert_eq!(v["type"], "remote");
    assert_eq!(v["url"], "https://example.com/mcp");
    assert_eq!(v["headers"]["Authorization"], "Bearer xxx");
    assert_eq!(v["enabled"], true);
}

#[test]
fn codex_stdio_table_shape() {
    let t = codex_toml_table(&stdio("fs"));
    assert_eq!(t["type"].as_str(), Some("stdio"));
    assert_eq!(t["command"].as_str(), Some("npx"));
    assert_eq!(t["args"].as_array().unwrap().len(), 2);
    assert_eq!(
        t["env"].as_table().unwrap()["HOME"].as_str(),
        Some("/Users/test")
    );
}

#[test]
fn codex_http_uses_http_headers() {
    let t = codex_toml_table(&http("r"));
    assert_eq!(t["type"].as_str(), Some("http"));
    assert_eq!(t["url"].as_str(), Some("https://example.com/mcp"));
    assert!(t.get("http_headers").is_some());
}

#[test]
fn grok_stdio_omits_type_and_matches_native_shape() {
    let t = grok_toml_table(&stdio("fs"));
    assert!(t.get("type").is_none());
    assert_eq!(t["command"].as_str(), Some("npx"));
    assert_eq!(t["args"].as_array().unwrap().len(), 2);
}

#[test]
fn kiro_projects_auto_approve_all_as_wildcard() {
    let mut e = stdio("fs");
    e.auto_approve_all = true;
    e.disabled_tools = vec!["delete_file".into()];
    let v = kiro_spec(&e);
    assert_eq!(v["autoApprove"], serde_json::json!(["*"]));
    assert_eq!(v["disabledTools"], serde_json::json!(["delete_file"]));
}

#[test]
fn kiro_projects_specific_auto_approve_tools_when_not_all() {
    let mut e = stdio("fs");
    e.auto_approve_tools = vec!["read_file".into(), "list_dir".into()];
    let v = kiro_spec(&e);
    assert_eq!(
        v["autoApprove"],
        serde_json::json!(["read_file", "list_dir"])
    );
}

#[test]
fn codex_projects_disabled_tools_and_timeout_seconds() {
    let mut e = stdio("fs");
    e.disabled_tools = vec!["delete_file".into()];
    e.timeout_ms = Some(30_000);
    let t = codex_toml_table(&e);
    assert_eq!(
        t["disabled_tools"].as_array().unwrap()[0].as_str(),
        Some("delete_file")
    );
    assert_eq!(t["tool_timeout_sec"].as_integer(), Some(30));
}

#[test]
fn opencode_projects_timeout_ms_verbatim() {
    let mut e = stdio("fs");
    e.timeout_ms = Some(8_000);
    let v = opencode_spec(&e);
    assert_eq!(v["timeout"], 8_000);
}

#[test]
fn grok_http_uses_headers_not_http_headers() {
    let t = grok_toml_table(&http("r"));
    assert!(t.get("type").is_none());
    assert_eq!(t["url"].as_str(), Some("https://example.com/mcp"));
    assert!(t.get("headers").is_some());
    assert!(t.get("http_headers").is_none());
}

#[test]
fn create_assigns_id_and_rejects_dupes() {
    let mut store = McpStore::default();
    let e = create_server(&mut store, stdio("fs")).unwrap();
    assert!(!e.id.is_empty());
    assert!(create_server(&mut store, stdio("fs")).is_err());
}

#[test]
fn validate_requires_command_or_url() {
    let mut bad = blank_entry("x", "stdio");
    assert!(validate_entry(&bad).is_err());
    bad.command = Some("echo".into());
    assert!(validate_entry(&bad).is_ok());
    let mut badurl = blank_entry("y", "http");
    assert!(validate_entry(&badurl).is_err());
    badurl.url = Some("https://x".into());
    assert!(validate_entry(&badurl).is_ok());
}

#[test]
fn set_tool_enabled_updates_map() {
    let mut store = McpStore::default();
    let e = create_server(&mut store, stdio("fs")).unwrap();
    let updated = set_tool_enabled(&mut store, &e.id, "codex", true).unwrap();
    assert_eq!(updated.enabled.get("codex"), Some(&true));
    assert!(set_tool_enabled(&mut store, &e.id, "bogus", true).is_err());
}

#[test]
fn store_roundtrip_and_import_parse() {
    // canonical → json spec → parse back
    let e = stdio("fs");
    let spec = claude_code_spec(&e);
    let parsed = entry_from_json_spec("fs", &spec).unwrap();
    assert_eq!(parsed.command, Some("npx".to_string()));
    assert_eq!(parsed.args.len(), 2);
    assert_eq!(parsed.env.get("HOME"), Some(&"/Users/test".to_string()));

    // opencode roundtrip
    let oc = opencode_spec(&e);
    let back = entry_from_opencode_spec("fs", &oc).unwrap();
    assert_eq!(back.command, Some("npx".to_string()));
    assert_eq!(
        back.args,
        vec!["-y", "@modelcontextprotocol/server-filesystem"]
    );
}

#[test]
fn write_then_read_store() {
    let dir = std::env::temp_dir().join(format!("ss-mcp-test-{}", now_ms()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mcp_servers.json");
    let mut store = McpStore::default();
    create_server(&mut store, stdio("fs")).unwrap();
    write_mcp_store(&store, &path).unwrap();
    let loaded = read_mcp_store(&path).unwrap();
    assert_eq!(loaded.servers.len(), 1);
    assert_eq!(loaded.servers[0].name, "fs");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_store_preserves_hidden_legacy_projection_without_broadening_public_scope() {
    let dir = std::env::temp_dir().join(format!("ss-mcp-legacy-test-{}", now_ms()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mcp_servers.json");
    let mut entry = stdio("legacy-desktop-only");
    entry.id = "legacy-id".into();
    entry.enabled.insert("claude-desktop".into(), true);
    let store = McpStore {
        version: 1,
        servers: vec![entry],
    };
    write_mcp_store(&store, &path).unwrap();

    let loaded = read_mcp_store(&path).unwrap();
    let enabled = &loaded.servers[0].enabled;
    assert_eq!(enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID), Some(&true));
    assert_ne!(enabled.get("claude-code"), Some(&true));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn legacy_cleanup_tombstone_is_consumed_and_false_never_authorizes_deletion() {
    let mut entry = stdio("legacy-cleanup-state");
    entry
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);
    mark_legacy_desktop_chat_clean(&mut entry);
    assert_eq!(
        entry.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID),
        Some(&false)
    );
    assert!(cleanup_legacy_desktop_chat(&mut entry).is_none());

    let mut public_only = stdio("public-only-state");
    mark_legacy_desktop_chat_clean(&mut public_only);
    assert!(
        !public_only
            .enabled
            .contains_key(LEGACY_CLAUDE_DESKTOP_TOOL_ID)
    );
}

#[test]
fn legacy_cleanup_preserves_other_desktop_chat_config() {
    let dir = std::env::temp_dir().join(format!("ss-mcp-desktop-cleanup-{}", now_ms()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("claude_desktop_config.json");
    std::fs::write(
        &path,
        r#"{
  "theme": "dark",
  "mcpServers": {
    "legacy-managed": { "command": "old" },
    "user-owned": { "command": "keep" }
  }
}"#,
    )
    .unwrap();

    json_mcpservers_remove_strict(&path, "legacy-managed").unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["theme"], "dark");
    assert!(value["mcpServers"].get("legacy-managed").is_none());
    assert_eq!(value["mcpServers"]["user-owned"]["command"], "keep");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn legacy_cleanup_refuses_to_overwrite_malformed_desktop_chat_config() {
    let dir = std::env::temp_dir().join(format!("ss-mcp-desktop-malformed-{}", now_ms()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("claude_desktop_config.json");
    let original = "{ definitely-not-json";
    std::fs::write(&path, original).unwrap();

    assert!(json_mcpservers_remove_strict(&path, "legacy-managed").is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

    std::fs::remove_dir_all(&dir).ok();
}

/// Renaming an entry that still carries the Desktop Chat tombstone drops the
/// old key, consumes the tombstone, and leaves every key SkillStar does not
/// manage alone.
///
/// The renamed key itself is *absent* afterwards, because this entry leaves
/// [`CLAUDE_DESKTOP_CHAT_TOOL_ID`] off and "off" means "remove this name from
/// that target" for every public target — the file is no longer cleanup-only.
#[test]
fn update_helper_renames_and_consumes_pending_desktop_chat_cleanup() {
    let _guard = LEGACY_DESKTOP_CONFIG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_legacy_desktop_config();
    std::fs::write(
        &path,
        r#"{
  "theme": "dark",
  "mcpServers": {
    "old-name": { "command": "old" },
    "user-owned": { "command": "keep" }
  }
}"#,
    )
    .unwrap();

    let mut entry = stdio("old-name");
    entry.id = "rename-id".into();
    entry
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);
    let mut store = McpStore {
        version: 1,
        servers: vec![entry],
    };
    let (updated, results) = update_server_and_sync(
        &mut store,
        "rename-id",
        McpServerPatch {
            name: Some("new-name".into()),
            ..McpServerPatch::default()
        },
        false,
    )
    .unwrap();

    assert_eq!(updated.name, "new-name");
    assert_eq!(
        updated.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID),
        Some(&false)
    );
    assert!(
        results
            .iter()
            .any(|result| result.tool_id == LEGACY_CLAUDE_DESKTOP_TOOL_ID)
    );
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(value["mcpServers"].get("old-name").is_none());
    assert!(value["mcpServers"].get("new-name").is_none());
    assert_eq!(value["mcpServers"]["user-owned"]["command"], "keep");
    assert_eq!(value["theme"], "dark");
    std::fs::remove_file(path).ok();
}

/// The same rename with the Chat target switched **on** projects the new name
/// instead of removing it, and still consumes the tombstone — subsumption, so
/// the cleanup pass does not undo the write the public pass just made.
#[test]
fn update_helper_keeps_the_renamed_key_when_the_chat_target_is_on() {
    let _guard = LEGACY_DESKTOP_CONFIG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_legacy_desktop_config();
    std::fs::write(
        &path,
        r#"{"mcpServers":{"old-name":{"command":"old"},"user-owned":{"command":"keep"}}}"#,
    )
    .unwrap();

    let mut entry = stdio("old-name");
    entry.id = "rename-id".into();
    entry
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);
    entry
        .enabled
        .insert(CLAUDE_DESKTOP_CHAT_TOOL_ID.into(), true);
    let mut store = McpStore {
        version: 1,
        servers: vec![entry],
    };
    let (updated, _) = update_server_and_sync(
        &mut store,
        "rename-id",
        McpServerPatch {
            name: Some("new-name".into()),
            ..McpServerPatch::default()
        },
        true,
    )
    .unwrap();

    assert_eq!(
        updated.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID),
        Some(&false)
    );
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(value["mcpServers"].get("old-name").is_none());
    assert_eq!(value["mcpServers"]["new-name"]["command"], "npx");
    assert!(value["mcpServers"]["new-name"].get("type").is_none());
    assert_eq!(value["mcpServers"]["user-owned"]["command"], "keep");
    std::fs::remove_file(path).ok();
}

#[test]
fn malformed_desktop_chat_config_keeps_update_and_delete_store_evidence() {
    let _guard = LEGACY_DESKTOP_CONFIG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_legacy_desktop_config();
    let malformed = "{ definitely-not-json";
    std::fs::write(&path, malformed).unwrap();

    let mut entry = stdio("retryable-name");
    entry.id = "retryable-id".into();
    entry
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);
    let original = McpStore {
        version: 1,
        servers: vec![entry],
    };

    let mut update_store = original.clone();
    assert!(
        update_server_and_sync(
            &mut update_store,
            "retryable-id",
            McpServerPatch {
                name: Some("lost-name".into()),
                ..McpServerPatch::default()
            },
            false,
        )
        .is_err()
    );
    assert_eq!(update_store.servers, original.servers);

    let mut delete_store = original.clone();
    assert!(delete_server_and_sync(&mut delete_store, "retryable-id").is_err());
    assert_eq!(delete_store.servers, original.servers);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), malformed);
    std::fs::remove_file(path).ok();
}

#[test]
fn mcp_presets_catalog_is_well_formed() {
    let presets = get_mcp_presets();
    assert!(presets.len() >= 10, "preset catalog should be substantial");

    let mut seen = std::collections::HashSet::new();
    for p in &presets {
        assert!(!p.name.is_empty(), "preset '{}' has empty name", p.id);
        assert!(seen.insert(p.id.clone()), "duplicate preset id '{}'", p.id);
        match p.transport.as_str() {
            "http" | "sse" => assert!(p.url.is_some(), "remote preset '{}' must carry a url", p.id),
            _ => assert!(
                p.command.is_some(),
                "stdio preset '{}' must carry a command",
                p.id
            ),
        }
        // Every required_env key must exist in the env map so the form can
        // surface it as a blank-to-fill field.
        for key in &p.required_env {
            assert!(
                p.env.contains_key(key),
                "preset '{}' lists required env '{key}' missing from env map",
                p.id
            );
        }
    }
}

#[test]
fn kiro_resolves_to_settings_mcp_json() {
    // Kiro reads user-scope MCP from ~/.kiro/settings/mcp.json (top-level mcpServers).
    let p = resolve_mcp_config_path("kiro").unwrap();
    assert!(
        p.ends_with(".kiro/settings/mcp.json"),
        "unexpected kiro path: {}",
        p.display()
    );
}

#[test]
fn cursor_resolves_to_mcp_json() {
    // Cursor reads user-scope MCP from ~/.cursor/mcp.json (top-level mcpServers).
    let p = resolve_mcp_config_path("cursor").unwrap();
    assert!(
        p.ends_with(".cursor/mcp.json"),
        "unexpected cursor path: {}",
        p.display()
    );
}

#[test]
fn cursor_spec_matches_community_canonical_shape() {
    // Cursor uses the plain community mcpServers shape (type/command/args/env),
    // with no per-server approval/exposure/timeout fields projected.
    let v = cursor_spec(&stdio("fs"));
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("stdio"));
    assert_eq!(v.get("command").and_then(|x| x.as_str()), Some("npx"));
    assert_eq!(
        v.get("args").and_then(|x| x.as_array()).map(|a| a.len()),
        Some(2)
    );
    assert!(v.get("autoApprove").is_none());
    assert!(v.get("disabledTools").is_none());
    assert!(v.get("timeout").is_none());
}

#[test]
fn zcode_cli_spec_matches_community_stdio() {
    let v = zcode_cli_spec(&stdio("ads-mcp"));
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("stdio"));
    assert_eq!(v.get("command").and_then(|x| x.as_str()), Some("npx"));
}

// ── sync projection tests ───────────────────────────────────────────────
// `sync_server_to_tool` is the bridge from a stored MCP server entry to a
// tool's live config file. These cover the two non-writing branches that are
// easy to regress: unknown tool id, and force=false on an absent tool.

#[test]
fn sync_to_unknown_tool_errors_instead_of_silently_passing() {
    // An unsupported tool_id must surface an error, not quietly succeed —
    // otherwise a typo would look like the server was deployed.
    let result = sync_server_to_tool(&stdio("fs"), "not-a-real-tool", true);
    assert!(
        !result.success || result.error.is_some(),
        "unknown tool_id should not report clean success"
    );
}

#[test]
fn sync_all_returns_one_result_per_known_tool() {
    // sync_server_all_tools iterates MCP_TOOL_IDS (one per supported tool), so
    // the result vector length is a stable contract the UI depends on.
    let _guard = LEGACY_DESKTOP_CONFIG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut entry = stdio("fs");
    let results = sync_server_all_tools(&mut entry, false);
    assert_eq!(
        results.len(),
        MCP_TOOL_IDS.len(),
        "one result per known tool id"
    );
    // Every result must carry its tool_id back for the UI to map.
    for r in &results {
        assert!(
            MCP_TOOL_IDS.contains(&r.tool_id.as_str()),
            "result tool_id '{}' must be a known id",
            r.tool_id
        );
    }
}

#[test]
fn sync_all_keeps_legacy_cleanup_internal_and_only_for_existing_entries() {
    let _guard = LEGACY_DESKTOP_CONFIG_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_legacy_desktop_config();
    let mut public = stdio("public-only");
    let public_results = sync_server_all_tools(&mut public, false);
    assert_eq!(public_results.len(), MCP_TOOL_IDS.len());

    let mut legacy = stdio("legacy-managed");
    legacy
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);
    let results = sync_server_all_tools(&mut legacy, false);
    assert_eq!(results.len(), MCP_TOOL_IDS.len() + 1);
    assert_eq!(
        results.last().map(|result| result.tool_id.as_str()),
        Some(LEGACY_CLAUDE_DESKTOP_TOOL_ID)
    );
    assert_eq!(
        legacy.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID),
        Some(&false)
    );

    let mut gemini_legacy = stdio("gemini-managed");
    gemini_legacy
        .enabled
        .insert(LEGACY_GEMINI_TOOL_ID.into(), true);
    let gemini_results = sync_server_all_tools(&mut gemini_legacy, false);
    assert_eq!(gemini_results.len(), MCP_TOOL_IDS.len() + 1);
    assert_eq!(
        gemini_results.last().map(|result| result.tool_id.as_str()),
        Some(LEGACY_GEMINI_TOOL_ID)
    );
    assert_eq!(
        gemini_legacy.enabled.get(LEGACY_GEMINI_TOOL_ID),
        Some(&false)
    );
    std::fs::remove_file(path).ok();
}
