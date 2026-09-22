//! HTTP for Qoder poll and quota.
//!
//! Status table: 401 / `invalid_grant` / `invalid_token` → [`UsageError::AuthRequired`],
//! 429 / 5xx / transport → [`UsageError::Transient`], anything else including
//! 403 → [`UsageError::Fetcher`]. A JSON `code` on HTTP 200 that is not a
//! success code is Fetcher, not Transient (same lesson as CodeBuddy). Poll
//! 404 means "not signed in yet" and is not an error.
//!
//! Cosy headers are built only in [`header_pairs`]. Poll and quota both call
//! [`apply_headers`].

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue};
use serde_json::Value;

use super::{QoderMachine, nonempty};
use crate::{UsageError, UsageResult};

#[cfg(test)]
pub(super) const OFFICIAL_COSY_HEADERS: &[&str] = &[
    "Cosy-Version",
    "Cosy-MachineToken",
    "Cosy-MachineType",
    "Cosy-MachineCode",
    "Cosy-MachineId",
    "Cosy-MachineHostname",
    "Cosy-MachineOS",
    "Cosy-ClientType",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeviceGrant {
    pub token: String,
    pub refresh_token: Option<String>,
    pub user_id: Option<String>,
    pub expires_at: Option<i64>,
}

pub(super) enum PollBody {
    Pending,
    Ready(DeviceGrant),
}

pub(super) fn apply_headers(
    mut request: reqwest::RequestBuilder,
    access_token: Option<&str>,
    machine: &QoderMachine,
) -> reqwest::RequestBuilder {
    request = request.header(ACCEPT, "application/json");
    if let Some(token) = access_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let bearer = format!("Bearer {token}");
        if let Ok(value) = HeaderValue::from_str(&bearer) {
            request = request.header(AUTHORIZATION, value);
        }
    }
    for (name, value) in header_pairs(machine) {
        if let Ok(value) = HeaderValue::from_str(&value) {
            request = request.header(name, value);
        }
    }
    request
}

/// The only Cosy header list. Missing machine fields are omitted; `Cosy-MachineOS`
/// falls back to this process, and `Cosy-ClientType` is always `0` (cockpit).
pub(super) fn header_pairs(machine: &QoderMachine) -> Vec<(&'static str, String)> {
    let mut pairs = Vec::new();
    push(&mut pairs, "Cosy-Version", machine.cosy_version.as_deref());
    push(
        &mut pairs,
        "Cosy-MachineToken",
        machine.machine_token.as_deref(),
    );
    push(
        &mut pairs,
        "Cosy-MachineType",
        machine.machine_type.as_deref(),
    );
    push(
        &mut pairs,
        "Cosy-MachineCode",
        machine.machine_code.as_deref(),
    );
    push(&mut pairs, "Cosy-MachineId", machine.machine_id.as_deref());
    push(
        &mut pairs,
        "Cosy-MachineHostname",
        machine.hostname.as_deref(),
    );
    let os = machine
        .os
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(cosy_machine_os);
    push(&mut pairs, "Cosy-MachineOS", Some(&os));
    push(&mut pairs, "Cosy-ClientType", Some("0"));
    pairs
}

pub(super) fn cosy_machine_os() -> String {
    let arch = match std::env::consts::ARCH {
        "arm64" | "aarch64" => "aarch64",
        other => other,
    };
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{arch}_{os}")
}

pub(super) async fn get_json(
    client: &reqwest::Client,
    url: &str,
    access_token: Option<&str>,
    machine: &QoderMachine,
    label: &str,
) -> UsageResult<Value> {
    let response = apply_headers(client.get(url), access_token, machine)
        .send()
        .await
        .map_err(|err| UsageError::transport(label, err))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    classify_json(status, &body, label)
}

pub(super) fn classify_json(status: u16, body: &str, label: &str) -> UsageResult<Value> {
    if status == 429 || (500..600).contains(&status) {
        return Err(UsageError::http_status(label, status, body));
    }
    if is_auth(status, body) {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(label, status, body));
    }
    let value: Value = serde_json::from_str(body)
        .map_err(|err| UsageError::Fetcher(format!("{label} 响应解析失败: {err}")))?;
    if let Some(err) = business_error(&value, label) {
        return Err(err);
    }
    Ok(value)
}

