//! Per-provider OAuth client credential resolution (codex / xai / trae / opencode).
//!
//! Each provider ships a built-in default `client_id` (and `client_secret` where
//! applicable) so the app works out of the box. Deployers can override them
//! without touching source — useful when a built-in client gets rotated or a
//! self-hosted proxy registers its own client. Resolution order (first hit wins):
//!
//! 1. `SKILLSTAR_<PROVIDER>_CLIENT_ID` / `_CLIENT_SECRET` env vars
//! 2. Compile-time `option_env!` (release CI / `cargo build` with env set)
//! 3. `~/.skillstar/config/oauth_clients.json` (`{ "<provider>": { "client_id", "client_secret" } }`)
//! 4. Built-in hard-coded default
//!
//! Antigravity is intentionally *not* here — its Google client is required (no
//! built-in default) and lives in [`crate::antigravity_oauth_config`].

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Default, Deserialize)]
struct ClientEntry {
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
}

static FILE_CONFIG: OnceLock<HashMap<String, ClientEntry>> = OnceLock::new();

fn file_config() -> &'static HashMap<String, ClientEntry> {
    FILE_CONFIG.get_or_init(|| {
        let path = skillstar_core::infra::paths::oauth_clients_config_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    })
}

fn non_empty(s: String) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn from_env(var: &str) -> Option<String> {
    std::env::var(var).ok().and_then(non_empty)
}

fn from_file(provider: &str, field: ClientField) -> Option<String> {
    let entry = file_config().get(provider)?;
    let raw = match field {
        ClientField::Id => entry.client_id.clone(),
        ClientField::Secret => entry.client_secret.clone(),
    };
    raw.and_then(non_empty)
}

#[derive(Clone, Copy)]
enum ClientField {
    Id,
    Secret,
}

/// Resolve `client_id` for a provider: env → compile-time → file → built-in default.
///
/// `compile_time` is the caller's `option_env!(...)` result (it must be expanded
/// at the call site so the literal env-var name is baked into that crate).
fn resolve(
    provider: &str,
    env_var: &str,
    compile_time: Option<&'static str>,
    field: ClientField,
    default: &'static str,
) -> String {
    from_env(env_var)
        .or_else(|| compile_time.map(str::to_string).and_then(non_empty))
        .or_else(|| from_file(provider, field))
        .unwrap_or_else(|| default.to_string())
}

/// Resolve a provider's `client_id`. Call as `client_id!("codex", "SKILLSTAR_CODEX_CLIENT_ID", "default")`.
/// The `option_env!` is expanded at the call site so the literal var name is
/// baked into the caller's compilation.
macro_rules! client_id {
    ($provider:literal, $env_var:literal, $default:expr) => {
        $crate::oauth_clients::resolve_client_id(
            $provider,
            $env_var,
            option_env!($env_var),
            $default,
        )
    };
}

/// Resolve a provider's `client_secret`. Same resolution order as [`client_id!`].
macro_rules! client_secret {
    ($provider:literal, $env_var:literal, $default:expr) => {
        $crate::oauth_clients::resolve_client_secret(
            $provider,
            $env_var,
            option_env!($env_var),
            $default,
        )
    };
}

pub(crate) use client_id;
pub(crate) use client_secret;

#[doc(hidden)]
pub fn resolve_client_id(
    provider: &str,
    env_var: &str,
    compile_time: Option<&'static str>,
    default: &'static str,
) -> String {
    resolve(provider, env_var, compile_time, ClientField::Id, default)
}

#[doc(hidden)]
pub fn resolve_client_secret(
    provider: &str,
    env_var: &str,
    compile_time: Option<&'static str>,
    default: &'static str,
) -> String {
    resolve(provider, env_var, compile_time, ClientField::Secret, default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn falls_back_to_default_when_nothing_set() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized by ENV_LOCK.
        unsafe {
            std::env::remove_var("SKILLSTAR_TEST_CLIENT_ID");
        }
        let got = resolve_client_id("nonexistent", "SKILLSTAR_TEST_CLIENT_ID", None, "built-in");
        assert_eq!(got, "built-in");
    }

    #[test]
    fn env_var_overrides_default() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized by ENV_LOCK; restored below.
        unsafe {
            std::env::set_var("SKILLSTAR_TEST_CLIENT_ID", "  from-env  ");
        }
        let got = resolve_client_id("nonexistent", "SKILLSTAR_TEST_CLIENT_ID", None, "built-in");
        unsafe {
            std::env::remove_var("SKILLSTAR_TEST_CLIENT_ID");
        }
        assert_eq!(got, "from-env", "env var should win and be trimmed");
    }

    #[test]
    fn blank_env_var_falls_through_to_default() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized by ENV_LOCK; restored below.
        unsafe {
            std::env::set_var("SKILLSTAR_TEST_CLIENT_ID", "   ");
        }
        let got = resolve_client_id("nonexistent", "SKILLSTAR_TEST_CLIENT_ID", None, "built-in");
        unsafe {
            std::env::remove_var("SKILLSTAR_TEST_CLIENT_ID");
        }
        assert_eq!(got, "built-in", "whitespace-only env var must not be used");
    }
}
