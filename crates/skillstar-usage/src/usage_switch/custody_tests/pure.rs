//! Target-level pure-function units: no sandbox and no filesystem, just
//! the access-token / expiry / identity / link-mode contracts.

use super::*;

// ── target-level units (no filesystem) ───────────────────────────────────

#[test]
fn opencode_expiry_is_read_as_milliseconds() {
    let target = OpenCodeTarget;
    let root =
        json!({ "opencode": { "type": "oauth", "access": "a", "expires": 1_700_000_000_000i64 } });
    assert_eq!(target.expires_at(&root), Some(1_700_000_000));
    assert!(
        target
            .expires_at(&json!({ "opencode": { "type": "api", "key": "k" } }))
            .is_none()
    );
}

#[test]
fn an_opencode_credential_has_no_identity_so_it_can_never_falsely_conflict() {
    let target = OpenCodeTarget;
    let identity = target.identity(&json!({ "opencode": { "type": "api", "key": "k" } }));
    assert!(identity.is_empty());
    assert!(
        !identity.conflicts(&super::subscription_identity(&subscription(
            "x", "opencode"
        )))
    );
}

#[test]
fn link_mode_names_are_stable() {
    assert_eq!(LinkMode::Symlink.as_str(), "symlink");
    assert_eq!(LinkMode::Copy.as_str(), "copy");
}
