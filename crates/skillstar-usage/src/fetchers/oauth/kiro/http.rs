//! HTTP for Kiro token and quota calls.
//!
//! Status table matches `oauth::token_endpoint::parse_token_body`:
//! 401 / `invalid_grant` / `invalid_token` → [`UsageError::AuthRequired`],
//! 429 / 5xx / transport → [`UsageError::Transient`], anything else
//! (including 403) → [`UsageError::Fetcher`]. Device-flow
//! `authorization_pending` and `slow_down` are not failures; the IDC poll
//! handles those before this table.

use serde_json::Value;

use crate::{UsageError, UsageResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TokenGrant {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub raw: Value,
}

pub(super) async fn post_json(
    client: &reqwest::Client,
    url: &str,
    body: &Value,
    label: &str,
) -> UsageResult<Value> {
    let (status, text) = send(client.post(url).json(body), label).await?;
    classify_status(status, &text, label)?;
    serde_json::from_str(&text)
        .map_err(|err| UsageError::Fetcher(format!("{label} 响应解析失败: {err}")))
}

pub(super) async fn post_json_token(
    client: &reqwest::Client,
    url: &str,
    body: &Value,
    label: &str,
) -> UsageResult<TokenGrant> {
    let (status, text) = send(client.post(url).json(body), label).await?;
    token_from_http(status, &text, label)
}

pub(super) async fn post_form_token(
    client: &reqwest::Client,
    url: &str,
    form: &[(&str, &str)],
    label: &str,
) -> UsageResult<TokenGrant> {
    let (status, text) = send(client.post(url).form(form), label).await?;
    token_from_http(status, &text, label)
}

pub(super) async fn get_bearer(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
    label: &str,
) -> UsageResult<Value> {
    let (status, text) = send(
        client
            .get(url)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", access_token.trim()),
            )
            .header(reqwest::header::ACCEPT, "application/json"),
        label,
    )
    .await?;
    classify_status(status, &text, label)?;
    serde_json::from_str(&text)
        .map_err(|err| UsageError::Fetcher(format!("{label} 响应解析失败: {err}")))
}

pub(super) fn token_from_http(status: u16, body: &str, label: &str) -> UsageResult<TokenGrant> {
    classify_status(status, body, label)?;
    let grant = parse_token_grant(body, label)?;
    if grant.access_token.trim().is_empty() {
        return Err(UsageError::AuthRequired);
    }
    Ok(grant)
}

pub(super) fn classify_status(status: u16, body: &str, label: &str) -> UsageResult<()> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    let code = oauth_error_code(body);
    if status == 401 || matches!(code.as_deref(), Some("invalid_grant" | "invalid_token")) {
        return Err(UsageError::AuthRequired);
    }
    Err(UsageError::http_status(label, status, body))
}

pub(super) fn oauth_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("error")
        .and_then(|error| {
            error
                .as_str()
                .map(str::to_string)
                .or_else(|| error.get("status")?.as_str().map(str::to_string))
        })
        .map(|code| code.trim().to_ascii_lowercase())
        .filter(|code| !code.is_empty())
}

pub(super) fn parse_token_grant(body: &str, label: &str) -> UsageResult<TokenGrant> {
    let value: Value = serde_json::from_str(body)
        .map_err(|err| UsageError::Fetcher(format!("{label} 响应解析失败: {err}")))?;
    let token = unwrap_data(value);
    let access_token = super::string_field(
        Some(&token),
        &["accessToken", "access_token", "token", "accessTokenJwt"],
    )
    .unwrap_or_default();
    let refresh_token = super::string_field(Some(&token), &["refreshToken", "refresh_token"]);
    let expires_in = number_field(&token, &["expiresIn", "expires_in"]);
    Ok(TokenGrant {
        access_token,
        refresh_token,
        expires_in,
        raw: token,
    })
}

fn unwrap_data(mut value: Value) -> Value {
    if let Some(data) = value
        .as_object_mut()
        .and_then(|obj| obj.remove("data"))
        .filter(|data| data.is_object())
    {
        data
    } else {
        value
    }
}

fn number_field(value: &Value, keys: &[&str]) -> Option<i64> {
    let map = value.as_object()?;
    for key in keys {
        let Some(item) = map.get(*key) else {
            continue;
        };
        if let Some(number) = item.as_i64().or_else(|| item.as_u64().map(|n| n as i64)) {
            return Some(number);
        }
        if let Some(number) = item.as_f64().filter(|n| n.is_finite()) {
            return Some(number.round() as i64);
        }
        if let Some(text) = item.as_str()
            && let Ok(number) = text.trim().parse::<i64>()
        {
            return Some(number);
        }
    }
    None
}

async fn send(request: reqwest::RequestBuilder, label: &str) -> UsageResult<(u16, String)> {
    let response = request
        .send()
        .await
        .map_err(|err| UsageError::transport(label, err))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
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

    pub struct ScriptedHttp {
        pub base: String,
        addr: std::net::SocketAddr,
        seen: Arc<Mutex<Vec<String>>>,
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
                            {
                                let mut sink = Vec::new();
                                let _ = request.as_reader().read_to_end(&mut sink);
                            }
                            seen_flag.lock().expect("seen").push(format!(
                                "{} {}",
                                request.method(),
                                request.url()
                            ));
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

        pub fn seen(&self) -> Vec<String> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiro_token_status_matches_post_token_and_403_is_not_auth() {
        let auth = token_from_http(401, "nope", "Kiro token").unwrap_err();
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let revoked = token_from_http(
            400,
            r#"{"error":"invalid_grant","error_description":"revoked"}"#,
            "Kiro token",
        )
        .unwrap_err();
        assert!(matches!(revoked, UsageError::AuthRequired), "{revoked:?}");

        let forbidden = token_from_http(403, r#"{"message":"no"}"#, "Kiro 用量").unwrap_err();
        assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");
        assert!(!forbidden.is_transient());

        let limited = token_from_http(429, "slow", "Kiro token").unwrap_err();
        assert!(matches!(limited, UsageError::Transient(_)), "{limited:?}");

        let down = classify_status(503, "down", "Kiro token").unwrap_err();
        assert!(matches!(down, UsageError::Transient(_)), "{down:?}");

        let other = token_from_http(400, r#"{"error":"unsupported_grant_type"}"#, "Kiro token")
            .unwrap_err();
        assert!(matches!(other, UsageError::Fetcher(_)), "{other:?}");

        let empty = token_from_http(200, r#"{"refreshToken":"rt"}"#, "Kiro token").unwrap_err();
        assert!(matches!(empty, UsageError::AuthRequired), "{empty:?}");
    }

    #[test]
    fn kiro_token_grant_accepts_camel_case_and_data_wrapper() {
        let grant = parse_token_grant(
            r#"{"data":{"accessToken":"at","refreshToken":"rt","expiresIn":"3600"}}"#,
            "Kiro token",
        )
        .unwrap();
        assert_eq!(grant.access_token, "at");
        assert_eq!(grant.refresh_token.as_deref(), Some("rt"));
        assert_eq!(grant.expires_in, Some(3600));
    }
}
