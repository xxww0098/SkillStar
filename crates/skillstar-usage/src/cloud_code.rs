//! Google Cloud Code Assist APIs (Antigravity quota).

use serde_json::{Value, json};

use crate::subscription::{CreditInfo, UsageWindow};
use crate::{UsageError, UsageResult};

const CLOUD_CODE_BASE: &str = "https://cloudcode-pa.googleapis.com";
const DAILY_CLOUD_CODE_BASE: &str = "https://daily-cloudcode-pa.googleapis.com";
const DAILY_SANDBOX_CLOUD_CODE_BASE: &str = "https://daily-cloudcode-pa.sandbox.googleapis.com";
const LOAD_PATH: &str = "v1internal:loadCodeAssist";
const MODELS_PATH: &str = "v1internal:fetchAvailableModels";
const SUMMARY_PATH: &str = "v1internal:retrieveUserQuotaSummary";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_IDE_VERSION: &str = "1.21.9";
const X_GOOG_API_CLIENT: &str = "gl-node/22.21.1";

/// Try to detect the installed Antigravity IDE version for more authentic UA.
fn detect_ide_version() -> String {
    #[cfg(target_os = "macos")]
    {
        // Parse Info.plist for CFBundleShortVersionString
        for path in [
            "/Applications/Antigravity IDE.app/Contents/Info.plist",
            "/Applications/Antigravity.app/Contents/Info.plist",
        ] {
            if let Ok(content) = std::fs::read_to_string(path)
                && let Some(ver) = extract_plist_version(&content)
            {
                return ver;
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
            r#"$paths = @(
                "$env:LOCALAPPDATA\Programs\Antigravity IDE\Antigravity.exe",
                "$env:LOCALAPPDATA\Programs\antigravity\Antigravity.exe"
            );
            ($paths | Where-Object { Test-Path $_ } | Select-Object -First 1 | Get-Item).VersionInfo.FileVersion"#,
            ])
            .output()
        {
            if output.status.success() {
                let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !ver.is_empty() {
                    return ver;
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("antigravity")
            .arg("--version")
            .output()
        {
            if output.status.success() {
                let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !ver.is_empty() {
                    return ver;
                }
            }
        }
    }
    DEFAULT_IDE_VERSION.to_string()
}

/// Extract CFBundleShortVersionString from a macOS Info.plist XML string.
#[cfg(target_os = "macos")]
fn extract_plist_version(plist_xml: &str) -> Option<String> {
    // Simple key-value extraction from plist XML without external deps.
    // Looks for: <key>CFBundleShortVersionString</key>\n\t<string>X.Y.Z</string>
    let mut in_version_key = false;
    for line in plist_xml.lines() {
        let trimmed = line.trim();
        if trimmed == "<key>CFBundleShortVersionString</key>" {
            in_version_key = true;
        } else if in_version_key && trimmed.starts_with("<string>") {
            let ver = trimmed
                .strip_prefix("<string>")?
                .strip_suffix("</string>")?
                .trim();
            if !ver.is_empty() {
                return Some(ver.to_string());
            }
            in_version_key = false;
        }
    }
    None
}

/// Google's token endpoint speaks plain RFC 6749, so it shares the crate-wide
/// [`crate::oauth::token_endpoint::TokenResponse`] rather than a private copy
/// with its own (previously missing) error semantics.
pub type GoogleTokenResponse = crate::oauth::token_endpoint::TokenResponse;

#[derive(Debug, Clone)]
pub struct LoadCodeAssistResult {
    pub raw: Value,
    pub plan_name: String,
    pub project_id: Option<String>,
    pub tier_id: Option<String>,
    pub credits: Vec<CreditInfo>,
}

/// Build Antigravity-style User-Agent for Cloud Code endpoints.
pub fn cloud_code_user_agent() -> String {
    let (os, arch) = cloud_code_platform();
    let version = detect_ide_version();
    format!("antigravity/{} {}/{}", version, os, arch)
}

pub async fn refresh_antigravity_access_token(
    refresh_token: &str,
) -> UsageResult<GoogleTokenResponse> {
    let oauth = crate::antigravity_oauth_config::antigravity_oauth_config()?;
    refresh_google_access_token(refresh_token, &oauth.client_id, &oauth.client_secret).await
}

/// Swap a Google refresh token for a fresh access token.
///
/// Routed through [`crate::oauth::token_endpoint`] so a revoked grant — which
/// Google reports as **400 + `invalid_grant`**, not 401 — becomes
/// [`UsageError::AuthRequired`] and gives the card its re-authorize button,
/// instead of dumping Google's raw JSON body onto the UI with
/// `requires_reauth` still false.
async fn refresh_google_access_token(
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> UsageResult<GoogleTokenResponse> {
    crate::oauth::token_endpoint::post_token(
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ],
        "Google refresh",
    )
    .await
}

pub async fn load_code_assist(
    access_token: &str,
    project_id: Option<&str>,
) -> UsageResult<LoadCodeAssistResult> {
    load_code_assist_with_body(
        access_token,
        project_id,
        antigravity_code_assist_metadata_payload(),
    )
    .await
}

fn antigravity_code_assist_metadata_payload() -> Value {
    let (os, arch) = cloud_code_platform();
    serde_json::json!({
        "metadata": {
            "ideName": "antigravity",
            "ideType": "ANTIGRAVITY",
            "ideVersion": detect_ide_version(),
            "platform": format!("{}_{}", os.to_ascii_uppercase(), arch.to_ascii_uppercase()),
            "pluginVersion": env!("CARGO_PKG_VERSION"),
            "updateChannel": "stable",
            "pluginType": "GEMINI"
        },
        "mode": "FULL_ELIGIBILITY_CHECK"
    })
}

fn cloud_code_platform() -> (&'static str, &'static str) {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "windows",
        "linux" => "linux",
        _ => "unknown",
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => "unknown",
    };
    (os, arch)
}

fn attach_code_assist_project(payload: &mut Value, project_id: Option<&str>) {
    let Some(pid) = project_id.filter(|s| !s.is_empty()) else {
        return;
    };
    payload["cloudaicompanionProject"] = json!(pid);
    if let Some(metadata) = payload.get_mut("metadata").and_then(|v| v.as_object_mut()) {
        metadata.insert("duetProject".to_string(), json!(pid));
    }
}

fn extract_project_id(value: &Value) -> Option<String> {
    if let Some(text) = value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(text.to_string());
    }

    let object = value.as_object()?;
    ["id", "projectId", "project_id", "name"]
        .into_iter()
        .find_map(|key| object.get(key).and_then(extract_project_id))
}

