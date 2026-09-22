//! Lossless-write tests: what a write into an Agent's own config file must
//! leave alone.
//!
//! `tests_targets.rs` pins *what* each writer emits and `tests_fail_closed.rs`
//! pins *when* it refuses. This module pins the third rule: a write with nothing
//! to do must not touch the file at all. Every writer here replaces the whole
//! document, so "removed nothing" and "rewrote the file" are different outcomes
//! for the user even when the parsed value is identical.

use super::*;
use crate::mcp::tests_targets::TempDir;

/// A **valid** config for `tool_id` whose bytes are deliberately not what the
/// writer would emit: wrong key order for JSON/TOML, a comment for YAML.
///
/// Canonical bytes would make the no-op assertions vacuous — re-serializing a
/// file the writer itself produced yields the same bytes, so "did not write" and
/// "wrote an identical file" would be indistinguishable.
fn non_canonical_seed(tool_id: &str) -> &'static str {
    match tool_id {
        // TOML: `toml::Table` is a BTreeMap, so a rewrite sorts `zzz` after
        // `theme` and drops the comment. The server table has to be present
        // for the writer to reach its "nothing to remove" branch at all.
        "codex" | "grok" => "zzz = 1\n# keep me\n[mcp_servers.user-owned]\ncommand = \"keep\"\n",
        // YAML: `serde_yaml::Mapping` keeps insertion order, so the comment is
        // the only thing a rewrite can destroy here.
        "hermes" => "# keep me\nzzz: 1\ntheme: dark\n",
        // DSH's file is a sequence of Cordis patch ops, not a mapping.
        "deepseek" => "# keep me\n- insert:\n    - id: other-plugin\n      serverName: other\n",
        // JSON: the server map sits under each target's own root key.
        "vscode" => r#"{"zzz":1,"servers":{"user-owned":{"command":"keep"}},"aaa":2}"#,
        "zed" => r#"{"zzz":1,"context_servers":{"user-owned":{"command":"keep"}},"aaa":2}"#,
        "opencode" => r#"{"zzz":1,"mcp":{"user-owned":{"type":"local"}},"aaa":2}"#,
        "zcode" => r#"{"zzz":1,"mcp":{"servers":{"user-owned":{"command":"keep"}}},"aaa":2}"#,
        _ => r#"{"zzz":1,"mcpServers":{"user-owned":{"command":"keep"}},"aaa":2}"#,
    }
}

fn stdio(name: &str) -> McpServerEntry {
    let mut e = blank_entry(name, "stdio");
    e.command = Some("npx".into());
    e.args = vec!["-y".into(), "example-mcp".into()];
    e
}

