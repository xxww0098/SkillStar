use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::test_support::{ENV_LOCK, EnvGuard};

const BILLING_ONLY_TOKEN: &str = concat!(
    "e30.",
    "eyJzY29wZSI6Im9wZW5pZCBwcm9maWxlIGVtYWlsIG9mZmxpbmVfYWNjZXNzIGdyb2stY2xpOmFjY2VzcyBhcGk6YWNjZXNzIiwiZW1haWwiOiJhbGljZUBleGFtcGxlLmNvbSIsInN1YiI6InVpZC1hbGljZSIsImV4cCI6MTk5OTk5OTk5OX0",
    "."
);

const FULL_CLI_TOKEN: &str = concat!(
    "e30.",
    "eyJzY29wZSI6Im9wZW5pZCBwcm9maWxlIGVtYWlsIG9mZmxpbmVfYWNjZXNzIGdyb2stY2xpOmFjY2VzcyBjb252ZXJzYXRpb25zOnJlYWQgY29udmVyc2F0aW9uczp3cml0ZSBhcGk6YWNjZXNzIiwiZW1haWwiOiJhbGljZUBleGFtcGxlLmNvbSIsInN1YiI6InVpZC1hbGljZSIsInByaW5jaXBhbF90eXBlIjoidXNlciIsInByaW5jaXBhbF9pZCI6InVpZC1hbGljZSIsInRlYW1faWQiOiJ0ZWFtLWFsaWNlIiwiY29kaW5nX2RhdGFfcmV0ZW50aW9uX29wdF9vdXQiOnRydWUsImV4cCI6MTk5OTk5OTk5OX0",
    "."
);

const EXPIRED_FULL_CLI_TOKEN: &str = concat!(
    "e30.",
    "eyJzY29wZSI6Im9wZW5pZCBwcm9maWxlIGVtYWlsIG9mZmxpbmVfYWNjZXNzIGdyb2stY2xpOmFjY2VzcyBjb252ZXJzYXRpb25zOnJlYWQgY29udmVyc2F0aW9uczp3cml0ZSBhcGk6YWNjZXNzIiwiZW1haWwiOiJhbGljZUBleGFtcGxlLmNvbSIsInN1YiI6InVpZC1hbGljZSIsImV4cCI6MX0",
    "."
);

fn grok_subscription(token: &str) -> Subscription {
    Subscription {
        id: "grok-alice".into(),
        catalog_id: "xai".into(),
        display_name: "alice@example.com".into(),
        auth_mode: skillstar_usage::AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "USD".into(),
        billing_cycle: skillstar_usage::BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(token)),
        refresh_token_encrypted: Some(crypto::encrypt("alice-refresh")),
        access_token_expires_at: Some(1_999_999_999),
        id_token_encrypted: None,
        oauth_account_id: Some("uid-alice".into()),
        oauth_region: None,
        requires_reauth: false,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: 0,
        updated_at: 0,
    }
}

fn working_bob_root() -> Value {
    serde_json::json!({
        oidc_scope_key(): {
            "key": "working-cli-token",
            "refresh_token": "working-cli-refresh",
            "email": "bob@example.com",
            "user_id": "uid-bob",
            "team_id": "team-bob"
        },
        "https://accounts.x.ai/sign-in": {
            "key": "legacy-session"
        }
    })
}

#[derive(Default)]
struct MemorySessions {
    entries: HashMap<String, Value>,
    fail_save: bool,
}

impl GrokSessionStore for MemorySessions {
    fn load(&self, subscription_id: &str) -> Result<Option<Value>, GrokIoError> {
        Ok(self.entries.get(subscription_id).cloned())
    }

    fn save(&mut self, subscription_id: &str, entry: &Value) -> Result<(), GrokIoError> {
        if self.fail_save {
            return Err(GrokIoError::Replace {
                file: GrokFile::SessionStore,
                source: std::io::Error::other("snapshot store unavailable"),
            });
        }
        self.entries
            .insert(subscription_id.to_string(), entry.clone());
        Ok(())
    }

