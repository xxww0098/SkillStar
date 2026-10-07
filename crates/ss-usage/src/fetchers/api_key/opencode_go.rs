//! OpenCode Go API-key quota: Console meters, then the legacy usage API.
use crate::fetchers::account_json::{number, read, stamp};
use crate::subscription::{SubscriptionUsage, UsageUnit, UsageWindow};
use crate::{UsageError, UsageResult};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike, Utc};
use serde_json::Value;

pub(super) async fn fetch(id: &str, key: &str) -> UsageResult<SubscriptionUsage> {
    fetch_at(
        &super::super::http_client()?,
        "https://opencode.ai",
        id,
        key,
    )
    .await
}

async fn fetch_at(
    client: &reqwest::Client,
    base: &str,
    id: &str,
    key: &str,
) -> UsageResult<SubscriptionUsage> {
    if key.trim().is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UsageError::Other("OpenCode Go API Key 无效".into()));
    }
    let get = |path: &str| {
        client
            .get(format!("{base}{path}"))
            .bearer_auth(key)
            .header("Accept", "application/json")
    };
    let primary = match read(get("/console/api/go/status")).await {
        Ok(value) => parse(id, &value),
        Err(error) => Err(error),
    };
    match primary {
        Ok(usage) => Ok(usage),
        Err(primary_error) => match read(get("/zen/go/v1/usage")).await {
            Ok(value) => parse(id, &value).map_err(|_| primary_error),
            Err(_) => Err(primary_error),
        },
    }
}

fn window(value: &Value, label: &str, fallback_reset: Option<i64>) -> Option<UsageWindow> {
    let raw = micro_cents(value);
    let percent = number(&value["usagePercent"])
        .or_else(|| number(&value["percent"]))
        .or_else(|| {
            let (used, limit) = raw?;
            Some(used / limit * 100.0)
        })?
        .clamp(0.0, 100.0)
        .round() as i32;
    let (used, total, unit) = match raw.map(|(used, limit)| (to_cents(used), to_cents(limit))) {
        Some((used, limit)) => (used.max(0), Some(limit), UsageUnit::UsdCents),
        None => (0, None, UsageUnit::Count),
    };
    Some(UsageWindow {
        label: label.into(),
        used,
        total,
        percent: Some(percent),
        reset_at: stamp(&value["resetsAt"]).or(fallback_reset),
        breakdown: vec![],
        unit,
    })
}

/// `usedMicroCents` / `limitMicroCents`. A micro-cent is 1e-8 dollars, so
/// dividing by 1e6 yields US cents.
fn micro_cents(value: &Value) -> Option<(f64, f64)> {
    let limit = number(&value["limitMicroCents"]).filter(|n| *n > 0.0)?;
    let used = number(&value["usedMicroCents"])?.max(0.0);
    Some((used, limit))
}

fn to_cents(micro_cents: f64) -> i64 {
    (micro_cents / 1_000_000.0).round() as i64
}

/// Month length follows OpenCode `getMonthlyBounds`: the subscription anniversary,
/// not a fixed 30 days. A measured billing span wins. Otherwise the reset instant
/// is one anniversary, and the previous one is the same UTC clock clamped to the
/// shorter month.
fn month_label(meter: &Value, period_start: Option<i64>, period_end: Option<i64>) -> String {
    let reset = stamp(&meter["resetsAt"]).or(period_end);
    period_days(billing_start(meter, reset, period_start, period_end), reset)
        .or_else(|| reset.and_then(days_ending_at))
        .map(|days| format!("{days}d"))
        .unwrap_or_else(|| "Monthly".to_string())
}

fn billing_start(
    meter: &Value,
    reset: Option<i64>,
    period_start: Option<i64>,
    period_end: Option<i64>,
) -> Option<i64> {
    if reset.is_some() && reset == period_end {
        period_start
    } else {
        stamp(&meter["startsAt"])
    }
}

