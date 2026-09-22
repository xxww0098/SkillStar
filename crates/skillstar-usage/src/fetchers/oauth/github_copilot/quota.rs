//! Copilot quota: GitHub token → short-lived Copilot token → user snapshots.
//!
//! `copilot_internal/v2/token` authenticates with `Authorization: token <github>`.
//! The Copilot session token is not stored. A missing quota bucket omits that
//! window; it does not fail the card.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Map, Value};

use super::login;
use crate::fetchers::oauth::common::impl_oauth_fetch;
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

pub(crate) const CATALOG_ID: &str = "github-copilot";

const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const COPILOT_USER_URL: &str = "https://api.github.com/copilot_internal/user";
const GITHUB_USER_URL: &str = "https://api.github.com/user";
const GITHUB_EMAILS_URL: &str = "https://api.github.com/user/emails";
const API_VERSION: &str = "2025-04-01";

const LABEL_PARENT: &str = "Copilot";
const LABEL_INLINE: &str = "Inline Suggestions";
const LABEL_CHAT: &str = "Chat messages";
const LABEL_PREMIUM: &str = "Premium requests";

pub(super) struct Endpoints {
    pub copilot_token: String,
    pub copilot_user: String,
    pub github_user: String,
    pub github_emails: String,
}

impl Endpoints {
    pub(super) fn production() -> Self {
        Self {
            copilot_token: COPILOT_TOKEN_URL.to_string(),
            copilot_user: COPILOT_USER_URL.to_string(),
            github_user: GITHUB_USER_URL.to_string(),
            github_emails: GITHUB_EMAILS_URL.to_string(),
        }
    }
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let github_token =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    let client = crate::fetchers::http_client()?;
    let endpoints = Endpoints::production();
    let usage =
        fetch_with_github_token(&client, &subscription.id, &github_token, &endpoints).await?;
    if subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .is_none()
    {
        match login::load_github_identity(&client, &github_token, &endpoints).await {
            Ok(identity) => login::apply_identity(subscription, &identity),
            Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
            Err(_) => {}
        }
    }
    Ok(usage)
}

impl_oauth_fetch!();

pub(super) async fn fetch_with_github_token(
    client: &reqwest::Client,
    subscription_id: &str,
    github_token: &str,
    endpoints: &Endpoints,
) -> UsageResult<SubscriptionUsage> {
    let token_body = copilot_get(
        client,
        &endpoints.copilot_token,
        &format!("token {}", github_token.trim()),
        "GitHub Copilot token",
    )
    .await?;
    let copilot_token = token_body
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(UsageError::AuthRequired)?;
    let user_body = match copilot_get(
        client,
        &endpoints.copilot_user,
        &format!("Bearer {copilot_token}"),
        "GitHub Copilot user",
    )
    .await
    {
        Ok(body) => Some(body),
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(error) if error.is_transient() => return Err(error),
        Err(UsageError::Fetcher(_)) => None,
        Err(error) => return Err(error),
    };
    Ok(usage_from_payloads(
        subscription_id,
        &token_body,
        user_body.as_ref(),
    ))
}

async fn copilot_get(
    client: &reqwest::Client,
    url: &str,
    authorization: &str,
    label: &str,
) -> UsageResult<Value> {
    let response = client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "SkillStar")
        .header("X-GitHub-Api-Version", API_VERSION)
        .send()
        .await
        .map_err(|error| UsageError::transport(label, error))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    if status == 401 {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(label, status, &body));
    }
    serde_json::from_str(&body)
        .map_err(|error| UsageError::Fetcher(format!("{label} 响应解析失败: {error}")))
}

fn usage_from_payloads(
    subscription_id: &str,
    token_body: &Value,
    user_body: Option<&Value>,
) -> SubscriptionUsage {
    let reset_at = reset_timestamp(user_body, token_body);
    let mut rows = Vec::new();
    if let Some(snapshots) = user_body
        .and_then(|body| body.get("quota_snapshots"))
        .and_then(Value::as_object)
    {
        push_snapshot(
            &mut rows,
            snapshots,
            &["completions"],
            LABEL_INLINE,
            reset_at,
        );
        push_snapshot(&mut rows, snapshots, &["chat"], LABEL_CHAT, reset_at);
        push_snapshot(
            &mut rows,
            snapshots,
            &["premium_models", "premium_interactions"],
            LABEL_PREMIUM,
            reset_at,
        );
    } else {
        push_limited(&mut rows, token_body, reset_at);
    }
    let monthly = monthly_window(rows, reset_at);
    SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: plan_name(user_body, token_body),
        monthly,
        ..SubscriptionUsage::default()
    }
}

