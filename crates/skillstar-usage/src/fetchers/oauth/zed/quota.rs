//! `GET /client/users/me` mapped onto the existing usage windows.
//!
//! A window exists only when `used` is known. An explicit zero stays zero.
//! Unlimited edit predictions and a spend cap that is not itself a usage
//! total become credit lines, not empty bars.

use chrono::DateTime;
use serde_json::Value;

use super::{CLOUD_BASE, USER_PATH};
use crate::subscription::{CreditInfo, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const TOKEN_SPEND: &str = "Token Spend";
const MONTHLY_CREDITS: &str = "Monthly credits";
const EDIT_PREDICTIONS: &str = "Edit Predictions";
const SPEND_LIMIT: &str = "Spend Limit";

pub(super) async fn fetch_quota(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let access =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    let user_id = subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or(UsageError::AuthRequired)?
        .to_string();
    let client = crate::fetchers::http_client()?;
    let body = fetch_me(&client, CLOUD_BASE, &user_id, &access).await?;
    apply_profile(subscription, &body);
    Ok(usage_from_body(&subscription.id, &body))
}

pub(super) fn authorization_header(user_id: &str, access_token: &str) -> String {
    format!("{} {}", user_id.trim(), access_token.trim())
}

pub(super) async fn fetch_me(
    client: &reqwest::Client,
    base: &str,
    user_id: &str,
    access_token: &str,
) -> UsageResult<Value> {
    let url = format!("{}{USER_PATH}", base.trim_end_matches('/'));
    let response = client
        .get(&url)
        .header(
            reqwest::header::AUTHORIZATION,
            authorization_header(user_id, access_token),
        )
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "SkillStar")
        .send()
        .await
        .map_err(|error| UsageError::transport("Zed", error))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    if status == 401 {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status("Zed", status, &body));
    }
    serde_json::from_str(&body)
        .map_err(|error| UsageError::Fetcher(format!("Zed 响应解析失败: {error}")))
}

pub(super) fn usage_from_body(subscription_id: &str, body: &Value) -> SubscriptionUsage {
    let period_end = first_timestamp(
        body,
        &[
            &["plan", "subscription_period", "ended_at"],
            &["plan", "subscription", "period", "end_at"],
            &["subscription", "period", "end_at"],
        ],
    );
    let token_used = first_i64(
        body,
        &[
            &["plan", "usage", "token_spend", "used"],
            &["plan", "usage", "current_usage", "token_spend", "used"],
            &["usage", "token_spend", "used"],
            &["current_usage", "token_spend", "used"],
            &["plan", "usage", "token_spend_cents"],
        ],
    );
    let token_limit = first_i64(
        body,
        &[
            &["plan", "usage", "token_spend", "limit"],
            &["plan", "usage", "current_usage", "token_spend", "limit"],
            &["usage", "token_spend", "limit"],
            &["current_usage", "token_spend", "limit"],
        ],
    );
    let token_remaining = first_i64(
        body,
        &[
            &["plan", "usage", "token_spend", "remaining"],
            &["current_usage", "token_spend", "remaining"],
        ],
    );
    let token_used = token_used.or_else(|| used_from_remaining(token_limit, token_remaining));

    let edit_used = first_i64(
        body,
        &[
            &["plan", "usage", "edit_predictions", "used"],
            &["usage", "edit_predictions", "used"],
            &["current_usage", "edit_predictions", "used"],
        ],
    );
    let edit_limit = first_limit(
        body,
        &[
            &["plan", "usage", "edit_predictions", "limit"],
            &["usage", "edit_predictions", "limit"],
            &["current_usage", "edit_predictions", "limit"],
        ],
    );
    let edit_remaining = first_i64(
        body,
        &[
            &["plan", "usage", "edit_predictions", "remaining"],
            &["current_usage", "edit_predictions", "remaining"],
        ],
    );
    let edit_used = edit_used.or_else(|| match edit_limit {
        Limit::Finite(total) => used_from_remaining(Some(total), edit_remaining),
        _ => None,
    });

    let spend_limit = first_i64(
        body,
        &[
            &["plan", "max_monthly_llm_usage_spending_in_cents"],
            &["plan", "spend_limit_in_cents"],
            &["max_monthly_llm_usage_spending_in_cents"],
            &["spend_limit_in_cents"],
            &["preferences", "max_monthly_llm_usage_spending_in_cents"],
            &["preferences", "spend_limit_in_cents"],
        ],
    );

    let mut monthly = None;
    let mut weekly = None;
    if let Some(used) = token_used {
        let total = token_limit.filter(|value| *value > 0);
        let label = if total.is_some() {
            MONTHLY_CREDITS
        } else {
            TOKEN_SPEND
        };
        monthly = Some(window(label, used, total, period_end));
    }
    let mut credits = Vec::new();
    if let Some(used) = edit_used {
        match edit_limit {
            Limit::Finite(total) if total > 0 => {
                let row = window(EDIT_PREDICTIONS, used, Some(total), period_end);
                if monthly.is_none() {
                    monthly = Some(row);
                } else {
                    weekly = Some(row);
                }
            }
            Limit::Unlimited => {
                credits.push(credit(EDIT_PREDICTIONS, format!("{used} / unlimited")))
            }
            _ => {
                let row = window(EDIT_PREDICTIONS, used, None, period_end);
                if monthly.is_none() {
                    monthly = Some(row);
                } else {
                    weekly = Some(row);
                }
            }
        }
    } else if matches!(edit_limit, Limit::Unlimited) {
        credits.push(credit(EDIT_PREDICTIONS, "unlimited".to_string()));
    }
    if let Some(cents) = spend_limit {
        credits.push(credit(SPEND_LIMIT, format_cents(cents)));
    }

    SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: plan_name(body),
        monthly,
        weekly,
        credits,
        ..SubscriptionUsage::default()
    }
}

