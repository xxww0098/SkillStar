//! URLs this process handed to an HTTP client.
//!
//! Subscription Claude never adds to this list. Tests read it to prove the
//! bridge did not call Anthropic.

use std::sync::Mutex;

static OUTBOUND: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn note_outbound(url: &str) {
    OUTBOUND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(url.to_string());
}

/// URLs recorded since the last [`clear_outbound_log`].
pub fn outbound_log() -> Vec<String> {
    OUTBOUND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Drop recorded URLs. Parallel tests that only record loopback callbacks
/// may still append after this returns.
pub fn clear_outbound_log() {
    OUTBOUND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}
