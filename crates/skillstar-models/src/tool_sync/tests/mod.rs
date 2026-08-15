//! Unit + property tests for the tool_sync module.
//!
//! Shared sandbox-home guard and builder helpers live here; the actual
//! test cases are split across `part1`..`part5` to keep each file small.

mod golden;
mod part1;
mod part2;
mod part3;
mod part4;
mod part5;
mod part6;

use super::*;
use crate::providers::{BindingEntry, Credential, Endpoints, Provider};
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

/// A fully-populated v4 provider row for writer tests.
fn make_test_provider_flat() -> Provider {
    let mut provider = Provider::new("test-uuid-1234", "Test Provider");
    provider.endpoints = Endpoints {
        openai_chat: Some("https://api.example.com/v1".to_string()),
        // Present so the Codex writer tests have a bindable row. A host with no
        // Responses endpoint is correctly refused, which part6 covers.
        openai_responses: Some("https://api.example.com/v1".to_string()),
        anthropic_messages: Some("https://api.example.com/anthropic".to_string()),
        models_list: Some("https://api.example.com/v1/models".to_string()),
    };
    provider.credential = Credential::single_key("k1", "sk-test-key-flat-12345");
    provider.models = vec!["model-a".to_string(), "model-b".to_string()];
    provider.default_model = Some("model-a".to_string());
    provider.preset_id = Some("test-preset".to_string());
    provider.icon_color = Some("#FF0000".to_string());
    provider.created_at_ms = Some(1719000000000);
    provider
}

/// A minimal chat-only relay row: the shape most writer tests want.
fn flat(id: &str, name: &str) -> Provider {
    let mut provider = Provider::new(id, name);
    provider.endpoints.openai_chat = Some(format!("https://{name}.example.com/v1"));
    provider.credential = Credential::single_key(format!("k-{id}"), format!("sk-{id}"));
    provider.models = vec!["model-a".to_string(), "model-b".to_string()];
    provider.default_model = Some("model-a".to_string());
    provider
}

/// The same, but able to serve Codex — i.e. it has a `/v1/responses` endpoint.
///
/// Named for what it enables rather than for a URL, because "has a responses
/// endpoint" is the single fact that decides whether Codex can be bound at all.
fn responses_capable(id: &str, name: &str) -> Provider {
    let mut provider = flat(id, name);
    provider.endpoints.openai_responses = Some(format!("https://{name}.example.com/v1"));
    provider.caps.responses_api = crate::providers::Tri::Yes;
    provider
}

fn entry(provider_id: &str, model: &str) -> BindingEntry {
    BindingEntry::new(provider_id, model)
}

/// Give the current test its own model-catalog cache directory.
///
/// Thread-local, for the same reason [`use_sandbox_home`] is: an env var is
/// process-global, so one test's sandbox would re-root every test running
/// beside it, and dropping that test's temp dir would leave the others writing
/// into a directory that no longer exists.
#[must_use = "the override is only installed while the guard is alive"]
struct DataDirSandbox {
    _dir: TempDir,
    prev: Option<std::path::PathBuf>,
}

impl DataDirSandbox {
    fn new() -> Self {
        let dir = TempDir::new().expect("create catalog cache sandbox");
        let prev = crate::providers::catalog_cache::set_test_cache_override(Some(
            dir.path().to_path_buf(),
        ));
        Self { _dir: dir, prev }
    }
}

impl Drop for DataDirSandbox {
    fn drop(&mut self) {
        crate::providers::catalog_cache::set_test_cache_override(self.prev.take());
    }
}

/// No role assignments — what most Claude writer tests want.
fn no_roles() -> std::collections::BTreeMap<String, crate::providers::ModelRef> {
    std::collections::BTreeMap::new()
}