pub(super) fn apply_profile(subscription: &mut Subscription, body: &Value) {
    let candidate = first_string(
        body,
        &[
            &["user", "github_login"],
            &["user", "githubLogin"],
            &["github_login"],
            &["githubLogin"],
            &["user", "name"],
            &["name"],
        ],
    );
    let Some(candidate) = candidate else {
        return;
    };
    let current = subscription.display_name.trim();
    let user_id = subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .unwrap_or("");
    let placeholder = current.is_empty()
        || current.eq_ignore_ascii_case(super::PLACEHOLDER_NAME)
        || current == user_id
        || current.starts_with("Zed · ");
    if placeholder {
        subscription.display_name = candidate;
    }
}

fn window(label: &str, used: i64, total: Option<i64>, reset_at: Option<i64>) -> UsageWindow {
    let total = total.filter(|value| *value > 0);
    let percent = total.map(|total| percent(used, total));
    UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent,
        reset_at,
        breakdown: Vec::new(),
    }
}

fn credit(credit_type: &str, amount: String) -> CreditInfo {
    CreditInfo {
        credit_type: credit_type.to_string(),
        credit_amount: Some(amount),
        minimum_credit_amount_for_usage: None,
    }
}

fn percent(used: i64, total: i64) -> i32 {
    if total <= 0 {
        return 0;
    }
    let value = ((used as f64 / total as f64) * 100.0).round() as i32;
    value.clamp(0, 100)
}

fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}${}.{:02}", abs / 100, abs % 100)
}

fn used_from_remaining(limit: Option<i64>, remaining: Option<i64>) -> Option<i64> {
    let limit = limit.filter(|value| *value >= 0)?;
    let remaining = remaining?;
    Some(limit.saturating_sub(remaining))
}

