//! Tauri commands for OAuth and account management.

use crate::core::app_shell::refresh_tray_menu;
use crate::core::infra::error::AppError;
use crate::core::model_config::{
    codex, codex_accounts, codex_oauth, opencode,
};

// ── Codex Auth ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_codex_auth() -> Result<serde_json::Value, AppError> {
    codex::read_auth().map_err(|e| AppError::Other(format!("Codex auth read error: {e}")))
}

#[tauri::command]
pub async fn save_codex_auth(
    fields: std::collections::HashMap<String, String>,
) -> Result<(), AppError> {
    codex::merge_auth_fields(&fields)
        .map_err(|e| AppError::Other(format!("Codex auth merge error: {e}")))
}

#[tauri::command]
pub async fn get_codex_auth_status() -> Result<codex::CodexAuthStatus, AppError> {
    codex::read_auth_status().map_err(|e| AppError::Other(format!("Codex auth status error: {e}")))
}

// ── Codex OAuth + Multi-Account ────────────────────────────────────

#[tauri::command]
pub async fn codex_oauth_start(
    app: tauri::AppHandle,
) -> Result<codex_oauth::OAuthLoginStartResponse, AppError> {
    codex_oauth::start_oauth_login(app)
        .await
        .map_err(|e| AppError::Other(format!("OAuth start error: {e}")))
}

#[tauri::command]
pub async fn codex_oauth_complete(
    app: tauri::AppHandle,
    login_id: String,
) -> Result<codex_accounts::CodexAccount, AppError> {
    let tokens = codex_oauth::complete_oauth_login(&login_id)
        .await
        .map_err(|e| AppError::Other(format!("OAuth complete error: {e}")))?;

    let account = codex_accounts::create_account_from_tokens(tokens)
        .map_err(|e| AppError::Other(format!("Create account error: {e}")))?;

    let _ = refresh_tray_menu(&app);
    Ok(account)
}

#[tauri::command]
pub async fn codex_oauth_cancel(login_id: Option<String>) -> Result<(), AppError> {
    codex_oauth::cancel_oauth_flow(login_id.as_deref())
        .map_err(|e| AppError::Other(format!("OAuth cancel error: {e}")))
}

#[tauri::command]
pub async fn codex_oauth_submit_callback(
    login_id: String,
    callback_url: String,
) -> Result<(), AppError> {
    codex_oauth::submit_callback_url(&login_id, &callback_url)
        .map_err(|e| AppError::Other(format!("OAuth callback error: {e}")))
}

// ── Gemini OAuth ───────────────────────────────────────────────────

#[tauri::command]
pub async fn gemini_oauth_start()
-> Result<crate::core::model_config::gemini_oauth::GeminiOAuthStartResponse, AppError> {
    crate::core::model_config::gemini_oauth::start_login()
        .await
        .map_err(|e| AppError::Other(format!("Gemini OAuth start error: {e}")))
}

#[tauri::command]
pub async fn gemini_oauth_complete(
    login_id: String,
) -> Result<crate::core::model_config::gemini_oauth::GeminiOAuthCompletePayload, AppError> {
    crate::core::model_config::gemini_oauth::complete_login(&login_id)
        .await
        .map_err(|e| AppError::Other(format!("Gemini OAuth complete error: {e}")))
}

#[tauri::command]
pub fn gemini_oauth_cancel(login_id: Option<String>) -> Result<(), AppError> {
    crate::core::model_config::gemini_oauth::cancel_login(login_id.as_deref())
        .map_err(|e| AppError::Other(format!("Gemini OAuth cancel error: {e}")))
}

#[tauri::command]
pub fn gemini_oauth_submit_callback(
    login_id: String,
    callback_url: String,
) -> Result<(), AppError> {
    crate::core::model_config::gemini_oauth::submit_callback_url(&login_id, &callback_url)
        .map_err(|e| AppError::Other(format!("Gemini OAuth callback error: {e}")))
}

#[tauri::command]
pub fn gemini_oauth_is_configured() -> bool {
    !crate::core::model_config::gemini_oauth::gemini_oauth_client_id().is_empty()
}

// ── Codex Multi-Account ────────────────────────────────────────────

#[tauri::command]
pub async fn list_codex_accounts() -> Result<Vec<codex_accounts::CodexAccount>, AppError> {
    Ok(codex_accounts::list_accounts())
}

#[tauri::command]
pub async fn get_current_codex_account_id() -> Result<Option<String>, AppError> {
    Ok(codex_accounts::get_current_account_id())
}

#[tauri::command]
pub async fn switch_codex_account(
    app: tauri::AppHandle,
    account_id: String,
) -> Result<codex_accounts::CodexAccount, AppError> {
    if let Some(account) = codex_accounts::load_account(&account_id) {
        if account.auth_mode == "oauth"
            && codex_oauth::is_token_expired(&account.tokens.access_token)
        {
            if let Some(ref refresh_token) = account.tokens.refresh_token {
                match codex_oauth::refresh_access_token(refresh_token).await {
                    Ok(new_tokens) => {
                        let mut refreshed = account.clone();
                        refreshed.tokens = codex_accounts::CodexTokens {
                            id_token: new_tokens.id_token,
                            access_token: new_tokens.access_token,
                            refresh_token: new_tokens.refresh_token,
                        };
                        if let Ok((_, _, plan_type, _)) =
                            codex_accounts::extract_user_info(&refreshed.tokens.id_token)
                        {
                            if plan_type.is_some() {
                                refreshed.plan_type = plan_type;
                            }
                        }
                        codex_accounts::save_account(&refreshed)
                            .map_err(|e| AppError::Other(format!("Save refreshed tokens: {e}")))?;
                        tracing::info!("Codex OAuth token refreshed during account switch");
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Token refresh failed during switch (will use stale): {}",
                            e
                        );
                    }
                }
            }
        }
    }

    let result = codex_accounts::switch_account(&account_id)
        .map_err(|e| AppError::Other(format!("Switch account error: {e}")))?;

    let _ = refresh_tray_menu(&app);
    Ok(result)
}

