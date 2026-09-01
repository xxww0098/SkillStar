//! Integrity tests: the store version gate, the input-validation policy, and
//! the rollback contract.
//!
//! These all answer the same question from different angles — after something
//! goes wrong, is the user's data still what it was?

use super::tests_targets::{TempDir, SANDBOX_HOME_TEST_LOCK};
use super::*;
use serde_json::Value;

fn stdio(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "stdio");
    e.command = Some("npx".into());
    e
}

fn http(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "http");
    e.url = Some("https://example.com/mcp".into());
    e
}

// ---------------------------------------------------------------------------
// Store schema version gate (audit R1 #3)
// ---------------------------------------------------------------------------

/// A store written by a newer SkillStar must not be opened by this build.
///
/// Serde would read it happily, dropping every field this build does not know
/// about — and the next save would write that lossy copy back over the user's
/// real data. Refusing is the only non-destructive option.
#[test]
fn a_store_from_a_newer_schema_is_refused_rather_than_downgraded() {
    let dir = TempDir::new("store-newer");
    let path = dir.path().join("mcp_servers.json");
    let future = serde_json::json!({
        "version": MCP_STORE_VERSION + 1,
        "servers": [{ "name": "kept", "transport": "stdio", "command": "npx" }],
    });
    std::fs::write(&path, serde_json::to_string_pretty(&future).unwrap()).unwrap();
    let before = std::fs::read(&path).unwrap();

    let err = read_mcp_store(&path).unwrap_err().to_string();
    assert!(err.contains("newer version"), "{err}");
    assert!(err.contains(&(MCP_STORE_VERSION + 1).to_string()), "{err}");
    // The refusal must not have touched the file.
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

/// A store from an older schema is upgraded and written straight back, so the
/// on-disk version never lags the code that owns it.
#[test]
fn a_store_from_an_older_schema_is_upgraded_and_persisted() {
    let dir = TempDir::new("store-older");
    let path = dir.path().join("mcp_servers.json");
    let old = serde_json::json!({
        "version": 0,
        "servers": [{ "name": "kept", "transport": "stdio", "command": "npx" }],
    });
    std::fs::write(&path, serde_json::to_string_pretty(&old).unwrap()).unwrap();

    let store = read_mcp_store(&path).unwrap();
    assert_eq!(store.version, MCP_STORE_VERSION);
    assert_eq!(store.servers.len(), 1, "the upgrade must not lose servers");

    let persisted: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(persisted["version"], MCP_STORE_VERSION);
    assert_eq!(persisted["servers"][0]["name"], "kept");
}

/// The provenance fields are purely additive: a store written before they
/// existed must still parse, and must not gain junk values.
#[test]
fn a_store_without_the_provenance_fields_still_parses() {
    let dir = TempDir::new("store-legacy-fields");
    let path = dir.path().join("mcp_servers.json");
    // Exactly the shape an older release wrote — no sourceId, no
    // registryName, no installedVersion, no runtimeKind.
    std::fs::write(
        &path,
        r#"{"version":1,"servers":[{"id":"abc","name":"legacy","transport":"stdio","command":"npx","enabled":{"cursor":true}}]}"#,
    )
    .unwrap();

    let store = read_mcp_store(&path).unwrap();
    let entry = &store.servers[0];
    assert_eq!(entry.name, "legacy");
    assert_eq!(entry.source_id, None);
    assert_eq!(entry.registry_name, None);
    assert_eq!(entry.installed_version, None);
    assert_eq!(entry.runtime_kind, None);
    assert_eq!(entry.enabled.get("cursor"), Some(&true));
}

/// Provenance survives a write/read cycle, and absent values stay absent in
/// the serialized form rather than becoming explicit nulls.
#[test]
fn provenance_round_trips_and_stays_omitted_when_unset() {
    let dir = TempDir::new("store-provenance");
    let path = dir.path().join("mcp_servers.json");
    let mut store = McpStore::default();

    let mut tagged = stdio("tagged");
    tagged.source_id = Some("official".into());
    tagged.registry_name = Some("io.github.example/server".into());
    tagged.installed_version = Some("1.4.0".into());
    tagged.runtime_kind = Some(McpRuntimeKind::PackageOci.as_str().to_string());
    create_server(&mut store, tagged).unwrap();
    create_server(&mut store, stdio("plain")).unwrap();
    write_mcp_store(&store, &path).unwrap();

    let raw: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(raw["servers"][1].get("sourceId").is_none(), "{raw}");

    let read = read_mcp_store(&path).unwrap();
    let tagged = read.servers.iter().find(|s| s.name == "tagged").unwrap();
    assert_eq!(tagged.source_id.as_deref(), Some("official"));
    assert_eq!(
        tagged.registry_name.as_deref(),
        Some("io.github.example/server")
    );
    assert_eq!(tagged.installed_version.as_deref(), Some("1.4.0"));
    assert_eq!(
        McpRuntimeKind::from_stored(tagged.runtime_kind.as_deref().unwrap()),
        Some(McpRuntimeKind::PackageOci)
    );
}

/// An unrecognized runtime token (written by a newer release) must never make
/// the store unreadable — provenance is advisory.
#[test]
fn an_unknown_runtime_kind_parses_as_none_without_erroring() {
    assert_eq!(McpRuntimeKind::from_stored("package-something-new"), None);
    assert_eq!(
        McpRuntimeKind::from_stored(McpRuntimeKind::RemoteStreamableHttp.as_str()),
        Some(McpRuntimeKind::RemoteStreamableHttp)
    );
}

// ---------------------------------------------------------------------------
// Input validation policy (audit R3)
// ---------------------------------------------------------------------------

#[test]
fn new_names_must_survive_being_used_as_a_config_key() {
    for bad in [
        "has space",
        "has/slash",
        "has:colon",
        "quote\"inside",
        " leading",
        "trailing ",
        "",
    ] {
        assert!(
            validate_server_name(bad).is_err(),
            "'{bad}' should be rejected as a server name"
        );
    }
    for good in ["fs", "server-one", "server_two", "io.example.server", "a1"] {
        validate_server_name(good).unwrap_or_else(|e| panic!("'{good}' should be accepted: {e}"));
    }
}

/// Claude Code silently skips servers using its reserved names, so accepting
/// one would produce an entry that looks installed and never appears.
#[test]
fn claude_code_reserved_names_are_rejected() {
    for reserved in ["workspace", "computer-use", "Claude Preview"] {
        assert!(
            validate_server_name(reserved).is_err(),
            "'{reserved}' is reserved by Claude Code"
        );
    }
}

#[test]
fn remote_urls_must_be_absolute_http_endpoints() {
    for bad in [
        "not a url",
        "ftp://example.com/mcp",
        "ws://example.com/mcp",
        "/relative/path",
        // `https:///nohost` is deliberately absent: the WHATWG parser
        // normalizes it to host `nohost`, so it is a real URL, not a typo the
        // validator should be catching.
        "https://",
        "http://:8080/x",
    ] {
        assert!(
            validate_server_url(bad).is_err(),
            "'{bad}' should be rejected"
        );
    }
    validate_server_url("https://example.com/mcp").unwrap();
    validate_server_url("http://127.0.0.1:8080/mcp").unwrap();
}

#[test]
fn env_keys_follow_the_portable_posix_shape() {
    for bad in ["", "1START", "has space", "has=equals", "has-dash"] {
        assert!(validate_env_key(bad).is_err(), "'{bad}' should be rejected");
    }
    for good in ["API_KEY", "_private", "PORT2"] {
        validate_env_key(good).unwrap();
    }
}

/// A newline in a header value is header injection, and these values are
/// written into files other agents send as real HTTP headers.
#[test]
fn header_names_are_tokens_and_values_may_not_contain_line_breaks() {
    assert!(validate_header_name("Authorization").is_ok());
    assert!(validate_header_name("X-Custom_Header").is_ok());
    assert!(validate_header_name("bad header").is_err());
    assert!(validate_header_name("bad:header").is_err());
    assert!(validate_header_name("").is_err());

    assert!(validate_header_value("Authorization", "Bearer abc").is_ok());
    assert!(validate_header_value("Authorization", "Bearer abc\r\nX-Evil: 1").is_err());
    assert!(validate_header_value("Authorization", "line\nbreak").is_err());
}

#[test]
fn creating_a_server_applies_the_strict_policy() {
    let mut store = McpStore::default();
    let mut bad_name = stdio("has space");
    bad_name.name = "has space".into();
    assert!(create_server(&mut store, bad_name).is_err());

    let mut bad_url = http("remote");
    bad_url.url = Some("ftp://example.com".into());
    assert!(create_server(&mut store, bad_url).is_err());

    let mut bad_env = stdio("ok-name");
    bad_env.env.insert("BAD KEY".into(), "v".into());
    assert!(create_server(&mut store, bad_env).is_err());

    create_server(&mut store, stdio("good-name")).unwrap();
}

/// The whole reason input validation is a separate function: an entry saved by
/// an older release keeps syncing. Rejecting it here would strand the key it
/// already wrote into a dozen tool configs, since removal is keyed on the very
/// name being rejected.
#[test]
fn a_legacy_name_still_passes_the_sync_path_invariant() {
    let mut legacy = stdio("legacy name with spaces");
    legacy.name = "legacy name with spaces".into();
    validate_entry(&legacy).expect("the sync-path invariant must still accept legacy names");
    validate_entry_input(&legacy).expect_err("but the input policy must reject it for new entries");
}

/// A legacy entry can still be edited: the name rule only applies to a name
/// the edit actually changes. Otherwise the user could not fix anything about
/// the entry — including the name itself.
#[test]
fn editing_a_legacy_entry_is_allowed_but_renaming_to_another_bad_name_is_not() {
    let mut store = McpStore::default();
    store.servers.push({
        let mut e = stdio("legacy name");
        e.id = "legacy-id".into();
        e.name = "legacy name".into();
        e
    });

    // Editing another field leaves the grandfathered name alone.
    let patch = McpServerPatch {
        description: Some("now documented".into()),
        ..Default::default()
    };
    update_server(&mut store, "legacy-id", patch).expect("an unrelated edit must be allowed");

    // Renaming to a valid name is the escape hatch.
    let rename = McpServerPatch {
        name: Some("legacy-name".into()),
        ..Default::default()
    };
    update_server(&mut store, "legacy-id", rename).expect("renaming to a valid name must work");

    // Renaming to another invalid name is still refused.
    let bad_rename = McpServerPatch {
        name: Some("still bad".into()),
        ..Default::default()
    };
    assert!(update_server(&mut store, "legacy-id", bad_rename).is_err());
}

// ---------------------------------------------------------------------------
// Rollback (audit F3 / F4 / F6)
// ---------------------------------------------------------------------------

#[test]
fn restoring_from_a_backup_puts_the_original_bytes_back() {
    let dir = TempDir::new("restore");
    let path = dir.path().join("config.json");
    std::fs::write(&path, r#"{"kept":"original"}"#).unwrap();
    let backup = backup_if_exists(&path)
        .unwrap()
        .expect("a backup was taken");

    std::fs::write(&path, "clobbered").unwrap();
    restore_from_backup(&path, Some(&backup)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"kept":"original"}"#
    );
}

/// With no backup there was no file, so "back to before" means the file the
/// failed attempt created must be gone — not left behind as a stub.
#[test]
fn restoring_with_no_backup_removes_a_file_the_attempt_created() {
    let dir = TempDir::new("restore-none");
    let path = dir.path().join("created.json");
    assert!(backup_if_exists(&path).unwrap().is_none());

    std::fs::write(&path, "half written").unwrap();
    restore_from_backup(&path, None).unwrap();
    assert!(!path.exists(), "the created file should have been removed");

    // Idempotent: nothing to undo is not an error.
    restore_from_backup(&path, None).unwrap();
}

/// A failed projection must leave that tool's config byte-for-byte intact and
/// say so, rather than reporting a bare failure and leaving the backup unused.
#[test]
fn a_failed_projection_rolls_the_tool_config_back_and_reports_it() {
    // Goes through the real resolver, so it shares the process-wide sandbox
    // home with every other resolver-driven test.
    let _guard = SANDBOX_HOME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = resolve_windsurf_config_path().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let malformed = "{ \"mcpServers\": { \"kept\": {} },, oops";
    std::fs::write(&path, malformed).unwrap();

    let result = sync_server_to_tool(&stdio("rollback-probe"), "windsurf", true);

    assert!(
        !result.success,
        "a malformed config must not be overwritten"
    );
    assert!(result.rolled_back, "the failed write must be undone");
    assert!(
        result.rollback_error.is_none(),
        "{:?}",
        result.rollback_error
    );
    assert!(
        result.backup_path.is_some(),
        "a backup must have been taken"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        malformed,
        "the user's file must be untouched"
    );
    assert!(result.is_settled(), "a fully undone write is settled");

    std::fs::remove_file(&path).ok();
}

/// The consistency summary is how a caller learns a batch was only partly
/// applied — the audit's "silent drift" case.
#[test]
fn consistency_separates_applied_rolled_back_and_drifted() {
    let mut applied = McpSyncResult::pending("cursor", "s");
    applied.success = true;
    let mut skipped = McpSyncResult::pending("kiro", "s");
    skipped.success = true;
    skipped.skipped = true;
    let mut undone = McpSyncResult::pending("zed", "s");
    undone.error = Some("boom".into());
    undone.rolled_back = true;
    let mut drifted = McpSyncResult::pending("cline", "s");
    drifted.error = Some("boom".into());
    drifted.rollback_error = Some("could not restore".into());

    let verdict = mcp_sync_consistency(&[applied, skipped, undone, drifted]);
    assert!(!verdict.consistent);
    assert_eq!(verdict.applied, vec!["cursor", "kiro"]);
    assert_eq!(verdict.rolled_back, vec!["zed"]);
    assert_eq!(verdict.drifted, vec!["cline"]);

    let mut ok = McpSyncResult::pending("cursor", "s");
    ok.success = true;
    assert!(mcp_sync_consistency(&[ok]).consistent);
}

// ---------------------------------------------------------------------------
// Gemini: legacy tombstone vs the public target
// ---------------------------------------------------------------------------

fn reset_gemini_settings(content: &str) -> std::path::PathBuf {
    let path = resolve_gemini_cli_config_path().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

/// The tombstone and the public target write the same file. When both are set,
/// deleting would erase the projection the public target just wrote — so the
/// tombstone is consumed without touching the file.
#[test]
fn a_gemini_tombstone_is_subsumed_by_the_public_target() {
    let _guard = SANDBOX_HOME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_gemini_settings(r#"{"mcpServers":{"shared":{"command":"npx"}}}"#);

    let mut entry = stdio("shared");
    entry.enabled.insert(LEGACY_GEMINI_TOOL_ID.into(), true);
    entry.enabled.insert(GEMINI_CLI_TOOL_ID.into(), true);

    let cleanup = cleanup_legacy_gemini(&mut entry).expect("the tombstone must be consumed");
    assert!(cleanup.success);
    assert!(cleanup.skipped, "subsumption is a deliberate no-op");
    assert_eq!(entry.enabled.get(LEGACY_GEMINI_TOOL_ID), Some(&false));

    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        root["mcpServers"].get("shared").is_some(),
        "the live projection must survive: {root}"
    );
    std::fs::remove_file(&path).ok();
}

/// Without the public target the tombstone still does its original job.
#[test]
fn a_gemini_tombstone_alone_still_removes_the_old_projection() {
    let _guard = SANDBOX_HOME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_gemini_settings(
        r#"{"mcpServers":{"orphan":{"command":"npx"},"other":{"command":"uvx"}}}"#,
    );

    let mut entry = stdio("orphan");
    entry.enabled.insert(LEGACY_GEMINI_TOOL_ID.into(), true);

    let cleanup = cleanup_legacy_gemini(&mut entry).expect("the tombstone must run");
    assert!(cleanup.success);
    assert!(!cleanup.skipped);
    assert_eq!(entry.enabled.get(LEGACY_GEMINI_TOOL_ID), Some(&false));

    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(root["mcpServers"].get("orphan").is_none(), "{root}");
    assert!(
        root["mcpServers"].get("other").is_some(),
        "unrelated servers must be preserved: {root}"
    );
    std::fs::remove_file(&path).ok();
}

/// End to end: an entry carrying both flags keeps its Gemini projection after
/// a full sync, even though the public pass runs before the cleanup pass.
#[test]
fn syncing_all_tools_keeps_the_gemini_projection_when_both_flags_are_set() {
    let _guard = SANDBOX_HOME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_gemini_settings("{}");

    let mut entry = stdio("both-flags");
    entry.enabled.insert(LEGACY_GEMINI_TOOL_ID.into(), true);
    entry.enabled.insert(GEMINI_CLI_TOOL_ID.into(), true);
    let results = sync_server_all_tools(&mut entry, true);

    let gemini = results
        .iter()
        .find(|r| r.tool_id == GEMINI_CLI_TOOL_ID)
        .expect("gemini-cli must be a public target");
    assert!(gemini.success, "{:?}", gemini.error);

    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        root["mcpServers"].get("both-flags").is_some(),
        "the tombstone must not delete what the public target wrote: {root}"
    );
    std::fs::remove_file(&path).ok();
}

// ---------------------------------------------------------------------------
// Claude Desktop Chat: legacy tombstone vs the public target
// ---------------------------------------------------------------------------

fn reset_desktop_chat_config(content: &str) -> std::path::PathBuf {
    let path = resolve_claude_desktop_chat_config_path().unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    path
}

/// The public target's config path must be resolved inside the tool-sync
/// sandbox, never the real `$HOME` / real OS config dir.
///
/// It also has to be the *same* file the tombstone cleans, which is the whole
/// reason the subsumption rule below exists.
#[test]
fn the_desktop_chat_config_path_resolves_inside_the_sandbox_home() {
    let path = resolve_claude_desktop_chat_config_path().unwrap();

    assert!(path.is_absolute(), "{path:?}");
    assert!(
        path.ends_with("Claude/claude_desktop_config.json"),
        "{path:?}"
    );
    assert_eq!(
        path,
        resolve_mcp_config_path(LEGACY_CLAUDE_DESKTOP_TOOL_ID).unwrap(),
        "the tombstone and the public target must own the same file"
    );
    assert_eq!(
        path,
        resolve_mcp_config_path(CLAUDE_DESKTOP_CHAT_TOOL_ID).unwrap()
    );

    // The sandbox root is a temp dir; the real home must be nowhere near it.
    assert!(
        path.starts_with(std::env::temp_dir()),
        "{path:?} escaped the tool-sync sandbox"
    );
    if let Some(real_home) = dirs::home_dir() {
        assert!(
            !path.starts_with(&real_home),
            "{path:?} points at the real home"
        );
    }
}

/// Hermes lives under `~/.hermes/config.yaml` inside the sandbox home.
#[test]
fn the_hermes_config_path_resolves_inside_the_sandbox_home() {
    let path = resolve_hermes_config_path().unwrap();
    assert!(path.is_absolute(), "{path:?}");
    assert!(path.ends_with(".hermes/config.yaml"), "{path:?}");
    assert_eq!(path, resolve_mcp_config_path("hermes").unwrap());
    assert!(
        path.starts_with(std::env::temp_dir()),
        "{path:?} escaped the tool-sync sandbox"
    );
}

/// Antigravity's default (no migration marker) is the legacy path, still
/// inside the sandbox and never Gemini CLI's `settings.json`.
#[test]
fn the_antigravity_config_path_resolves_inside_the_sandbox_home() {
    let path = resolve_antigravity_config_path().unwrap();
    assert!(path.is_absolute(), "{path:?}");
    assert!(
        path.ends_with(".gemini/antigravity/mcp_config.json"),
        "{path:?}"
    );
    assert_eq!(path, resolve_mcp_config_path("antigravity").unwrap());
    assert_ne!(path, resolve_gemini_cli_config_path().unwrap());
    assert!(
        path.starts_with(std::env::temp_dir()),
        "{path:?} escaped the tool-sync sandbox"
    );
}

/// Maka's MCP file lives under the OS config dir, same sandbox as Desktop Chat.
#[test]
fn the_maka_config_path_resolves_inside_the_sandbox_home() {
    let path = resolve_maka_config_path().unwrap();

    assert!(path.is_absolute(), "{path:?}");
    assert!(
        path.ends_with("Maka/workspaces/default/mcp.json"),
        "{path:?}"
    );
    assert_eq!(path, resolve_mcp_config_path("maka").unwrap());
    assert!(
        path.starts_with(std::env::temp_dir()),
        "{path:?} escaped the tool-sync sandbox"
    );
    if let Some(real_home) = dirs::home_dir() {
        assert!(
            !path.starts_with(&real_home),
            "{path:?} points at the real home"
        );
    }
}

/// The tombstone and the public target write the same file. When both are set,
/// deleting would erase the projection the public target just wrote — so the
/// tombstone is consumed without touching the file.
#[test]
fn a_desktop_chat_tombstone_is_subsumed_by_the_public_target() {
    let _guard = SANDBOX_HOME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_desktop_chat_config(r#"{"mcpServers":{"shared":{"command":"npx"}}}"#);

    let mut entry = stdio("shared");
    entry
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);
    entry
        .enabled
        .insert(CLAUDE_DESKTOP_CHAT_TOOL_ID.into(), true);

    let cleanup = cleanup_legacy_desktop_chat(&mut entry).expect("the tombstone must be consumed");
    assert!(cleanup.success);
    assert!(cleanup.skipped, "subsumption is a deliberate no-op");
    assert_eq!(
        entry.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID),
        Some(&false)
    );

    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        root["mcpServers"].get("shared").is_some(),
        "the live projection must survive: {root}"
    );
    std::fs::remove_file(&path).ok();
}

