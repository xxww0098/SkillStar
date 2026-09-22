//! HTTP classification for ZCode.
//!
//! Same status table as [`crate::oauth::token_endpoint`]: 401 or
//! `invalid_grant` / `invalid_token` is auth, 429 / 5xx / transport is
//! transient, every other non-2xx including 403 is [`crate::UsageError::Fetcher`].
//! An HTTP 200 body whose business `code` is not a success is Fetcher, not auth
//! and not transient. JSON `code: 401` is not an OAuth 401.

use serde_json::Value;

use crate::{UsageError, UsageResult};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TokenEnvelope {
    pub jwt: String,
    pub provider_access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub user_info: Value,
}

pub(super) async fn request_json(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    headers: &[(&str, String)],
    body: Option<&Value>,
    label: &str,
) -> UsageResult<Value> {
    let mut request = client.request(method, url);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| UsageError::transport(label, error))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    read_json(status, &text, label)
}

/// Status and JSON parse only. Callers apply the endpoint's business `code`.
pub(super) fn read_json(status: u16, body: &str, label: &str) -> UsageResult<Value> {
    if status == 401 || is_revoked(body) {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(label, status, body));
    }
    serde_json::from_str(body)
        .map_err(|error| UsageError::Fetcher(format!("{label} 响应解析失败: {error}")))
}

pub(super) fn require_token_envelope(provider: &str, body: &Value) -> UsageResult<TokenEnvelope> {
    let code = body.get("code").and_then(Value::as_i64);
    let ok = if provider == "zai" {
        code == Some(0)
    } else {
        code.is_none() || code == Some(0)
    };
    if !ok {
        return Err(business_error(
            "ZCode OAuth Token",
            body,
            "ZCode OAuth Token 交换失败",
        ));
    }
    let jwt = pick_string(body, &[&["data", "token"]])
        .ok_or_else(|| UsageError::Fetcher("ZCode OAuth 响应缺少 data.token".into()))?;
    if provider == "zai" {
        let access = pick_string(
            body,
            &[
                &["data", "zai", "access_token"],
                &["data", "zai", "accessToken"],
            ],
        )
        .ok_or(UsageError::AuthRequired)?;
        return Ok(TokenEnvelope {
            jwt,
            provider_access_token: access,
            refresh_token: None,
            expires_in: body.pointer("/data/expires_in").and_then(Value::as_i64),
            user_info: body
                .pointer("/data/user")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default())),
        });
    }
    let access = pick_string(
        body,
        &[
            &["data", "bigmodel", "access_token"],
            &["data", "bigmodel", "accessToken"],
            &["data", "access_token"],
            &["data", "accessToken"],
        ],
    )
    .ok_or(UsageError::AuthRequired)?;
    Ok(TokenEnvelope {
        jwt,
        provider_access_token: access,
        refresh_token: pick_string(
            body,
            &[
                &["data", "bigmodel", "refresh_token"],
                &["data", "bigmodel", "refreshToken"],
            ],
        ),
        expires_in: None,
        user_info: Value::Object(Default::default()),
    })
}

pub(super) fn require_zai_business_token(body: &Value) -> UsageResult<String> {
    let code_ok = match body.get("code") {
        None | Some(Value::Null) => true,
        Some(Value::Number(number)) => number.as_i64().is_some_and(|code| code == 0 || code == 200),
        Some(Value::String(text)) => matches!(text.trim(), "0" | "200"),
        _ => false,
    };
    if !code_ok || body.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(business_error(
            "Z.ai 业务 Token",
            body,
            "Z.ai 业务 Token 交换失败",
        ));
    }
    pick_string(body, &[&["data", "access_token"], &["data", "accessToken"]])
        .ok_or(UsageError::AuthRequired)
}

pub(super) fn require_billing(body: &Value) -> UsageResult<()> {
    if body.get("code").and_then(Value::as_i64) == Some(0) {
        return Ok(());
    }
    Err(business_error("ZCode 配额", body, "ZCode 配额接口返回失败"))
}

pub(super) fn require_profile(body: &Value, label: &str) -> UsageResult<()> {
    if body.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(business_error(label, body, "用户信息失败"));
    }
    match body.get("code") {
        None | Some(Value::Null) => Ok(()),
        Some(code) if profile_success(code) => Ok(()),
        Some(_) => Err(business_error(label, body, "用户信息失败")),
    }
}