    fn remove(&mut self, subscription_id: &str) -> Result<(), GrokIoError> {
        self.entries.remove(subscription_id);
        Ok(())
    }
}

struct MemoryAuthFile {
    path: PathBuf,
    root: RefCell<Value>,
    conflict_before_commit: Cell<bool>,
    overwrite_after_commit: Cell<bool>,
    fail_after_commit: Cell<bool>,
    commits: Cell<usize>,
}

impl MemoryAuthFile {
    fn new(root: Value) -> Self {
        Self {
            path: PathBuf::from("/memory/.grok/auth.json"),
            root: RefCell::new(root),
            conflict_before_commit: Cell::new(false),
            overwrite_after_commit: Cell::new(false),
            fail_after_commit: Cell::new(false),
            commits: Cell::new(0),
        }
    }

    fn raw_revision(&self) -> [u8; 32] {
        revision(&serde_json::to_vec(&*self.root.borrow()).unwrap())
    }
}

impl GrokAuthFile for MemoryAuthFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<LoadedAuth, GrokIoError> {
        Ok(LoadedAuth {
            root: self.root.borrow().clone(),
            revision: self.raw_revision(),
        })
    }

    fn commit_verified(
        &self,
        loaded: &LoadedAuth,
        scope: &str,
        expected_entry: &Value,
    ) -> Result<Option<PathBuf>, AuthCommitError> {
        if self.conflict_before_commit.get() {
            self.root
                .borrow_mut()
                .as_object_mut()
                .unwrap()
                .insert("external-change".into(), Value::Bool(true));
        }
        if self.raw_revision() != loaded.revision {
            return Err(AuthCommitError {
                reason: AuthCommitReason::ConcurrentModification,
                target_installed: false,
            });
        }
        *self.root.borrow_mut() = loaded.root.clone();
        self.commits.set(self.commits.get() + 1);
        if self.fail_after_commit.get() {
            return Err(AuthCommitError {
                reason: AuthCommitReason::Io(GrokIoError::UnexpectedMode {
                    path: self.path.clone(),
                    mode: 0o644,
                }),
                target_installed: true,
            });
        }
        if self.overwrite_after_commit.get() {
            self.root.borrow_mut()[scope] = serde_json::json!({
                "key": "overwritten-by-running-grok",
                "email": "bob@example.com"
            });
        }
        if self.root.borrow().get(scope) != Some(expected_entry) {
            return Err(AuthCommitError {
                reason: AuthCommitReason::OverwrittenAfterCommit,
                target_installed: false,
            });
        }
        Ok(None)
    }
}

#[test]
fn grok_switch_rejects_billing_only_token_without_destroying_cli_session() {
    let original = working_bob_root();
    let auth = MemoryAuthFile::new(original.clone());
    let mut sessions = MemorySessions::default();
    let alice = grok_subscription(BILLING_ONLY_TOKEN);

    let error = install_target(&auth, &mut sessions, &alice).unwrap_err();

    assert!(error.requires_reauth);
    assert!(error.message.contains("conversations:read"));
    assert_eq!(*auth.root.borrow(), original);
    assert_eq!(auth.commits.get(), 0);
    assert!(sessions.entries.is_empty());
}

#[test]
fn full_scope_switch_preserves_siblings_and_builds_cli_identity() {
    let auth = MemoryAuthFile::new(working_bob_root());
    let mut sessions = MemorySessions::default();
    let alice = grok_subscription(FULL_CLI_TOKEN);

    install_target(&auth, &mut sessions, &alice).unwrap();

    let root = auth.root.borrow();
    let entry = &root[oidc_scope_key()];
    assert_eq!(entry["key"], FULL_CLI_TOKEN);
    assert_eq!(entry["refresh_token"], "alice-refresh");
    assert_eq!(entry["email"], "alice@example.com");
    assert_eq!(entry["user_id"], "uid-alice");
    assert_eq!(entry["principal_id"], "uid-alice");
    assert_eq!(entry["principal_type"], "user");
    assert_eq!(entry["team_id"], "team-alice");
    assert_eq!(entry["coding_data_retention_opt_out"], true);
    assert_eq!(
        root["https://accounts.x.ai/sign-in"]["key"],
        "legacy-session"
    );
    assert_eq!(sessions.entries[&alice.id], *entry);
}