pub(super) fn interpret_poll(status: u16, body: &str) -> UsageResult<PollBody> {
    if status == 404 {
        return Ok(PollBody::Pending);
    }
    if status == 429 || (500..600).contains(&status) {
        return Err(UsageError::http_status("Qoder deviceToken", status, body));
    }
    if is_auth(status, body) {
        return Err(UsageError::AuthRequired);
    }
    if (200..300).contains(&status) {
        if body.trim().is_empty() {
            return Ok(PollBody::Pending);
        }
        let value: Value = serde_json::from_str(body)
            .map_err(|err| UsageError::Fetcher(format!("Qoder deviceToken 响应解析失败: {err}")))?;
        if let Some(err) = business_error(&value, "Qoder deviceToken") {
            return Err(err);
        }
        if let Some(grant) = device_grant(&value) {
            return Ok(PollBody::Ready(grant));
        }
        return Ok(PollBody::Pending);
    }
    Err(UsageError::http_status("Qoder deviceToken", status, body))
}

pub(super) fn endpoint(base: &str, path: &str) -> String {
    format!("{}{path}", base.trim_end_matches('/'))
}

fn device_grant(value: &Value) -> Option<DeviceGrant> {
    let token = super::pick_string(
        value,
        &["token", "accessToken", "access_token", "securityOauthToken"],
    )
    .filter(|token| !token.eq_ignore_ascii_case("null"))?;
    // A machine blob also has `token` in cockpit's cache file. Poll responses
    // carry the user token at the top or under `data`.
    Some(DeviceGrant {
        token,
        refresh_token: super::pick_string(value, &["refreshToken", "refresh_token"]),
        user_id: super::pick_string(value, &["user_id", "userId", "uid"]),
        expires_at: expiry_of(value),
    })
}

fn expiry_of(value: &Value) -> Option<i64> {
    let raw = super::pick_string(value, &["expires_at", "expiresAt", "expireTime"])?;
    parse_expiry(&raw)
}

pub(super) fn parse_expiry(raw: &str) -> Option<i64> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(number) = text.parse::<i64>() {
        return normalize_epoch(number);
    }
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|value| value.timestamp())
}

fn normalize_epoch(number: i64) -> Option<i64> {
    if number <= 0 {
        return None;
    }
    if number > 1_000_000_000_000 {
        Some(number / 1000)
    } else {
        Some(number)
    }
}

fn is_auth(status: u16, body: &str) -> bool {
    if status == 401 {
        return true;
    }
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    invalid_grant(&value)
}

fn invalid_grant(value: &Value) -> bool {
    for key in ["error", "code", "error_description"] {
        if let Some(text) = super::object_string(value, &[key])
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
        "invalid_grant" | "invalid_token" | "loginexpire" | "login_expire"
    )
}

/// HTTP 200 business failure. Success codes are `0`, `200`, `ok`, `success`.
fn business_error(value: &Value, label: &str) -> Option<UsageError> {
    let code = value.get("code")?;
    if code.is_null() || is_success_code(code) {
        return None;
    }
    if code_text(code).is_some_and(|text| is_invalid_grant_text(&text)) {
        return Some(UsageError::AuthRequired);
    }
    let detail =
        super::object_string(value, &["message", "msg"]).unwrap_or_else(|| "business error".into());
    let rendered = code_text(code).unwrap_or_else(|| "unknown".into());
    Some(UsageError::Fetcher(format!(
        "{label} 业务错误 (code={rendered}): {detail}"
    )))
}

