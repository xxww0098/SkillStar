//! GLM Coding Plan cards; IDs remain in memory, only counts/expiries are saved.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{Value, json};

use super::{Endpoints, http, normalize_provider};
use crate::subscription::{CreditInfo, ResetWindow, Subscription};
use crate::{UsageError, UsageResult};

#[derive(Clone)]
struct Card {
    id: String,
    window: ResetWindow,
    expires: Option<i64>,
}

#[derive(Clone)]
struct Pending {
    card: Card,
    request_id: String,
}

// Calls are serialized by the account facade's existing catalog lock.
static REQUESTS: LazyLock<Mutex<HashMap<(String, ResetWindow), Pending>>> =
    LazyLock::new(Mutex::default);

fn url(endpoints: &Endpoints, region: &str, action: &str) -> String {
    let base = if region == "bigmodel" {
        endpoints
            .bigmodel_customer
            .trim_end_matches("/api/biz/customer/getCustomerInfo")
    } else {
        endpoints.zai_business.trim_end_matches("/api/auth/z/login")
    };
    let suffix = if action == "list" {
        "list?targetType=PERSONAL"
    } else {
        "use"
    };
    format!("{base}/api/biz/customer-package-reset/{suffix}")
}

fn headers(sub: &Subscription) -> UsageResult<Vec<(&'static str, String)>> {
    let token = super::decrypt_optional(&sub.api_key_encrypted)
        .or_else(|| super::decrypt_optional(&sub.access_token_encrypted))
        .ok_or(UsageError::AuthRequired)?;
    Ok(vec![
        ("authorization", format!("Bearer {token}")),
        ("accept", "application/json".into()),
    ])
}

fn require_success(body: &Value) -> UsageResult<()> {
    let code = body
        .get("code")
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()));
    if body.get("success").and_then(Value::as_bool) == Some(true) && matches!(code, Some(0 | 200)) {
        Ok(())
    } else {
        Err(UsageError::Fetcher(format!(
            "GLM reset cards: {}",
            body.get("msg")
                .or_else(|| body.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("provider rejected request")
        )))
    }
}

fn stamp(value: &Value, region: &str) -> Option<i64> {
    if let Some(number) = value
        .as_i64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|v| *v > 0)
    {
        return Some(if number > 1_000_000_000_000 {
            number / 1000
        } else {
            number
        });
    }
    let text = value.as_str()?.trim();
    if let Ok(stamp) = DateTime::parse_from_rfc3339(text) {
        return Some(stamp.timestamp());
    }
    let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f"))
        .ok()?;
    Some(naive.and_utc().timestamp() - if region == "bigmodel" { 8 * 3600 } else { 0 })
}

fn parse(body: &Value, region: &str, now: i64) -> UsageResult<Vec<Card>> {
    require_success(body)?;
    let mut cards = Vec::new();
    for (key, window) in [
        ("fiveHourResets", ResetWindow::FiveHour),
        ("weekResets", ResetWindow::Weekly),
    ] {
        let items = body
            .get("data")
            .and_then(|v| v.get(key))
            .and_then(Value::as_array)
            .ok_or_else(|| UsageError::Fetcher("GLM reset cards: incomplete bank".into()))?;
        for item in items {
            if item.get("available").and_then(Value::as_bool) == Some(false)
                || ["consumed", "redeemed"]
                    .iter()
                    .any(|k| item.get(k).and_then(Value::as_bool) == Some(true))
            {
                continue;
            }
            let Some(id) = item
                .get("recordId")
                .or_else(|| item.get("id"))
                .and_then(|v| {
                    v.as_str()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                        .or_else(|| v.as_i64().map(|n| n.to_string()))
                })
            else {
                continue;
            };
            let expires = ["expireTime", "expiredTime", "expiresAt"]
                .iter()
                .find_map(|key| item.get(*key).and_then(|v| stamp(v, region)));
            if expires.is_some_and(|v| v <= now) {
                continue;
            }
            cards.push(Card {
                id,
                window,
                expires,
            });
        }
    }
    cards.sort_by_key(|card| card.expires.unwrap_or(i64::MAX));
    Ok(cards)
}

async fn list(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    sub: &Subscription,
) -> UsageResult<Vec<Card>> {
    let region = normalize_provider(sub.oauth_region.as_deref())?;
    let body = http::request_json(
        client,
        reqwest::Method::GET,
        &url(endpoints, region, "list"),
        &headers(sub)?,
        None,
        "GLM reset cards",
    )
    .await?;
    parse(&body, region, Utc::now().timestamp())
}

pub(super) async fn fetch(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    sub: &Subscription,
) -> UsageResult<Vec<CreditInfo>> {
    let cards = list(client, endpoints, sub).await?;
    let mut rows = Vec::new();
    for window in ResetWindow::for_catalog("zcode") {
        let (count_key, card_key) = window.credit_keys("zcode").unwrap();
        let group: Vec<_> = cards.iter().filter(|c| c.window == *window).collect();
        rows.push(CreditInfo::reset_record(count_key, group.len() as i64));
        rows.extend(
            group
                .into_iter()
                .map(|c| CreditInfo::reset_record(card_key, c.expires.unwrap_or(0))),
        );
    }
    Ok(rows)
}