#[test]
fn valid_full_entry_snapshot_can_restore_an_old_narrow_scope_card() {
    let auth = MemoryAuthFile::new(working_bob_root());
    let alice = grok_subscription(BILLING_ONLY_TOKEN);
    let snapshot = serde_json::json!({
        "key": "opaque-cli-token-from-grok",
        "refresh_token": "rotated-cli-refresh",
        "expires_at": "2035-01-01T00:00:00.000000Z",
        "email": "alice@example.com",
        "user_id": "uid-alice",
        "principal_id": "uid-alice",
        "principal_type": "user",
        "team_id": "team-alice",
        "auth_mode": "oidc",
        "oidc_issuer": "https://auth.x.ai",
        "oidc_client_id": oidc_client_id()
    });
    let mut sessions = MemorySessions::default();
    sessions.entries.insert(alice.id.clone(), snapshot.clone());

    install_target(&auth, &mut sessions, &alice).unwrap();

    let root = auth.root.borrow();
    assert_eq!(root[oidc_scope_key()]["key"], snapshot["key"]);
    assert!(root[oidc_scope_key()]["create_time"].is_string());
    assert!(root[oidc_scope_key()]["coding_data_retention_opt_out"].is_boolean());
}

#[test]
fn mismatched_snapshot_is_never_assigned_to_the_target() {
    let auth = MemoryAuthFile::new(working_bob_root());
    let alice = grok_subscription(BILLING_ONLY_TOKEN);
    let mut sessions = MemorySessions::default();
    sessions.entries.insert(
        alice.id.clone(),
        serde_json::json!({
            "key": "bob-token",
            "email": "bob@example.com",
            "user_id": "uid-bob"
        }),
    );

    let error = install_target(&auth, &mut sessions, &alice).unwrap_err();

    assert!(error.requires_reauth);
    assert_eq!(auth.commits.get(), 0);
}

#[test]
fn conflicting_subject_rejects_entry_even_when_email_matches() {
    let alice = grok_subscription(FULL_CLI_TOKEN);
    let conflicting = serde_json::json!({
        "key": "opaque-token",
        "email": "alice@example.com",
        "user_id": "uid-alice",
        "principal_id": "uid-bob"
    });

    assert!(!entry_matches_subscription(&conflicting, &alice));
    assert!(entry_identity_conflicts(&conflicting, &alice));
    assert!(validate_entry_identity(&conflicting, &alice).is_err());
}

#[test]
fn current_access_token_identity_outranks_stale_subscription_metadata() {
    let mut alice = grok_subscription(FULL_CLI_TOKEN);
    alice.oauth_account_id = Some("uid-bob".into());

    let identity = subscription_identity(&alice);

    assert!(identity.subjects.contains("uid-alice"));
    assert!(!identity.subjects.contains("uid-bob"));
}

#[tokio::test(flavor = "current_thread")]
async fn outgoing_capture_never_downgrades_a_newer_card_from_stale_disk_auth() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);
    let bob = grok_subscription(FULL_CLI_TOKEN);
    storage::upsert_subscription(bob.clone()).unwrap();
    let root = serde_json::json!({
        oidc_scope_key(): {
            "key": "opaque-old-disk-token",
            "refresh_token": "old-disk-refresh",
            "expires_at": "2030-01-01T00:00:00.000000Z",
            "email": "alice@example.com",
            "user_id": "uid-alice",
            "auth_mode": "oidc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": oidc_client_id()
        }
    });
    let mut sessions = MemorySessions::default();

    capture_disk_owner(&root, &mut sessions, None).unwrap();

    let saved = storage::get_subscription(&bob.id).unwrap();
    assert_eq!(
        crypto::decrypt(saved.access_token_encrypted.as_deref().unwrap()),
        FULL_CLI_TOKEN
    );
    assert_eq!(sessions.entries[&bob.id]["key"], FULL_CLI_TOKEN);
}

