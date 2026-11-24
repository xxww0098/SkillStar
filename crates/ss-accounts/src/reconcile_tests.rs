//! Reconcile tests: the client's own file is the truth, and a client that
//! cannot be read is an empty identity rather than a failed page.

use serde_json::json;
use crate::oauth::{Identity, TokenSet};

use crate::record::Account;
use crate::store;
use crate::test_support::{Sandbox, jwt_with_claims};

fn tokens(access: &str, refresh: Option<&str>) -> TokenSet {
    TokenSet {
        access_token: access.to_string(),
        refresh_token: refresh.map(str::to_string),
        id_token: None,
        expires_at: None,
        token_type: "Bearer".to_string(),
    }
}

fn store_account(agent: &str, identity: Identity, tokens: TokenSet) -> Account {
    let account = Account::from_login(agent, &identity, &tokens, None, 1_000).expect("record");
    store::upsert(&account).expect("store");
    account
}

fn codex_identity(account_id: &str) -> Identity {
    Identity {
        email: None,
        display_name: None,
        plan: None,
        account_id: Some(account_id.to_string()),
    }
}

#[test]
fn the_live_client_is_matched_to_its_row() {
    let sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-live"),
        tokens("at-live", Some("rt-live")),
    );
    // What the Codex CLI itself would hold.
    let auth = json!({ "tokens": { "access_token": "at-live", "account_id": "acc-live" } });
    sandbox.write_file(
        &sandbox.tool_path(&[".codex", "auth.json"]),
        auth.to_string(),
    );

    let live = crate::reconcile().expect("reconcile");
    let codex = live
        .iter()
        .find(|agent| agent.agent == "codex")
        .expect("codex present");
    assert_eq!(codex.identity.as_deref(), Some("acc-live"));
    assert_eq!(codex.account_id.as_deref(), Some(stored.id.as_str()));
    assert!(std::path::Path::new(&codex.source).ends_with(".codex/auth.json"));

    // The other agents have no credential here, and saying so is not an error.
    assert_eq!(live.len(), crate::Agent::supported().len());
    let cursor = live.iter().find(|agent| agent.agent == "cursor").unwrap();
    assert_eq!(cursor.identity, None);
    assert_eq!(cursor.account_id, None);
    let antigravity = live
        .iter()
        .find(|agent| agent.agent == "antigravity")
        .expect("antigravity is one of the agents");
    assert_eq!(antigravity.identity, None);
    assert!(
        std::path::Path::new(&antigravity.source)
            .ends_with("Antigravity IDE/User/globalStorage/state.vscdb"),
        "{}",
        antigravity.source
    );
}

#[test]
fn a_client_with_no_identity_falls_back_to_its_token() {
    let sandbox = Sandbox::new();
    let access = jwt_with_claims(json!({ "sub": "acc-token-only", "exp": 1_800_000_000_i64 }));
    let stored = store_account(
        "codex",
        Identity::default(),
        tokens(&access, Some("rt-token-only")),
    );
    // An old credential with no account_id: the CLI shows no identity, so only
    // the tokens it holds can tie it to a row.
    let auth = json!({ "tokens": { "access_token": access, "refresh_token": "rt-token-only" } });
    sandbox.write_file(
        &sandbox.tool_path(&[".codex", "auth.json"]),
        auth.to_string(),
    );

    let live = crate::reconcile().expect("reconcile");
    let codex = live.iter().find(|agent| agent.agent == "codex").unwrap();
    assert_eq!(codex.identity, None);
    assert_eq!(codex.account_id.as_deref(), Some(stored.id.as_str()));
}

#[test]
fn an_unreadable_client_is_an_empty_identity_not_an_error() {
    let _sandbox = Sandbox::new();
    let _stored = store_account(
        "claude-code",
        Identity {
            email: Some("dev@example.com".to_string()),
            ..Identity::default()
        },
        tokens("at-claude", Some("rt-claude")),
    );

    let live = crate::reconcile().expect("reconcile");
    let claude = live.iter().find(|agent| agent.agent == "claude-code").unwrap();
    assert_eq!(claude.identity, None);
    assert_eq!(claude.account_id, None);
    assert!(claude.source.ends_with(".credentials.json"));
}

#[test]
fn fingerprint_matching_uses_index_without_decrypting_records() {
    let sandbox = Sandbox::new();
    let stored = store_account(
        "claude-code",
        Identity::default(),
        tokens("sk-ant-oat01-live", Some("sk-ant-ort01-live")),
    );
    // Corrupt the encrypted token field on disk (and clear email/accountId):
    // if reconcile tried to open and decrypt the record file, matching would fail.
    let record_path = store::record_path(&stored.id);
    let mut raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record_path).unwrap()).unwrap();
    raw["tokensEncrypted"] = json!("not-decryptable-ciphertext");
    std::fs::write(&record_path, serde_json::to_vec(&raw).unwrap()).unwrap();

    sandbox.write_file(
        &sandbox.tool_path(&[".claude", ".credentials.json"]),
        json!({
            "claudeAiOauth": {
                "accessToken": "sk-ant-oat01-live",
                "refreshToken": "sk-ant-ort01-live"
            }
        })
        .to_string(),
    );

    let matched = super::live_for_agent("claude-code").expect("claude-code live");
    assert_eq!(matched.account_id.as_deref(), Some(stored.id.as_str()));
}
