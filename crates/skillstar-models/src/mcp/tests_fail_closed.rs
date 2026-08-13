//! Fail-closed IO tests: a malformed config or store must never be silently
//! replaced by SkillStar's own serialization.
//!
//! Split out of `tests.rs` (already ~570 lines) so neither file approaches the
//! repo's file-size threshold.
//!
//! Every case here drives the writers through an **explicit** temp path, so no
//! test in this module resolves `$HOME` at all. The paths that do resolve a home
//! (`resolve_claude_json_path` and friends) are additionally covered by the
//! crate's `cfg(test)` sandbox in `tool_sync::sandbox_home`, which redirects
//! `~` into a per-process temp dir even when `SKILLSTAR_TOOL_SYNC_HOME` is unset.

use super::*;

/// A throwaway directory owned by one test, plus a helper to seed files in it.
struct TempConfigDir {
    dir: std::path::PathBuf,
}

impl TempConfigDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ss-mcp-failclosed-{tag}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    /// Write `content` to `name` inside the sandbox and hand back its path.
    fn seed(&self, name: &str, content: &str) -> std::path::PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }
}

impl Drop for TempConfigDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn read_bytes(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

// ---------------------------------------------------------------------------
// Live tool configs (B.4-a / F2): malformed file → Err, bytes untouched
// ---------------------------------------------------------------------------

/// `~/.claude.json` carries most of Claude Code's user settings. A syntax error
/// there used to make one MCP toggle rewrite the whole file as `{"mcpServers": …}`.
#[test]
fn json_mcpservers_upsert_refuses_to_rewrite_malformed_config() {
    let sandbox = TempConfigDir::new("claude");
    let malformed = "{ \"mcpServers\": { \"kept\": {} },, oops";
    let path = sandbox.seed("claude.json", malformed);
    let before = read_bytes(&path);

    let err = json_mcpservers_upsert(&path, "new-server", serde_json::json!({"command": "npx"}))
        .expect_err("malformed JSON must not be overwritten");

    assert!(
        format!("{err:#}").contains("Invalid JSON"),
        "error should name the problem: {err:#}"
    );
    assert_eq!(
        read_bytes(&path),
        before,
        "file must be byte-for-byte intact"
    );
}

#[test]
fn json_mcpservers_remove_refuses_to_rewrite_malformed_config() {
    let sandbox = TempConfigDir::new("claude-remove");
    let path = sandbox.seed("claude.json", "{ definitely-not-json");
    let before = read_bytes(&path);

    assert!(json_mcpservers_remove(&path, "anything").is_err());
    assert_eq!(read_bytes(&path), before);
}

/// A hand-written non-object `mcpServers` is a user value too — refuse rather
/// than replace it.
#[test]
fn json_mcpservers_upsert_refuses_non_object_mcp_servers() {
    let sandbox = TempConfigDir::new("claude-scalar");
    let path = sandbox.seed("claude.json", r#"{"mcpServers": true, "theme": "dark"}"#);
    let before = read_bytes(&path);

    let err = json_mcpservers_upsert(&path, "new-server", serde_json::json!({"command": "npx"}))
        .expect_err("non-object mcpServers must not be replaced");

    assert!(format!("{err:#}").contains("mcpServers"), "{err:#}");
    assert_eq!(read_bytes(&path), before);
}

#[test]
fn opencode_upsert_refuses_to_rewrite_malformed_config() {
    let sandbox = TempConfigDir::new("opencode");
    let path = sandbox.seed("opencode.json", "{ \"mcp\": { \"kept\": {} }, ,");
    let before = read_bytes(&path);

    assert!(opencode_upsert(&path, "new-server", serde_json::json!({"type": "local"})).is_err());
    assert_eq!(read_bytes(&path), before);

    assert!(opencode_remove(&path, "kept").is_err());
    assert_eq!(read_bytes(&path), before);
}

#[test]
fn zcode_cli_upsert_refuses_to_rewrite_malformed_config() {
    let sandbox = TempConfigDir::new("zcode");
    let path = sandbox.seed("config.json", "not json at all");
    let before = read_bytes(&path);

    assert!(zcode_cli_upsert(&path, "new-server", serde_json::json!({"command": "npx"})).is_err());
    assert_eq!(read_bytes(&path), before);

    assert!(zcode_cli_remove(&path, "new-server").is_err());
    assert_eq!(read_bytes(&path), before);
}

#[test]
fn codex_upsert_refuses_to_rewrite_malformed_toml() {
    let sandbox = TempConfigDir::new("codex");
    let malformed = "model = \"gpt-5\"\n[mcp_servers.kept\ncommand = \"npx\"\n";
    let path = sandbox.seed("config.toml", malformed);
    let before = read_bytes(&path);

    let mut table = toml::Table::new();
    table.insert("command".into(), toml::Value::String("npx".into()));

    let err = codex_upsert(&path, "new-server", table)
        .expect_err("malformed TOML must not be overwritten");
    assert!(format!("{err:#}").contains("Invalid TOML"), "{err:#}");
    assert_eq!(read_bytes(&path), before);

    assert!(codex_remove(&path, "kept").is_err());
    assert_eq!(read_bytes(&path), before);
}

/// The fail-closed rule must not break the ordinary paths: a missing file is
/// still created, and an existing well-formed file keeps its unrelated keys.
#[test]
fn upserts_still_create_missing_files_and_preserve_foreign_keys() {
    let sandbox = TempConfigDir::new("happy");

    let fresh = sandbox.dir.join("nested").join("claude.json");
    json_mcpservers_upsert(&fresh, "srv", serde_json::json!({"command": "npx"})).unwrap();
    let created: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fresh).unwrap()).unwrap();
    assert_eq!(created["mcpServers"]["srv"]["command"], "npx");

    let existing = sandbox.seed(
        "existing.json",
        r#"{"theme":"dark","mcpServers":{"user-owned":{"command":"keep"}}}"#,
    );
    json_mcpservers_upsert(&existing, "srv", serde_json::json!({"command": "npx"})).unwrap();
    let merged: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&existing).unwrap()).unwrap();
    assert_eq!(merged["theme"], "dark");
    assert_eq!(merged["mcpServers"]["user-owned"]["command"], "keep");
    assert_eq!(merged["mcpServers"]["srv"]["command"], "npx");

    // An empty file is indistinguishable from "not created yet" — treat it as
    // an empty config rather than erroring the user out of every MCP action.
    let empty = sandbox.seed("empty.json", "");
    json_mcpservers_upsert(&empty, "srv", serde_json::json!({"command": "npx"})).unwrap();
    let filled: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&empty).unwrap()).unwrap();
    assert_eq!(filled["mcpServers"]["srv"]["command"], "npx");
}

