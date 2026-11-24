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
use std::sync::{Mutex, OnceLock};

use crate::oauth::error::{OAuthError, OAuthResult};

static SHARED_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static RUNTIME_INIT_LOCK: Mutex<()> = Mutex::new(());

/// Shared multi-threaded Tokio runtime with time and IO drivers enabled so
/// `SHARED_PROBE_CLIENT` keeps pooled TLS/HTTP2 connections alive across calls.
/// Successful builds are cached once; transient build errors are not cached.
pub(crate) fn shared_runtime() -> Result<&'static tokio::runtime::Runtime, std::io::Error> {
    if let Some(rt) = SHARED_RUNTIME.get() {
        return Ok(rt);
    }
    let _guard = RUNTIME_INIT_LOCK
        .lock()
        .expect("shared runtime init lock poisoned");
    if let Some(rt) = SHARED_RUNTIME.get() {
        return Ok(rt);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("ss-accounts-net")
        .enable_all()
        .build()?;
    let _ = SHARED_RUNTIME.set(rt);
    Ok(SHARED_RUNTIME
        .get()
        .expect("shared runtime set right above"))
}

/// Drive one future to completion on the shared network runtime.
pub(crate) fn block_on<F: Future>(future: F) -> OAuthResult<F::Output> {
    let runtime = shared_runtime()
        .map_err(|error| OAuthError::Other(format!("无法启动网络运行时: {error}")))?;
    Ok(runtime.block_on(future))
}
