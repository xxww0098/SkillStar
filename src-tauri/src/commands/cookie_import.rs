use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_core::infra::{error::AppError, paths};
use skillstar_usage::catalog::AuthMode;
use skillstar_usage::cookie_jar::{self, CookieEntry};
use skillstar_usage::subscription::{BillingCycle, Subscription};
use skillstar_usage::{crypto, storage, UsageError};
use tiny_http::{Header, Method, Response, Server, StatusCode};

use super::usage_dto::SubscriptionDto;

const IMPORT_PORT: u16 = 1461;
const SESSION_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
struct ImportSession {
    token: String,
    provider: String,
    subscription_id: Option<String>,
    created_at: Instant,
    imported_subscription_id: Option<String>,
    error: Option<String>,
}

static SESSIONS: LazyLock<Mutex<HashMap<String, ImportSession>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SERVER_STARTED: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

#[derive(Debug, Clone, Serialize)]
pub struct CookieImportSessionDto {
    pub session_id: String,
    pub token: String,
    pub bind_url: String,
    pub provider: String,
    pub url: String,
    pub expires_in_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CookieImportStatusDto {
    pub status: String,
    pub subscription_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CookieBridgeBindingStatusDto {
    pub provider: String,
    pub bound: bool,
    pub subscription_id: Option<String>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CookieImportPayload {
    provider: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    bind_token: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    cookies: Vec<BrowserCookie>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CookieBridgeBinding {
    provider: String,
    token_hash: String,
    subscription_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CookieBridgeBindingsFile {
    bindings: Vec<CookieBridgeBinding>,
}

#[derive(Debug, Deserialize)]
struct BrowserCookie {
    name: String,
    value: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default, alias = "expirationDate")]
    expiration_date: Option<f64>,
    #[serde(default, alias = "httpOnly")]
    http_only: bool,
    #[serde(default)]
    secure: bool,
}

#[derive(Debug, Serialize)]
struct CookieImportResponse {
    ok: bool,
    subscription_id: Option<String>,
    bind_token: Option<String>,
    error: Option<String>,
}

fn map_usage_err(e: UsageError) -> AppError {
    AppError::Other(format!("Usage: {}", e))
}

#[tauri::command]
pub fn start_cookie_import_session(
    provider: String,
    subscription_id: Option<String>,
) -> Result<CookieImportSessionDto, AppError> {
    if provider != "opencode" {
        return Err(AppError::Other(format!(
            "Usage: 暂不支持通过插件导入 {} Cookie",
            provider
        )));
    }

    ensure_server_started()?;
    prune_sessions();

    let id = uuid::Uuid::new_v4().to_string();
    let token = uuid::Uuid::new_v4().to_string();
    let session = ImportSession {
        token: token.clone(),
        provider: provider.clone(),
        subscription_id,
        created_at: Instant::now(),
        imported_subscription_id: None,
        error: None,
    };
    SESSIONS.lock().unwrap().insert(id.clone(), session);

    Ok(CookieImportSessionDto {
        session_id: id,
        token,
        bind_url: format!("http://127.0.0.1:{IMPORT_PORT}/usage/cookie-import"),
        provider,
        url: format!("http://127.0.0.1:{IMPORT_PORT}/usage/cookie-import"),
        expires_in_secs: SESSION_TTL.as_secs(),
    })
}

#[tauri::command]
pub fn get_cookie_import_status(session_id: String) -> Result<CookieImportStatusDto, AppError> {
    let mut sessions = SESSIONS.lock().unwrap();
    let Some(session) = sessions.get(&session_id) else {
        return Ok(CookieImportStatusDto {
            status: "expired".to_string(),
            subscription_id: None,
            error: Some("导入会话不存在或已过期".to_string()),
        });
    };
    if session.created_at.elapsed() > SESSION_TTL {
        sessions.remove(&session_id);
        return Ok(CookieImportStatusDto {
            status: "expired".to_string(),
            subscription_id: None,
            error: Some("导入会话已过期".to_string()),
        });
    }
    if let Some(error) = &session.error {
        return Ok(CookieImportStatusDto {
            status: "error".to_string(),
            subscription_id: None,
            error: Some(error.clone()),
        });
    }
    if let Some(id) = &session.imported_subscription_id {
        return Ok(CookieImportStatusDto {
            status: "completed".to_string(),
            subscription_id: Some(id.clone()),
            error: None,
        });
    }
    Ok(CookieImportStatusDto {
        status: "pending".to_string(),
        subscription_id: None,
        error: None,
    })
}

#[tauri::command]
pub fn cancel_cookie_import_session(session_id: String) -> Result<(), AppError> {
    SESSIONS.lock().unwrap().remove(&session_id);
    Ok(())
}

#[tauri::command]
pub fn get_cookie_import_subscription(id: String) -> Result<SubscriptionDto, AppError> {
    let sub = storage::get_subscription(&id).map_err(map_usage_err)?;
    let usage = storage::get_usage_snapshot(&id).map_err(map_usage_err)?;
    Ok(SubscriptionDto::from_parts(sub, usage))
}

#[tauri::command]
pub fn get_cookie_bridge_binding_status(provider: String) -> Result<CookieBridgeBindingStatusDto, AppError> {
    let binding = load_bindings()
        .map_err(|e| AppError::Other(format!("Usage: {}", e)))?
        .bindings
        .into_iter()
        .find(|binding| binding.provider == provider);
    Ok(CookieBridgeBindingStatusDto {
        provider,
        bound: binding.is_some(),
        subscription_id: binding.as_ref().and_then(|binding| binding.subscription_id.clone()),
        updated_at: binding.map(|binding| binding.updated_at),
    })
}

#[tauri::command]
pub fn reset_cookie_bridge_binding(provider: String) -> Result<(), AppError> {
    let mut file = load_bindings().map_err(|e| AppError::Other(format!("Usage: {}", e)))?;
    file.bindings.retain(|binding| binding.provider != provider);
    save_bindings(&file).map_err(|e| AppError::Other(format!("Usage: {}", e)))
}

fn ensure_server_started() -> Result<(), AppError> {
    let mut started = SERVER_STARTED.lock().unwrap();
    if *started {
        return Ok(());
    }
    let server = Server::http(format!("127.0.0.1:{IMPORT_PORT}"))
        .map_err(|e| AppError::Other(format!("Usage: 无法启动 Cookie 导入服务：{}", e)))?;
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            handle_request(request);
        }
    });
    *started = true;
    Ok(())
}

