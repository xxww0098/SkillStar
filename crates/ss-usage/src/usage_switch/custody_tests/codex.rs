//! Codex CLI switch tests: writing the CLI's own `auth.json` token schema,
//! failing without moving the pin, absorbing CLI-side rotations, and the
//! target-level unit that needs no filesystem.
//! Like the rest of `custody_tests`, every test runs against a sandboxed
//! `SKILLSTAR_DATA_DIR` + `SKILLSTAR_TOOL_SYNC_HOME` + temporary `HOME`;
//! nothing here may touch a real `$HOME` or the macOS login keychain.

use super::*;

const CODEX_ID_TOKEN: &str = concat!(
    "e30.",
    "eyJlbWFpbCI6ImRhbmFAZXhhbXBsZS5jb20iLCJzdWIiOiJ1aWQtZGFuYSIsImV4cCI6MTk5OTk5OTk5OX0",
    "."
);

#[tokio::test(flavor = "current_thread")]
async fn codex_refresh_preserves_client_fields_and_newer_logins() {
    let sb = sandbox();
    let mut row = subscription("codex-dana", "codex");
    row.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    row.refresh_token_encrypted = Some(crypto::encrypt("codex-refresh"));
    row.id_token_encrypted = Some(crypto::encrypt(CODEX_ID_TOKEN));
    storage::upsert_subscription(row).unwrap();
    activate_subscription("codex-dana").await.unwrap();
    let path = sb.live("codex");
    let mut root = read_json(&path);
    root["tokens"]["clientExtension"] = json!({"version": 1});
    fs::write(&path, serde_json::to_vec(&root).unwrap()).unwrap();
    let lease = acquire_cli_refresh_lease("codex").await.unwrap();
    let mut row = storage::get_subscription("codex-dana").unwrap();
    adopt_active_cli_session_before_refresh(&mut row, &lease).unwrap();
    let before = row.clone();
    let original = fs::read(&path).unwrap();
    row.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    row.display_name = "new label".into();
    row.plan_tier = Some("new plan".into());
    assert!(
        sync_refreshed_active_subscription(&before, &mut row, &lease)
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read(&path).unwrap(), original);

    row.access_token_encrypted = Some(crypto::encrypt("rotated-access"));
    row.refresh_token_encrypted = Some(crypto::encrypt("rotated-refresh"));
    assert!(
        sync_refreshed_active_subscription(&before, &mut row, &lease)
            .unwrap()
            .unwrap()
            .success
    );
    assert_eq!(
        read_json(&path)["tokens"]["clientExtension"],
        json!({"version": 1})
    );
    assert_eq!(read_json(&path)["tokens"]["access_token"], "rotated-access");

    let before = row.clone();
    let mut newer = read_json(&path);
    newer["tokens"]["access_token"] = json!("client-newer-access");
    newer["tokens"]["refresh_token"] = json!("client-newer-refresh");
    fs::write(&path, serde_json::to_vec(&newer).unwrap()).unwrap();
    row.access_token_encrypted = Some(crypto::encrypt("late-access"));
    assert!(
        sync_refreshed_active_subscription(&before, &mut row, &lease)
            .unwrap()
            .is_none()
    );
    assert_eq!(read_json(&path), newer);
    fs::remove_file(&path).unwrap();
    assert!(
        sync_refreshed_active_subscription(&before, &mut row, &lease)
            .unwrap()
            .is_none()
    );
    assert!(
        !path.exists(),
        "quota refresh must not log a client back in"
    );
}

// ── Codex ────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn codex_activation_writes_the_cli_token_schema() {
    let sb = sandbox();
    let mut sub = subscription("codex-dana", "codex");
    sub.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    sub.id_token_encrypted = Some(crypto::encrypt(CODEX_ID_TOKEN));
    sub.refresh_token_encrypted = Some(crypto::encrypt("codex-refresh"));
    sub.oauth_account_id = Some("uid-dana".into());
    storage::upsert_subscription(sub).unwrap();

    let result = activate_subscription("codex-dana").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    let live = read_json(&sb.live("codex"));
    assert!(live["OPENAI_API_KEY"].is_null());
    assert_eq!(live["tokens"]["access_token"], "codex-access");
    assert_eq!(live["tokens"]["id_token"], CODEX_ID_TOKEN);
    assert_eq!(live["tokens"]["refresh_token"], "codex-refresh");
    assert_eq!(live["tokens"]["account_id"], "uid-dana");
    assert!(live["last_refresh"].is_string());
    assert!(is_symlink(&sb.live("codex")));
}

#[tokio::test(flavor = "current_thread")]
async fn codex_missing_id_token_fails_without_moving_the_pin() {
    let _sb = sandbox();
    let mut sub = subscription("codex-dana", "codex");
    sub.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    storage::upsert_subscription(sub).unwrap();

    let result = activate_subscription("codex-dana").await.unwrap();

    assert!(!result.switch_result.success);
    assert_eq!(
        result.switch_result.error.as_deref(),
        Some(
            MaterializeError::MissingSecret {
                tool: "Codex",
                field: "id_token",
                remedy: "请重新登录该账号补充凭证",
            }
            .to_string()
            .as_str()
        )
    );
    assert!(storage::get_active_subscription("codex").unwrap().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn codex_absorbs_a_rotation_the_cli_wrote_into_the_snapshot() {
    let sb = sandbox();
    let mut sub = subscription("codex-dana", "codex");
    sub.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    sub.id_token_encrypted = Some(crypto::encrypt(CODEX_ID_TOKEN));
    sub.refresh_token_encrypted = Some(crypto::encrypt("codex-refresh"));
    storage::upsert_subscription(sub).unwrap();
    activate_subscription("codex-dana").await.unwrap();

    let mut root = read_json(&sb.live("codex"));
    root["tokens"]["access_token"] = json!("codex-access-v2");
    root["tokens"]["refresh_token"] = json!("codex-refresh-v2");
    fs::write(sb.live("codex"), serde_json::to_vec_pretty(&root).unwrap()).unwrap();

    assert_eq!(
        sb.custody("codex").reconcile().unwrap(),
        LinkState::LinkedTo("codex-dana".into())
    );
    let row = storage::get_subscription("codex-dana").unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        "codex-access-v2"
    );
    assert_eq!(
        crypto::decrypt(row.refresh_token_encrypted.as_deref().unwrap()),
        "codex-refresh-v2"
    );
}

#[test]
fn codex_access_token_covers_both_oauth_and_api_key_shapes() {
    let target = CodexTarget;
    assert_eq!(
        target
            .access_token(&json!({ "tokens": { "access_token": "at" } }))
            .as_deref(),
        Some("at")
    );
    assert_eq!(
        target
            .access_token(&json!({ "OPENAI_API_KEY": "sk-1" }))
            .as_deref(),
        Some("sk-1")
    );
    assert!(
        target
            .access_token(&json!({ "OPENAI_API_KEY": null }))
            .is_none()
    );
}
