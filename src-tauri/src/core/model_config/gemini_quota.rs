use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

use super::gemini_oauth;
use super::providers::{self, ProviderEntry};
use crate::core::infra::error::AppError;

// ── Cloud Code API endpoints ────────────────────────────────────────
const CLOUD_CODE_DAILY_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
const CLOUD_CODE_PROD_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
const FETCH_AVAILABLE_MODELS_PATH: &str = "v1internal:fetchAvailableModels";
const LOAD_CODE_ASSIST_PATH: &str = "v1internal:loadCodeAssist";
const ONBOARD_USER_PATH: &str = "v1internal:onboardUser";

// ── Request defaults ────────────────────────────────────────────────
const DEFAULT_IDE_VERSION: &str = "1.20.5";
const DEFAULT_NODE_VERSION: &str = "22.21.1";
const DEFAULT_GOOGLE_API_CLIENT_VERSION: &str = "10.3.0";
const MAX_ATTEMPTS: usize = 2;
const ONBOARD_POLL_DELAY_MS: u64 = 500;

// ── Public result types ─────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelQuota {
    pub name: String,
    pub display_name: Option<String>,
    pub percentage: i32,
    pub reset_time: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GeminiQuota {
    pub percentage: i32,
    pub reset_time: String,
    pub plan_name: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelQuota>,
    #[serde(default)]
    pub available_credits: Option<String>,
    #[serde(default)]
    pub is_forbidden: bool,
    #[serde(default)]
    pub error_message: Option<String>,
}

// ── Internal API response types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoadProjectResponse {
    #[serde(rename = "paidTier")]
    pub paid_tier: Option<Tier>,
    #[serde(rename = "currentTier")]
    pub current_tier: Option<Tier>,
    #[serde(rename = "allowedTiers")]
    pub allowed_tiers: Option<Vec<AllowedTier>>,
    #[serde(rename = "cloudaicompanionProject")]
    pub project: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AllowedTier {
    id: Option<String>,
    #[serde(rename = "isDefault")]
    is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreditInfoPayload {
    #[serde(rename = "creditAmount")]
    pub credit_amount: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Tier {
    pub id: Option<String>,
    #[serde(rename = "availableCredits", default)]
    pub available_credits: Option<Vec<CreditInfoPayload>>,
}

#[derive(Debug, Deserialize)]
struct QuotaResponse {
    models: HashMap<String, ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "quotaInfo")]
    quota_info: Option<QuotaInfo>,
}

#[derive(Debug, Deserialize)]
struct QuotaInfo {
    #[serde(rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OnboardUserResponse {
    name: Option<String>,
    done: Option<bool>,
    response: Option<OnboardResponse>,
}

#[derive(Debug, Deserialize)]
struct OnboardResponse {
    #[serde(rename = "cloudaicompanionProject")]
    project: Option<serde_json::Value>,
}

// ── Header / metadata helpers ───────────────────────────────────────

fn platform_name() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "DARWIN_ARM64",
        ("macos", "x86_64") => "DARWIN_AMD64",
        ("linux", "x86_64") => "LINUX_AMD64",
        ("linux", "aarch64") => "LINUX_ARM64",
        ("windows", "x86_64") => "WINDOWS_AMD64",
        _ => "PLATFORM_UNSPECIFIED",
    }
}

fn os_label() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "windows",
        "linux" => "linux",
        _ => "unknown",
    }
}

fn arch_label() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => "amd64",
    }
}

/// User-Agent for loadCodeAssist (IDE + google-api-nodejs-client).
fn load_code_assist_user_agent() -> String {
    format!(
        "antigravity/{} {}/{} google-api-nodejs-client/{}",
        DEFAULT_IDE_VERSION,
        os_label(),
        arch_label(),
        DEFAULT_GOOGLE_API_CLIENT_VERSION,
    )
}