// ---------------------------------------------------------------------------
// Unified store (F1 / R1): malformed store → Err + recoverable quarantine copy
// ---------------------------------------------------------------------------

fn corrupt_copies(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(".corrupt."))
        })
        .collect();
    found.sort();
    found
}

#[test]
fn malformed_store_errors_and_is_quarantined_recoverably() {
    let sandbox = TempConfigDir::new("store");
    // Valid JSON, wrong shape for `McpStore` — the realistic breakage (a schema
    // change), not just a stray brace.
    let original = r#"{"version":1,"servers":[{"id":"a","name":"kept","transport":42}]}"#;
    let path = sandbox.seed("mcp_servers.json", original);

    let err = read_mcp_store(&path).expect_err("a malformed store must not read as empty");
    assert!(format!("{err:#}").contains("not valid JSON"), "{err:#}");

    // The original file is untouched and its data survives in the copy.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let copies = corrupt_copies(&sandbox.dir);
    assert_eq!(copies.len(), 1, "expected exactly one quarantine copy");
    assert_eq!(std::fs::read_to_string(&copies[0]).unwrap(), original);

    // Re-reading the same broken file reuses the copy instead of piling up one
    // snapshot per MCP screen visit.
    assert!(read_mcp_store(&path).is_err());
    assert_eq!(corrupt_copies(&sandbox.dir).len(), 1);
}

#[test]
fn missing_store_still_reads_as_an_empty_default() {
    let sandbox = TempConfigDir::new("store-missing");
    let path = sandbox.dir.join("mcp_servers.json");

    let store = read_mcp_store(&path).unwrap();
    assert!(store.servers.is_empty());
    assert!(corrupt_copies(&sandbox.dir).is_empty());
}

