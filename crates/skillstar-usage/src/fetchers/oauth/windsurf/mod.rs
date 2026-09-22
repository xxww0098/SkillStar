//! Windsurf quota, browser login, and local/token import.
//!
mod import;
mod login;
mod quota;

use serde_json::{Map, Value};

use crate::UsageResult;
use crate::subscription::{Subscription, SubscriptionUsage};

pub(crate) const CATALOG_ID: &str = "windsurf";
pub(crate) const AUTH_BASE: &str = "https://www.windsurf.com";
pub(crate) const REGISTER_BASE: &str = "https://register.windsurf.com";
pub(crate) const DEFAULT_API_SERVER: &str = "https://server.codeium.com";
pub(crate) const AUTH1_API_SERVER: &str = "https://server.self-serve.windsurf.com";
pub(crate) const CLIENT_ID: &str = "3GUryQ7ldAeKEuD2obYnppsnmj58eP5u";
pub(crate) const SEAT_SERVICE: &str = "exa.seat_management_pb.SeatManagementService";
pub(crate) const CALLBACK_PATH: &str = "/windsurf-auth-callback";
pub const AUTH_STATUS_KEY: &str = "windsurfAuthStatus";
pub const SESSIONS_SECRET_KEY: &str =
    r#"secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.sessions"}"#;
pub const API_SERVER_SECRET_KEY: &str =
    r#"secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.apiServerUrl"}"#;

#[allow(unused_imports)]
pub(crate) use import::{import_from_local, import_from_token, oauth_row_from_imported};
#[allow(unused_imports)]
pub(crate) use login::start_login;

/// Quota refresh. The integrator's `dispatch` arm calls this.
pub(crate) async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    quota::fetch_quota(subscription).await
}

/// Plaintext provider blob. The token-import pipeline encrypts the string.
///
/// A bare apiKey (no JSON) is accepted on read so a row that stored only the
/// key still refreshes. Writers emit JSON so `apiServerUrl` survives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WindsurfState {
    pub api_key: Option<String>,
    pub api_server_url: Option<String>,
    pub auth1_token: Option<String>,
}

impl WindsurfState {
    pub(crate) fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(trimmed) {
            return Self {
                api_key: string_field(&map, &["apiKey", "api_key"]),
                api_server_url: string_field(&map, &["apiServerUrl", "api_server_url"]),
                auth1_token: string_field(&map, &["auth1Token", "auth1_token"]),
            };
        }
        if !trimmed.is_empty() && !trimmed.contains(char::is_whitespace) {
            return Self {
                api_key: Some(trimmed.to_string()),
                ..Self::default()
            };
        }
        Self::default()
    }

    pub(crate) fn to_json(&self) -> Option<String> {
        let mut map = Map::new();
        if let Some(api_key) = self.api_key.as_ref().filter(|value| !value.is_empty()) {
            map.insert("apiKey".to_string(), Value::String(api_key.clone()));
        }
        if let Some(url) = self
            .api_server_url
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            map.insert("apiServerUrl".to_string(), Value::String(url.clone()));
        }
        if let Some(token) = self.auth1_token.as_ref().filter(|value| !value.is_empty()) {
            map.insert("auth1Token".to_string(), Value::String(token.clone()));
        }
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map).to_string())
        }
    }
}

pub(crate) fn string_field(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = map.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub(crate) fn pick_string(value: Option<&Value>, keys: &[&str]) -> Option<String> {
    value
        .and_then(Value::as_object)
        .and_then(|map| string_field(map, keys))
}

pub(crate) fn decrypt_optional(cipher: &Option<String>) -> Option<String> {
    let plain = crate::crypto::decrypt(cipher.as_deref().unwrap_or(""));
    if plain.is_empty() { None } else { Some(plain) }
}

#[cfg(test)]
mod test_support {
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    pub struct MockSeat {
        pub base: String,
        addr: std::net::SocketAddr,
        stop: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl MockSeat {
        pub fn start(respond: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> Self {
            let server = tiny_http::Server::http("127.0.0.1:0").expect("bind mock seat");
            let addr = server
                .server_addr()
                .to_ip()
                .expect("mock seat is a TCP listener");
            let base = format!("http://{addr}");
            let stop = Arc::new(AtomicBool::new(false));
            let stop_flag = Arc::clone(&stop);
            let base_for_handler = base.clone();
            let handle = thread::spawn(move || {
                while !stop_flag.load(Ordering::SeqCst) {
                    match server.recv_timeout(Duration::from_millis(200)) {
                        Ok(Some(mut request)) => {
                            {
                                let reader = request.as_reader();
                                let mut sink = Vec::new();
                                let _ = reader.read_to_end(&mut sink);
                            }
                            let (status, body) = respond(&base_for_handler, request.url());
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
                stop,
                handle: Some(handle),
            }
        }
    }

    impl Drop for MockSeat {
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
    fn entry_points_stay_on_the_module() {
        // Names the re-exports so an unregistered module still typechecks as
        // the integrator's `windsurf::start_login` / `fetch` / import surface.
        let _ = (
            start_login,
            fetch,
            import_from_local,
            import_from_token,
            oauth_row_from_imported,
        );
    }

    #[test]
    fn bare_api_key_is_provider_state_and_garbage_is_rejected() {
        let imported = import_from_token("sk-ws-testkey12").expect("api key");
        let state = WindsurfState::parse(imported.provider_state.as_deref().unwrap());
        assert_eq!(state.api_key.as_deref(), Some("sk-ws-testkey12"));
        assert!(imported.access_token.is_empty());
        assert!(import_from_token("hello").is_err());
        assert!(import_from_token("{").is_err());
        assert!(import_from_token("{}").is_err());
    }

    #[test]
    fn bare_provider_state_still_parses_as_an_api_key() {
        let state = WindsurfState::parse("sk-ws-testkey12");
        assert_eq!(state.api_key.as_deref(), Some("sk-ws-testkey12"));
        assert!(state.api_server_url.is_none());
    }
}