#[tokio::test(flavor = "current_thread")]
async fn outgoing_capture_fails_closed_when_neither_disk_nor_card_is_restorable() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);
    let alice = grok_subscription(BILLING_ONLY_TOKEN);
    storage::upsert_subscription(alice).unwrap();
    let root = serde_json::json!({
        oidc_scope_key(): {
            "key": "opaque-token-without-expiry-or-refresh",
            "email": "alice@example.com",
            "user_id": "uid-alice"
        }
    });
    let mut sessions = MemorySessions::default();

    let error = capture_disk_owner(&root, &mut sessions, None).unwrap_err();

    assert!(error.to_string().contains("已中止切换以避免覆盖"));
    assert!(sessions.entries.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn disk_session_store_merges_stale_instances_and_forget_removes_only_target() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);
    let mut first = DiskGrokSessionStore::open_default().unwrap();
    let mut second = DiskGrokSessionStore::open_default().unwrap();
    let alice = serde_json::json!({"key": "alice-token"});
    let bob = serde_json::json!({"key": "bob-token"});

    first.save("alice", &alice).unwrap();
    second.save("bob", &bob).unwrap();

    assert_eq!(first.load("alice").unwrap(), Some(alice));
    assert_eq!(first.load("bob").unwrap(), Some(bob.clone()));
    forget("alice").unwrap();
    assert_eq!(second.load("alice").unwrap(), None);
    assert_eq!(second.load("bob").unwrap(), Some(bob));
}

#[test]
fn matching_newer_disk_session_wins_over_stale_snapshot_and_card_token() {
    let alice = grok_subscription(FULL_CLI_TOKEN);
    let live_entry = serde_json::json!({
        "key": "opaque-token-rotated-by-grok",
        "refresh_token": "refresh-rotated-by-grok",
        "expires_at": "2035-01-01T00:00:00.000000Z",
        "email": "alice@example.com",
        "user_id": "uid-alice",
        "auth_mode": "oidc",
        "oidc_issuer": "https://auth.x.ai",
        "oidc_client_id": oidc_client_id()
    });
    let auth = MemoryAuthFile::new(serde_json::json!({
        oidc_scope_key(): live_entry.clone()
    }));
    let mut sessions = MemorySessions::default();
    sessions.entries.insert(
        alice.id.clone(),
        serde_json::json!({
            "key": "stale-snapshot-token",
            "refresh_token": "stale-snapshot-refresh",
            "expires_at": "2030-01-01T00:00:00.000000Z",
            "email": "alice@example.com",
            "user_id": "uid-alice",
            "auth_mode": "oidc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": oidc_client_id()
        }),
    );

    install_target(&auth, &mut sessions, &alice).unwrap();

    let root = auth.root.borrow();
    assert_eq!(root[oidc_scope_key()]["key"], live_entry["key"]);
    assert!(
        root[oidc_scope_key()]["create_time"].is_string(),
        "a matching current entry must be normalized before it is written back"
    );
    assert_eq!(sessions.entries[&alice.id], root[oidc_scope_key()]);
}

#[test]
fn expired_snapshot_without_refresh_token_is_rejected() {
    let alice = grok_subscription(BILLING_ONLY_TOKEN);
    let auth = MemoryAuthFile::new(working_bob_root());
    let mut sessions = MemorySessions::default();
    sessions.entries.insert(
        alice.id.clone(),
        serde_json::json!({
            "key": "opaque-expired-token",
            "expires_at": "2020-01-01T00:00:00.000000Z",
            "email": "alice@example.com",
            "user_id": "uid-alice",
            "auth_mode": "oidc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": oidc_client_id()
        }),
    );

    let error = install_target(&auth, &mut sessions, &alice).unwrap_err();

    assert!(error.requires_reauth);
    assert!(error.message.contains("缺少 refresh token"));
    assert_eq!(auth.commits.get(), 0);
}