async fn load_code_assist_with_body(
    access_token: &str,
    project_id: Option<&str>,
    mut payload: Value,
) -> UsageResult<LoadCodeAssistResult> {
    let client = crate::http_client::usage_http_client()?;
    let ua = cloud_code_user_agent();
    attach_code_assist_project(&mut payload, project_id);
    let mut last_error = None;

    for base in [
        DAILY_CLOUD_CODE_BASE,
        DAILY_SANDBOX_CLOUD_CODE_BASE,
        CLOUD_CODE_BASE,
    ] {
        let resp = match client
            .post(format!("{base}/{LOAD_PATH}"))
            .bearer_auth(access_token)
            .header(reqwest::header::USER_AGENT, &ua)
            .header("x-goog-api-client", X_GOOG_API_CLIENT)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "*/*")
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(error) => {
                last_error = Some(UsageError::transport("loadCodeAssist", error));
                continue;
            }
        };

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(UsageError::AuthRequired);
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            last_error = Some(UsageError::http_status("loadCodeAssist", status, &body));
            continue;
        }

        let raw: Value = match resp.json().await {
            Ok(raw) => raw,
            Err(error) => {
                last_error = Some(UsageError::Fetcher(format!(
                    "loadCodeAssist 解析：{}",
                    error
                )));
                continue;
            }
        };

        let plan_name = pick_plan_name(&raw).unwrap_or_else(|| "FREE".to_string());
        let project_id = raw
            .get("cloudaicompanionProject")
            .or_else(|| raw.get("project"))
            .and_then(extract_project_id)
            .or_else(|| {
                raw.get("cloudaicompanionProjectId")
                    .and_then(extract_project_id)
            });

        let tier_id = raw
            .get("paidTier")
            .or_else(|| raw.get("currentTier"))
            .and_then(|t| t.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let credits = parse_paid_credits(&raw);

        return Ok(LoadCodeAssistResult {
            raw,
            plan_name,
            project_id,
            tier_id,
            credits,
        });
    }

    Err(last_error.unwrap_or_else(|| {
        UsageError::Fetcher("loadCodeAssist 没有可用的 Cloud Code endpoint".to_string())
    }))
}