fn monthly_window(rows: Vec<UsageWindow>, reset_at: Option<i64>) -> Option<UsageWindow> {
    if rows.is_empty() {
        return None;
    }
    let percent = rows.iter().filter_map(|row| row.percent).max().unwrap_or(0);
    Some(UsageWindow {
        label: LABEL_PARENT.to_string(),
        used: i64::from(percent),
        total: Some(100),
        percent: Some(percent),
        reset_at,
        breakdown: rows,
    })
}

fn push_snapshot(
    rows: &mut Vec<UsageWindow>,
    snapshots: &Map<String, Value>,
    keys: &[&str],
    label: &str,
    reset_at: Option<i64>,
) {
    for key in keys {
        if let Some(window) = snapshots
            .get(*key)
            .and_then(|node| window_from_snapshot(label, node, reset_at))
        {
            rows.push(window);
            return;
        }
    }
}

fn push_limited(rows: &mut Vec<UsageWindow>, token_body: &Value, reset_at: Option<i64>) {
    let Some(limited) = token_body
        .get("limited_user_quotas")
        .and_then(Value::as_object)
    else {
        return;
    };
    let copilot_token = token_body
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or("");
    let (completions_total, chat_total) = token_quota_totals(copilot_token);
    if let Some(window) = limited
        .get("completions")
        .and_then(json_f64)
        .and_then(|remaining| {
            window_from_remaining(LABEL_INLINE, remaining, completions_total, reset_at)
        })
    {
        rows.push(window);
    }
    if let Some(window) = limited
        .get("chat")
        .and_then(json_f64)
        .and_then(|remaining| window_from_remaining(LABEL_CHAT, remaining, chat_total, reset_at))
    {
        rows.push(window);
    }
}

fn window_from_snapshot(label: &str, node: &Value, reset_at: Option<i64>) -> Option<UsageWindow> {
    let obj = node.as_object()?;
    if obj.get("unlimited").and_then(Value::as_bool) == Some(true) {
        return Some(full_window(label, reset_at));
    }
    let entitlement = obj.get("entitlement").and_then(json_f64);
    if entitlement.is_some_and(|value| value < 0.0) {
        return Some(full_window(label, reset_at));
    }
    if entitlement.is_some_and(|value| value <= 0.0)
        || (entitlement.is_none() && obj.get("has_quota").and_then(Value::as_bool) == Some(false))
    {
        return None;
    }
    let percent_remaining = obj.get("percent_remaining").and_then(json_f64);
    let remaining = obj.get("remaining").and_then(json_f64);
    let used_percent = if let Some(percent) = percent_remaining {
        used_from_remaining_percent(percent)
    } else if let (Some(remaining), Some(total)) =
        (remaining, entitlement.filter(|value| *value > 0.0))
    {
        if total <= 0.0 {
            return None;
        }
        used_from_remaining_percent((remaining.max(0.0) / total) * 100.0)
    } else {
        return None;
    };
    let (used, total) = match (remaining, entitlement.filter(|value| *value > 0.0)) {
        (Some(remaining), Some(total)) => {
            let used = (total - remaining.max(0.0)).round().max(0.0) as i64;
            (used, Some(total.round() as i64))
        }
        _ => (i64::from(used_percent), Some(100)),
    };
    Some(window(label, used, total, used_percent, reset_at))
}

fn window_from_remaining(
    label: &str,
    remaining: f64,
    declared_total: Option<f64>,
    reset_at: Option<i64>,
) -> Option<UsageWindow> {
    let total = declared_total.unwrap_or(remaining).max(remaining).max(0.0);
    if total <= 0.0 {
        return None;
    }
    let remaining = remaining.clamp(0.0, total);
    let used = total - remaining;
    let percent = used_from_remaining_percent((remaining / total) * 100.0);
    Some(window(
        label,
        used.round() as i64,
        Some(total.round() as i64),
        percent,
        reset_at,
    ))
}

fn full_window(label: &str, reset_at: Option<i64>) -> UsageWindow {
    window(label, 0, Some(100), 0, reset_at)
}

fn window(
    label: &str,
    used: i64,
    total: Option<i64>,
    percent: i32,
    reset_at: Option<i64>,
) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent: Some(percent),
        reset_at,
        breakdown: Vec::new(),
    }
}

fn used_from_remaining_percent(percent_remaining: f64) -> i32 {
    (100.0 - percent_remaining).round().clamp(0.0, 100.0) as i32
}

fn token_quota_totals(token: &str) -> (Option<f64>, Option<f64>) {
    let prefix = token.split(':').next().unwrap_or(token);
    let mut completions = None;
    let mut chat = None;
    for item in prefix.split(';') {
        let Some((key, value)) = item.split_once('=') else {
            continue;
        };
        let parsed = value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite());
        match key.trim() {
            "cq" => completions = parsed,
            "tq" => chat = parsed,
            _ => {}
        }
    }
    (completions, chat)
}