#[test]
fn snapshot_without_verifiable_expiry_is_rejected() {
    let alice = grok_subscription(BILLING_ONLY_TOKEN);
    let auth = MemoryAuthFile::new(working_bob_root());
    let mut sessions = MemorySessions::default();
    sessions.entries.insert(
        alice.id.clone(),
        serde_json::json!({
            "key": "opaque-token-with-unknown-expiry",
            "refresh_token": "refresh-without-expiry",
            "email": "alice@example.com",
            "user_id": "uid-alice",
            "auth_mode": "oidc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": oidc_client_id()
        }),
    );

    let error = install_target(&auth, &mut sessions, &alice).unwrap_err();

    assert!(error.requires_reauth);
    assert!(error.message.contains("缺少可验证的有效期"));
    assert_eq!(auth.commits.get(), 0);
}

#[test]
fn jwt_expiry_outranks_incorrect_future_subscription_metadata() {
    let mut alice = grok_subscription(EXPIRED_FULL_CLI_TOKEN);
    alice.access_token_expires_at = Some(1_999_999_999);

    assert_eq!(effective_access_token_expiry(&alice), Some(1));
    assert!(!token_is_known_valid(&alice));
}

#[test]
fn snapshot_persistence_failure_happens_before_auth_commit() {
    let original = working_bob_root();
    let auth = MemoryAuthFile::new(original.clone());
    let mut sessions = MemorySessions {
        fail_save: true,
        ..Default::default()
    };
    let alice = grok_subscription(FULL_CLI_TOKEN);

    let error = install_target(&auth, &mut sessions, &alice).unwrap_err();

    assert!(error.message.contains("snapshot store unavailable"));
    assert_eq!(*auth.root.borrow(), original);
    assert_eq!(auth.commits.get(), 0);
}

#[test]
fn pre_write_revision_conflict_is_reported_without_overwrite() {
    let auth = MemoryAuthFile::new(working_bob_root());
    auth.conflict_before_commit.set(true);
    let mut sessions = MemorySessions::default();
    let alice = grok_subscription(FULL_CLI_TOKEN);

    let error = install_target(&auth, &mut sessions, &alice).unwrap_err();

    assert!(error.message.contains("其他进程修改"));
    assert_eq!(auth.commits.get(), 0);
    assert_eq!(auth.root.borrow()[oidc_scope_key()]["user_id"], "uid-bob");
}

#[test]
fn post_write_overwrite_is_reported_as_failure() {
    let auth = MemoryAuthFile::new(working_bob_root());
    auth.overwrite_after_commit.set(true);
    let mut sessions = MemorySessions::default();
    let alice = grok_subscription(FULL_CLI_TOKEN);

    let error = install_target(&auth, &mut sessions, &alice).unwrap_err();

    assert!(error.message.contains("写入后被覆盖"));
    assert_eq!(auth.commits.get(), 1);
}

#[test]
fn post_replace_failure_reports_that_target_is_already_installed() {
    let auth = MemoryAuthFile::new(working_bob_root());
    auth.fail_after_commit.set(true);
    let mut sessions = MemorySessions::default();
    let mut alice = grok_subscription(FULL_CLI_TOKEN);

    let attempt = install_prepared(&auth, &mut sessions, &mut alice);

    assert!(!attempt.outcome.success);
    assert!(attempt.target_installed);
    assert_eq!(auth.root.borrow()[oidc_scope_key()]["key"], FULL_CLI_TOKEN);
}

#[test]
fn disk_adapter_fails_closed_on_malformed_json() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(".grok/auth.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{not-json").unwrap();
    let auth = DiskGrokAuthFile { path: path.clone() };

    let error = auth.load().unwrap_err();

    assert!(error.to_string().contains("不是有效 JSON"));
    assert_eq!(fs::read(path).unwrap(), b"{not-json");
}