pub async fn fetch_model_quotas(
    access_token: &str,
    project_id: Option<&str>,
) -> UsageResult<Vec<UsageWindow>> {
    let client = crate::http_client::usage_http_client()?;
    let ua = cloud_code_user_agent();
    let payload = project_id
        .filter(|s| !s.is_empty())
        .map(|id| json!({ "project": id }))
        .unwrap_or_else(|| json!({}));
    fetch_model_quotas_from_bases(
        &client,
        access_token,
        &ua,
        &payload,
        &[
            DAILY_CLOUD_CODE_BASE,
            DAILY_SANDBOX_CLOUD_CODE_BASE,
            CLOUD_CODE_BASE,
        ],
    )
    .await
}

#[derive(Debug)]
enum QuotaSummaryResult {
    Success(Vec<UsageWindow>),
    Unsupported,
    Failed(UsageError),
}

async fn fetch_model_quotas_from_bases(
    client: &reqwest::Client,
    access_token: &str,
    user_agent: &str,
    payload: &Value,
    bases: &[&str],
) -> UsageResult<Vec<UsageWindow>> {
    let mut saw_summary_empty = false;
    let mut saw_model_success = false;
    let mut last_status: Option<u16> = None;
    let mut last_error: Option<UsageError> = None;
    let mut model_windows_fallback = None;

    for base in bases {
        match fetch_quota_summary(client, access_token, user_agent, base, payload).await {
            QuotaSummaryResult::Success(summary_windows) => {
                if !summary_windows.is_empty() {
                    return Ok(summary_windows);
                }
                // A valid 2xx summary with no buckets is a supported empty
                // response. Do not immediately issue a second endpoint call.
                saw_summary_empty = true;
                continue;
            }
            QuotaSummaryResult::Unsupported => {}
            QuotaSummaryResult::Failed(error) => {
                if matches!(error, UsageError::AuthRequired) {
                    return Err(error);
                }
                last_error = Some(error);
                continue;
            }
        }

        // The model catalog is a compatibility fallback only when the summary
        // endpoint explicitly does not exist on this base (404/405).
        let resp = match client
            .post(format!("{base}/{MODELS_PATH}"))
            .bearer_auth(access_token)
            .header(reqwest::header::USER_AGENT, user_agent)
            .header("x-goog-api-client", X_GOOG_API_CLIENT)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(payload)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(error) => {
                last_error = Some(UsageError::transport("fetchAvailableModels", error));
                continue;
            }
        };

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(UsageError::AuthRequired);
        }
        if !resp.status().is_success() {
            last_status = Some(resp.status().as_u16());
            continue;
        }

        saw_model_success = true;
        let body: Value = match resp.json().await {
            Ok(body) => body,
            Err(error) => {
                last_error = Some(UsageError::Fetcher(format!(
                    "fetchAvailableModels 解析：{}",
                    error
                )));
                continue;
            }
        };

        if model_windows_fallback.is_none() {
            let windows = parse_model_windows(&body);
            if !windows.is_empty() {
                model_windows_fallback = Some(windows);
            }
        }
    }

    if let Some(windows) = model_windows_fallback {
        return Ok(windows);
    }
    if saw_model_success || saw_summary_empty {
        return Ok(Vec::new());
    }

    Err(last_error.unwrap_or_else(|| match last_status {
        Some(status) => UsageError::http_status("fetchAvailableModels", status, ""),
        None => UsageError::Fetcher("没有可用的 Cloud Code quota endpoint".to_string()),
    }))
}

async fn fetch_quota_summary(
    client: &reqwest::Client,
    access_token: &str,
    user_agent: &str,
    base: &str,
    payload: &Value,
) -> QuotaSummaryResult {
    let response = match client
        .post(format!("{base}/{SUMMARY_PATH}"))
        .bearer_auth(access_token)
        .header(reqwest::header::USER_AGENT, user_agent)
        .header("x-goog-api-client", X_GOOG_API_CLIENT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(payload)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return QuotaSummaryResult::Failed(UsageError::transport(
                "retrieveUserQuotaSummary",
                error,
            ));
        }
    };

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return QuotaSummaryResult::Failed(UsageError::AuthRequired);
    }
    if matches!(
        status,
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
    ) {
        return QuotaSummaryResult::Unsupported;
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return QuotaSummaryResult::Failed(UsageError::http_status(
            "retrieveUserQuotaSummary",
            status.as_u16(),
            &body,
        ));
    }

    let body = match response.json::<Value>().await {
        Ok(body) => body,
        Err(error) => {
            return QuotaSummaryResult::Failed(UsageError::Fetcher(format!(
                "retrieveUserQuotaSummary 解析：{}",
                error
            )));
        }
    };
    match parse_quota_summary_windows(&body) {
        Some(windows) => QuotaSummaryResult::Success(windows),
        None => QuotaSummaryResult::Failed(UsageError::Fetcher(
            "retrieveUserQuotaSummary 响应缺少有效 groups".to_string(),
        )),
    }
}