fn read(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

/// Removing a name that is not in the file must leave the file byte-for-byte
/// alone: no re-serialization, no key-order churn, no lost comments.
///
/// This is not a corner case. `sync_server_public_tools` calls `remove` for
/// every *disabled* target on every create and update, so before this rule one
/// install re-serialized the config file of every Agent on the machine.
#[test]
fn removing_an_absent_server_never_rewrites_any_target() {
    let dir = TempDir::new("lossless-noop-remove");
    // Collect every offender instead of failing on the first: a regression that
    // reintroduces the rewrite usually does it through a shared helper, so the
    // useful output is the full list of formats that lost their bytes.
    let mut offenders = Vec::new();
    for spec in mcp_tool_specs() {
        let path = dir.path().join(format!("{}.cfg", spec.id));
        std::fs::write(&path, non_canonical_seed(spec.id)).unwrap();
        let before = read(&path);
        (spec.remove)(&path, "definitely-absent").unwrap();
        if read(&path) != before {
            offenders.push(spec.id);
        }
    }
    assert!(
        offenders.is_empty(),
        "these targets rewrote a config they had nothing to remove from: {offenders:?}"
    );
}

/// The hidden legacy ids (`claude-desktop`, `gemini`) remove through the shared
/// JSON helpers instead of a registry row; those helpers skip the rewrite too.
#[test]
fn removing_an_absent_server_via_the_shared_json_helpers_is_a_no_op() {
    let dir = TempDir::new("lossless-noop-legacy");
    let path = dir.path().join("claude_desktop_config.json");
    std::fs::write(&path, non_canonical_seed("claude-desktop-chat")).unwrap();
    let before = read(&path);
    json_mcpservers_remove(&path, "absent").unwrap();
    json_mcpservers_remove_strict(&path, "absent").unwrap();
    // A missing root key is "nothing to remove", not "create the key".
    json_named_map_remove(&path, "not_the_root_key", "absent").unwrap();
    assert_eq!(read(&path), before);
}

/// The skip must not turn into "removal stopped working": a present key still
/// goes, and everything around it stays.
#[test]
fn removing_a_present_server_drops_exactly_that_key() {
    let dir = TempDir::new("lossless-remove");
    let path = dir.path().join("claude.json");
    std::fs::write(&path, non_canonical_seed("claude-code")).unwrap();

    json_mcpservers_remove(&path, "user-owned").unwrap();

    let root: serde_json::Value = serde_json::from_slice(&read(&path)).unwrap();
    assert_eq!(root["zzz"], 1, "unrelated top-level key was dropped");
    assert_eq!(root["aaa"], 2, "unrelated top-level key was dropped");
    assert!(
        !root["mcpServers"]
            .as_object()
            .unwrap()
            .contains_key("user-owned"),
        "the named server survived the removal"
    );
}

/// Every writer goes through `skillstar_core::infra::fs_ops::atomic_write`
/// (tmp + fsync + rename). A crash can therefore never leave a truncated Agent
/// config — and a successful write never leaves its scratch file behind.
#[test]
fn a_successful_write_leaves_no_temp_file_behind() {
    let dir = TempDir::new("lossless-atomic");
    for spec in mcp_tool_specs() {
        let path = dir.path().join(format!("{}.cfg", spec.id));
        (spec.upsert)(&path, &stdio("atomic")).unwrap();
        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{} left {leftovers:?}", spec.id);
    }
}

/// Codex and Grok read user-authored TOML. Adding or removing a server there
/// must leave everything else in the document alone — comments included.
///
/// This is what `toml_edit` buys over the `toml` data model: the latter cannot
/// carry a comment, so a merge through it deleted every comment in
/// `~/.codex/config.toml` as a side effect of toggling one server.
#[test]
fn toml_targets_keep_comments_and_foreign_tables() {
    let dir = TempDir::new("lossless-toml");
    let seed = "# top of file\nmodel = \"gpt-5\"\n\n# my servers\n[mcp_servers.other]\ncommand = \"keep\"\n";

    for tool_id in ["codex", "grok"] {
        let spec = mcp_tool_spec(tool_id).unwrap();
        let path = dir.path().join(format!("{tool_id}.toml"));
        std::fs::write(&path, seed).unwrap();

        (spec.upsert)(&path, &stdio("mine")).unwrap();
        let after_upsert = std::fs::read_to_string(&path).unwrap();
        for expected in [
            "# top of file",
            "model = \"gpt-5\"",
            "# my servers",
            "[mcp_servers.other]",
            "command = \"keep\"",
            "example-mcp",
        ] {
            assert!(
                after_upsert.contains(expected),
                "{tool_id} lost {expected:?} on upsert:\n{after_upsert}"
            );
        }

        (spec.remove)(&path, "mine").unwrap();
        let after_remove = std::fs::read_to_string(&path).unwrap();
        for expected in [
            "# top of file",
            "model = \"gpt-5\"",
            "# my servers",
            "command = \"keep\"",
        ] {
            assert!(
                after_remove.contains(expected),
                "{tool_id} lost {expected:?} on remove:\n{after_remove}"
            );
        }
        assert!(
            !after_remove.contains("example-mcp"),
            "{tool_id}: the removed server is still in the file:\n{after_remove}"
        );

        // And the result is still a well-formed document with the same data.
        let root = toml::from_str::<toml::Table>(&after_remove).unwrap();
        assert_eq!(root["model"].as_str(), Some("gpt-5"));
        assert_eq!(
            root["mcp_servers"]["other"]["command"].as_str(),
            Some("keep")
        );
    }
}