#[cfg(unix)]
#[test]
fn disk_adapter_commits_with_private_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(".grok/auth.json");
    let auth = DiskGrokAuthFile { path: path.clone() };
    let mut loaded = auth.load().unwrap();
    let scope = oidc_scope_key();
    let entry = serde_json::json!({
        "key": FULL_CLI_TOKEN,
        "email": "alice@example.com",
        "user_id": "uid-alice"
    });
    loaded.root[&scope] = entry.clone();

    auth.commit_verified(&loaded, &scope, &entry).unwrap();

    let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

/// Unix-only: the test reads the lock file while the lease holds it, which a
/// Windows region lock forbids (os error 33) while Unix advisory locks allow.
#[cfg(unix)]
#[test]
fn disk_adapter_uses_groks_official_lock_and_updates_holder() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(".grok/auth.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let auth = DiskGrokAuthFile { path: path.clone() };

    let _lease = auth.lock_transaction().unwrap();

    let official_lock = path.with_extension("json.lock");
    let holder = fs::read_to_string(&official_lock).unwrap();
    assert!(holder.starts_with(&format!("{}:", std::process::id())));
    assert!(!path.with_extension("json.skillstar.lock").exists());
}

#[tokio::test(flavor = "current_thread")]
async fn activation_facade_captures_outgoing_then_pins_and_installs_target() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let tool_home = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data_root.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", tool_home.path()),
    ]);

    let mut bob = grok_subscription("opaque-bob-subscription-token");
    bob.id = "grok-bob".into();
    bob.display_name = "bob@example.com".into();
    bob.oauth_account_id = Some("uid-bob".into());
    bob.note = Some("keep bob metadata".into());
    storage::upsert_subscription(bob.clone()).unwrap();
    let alice = grok_subscription(FULL_CLI_TOKEN);
    storage::upsert_subscription(alice.clone()).unwrap();

    let auth_path = tool_home.path().join(".grok/auth.json");
    fs::create_dir_all(auth_path.parent().unwrap()).unwrap();
    let bob_entry = serde_json::json!({
        "key": "opaque-bob-token-rotated-by-cli",
        "refresh_token": "bob-refresh-rotated-by-cli",
        "expires_at": "2033-05-18T03:33:19.000000Z",
        "email": "bob@example.com",
        "user_id": "uid-bob",
        "principal_id": "uid-bob",
        "principal_type": "user",
        "team_id": "team-bob",
        "auth_mode": "oidc",
        "oidc_issuer": "https://auth.x.ai",
        "oidc_client_id": oidc_client_id()
    });
    let initial_root = serde_json::json!({
        oidc_scope_key(): bob_entry,
        "https://accounts.x.ai/sign-in": { "key": "legacy-session" }
    });
    fs::write(
        &auth_path,
        serde_json::to_vec_pretty(&initial_root).unwrap(),
    )
    .unwrap();

    let result = crate::usage_switch::activate_subscription(&alice.id)
        .await
        .unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some(alice.id.as_str())
    );
    let final_root: Value = serde_json::from_slice(&fs::read(&auth_path).unwrap()).unwrap();
    assert_eq!(final_root[oidc_scope_key()]["key"], FULL_CLI_TOKEN);
    assert_eq!(final_root[oidc_scope_key()]["user_id"], "uid-alice");
    assert_eq!(
        final_root["https://accounts.x.ai/sign-in"]["key"],
        "legacy-session"
    );

    let sessions_path = data_root.path().join("config/usage/grok_cli_sessions.json");
    let raw_sessions = fs::read_to_string(&sessions_path).unwrap();
    assert!(!raw_sessions.contains("opaque-bob-token-rotated-by-cli"));
    let sessions: StoredSessions = serde_json::from_str(&raw_sessions).unwrap();
    let bob_snapshot: Value =
        serde_json::from_str(&crypto::decrypt(sessions.entries.get(&bob.id).unwrap())).unwrap();
    assert_eq!(bob_snapshot["key"], "opaque-bob-token-rotated-by-cli");
    assert_eq!(bob_snapshot["team_id"], "team-bob");

    let saved_bob = storage::get_subscription(&bob.id).unwrap();
    assert_eq!(
        crypto::decrypt(saved_bob.access_token_encrypted.as_deref().unwrap()),
        "opaque-bob-token-rotated-by-cli"
    );
    assert_eq!(saved_bob.note.as_deref(), Some("keep bob metadata"));
}