/// Without the public target the tombstone still does its original job.
#[test]
fn a_desktop_chat_tombstone_alone_still_removes_the_old_projection() {
    let _guard = SANDBOX_HOME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_desktop_chat_config(
        r#"{"theme":"dark","mcpServers":{"orphan":{"command":"npx"},"other":{"command":"uvx"}}}"#,
    );

    let mut entry = stdio("orphan");
    entry
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);

    let cleanup = cleanup_legacy_desktop_chat(&mut entry).expect("the tombstone must run");
    assert!(cleanup.success);
    assert!(!cleanup.skipped);
    assert_eq!(
        entry.enabled.get(LEGACY_CLAUDE_DESKTOP_TOOL_ID),
        Some(&false)
    );

    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(root["mcpServers"].get("orphan").is_none(), "{root}");
    assert!(
        root["mcpServers"].get("other").is_some(),
        "unrelated servers must be preserved: {root}"
    );
    assert_eq!(root["theme"], "dark", "unrelated settings must survive");
    std::fs::remove_file(&path).ok();
}

/// End to end: an entry carrying both flags keeps its Desktop Chat projection
/// after a full sync, even though the public pass runs before the cleanup pass.
#[test]
fn syncing_all_tools_keeps_the_desktop_chat_projection_when_both_flags_are_set() {
    let _guard = SANDBOX_HOME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let path = reset_desktop_chat_config("{}");

    let mut entry = stdio("both-flags");
    entry
        .enabled
        .insert(LEGACY_CLAUDE_DESKTOP_TOOL_ID.into(), true);
    entry
        .enabled
        .insert(CLAUDE_DESKTOP_CHAT_TOOL_ID.into(), true);
    let results = sync_server_all_tools(&mut entry, true);

    let desktop = results
        .iter()
        .find(|r| r.tool_id == CLAUDE_DESKTOP_CHAT_TOOL_ID)
        .expect("claude-desktop-chat must be a public target");
    assert!(desktop.success, "{:?}", desktop.error);

    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        root["mcpServers"].get("both-flags").is_some(),
        "the tombstone must not delete what the public target wrote: {root}"
    );
    // The Chat format carries no `type`; writing Claude Code's would be wrong.
    assert!(
        root["mcpServers"]["both-flags"].get("type").is_none(),
        "{root}"
    );
    std::fs::remove_file(&path).ok();
}
