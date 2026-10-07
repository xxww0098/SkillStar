//! Codex weekly reset credits. Redemption IDs survive an uncertain response
//! in this process so clicking retry cannot consume a second credit.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::subscription::{CreditInfo, ResetWindow, Subscription};
use crate::{UsageError, UsageResult};

const URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";
static REQUESTS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Mutex::default);

fn request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    token: &str,
    account: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut request = client
        .request(method, url)
        .bearer_auth(token)
        .header("accept", "application/json");
    if let Some(account) = account {
        request = request.header("ChatGPT-Account-Id", account);
    }
    request
}

async fn response(request: reqwest::RequestBuilder) -> UsageResult<Value> {
    let response = request
        .send()
        .await
        .map_err(|e| UsageError::transport("Codex reset cards", e))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    let body = response
        .text()
        .await
        .map_err(|e| UsageError::transport("Codex reset cards", e))?;
    if !status.is_success() {
        return Err(UsageError::http_status(
            "Codex reset cards",
            status.as_u16(),
            &body,
        ));
    }
    serde_json::from_str(&body).map_err(|e| UsageError::Fetcher(format!("Codex reset cards: {e}")))
}

pub(super) async fn fetch(token: &str, account: Option<&str>) -> UsageResult<Vec<CreditInfo>> {
    let client = crate::fetchers::http_client()?;
    let body = response(request(&client, reqwest::Method::GET, URL, token, account)).await?;
    parse(&body, Utc::now().timestamp())
}

pub(crate) async fn consume(sub: &mut Subscription) -> UsageResult<()> {
    if crate::oauth::token_refresh::needs_refresh(sub.access_token_expires_at) {
        super::refresh_codex_tokens(sub).await?;
    }
    let token = crate::fetchers::decrypt_required(&sub.access_token_encrypted, "access_token")?;
    let client = crate::fetchers::http_client()?;
    consume_with(
        &client,
        URL,
        &sub.id,
        &token,
        sub.oauth_account_id.as_deref(),
    )
    .await
}

async fn consume_with(
    client: &reqwest::Client,
    url: &str,
    id: &str,
    token: &str,
    account: Option<&str>,
) -> UsageResult<()> {
    let request_id = REQUESTS
        .lock()
        .map_err(|_| UsageError::Other("Codex reset request lock poisoned".into()))?
        .entry(id.to_string())
        .or_insert_with(|| uuid::Uuid::new_v4().to_string())
        .clone();
    let result = response(
        request(
            client,
            reqwest::Method::POST,
            &format!("{url}/consume"),
            token,
            account,
        )
        .json(&json!({"redeem_request_id": request_id, "idempotencyKey": request_id})),
    )
    .await;
    // Keep the UUID on any uncertain result, including a malformed 2xx body.
    if result.is_ok() {
        REQUESTS
            .lock()
            .map_err(|_| UsageError::Other("Codex reset request lock poisoned".into()))?
            .remove(id);
    }
    result.map(|_| ())
}

fn stamp(value: &Value) -> Option<i64> {
    let number = value.as_i64().or_else(|| value.as_str()?.parse().ok());
    if let Some(number) = number.filter(|v| *v > 0) {
        return Some(if number > 1_000_000_000_000 {
            number / 1000
        } else {
            number
        });
    }
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|v| v.timestamp())
}

pub(super) fn parse(body: &Value, now: i64) -> UsageResult<Vec<CreditInfo>> {
    let body = body.get("data").filter(|v| v.is_object()).unwrap_or(body);
    let (count_key, card_key) = ResetWindow::Weekly.credit_keys("codex").unwrap();
    let mut rows = Vec::new();
    if let Some(cards) = body
        .get("credits")
        .and_then(Value::as_array)
        .filter(|cards| !cards.is_empty())
    {
        for card in cards.iter().filter(|v| v.is_object()) {
            let status = card
                .get("status")
                .or_else(|| card.get("state"))
                .and_then(Value::as_str)
                .unwrap_or("available")
                .trim()
                .to_ascii_lowercase();
            if matches!(
                status.as_str(),
                "redeemed" | "used" | "consumed" | "expired"
            ) {
                continue;
            }
            let expiry = ["expires_at", "expire_at", "expiresAt"]
                .iter()
                .find_map(|key| card.get(*key).and_then(stamp));
            if expiry.is_some_and(|v| v <= now) {
                continue;
            }
            rows.push(CreditInfo::reset_record(card_key, expiry.unwrap_or(0)));
        }
    } else if let Some(count) = body
        .get("available_count")
        .or_else(|| body.get("availableCount"))
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
    {
        let expiry = ["expires_at", "expire_at", "next_expire_at", "nextExpiresAt"]
            .iter()
            .find_map(|key| body.get(*key).and_then(stamp));
        if count > 10_000 {
            return Err(UsageError::Fetcher(
                "Codex reset cards: invalid count".into(),
            ));
        }
        if !expiry.is_some_and(|v| v <= now) {
            rows.extend(
                (0..count.max(0)).map(|_| CreditInfo::reset_record(card_key, expiry.unwrap_or(0))),
            );
        }
    } else if !body.get("credits").is_some_and(|v| v.is_array()) {
        return Err(UsageError::Fetcher(
            "Codex reset cards: incomplete bank".into(),
        ));
    }
    rows.push(CreditInfo::reset_record(count_key, rows.len() as i64));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bank_filters_used_and_expired_but_keeps_unknown_expiry() {
        let rows = parse(
            &json!({"credits":[
                {"expires_at": 200}, {"expires_at": 100},
                {"state":"USED", "expires_at":300}, {},
                {"expiresAt":"1970-01-01T00:05:00Z"}
            ]}),
            100,
        )
        .unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.credit_amount.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["200", "0", "300", "3"]
        );
        assert_eq!(
            parse(&json!({"data":{"available_count":2}}), 100)
                .unwrap()
                .last()
                .unwrap()
                .credit_amount
                .as_deref(),
            Some("2")
        );
        assert!(parse(&json!({"error":"unavailable"}), 100).is_err());
    }
    #[tokio::test]
    async fn reset_retry_reuses_both_idempotency_fields_and_account_header() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        let worker = std::thread::spawn(move || {
            let mut bodies = Vec::new();
            for status in [503, 200] {
                let mut request = server
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap();
                assert_eq!(request.url(), "/consume");
                assert!(request.headers().iter().any(
                    |h| h.field.equiv("ChatGPT-Account-Id") && h.value.as_str() == "account-1"
                ));
                let mut text = String::new();
                request.as_reader().read_to_string(&mut text).unwrap();
                bodies.push(serde_json::from_str::<Value>(&text).unwrap());
                request
                    .respond(tiny_http::Response::from_string("{}").with_status_code(status))
                    .unwrap();
            }
            assert_eq!(bodies[0], bodies[1]);
            assert_eq!(bodies[0]["idempotencyKey"], bodies[0]["redeem_request_id"]);
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        assert!(
            consume_with(&client, &url, &id, "test-token", Some("account-1"))
                .await
                .is_err()
        );
        consume_with(&client, &url, &id, "test-token", Some("account-1"))
            .await
            .unwrap();
        worker.join().unwrap();
    }
}
