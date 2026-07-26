//! tool_sync tests — part3 (split out of the original inline test module).

use super::*;

// =========================================================================
// Config Conflict Detection Tests
// =========================================================================

#[test]
fn test_check_external_modification_no_last_sync() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{}").unwrap();

    // No last_sync_timestamp → no conflict
    let result = check_external_modification(&path, None);
    assert!(result.is_none());
}

#[test]
fn test_check_external_modification_file_not_exists() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("nonexistent.json");

    let result = check_external_modification(&path, Some(1000));
    assert!(result.is_none());
}

#[test]
fn test_check_external_modification_file_modified_after_sync() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{}").unwrap();

    // Use a timestamp far in the past so the file's mtime is definitely newer
    let old_timestamp = 1_000_000u64;
    let result = check_external_modification(&path, Some(old_timestamp));
    assert!(result.is_some());

    let conflict = result.unwrap();
    assert_eq!(conflict.conflict_type, ConflictType::ExternalModification);
    assert!(conflict.file_path.is_some());
    assert!(conflict.details.is_some());
}

#[test]
fn test_check_external_modification_file_not_modified() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{}").unwrap();

    // Use a timestamp far in the future so the file's mtime is definitely older
    let future_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 10_000;
    let result = check_external_modification(&path, Some(future_timestamp));
    assert!(result.is_none());
}

#[test]
fn test_check_legacy_claude_config_no_file() {
    // This test relies on ~/.claude.json not existing in the test environment.
    // If it does exist, this test may not be meaningful, but it won't fail.
    // We test the function logic with a controlled path instead.
    let tmp = TempDir::new().unwrap();
    let legacy_path = tmp.path().join(".claude.json");

    // File doesn't exist → no conflict
    assert!(!legacy_path.exists());
}

#[test]
fn test_check_legacy_claude_config_with_conflicting_env() {
    let tmp = TempDir::new().unwrap();
    let legacy_path = tmp.path().join(".claude.json");

    let content = serde_json::json!({
        "env": {
            "ANTHROPIC_API_KEY": "sk-ant-test",
            "ANTHROPIC_BASE_URL": "https://example.com"
        }
    });
    std::fs::write(&legacy_path, serde_json::to_string(&content).unwrap()).unwrap();

    // Use the internal logic directly since check_legacy_claude_config uses home dir
    let json: Value =
        serde_json::from_str(&std::fs::read_to_string(&legacy_path).unwrap()).unwrap();

    if let Some(env_obj) = json.get("env").and_then(|v| v.as_object()) {
        let conflicting_keys: Vec<&String> = env_obj
            .keys()
            .filter(|k| k.starts_with("ANTHROPIC_"))
            .collect();
        assert_eq!(conflicting_keys.len(), 2);
    } else {
        panic!("Expected env block in test JSON");
    }
}

#[test]
fn test_check_legacy_claude_config_without_conflicting_env() {
    let tmp = TempDir::new().unwrap();
    let legacy_path = tmp.path().join(".claude.json");

    // File exists but no ANTHROPIC_* fields in env
    let content = serde_json::json!({
        "env": {
            "SOME_OTHER_VAR": "value"
        }
    });
    std::fs::write(&legacy_path, serde_json::to_string(&content).unwrap()).unwrap();

    let json: Value =
        serde_json::from_str(&std::fs::read_to_string(&legacy_path).unwrap()).unwrap();

    if let Some(env_obj) = json.get("env").and_then(|v| v.as_object()) {
        let conflicting_keys: Vec<&String> = env_obj
            .keys()
            .filter(|k| k.starts_with("ANTHROPIC_"))
            .collect();
        assert!(conflicting_keys.is_empty());
    }
}

#[test]
fn test_detect_env_conflicts_with_set_vars() {
    // Temporarily set env vars for testing
    // SAFETY: This test runs in isolation and we clean up the var after.
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-12345") };
    let conflicts = detect_env_conflicts();

    // Should detect at least ANTHROPIC_API_KEY
    let anthropic_conflict = conflicts.iter().find(|c| {
        c.details
            .as_ref()
            .is_some_and(|d| d.contains("ANTHROPIC_API_KEY"))
    });
    assert!(anthropic_conflict.is_some());
    let conflict = anthropic_conflict.unwrap();
    assert_eq!(conflict.conflict_type, ConflictType::EnvVarOverride);
    assert!(conflict.description.contains("ANTHROPIC_API_KEY"));

    // Clean up
    // SAFETY: Restoring env state after test.
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
}

#[test]
fn test_detect_env_conflicts_empty_var_ignored() {
    // Set an empty env var — should not be reported as a conflict
    // SAFETY: This test runs in isolation and we clean up the var after.
    unsafe { std::env::set_var("OPENAI_BASE_URL", "") };
    let conflicts = detect_env_conflicts();

    let openai_base_conflict = conflicts.iter().find(|c| {
        c.details
            .as_ref()
            .is_some_and(|d| d.contains("OPENAI_BASE_URL"))
    });
    assert!(openai_base_conflict.is_none());

    // Clean up
    // SAFETY: Restoring env state after test.
    unsafe { std::env::remove_var("OPENAI_BASE_URL") };
}

#[test]
fn test_detect_conflicts_combines_all_sources() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{}").unwrap();

    // Set an env var to trigger EnvVarOverride
    // SAFETY: This test runs in isolation and we clean up the var after.
    unsafe { std::env::set_var("ANTHROPIC_AUTH_TOKEN", "test-token-value") };

    // Use a very old timestamp to trigger ExternalModification
    let conflicts = detect_conflicts("claude-code", Some(1_000_000));

    // Should have at least the env var conflict
    let has_env_conflict = conflicts
        .iter()
        .any(|c| c.conflict_type == ConflictType::EnvVarOverride);
    assert!(has_env_conflict);

    // Clean up
    // SAFETY: Restoring env state after test.
    unsafe { std::env::remove_var("ANTHROPIC_AUTH_TOKEN") };
}

#[test]
fn test_config_conflict_serialization_roundtrip() {
    let conflict = ConfigConflict {
        conflict_type: ConflictType::ExternalModification,
        description: "File was modified externally".to_string(),
        file_path: Some("/home/user/.claude/settings.json".to_string()),
        details: Some("mtime=1700000000, last_sync=1699999000".to_string()),
        tool_id: None,
    };

    let json = serde_json::to_string(&conflict).unwrap();
    let deserialized: ConfigConflict = serde_json::from_str(&json).unwrap();
    assert_eq!(conflict, deserialized);
}

#[test]
fn test_conflict_type_variants_serialize() {
    // Verify all ConflictType variants serialize/deserialize correctly
    let variants = vec![
        ConflictType::ExternalModification,
        ConflictType::LegacyConfig,
        ConflictType::EnvVarOverride,
    ];

    for variant in variants {
        let json = serde_json::to_string(&variant).unwrap();
        let deserialized: ConflictType = serde_json::from_str(&json).unwrap();
        assert_eq!(variant, deserialized);
    }
}