fn is_success_code(code: &Value) -> bool {
    match code {
        Value::Number(number) => {
            let integer = number.as_i64();
            integer == Some(0) || integer == Some(200)
        }
        Value::String(text) => {
            let lower = text.trim().to_ascii_lowercase();
            matches!(lower.as_str(), "0" | "200" | "ok" | "success")
        }
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

fn push(pairs: &mut Vec<(&'static str, String)>, name: &'static str, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    if value.chars().any(|ch| ch.is_control() || !ch.is_ascii()) {
        return;
    }
    pairs.push((name, value.to_string()));
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
                            {
                                let mut sink = Vec::new();
                                let _ = request.as_reader().read_to_end(&mut sink);
                            }
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

    #[test]
    fn qoder_status_table_keeps_403_out_of_auth_and_business_code_out_of_transient() {
        let auth = classify_json(401, "nope", "Qoder userinfo").unwrap_err();
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let revoked = classify_json(
            400,
            r#"{"error":"invalid_grant","error_description":"revoked"}"#,
            "Qoder userinfo",
        )
        .unwrap_err();
        assert!(matches!(revoked, UsageError::AuthRequired), "{revoked:?}");

        let forbidden = classify_json(403, r#"{"message":"no"}"#, "Qoder userinfo").unwrap_err();
        assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");
        assert!(!forbidden.is_transient());

        let limited = classify_json(429, "slow", "Qoder userinfo").unwrap_err();
        assert!(matches!(limited, UsageError::Transient(_)), "{limited:?}");

        let down = classify_json(503, "down", "Qoder userinfo").unwrap_err();
        assert!(matches!(down, UsageError::Transient(_)), "{down:?}");

        let business = classify_json(200, r#"{"code":1001,"message":"denied"}"#, "Qoder userinfo")
            .unwrap_err();
        assert!(matches!(business, UsageError::Fetcher(_)), "{business:?}");
        assert!(!business.is_transient());
        assert!(business.to_string().contains("1001"), "{business}");

        let grant =
            classify_json(200, r#"{"code":"invalid_grant"}"#, "Qoder userinfo").unwrap_err();
        assert!(matches!(grant, UsageError::AuthRequired), "{grant:?}");

        let ok = classify_json(200, r#"{"code":0,"data":{"id":"u"}}"#, "Qoder userinfo").unwrap();
        assert_eq!(ok["data"]["id"], "u");
    }

    #[test]
    fn qoder_header_pairs_are_the_cockpit_set_and_skip_blanks() {
        let full = QoderMachine {
            machine_token: Some("machine-token".into()),
            machine_id: Some("machine-id".into()),
            machine_type: Some("machine-type".into()),
            machine_code: Some("machine-code".into()),
            hostname: Some("machine-hostname".into()),
            os: Some("aarch64_darwin".into()),
            cosy_version: Some("1.27.1".into()),
        };
        let pairs = header_pairs(&full);
        let names: Vec<_> = pairs.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, OFFICIAL_COSY_HEADERS);
        assert_eq!(pair(&pairs, "Cosy-Version"), Some("1.27.1"));
        assert_eq!(pair(&pairs, "Cosy-MachineToken"), Some("machine-token"));
        assert_eq!(pair(&pairs, "Cosy-MachineType"), Some("machine-type"));
        assert_eq!(pair(&pairs, "Cosy-MachineCode"), Some("machine-code"));
        assert_eq!(pair(&pairs, "Cosy-MachineId"), Some("machine-id"));
        assert_eq!(
            pair(&pairs, "Cosy-MachineHostname"),
            Some("machine-hostname")
        );
        assert_eq!(pair(&pairs, "Cosy-MachineOS"), Some("aarch64_darwin"));
        assert_eq!(pair(&pairs, "Cosy-ClientType"), Some("0"));

        let bare = header_pairs(&QoderMachine::default());
        assert!(pair(&bare, "Cosy-MachineToken").is_none());
        assert!(pair(&bare, "Cosy-Version").is_none());
        assert_eq!(
            pair(&bare, "Cosy-MachineOS"),
            Some(cosy_machine_os().as_str())
        );
        assert_eq!(pair(&bare, "Cosy-ClientType"), Some("0"));
    }

    fn pair<'a>(pairs: &'a [(&'static str, String)], name: &str) -> Option<&'a str> {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }
}