fn handle_request(mut request: tiny_http::Request) {
    let method = request.method().clone();
    let path = request.url().split('?').next().unwrap_or_default().to_string();
    if method == Method::Options {
        let _ = request.respond(cors_response(Response::from_data(Vec::new()).with_status_code(StatusCode(204))));
        return;
    }
    if method != Method::Post || path != "/usage/cookie-import" {
        let _ = request.respond(cors_response(json_response(
            StatusCode(404),
            &CookieImportResponse {
                ok: false,
                subscription_id: None,
                bind_token: None,
                error: Some("not found".to_string()),
            },
        )));
        return;
    }

    let mut body = String::new();
    let result = request
        .as_reader()
        .take(256 * 1024)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str::<CookieImportPayload>(&body).map_err(|e| e.to_string()))
        .and_then(import_cookies);

    let response = match result {
        Ok(result) => CookieImportResponse {
            ok: true,
            subscription_id: Some(result.subscription_id),
            bind_token: result.bind_token,
            error: None,
        },
        Err(error) => CookieImportResponse {
            ok: false,
            subscription_id: None,
            bind_token: None,
            error: Some(error),
        },
    };
    let status = if response.ok { StatusCode(200) } else { StatusCode(400) };
    let _ = request.respond(cors_response(json_response(status, &response)));
}

struct ImportCookiesResult {
    subscription_id: String,
    bind_token: Option<String>,
}