fn plan_name(user_body: Option<&Value>, token_body: &Value) -> Option<String> {
    let raw = user_body
        .and_then(|body| body.get("copilot_plan"))
        .and_then(Value::as_str)
        .or_else(|| token_body.get("sku").and_then(Value::as_str))
        .or_else(|| {
            sku_from_token(
                token_body
                    .get("token")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
        });
    raw.map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(normalize_plan)
}

fn sku_from_token(token: &str) -> Option<&str> {
    let prefix = token.split(':').next().unwrap_or(token);
    prefix.split(';').find_map(|item| {
        let (key, value) = item.split_once('=')?;
        (key.trim() == "sku" && !value.trim().is_empty()).then_some(value.trim())
    })
}

fn normalize_plan(raw: &str) -> String {
    let key = raw.trim().to_ascii_lowercase().replace('-', "_");
    match key.as_str() {
        "free" | "free_limited" | "copilot_free" => "Free".to_string(),
        "individual" | "monthly_subscriber" => "Individual".to_string(),
        "pro" | "individual_pro" | "copilot_pro" => "Pro".to_string(),
        "business" | "copilot_business" => "Business".to_string(),
        "enterprise" | "copilot_enterprise" => "Enterprise".to_string(),
        _ => raw
            .split(['_', '-'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_ascii_uppercase().to_string()
                            + &chars.as_str().to_ascii_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn reset_timestamp(user_body: Option<&Value>, token_body: &Value) -> Option<i64> {
    user_body
        .and_then(|body| parse_time(body.get("quota_reset_date_utc")))
        .or_else(|| user_body.and_then(|body| parse_time(body.get("quota_reset_date"))))
        .or_else(|| {
            token_body
                .get("limited_user_reset_date")
                .and_then(json_f64)
                .and_then(|raw| normalize_epoch(raw.round() as i64))
        })
}

fn parse_time(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(number) = json_f64(value) {
        return normalize_epoch(number.round() as i64);
    }
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.timestamp());
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc().timestamp())
}

fn normalize_epoch(raw: i64) -> Option<i64> {
    if raw <= 0 {
        None
    } else if raw > 10_000_000_000 {
        Some(raw / 1000)
    } else {
        Some(raw)
    }
}

fn json_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|number| number.is_finite())
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::mpsc::{self, Receiver};

    pub(crate) struct Hit {
        pub path: String,
        pub authorization: String,
    }

    pub(crate) fn no_proxy_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    pub(crate) fn spawn(responses: Vec<(u16, &'static str)>) -> (String, Receiver<Hit>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let address = server.server_addr().to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in responses {
                let request = server.recv().unwrap();
                let authorization = request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv("Authorization"))
                    .map(|header| header.value.as_str().to_string())
                    .unwrap_or_default();
                let _ = tx.send(Hit {
                    path: request.url().to_string(),
                    authorization,
                });
                let _ = request
                    .respond(tiny_http::Response::from_string(body).with_status_code(status));
            }
        });
        (format!("http://{address}"), rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn endpoints(base: &str) -> Endpoints {
        Endpoints {
            copilot_token: format!("{base}/copilot_internal/v2/token"),
            copilot_user: format!("{base}/copilot_internal/user"),
            github_user: format!("{base}/user"),
            github_emails: format!("{base}/user/emails"),
        }
    }

    fn labels(usage: &SubscriptionUsage) -> Vec<&str> {
        usage
            .monthly
            .as_ref()
            .map(|window| {
                window
                    .breakdown
                    .iter()
                    .map(|row| row.label.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn snapshots_omit_a_missing_bucket_and_name_the_plan() {
        let usage = usage_from_payloads(
            "sub",
            &json!({"token": "copilot-session", "sku": "monthly_subscriber"}),
            Some(&json!({
                "copilot_plan": "individual",
                "quota_reset_date_utc": "2026-10-01T00:00:00Z",
                "quota_snapshots": {
                    "completions": {
                        "entitlement": 2000,
                        "remaining": 1500,
                        "percent_remaining": 75
                    },
                    "premium_interactions": {
                        "entitlement": 300,
                        "remaining": 210,
                        "percent_remaining": 70
                    }
                }
            })),
        );
        assert_eq!(usage.plan_name.as_deref(), Some("Individual"));
        assert_eq!(labels(&usage), vec![LABEL_INLINE, LABEL_PREMIUM]);
        let inline = &usage.monthly.as_ref().unwrap().breakdown[0];
        assert_eq!(inline.percent, Some(25));
        assert_eq!(inline.used, 500);
        assert_eq!(inline.total, Some(2000));
        assert_eq!(
            usage.monthly.as_ref().unwrap().reset_at,
            Some(
                DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z")
                    .unwrap()
                    .timestamp()
            )
        );
    }

    #[test]
    fn limited_quotas_fill_in_when_snapshots_are_absent() {
        let usage = usage_from_payloads(
            "sub",
            &json!({
                "token": "sku=free;cq=2000;tq=50",
                "sku": "free",
                "limited_user_quotas": {"completions": 1000, "chat": 10},
                "limited_user_reset_date": 1_893_456_000_i64
            }),
            Some(&json!({"copilot_plan": "free"})),
        );
        assert_eq!(usage.plan_name.as_deref(), Some("Free"));
        assert_eq!(labels(&usage), vec![LABEL_INLINE, LABEL_CHAT]);
        let rows = &usage.monthly.as_ref().unwrap().breakdown;
        assert_eq!(rows[0].used, 1000);
        assert_eq!(rows[0].total, Some(2000));
        assert_eq!(rows[0].percent, Some(50));
        assert_eq!(rows[1].used, 40);
        assert_eq!(rows[1].percent, Some(80));
        assert_eq!(
            usage.monthly.as_ref().unwrap().reset_at,
            Some(1_893_456_000)
        );
    }

    #[test]
    fn known_plan_names_map_onto_the_five_labels() {
        assert_eq!(normalize_plan("pro"), "Pro");
        assert_eq!(normalize_plan("individual_pro"), "Pro");
        assert_eq!(normalize_plan("business"), "Business");
        assert_eq!(normalize_plan("enterprise"), "Enterprise");
        assert_eq!(normalize_plan("free_limited"), "Free");
    }

    #[tokio::test]
    async fn quota_fetch_uses_the_token_scheme_then_the_copilot_token() {
        let (base, hits) = test_support::spawn(vec![
            (
                200,
                r#"{"token":"copilot-session","sku":"business","limited_user_quotas":{"chat":4}}"#,
            ),
            (
                200,
                r#"{"copilot_plan":"business","quota_snapshots":{"chat":{"entitlement":10,"remaining":4,"percent_remaining":40}}}"#,
            ),
        ]);
        let usage = fetch_with_github_token(
            &test_support::no_proxy_client(),
            "sub",
            "gho_example",
            &endpoints(&base),
        )
        .await
        .unwrap();
        let token_hit = hits.recv().unwrap();
        let user_hit = hits.recv().unwrap();
        assert_eq!(token_hit.path, "/copilot_internal/v2/token");
        assert_eq!(token_hit.authorization, "token gho_example");
        assert!(!token_hit.authorization.starts_with("Bearer "));
        assert_eq!(user_hit.path, "/copilot_internal/user");
        assert_eq!(user_hit.authorization, "Bearer copilot-session");
        assert_eq!(usage.plan_name.as_deref(), Some("Business"));
        assert_eq!(labels(&usage), vec![LABEL_CHAT]);
    }

    #[tokio::test]
    async fn copilot_internal_401_is_auth_required_and_429_is_transient() {
        let client = test_support::no_proxy_client();
        let (base, _hits) = test_support::spawn(vec![(401, "nope")]);
        let err = fetch_with_github_token(&client, "sub", "gho_example", &endpoints(&base))
            .await
            .unwrap_err();
        assert!(matches!(err, UsageError::AuthRequired), "{err:?}");

        let (base, _hits) = test_support::spawn(vec![(429, "slow")]);
        let err = fetch_with_github_token(&client, "sub", "gho_example", &endpoints(&base))
            .await
            .unwrap_err();
        assert!(err.is_transient(), "{err:?}");
        assert!(matches!(err, UsageError::Transient(_)), "{err:?}");
    }

    #[tokio::test]
    async fn copilot_internal_403_is_not_auth_required() {
        let (base, _hits) = test_support::spawn(vec![(403, "forbidden")]);
        let err = fetch_with_github_token(
            &test_support::no_proxy_client(),
            "sub",
            "gho_example",
            &endpoints(&base),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
    }

    #[tokio::test]
    async fn user_endpoint_403_keeps_limited_windows() {
        let (base, _hits) = test_support::spawn(vec![
            (
                200,
                r#"{"token":"sku=enterprise;cq=100;tq=10","limited_user_quotas":{"completions":80,"chat":2}}"#,
            ),
            (403, "rate limit"),
        ]);
        let usage = fetch_with_github_token(
            &test_support::no_proxy_client(),
            "sub",
            "ghp_example",
            &endpoints(&base),
        )
        .await
        .unwrap();
        assert_eq!(usage.plan_name.as_deref(), Some("Enterprise"));
        assert_eq!(labels(&usage), vec![LABEL_INLINE, LABEL_CHAT]);
    }
}
