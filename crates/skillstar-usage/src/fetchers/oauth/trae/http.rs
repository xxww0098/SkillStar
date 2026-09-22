//! Trae HTTP. 401 / `invalid_grant` / `invalid_token` → [`UsageError::AuthRequired`].
//! 429 / 5xx / transport → [`UsageError::Transient`]. Anything else, including
//! 403, is [`UsageError::Fetcher`] even when the body says `invalid_grant`.
//! HTTP 200 with a numeric `code` other than 0 is Fetcher, not Transient.

use serde_json::{Map, Value};

use crate::{UsageError, UsageResult};

pub(super) const USER_AGENT: &str = "Trae/1.0.0 antigravity-cockpit-tools";

pub(super) fn classify(status: u16, body: &str, label: &str) -> UsageResult<Value> {
    if status == 403 {
        return Err(UsageError::http_status(label, status, body));
    }
    if status == 401 || is_invalid_grant(body) {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(label, status, body));
    }
    if body.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let value: Value = serde_json::from_str(body).map_err(|err| {
        UsageError::Fetcher(format!(
            "{label} 响应解析失败: {err}: {}",
            crate::truncate_body(body)
        ))
    })?;
    if let Some(code) = value.get("code").and_then(super::json_i64)
        && code != 0 {
            let detail = super::pick_string(&value, &[&["message"], &["msg"], &["error"]])
                .unwrap_or_else(|| "business error".into());
            return Err(UsageError::Fetcher(format!(
                "{label} 业务错误 {code}: {detail}"
            )));
        }
    Ok(value)
}

pub(super) async fn post_json(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&'static str, String)],
    body: &Value,
    label: &str,
) -> UsageResult<Value> {
    let mut request = client.post(url);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    let response = request
        .json(body)
        .send()
        .await
        .map_err(|err| UsageError::transport(label, err))?;
    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|err| UsageError::transport(label, err))?;
    classify(status, &text, label)
}

/// Keep the more serious retry class. Transient outranks auth, so one region
/// blip does not latch reauth when another host returned 401.
pub(super) fn prefer_error(current: Option<UsageError>, new: UsageError) -> UsageError {
    match current {
        Some(existing) if rank(&existing) >= rank(&new) => existing,
        _ => new,
    }
}

fn rank(err: &UsageError) -> u8 {
    match err {
        UsageError::Transient(_) => 3,
        UsageError::AuthRequired => 2,
        _ => 1,
    }
}

fn is_invalid_grant(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    for key in ["error", "error_description", "code"] {
        if let Some(text) = super::pick_string(&value, &[&[key]])
            && is_invalid_grant_text(&text)
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

pub(super) fn cloudide_headers(access_token: &str) -> Vec<(&'static str, String)> {
    let token = access_token.trim();
    vec![
        ("Accept", "application/json".to_string()),
        ("User-Agent", USER_AGENT.to_string()),
        ("Authorization", format!("Bearer {token}")),
        ("x-cloudide-token", token.to_string()),
    ]
}

pub(super) fn pay_headers(access_token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Accept", "application/json".to_string()),
        ("User-Agent", USER_AGENT.to_string()),
        (
            "Authorization",
            format!("Cloud-IDE-JWT {}", access_token.trim()),
        ),
    ]
}

pub(super) fn exchange_headers(access_token: Option<&str>) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Accept", "application/json".to_string()),
        ("User-Agent", USER_AGENT.to_string()),
    ];
    if let Some(token) = access_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        headers.push(("Authorization", format!("Bearer {token}")));
        headers.push(("x-cloudide-token", token.to_string()));
    }
    headers
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
    fn status_table_keeps_403_out_of_auth_and_business_codes_as_fetcher() {
        let auth = classify(401, "nope", "Trae exchange").unwrap_err();
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let revoked = classify(
            400,
            r#"{"error":"invalid_grant","error_description":"revoked"}"#,
            "Trae exchange",
        )
        .unwrap_err();
        assert!(matches!(revoked, UsageError::AuthRequired), "{revoked:?}");

        let token = classify(400, r#"{"error":"invalid_token"}"#, "Trae exchange").unwrap_err();
        assert!(matches!(token, UsageError::AuthRequired), "{token:?}");

        let coded = classify(200, r#"{"code":"invalid_grant"}"#, "Trae exchange").unwrap_err();
        assert!(matches!(coded, UsageError::AuthRequired), "{coded:?}");

        let forbidden = classify(
            403,
            r#"{"error":"invalid_grant","message":"no"}"#,
            "Trae exchange",
        )
        .unwrap_err();
        assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");
        assert!(!forbidden.is_transient());

        for status in [429_u16, 500, 502, 503] {
            let error = classify(status, "down", "Trae exchange").unwrap_err();
            assert!(
                matches!(error, UsageError::Transient(_)),
                "{status} {error:?}"
            );
        }

        let other = classify(
            400,
            r#"{"error":"unsupported_grant_type"}"#,
            "Trae exchange",
        )
        .unwrap_err();
        assert!(matches!(other, UsageError::Fetcher(_)), "{other:?}");

        let business =
            classify(200, r#"{"code":120,"message":"region"}"#, "Trae quota").unwrap_err();
        assert!(matches!(business, UsageError::Fetcher(_)), "{business:?}");
        assert!(business.to_string().contains("120"), "{business}");
        assert!(!business.is_transient());

        let ok = classify(200, r#"{"code":0,"Result":{"Token":"a"}}"#, "Trae exchange").unwrap();
        assert_eq!(ok["Result"]["Token"], "a");
    }
}