fn plan_name(body: &Value) -> Option<String> {
    let plan = plan_raw(body).map(|raw| display_plan(&raw));
    let status = first_string(
        body,
        &[
            &["plan", "subscription_status"],
            &["subscription", "status"],
            &["plan", "subscription", "status"],
        ],
    );
    let overdue = first_bool(body, &[&["plan", "has_overdue_invoices"]]).unwrap_or(false);
    match (plan, status, overdue) {
        (Some(plan), Some(status), true) if !status.to_ascii_lowercase().contains("overdue") => {
            Some(format!("{plan} · {status} · overdue"))
        }
        (Some(plan), Some(status), _) => Some(format!("{plan} · {status}")),
        (Some(plan), None, true) => Some(format!("{plan} · overdue")),
        (Some(plan), None, false) => Some(plan),
        (None, Some(status), true) if !status.to_ascii_lowercase().contains("overdue") => {
            Some(format!("{status} · overdue"))
        }
        (None, Some(status), _) => Some(status),
        (None, None, true) => Some("overdue".to_string()),
        (None, None, false) => None,
    }
}

fn plan_raw(body: &Value) -> Option<String> {
    if let Some(text) = body.get("plan").and_then(Value::as_str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    first_string(
        body,
        &[
            &["plan", "plan_v3"],
            &["plan", "plan"],
            &["plan", "name"],
            &["subscription", "name"],
        ],
    )
}

fn display_plan(raw: &str) -> String {
    match raw.trim() {
        "zed_free" | "free" | "ZedFree" => "Zed Free".to_string(),
        "zed_pro" | "pro" | "ZedPro" => "Zed Pro".to_string(),
        "zed_pro_trial" | "ZedProTrial" => "Zed Pro Trial".to_string(),
        "zed_business" | "business" | "ZedBusiness" => "Zed Business".to_string(),
        "zed_vip" | "ZedVip" => "Zed VIP".to_string(),
        "zed_student" | "ZedStudent" => "Zed Student".to_string(),
        other => title_snake(other),
    }
}

fn title_snake(raw: &str) -> String {
    raw.split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Copy)]
enum Limit {
    Finite(i64),
    Unlimited,
    Missing,
}

fn first_limit(body: &Value, paths: &[&[&str]]) -> Limit {
    for path in paths {
        if let Some(value) = dig(body, path) {
            return parse_limit(value);
        }
    }
    Limit::Missing
}

fn parse_limit(value: &Value) -> Limit {
    match value {
        Value::String(text) if text.trim().eq_ignore_ascii_case("unlimited") => Limit::Unlimited,
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value.round() as i64))
            .map(Limit::Finite)
            .unwrap_or(Limit::Missing),
        Value::Object(map) => {
            if map.contains_key("unlimited") {
                return Limit::Unlimited;
            }
            map.get("limited")
                .and_then(Value::as_i64)
                .or_else(|| {
                    map.get("limited")
                        .and_then(Value::as_f64)
                        .map(|value| value.round() as i64)
                })
                .or_else(|| {
                    map.get("limited")
                        .and_then(Value::as_str)
                        .and_then(|text| text.trim().parse().ok())
                })
                .map(Limit::Finite)
                .unwrap_or(Limit::Missing)
        }
        _ => Limit::Missing,
    }
}

fn first_string(body: &Value, paths: &[&[&str]]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| dig(body, path).and_then(stringish))
}

fn first_i64(body: &Value, paths: &[&[&str]]) -> Option<i64> {
    paths
        .iter()
        .find_map(|path| dig(body, path).and_then(numberish))
}

fn first_bool(body: &Value, paths: &[&[&str]]) -> Option<bool> {
    paths.iter().find_map(|path| match dig(body, path)? {
        Value::Bool(flag) => Some(*flag),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        Value::Number(number) => number.as_i64().map(|value| value != 0),
        _ => None,
    })
}

fn first_timestamp(body: &Value, paths: &[&[&str]]) -> Option<i64> {
    paths
        .iter()
        .find_map(|path| dig(body, path).and_then(timestamp))
}