#[tokio::test(flavor = "current_thread")]
async fn official_lease_adopts_cli_rotated_token_before_usage_refresh() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let tool_home = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data_root.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", tool_home.path()),
    ]);
    let mut alice = grok_subscription(FULL_CLI_TOKEN);
    storage::upsert_subscription(alice.clone()).unwrap();
    storage::set_active_subscription("xai", &alice.id).unwrap();
    let auth_path = tool_home.path().join(".grok/auth.json");
    fs::create_dir_all(auth_path.parent().unwrap()).unwrap();
    let disk = serde_json::json!({
        oidc_scope_key(): {
            "key": "opaque-r2-rotated-by-grok",
            "refresh_token": "refresh-r2-rotated-by-grok",
            "expires_at": "2035-01-01T00:00:00.000000Z",
            "email": "alice@example.com",
            "user_id": "uid-alice",
            "auth_mode": "oidc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": oidc_client_id()
        }
    });
    fs::write(&auth_path, serde_json::to_vec_pretty(&disk).unwrap()).unwrap();
    let lease = acquire_refresh_lease().await.unwrap();

    adopt_active_before_refresh(&mut alice, &lease).unwrap();

    assert_eq!(
        crypto::decrypt(alice.access_token_encrypted.as_deref().unwrap()),
        "opaque-r2-rotated-by-grok"
    );
    assert_eq!(
        crypto::decrypt(alice.refresh_token_encrypted.as_deref().unwrap()),
        "refresh-r2-rotated-by-grok"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn active_usage_refresh_projects_new_card_generation_back_to_cli() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let tool_home = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data_root.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", tool_home.path()),
    ]);
    let mut alice = grok_subscription(FULL_CLI_TOKEN);
    storage::upsert_subscription(alice.clone()).unwrap();
    storage::set_active_subscription("xai", &alice.id).unwrap();
    let auth_path = tool_home.path().join(".grok/auth.json");
    fs::create_dir_all(auth_path.parent().unwrap()).unwrap();
    let old_disk = serde_json::json!({
        oidc_scope_key(): {
            "key": "opaque-old-a1",
            "refresh_token": "old-r1",
            "expires_at": "2030-01-01T00:00:00.000000Z",
            "email": "alice@example.com",
            "user_id": "uid-alice",
            "auth_mode": "oidc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": oidc_client_id()
        }
    });
    fs::write(&auth_path, serde_json::to_vec_pretty(&old_disk).unwrap()).unwrap();
    let lease = acquire_refresh_lease().await.unwrap();

    let outcome = sync_refreshed_active(&mut alice, &lease).unwrap();

    assert!(outcome.success, "{:?}", outcome.error);
    let written: Value = serde_json::from_slice(&fs::read(auth_path).unwrap()).unwrap();
    assert_eq!(written[oidc_scope_key()]["key"], FULL_CLI_TOKEN);
    assert!(written[oidc_scope_key()]["create_time"].is_string());
}