fn import_cookies(payload: CookieImportPayload) -> Result<ImportCookiesResult, String> {
    if payload.provider != "opencode" {
        return Err("provider 不匹配".to_string());
    }

    let (session_id, subscription_id, new_bind_token) = if let Some(bind_token) = payload.bind_token.as_deref() {
        let binding = find_binding(&payload.provider, bind_token)?;
        (None, binding.subscription_id, None)
    } else {
        let token = payload
            .token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| "缺少导入码或绑定 token".to_string())?;

        let mut sessions = SESSIONS.lock().unwrap();
        let session_id = sessions
            .iter()
            .find_map(|(id, session)| {
                (session.provider == payload.provider && session.token == token).then(|| id.clone())
            })
            .ok_or_else(|| "导入码无效或已过期".to_string())?;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| "导入会话不存在".to_string())?;
        if session.created_at.elapsed() > SESSION_TTL {
            sessions.remove(&session_id);
            return Err("导入会话已过期".to_string());
        }
        (
            Some(session_id),
            session.subscription_id.clone(),
            Some(uuid::Uuid::new_v4().to_string()),
        )
    };

    let entries = payload
        .cookies
        .into_iter()
        .filter(valid_opencode_cookie)
        .map(|cookie| CookieEntry {
            name: cookie.name,
            value: cookie.value,
            domain: cookie.domain,
            path: cookie.path,
            expires: cookie.expiration_date.map(|v| v as i64),
            http_only: cookie.http_only,
            secure: cookie.secure,
            source_url: payload.source_url.clone(),
        })
        .collect::<Vec<_>>();

    if entries.is_empty() {
        if let Some(session_id) = &session_id
            && let Some(session) = SESSIONS.lock().unwrap().get_mut(session_id) {
                session.error = Some("没有收到 opencode.ai Cookie".to_string());
            }
        return Err("没有收到 opencode.ai Cookie".to_string());
    }

    let sub = upsert_opencode_cookie_subscription(subscription_id, &entries).map_err(|e| e.to_string())?;

    if let Some(session_id) = &session_id
        && let Some(session) = SESSIONS.lock().unwrap().get_mut(session_id) {
            session.imported_subscription_id = Some(sub.id.clone());
        }

    if let Some(bind_token) = &new_bind_token {
        upsert_binding(&payload.provider, bind_token, Some(sub.id.clone()))?;
    } else if let Some(bind_token) = payload.bind_token.as_deref() {
        touch_binding(&payload.provider, bind_token, Some(sub.id.clone()))?;
    }

    Ok(ImportCookiesResult {
        subscription_id: sub.id,
        bind_token: new_bind_token,
    })
}

fn bindings_path() -> std::path::PathBuf {
    paths::config_dir().join("cookie_bridge_bindings.json")
}

