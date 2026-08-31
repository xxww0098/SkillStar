//! Antigravity Google OAuth client credentials.
//!
//! Resolution order:
//! 1. `SKILLSTAR_ANTIGRAVITY_CLIENT_ID` / `SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET` env vars
//! 2. Compile-time `option_env!` (release CI / local `cargo build` with env set)
//! 3. `~/.skillstar/config/antigravity_oauth.json`
//! 4. The public desktop OAuth client bundled by Antigravity/Cockpit Tools

use crate::{UsageError, UsageResult};
use serde::Deserialize;

// This is an installed-desktop OAuth client, not a user credential. The same
// public client is used by Antigravity and the referenced Cockpit Tools app;
// environment/file overrides remain available if Google rotates it.
const DEFAULT_CLIENT_ID: &str = concat!(
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep",
    ".apps.googleusercontent.com",
);
const DEFAULT_CLIENT_SECRET: &str = concat!("GOCSPX-", "K58FWR486LdLJ1mLB8sXC4z6qDAf");

#[derive(Debug, Clone, Deserialize)]
struct AntigravityOAuthFile {
    client_id: String,
    client_secret: String,
}

#[derive(Debug, Clone)]
pub struct AntigravityOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}

fn read_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn read_compile_time(key: &str) -> Option<String> {
    match key {
        "SKILLSTAR_ANTIGRAVITY_CLIENT_ID" => option_env!("SKILLSTAR_ANTIGRAVITY_CLIENT_ID")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string),
        "SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET" => option_env!("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn read_config_file() -> Option<AntigravityOAuthFile> {
    let path = skillstar_core::infra::paths::antigravity_oauth_config_path();
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn load_config() -> UsageResult<AntigravityOAuthConfig> {
    let from_file = read_config_file();
    let client_id = read_env("SKILLSTAR_ANTIGRAVITY_CLIENT_ID")
        .or_else(|| read_compile_time("SKILLSTAR_ANTIGRAVITY_CLIENT_ID"))
        .or_else(|| from_file.as_ref().map(|f| f.client_id.clone()))
        .or_else(|| Some(DEFAULT_CLIENT_ID.to_string()));
    let client_secret = read_env("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET")
        .or_else(|| read_compile_time("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET"))
        .or_else(|| from_file.as_ref().map(|f| f.client_secret.clone()))
        .or_else(|| Some(DEFAULT_CLIENT_SECRET.to_string()));

    match (client_id, client_secret) {
        (Some(client_id), Some(client_secret)) => Ok(AntigravityOAuthConfig {
            client_id,
            client_secret,
        }),
        _ => Err(UsageError::Other(
            "Antigravity OAuth client 配置为空".into(),
        )),
    }
}

pub fn antigravity_oauth_config() -> UsageResult<AntigravityOAuthConfig> {
    // Resolve on every call. This lets a running dev build recover after the
    // user adds the optional override file, and mirrors the GitHub login fix.
    load_config()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_config_is_used_when_no_override_exists() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        // SAFETY: serialized by the crate-wide test_env_lock.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
            std::env::remove_var("SKILLSTAR_ANTIGRAVITY_CLIENT_ID");
            std::env::remove_var("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET");
        }

        let config = load_config().expect("bundled OAuth client should be available");
        assert_eq!(config.client_id, DEFAULT_CLIENT_ID);
        assert_eq!(config.client_secret, DEFAULT_CLIENT_SECRET);

        // SAFETY: still serialized by the crate-wide test_env_lock.
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}