#[tokio::test(flavor = "current_thread")]
async fn failed_activation_restores_previous_pin_and_keeps_working_auth() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let tool_home = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data_root.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", tool_home.path()),
    ]);

    let mut bob = grok_subscription("opaque-bob-subscription-token");
    bob.id = "grok-bob".into();
    bob.display_name = "bob@example.com".into();
    bob.oauth_account_id = Some("uid-bob".into());
    storage::upsert_subscription(bob.clone()).unwrap();
    let alice = grok_subscription(BILLING_ONLY_TOKEN);
    storage::upsert_subscription(alice.clone()).unwrap();
    storage::set_active_subscription("xai", &bob.id).unwrap();

    let auth_path = tool_home.path().join(".grok/auth.json");
    fs::create_dir_all(auth_path.parent().unwrap()).unwrap();
    let bob_root = serde_json::json!({
        oidc_scope_key(): {
            "key": "opaque-bob-token-rotated-by-cli",
            "refresh_token": "bob-refresh-rotated-by-cli",
            "expires_at": "2033-05-18T03:33:19.000000Z",
            "email": "bob@example.com",
            "user_id": "uid-bob",
            "auth_mode": "oidc",
            "oidc_issuer": "https://auth.x.ai",
            "oidc_client_id": oidc_client_id()
        }
    });
    fs::write(&auth_path, serde_json::to_vec_pretty(&bob_root).unwrap()).unwrap();

    let result = crate::usage_switch::activate_subscription(&alice.id)
        .await
        .unwrap();

    assert!(!result.switch_result.success);
    assert!(
        result
            .switch_result
            .error
            .as_deref()
            .unwrap()
            .contains("conversations:read")
    );
    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some(bob.id.as_str())
    );
    let after: Value = serde_json::from_slice(&fs::read(auth_path).unwrap()).unwrap();
    assert_eq!(after, bob_root);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_attempt_reconciles_pin_to_another_processes_installed_identity() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);
    let mut bob = grok_subscription(FULL_CLI_TOKEN);
    bob.id = "grok-bob".into();
    bob.oauth_account_id = Some("uid-bob".into());
    bob.access_token_encrypted = Some(crypto::encrypt("opaque-bob-token"));
    let alice = grok_subscription(FULL_CLI_TOKEN);
    storage::upsert_subscription(bob.clone()).unwrap();
    storage::upsert_subscription(alice.clone()).unwrap();
    storage::set_active_subscription("xai", &bob.id).unwrap();
    let auth = MemoryAuthFile::new(serde_json::json!({
        oidc_scope_key(): {
            "key": FULL_CLI_TOKEN,
            "email": "alice@example.com",
            "user_id": "uid-alice"
        }
    }));

    reconcile_active_pin(&auth, Some(&bob.id)).unwrap();

    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some(alice.id.as_str())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn rebound_row_is_not_pinned_when_disk_still_has_the_old_identity() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);
    let bob = grok_subscription(FULL_CLI_TOKEN);
    storage::upsert_subscription(bob.clone()).unwrap();
    storage::set_active_subscription("xai", &bob.id).unwrap();
    let auth = MemoryAuthFile::new(serde_json::json!({
        oidc_scope_key(): {
            "key": "old-alice-token",
            "email": "old-alice@example.com",
            "user_id": "uid-old-alice"
        }
    }));

    reconcile_active_pin(&auth, Some(&bob.id)).unwrap();

    assert_eq!(storage::get_active_subscription("xai").unwrap(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn stale_active_pin_never_claims_another_accounts_disk_session() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let tool_home = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data_root.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", tool_home.path()),
    ]);

    let alice = grok_subscription(BILLING_ONLY_TOKEN);
    storage::upsert_subscription(alice.clone()).unwrap();
    storage::set_active_subscription("xai", &alice.id).unwrap();
    let auth_path = tool_home.path().join(".grok/auth.json");
    fs::create_dir_all(auth_path.parent().unwrap()).unwrap();
    let bob_root = working_bob_root();
    fs::write(&auth_path, serde_json::to_vec_pretty(&bob_root).unwrap()).unwrap();

    let outcome = crate::usage_switch::resync_active_subscription("xai")
        .await
        .unwrap();

    assert!(!outcome.success);
    assert!(
        outcome
            .error
            .as_deref()
            .unwrap()
            .contains("conversations:read")
    );
    let after: Value = serde_json::from_slice(&fs::read(&auth_path).unwrap()).unwrap();
    assert_eq!(after, bob_root);
    let sessions_path = data_root.path().join("config/usage/grok_cli_sessions.json");
    if sessions_path.exists() {
        let sessions: StoredSessions =
            serde_json::from_slice(&fs::read(sessions_path).unwrap()).unwrap();
        assert!(!sessions.entries.contains_key(&alice.id));
    }
}
