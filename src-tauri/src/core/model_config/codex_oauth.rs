//! OpenAI OAuth PKCE flow for Codex multi-account management.
//!
//! Uses the same public OAuth client as Codex CLI (`app_EMoamEEZ73f0CkXaXp7hrann`).
//! Flow: browser → auth.openai.com → local callback → token exchange → account creation.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::distr::{Distribution, StandardUniform};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const SCOPES: &str = "openid profile email offline_access";
const ORIGINATOR: &str = "codex_vscode";
const OAUTH_CALLBACK_PORT: u16 = 1455;
const OAUTH_TIMEOUT_SECONDS: i64 = 300;

pub fn get_callback_port() -> u16 {
    OAUTH_CALLBACK_PORT
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthLoginStartResponse {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthLoginCallbackEvent {
    login_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthLoginTimeoutEvent {
    login_id: String,
    callback_url: String,
    timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OAuthState {
    login_id: String,
    auth_url: String,
    redirect_uri: String,
    code_verifier: String,
    state: String,
    port: u16,
    expires_at: i64,
    code: Option<String>,
}

lazy_static::lazy_static! {
    static ref OAUTH_STATE: Arc<Mutex<Option<OAuthState>>> = Arc::new(Mutex::new(None));
    static ref COMPLETE_ATTEMPT_SEQ: AtomicU64 = AtomicU64::new(0);
}

fn generate_base64url_token() -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..32).map(|_| StandardUniform.sample(&mut rng)).collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let result = hasher.finalize();
    URL_SAFE_NO_PAD.encode(result)
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

fn set_oauth_state(state: Option<OAuthState>) {
    let mut guard = OAUTH_STATE.lock().unwrap();
    *guard = state;
}

fn find_available_port() -> Result<u16, String> {
    match TcpListener::bind(("127.0.0.1", OAUTH_CALLBACK_PORT)) {
        Ok(listener) => {
            drop(listener);
            Ok(OAUTH_CALLBACK_PORT)
        }
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            Err(format!("CODEX_OAUTH_PORT_IN_USE:{}", OAUTH_CALLBACK_PORT))
        }
        Err(e) => Err(format!("无法绑定端口 {}: {}", OAUTH_CALLBACK_PORT, e)),
    }
}

fn notify_cancel(port: u16) {
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
        let _ = stream
            .write_all(b"GET /cancel HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        let _ = stream.flush();
    }
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim();
            if key.is_empty() {
                return None;
            }
            let raw_value = parts.next().unwrap_or("");
            let decoded = urlencoding::decode(raw_value)
                .map(|v| v.into_owned())
                .unwrap_or_else(|_| raw_value.to_string());
            Some((key.to_string(), decoded))
        })
        .collect()
}

fn build_auth_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&state={}&originator={}",
        AUTH_ENDPOINT,
        CLIENT_ID,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(SCOPES),
        code_challenge,
        state,
        urlencoding::encode(ORIGINATOR)
    )
}

fn to_start_response(state: &OAuthState) -> OAuthLoginStartResponse {
    OAuthLoginStartResponse {
        login_id: state.login_id.clone(),
        auth_url: state.auth_url.clone(),
    }
}

fn clear_oauth_state_if_matches(expected_state: &str, expected_login_id: &str) {
    let should_clear = {
        let oauth_state = OAUTH_STATE.lock().unwrap();
        oauth_state
            .as_ref()
            .is_some_and(|s| s.state == expected_state && s.login_id == expected_login_id)
    };
    if should_clear {
        set_oauth_state(None);
    }
}