fn parse_quota_summary_windows(value: &Value) -> Option<Vec<UsageWindow>> {
    let groups = value
        .get("groups")
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("groups"))
        })?
        .as_array()?;
    let mut windows = Vec::new();

    for group in groups {
        let group_label = group
            .get("displayName")
            .or_else(|| group.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or("Antigravity");
        let Some(buckets) = group.get("buckets").and_then(Value::as_array) else {
            continue;
        };

        for bucket in buckets {
            let remaining = bucket
                .get("remainingFraction")
                .or_else(|| bucket.get("remaining_fraction"))
                .or_else(|| {
                    bucket
                        .get("remaining")
                        .and_then(|remaining| remaining.get("remainingFraction"))
                })
                .or_else(|| {
                    bucket
                        .get("remaining")
                        .and_then(|remaining| remaining.get("remaining_fraction"))
                })
                .and_then(normalize_quota_fraction);
            let Some(remaining) = remaining else {
                continue;
            };
            let window = bucket
                .get("window")
                .or_else(|| bucket.get("windowName"))
                .or_else(|| bucket.get("window_name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|label| !label.is_empty());
            let bucket_name = bucket
                .get("displayName")
                .or_else(|| bucket.get("display_name"))
                .and_then(Value::as_str)
                .and_then(normalize_quota_bucket_name);
            let bucket_label = bucket_name
                .map(|name| format!("{group_label} · {name}"))
                .or_else(|| window.map(|window| format!("{group_label} · {window}")))
                .unwrap_or_else(|| group_label.to_string());
            let remaining_pct = (remaining * 100.0).round().clamp(0.0, 100.0) as i32;
            let used_pct = (100 - remaining_pct).clamp(0, 100);
            windows.push(UsageWindow {
                label: bucket_label,
                used: used_pct as i64,
                total: Some(100),
                percent: Some(used_pct),
                reset_at: bucket
                    .get("resetTime")
                    .or_else(|| bucket.get("reset_time"))
                    .and_then(parse_reset_at),
                breakdown: Vec::new(),
            });
        }
    }

    Some(windows)
}

fn normalize_quota_bucket_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let name = [" remaining", " left", " available"]
        .into_iter()
        .find_map(|suffix| {
            lower
                .ends_with(suffix)
                .then(|| trimmed[..trimmed.len() - suffix.len()].trim())
        })
        .unwrap_or(trimmed);
    (!name.is_empty()).then(|| name.to_string())
}

fn parse_model_windows(value: &Value) -> Vec<UsageWindow> {
    let models = value
        .get("models")
        .and_then(|v| v.as_object())
        .or_else(|| value.as_object());

    let Some(models) = models else {
        return Vec::new();
    };

    let mut matched_model_ids = Vec::new();
    let mut windows = Vec::new();

    for definition in antigravity_quota_groups() {
        if let Some(window) = build_antigravity_quota_window(models, definition) {
            for identifier in definition.identifiers {
                if let Some((id, _)) = find_antigravity_model(models, identifier) {
                    matched_model_ids.push(id.to_string());
                }
            }
            windows.push(window);
        }
    }

    // The model catalog changes independently from SkillStar. Keep showing
    // valid Gemini/Claude/GPT quota entries even when Google adds or renames a
    // model before the fixed grouping table is updated.
    windows.extend(
        models
            .iter()
            .filter(|(id, _)| !matched_model_ids.iter().any(|matched| matched == *id))
            .filter_map(|(id, entry)| build_dynamic_model_quota_window(id, entry)),
    );
    windows
}

#[derive(Clone, Copy)]
struct AntigravityQuotaGroup {
    label: &'static str,
    identifiers: &'static [&'static str],
    label_from_model: bool,
}

