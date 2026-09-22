//! HTTP for CodeBuddy auth, refresh, and quota.
//!
//! Status table matches [`crate::oauth::token_endpoint::post_token`]:
//! 401 / `error=invalid_grant|invalid_token` → [`UsageError::AuthRequired`],
//! 429 / 5xx / transport → [`UsageError::Transient`], anything else including
//! 403 → [`UsageError::Fetcher`]. HTTP 200 with a non-success business `code`
//! is Fetcher, not Transient and not AuthRequired. Poll is the exception:
//! a 200 without an access token keeps waiting, except UA reject `10085`.

use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderValue, USER_AGENT};
use serde_json::Value;

use super::{
    Enterprise, REFRESH_SOURCE, UA_REJECT_CODE, USER_AGENT as BROWSER_UA, endpoint, json_i64,
    nonempty, object_string, pick_string,
};
use crate::{UsageError, UsageResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IssuedToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub domain: Option<String>,
    pub uid: Option<String>,
    pub enterprise_id: Option<String>,
    pub enterprise_name: Option<String>,
}

impl IssuedToken {
    pub(super) fn enterprise(&self) -> Enterprise {
        Enterprise {
            enterprise_id: self.enterprise_id.clone(),
            enterprise_name: self.enterprise_name.clone(),
            domain: self.domain.clone(),
        }
    }
}

#[derive(Debug)]
pub(super) enum PollBody {
    Pending,
    Ready(IssuedToken),
}

#[derive(Debug, Clone)]
pub(super) struct QuotaContext {
    pub user_id: Option<String>,
    pub enterprise_id: Option<String>,
    pub domain: Option<String>,
}

pub(super) async fn execute(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    headers: &[(&str, String)],
    body: Option<&Value>,
    label: &str,
) -> UsageResult<(u16, String)> {
    let mut request = client
        .request(method, url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, HeaderValue::from_static(BROWSER_UA));
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("user-agent") {
            continue;
        }
        if let Ok(value) = HeaderValue::from_str(value) {
            request = request.header(*name, value);
        }
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| UsageError::transport(label, err))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    Ok((status, text))
}

pub(super) async fn refresh_access(
    client: &reqwest::Client,
    origin: &str,
    access_token: &str,
    refresh_token: &str,
    domain: Option<&str>,
    label: &str,
) -> UsageResult<IssuedToken> {
    let url = endpoint(origin, super::AUTH_REFRESH_PATH);
    let (status, body) = execute(
        client,
        reqwest::Method::POST,
        &url,
        &refresh_headers(access_token, refresh_token, domain),
        None,
        label,
    )
    .await?;
    let value = classify_exchange(status, &body, label)?;
    issued_token(&value).ok_or(UsageError::AuthRequired)
}

pub(super) fn anonymous_headers() -> Vec<(&'static str, String)> {
    [
        "X-No-Authorization",
        "X-No-User-Id",
        "X-No-Enterprise-Id",
        "X-No-Department-Info",
    ]
    .into_iter()
    .map(|name| (name, "true".to_string()))
    .collect()
}

pub(super) fn refresh_headers(
    access_token: &str,
    refresh_token: &str,
    domain: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Authorization", format!("Bearer {}", access_token.trim())),
        ("X-Refresh-Token", refresh_token.trim().to_string()),
        ("X-Auth-Refresh-Source", REFRESH_SOURCE.to_string()),
    ];
    if let Some(domain) = nonempty(domain) {
        headers.push(("X-Domain", domain));
    }
    headers
}

pub(super) fn account_headers(
    access_token: &str,
    domain: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Authorization", format!("Bearer {}", access_token.trim())),
        ("X-No-User-Id", "true".to_string()),
        ("X-No-Enterprise-Id", "true".to_string()),
        ("X-No-Department-Info", "true".to_string()),
    ];
    if let Some(domain) = nonempty(domain) {
        headers.push(("X-Domain", domain));
    }
    headers
}

/// Bearer plus identity headers. Blank fields are omitted, not sent empty.
pub(super) fn quota_headers(
    access_token: &str,
    context: &QuotaContext,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        (CONTENT_TYPE.as_str(), "application/json".to_string()),
        ("Authorization", format!("Bearer {}", access_token.trim())),
    ];
    push(&mut headers, "X-User-Id", context.user_id.as_deref());
    if let Some(enterprise_id) = nonempty(context.enterprise_id.as_deref()) {
        headers.push(("X-Enterprise-Id", enterprise_id.clone()));
        headers.push(("X-Tenant-Id", enterprise_id));
    }
    push(&mut headers, "X-Domain", context.domain.as_deref());
    headers
}

