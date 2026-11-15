//! Store round-trip, self-healing index, and narrow-update tests.

use super::{
    StorageLock, get, index, list, record_path, records_dir, rebuild_index, remove, update, upsert,
    upsert_replacing_identity,
};
use crate::record::Account;
use crate::test_support::Sandbox;
use crate::oauth::{Identity, TokenSet};

fn tokens(marker: &str) -> TokenSet {
    TokenSet {
        access_token: format!("at-{marker}"),
        refresh_token: Some(format!("rt-{marker}")),
        id_token: Some(format!("id-{marker}")),
        expires_at: None,
        token_type: "Bearer".to_string(),
    }
}

fn account(agent: &str, email: &str, created_at: i64) -> Account {
    let identity = Identity {
        email: Some(email.to_string()),
        display_name: None,
        plan: None,
        account_id: None,
    };
    Account::from_login(agent, &identity, &tokens(email), None, created_at).unwrap()
}

#[test]
fn round_trips_one_account_through_the_index_and_the_record() {
    let sandbox = Sandbox::new();
    let stored = account("codex", "dev@example.com", 100);
    upsert(&stored).unwrap();

    let loaded = get(&stored.id).unwrap();
    assert_eq!(loaded.id, stored.id);
    assert_eq!(loaded.email.as_deref(), Some("dev@example.com"));
    assert_eq!(loaded.tokens().unwrap().access_token, "at-dev@example.com");
    assert!(record_path(&stored.id).is_file());

    let entries = index().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, stored.id);
    assert_eq!(entries[0].display_name, "dev@example.com");
    assert_eq!(entries[0].created_at, 100);

    // The cheap index never carries a secret.
    let index_text = std::fs::read_to_string(super::index_path()).unwrap();
    assert!(!index_text.contains("at-dev@example.com"));
    assert!(!index_text.contains(&stored.tokens_encrypted));

    let all = list().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(
        sandbox.root().join("data").join("secrets").join("accounts"),
        super::root()
    );

    // Lock and reload: the lock file lives under runtime/locks, not the data.
    assert!(StorageLock::acquire().is_ok());
}

#[cfg(unix)]
#[test]
fn record_and_index_are_sealed() {
    use std::os::unix::fs::PermissionsExt;

    let _sandbox = Sandbox::new();
    let stored = account("codex", "dev@example.com", 100);
    upsert(&stored).unwrap();

    for path in [record_path(&stored.id), super::index_path()] {
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{} is not 0600", path.display());
    }
}

#[test]
fn index_rebuilds_from_records_when_it_is_missing_or_stale() {
    let _sandbox = Sandbox::new();
    let first = account("codex", "one@example.com", 100);
    let second = account("claude-code", "two@example.com", 200);
    upsert(&first).unwrap();
    upsert(&second).unwrap();

    // Losing the index is survivable: the records are the truth.
    std::fs::remove_file(super::index_path()).unwrap();
    let entries = index().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, first.id);
    assert_eq!(entries[1].id, second.id);
    assert!(super::index_path().is_file());

    // A record that vanished behind the index's back drops out of it.
    std::fs::remove_file(record_path(&second.id)).unwrap();
    let entries = index().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, first.id);

    // A garbage index is rebuilt, not trusted.
    std::fs::write(super::index_path(), b"{ not json").unwrap();
    let entries = rebuild_index().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(index().unwrap().len(), 1);
}

#[test]
fn narrow_update_touches_only_what_the_closure_sets() {
    let _sandbox = Sandbox::new();
    let stored = account("codex", "dev@example.com", 100);
    upsert(&stored).unwrap();

    let renamed = update(&stored.id, |row| {
        row.display_name = "Work laptop".to_string();
        Ok(())
    })
    .unwrap();
    assert_eq!(renamed.display_name, "Work laptop");

    // A refresh-shaped patch must not undo the rename.
    let patched = update(&stored.id, |row| {
        row.set_tokens(&tokens("rotated"))?;
        row.requires_reauth = true;
        Ok(())
    })
    .unwrap();
    assert_eq!(patched.display_name, "Work laptop");
    assert_eq!(patched.email.as_deref(), Some("dev@example.com"));
    assert_eq!(patched.created_at, 100);
    assert!(patched.requires_reauth);
    assert_eq!(patched.tokens().unwrap().access_token, "at-rotated");

    let indexed = index().unwrap();
    assert_eq!(indexed[0].display_name, "Work laptop");
    assert!(indexed[0].requires_reauth);
}