pub async fn start_oauth_login(app_handle: AppHandle) -> Result<OAuthLoginStartResponse, String> {
    // Check if there's an existing session
    {
        let oauth_state = OAUTH_STATE.lock().unwrap();
        if let Some(state) = oauth_state.as_ref() {
            if state.expires_at > now_timestamp() {
                tracing::info!(
                    "Codex OAuth 复用进行中的登录会话: login_id={}",
                    state.login_id
                );
                return Ok(to_start_response(state));
            }
        }
    }

    let port = find_available_port()?;
    let code_verifier = generate_base64url_token();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state_token = generate_base64url_token();
    let login_id = generate_base64url_token();
    let redirect_uri = format!("http://localhost:{}/auth/callback", port);
    let auth_url = build_auth_url(&redirect_uri, &code_challenge, &state_token);

    let oauth_state = OAuthState {
        login_id: login_id.clone(),
        auth_url: auth_url.clone(),
        redirect_uri: redirect_uri.clone(),
        code_verifier,
        state: state_token.clone(),
        port,
        expires_at: now_timestamp() + OAUTH_TIMEOUT_SECONDS,
        code: None,
    };

    set_oauth_state(Some(oauth_state));

    let expected_state = state_token;
    let expected_login_id = login_id.clone();
    let callback_url = redirect_uri;
    tokio::spawn(async move {
        if let Err(e) = start_callback_server(
            port,
            expected_state,
            expected_login_id,
            callback_url,
            app_handle,
        )
        .await
        {
            tracing::error!("OAuth 回调服务器错误: {}", e);
        }
    });

    tracing::info!("Codex OAuth 登录会话已创建: login_id={}", login_id);

    Ok(OAuthLoginStartResponse { login_id, auth_url })
}

async fn start_callback_server(
    port: u16,
    expected_state: String,
    expected_login_id: String,
    callback_url: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    use tiny_http::{Response, Server};

    let server = Server::http(format!("127.0.0.1:{}", port))
        .map_err(|e| format!("启动服务器失败: {}", e))?;
    let timeout = std::time::Duration::from_secs(OAUTH_TIMEOUT_SECONDS as u64);

    tracing::info!(
        "Codex OAuth 回调服务器启动: login_id={}, port={}",
        expected_login_id,
        port
    );

    let start = std::time::Instant::now();
    let mut clear_state_on_exit = false;

    loop {
        let should_stop = {
            let oauth_state = OAUTH_STATE.lock().unwrap();
            match oauth_state.as_ref() {
                Some(state) => state.state != expected_state || state.login_id != expected_login_id,
                None => true,
            }
        };

        if should_stop {
            tracing::info!(
                "Codex OAuth 已取消或状态已变更: login_id={}",
                expected_login_id
            );
            break;
        }

        if start.elapsed() > timeout {
            tracing::error!("Codex OAuth 回调超时: login_id={}", expected_login_id);
            clear_state_on_exit = true;
            break;
        }

        if let Ok(Some(request)) = server.try_recv() {
            let url = request.url().to_string();

            if url.starts_with("/auth/callback") {
                let query = url.split('?').nth(1).unwrap_or("");
                let params = parse_query_params(query);
                let code = params.get("code").cloned().unwrap_or_default();
                let state = params.get("state").cloned().unwrap_or_default();

                if state != expected_state {
                    let response = Response::from_string("State mismatch").with_status_code(400);
                    let _ = request.respond(response);
                    continue;
                }

                if code.is_empty() {
                    let response = Response::from_string("Missing code").with_status_code(400);
                    let _ = request.respond(response);
                    continue;
                }

                let html = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>授权成功</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); }
        .container { text-align: center; color: white; }
        h1 { font-size: 2.5rem; margin-bottom: 1rem; }
        p { font-size: 1.2rem; opacity: 0.9; }
    </style>
</head>
<body>
    <div class="container">
        <h1>✅ 授权成功</h1>
        <p>您可以关闭此窗口并返回 SkillStar</p>
    </div>
</body>
</html>"#;

                let response = Response::from_string(html).with_header(
                    tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"text/html; charset=utf-8"[..],
                    )
                    .unwrap(),
                );
                let _ = request.respond(response);

                // Store code in state
                {
                    let mut oauth_state = OAUTH_STATE.lock().unwrap();
                    if let Some(state_data) = oauth_state.as_mut() {
                        if state_data.state == expected_state
                            && state_data.login_id == expected_login_id
                        {
                            state_data.code = Some(code);
                        }
                    }
                }

                let _ = app_handle.emit(
                    "codex-oauth-login-completed",
                    OAuthLoginCallbackEvent {
                        login_id: expected_login_id.clone(),
                    },
                );

                tracing::info!("Codex OAuth 回调校验通过: login_id={}", expected_login_id);
                break;
            } else if url.starts_with("/cancel") {
                let response = Response::from_string("Login cancelled").with_status_code(200);
                let _ = request.respond(response);
                clear_oauth_state_if_matches(&expected_state, &expected_login_id);
                break;
            } else {
                let response = Response::from_string("Not Found").with_status_code(404);
                let _ = request.respond(response);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    if clear_state_on_exit {
        clear_oauth_state_if_matches(&expected_state, &expected_login_id);
        let _ = app_handle.emit(
            "codex-oauth-login-timeout",
            OAuthLoginTimeoutEvent {
                login_id: expected_login_id.clone(),
                callback_url,
                timeout_seconds: timeout.as_secs(),
            },
        );
    }

    Ok(())
}