fn antigravity_quota_groups() -> Vec<AntigravityQuotaGroup> {
    vec![
        AntigravityQuotaGroup {
            label: "Claude/GPT",
            identifiers: &[
                "claude-sonnet-4-6",
                "claude-opus-4-6-thinking",
                "gpt-oss-120b-medium",
            ],
            label_from_model: false,
        },
        AntigravityQuotaGroup {
            label: "Gemini 3.1 Pro Series",
            identifiers: &["gemini-3.1-pro-high", "gemini-3.1-pro-low"],
            label_from_model: false,
        },
        AntigravityQuotaGroup {
            label: "Gemini 3 Pro",
            identifiers: &["gemini-3-pro-high", "gemini-3-pro-low"],
            label_from_model: false,
        },
        AntigravityQuotaGroup {
            label: "Gemini 2.5 Flash",
            identifiers: &["gemini-2.5-flash", "gemini-2.5-flash-thinking"],
            label_from_model: false,
        },
        AntigravityQuotaGroup {
            label: "Gemini 2.5 Flash Lite",
            identifiers: &["gemini-2.5-flash-lite"],
            label_from_model: false,
        },
        AntigravityQuotaGroup {
            label: "Gemini 2.5 CU",
            identifiers: &["rev19-uic3-1p"],
            label_from_model: false,
        },
        AntigravityQuotaGroup {
            label: "Gemini 3 Flash",
            identifiers: &["gemini-3-flash"],
            label_from_model: false,
        },
        AntigravityQuotaGroup {
            label: "gemini-3.1-flash-image",
            identifiers: &["gemini-3.1-flash-image"],
            label_from_model: true,
        },
    ]
}

fn build_antigravity_quota_window(
    models: &serde_json::Map<String, Value>,
    group: AntigravityQuotaGroup,
) -> Option<UsageWindow> {
    let mut fractions = Vec::new();
    let mut display_name = None;

    for identifier in group.identifiers {
        let Some((_, entry)) = find_antigravity_model(models, identifier) else {
            continue;
        };
        let remaining = model_remaining_fraction(entry)?;
        fractions.push(remaining);
        if display_name.is_none() {
            display_name = entry
                .get("displayName")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
    }

    let remaining = fractions.into_iter().reduce(f64::min)?;
    let remaining_pct = (remaining * 100.0).round().clamp(0.0, 100.0) as i32;
    let used_pct = (100 - remaining_pct).clamp(0, 100);
    let label = if group.label_from_model {
        display_name.unwrap_or_else(|| group.label.to_string())
    } else {
        group.label.to_string()
    };

    Some(UsageWindow {
        label,
        used: used_pct as i64,
        total: Some(100),
        percent: Some(used_pct),
        reset_at: None,
        breakdown: Vec::new(),
    })
}

fn build_dynamic_model_quota_window(id: &str, entry: &Value) -> Option<UsageWindow> {
    if !is_displayable_model(id, entry) {
        return None;
    }
    let remaining = model_remaining_fraction(entry)?;
    let remaining_pct = (remaining * 100.0).round().clamp(0.0, 100.0) as i32;
    let used_pct = (100 - remaining_pct).clamp(0, 100);
    let label = entry
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(id)
        .to_string();

    Some(UsageWindow {
        label,
        used: used_pct as i64,
        total: Some(100),
        percent: Some(used_pct),
        reset_at: model_reset_at(entry),
        breakdown: Vec::new(),
    })
}

fn model_remaining_fraction(entry: &Value) -> Option<f64> {
    let quota_info = entry.get("quotaInfo").or_else(|| entry.get("quota_info"));
    let remaining = quota_info.and_then(|qi| {
        qi.get("remainingFraction")
            .or_else(|| qi.get("remaining_fraction"))
            .or_else(|| qi.get("remaining"))
    });
    // A reset timestamp without a remaining fraction is only reset context;
    // it does not mean the quota is exhausted. Never turn an unknown window
    // into a fake 100% used bar.
    remaining.and_then(normalize_quota_fraction)
}

fn model_reset_at(entry: &Value) -> Option<i64> {
    let reset = entry
        .get("quotaInfo")
        .or_else(|| entry.get("quota_info"))
        .and_then(|qi| qi.get("resetTime").or_else(|| qi.get("reset_time")))
        .and_then(Value::as_str)?;
    parse_reset_at(&Value::String(reset.to_string()))
}

fn parse_reset_at(value: &Value) -> Option<i64> {
    let reset = value.as_str()?.trim();
    chrono::DateTime::parse_from_rfc3339(reset)
        .ok()
        .map(|value| value.timestamp())
}

fn is_displayable_model(id: &str, entry: &Value) -> bool {
    let candidates = [
        Some(id),
        entry.get("model").and_then(Value::as_str),
        entry.get("modelId").and_then(Value::as_str),
        entry.get("model_id").and_then(Value::as_str),
        entry.get("name").and_then(Value::as_str),
        entry.get("displayName").and_then(Value::as_str),
    ];
    candidates.into_iter().flatten().any(|candidate| {
        let normalized = candidate.to_ascii_lowercase();
        ["gemini", "claude", "gpt", "image", "imagen"]
            .iter()
            .any(|prefix| normalized.contains(prefix))
    })
}

fn find_antigravity_model<'a>(
    models: &'a serde_json::Map<String, Value>,
    identifier: &str,
) -> Option<(&'a str, &'a Value)> {
    if let Some((id, entry)) = models.get_key_value(identifier) {
        return Some((id.as_str(), entry));
    }
    let normalized_identifier = normalize_model_id(identifier);
    models.iter().find_map(|(id, entry)| {
        let candidates = [
            Some(id.as_str()),
            entry.get("model").and_then(Value::as_str),
            entry.get("modelId").and_then(Value::as_str),
            entry.get("model_id").and_then(Value::as_str),
            entry.get("name").and_then(Value::as_str),
            entry.get("displayName").and_then(Value::as_str),
        ];
        if candidates
            .into_iter()
            .flatten()
            .any(|candidate| normalize_model_id(candidate) == normalized_identifier)
        {
            Some((id.as_str(), entry))
        } else {
            None
        }
    })
}