#[test]
fn removing_an_account_leaves_no_record_and_no_index_entry() {
    let _sandbox = Sandbox::new();
    let kept = account("codex", "keep@example.com", 100);
    let gone = account("codex", "gone@example.com", 200);
    upsert(&kept).unwrap();
    upsert(&gone).unwrap();

    remove(&gone.id).unwrap();
    assert!(!record_path(&gone.id).exists());
    assert!(matches!(get(&gone.id), Err(crate::AccountError::NotFound(_))));
    let entries = index().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, kept.id);

    // Removing something twice is not an error (a double click must not fail).
    remove(&gone.id).unwrap();
}

#[test]
fn the_same_identity_replaces_its_row_in_place() {
    let _sandbox = Sandbox::new();
    let other = account("codex", "other@example.com", 50);
    upsert(&other).unwrap();
    let first = account("codex", "dev@example.com", 100);
    upsert(&first).unwrap();
    update(&first.id, |row| {
        row.display_name = "Renamed by hand".to_string();
        row.last_used_at = Some(150);
        Ok(())
    })
    .unwrap();

    // A fresh login hands over new tokens but the same identity.
    let relogin = account("codex", "dev@example.com", 900);
    let replaced = upsert_replacing_identity(relogin).unwrap();

    assert_eq!(replaced.id, first.id, "the row must keep its id");
    assert_eq!(replaced.created_at, 100, "creation time survives a re-login");
    assert_eq!(replaced.last_used_at, Some(150));
    assert_eq!(replaced.display_name, "Renamed by hand");
    assert_eq!(
        replaced.tokens().unwrap().access_token,
        "at-dev@example.com",
        "the new credential is what gets stored"
    );

    let mut ids: Vec<String> = list().unwrap().into_iter().map(|row| row.id).collect();
    ids.sort();
    let mut expected = vec![first.id.clone(), other.id.clone()];
    expected.sort();
    assert_eq!(ids, expected);
}

#[test]
fn identity_matching_uses_the_provider_account_id_too() {
    let _sandbox = Sandbox::new();
    let identity = Identity {
        email: None,
        display_name: None,
        plan: None,
        account_id: Some("acc-1".to_string()),
    };
    let first = Account::from_login("codex", &identity, &tokens("one"), None, 100).unwrap();
    upsert(&first).unwrap();

    let second = Account::from_login("codex", &identity, &tokens("two"), None, 200).unwrap();
    let replaced = upsert_replacing_identity(second).unwrap();
    assert_eq!(replaced.id, first.id);
    assert_eq!(replaced.account_id.as_deref(), Some("acc-1"));
    assert_eq!(list().unwrap().len(), 1);
}

#[test]
fn records_with_a_wrong_or_traversing_id_are_rejected() {
    let _sandbox = Sandbox::new();
    assert!(matches!(
        get("../secrets"),
        Err(crate::AccountError::NotFound(_))
    ));
    assert!(matches!(get("nope"), Err(crate::AccountError::NotFound(_))));

    let stored = account("codex", "dev@example.com", 100);
    upsert(&stored).unwrap();
    // A record filed under a name its own content contradicts is not trusted.
    std::fs::create_dir_all(records_dir()).unwrap();
    std::fs::write(record_path("someone-else"), serde_json::to_vec(&stored).unwrap()).unwrap();
    assert!(matches!(
        get("someone-else"),
        Err(crate::AccountError::Client(_))
    ));
    // A record that cannot be trusted is skipped rather than failing the page.
    assert_eq!(list().unwrap().len(), 1);
}
#[test]
fn the_per_agent_refresh_lock_is_exclusive_but_per_agent() {
    let _sandbox = Sandbox::new();
    {
        let first = super::AgentRefreshLock::acquire("codex").expect("first turn");
        // A different agent is a different file, so it does not queue behind.
        let other = super::AgentRefreshLock::acquire("cursor").expect("other agent");
        drop(other);
        assert!(super::refresh_lock_path("codex").is_file());
        drop(first);
    }
    // Released on drop: taking it again from the same process must not block.
    super::AgentRefreshLock::acquire("codex").expect("second turn");
    assert!(super::refresh_lock_path("codex").ends_with("catalog-codex.lock"));
}