/// User-Agent for fetchAvailableModels (IDE only).
fn cloud_code_user_agent() -> String {
    format!(
        "antigravity/{} {}/{}",
        DEFAULT_IDE_VERSION,
        os_label(),
        arch_label(),
    )
}

/// x-goog-api-client header value.
fn x_goog_api_client() -> String {
    format!("gl-node/{}", DEFAULT_NODE_VERSION)
}

/// Build comprehensive metadata matching the IDE client format.
fn build_cloud_code_metadata(project_id: Option<&str>) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("ideName".into(), Value::String("antigravity".into()));
    metadata.insert("ideType".into(), Value::String("ANTIGRAVITY".into()));
    metadata.insert(
        "ideVersion".into(),
        Value::String(DEFAULT_IDE_VERSION.into()),
    );
    metadata.insert(
        "pluginVersion".into(),
        Value::String(env!("CARGO_PKG_VERSION").into()),
    );
    metadata.insert("platform".into(), Value::String(platform_name().into()));
    metadata.insert("updateChannel".into(), Value::String("stable".into()));
    metadata.insert("pluginType".into(), Value::String("GEMINI".into()));
    if let Some(pid) = project_id.filter(|v| !v.trim().is_empty()) {
        metadata.insert("duetProject".into(), Value::String(pid.into()));
    }
    Value::Object(metadata)
}

fn build_load_code_assist_payload(project_id: Option<&str>) -> Value {
    let mut payload = json!({
        "metadata": build_cloud_code_metadata(project_id),
        "mode": "FULL_ELIGIBILITY_CHECK"
    });
    if let Some(pid) = project_id.filter(|v| !v.trim().is_empty()) {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("cloudaicompanionProject".into(), Value::String(pid.into()));
        }
    }
    payload
}