async fn exchange_code_for_tokens(
    code: &str,
    code_verifier: &str,
    port: u16,
) -> Result<OAuthTokens, String> {
    let redirect_uri = format!("http://localhost:{}/auth/callback", port);
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &redirect_uri),
        ("client_id", CLIENT_ID),
        ("code_verifier", code_verifier),
    ];

    tracing::info!("Codex OAuth 开始交换 Token");

    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token 请求失败: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        tracing::error!(
            "Token 交换失败: {} - {}",
            status,
            &body[..body.len().min(200)]
        );
        return Err(format!("Token 交换失败: {}", body));
    }

    tracing::info!("Codex OAuth Token 交换成功");

    let token_response: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("解析 Token 响应失败: {}", e))?;

    let id_token = token_response
        .get("id_token")
        .and_then(|v| v.as_str())
        .ok_or("响应中缺少 id_token")?
        .to_string();

    let access_token = token_response
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("响应中缺少 access_token")?
        .to_string();

    let refresh_token = token_response
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(OAuthTokens {
        id_token,
        access_token,
        refresh_token,
    })
}

pub async fn complete_oauth_login(login_id: &str) -> Result<OAuthTokens, String> {
    let attempt_id = COMPLETE_ATTEMPT_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    tracing::info!(
        "Codex OAuth 开始完成登录: attempt_id={}, login_id={}",
        attempt_id,
        login_id
    );

    let (code, code_verifier, port) = {
        let oauth_state = OAUTH_STATE.lock().unwrap();
        let state = oauth_state
            .as_ref()
            .ok_or("OAuth 状态不存在，请重新发起授权")?;
        if state.expires_at <= now_timestamp() {
            return Err("OAuth 登录已超时，请重新发起授权".to_string());
        }
        if state.login_id != login_id {
            return Err("OAuth loginId 不匹配".to_string());
        }
        let code = state
            .code
            .clone()
            .ok_or("授权尚未完成，请先在浏览器中授权")?;
        (code, state.code_verifier.clone(), state.port)
    };

    let tokens = exchange_code_for_tokens(&code, &code_verifier, port).await?;

    set_oauth_state(None);

    tracing::info!(
        "Codex OAuth 完成: attempt_id={}, login_id={}",
        attempt_id,
        login_id
    );

    Ok(tokens)
}

pub fn cancel_oauth_flow(login_id: Option<&str>) -> Result<(), String> {
    let port = {
        let oauth_state = OAUTH_STATE.lock().unwrap();
        let Some(current) = oauth_state.as_ref() else {
            return Ok(());
        };

        if let Some(login_id) = login_id {
            if current.login_id != login_id {
                return Err("OAuth loginId 不匹配".to_string());
            }
        }
        current.port
    };

    set_oauth_state(None);
    notify_cancel(port);
    tracing::info!("Codex OAuth 流程已取消");
    Ok(())
}