pub(super) fn classify_exchange(status: u16, body: &str, label: &str) -> UsageResult<Value> {
    if is_oauth_revoked(status, body) {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(label, status, body));
    }
    let value: Value = serde_json::from_str(body)
        .map_err(|err| UsageError::Fetcher(format!("{label} 响应解析失败: {err}")))?;
    if let Some(err) = business_failure(&value, label) {
        return Err(err);
    }
    Ok(value)
}

pub(super) fn interpret_poll(status: u16, body: &str) -> UsageResult<PollBody> {
    if status == 404 || ((200..300).contains(&status) && body.trim().is_empty()) {
        return Ok(PollBody::Pending);
    }
    if is_oauth_revoked(status, body) {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(
            "CodeBuddy auth/token",
            status,
            body,
        ));
    }
    let value: Value = serde_json::from_str(body)
        .map_err(|err| UsageError::Fetcher(format!("CodeBuddy auth/token 响应解析失败: {err}")))?;
    if let Some(err) = business_failure(&value, "CodeBuddy auth/token") {
        if is_ua_reject(&value) {
            return Err(err);
        }
        if issued_token(&value).is_some() {
            return Err(err);
        }
        return Ok(PollBody::Pending);
    }
    if let Some(token) = issued_token(&value) {
        return Ok(PollBody::Ready(token));
    }
    Ok(PollBody::Pending)
}

pub(super) fn issued_token(body: &Value) -> Option<IssuedToken> {
    let data = body
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(body);
    let access_token = pick_string(data, &["accessToken", "access_token"])
        .or_else(|| pick_string(body, &["accessToken", "access_token"]))
        .filter(|token| !token.eq_ignore_ascii_case("null"))?;
    let (split_uid, access_token) = split_uid_token(&access_token);
    if access_token.is_empty() {
        return None;
    }
    Some(IssuedToken {
        access_token,
        refresh_token: pick_string(data, &["refreshToken", "refresh_token"])
            .or_else(|| pick_string(body, &["refreshToken", "refresh_token"])),
        expires_at: token_expires_at(data).or_else(|| token_expires_at(body)),
        domain: pick_string(data, &["domain"]).or_else(|| pick_string(body, &["domain"])),
        uid: pick_string(data, &["uid", "userId", "user_id"])
            .or_else(|| pick_string(body, &["uid", "userId", "user_id"]))
            .or(split_uid),
        enterprise_id: pick_string(data, &["enterpriseId", "enterprise_id"]),
        enterprise_name: pick_string(data, &["enterpriseName", "enterprise_name"]),
    })
}

pub(super) fn token_expires_at(data: &Value) -> Option<i64> {
    if let Some(absolute) = first_i64(data, &["expiresAt", "expires_at"]) {
        return normalize_epoch_seconds(absolute);
    }
    let seconds = first_i64(data, &["expiresIn", "expires_in"])?;
    if seconds <= 0 {
        return None;
    }
    Some(chrono::Utc::now().timestamp().saturating_add(seconds))
}

pub(super) fn normalize_epoch_seconds(value: i64) -> Option<i64> {
    if value <= 0 {
        return None;
    }
    if value >= 100_000_000_000 {
        return Some(value / 1000);
    }
    if value >= 1_000_000_000 {
        return Some(value);
    }
    None
}

pub(super) fn split_uid_token(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim();
    if let Some((prefix, suffix)) = trimmed.split_once('+') {
        let prefix = prefix.trim();
        let suffix = suffix.trim();
        if !prefix.is_empty()
            && !suffix.is_empty()
            && prefix.len() <= 128
            && !prefix.contains('.')
            && !prefix.contains(' ')
        {
            return (Some(prefix.to_string()), suffix.to_string());
        }
    }
    (None, trimmed.to_string())
}

fn first_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(raw) = value.get(*key)
            && let Some(number) = json_i64(Some(raw))
        {
            return Some(number);
        }
    }
    None
}