#[tauri::command]
pub async fn delete_codex_account(account_id: String) -> Result<(), AppError> {
    codex_accounts::delete_account(&account_id)
        .map_err(|e| AppError::Other(format!("Delete account error: {e}")))
}

#[tauri::command]
pub async fn add_codex_api_key_account(
    app: tauri::AppHandle,
    api_key: String,
    api_base_url: Option<String>,
) -> Result<codex_accounts::CodexAccount, AppError> {
    let account = codex_accounts::create_api_key_account(api_key, api_base_url)
        .map_err(|e| AppError::Other(format!("Add API key account error: {e}")))?;

    codex_accounts::switch_account(&account.id)
        .map_err(|e| AppError::Other(format!("Switch account error: {e}")))?;

    let _ = refresh_tray_menu(&app);
    Ok(account)
}

// ── OpenCode Auth Providers ────────────────────────────────────────

fn get_opencode_auth_path() -> std::path::PathBuf {
    opencode::auth_json_path()
}

#[tauri::command]
pub async fn get_opencode_auth_providers() -> Result<serde_json::Value, AppError> {
    let path = get_opencode_auth_path();
    let mut parsed: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Other(format!("Failed to read auth.json: {}", e)))?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if let Ok(output) = crate::core::path_env::command_with_path("opencode")
        .arg("providers")
        .arg("list")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut obj = parsed.as_object_mut().cloned().unwrap_or_default();

        let env_mapping: Vec<(&str, &str)> = vec![
            ("DASHSCOPE_API_KEY", "alibaba"),
            ("OPENAI_API_KEY", "openai"),
            ("ANTHROPIC_API_KEY", "anthropic"),
            ("MOONSHOT_API_KEY", "moonshot"),
            ("ZHIPU_API_KEY", "zhipu"),
            ("DEEPSEEK_API_KEY", "deepseek"),
            ("ARK_API_KEY", "bytedance"),
            ("MINIMAX_API_KEY", "minimax"),
        ];

        for line in stdout.lines() {
            if line.contains('●') {
                for &(env_var, provider_id) in &env_mapping {
                    if line.contains(env_var) {
                        let is_ignored = obj
                            .get(provider_id)
                            .and_then(|v| v.get("type"))
                            .map_or(false, |t| t == "env_ignored");

                        if !obj.contains_key(provider_id) || is_ignored {
                            if !is_ignored {
                                obj.insert(
                                    provider_id.to_string(),
                                    serde_json::json!({ "type": "env", "key": env_var }),
                                );
                            }
                        }
                    }
                }
            }
        }
        parsed = serde_json::Value::Object(obj);
    }

    if let Ok(oc_config) = crate::core::model_config::opencode::read_config() {
        if let Some(providers) = oc_config.get("provider").and_then(|v| v.as_object()) {
            let mut obj = parsed.as_object_mut().cloned().unwrap_or_default();
            for (key, val) in providers {
                let mut p = serde_json::json!({
                    "type": "custom",
                    "baseURL": "",
                    "key": ""
                });

                if let Some(options) = val.get("options").and_then(|v| v.as_object()) {
                    if let Some(key_str) = options.get("apiKey").and_then(|v| v.as_str()) {
                        p["key"] = serde_json::json!(key_str);
                    }
                    if let Some(base_str) = options.get("baseURL").and_then(|v| v.as_str()) {
                        p["baseURL"] = serde_json::json!(base_str);
                    }
                }

                if !obj.contains_key(key) {
                    obj.insert(key.clone(), p);
                }
            }
            parsed = serde_json::Value::Object(obj);
        }
    }

    Ok(parsed)
}

#[tauri::command]
pub async fn add_opencode_auth_provider(provider: String, key: String) -> Result<(), AppError> {
    let path = get_opencode_auth_path();
    let mut parsed = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Other(format!("Failed to read auth.json: {}", e)))?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = parsed.as_object_mut() {
        obj.insert(provider, serde_json::json!({ "type": "api", "key": key }));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    std::fs::write(&path, serde_json::to_string_pretty(&parsed).unwrap())
        .map_err(|e| AppError::Other(format!("Failed to write auth.json: {}", e)))?;
    Ok(())
}

#[tauri::command]
pub async fn remove_opencode_auth_provider(
    provider: String,
    is_env: Option<bool>,
    is_custom: Option<bool>,
) -> Result<(), AppError> {
    if is_custom.unwrap_or(false) {
        crate::core::model_config::opencode::set_field(
            &format!("provider.{}", provider),
            serde_json::Value::Null,
        )?;
        return Ok(());
    }

    let path = get_opencode_auth_path();
    let content = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|e| AppError::Other(format!("Failed to read auth.json: {}", e)))?
    } else {
        "{}".to_string()
    };

    let mut parsed: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    if let Some(obj) = parsed.as_object_mut() {
        if is_env.unwrap_or(false) {
            obj.insert(
                provider.clone(),
                serde_json::json!({ "type": "env_ignored" }),
            );
        } else {
            obj.remove(&provider);
        }
    }

    std::fs::write(&path, serde_json::to_string_pretty(&parsed).unwrap())
        .map_err(|e| AppError::Other(format!("Failed to write auth.json: {}", e)))?;
    Ok(())
}