pub(super) fn pick_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        let mut current = value;
        let mut found = true;
        for key in *path {
            match current.get(*key) {
                Some(next) => current = next,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if found
            && let Some(text) = current
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

fn profile_success(code: &Value) -> bool {
    match code {
        Value::Number(number) => matches!(number.as_i64(), Some(0 | 200)),
        Value::String(text) => matches!(text.trim(), "0" | "200"),
        _ => false,
    }
}

fn is_revoked(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    let code = value.get("error").and_then(|error| {
        error
            .as_str()
            .map(str::to_string)
            .or_else(|| error.get("status")?.as_str().map(str::to_string))
    });
    matches!(
        code.as_deref().map(str::trim).map(|text| text.to_ascii_lowercase()),
        Some(text) if text == "invalid_grant" || text == "invalid_token"
    )
}

fn business_error(label: &str, body: &Value, fallback: &str) -> UsageError {
    let detail = ["msg", "message", "error_description"]
        .iter()
        .find_map(|key| {
            body.get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| fallback.to_string());
    let rendered = body
        .get("code")
        .map(code_text)
        .unwrap_or_else(|| "unknown".into());
    UsageError::Fetcher(format!("{label} 业务错误 (code={rendered}): {detail}"))
}

fn code_text(code: &Value) -> String {
    match code {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".into(),
        _ => "unknown".into(),
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
                            seen_flag.lock().expect("seen").push(Seen {
                                method: request.method().as_str().to_string(),
                                path: request.url().to_string(),
                                headers,
                            });
                            let mut sink = Vec::new();
                            let _ = request.as_reader().read_to_end(&mut sink);
                            let next = script.lock().expect("script").pop_front();
                            let (status, body) = next.unwrap_or((500, "script exhausted".into()));
                            let _ = request.respond(
                                tiny_http::Response::from_string(body).with_status_code(status),
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
    use serde_json::json;

    #[test]
    fn status_table_matches_post_token_and_keeps_403_and_business_code() {
        let auth = read_json(401, "nope", "ZCode").unwrap_err();
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let revoked = read_json(400, r#"{"error":"invalid_grant"}"#, "ZCode").unwrap_err();
        assert!(matches!(revoked, UsageError::AuthRequired), "{revoked:?}");

        let forbidden = read_json(403, r#"{"msg":"no"}"#, "ZCode").unwrap_err();
        assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");
        assert!(!forbidden.is_transient());

        let limited = read_json(429, "slow", "ZCode").unwrap_err();
        assert!(matches!(limited, UsageError::Transient(_)), "{limited:?}");
        let down = read_json(503, "down", "ZCode").unwrap_err();
        assert!(matches!(down, UsageError::Transient(_)), "{down:?}");

        let business = read_json(200, r#"{"code":401,"msg":"expired"}"#, "ZCode").unwrap();
        let failed = require_token_envelope("zai", &business).unwrap_err();
        assert!(matches!(failed, UsageError::Fetcher(_)), "{failed:?}");
        assert!(!failed.is_transient());
        assert!(failed.to_string().contains("expired"), "{failed}");
        assert!(!matches!(failed, UsageError::AuthRequired));
    }

    #[test]
    fn zai_envelope_requires_code_zero_and_bigmodel_allows_a_missing_code() {
        let zai = require_token_envelope(
            "zai",
            &json!({
                "code": 0,
                "data": {
                    "token": "jwt",
                    "zai": {"access_token": "oauth"},
                    "expires_in": 3600,
                    "user": {"user_id": "u", "email": "a@b.c"}
                }
            }),
        )
        .unwrap();
        assert_eq!(zai.jwt, "jwt");
        assert_eq!(zai.provider_access_token, "oauth");
        assert_eq!(zai.expires_in, Some(3600));
        assert!(zai.refresh_token.is_none());

        let missing = require_token_envelope(
            "zai",
            &json!({"data": {"token": "jwt", "zai": {"access_token": "oauth"}}}),
        )
        .unwrap_err();
        assert!(matches!(missing, UsageError::Fetcher(_)), "{missing:?}");

        let bigmodel = require_token_envelope(
            "bigmodel",
            &json!({
                "data": {
                    "token": "jwt",
                    "bigmodel": {"accessToken": "access", "refreshToken": "refresh"}
                }
            }),
        )
        .unwrap();
        assert_eq!(bigmodel.provider_access_token, "access");
        assert_eq!(bigmodel.refresh_token.as_deref(), Some("refresh"));
        assert!(bigmodel.expires_in.is_none());

        let code_200 = require_token_envelope(
            "bigmodel",
            &json!({"code": 200, "data": {"token": "jwt", "access_token": "access"}}),
        )
        .unwrap_err();
        assert!(matches!(code_200, UsageError::Fetcher(_)), "{code_200:?}");
    }

    #[test]
    fn zai_business_token_accepts_zero_and_two_hundred_and_rejects_success_false() {
        let token = require_zai_business_token(&json!({
            "code": 200,
            "success": true,
            "data": {"accessToken": "business"}
        }))
        .unwrap();
        assert_eq!(token, "business");
        assert_eq!(
            require_zai_business_token(&json!({"code": "0", "data": {"access_token": "b2"}}))
                .unwrap(),
            "b2"
        );
        let failed = require_zai_business_token(&json!({"code": 0, "success": false, "msg": "no"}))
            .unwrap_err();
        assert!(matches!(failed, UsageError::Fetcher(_)), "{failed:?}");
        assert!(failed.to_string().contains("no"), "{failed}");
    }

    #[test]
    fn billing_requires_integer_zero_and_profile_accepts_two_hundred() {
        assert!(require_billing(&json!({"code": 0})).is_ok());
        let failed = require_billing(&json!({"code": "0", "msg": "bad"})).unwrap_err();
        assert!(matches!(failed, UsageError::Fetcher(_)), "{failed:?}");
        assert!(require_profile(&json!({"email": "a@b.c"}), "Z.ai 用户信息").is_ok());
        assert!(require_profile(&json!({"code": 200, "data": {}}), "BigModel 用户信息").is_ok());
        let denied =
            require_profile(&json!({"code": 7, "msg": "no"}), "Z.ai 用户信息").unwrap_err();
        assert!(matches!(denied, UsageError::Fetcher(_)), "{denied:?}");
        assert!(!denied.is_transient());
    }
}