fn extract_project_id(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    if let Some(obj) = value.as_object() {
        for key in &["projectId", "projectNumber", "id"] {
            if let Some(id) = obj
                .get(*key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn pick_onboard_tier(allowed: &[AllowedTier]) -> Option<String> {
    if let Some(default) = allowed.iter().find(|t| t.is_default.unwrap_or(false)) {
        if let Some(id) = default.id.clone() {
            return Some(id);
        }
    }
    if let Some(first) = allowed.iter().find(|t| t.id.is_some()) {
        return first.id.clone();
    }
    if !allowed.is_empty() {
        return Some("LEGACY".to_string());
    }
    None
}

fn create_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Other(format!("Failed to build HTTP client: {}", e)))
}

/// Resolve the Cloud Code base URL based on the provider's stored `is_gcp_tos` flag.
///
/// - GCP ToS enterprise accounts → prod URL (`cloudcode-pa.googleapis.com`)
/// - Personal OAuth accounts (default) → daily URL (`daily-cloudcode-pa.googleapis.com`)
///
/// The flag is stored in `provider.meta.is_gcp_tos` and auto-detected after a
/// successful `loadCodeAssist` response that contains a real paid tier + project.
fn resolve_base_url(provider: &ProviderEntry) -> &'static str {
    let is_gcp = provider
        .meta
        .as_ref()
        .and_then(|m| m.get("is_gcp_tos"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if is_gcp {
        CLOUD_CODE_PROD_BASE_URL
    } else {
        CLOUD_CODE_DAILY_BASE_URL
    }
}

/// After a successful loadCodeAssist, detect whether the account is GCP ToS
/// (has paidTier + project) and persist the flag into `provider.meta.is_gcp_tos`
/// so future requests use the correct base URL.
fn persist_gcp_tos_detection(
    app_id: &str,
    provider: &mut ProviderEntry,
    has_paid_tier: bool,
    has_project: bool,
) {
    // GCP ToS heuristic: if the account has both a paid tier and a project,
    // it's likely an enterprise account that should use the prod endpoint.
    let detected_gcp_tos = has_paid_tier && has_project;
    let current_flag = provider
        .meta
        .as_ref()
        .and_then(|m| m.get("is_gcp_tos"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if detected_gcp_tos != current_flag {
        let meta = provider.meta.get_or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("is_gcp_tos".into(), serde_json::json!(detected_gcp_tos));
        }
        let _ = providers::update_provider(app_id, provider.clone());
        tracing::info!(
            "[Gemini] GCP ToS detection updated: is_gcp_tos={} for provider={}",
            detected_gcp_tos,
            provider.id
        );
    }
}

// ── onboardUser ─────────────────────────────────────────────────────

async fn try_onboard_user(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &str,
    tier_id: &str,
    project_id: Option<&str>,
) -> Result<Option<String>, String> {
    let mut payload = json!({
        "tierId": tier_id,
        "metadata": build_cloud_code_metadata(project_id)
    });
    let ua = load_code_assist_user_agent();
    if let Some(pid) = project_id.filter(|v| !v.trim().is_empty()) {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("cloudaicompanionProject".into(), Value::String(pid.into()));
        }
    }

    let response = client
        .post(format!("{}/{}", base_url, ONBOARD_USER_PATH))
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, &ua)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("onboardUser network error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("onboardUser failed: {} - {}", status, text));
    }

    let mut data = response
        .json::<OnboardUserResponse>()
        .await
        .map_err(|e| format!("onboardUser parse error: {}", e))?;

    loop {
        if data.done.unwrap_or(false) {
            if let Some(project) = data.response.and_then(|resp| resp.project) {
                return Ok(extract_project_id(&project));
            }
            return Ok(None);
        }

        let op_name = data
            .name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "onboardUser incomplete but missing operation name".to_string())?;

        let poll_response = client
            .get(format!("{}/v1internal/{}", base_url, op_name))
            .bearer_auth(access_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, &ua)
            .send()
            .await
            .map_err(|e| format!("onboardUser poll network error: {}", e))?;

        if !poll_response.status().is_success() {
            let status = poll_response.status();
            let text = poll_response.text().await.unwrap_or_default();
            return Err(format!("onboardUser poll failed: {} - {}", status, text));
        }

        data = poll_response
            .json::<OnboardUserResponse>()
            .await
            .map_err(|e| format!("onboardUser poll parse error: {}", e))?;

        tokio::time::sleep(Duration::from_millis(ONBOARD_POLL_DELAY_MS)).await;
    }
}

// ── Main entry point ────────────────────────────────────────────────

pub async fn refresh_gemini_quota(
    app_id: &str,
    provider_id: &str,
) -> Result<GeminiQuota, AppError> {
    let (providers_map, _) = providers::get_providers(app_id)?;
    let mut provider = providers_map
        .get(provider_id)
        .cloned()
        .ok_or_else(|| AppError::Other("Provider not found".into()))?;

    let access_token = provider
        .settings_config
        .get("env")
        .and_then(|v: &serde_json::Value| v.get("GEMINI_API_KEY"))
        .and_then(|v: &serde_json::Value| v.as_str())
        .unwrap_or_default()
        .to_string();

    if access_token.is_empty() {
        return Err(AppError::Other("Missing access token".into()));
    }

    let client = create_client()?;

    // Resolve base URL based on stored is_gcp_tos flag (daily for personal, prod for enterprise).
    let base_url = resolve_base_url(&provider);
    let ua = load_code_assist_user_agent();
    let x_goog = x_goog_api_client();

    // ── Step 1: loadCodeAssist (with retry + 401 token refresh) ──────

    let mut access_token_to_use = access_token.clone();
    let mut plan_name = None;
    let mut total_credits = None;
    let mut project_id: Option<String> = None;
    let mut allowed_tiers: Vec<AllowedTier> = Vec::new();
    let mut subscription_tier: Option<String> = None;
    let mut last_load_error: Option<String> = None;

    for attempt in 1..=MAX_ATTEMPTS {
        tracing::info!(
            "[Gemini][loadCodeAssist] provider={} attempt={}/{} url={}/{} user-agent=\"{}\"",
            provider_id,
            attempt,
            MAX_ATTEMPTS,
            base_url,
            LOAD_CODE_ASSIST_PATH,
            ua
        );

        let response = client
            .post(&format!("{}/{}", base_url, LOAD_CODE_ASSIST_PATH))
            .bearer_auth(&access_token_to_use)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, &ua)
            .header("x-goog-api-client", &x_goog)
            .header(reqwest::header::ACCEPT, "*/*")
            .json(&build_load_code_assist_payload(None))
            .send()
            .await;

        match response {
            Ok(res) => {
                let status = res.status();
                if status.is_success() {
                    match res.text().await {
                        Ok(text) => {
                            tracing::info!(
                                "[Gemini][loadCodeAssist] Raw response (first 800 chars): {}",
                                &text[..text.len().min(800)]
                            );
                            match serde_json::from_str::<LoadProjectResponse>(&text) {
                                Ok(data) => {
                                    // Extract subscription tier
                                    let paid_tier_id =
                                        data.paid_tier.as_ref().and_then(|t| t.id.clone());
                                    let current_tier_id =
                                        data.current_tier.as_ref().and_then(|t| t.id.clone());
                                    subscription_tier =
                                        paid_tier_id.clone().or(current_tier_id.clone());

                                    // Derive plan_name from tier ID
                                    if let Some(ref t) = subscription_tier {
                                        plan_name = Some(classify_plan_name(t));
                                    }

                                    // Extract credits
                                    let tier_obj =
                                        data.paid_tier.as_ref().or(data.current_tier.as_ref());
                                    if let Some(credits_array) =
                                        tier_obj.and_then(|t| t.available_credits.as_ref())
                                    {
                                        let sum: i32 = credits_array
                                            .iter()
                                            .filter_map(|c| {
                                                c.credit_amount
                                                    .as_deref()
                                                    .and_then(|v| v.parse::<i32>().ok())
                                            })
                                            .sum();
                                        if sum > 0 {
                                            total_credits = Some(sum.to_string());
                                        }
                                    }

                                    tracing::info!(
                                        "[Gemini][loadCodeAssist] 订阅识别: tier={:?}, paidTier={:?}, currentTier={:?}, hasProject={}",
                                        subscription_tier,
                                        paid_tier_id,
                                        current_tier_id,
                                        data.project.is_some()
                                    );

                                    // Extract project ID
                                    let response_project_id =
                                        data.project.as_ref().and_then(extract_project_id);
                                    if let Some(pid) = response_project_id.clone() {
                                        project_id = Some(pid);
                                    }

                                    // Save allowed tiers for onboarding
                                    if let Some(tiers) = data.allowed_tiers {
                                        allowed_tiers = tiers;
                                    }

                                    // Auto-detect GCP ToS and persist for future URL resolution
                                    persist_gcp_tos_detection(
                                        app_id,
                                        &mut provider,
                                        paid_tier_id.is_some(),
                                        response_project_id.is_some(),
                                    );

                                    last_load_error = None;
                                    break; // Success, exit retry loop
                                }
                                Err(err) => {
                                    last_load_error = Some(format!(
                                        "loadCodeAssist parse error: {}, body: {}",
                                        err,
                                        &text[..text.len().min(500)]
                                    ));
                                    tracing::warn!("[Gemini] loadCodeAssist parse failed: {}", err);
                                }
                            }
                        }
                        Err(err) => {
                            last_load_error =
                                Some(format!("loadCodeAssist read body error: {}", err));
                        }
                    }
                } else if status == reqwest::StatusCode::UNAUTHORIZED {
                    // Try token refresh
                    let refresh_token = provider
                        .settings_config
                        .get("env")
                        .and_then(|v: &serde_json::Value| v.get("GEMINI_REFRESH_TOKEN"))
                        .and_then(|v: &serde_json::Value| v.as_str())
                        .unwrap_or_default()
                        .to_string();

                    if !refresh_token.is_empty() {
                        tracing::info!("[Gemini] Token expired, refreshing...");
                        if let Ok(new_tokens) =
                            gemini_oauth::refresh_access_token(&refresh_token).await
                        {
                            if let Some(new_access) = new_tokens.access_token {
                                if !new_access.is_empty() {
                                    access_token_to_use = new_access.clone();
                                    if let Some(env_obj) = provider
                                        .settings_config
                                        .get_mut("env")
                                        .and_then(|v: &mut serde_json::Value| v.as_object_mut())
                                    {
                                        env_obj.insert(
                                            "GEMINI_API_KEY".to_string(),
                                            json!(new_access),
                                        );
                                    }
                                    let _ = providers::update_provider(app_id, provider.clone());
                                    continue; // Retry with new token
                                }
                            }
                        }
                    }
                    return Err(AppError::Other(
                        "401 Unauthorized: 授权凭证已过期且无法自动刷新，请删除该账号后重新添加！"
                            .to_string(),
                    ));
                } else {
                    let text = res.text().await.unwrap_or_default();
                    let retryable =
                        status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.as_u16() >= 500;
                    last_load_error = Some(format!(
                        "loadCodeAssist failed: status={}, body={}",
                        status,
                        &text[..text.len().min(500)]
                    ));
                    if retryable && attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                }
            }
            Err(e) => {
                last_load_error = Some(format!("loadCodeAssist network error: {}", e));
                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
            }
        }
    }

    if let Some(ref err) = last_load_error {
        tracing::warn!("[Gemini] loadCodeAssist final error: {}", err);
    }

    // ── Step 1b: onboardUser if no project yet ──────────────────────

    if project_id.is_none() {
        let onboard_tier = pick_onboard_tier(&allowed_tiers).or_else(|| subscription_tier.clone());
        if let Some(tier_id) = onboard_tier {
            tracing::info!(
                "[Gemini] No project ID, attempting onboardUser with tier={}",
                tier_id
            );
            match try_onboard_user(&client, base_url, &access_token_to_use, &tier_id, None).await {
                Ok(pid) => {
                    if let Some(pid) = pid {
                        tracing::info!("[Gemini] onboardUser succeeded: project_id={}", pid);
                        project_id = Some(pid);
                    }
                }
                Err(err) => {
                    tracing::warn!("[Gemini] onboardUser failed: {}", err);
                }
            }
        }
    }

    // ── Step 2: fetchAvailableModels (with retry) ───────────────────

    let models_payload = if let Some(ref pid) = project_id {
        json!({ "project": pid })
    } else {
        json!({})
    };

    let cc_ua = cloud_code_user_agent();
    let mut best_percentage = 100;
    let mut best_reset_time = String::new();
    let mut models_list = Vec::new();
    let mut is_forbidden = false;
    let mut fetch_models_error = None;

    for attempt in 1..=3 {
        let models_response = client
            .post(&format!("{}/{}", base_url, FETCH_AVAILABLE_MODELS_PATH))
            .bearer_auth(&access_token_to_use)
            .header(reqwest::header::USER_AGENT, &cc_ua)
            .json(&models_payload)
            .send()
            .await;

        match models_response {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(payload_text) = response.text().await {
                        tracing::info!(
                            "[Gemini][fetchAvailableModels] Raw response (first 1000 chars): {}",
                            &payload_text[..payload_text.len().min(1000)]
                        );
                        if let Ok(payload) = serde_json::from_str::<QuotaResponse>(&payload_text) {
                            tracing::info!(
                                "[Gemini][fetchAvailableModels] Parsed {} models, keys: {:?}",
                                payload.models.len(),
                                payload.models.keys().collect::<Vec<_>>()
                            );
                            for (name, info) in payload.models {
                                // Skip internal/experimental models identical to reference proxy.
                                match name.as_str() {
                                    "chat_20706"
                                    | "chat_23310"
                                    | "tab_flash_lite_preview"
                                    | "tab_jump_flash_lite_preview"
                                    | "gemini-2.5-flash-thinking"
                                    | "gemini-2.5-pro" => continue,
                                    _ => {}
                                }

                                if name.contains("gemini") || name.contains("claude") {
                                    if let Some(quota) = info.quota_info {
                                        if let Some(frac) = quota.remaining_fraction {
                                            let pct = (frac * 100.0) as i32;
                                            if pct < best_percentage {
                                                best_percentage = pct;
                                                best_reset_time =
                                                    quota.reset_time.clone().unwrap_or_default();
                                            }

                                            let display_name = info
                                                .display_name
                                                .clone()
                                                .filter(|s| !s.trim().is_empty())
                                                .unwrap_or_else(|| name.clone());

                                            models_list.push(ModelQuota {
                                                name: name.clone(),
                                                display_name: Some(display_name),
                                                percentage: pct,
                                                reset_time: quota.reset_time.unwrap_or_default(),
                                            });
                                        }
                                    }
                                }
                            }
                            tracing::info!(
                                "[Gemini][fetchAvailableModels] Final models_list count: {}",
                                models_list.len()
                            );
                        } else {
                            tracing::warn!(
                                "[Gemini] Failed to parse QuotaResponse: {}",
                                &payload_text[..payload_text.len().min(500)]
                            );
                        }
                    }
                    break; // Success
                } else if response.status() == reqwest::StatusCode::FORBIDDEN {
                    tracing::warn!(
                        "[Gemini] fetchAvailableModels returned 403 (project_id={:?})",
                        project_id
                    );
                    is_forbidden = true;
                    fetch_models_error = Some("403 Forbidden: 账户未通过模型配额接口权限验证 (但不影响已获取到的基础积分)".to_string());
                    break;
                } else {
                    let status = response.status();
                    let retryable =
                        status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.as_u16() >= 500;
                    tracing::warn!(
                        "[Gemini] fetchAvailableModels failed: status={} attempt={}/3",
                        status,
                        attempt
                    );
                    if retryable && attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    break;
                }
            }
            Err(e) => {
                tracing::warn!(
                    "[Gemini] fetchAvailableModels network error: {} attempt={}/3",
                    e,
                    attempt
                );
                fetch_models_error = Some(format!("fetchAvailableModels error: {}", e));
                if attempt < 3 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                } else {
                    break;
                }
            }
        }
    }

    let quota = GeminiQuota {
        percentage: best_percentage,
        reset_time: best_reset_time,
        plan_name,
        models: models_list,
        available_credits: total_credits,
        is_forbidden,
        error_message: fetch_models_error,
    };

    let meta = provider.meta.get_or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = meta.as_object_mut() {
        if let Ok(quota_json) = serde_json::to_value(&quota) {
            obj.insert("gemini_quota".into(), quota_json);
        }
    }
    let _ = providers::update_provider(app_id, provider.clone());

    Ok(quota)
}

/// Classify a tier ID string into a friendly plan name.
fn classify_plan_name(tier_id: &str) -> String {
    let lower = tier_id.to_lowercase();
    if lower.contains("advanced") {
        "ADVANCED".to_string()
    } else if lower.contains("ultra") {
        "ULTRA".to_string()
    } else if lower.contains("pro") {
        "PRO".to_string()
    } else if lower.contains("premium") {
        "PREMIUM".to_string()
    } else if lower == "standard-tier" {
        "STANDARD".to_string()
    } else if lower.contains("free") || lower == "free-tier" {
        "FREE".to_string()
    } else {
        tier_id.to_uppercase()
    }
}
