//! A blocking bridge to the async HTTP client.
//!
//! `start`, `complete` and `refresh` are synchronous entry points — the GUI and
//! the CLI call them from a blocking thread — while `probe_http_client` (the
//! only HTTP client allowed to leave this workspace) is async. A short-lived
//! runtime bridges the two, the same way `ss-skills::git::gh_rest` does.
//!
//! Only call these from a blocking context: starting a runtime from inside an
//! async task panics.

use std::future::Future;

use crate::oauth::error::{OAuthError, OAuthResult};

/// A current-thread runtime with the time and IO drivers on.
pub(crate) fn runtime() -> OAuthResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| OAuthError::Other(format!("无法启动网络运行时: {error}")))
}

/// Drive one future to completion on a fresh runtime.
pub(crate) fn block_on<F: Future>(future: F) -> OAuthResult<F::Output> {
    Ok(runtime()?.block_on(future))
}
