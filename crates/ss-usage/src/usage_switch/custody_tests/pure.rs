//! Target-level pure-function units: no sandbox and no filesystem, just
//! the access-token / expiry / identity / link-mode contracts.

use super::*;

// ── target-level units (no filesystem) ───────────────────────────────────

#[test]
fn link_mode_names_are_stable() {
    assert_eq!(LinkMode::Symlink.as_str(), "symlink");
    assert_eq!(LinkMode::Copy.as_str(), "copy");
}
