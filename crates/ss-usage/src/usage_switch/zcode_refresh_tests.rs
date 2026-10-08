//! Refresh must not write the client session; explicit sync preserves its profile.

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn refresh_preserves_the_bigmodel_request_identity() {
    let _sb = sandbox();
    save_oauth("zcode-a", "bigmodel", "user-a", "access-a", None, "jwt-a");
    super::super::Adapter.activate("zcode-a").unwrap();
    let path = super::super::credentials_path();
    let profile = json!({"id": "user-a", "username": "alice", "displayName": "Alice",
        "avatarUrl": "https://example.com/avatar", "rawProfile": {"customerNumber": "user-a"}});
    let mut credentials = load(&path);
    credentials["oauth:bigmodel:user_info"] =
        json!(encrypt_enc_v1(&super::super::credential_key(), &profile.to_string()).unwrap());
    write_json(&path, &credentials);
    let mut row = storage::get_subscription("zcode-a").unwrap();
    let lease = crate::usage_switch::acquire_cli_refresh_lease(CATALOG_ID)
        .await
        .unwrap();
    crate::usage_switch::adopt_active_cli_session_before_refresh(&mut row, &lease).unwrap();
    let before_refresh = row.clone();
    let before = std::fs::read(&path).unwrap();
    let settings_before = std::fs::read(super::super::settings_path()).unwrap();
    let outcome =
        crate::usage_switch::sync_refreshed_active_subscription(&before_refresh, &mut row, &lease)
            .unwrap();
    assert!(outcome.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::read(super::super::settings_path()).unwrap(),
        settings_before
    );

    // Explicit resync must also leave the official account identity intact.
    let outcome = super::super::Adapter.sync(&row).unwrap();
    assert!(outcome.success, "{outcome:?}");
    let written: Value =
        serde_json::from_str(&reveal(&load(&path), "oauth:bigmodel:user_info")).unwrap();
    // ZCode OAuthCredentialRepo.loadUserProfile requires all three strings;
    // otherwise AccountProviderRequestAuthService cannot resolve a request key.
    assert!(
        ["id", "username", "displayName"]
            .iter()
            .all(|key| written[*key].is_string()),
        "Account request credential is unavailable: account:bigmodel-individual-coding-plan"
    );
    assert_eq!(written, profile);

    save_oauth("zcode-b", "bigmodel", "user-b", "access-b", None, "jwt-b");
    super::super::Adapter.activate("zcode-b").unwrap();
    let switched: Value =
        serde_json::from_str(&reveal(&load(&path), "oauth:bigmodel:user_info")).unwrap();
    assert_eq!(switched["id"], "user-b");
    assert!(
        ["username", "displayName"]
            .iter()
            .all(|key| switched[*key].is_string())
    );
    assert!(switched.get("rawProfile").is_none());
}
