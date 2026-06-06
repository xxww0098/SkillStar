//! Google Cloud Code Assist APIs (Antigravity quota).

use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

use crate::subscription::{CreditInfo, UsageWindow};
use crate::{UsageError, UsageResult};

const CLOUD_CODE_BASE: &str = "https://cloudcode-pa.googleapis.com";
const DAILY_CLOUD_CODE_BASE: &str = "https://daily-cloudcode-pa.googleapis.com";
const DAILY_SANDBOX_CLOUD_CODE_BASE: &str = "https://daily-cloudcode-pa.sandbox.googleapis.com";
const LOAD_PATH: &str = "v1internal:loadCodeAssist";
const MODELS_PATH: &str = "v1internal:fetchAvailableModels";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const ANTIGRAVITY_CLIENT_ID: &str =
    "ANTIGRAVITY_OAUTH_CLIENT_ID";
const ANTIGRAVITY_CLIENT_SECRET: &str = "ANTIGRAVITY_OAUTH_CLIENT_SECRET";
const DEFAULT_IDE_VERSION: &str = "1.21.9";

/// Try to detect the installed Antigravity IDE version for more authentic UA.
fn detect_ide_version() -> String {
    #[cfg(target_os = "macos")]
    {
        // Parse Info.plist for CFBundleShortVersionString
        if let Ok(content) =
            std::fs::read_to_string("/Applications/Antigravity.app/Contents/Info.plist")
            && let Some(ver) = extract_plist_version(&content)
        {
            return ver;
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                r#"(Get-Item "$env:LOCALAPPDATA\Programs\antigravity\Antigravity.exe").VersionInfo.FileVersion"#,
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

#[derive(Debug, Deserialize, Default)]
pub struct GoogleTokenResponse {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

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
    let (os, arch) = match std::env::consts::OS {
        "macos" => ("darwin", "arm64"),
        "windows" => ("windows", "amd64"),
        "linux" => ("linux", "amd64"),
        _ => ("darwin", "arm64"),
    };
    let version = detect_ide_version();
    format!("antigravity/{} {}/{}", version, os, arch)
}

pub async fn refresh_antigravity_access_token(
    refresh_token: &str,
) -> UsageResult<GoogleTokenResponse> {
    refresh_google_access_token(
        refresh_token,
        ANTIGRAVITY_CLIENT_ID,
        ANTIGRAVITY_CLIENT_SECRET,
    )
    .await
}

async fn refresh_google_access_token(
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> UsageResult<GoogleTokenResponse> {
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Google refresh：{}", e)))?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "Google refresh 返回：{}",
            body.chars().take(200).collect::<String>()
        )));
    }
    resp.json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Google refresh 解析：{}", e)))
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
    serde_json::json!({
        "metadata": {
            "ideType": "ANTIGRAVITY"
        }
    })
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

async fn load_code_assist_with_body(
    access_token: &str,
    project_id: Option<&str>,
    mut payload: Value,
) -> UsageResult<LoadCodeAssistResult> {
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let ua = cloud_code_user_agent();
    attach_code_assist_project(&mut payload, project_id);

    let resp = client
        .post(format!("{}/{}", CLOUD_CODE_BASE, LOAD_PATH))
        .bearer_auth(access_token)
        .header(reqwest::header::USER_AGENT, &ua)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "*/*")
        .json(&payload)
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("loadCodeAssist：{}", e)))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "loadCodeAssist 状态 {}：{}",
            status,
            body.chars().take(300).collect::<String>()
        )));
    }

    let raw: Value = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("loadCodeAssist 解析：{}", e)))?;

    let plan_name = pick_plan_name(&raw).unwrap_or_else(|| "FREE".to_string());
    let project_id = raw
        .get("cloudaicompanionProject")
        .or_else(|| raw.get("project"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            raw.get("cloudaicompanionProjectId")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });

    let tier_id = raw
        .get("paidTier")
        .or_else(|| raw.get("currentTier"))
        .and_then(|t| t.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let credits = parse_paid_credits(&raw);

    Ok(LoadCodeAssistResult {
        raw,
        plan_name,
        project_id,
        tier_id,
        credits,
    })
}

pub async fn fetch_model_quotas(
    access_token: &str,
    project_id: Option<&str>,
) -> UsageResult<Vec<UsageWindow>> {
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let ua = cloud_code_user_agent();
    let payload = project_id
        .filter(|s| !s.is_empty())
        .map(|id| json!({ "project": id }))
        .unwrap_or_else(|| json!({}));
    let bases = [
        DAILY_CLOUD_CODE_BASE,
        DAILY_SANDBOX_CLOUD_CODE_BASE,
        CLOUD_CODE_BASE,
    ];
    let mut saw_success = false;
    let mut last_error = None;

    for base in bases {
        let resp = client
            .post(format!("{}/{}", base, MODELS_PATH))
            .bearer_auth(access_token)
            .header(reqwest::header::USER_AGENT, &ua)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| UsageError::Fetcher(format!("fetchAvailableModels：{}", e)))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(UsageError::AuthRequired);
        }
        if !resp.status().is_success() {
            last_error = Some(resp.status().to_string());
            continue;
        }

        saw_success = true;
        let body: Value = resp
            .json()
            .await
            .map_err(|e| UsageError::Fetcher(format!("fetchAvailableModels 解析：{}", e)))?;
        let windows = parse_model_windows(&body);
        if !windows.is_empty() {
            return Ok(windows);
        }
    }

    if saw_success {
        Ok(Vec::new())
    } else {
        Err(UsageError::Fetcher(format!(
            "fetchAvailableModels 状态 {}",
            last_error.unwrap_or_else(|| "unknown".to_string())
        )))
    }
}

fn parse_model_windows(value: &Value) -> Vec<UsageWindow> {
    let models = value
        .get("models")
        .and_then(|v| v.as_object())
        .or_else(|| value.as_object());

    let Some(models) = models else {
        return Vec::new();
    };

    antigravity_quota_groups()
        .into_iter()
        .filter_map(|definition| build_antigravity_quota_window(models, definition))
        .collect()
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
        let quota_info = entry.get("quotaInfo").or_else(|| entry.get("quota_info"));
        let remaining = quota_info
            .and_then(|qi| {
                qi.get("remainingFraction")
                    .or_else(|| qi.get("remaining_fraction"))
                    .or_else(|| qi.get("remaining"))
            })
            .and_then(normalize_quota_fraction);
        let has_reset = quota_info
            .and_then(|qi| qi.get("resetTime").or_else(|| qi.get("reset_time")))
            .is_some();
        let remaining = remaining.or(if has_reset { Some(0.0) } else { None })?;
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

fn find_antigravity_model<'a>(
    models: &'a serde_json::Map<String, Value>,
    identifier: &str,
) -> Option<(&'a str, &'a Value)> {
    if let Some((id, entry)) = models.get_key_value(identifier) {
        return Some((id.as_str(), entry));
    }
    models.iter().find_map(|(id, entry)| {
        let display = entry.get("displayName").and_then(|v| v.as_str())?;
        if display.eq_ignore_ascii_case(identifier) {
            Some((id.as_str(), entry))
        } else {
            None
        }
    })
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

#[allow(dead_code)]
const _TIMEOUT: Duration = Duration::from_secs(30);
