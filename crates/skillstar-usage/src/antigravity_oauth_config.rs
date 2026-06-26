//! Antigravity Google OAuth client credentials (not committed — see `.env.example`).
//!
//! Resolution order:
//! 1. `SKILLSTAR_ANTIGRAVITY_CLIENT_ID` / `SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET` env vars
//! 2. Compile-time `option_env!` (release CI / local `cargo build` with env set)
//! 3. `~/.skillstar/config/antigravity_oauth.json`

use serde::Deserialize;
use std::sync::OnceLock;

use crate::{UsageError, UsageResult};

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

static CONFIG: OnceLock<UsageResult<AntigravityOAuthConfig>> = OnceLock::new();

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
        .or_else(|| from_file.as_ref().map(|f| f.client_id.clone()));
    let client_secret = read_env("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET")
        .or_else(|| read_compile_time("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET"))
        .or_else(|| from_file.as_ref().map(|f| f.client_secret.clone()));

    match (client_id, client_secret) {
        (Some(client_id), Some(client_secret)) => Ok(AntigravityOAuthConfig {
            client_id,
            client_secret,
        }),
        _ => Err(UsageError::Other(
            "Antigravity OAuth 未配置：请设置 SKILLSTAR_ANTIGRAVITY_CLIENT_ID / \
             SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET，或写入 ~/.skillstar/config/antigravity_oauth.json"
                .into(),
        )),
    }
}

pub fn antigravity_oauth_config() -> UsageResult<&'static AntigravityOAuthConfig> {
    match CONFIG.get_or_init(load_config) {
        Ok(cfg) => Ok(cfg),
        Err(e) => Err(UsageError::Other(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_lock() -> &'static Mutex<()> {
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn missing_config_returns_error() {
        let _guard = env_lock().lock().expect("env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        // SAFETY: serialized by ENV_LOCK; vars restored before return.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
            std::env::remove_var("SKILLSTAR_ANTIGRAVITY_CLIENT_ID");
            std::env::remove_var("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET");
        }

        let err = load_config().expect_err("expected missing config error");
        assert!(err.to_string().contains("Antigravity OAuth"));
    }
}