pub fn submit_callback_url(login_id: &str, callback_url: &str) -> Result<(), String> {
    let trimmed = callback_url.trim();
    if trimmed.is_empty() {
        return Err("回调链接不能为空".to_string());
    }

    let (expected_state, port) = {
        let guard = OAUTH_STATE.lock().unwrap();
        let state = guard
            .as_ref()
            .ok_or_else(|| "OAuth 状态不存在，请重新发起授权".to_string())?;
        if state.login_id != login_id {
            return Err("OAuth loginId 不匹配".to_string());
        }
        (state.state.clone(), state.port)
    };

    // Parse the callback URL to extract code and state
    let parsed = url::Url::parse(
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else if trimmed.starts_with('/') {
            format!("http://localhost:{}{}", port, trimmed)
        } else {
            format!(
                "http://localhost:{}/auth/callback?{}",
                port,
                trimmed.trim_start_matches('?')
            )
        }
        .as_str(),
    )
    .map_err(|e| format!("回调链接格式无效: {}", e))?;

    let params = parse_query_params(parsed.query().unwrap_or_default());
    let code = params
        .get("code")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "回调链接中缺少 code 参数".to_string())?
        .to_string();
    let state = params
        .get("state")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "回调链接中缺少 state 参数".to_string())?;

    if state != expected_state {
        return Err("回调 state 校验失败".to_string());
    }

    let mut guard = OAUTH_STATE.lock().unwrap();
    let current = guard
        .as_mut()
        .ok_or_else(|| "OAuth 状态不存在，请重新发起授权".to_string())?;
    if current.login_id != login_id {
        return Err("OAuth loginId 不匹配".to_string());
    }
    current.code = Some(code);

    tracing::info!("Codex OAuth 已接收手动回调链接: login_id={}", login_id);
    Ok(())
}

/// Check if a JWT access token is expired (with 60s buffer).
pub fn is_token_expired(access_token: &str) -> bool {
    let parts: Vec<&str> = access_token.split('.').collect();
    if parts.len() != 3 {
        return true;
    }

    let payload_base64 = parts[1];
    let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(payload_base64) else {
        return true;
    };
    let Ok(payload_str) = String::from_utf8(payload_bytes) else {
        return true;
    };
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload_str) else {
        return true;
    };

    let Some(exp) = payload.get("exp").and_then(|e| e.as_i64()) else {
        return true;
    };

    let now = chrono::Utc::now().timestamp();
    exp < now + 60
}

/// Refresh an access token using a refresh token.
pub async fn refresh_access_token(refresh_token: &str) -> Result<OAuthTokens, String> {
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ];

    tracing::info!("Codex Token 刷新中...");

    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token 刷新请求失败: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        tracing::error!(
            "Token 刷新失败: {} - {}",
            status,
            &body[..body.len().min(200)]
        );
        return Err(format!("Token 刷新失败: {}", status));
    }

    tracing::info!("Codex Token 刷新成功");

    let token_response: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("解析 Token 响应失败: {}", e))?;

    let id_token = token_response
        .get("id_token")
        .and_then(|v| v.as_str())
        .ok_or("响应中缺少 id_token")?
        .to_string();

    let access_token = token_response
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("响应中缺少 access_token")?
        .to_string();

    let new_refresh_token = token_response
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| Some(refresh_token.to_string()));

    Ok(OAuthTokens {
        id_token,
        access_token,
        refresh_token: new_refresh_token,
    })
}

/// Check if the OAuth callback port is currently in use.
pub fn is_port_in_use() -> bool {
    matches!(
        TcpListener::bind(("127.0.0.1", OAUTH_CALLBACK_PORT)),
        Err(e) if e.kind() == ErrorKind::AddrInUse
    )
}