fn normalize_model_id(value: &str) -> String {
    let value = value.rsplit('/').next().unwrap_or(value);
    let mut normalized = String::new();
    let mut pending_separator = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if pending_separator && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(ch);
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    normalized
}

fn normalize_quota_fraction(value: &Value) -> Option<f64> {
    if let Some(n) = value.as_f64().filter(|n| n.is_finite()) {
        return Some(n);
    }
    let raw = value.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(percent) = raw.strip_suffix('%') {
        let parsed = percent.trim().parse::<f64>().ok()?;
        return parsed.is_finite().then_some(parsed / 100.0);
    }
    let parsed = raw.parse::<f64>().ok()?;
    parsed.is_finite().then_some(parsed)
}

/// Extract credit info from `paidTier.availableCredits`.
fn parse_paid_credits(value: &Value) -> Vec<CreditInfo> {
    let credits = match value
        .get("paidTier")
        .and_then(|t| t.get("availableCredits"))
        .and_then(|v| v.as_array())
    {
        Some(arr) => arr,
        None => return Vec::new(),
    };
    credits
        .iter()
        .filter_map(|entry| {
            let credit_type = entry
                .get("creditType")
                .or_else(|| entry.get("credit_type"))?
                .as_str()?;
            if credit_type.is_empty() {
                return None;
            }
            let credit_amount = entry
                .get("creditAmount")
                .or_else(|| entry.get("credit_amount"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            // Only include entries that have a credit amount
            credit_amount.as_ref()?;
            Some(CreditInfo {
                credit_type: credit_type.to_string(),
                credit_amount,
                minimum_credit_amount_for_usage: entry
                    .get("minimumCreditAmountForUsage")
                    .or_else(|| entry.get("minimum_credit_amount_for_usage"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            })
        })
        .collect()
}

fn pick_plan_name(v: &Value) -> Option<String> {
    // Prefer the human-readable subscription_tier field (e.g. "PRO", "ULTRA", "FREE")
    if let Some(tier) = v
        .get("subscriptionTier")
        .or_else(|| v.get("subscription_tier"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(tier.to_uppercase());
    }
    for path in [
        &["paidTier", "id"],
        &["currentTier", "id"],
        &["paid_tier", "id"],
        &["current_tier", "id"],
    ] {
        let mut cur = v;
        let mut ok = true;
        for key in *path {
            match cur.get(key) {
                Some(next) => cur = next,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && let Some(s) = cur.as_str().filter(|s| !s.is_empty()) {
            return Some(s.to_uppercase());
        }
    }
    if let Some(arr) = v.get("allowedTiers").and_then(|v| v.as_array()) {
        for entry in arr {
            if entry
                .get("isDefault")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && let Some(id) = entry.get("id").and_then(|v| v.as_str())
            {
                return Some(id.to_uppercase());
            }
        }
        if let Some(first) = arr.first()
            && let Some(id) = first.get("id").and_then(|v| v.as_str())
        {
            return Some(id.to_uppercase());
        }
    }
    None
}

#[cfg(test)]
#[path = "cloud_code_tests.rs"]
mod tests;