fn period_days(start: Option<i64>, end: Option<i64>) -> Option<i64> {
    let span = end?.checked_sub(start?)?;
    if span <= 0 {
        return None;
    }
    let days = (span + 43_200) / 86_400;
    (28..=31).contains(&days).then_some(days)
}

fn days_ending_at(end_unix: i64) -> Option<i64> {
    let end = DateTime::from_timestamp(end_unix, 0)?;
    period_days(
        Some(previous_month_anchor(end)?.timestamp()),
        Some(end.timestamp()),
    )
}

fn previous_month_anchor(end: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let (year, month) = if end.month() == 1 {
        (end.year().checked_sub(1)?, 12)
    } else {
        (end.year(), end.month() - 1)
    };
    let day = end.day().min(last_utc_day(year, month)?);
    Utc.with_ymd_and_hms(year, month, day, end.hour(), end.minute(), end.second())
        .single()
}

fn last_utc_day(year: i32, month: u32) -> Option<u32> {
    let (year, month) = if month == 12 {
        (year.checked_add(1)?, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1)?
        .pred_opt()
        .map(|date| date.day())
}

fn parse(id: &str, value: &Value) -> UsageResult<SubscriptionUsage> {
    let (hourly, weekly, monthly) = if let Some(access) = value.get("access") {
        let meters = &access["meters"];
        let period_start = stamp(&access["startsAt"]);
        let period_end = stamp(&access["endsAt"]);
        (
            window(&meters["fiveHour"], "5h", None),
            window(&meters["week"], "7d", None),
            window(
                &meters["month"],
                &month_label(&meters["month"], period_start, period_end),
                period_end,
            ),
        )
    } else {
        let usage = &value["usage"];
        (
            window(&usage["rolling"], "5h", None),
            window(&usage["weekly"], "7d", None),
            window(
                &usage["monthly"],
                &month_label(&usage["monthly"], None, None),
                None,
            ),
        )
    };
    if hourly.is_none() && weekly.is_none() && monthly.is_none() {
        return Err(UsageError::Fetcher("OpenCode Go 未返回有效订阅额度".into()));
    }
    Ok(SubscriptionUsage {
        subscription_id: id.into(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: Some(
            value["renewalProduct"]
                .as_str()
                .unwrap_or("OpenCode Go")
                .into(),
        ),
        hourly,
        weekly,
        monthly,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn console_spend_and_legacy_percent_windows_do_not_claim_token_counts() {
        let usage = parse("go", &json!({"access":{"endsAt":"2026-11-01T00:00:00Z","meters":{
            "fiveHour":{"usedMicroCents":"300000000","limitMicroCents":"1200000000","resetsAt":1800000000000_i64},
            "week":{"usedMicroCents":0,"limitMicroCents":3000000000_i64},
            "month":{"usedMicroCents":900,"limitMicroCents":600}
        }},"renewalProduct":"go-plus"})).unwrap();
        let hourly = usage.hourly.unwrap();
        assert_eq!(hourly.percent, Some(25));
        assert_eq!(hourly.unit, UsageUnit::UsdCents);
        assert_eq!(hourly.used, 300);
        assert_eq!(hourly.total, Some(1_200));
        assert_eq!(hourly.reset_at, Some(1800000000));
        let weekly = usage.weekly.unwrap();
        assert_eq!(weekly.used, 0);
        assert_eq!(weekly.total, Some(3_000));
        let monthly = usage.monthly.unwrap();
        assert_eq!(monthly.percent, Some(100));
        assert_eq!(monthly.used, 0);
        assert_eq!(monthly.total, Some(0));
        assert_eq!(monthly.label, "31d");
        assert_eq!(monthly.reset_at, stamp(&json!("2026-11-01T00:00:00Z")));
        assert_eq!(usage.plan_name.as_deref(), Some("go-plus"));
        let mixed = parse(
            "go",
            &json!({"access":{"meters":{"fiveHour":{
                "usagePercent": 10,
                "usedMicroCents": "300000000",
                "limitMicroCents": "1200000000"
            }}}}),
        )
        .unwrap();
        let mixed_hour = mixed.hourly.unwrap();
        assert_eq!(mixed_hour.percent, Some(10));
        assert_eq!(mixed_hour.used, 300);
        assert_eq!(mixed_hour.total, Some(1_200));
        let legacy = parse(
            "go",
            &json!({"usage":{"rolling":{"percent":12,"resetsAt":1800000000}}}),
        )
        .unwrap();
        let legacy_hour = legacy.hourly.unwrap();
        assert_eq!(legacy_hour.percent, Some(12));
        assert_eq!(legacy_hour.unit, UsageUnit::Count);
        assert_eq!(legacy_hour.total, None);
        assert!(legacy.weekly.is_none());
        for invalid in [
            json!(null),
            json!({"access":null}),
            json!({"access":{"meters":{}}}),
            json!({"usage":{"rolling":{"percent":"NaN"}}}),
        ] {
            assert!(parse("go", &invalid).is_err());
        }
    }

    #[test]
    fn month_label_is_the_billing_period_length() {
        let label = |value| parse("go", &value).unwrap().monthly.unwrap().label;
        assert_eq!(
            label(json!({"access":{
                "startsAt":"2026-09-10T10:33:00Z",
                "endsAt":"2026-10-10T10:33:00Z",
                "meters":{"month":{"percent":6}}
            }})),
            "30d"
        );
        assert_eq!(
            label(json!({"access":{
                "startsAt":"2026-02-28T12:00:00Z",
                "endsAt":"2026-03-31T12:00:00Z",
                "meters":{"month":{"percent":1}}
            }})),
            "31d"
        );
        assert_eq!(
            label(json!({"access":{
                "startsAt":"2026-01-31T12:00:00Z",
                "endsAt":"2026-02-28T12:00:00Z",
                "meters":{"month":{"percent":1}}
            }})),
            "28d"
        );
        assert_eq!(
            label(json!({"usage":{"monthly":{"percent":6,"resetsAt":"2026-10-10T10:33:00Z"}}})),
            "30d"
        );
        assert_eq!(
            label(json!({"usage":{"monthly":{"percent":1,"resetsAt":"2024-03-29T00:00:00Z"}}})),
            "29d"
        );
        assert_eq!(
            label(json!({"access":{
                "startsAt":"2025-10-10T00:00:00Z",
                "endsAt":"2026-10-10T00:00:00Z",
                "meters":{"month":{"percent":6,"resetsAt":"2026-10-10T10:33:00Z"}}
            }})),
            "30d"
        );
        assert_eq!(label(json!({"usage":{"monthly":{"percent":6}}})), "Monthly");
    }

    #[tokio::test]
    async fn key_requests_fall_back_without_masking_console_outages() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr());
        let worker = std::thread::spawn(move || {
            for (path, status, body) in [
                ("/console/api/go/status", 403, "secret"),
                (
                    "/zen/go/v1/usage",
                    200,
                    r#"{"usage":{"rolling":{"percent":42}}}"#,
                ),
                ("/console/api/go/status", 503, "secret"),
                ("/zen/go/v1/usage", 401, "secret"),
            ] {
                let request = server
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap();
                assert_eq!(request.url(), path);
                assert!(request.headers().iter().any(
                    |h| h.field.equiv("Authorization") && h.value.as_str() == "Bearer test-key"
                ));
                request
                    .respond(tiny_http::Response::from_string(body).with_status_code(status))
                    .unwrap();
            }
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert_eq!(
            fetch_at(&client, &base, "go", "test-key")
                .await
                .unwrap()
                .hourly
                .unwrap()
                .percent,
            Some(42)
        );
        let error = fetch_at(&client, &base, "go", "test-key")
            .await
            .unwrap_err();
        assert!(error.is_transient());
        assert!(!error.to_string().contains("secret"));
        worker.join().unwrap();
    }
}
