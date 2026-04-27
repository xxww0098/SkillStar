//! Codex configuration management.
//!
//! Config file: `~/.codex/config.toml` — model provider, base_url, model settings

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::atomic_write;

/// Returns the Codex config directory: `~/.codex/`
pub fn config_dir() -> PathBuf {
    crate::home_dir().join(".codex")
}

/// Returns the path to `~/.codex/config.toml`
pub fn config_toml_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Returns the path to `~/.codex/auth.json`
pub fn auth_json_path() -> PathBuf {
    config_dir().join("auth.json")
}

/// Read auth.json as JSON Value.
/// Returns empty JSON object if the file does not exist.
pub fn read_auth() -> Result<serde_json::Value> {
    let path = auth_json_path();
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = std::fs::read_to_string(&path).context("Failed to read Codex auth.json")?;
    Ok(serde_json::from_str(&text).unwrap_or(serde_json::json!({})))
}

/// Overwrite specific key-value pairs into auth.json and clear OAuth fields if it's an API Key setup.
/// In SkillStar, `auth.json` configures the actual running Codex CLI. OAuth tokens are safely
/// preserved in `~/.skillstar/config/codex_accounts` database, so it is safe to overwrite them here
/// to ensure Codex launches in API Key mode instead of ChatGPT Session mode.
pub fn merge_auth_fields(fields: &std::collections::HashMap<String, String>) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;

    // We overwrite entirely to guarantee mutual exclusivity.
    // Mixing `tokens` and `OPENAI_API_KEY` causes Codex CLI to ignore the API key.
    let mut obj = serde_json::Map::new();

    for (key, value) in fields {
        if !value.is_empty() {
            obj.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
    }

    let json_text = serde_json::to_string_pretty(&serde_json::Value::Object(obj))?;
    atomic_write(&auth_json_path(), json_text.as_bytes())
        .context("Failed to write Codex auth.json")?;

    Ok(())
}

/// Auth status for frontend display.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAuthStatus {
    /// Whether a ChatGPT OAuth session token is present in auth.json
    pub has_chatgpt_session: bool,
    /// Map of env_key name → whether it has a non-empty value (don't expose secrets)
    pub configured_keys: std::collections::HashMap<String, bool>,
}

/// Read auth.json and return a structured status without exposing secret values.
pub fn read_auth_status() -> Result<CodexAuthStatus> {
    let auth = read_auth()?;
    let obj = auth.as_object();

    // Check nested tokens format (new style: { "tokens": { "access_token": ... } })
    let has_nested_session = obj
        .and_then(|o| o.get("tokens"))
        .and_then(|t| t.get("access_token"))
        .and_then(|v| v.as_str())
        .map_or(false, |s| !s.is_empty());

    // Also check flat format for backwards compatibility
    let has_flat_session = obj
        .and_then(|o| o.get("access_token"))
        .and_then(|v| v.as_str())
        .map_or(false, |s| !s.is_empty());

    let has_chatgpt_session = has_nested_session || has_flat_session;

    let mut configured_keys = std::collections::HashMap::new();
    // Known OAuth-internal fields to skip
    let oauth_fields = [
        "access_token",
        "refresh_token",
        "expires_at",
        "token_type",
        "scope",
        "tokens",
        "last_refresh",
    ];
    if let Some(o) = obj {
        for (key, val) in o {
            if oauth_fields.contains(&key.as_str()) {
                continue;
            }
            let has_value = val.as_str().map_or(false, |s| !s.is_empty());
            configured_keys.insert(key.clone(), has_value);
        }
    }

    Ok(CodexAuthStatus {
        has_chatgpt_session,
        configured_keys,
    })
}

/// Read config.toml as raw text.
/// Returns empty string if the file does not exist.
pub fn read_config_text() -> Result<String> {
    let path = config_toml_path();
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).context("Failed to read Codex config.toml")
}

/// Atomically write config.toml.
pub fn write_config(config_text: &str) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;

    atomic_write(&config_toml_path(), config_text.as_bytes())
        .context("Failed to write Codex config.toml")?;

    Ok(())
}

