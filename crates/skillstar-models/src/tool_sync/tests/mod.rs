//! Unit + property tests for the tool_sync module.
//!
//! Shared sandbox-home guard and builder helpers live here; the actual
//! test cases are split across `part1`/`part2`/`part3` to keep each file small.

mod part1;
mod part2;
mod part3;
mod part4;

use super::*;
use std::collections::HashMap;
use tempfile::TempDir;

/// A throwaway HOME sandbox owned by **one** test, alive while the guard is.
///
/// Every test gets its own temp dir, so the real `~/.claude`, `~/.codex`,
/// `~/.config/opencode`, … are NEVER touched by the suite *and* two parallel
/// tests can no longer clobber each other's `~/.codex/auth.json` (they used to
/// share a single process-wide sandbox, which made this suite flaky).
///
/// The override is thread-local, not the process-global [`TOOL_SYNC_HOME_ENV`]:
/// libtest runs one test per thread, while `set_var` is process-wide and would
/// simply move the race. Any test that drives a home-resolving sync MUST hold a
/// guard from [`use_sandbox_home`] for its whole body.
#[must_use = "the sandbox is only installed while the guard is alive"]
struct SandboxHome {
    _dir: TempDir,
    prev: Option<std::path::PathBuf>,
}

impl Drop for SandboxHome {
    fn drop(&mut self) {
        set_test_home_override(self.prev.take());
    }
}

/// Give the current test its own sandbox HOME. Call at the top of any test that
/// exercises a home-resolving sync function, binding the returned guard.
fn use_sandbox_home() -> SandboxHome {
    let dir = TempDir::new().expect("create tool-sync sandbox home");
    let prev = set_test_home_override(Some(dir.path().to_path_buf()));
    SandboxHome { _dir: dir, prev }
}

fn make_test_provider_flat() -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: "test-uuid-1234".to_string(),
        name: "Test Provider".to_string(),
        base_url_openai: "https://api.example.com/v1".to_string(),
        base_url_anthropic: "https://api.example.com/anthropic".to_string(),
        models_url: "https://api.example.com/v1/models".to_string(),
        api_key: "sk-test-key-flat-12345".to_string(),
        models: vec!["model-a".to_string(), "model-b".to_string()],
        default_model: "model-a".to_string(),
        sort_index: 0,
        preset_id: Some("test-preset".to_string()),
        icon_color: Some("#FF0000".to_string()),
        notes: None,
        created_at: Some(1719000000000),
        meta: None,
        codex_wire_api: "responses".to_string(),
        codex_auth_mode: "api_key".to_string(),
    }
}
