//! OpenCode switch tests: capturing the CLI's own login instead of
//! projecting an API key, and the dead end when there is no login to
//! capture.
//! Like the rest of `custody_tests`, every test runs against a sandboxed
//! `SKILLSTAR_DATA_DIR` + `SKILLSTAR_TOOL_SYNC_HOME` + temporary `HOME`.

use super::*;

// ── OpenCode: the auth_mode deadlock is gone ─────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn opencode_switches_by_capturing_the_cli_login_without_any_api_key() {
    let sb = sandbox();
    // Cookie auth mode, so `api_key_encrypted` can never be populated — this
    // is the account shape that made the old switch path unreachable.
    let mut sub = subscription("oc-go", "opencode");
    sub.auth_mode = crate::AuthMode::Cookie;
    storage::upsert_subscription(sub).unwrap();
    write_json(
        &sb.live("opencode"),
        &json!({
            "opencode": { "type": "api", "key": "sk-from-opencode-auth-login" },
            "anthropic": { "type": "oauth", "refresh": "r", "access": "a", "expires": 1 },
        }),
    );

    let result = activate_subscription("oc-go").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    assert!(is_symlink(&sb.live("opencode")));
    let snapshot = read_json(&sb.snapshot("opencode", "oc-go"));
    assert_eq!(snapshot["opencode"]["key"], "sk-from-opencode-auth-login");
    assert_eq!(
        snapshot["anthropic"]["refresh"], "r",
        "another provider's login travels with the snapshot"
    );
    assert_eq!(
        storage::get_active_subscription("opencode")
            .unwrap()
            .as_deref(),
        Some("oc-go")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn opencode_still_projects_an_api_key_when_the_row_has_one() {
    let sb = sandbox();
    let mut sub = subscription("oc-key", "opencode");
    sub.auth_mode = crate::AuthMode::ApiKey;
    sub.api_key_encrypted = Some(crypto::encrypt("sk-from-skillstar"));
    storage::upsert_subscription(sub).unwrap();
    write_json(
        &sb.live("opencode"),
        &json!({ "anthropic": { "type": "api", "key": "sk-anthropic" } }),
    );

    let result = activate_subscription("oc-key").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    let live = read_json(&sb.live("opencode"));
    assert_eq!(live["opencode"]["type"], "api");
    assert_eq!(live["opencode"]["key"], "sk-from-skillstar");
    assert_eq!(live["anthropic"]["key"], "sk-anthropic");
    assert!(
        live.get("skillstar").is_none(),
        "the invented provider key must stay gone"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn opencode_reports_the_missing_credential_instead_of_a_silent_dead_end() {
    let _sb = sandbox();
    let mut sub = subscription("oc-empty", "opencode");
    sub.auth_mode = crate::AuthMode::Cookie;
    storage::upsert_subscription(sub).unwrap();

    let result = activate_subscription("oc-empty").await.unwrap();

    assert!(!result.switch_result.success);
    assert_eq!(
        result.switch_result.error.as_deref(),
        Some(
            MaterializeError::NoCapturedSession { tool: "OpenCode" }
                .to_string()
                .as_str()
        ),
        "the dead end has to name the CLI the user must log into"
    );
}