/// Set a single field in config.toml, preserving formatting.
/// Supports dot-separated key paths (e.g. "features.fast_mode").
/// The `value` is an optional TOML-encoded string. If None, the field is removed.
pub fn set_field(key: &str, value: Option<&str>) -> Result<()> {
    use toml_edit::DocumentMut;

    let text = read_config_text()?;
    let mut doc: DocumentMut = text.parse().unwrap_or_else(|_| "".parse().unwrap());

    let parts: Vec<&str> = key.split('.').collect();
    match parts.len() {
        1 => {
            if let Some(v_str) = value {
                let parsed = v_str
                    .parse::<toml_edit::Value>()
                    .unwrap_or_else(|_| toml_edit::Value::from(v_str));
                doc[parts[0]] = toml_edit::Item::Value(parsed);
            } else {
                doc.remove(parts[0]);
            }
        }
        2 => {
            if let Some(v_str) = value {
                if doc.get(parts[0]).is_none() {
                    doc[parts[0]] = toml_edit::Item::Table(toml_edit::Table::new());
                }
                let parsed = v_str
                    .parse::<toml_edit::Value>()
                    .unwrap_or_else(|_| toml_edit::Value::from(v_str));
                if let Some(table) = doc[parts[0]].as_table_mut() {
                    table.insert(parts[1], toml_edit::Item::Value(parsed));
                }
            } else {
                if let Some(table) = doc[parts[0]].as_table_mut() {
                    table.remove(parts[1]);
                }
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Deep nesting mapping not supported: {}",
                key
            ));
        }
    }

    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    atomic_write(&config_toml_path(), doc.to_string().as_bytes())
        .context("Failed to write Codex config.toml")
}

/// Check if the Codex config.toml exists.
pub fn config_exists() -> bool {
    config_toml_path().exists()
}

/// Get the resolved config path as a string (for frontend display).
pub fn config_path_string() -> String {
    config_dir().to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::test_env_lock()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    fn with_temp_home<F>(suffix: &str, f: F) -> anyhow::Result<()>
    where
        F: FnOnce() -> anyhow::Result<()>,
    {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root = std::env::temp_dir().join(format!("skillstar-codex-{}-{}", suffix, stamp));
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = f();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    #[test]
    fn read_auth_returns_empty_object_when_missing() -> anyhow::Result<()> {
        with_temp_home("missing-auth", || {
            let auth = read_auth()?;
            assert_eq!(auth, serde_json::json!({}));
            Ok(())
        })
    }

    #[test]
    fn merge_auth_fields_writes_clean_json() -> anyhow::Result<()> {
        with_temp_home("merge-auth", || {
            let mut fields = std::collections::HashMap::new();
            fields.insert("OPENAI_API_KEY".to_string(), "sk-test".to_string());
            fields.insert("EMPTY_FIELD".to_string(), "".to_string());
            merge_auth_fields(&fields)?;

            let text = std::fs::read_to_string(auth_json_path())?;
            let parsed: serde_json::Value = serde_json::from_str(&text)?;
            assert_eq!(parsed["OPENAI_API_KEY"], "sk-test");
            assert!(!parsed.as_object().unwrap().contains_key("EMPTY_FIELD"));
            Ok(())
        })
    }

    #[test]
    fn read_auth_status_detects_oauth_tokens() -> anyhow::Result<()> {
        with_temp_home("auth-status", || {
            let nested = serde_json::json!({
                "tokens": { "access_token": "secret" },
                "OPENAI_API_KEY": "key"
            });
            std::fs::create_dir_all(config_dir())?;
            std::fs::write(auth_json_path(), nested.to_string())?;

            let status = read_auth_status()?;
            assert!(status.has_chatgpt_session);
            assert!(status.configured_keys.contains_key("OPENAI_API_KEY"));
            assert!(!status.configured_keys.contains_key("tokens"));

            let flat = serde_json::json!({ "access_token": "secret" });
            std::fs::write(auth_json_path(), flat.to_string())?;
            let status2 = read_auth_status()?;
            assert!(status2.has_chatgpt_session);

            Ok(())
        })
    }

    #[test]
    fn write_config_roundtrip() -> anyhow::Result<()> {
        with_temp_home("config-roundtrip", || {
            let text = "model = \"gpt-5.4\"\napproval_policy = \"suggest\"\n";
            write_config(text)?;
            assert!(config_exists());
            let read_back = read_config_text()?;
            assert_eq!(read_back.trim(), text.trim());
            Ok(())
        })
    }

    #[test]
    fn set_field_creates_and_updates() -> anyhow::Result<()> {
        with_temp_home("set-field", || {
            write_config("")?;
            set_field("model", Some("\"gpt-5.4\""))?;
            set_field("features.fast_mode", Some("true"))?;

            let text = read_config_text()?;
            assert!(text.contains("model = \"gpt-5.4\""));
            assert!(text.contains("[features]"));
            assert!(text.contains("fast_mode = true"));

            set_field("model", None)?;
            let text2 = read_config_text()?;
            assert!(!text2.contains("model"));

            Ok(())
        })
    }
}