fn dig<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn stringish(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn numberish(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value.round() as i64)),
        Value::String(text) => text.trim().parse().ok(),
        Value::Object(map) => map.get("limited").and_then(numberish),
        _ => None,
    }
}

fn timestamp(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().map(normalize_epoch),
        Value::String(text) => {
            let trimmed = text.trim();
            if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
                return Some(parsed.timestamp());
            }
            trimmed.parse::<i64>().ok().map(normalize_epoch)
        }
        _ => None,
    }
}

fn normalize_epoch(value: i64) -> i64 {
    if value.abs() > 10_000_000_000 {
        value / 1000
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn pro_body() -> Value {
        json!({
            "user": { "id": 42, "github_login": "ada", "name": "Ada Lovelace" },
            "plan": {
                "plan_v3": "zed_pro",
                "has_overdue_invoices": false,
                "subscription_period": {
                    "started_at": "2026-01-01T00:00:00.000Z",
                    "ended_at": "2026-02-01T00:00:00.000Z"
                },
                "usage": {
                    "edit_predictions": { "used": 12, "limit": { "limited": 2000 } },
                    "token_spend": { "used": 150, "limit": 500 }
                },
                "max_monthly_llm_usage_spending_in_cents": 1000
            }
        })
    }

    #[test]
    fn authorization_header_is_user_id_and_token_not_bearer() {
        assert_eq!(authorization_header(" user-1 ", " tok "), "user-1 tok");
        let header = authorization_header("user-1", "tok");
        assert!(!header.to_ascii_lowercase().starts_with("bearer"));
        assert!(!header.to_ascii_lowercase().contains("bearer "));
        assert_eq!(USER_PATH, "/client/users/me");
        assert!(!USER_PATH.contains("billing"));
        assert!(!CLOUD_BASE.contains("billing"));
    }

    #[test]
    fn users_me_maps_plan_spend_predictions_and_period_end() {
        let usage = usage_from_body("sub", &pro_body());
        assert_eq!(usage.plan_name.as_deref(), Some("Zed Pro"));
        let end = DateTime::parse_from_rfc3339("2026-02-01T00:00:00.000Z")
            .unwrap()
            .timestamp();
        let monthly = usage.monthly.expect("token spend");
        assert_eq!(monthly.label, MONTHLY_CREDITS);
        assert_eq!(monthly.used, 150);
        assert_eq!(monthly.total, Some(500));
        assert_eq!(monthly.percent, Some(30));
        assert_eq!(monthly.reset_at, Some(end));
        let weekly = usage.weekly.expect("edit predictions");
        assert_eq!(weekly.label, EDIT_PREDICTIONS);
        assert_eq!(weekly.used, 12);
        assert_eq!(weekly.total, Some(2000));
        assert_eq!(weekly.reset_at, Some(end));
        assert_eq!(usage.credits.len(), 1);
        assert_eq!(usage.credits[0].credit_type, SPEND_LIMIT);
        assert_eq!(usage.credits[0].credit_amount.as_deref(), Some("$10.00"));
        assert!(usage.hourly.is_none());
    }

    #[test]
    fn missing_usage_omits_windows_and_keeps_an_explicit_zero() {
        let sparse = json!({ "user": { "id": "u" }, "plan": { "plan_v3": "zed_free" } });
        let usage = usage_from_body("sub", &sparse);
        assert_eq!(usage.plan_name.as_deref(), Some("Zed Free"));
        assert!(usage.monthly.is_none());
        assert!(usage.weekly.is_none());
        assert!(usage.credits.is_empty());

        let limits_only = json!({
            "plan": {
                "usage": {
                    "edit_predictions": { "limit": { "limited": 2000 } },
                    "token_spend": { "limit": 500 }
                }
            }
        });
        let usage = usage_from_body("sub", &limits_only);
        assert!(
            usage.monthly.is_none(),
            "limit without used must not zero a bar"
        );
        assert!(usage.weekly.is_none());

        let zero = json!({
            "plan": { "usage": { "token_spend": { "used": 0, "limit": 500 } } }
        });
        let monthly = usage_from_body("sub", &zero).monthly.expect("zero used");
        assert_eq!(monthly.used, 0);
        assert_eq!(monthly.total, Some(500));
    }

    #[test]
    fn unlimited_predictions_and_overdue_do_not_invent_a_bar() {
        let body = json!({
            "plan": {
                "plan_v3": "zed_pro",
                "has_overdue_invoices": true,
                "usage": { "edit_predictions": { "used": 3, "limit": "unlimited" } }
            }
        });
        let usage = usage_from_body("sub", &body);
        assert_eq!(usage.plan_name.as_deref(), Some("Zed Pro · overdue"));
        assert!(usage.monthly.is_none());
        assert!(usage.weekly.is_none());
        assert_eq!(usage.credits[0].credit_type, EDIT_PREDICTIONS);
        assert_eq!(
            usage.credits[0].credit_amount.as_deref(),
            Some("3 / unlimited")
        );
    }

    #[test]
    fn profile_upgrades_a_placeholder_title_only() {
        let body = pro_body();
        let mut sub = crate::fetchers::oauth::common::SubscriptionBuilder::new(
            "zed", "user-1", "USD", "tok", None,
        )
        .oauth_account_id(Some("user-1".into()))
        .build();
        apply_profile(&mut sub, &body);
        assert_eq!(sub.display_name, "ada");

        sub.display_name = "Work laptop".into();
        apply_profile(&mut sub, &body);
        assert_eq!(sub.display_name, "Work laptop");
    }

    #[tokio::test]
    async fn fetch_sends_the_custom_authorization_header() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let address = format!("http://{}", server.server_addr());
        let responder = std::thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .unwrap();
            let authorization = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string())
                .unwrap_or_default();
            let path = request.url().to_string();
            request
                .respond(tiny_http::Response::from_string(
                    r#"{"plan":{"plan_v3":"zed_student"}}"#,
                ))
                .unwrap();
            (path, authorization)
        });

        let client = reqwest::Client::new();
        let body = fetch_me(&client, &address, "user-9", "secret-token")
            .await
            .expect("200");
        let (path, authorization) = responder.join().unwrap();
        assert_eq!(path, USER_PATH);
        assert_eq!(authorization, "user-9 secret-token");
        assert!(!authorization.to_ascii_lowercase().contains("bearer"));
        assert_eq!(
            usage_from_body("sub", &body).plan_name.as_deref(),
            Some("Zed Student")
        );
    }

    #[tokio::test]
    async fn status_401_is_auth_required_and_403_is_not() {
        for status in [401_u16, 403, 429, 503] {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let address = format!("http://{}", server.server_addr());
            let responder = std::thread::spawn(move || {
                let request = server
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap()
                    .unwrap();
                request
                    .respond(tiny_http::Response::from_string("nope").with_status_code(status))
                    .unwrap();
            });
            let error = fetch_me(&reqwest::Client::new(), &address, "u", "t")
                .await
                .expect_err("status");
            responder.join().unwrap();
            match status {
                401 => assert!(matches!(error, UsageError::AuthRequired), "{error}"),
                429 | 503 => {
                    assert!(
                        matches!(error, UsageError::Transient(_)),
                        "{status} {error}"
                    );
                    assert!(error.is_transient());
                }
                403 => {
                    assert!(matches!(error, UsageError::Fetcher(_)), "{error}");
                    assert!(!matches!(error, UsageError::AuthRequired));
                }
                _ => unreachable!(),
            }
        }
    }

    #[tokio::test]
    async fn transport_failure_is_transient() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(400))
            .build()
            .unwrap();
        let error = fetch_me(&client, "http://127.0.0.1:1", "u", "t")
            .await
            .expect_err("closed port");
        assert!(matches!(error, UsageError::Transient(_)), "{error}");
        assert!(error.is_transient());
    }
}