#[test]
fn writing_the_store_backs_up_the_file_it_replaces() {
    let sandbox = TempConfigDir::new("store-backup");
    let path = sandbox.dir.join("mcp_servers.json");

    let mut store = McpStore::default();
    create_server(&mut store, stdio_entry("first")).unwrap();
    write_mcp_store(&store, &path).unwrap();
    let first_bytes = std::fs::read_to_string(&path).unwrap();

    // No backup for the initial write — there was nothing to lose.
    assert!(backup_copies(&sandbox.dir).is_empty());

    let mut next = McpStore::default();
    create_server(&mut next, stdio_entry("second")).unwrap();
    write_mcp_store(&next, &path).unwrap();

    let backups = backup_copies(&sandbox.dir);
    assert_eq!(backups.len(), 1, "overwriting must leave a rolling backup");
    assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), first_bytes);
    assert_eq!(read_mcp_store(&path).unwrap().servers[0].name, "second");
}

fn backup_copies(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(".bak."))
        })
        .collect()
}

fn stdio_entry(name: &str) -> McpServerEntry {
    let mut entry = blank_entry(name, "stdio");
    entry.command = Some("npx".into());
    entry
}

// ---------------------------------------------------------------------------
// Presets (A.3-f): curated rows and the built-in catalog must both survive
// ---------------------------------------------------------------------------

fn curated_like(id: &str, name: &str) -> McpPreset {
    McpPreset {
        id: id.to_string(),
        name: name.to_string(),
        description: "curated".into(),
        homepage: "https://example.com".into(),
        transport: "stdio".into(),
        command: Some("npx".into()),
        args: Vec::new(),
        env: Default::default(),
        url: None,
        headers: Default::default(),
        tags: vec!["recommended".into()],
        required_env: Vec::new(),
    }
}

/// The regression this pins: `get_mcp_presets` used to pick *either* the
/// curated rows *or* the built-in catalog, so one promoted curated row hid all
/// of the built-ins.
#[test]
fn merging_curated_presets_keeps_the_whole_builtin_catalog() {
    let builtin = get_mcp_presets();
    let merged = merge_mcp_presets(vec![curated_like("curated-only", "curated-only")], builtin);

    assert_eq!(
        merged.len(),
        get_mcp_presets().len() + 1,
        "every built-in preset must survive alongside the curated row"
    );
    assert_eq!(merged[0].id, "curated-only", "curated rows lead");
    for preset in get_mcp_presets() {
        assert!(
            merged.iter().any(|m| m.id == preset.id),
            "built-in preset '{}' disappeared from the merge",
            preset.id
        );
    }
}

/// AdsPower ships both as a curated row and as a built-in preset; the chips
/// must show it once, with the curated version winning.
#[test]
fn merging_presets_dedupes_by_id_and_name() {
    let builtin = get_mcp_presets();
    let duplicate_id = builtin[0].id.clone();
    let duplicate_name = builtin[1].name.to_uppercase();

    let merged = merge_mcp_presets(
        vec![
            curated_like(&duplicate_id, "renamed-by-marketplace"),
            curated_like("fresh-id", &duplicate_name),
        ],
        builtin.clone(),
    );

    assert_eq!(merged.len(), builtin.len());
    assert_eq!(
        merged
            .iter()
            .filter(|p| p.id == duplicate_id)
            .collect::<Vec<_>>()
            .len(),
        1
    );
    assert_eq!(merged[0].description, "curated", "curated version wins");
    assert!(
        !merged
            .iter()
            .skip(2)
            .any(|p| p.name.eq_ignore_ascii_case(&duplicate_name)),
        "name collision must be deduped case-insensitively"
    );
}

#[test]
fn merging_presets_falls_back_to_the_builtin_catalog_when_curated_is_empty() {
    let merged = merge_mcp_presets(Vec::new(), get_mcp_presets());
    assert_eq!(merged.len(), get_mcp_presets().len());
    assert!(
        merged.len() >= 10,
        "the built-in floor must stay substantial"
    );
}