pub(crate) async fn consume(sub: &Subscription, window: ResetWindow) -> UsageResult<()> {
    let client = crate::fetchers::http_client()?;
    consume_with(&client, &Endpoints::production(), sub, window).await
}

async fn consume_with(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    sub: &Subscription,
    window: ResetWindow,
) -> UsageResult<()> {
    let key = (sub.id.clone(), window);
    let previous = REQUESTS
        .lock()
        .map_err(|_| UsageError::Other("GLM reset request lock poisoned".into()))?
        .get(&key)
        .cloned();
    let pending = if let Some(previous) = previous {
        previous
    } else {
        let card = list(client, endpoints, sub)
            .await?
            .into_iter()
            .find(|c| c.window == window)
            .ok_or_else(|| UsageError::Other("此窗口暂无可用的 GLM 重置卡，请刷新额度".into()))?;
        let pending = Pending {
            card,
            request_id: uuid::Uuid::new_v4().to_string(),
        };
        REQUESTS
            .lock()
            .map_err(|_| UsageError::Other("GLM reset request lock poisoned".into()))?
            .insert(key.clone(), pending.clone());
        pending
    };
    let region = normalize_provider(sub.oauth_region.as_deref())?;
    let body = redeem_body(&pending);
    let response = http::request_json(
        client,
        reqwest::Method::POST,
        &url(endpoints, region, "use"),
        &headers(sub)?,
        Some(&body),
        "GLM reset card",
    )
    .await?;
    // An explicit business response settles the attempt; network/parse errors
    // above retain the exact card + UUID, even if it disappeared from the list.
    let result = require_success(&response);
    REQUESTS
        .lock()
        .map_err(|_| UsageError::Other("GLM reset request lock poisoned".into()))?
        .remove(&key);
    result
}

fn redeem_body(pending: &Pending) -> Value {
    let record_id = pending
        .card
        .id
        .parse::<i64>()
        .ok()
        .map_or_else(|| json!(pending.card.id), |v| json!(v));
    json!({"targetType":"PERSONAL", "resetType":pending.card.window.key(), "recordId":record_id, "requestId":pending.request_id})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_bank_separates_windows_and_converts_china_time() {
        let body = json!({"success":true,"code":200,"data":{
            "fiveHourResets":[{"recordId":4,"expireTime":"2026-10-28 11:04:00"},{"recordId":5,"consumed":true}],
            "weekResets":[{"recordId":"7","expireTime":1},{"recordId":"9"}]
        }});
        let cards = parse(&body, "bigmodel", 100).unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].window, ResetWindow::FiveHour);
        assert_eq!(
            cards[0].expires,
            DateTime::parse_from_rfc3339("2026-10-28T03:04:00Z")
                .ok()
                .map(|v| v.timestamp())
        );
        assert_eq!(cards[1].expires, None);
        let pending = Pending {
            card: cards[0].clone(),
            request_id: "same-request".into(),
        };
        assert_eq!(
            redeem_body(&pending),
            json!({"targetType":"PERSONAL","resetType":"FIVE_HOUR","recordId":4,"requestId":"same-request"})
        );
        assert!(
            parse(
                &json!({"success":true,"code":0,"data":{"weekResets":[]}}),
                "zai",
                0
            )
            .is_err()
        );
        assert!(require_success(&json!({"success":false,"code":200})).is_err());
    }
    #[tokio::test]
    async fn reset_retry_reuses_the_selected_card_without_listing_or_spending_another() {
        let sandbox = tempfile::tempdir().unwrap();
        let _env = crate::test_support::EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", sandbox.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", sandbox.path()),
        ]);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let endpoints = Endpoints::from_base(&format!("http://{}", server.server_addr()));
        let worker = std::thread::spawn(move || {
            let list = server
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(
                list.url(),
                "/api/biz/customer-package-reset/list?targetType=PERSONAL"
            );
            list.respond(tiny_http::Response::from_string(r#"{"success":true,"code":0,"data":{"fiveHourResets":[{"recordId":4}],"weekResets":[{"recordId":9}]}}"#)).unwrap();
            let mut bodies = Vec::new();
            for status in [503, 200] {
                let mut request = server
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap();
                assert_eq!(request.url(), "/api/biz/customer-package-reset/use");
                let mut text = String::new();
                request.as_reader().read_to_string(&mut text).unwrap();
                bodies.push(serde_json::from_str::<Value>(&text).unwrap());
                request
                    .respond(
                        tiny_http::Response::from_string(r#"{"success":true,"code":200}"#)
                            .with_status_code(status),
                    )
                    .unwrap();
            }
            assert_eq!(bodies[0], bodies[1]);
            assert_eq!(bodies[0]["recordId"], 4);
            assert_eq!(bodies[0]["resetType"], "FIVE_HOUR");
        });
        let sub = crate::fetchers::oauth::common::SubscriptionBuilder::new(
            "zcode",
            "test",
            "USD",
            "test-token",
            None,
        )
        .build();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert!(
            consume_with(&client, &endpoints, &sub, ResetWindow::FiveHour)
                .await
                .is_err()
        );
        consume_with(&client, &endpoints, &sub, ResetWindow::FiveHour)
            .await
            .unwrap();
        worker.join().unwrap();
    }
}
