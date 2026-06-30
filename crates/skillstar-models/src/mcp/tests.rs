//! Unit tests for MCP server management (pure: in-memory specs/entries + temp-dir IO).

use super::*;

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
    let v = canonical_spec(&stdio("fs"));
    assert_eq!(v["type"], "stdio");
    assert_eq!(v["command"], "npx");
    assert_eq!(v["args"][0], "-y");
    assert_eq!(v["env"]["HOME"], "/Users/test");
}

#[test]
fn claude_desktop_omits_type_and_rejects_http() {
    let v = claude_desktop_spec(&stdio("fs")).unwrap();
    assert!(v.get("type").is_none());
    assert_eq!(v["command"], "npx");
    assert!(claude_desktop_spec(&http("remote")).is_err());
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
    let spec = canonical_spec(&e);
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
fn mcp_presets_catalog_is_well_formed() {
    let presets = get_mcp_presets();
    assert!(presets.len() >= 10, "preset catalog should be substantial");

    let mut seen = std::collections::HashSet::new();
    for p in &presets {
        assert!(!p.name.is_empty(), "preset '{}' has empty name", p.id);
        assert!(
            seen.insert(p.id.clone()),
            "duplicate preset id '{}'",
            p.id
        );
        match p.transport.as_str() {
            "http" | "sse" => assert!(
                p.url.is_some(),
                "remote preset '{}' must carry a url",
                p.id
            ),
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
    let results = sync_server_all_tools(&stdio("fs"), false);
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