fn is_oauth_revoked(status: u16, body: &str) -> bool {
    if status == 401 {
        return true;
    }
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    for key in ["error", "error_description"] {
        if let Some(text) = object_string(&value, &[key])
            && is_invalid_grant_text(&text)
        {
            return true;
        }
        if let Some(nested) = value.get(key)
            && let Some(text) = nested.get("status").and_then(Value::as_str)
            && is_invalid_grant_text(text)
        {
            return true;
        }
    }
    false
}

fn is_invalid_grant_text(text: &str) -> bool {
    matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "invalid_grant" | "invalid_token"
    )
}

fn business_failure(value: &Value, label: &str) -> Option<UsageError> {
    let code = value.get("code")?;
    if code.is_null() || is_success_code(code) {
        return None;
    }
    let detail = object_string(value, &["message", "msg", "error_description"])
        .unwrap_or_else(|| "business error".into());
    let rendered = code_text(code).unwrap_or_else(|| "unknown".into());
    Some(UsageError::Fetcher(format!(
        "{label} 业务错误 (code={rendered}): {detail}"
    )))
}

fn is_success_code(code: &Value) -> bool {
    match code {
        Value::Number(number) => matches!(number.as_i64(), Some(0 | 200)),
        Value::String(text) => {
            matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "0" | "200" | "ok" | "success"
            )
        }
        _ => false,
    }
}

fn is_ua_reject(value: &Value) -> bool {
    match value.get("code") {
        Some(Value::Number(number)) => number.as_i64() == Some(UA_REJECT_CODE),
        Some(Value::String(text)) => text.trim() == "10085",
        _ => false,
    }
}