fn load_bindings() -> Result<CookieBridgeBindingsFile, String> {
    let path = bindings_path();
    if !path.exists() {
        return Ok(CookieBridgeBindingsFile::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("读取 Cookie 插件绑定失败：{}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("解析 Cookie 插件绑定失败：{}", e))
}

fn save_bindings(file: &CookieBridgeBindingsFile) -> Result<(), String> {
    let path = bindings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{}", e))?;
    }
    let text = serde_json::to_string_pretty(file).map_err(|e| format!("序列化 Cookie 插件绑定失败：{}", e))?;
    fs::write(path, text).map_err(|e| format!("保存 Cookie 插件绑定失败：{}", e))
}

fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn find_binding(provider: &str, token: &str) -> Result<CookieBridgeBinding, String> {
    let hash = token_hash(token);
    load_bindings()?
        .bindings
        .into_iter()
        .find(|binding| binding.provider == provider && binding.token_hash == hash)
        .ok_or_else(|| "插件尚未绑定 SkillStar，请先在 SkillStar 中启动一次插件导入".to_string())
}

fn upsert_binding(provider: &str, token: &str, subscription_id: Option<String>) -> Result<(), String> {
    let mut file = load_bindings()?;
    let hash = token_hash(token);
    let now = Utc::now().timestamp();
    file.bindings.retain(|binding| binding.provider != provider);
    file.bindings.push(CookieBridgeBinding {
        provider: provider.to_string(),
        token_hash: hash,
        subscription_id,
        created_at: now,
        updated_at: now,
    });
    save_bindings(&file)
}

fn touch_binding(provider: &str, token: &str, subscription_id: Option<String>) -> Result<(), String> {
    let mut file = load_bindings()?;
    let hash = token_hash(token);
    let now = Utc::now().timestamp();
    let Some(binding) = file
        .bindings
        .iter_mut()
        .find(|binding| binding.provider == provider && binding.token_hash == hash)
    else {
        return Err("插件尚未绑定 SkillStar，请先在 SkillStar 中启动一次插件导入".to_string());
    };
    binding.subscription_id = subscription_id;
    binding.updated_at = now;
    save_bindings(&file)
}

fn valid_opencode_cookie(cookie: &BrowserCookie) -> bool {
    if cookie.name.trim().is_empty() || cookie.value.is_empty() {
        return false;
    }
    cookie
        .domain
        .as_deref()
        .map(|domain| {
            let domain = domain.trim_start_matches('.');
            domain == "opencode.ai" || domain.ends_with(".opencode.ai")
        })
        .unwrap_or(true)
}

fn upsert_opencode_cookie_subscription(
    target_id: Option<String>,
    entries: &[CookieEntry],
) -> Result<Subscription, UsageError> {
    let now = Utc::now().timestamp();
    let existing = target_id
        .as_deref()
        .and_then(|id| storage::get_subscription(id).ok())
        .filter(|sub| sub.catalog_id == "opencode")
        .or_else(|| {
            storage::list_subscriptions().ok()?.into_iter().find(|sub| {
                sub.catalog_id == "opencode" && matches!(sub.auth_mode, AuthMode::Cookie | AuthMode::OAuth)
            })
        });

    let cookie_json = cookie_jar::serialize_cookie_jar(entries);
    let sub = Subscription {
        id: existing
            .as_ref()
            .map(|sub| sub.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        catalog_id: "opencode".to_string(),
        display_name: existing
            .as_ref()
            .map(|sub| sub.display_name.clone())
            .unwrap_or_else(|| "OpenCode".to_string()),
        auth_mode: AuthMode::Cookie,
        plan_tier: existing
            .as_ref()
            .and_then(|sub| sub.plan_tier.clone())
            .or_else(|| Some("Go".to_string())),
        monthly_price: existing.as_ref().and_then(|sub| sub.monthly_price),
        currency: existing
            .as_ref()
            .map(|sub| sub.currency.clone())
            .unwrap_or_else(|| "USD".to_string()),
        billing_cycle: existing
            .as_ref()
            .map(|sub| sub.billing_cycle)
            .unwrap_or(BillingCycle::Monthly),
        start_date: existing.as_ref().map(|sub| sub.start_date).unwrap_or(0),
        renew_date: existing.as_ref().map(|sub| sub.renew_date).unwrap_or(0),
        auto_renew: existing.as_ref().map(|sub| sub.auto_renew).unwrap_or(false),
        api_key_encrypted: None,
        access_token_encrypted: None,
        refresh_token_encrypted: None,
        access_token_expires_at: None,
        oauth_account_id: None,
        oauth_region: None,
        requires_reauth: false,
        fingerprint_id: None,
        cookie_jar_encrypted: Some(crypto::encrypt(&cookie_json)),
        cookie_session_expires_at: entries.iter().filter_map(|cookie| cookie.expires).min(),
        manual_quota: existing.as_ref().and_then(|sub| sub.manual_quota.clone()),
        note: existing.as_ref().and_then(|sub| sub.note.clone()),
        sort_index: existing.as_ref().map(|sub| sub.sort_index).unwrap_or(0),
        created_at: existing.as_ref().map(|sub| sub.created_at).unwrap_or(now),
        updated_at: now,
    };
    storage::upsert_subscription(sub)
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    let response = Response::from_data(body).with_status_code(status);
    with_header(response, "Content-Type", "application/json; charset=utf-8")
}

fn cors_response(response: Response<std::io::Cursor<Vec<u8>>>) -> Response<std::io::Cursor<Vec<u8>>> {
    let response = with_header(response, "Access-Control-Allow-Origin", "*");
    let response = with_header(response, "Access-Control-Allow-Methods", "POST, OPTIONS");
    with_header(response, "Access-Control-Allow-Headers", "Content-Type")
}

fn with_header(
    response: Response<std::io::Cursor<Vec<u8>>>,
    name: &str,
    value: &str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    match Header::from_bytes(name.as_bytes(), value.as_bytes()) {
        Ok(header) => response.with_header(header),
        Err(_) => response,
    }
}

fn prune_sessions() {
    SESSIONS
        .lock()
        .unwrap()
        .retain(|_, session| session.created_at.elapsed() <= SESSION_TTL);
}