fn code_text(code: &Value) -> Option<String> {
    match code {
        Value::String(text) => nonempty(Some(text)),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn push(headers: &mut Vec<(&'static str, String)>, name: &'static str, value: Option<&str>) {
    if let Some(value) = nonempty(value) {
        headers.push((name, value));
    }
}

#[cfg(test)]
pub(super) mod scripted {
    use std::collections::VecDeque;
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Seen {
        pub method: String,
        pub path: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }

    pub struct ScriptedHttp {
        pub base: String,
        addr: std::net::SocketAddr,
        seen: Arc<Mutex<Vec<Seen>>>,
        stop: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl ScriptedHttp {
        pub fn start(script: Vec<(u16, String)>) -> Self {
            let script = Arc::new(Mutex::new(VecDeque::from(script)));
            let seen = Arc::new(Mutex::new(Vec::new()));
            let server = tiny_http::Server::http("127.0.0.1:0").expect("bind scripted http");
            let addr = server
                .server_addr()
                .to_ip()
                .expect("scripted http is a TCP listener");
            let base = format!("http://{addr}");
            let stop = Arc::new(AtomicBool::new(false));
            let stop_flag = Arc::clone(&stop);
            let seen_flag = Arc::clone(&seen);
            let handle = thread::spawn(move || {
                while !stop_flag.load(Ordering::SeqCst) {
                    match server.recv_timeout(Duration::from_millis(200)) {
                        Ok(Some(mut request)) => {
                            let headers = request
                                .headers()
                                .iter()
                                .map(|header| {
                                    (
                                        header.field.as_str().as_str().to_string(),
                                        header.value.as_str().to_string(),
                                    )
                                })
                                .collect();
                            let mut body = String::new();
                            {
                                let _ = request.as_reader().read_to_string(&mut body);
                            }
                            seen_flag.lock().expect("seen").push(Seen {
                                method: request.method().as_str().to_string(),
                                path: request.url().to_string(),
                                headers,
                                body,
                            });
                            let next = script.lock().expect("script").pop_front();
                            let (status, response_body) =
                                next.unwrap_or((500, "script exhausted".into()));
                            let _ = request.respond(
                                tiny_http::Response::from_string(response_body)
                                    .with_status_code(status),
                            );
                        }
                        Ok(None) => {}
                        Err(_) if stop_flag.load(Ordering::SeqCst) => break,
                        Err(_) => break,
                    }
                }
            });
            Self {
                base,
                addr,
                seen,
                stop,
                handle: Some(handle),
            }
        }

        pub fn seen(&self) -> Vec<Seen> {
            self.seen.lock().expect("seen").clone()
        }
    }

    impl Drop for ScriptedHttp {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Ok(mut stream) = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1)) {
                let _ = stream.write_all(b"GET /cancel HTTP/1.1\r\nConnection: close\r\n\r\n");
            }
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    pub fn header<'a>(seen: &'a Seen, name: &str) -> Option<&'a str> {
        seen.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_status_table_matches_post_token_and_keeps_business_code_as_fetcher() {
        let auth = classify_exchange(401, "nope", "CodeBuddy refresh").unwrap_err();
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let revoked = classify_exchange(
            400,
            r#"{"error":"invalid_grant","error_description":"revoked"}"#,
            "CodeBuddy refresh",
        )
        .unwrap_err();
        assert!(matches!(revoked, UsageError::AuthRequired), "{revoked:?}");

        let token = classify_exchange(400, r#"{"error":"invalid_token"}"#, "CodeBuddy refresh")
            .unwrap_err();
        assert!(matches!(token, UsageError::AuthRequired), "{token:?}");

        let forbidden =
            classify_exchange(403, r#"{"code":10085,"message":"ua"}"#, "CodeBuddy refresh")
                .unwrap_err();
        assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");
        assert!(!forbidden.is_transient());

        for status in [429_u16, 500, 502, 503] {
            let error = classify_exchange(status, "down", "CodeBuddy refresh").unwrap_err();
            assert!(
                matches!(error, UsageError::Transient(_)),
                "{status} {error:?}"
            );
            assert!(error.is_transient());
        }

        let other = classify_exchange(
            400,
            r#"{"error":"unsupported_grant_type"}"#,
            "CodeBuddy refresh",
        )
        .unwrap_err();
        assert!(matches!(other, UsageError::Fetcher(_)), "{other:?}");
        assert!(!other.is_transient());

        let business = classify_exchange(
            200,
            r#"{"code":10085,"message":"missing user agent"}"#,
            "CodeBuddy refresh",
        )
        .unwrap_err();
        assert!(matches!(business, UsageError::Fetcher(_)), "{business:?}");
        assert!(!business.is_transient());
        assert!(business.to_string().contains("10085"), "{business}");
        assert!(
            business.to_string().contains("missing user agent"),
            "{business}"
        );

        let coded_grant = classify_exchange(
            200,
            r#"{"code":"invalid_grant","message":"no"}"#,
            "CodeBuddy refresh",
        )
        .unwrap_err();
        assert!(
            matches!(coded_grant, UsageError::Fetcher(_)),
            "{coded_grant:?}"
        );
        assert!(!coded_grant.is_transient());

        let ok = classify_exchange(
            200,
            r#"{"code":0,"data":{"accessToken":"at"}}"#,
            "CodeBuddy refresh",
        )
        .unwrap();
        assert_eq!(ok["data"]["accessToken"], "at");
        let string_ok =
            classify_exchange(200, r#"{"code":"200","data":{}}"#, "CodeBuddy refresh").unwrap();
        assert!(string_ok.get("data").is_some());

        let no_access = issued_token(&serde_json::json!({"error":"invalid_grant"}));
        assert!(no_access.is_none());
    }

    #[test]
    fn poll_waits_on_business_code_but_stops_on_ua_reject_and_auth() {
        assert!(matches!(
            interpret_poll(200, r#"{"code":8,"message":"waiting"}"#).unwrap(),
            PollBody::Pending
        ));
        assert!(matches!(
            interpret_poll(404, "missing").unwrap(),
            PollBody::Pending
        ));
        assert!(matches!(
            interpret_poll(200, "").unwrap(),
            PollBody::Pending
        ));

        let ua = interpret_poll(200, r#"{"code":10085,"message":"ua"}"#).unwrap_err();
        assert!(matches!(ua, UsageError::Fetcher(_)), "{ua:?}");

        let denied = interpret_poll(401, "nope").unwrap_err();
        assert!(matches!(denied, UsageError::AuthRequired), "{denied:?}");

        let forbidden = interpret_poll(403, r#"{"message":"no"}"#).unwrap_err();
        assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");
        assert!(!forbidden.is_transient());

        let limited = interpret_poll(429, "slow").unwrap_err();
        assert!(matches!(limited, UsageError::Transient(_)), "{limited:?}");

        match interpret_poll(
            200,
            r#"{"code":0,"data":{"accessToken":"uid-1+access-token-value-0123456789","refreshToken":"refresh-token-value-0123456789","expiresIn":60,"domain":"team.example"}}"#,
        )
        .unwrap()
        {
            PollBody::Ready(token) => {
                assert_eq!(token.access_token, "access-token-value-0123456789");
                assert_eq!(token.uid.as_deref(), Some("uid-1"));
                assert_eq!(token.refresh_token.as_deref(), Some("refresh-token-value-0123456789"));
                assert_eq!(token.domain.as_deref(), Some("team.example"));
                let expires = token.expires_at.expect("expires");
                let now = chrono::Utc::now().timestamp();
                assert!((expires - now - 60).abs() <= 5, "{expires} vs {now}");
            }
            PollBody::Pending => panic!("token should be ready"),
        }
    }

    #[test]
    fn epoch_units_and_quota_headers_omit_blanks() {
        assert_eq!(normalize_epoch_seconds(1_793_368_047), Some(1_793_368_047));
        assert_eq!(
            normalize_epoch_seconds(1_793_368_047_633),
            Some(1_793_368_047)
        );
        assert_eq!(normalize_epoch_seconds(3_600), None);
        assert_eq!(
            token_expires_at(&serde_json::json!({"expiresAt": 1_793_368_047_633_i64})),
            Some(1_793_368_047)
        );

        let full = quota_headers(
            "access",
            &QuotaContext {
                user_id: Some("uid".into()),
                enterprise_id: Some("ent".into()),
                domain: Some("d.example".into()),
            },
        );
        assert!(
            full.iter()
                .any(|(name, value)| *name == "X-User-Id" && value == "uid")
        );
        assert!(
            full.iter()
                .any(|(name, value)| *name == "X-Enterprise-Id" && value == "ent")
        );
        assert!(
            full.iter()
                .any(|(name, value)| *name == "X-Tenant-Id" && value == "ent")
        );
        assert!(
            full.iter()
                .any(|(name, value)| *name == "X-Domain" && value == "d.example")
        );

        let bare = quota_headers(
            "access",
            &QuotaContext {
                user_id: None,
                enterprise_id: Some("  ".into()),
                domain: None,
            },
        );
        assert!(bare.iter().all(|(name, _)| !matches!(
            *name,
            "X-User-Id" | "X-Enterprise-Id" | "X-Tenant-Id" | "X-Domain"
        )));

        let refresh = refresh_headers("access", "refresh", None);
        assert!(
            refresh
                .iter()
                .any(|(name, value)| *name == "X-Refresh-Token" && value == "refresh")
        );
        assert!(
            refresh
                .iter()
                .any(|(name, value)| *name == "X-Auth-Refresh-Source" && value == "ide-main")
        );
        assert!(refresh.iter().all(|(name, _)| *name != "X-Domain"));
    }

    #[tokio::test]
    async fn refresh_request_uses_headers_not_a_form_grant() {
        let server = scripted::ScriptedHttp::start(vec![(
            200,
            r#"{"code":0,"data":{"accessToken":"new-access","refreshToken":"new-refresh","expiresAt":1793368047,"domain":"rotated.example"}}"#.into(),
        )]);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("client");
        let issued = refresh_access(
            &client,
            &server.base,
            "old-access",
            "old-refresh",
            Some("team.example"),
            "CodeBuddy refresh",
        )
        .await
        .expect("refresh");
        assert_eq!(issued.access_token, "new-access");
        assert_eq!(issued.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(issued.expires_at, Some(1_793_368_047));
        assert_eq!(issued.domain.as_deref(), Some("rotated.example"));

        let seen = server.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].path, super::super::AUTH_REFRESH_PATH);
        assert!(seen[0].body.is_empty(), "{}", seen[0].body);
        assert_eq!(
            scripted::header(&seen[0], "authorization"),
            Some("Bearer old-access")
        );
        assert_eq!(
            scripted::header(&seen[0], "x-refresh-token"),
            Some("old-refresh")
        );
        assert_eq!(
            scripted::header(&seen[0], "x-auth-refresh-source"),
            Some("ide-main")
        );
        assert_eq!(scripted::header(&seen[0], "x-domain"), Some("team.example"));
        assert_eq!(scripted::header(&seen[0], "user-agent"), Some(BROWSER_UA));
        assert!(scripted::header(&seen[0], "content-type").is_none());
    }
}
