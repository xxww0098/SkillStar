//! Kiro 登录：门户 PKCE（默认）与 AWS IAM Identity Center 设备码（可选）。
//!
//! 与 Cockpit Tools 一致的两条腿，靠 StartOptions.region 选：
//!
//! * 缺省 / portal / social → 门户 PKCE：授权页是
//!   https://app.kiro.dev/signin，回调是本机 /oauth/callback（粘贴
//!   /signin/callback 也能解析），换票打
//!   https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token。
//! * idc / aws-idc / 真实 AWS region → IdC 设备码：先 POST
//!   {oidc}.{region}.amazonaws.com/client/register 注册一个 public client，再
//!   POST /device_authorization 拿设备码，按 interval 轮询 /token。
//!
//! IdC 的 clientId / clientSecret 必须持久化，否则下一次刷新换不出票；它们进
//! provider_state（加密随记录落盘），这也是本 crate 里唯一需要 provider_state
//! 才能刷新的一家。门户腿只用 refreshToken，不需要注册信息。
//!
//! 不搬：浏览器侧的 login_option 分支（BuilderId / Google / GitHub 由门户页面
//! 自己处理），以及 Cockpit Tools 的 pending 轮询取消细节——这里用本 crate 的
//! CancelSignal。

use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use url::Url;

use crate::oauth::agents::{Started, code_from_query};
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::loopback::{CALLBACK_TIMEOUT, LoopbackServer, QueryMap};
use crate::oauth::session::{CancelSignal, LoginFlow, LoginStart, PendingLogin, StartOptions};
use crate::oauth::token::{self, FormGrant, GrantKind};
use crate::oauth::{Identity, LoginOutcome, TokenSet, pkce, rt};

pub(crate) const AGENT_ID: &str = "kiro";

pub(crate) const PORTAL_ORIGIN: &str = "https://app.kiro.dev/signin";
pub(crate) const PORTAL_TOKEN_URL: &str =
    "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token";
pub(crate) const PORTAL_REFRESH_URL: &str =
    "https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken";
pub(crate) const DEFAULT_REGION: &str = "us-east-1";
pub(crate) const BUILDER_ID_START_URL: &str = "https://view.awsapps.com/start";

/// 门户回调路径。Kiro 自己用其中之一；粘贴任一个都能解析出 query。
const CALLBACK_PATH: &str = "/oauth/callback";

/// IdC 设备码要求的五条 CodeWhisperer scope。
const IDC_SCOPES: [&str; 5] = [
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
    "codewhisperer:transformations",
    "codewhisperer:taskassist",
];

const DEFAULT_IDC_INTERVAL: u64 = 5;
const DEFAULT_IDC_EXPIRES: u64 = 600;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// 提前 5 分钟刷新：AWS 令牌按小时计，且刷新会真的换掉 refreshToken。
pub(crate) const REFRESH_SKEW_SECONDS: i64 = 300;

/// 这一家的 provider_state：IdC 注册信息 + 区域 + 门户 issuer + 用量画像。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct KiroState {
    pub(crate) client_id: Option<String>,
    pub(crate) client_secret: Option<String>,
    pub(crate) region: Option<String>,
    pub(crate) start_url: Option<String>,
    pub(crate) profile_arn: Option<String>,
}

impl KiroState {
    /// 从 JSON 读；格式不对当作空（旧记录里可能是别的形状）。
    pub(crate) fn parse(value: Option<&Value>) -> Self {
        let Some(map) = value.and_then(Value::as_object) else {
            return Self::default();
        };
        Self {
            client_id: owned(map, &["clientId", "client_id"]),
            client_secret: owned(map, &["clientSecret", "client_secret"]),
            region: owned(map, &["region", "idcRegion", "idc_region"]),
            start_url: owned(map, &["startUrl", "start_url", "issuerUrl", "issuer_url"]),
            profile_arn: owned(map, &["profileArn", "profile_arn"]),
        }
    }

    /// 写回 provider_state；全空时给 None，不存空对象。
    pub(crate) fn to_value(&self) -> Option<Value> {
        let mut map = Map::new();
        for (key, value) in [
            ("clientId", self.client_id.as_deref()),
            ("clientSecret", self.client_secret.as_deref()),
            ("region", self.region.as_deref()),
            ("startUrl", self.start_url.as_deref()),
            ("profileArn", self.profile_arn.as_deref()),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                map.insert(key.to_string(), Value::String(value.to_string()));
            }
        }
        (!map.is_empty()).then(|| Value::Object(map))
    }

    /// 有没有可用的 IdC 注册：有就走 IdC 刷新，没有就走门户。
    pub(crate) fn has_idc_client(&self) -> bool {
        non_empty(&self.client_id) && non_empty(&self.client_secret)
    }

    /// 这次刷新该用哪个区域。
    pub(crate) fn region_or_default(&self) -> String {
        resolve_region(self.region.as_deref(), self.profile_arn.as_deref())
    }
}

fn owned(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn non_empty(value: &Option<String>) -> bool {
    value
        .as_deref()
        .map(str::trim)
        .is_some_and(|text| !text.is_empty())
}

// ---------------------------------------------------------------------------
// 登录
// ---------------------------------------------------------------------------

/// 这一家开哪条腿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoginLeg {
    Portal,
    Idc { region: Option<String> },
}

/// 缺省 / portal / social 走门户；idc / aws-idc / 真实 region 走设备码。
pub(crate) fn login_leg(region: Option<&str>) -> LoginLeg {
    let Some(raw) = region.map(str::trim).filter(|value| !value.is_empty()) else {
        return LoginLeg::Portal;
    };
    match raw.to_ascii_lowercase().as_str() {
        "idc" | "aws-idc" | "awsidc" => LoginLeg::Idc { region: None },
        other if is_aws_region(other) => LoginLeg::Idc {
            region: Some(other.to_string()),
        },
        _ => LoginLeg::Portal,
    }
}

/// 启动一次 Kiro 登录。
pub(crate) fn start(login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    match login_leg(opts.region.as_deref()) {
        LoginLeg::Portal => start_portal(login_id, opts),
        LoginLeg::Idc { region } => start_idc(
            login_id,
            region.as_deref().unwrap_or(DEFAULT_REGION),
        ),
    }
}

// --- 门户 PKCE -------------------------------------------------------------

fn start_portal(login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    let state = uuid::Uuid::new_v4().to_string();
    let pkce = pkce::generate();
    let server = LoopbackServer::bind_with(&ports(opts), CALLBACK_PATH)?;
    let redirect_uri = server.redirect_uri();
    let auth_url = portal_auth_url(&redirect_uri, &state, &pkce.challenge);
    let (callback_tx, callback_rx) = mpsc::channel();
    let (pending, channel) = PendingLogin::pair(Some(callback_tx));
    let verifier = pkce.verifier.clone();
    std::thread::spawn(move || {
        let outcome = receive_portal_code(
            server,
            callback_rx,
            &state,
            &redirect_uri,
            &verifier,
            &channel.cancel,
        );
        let _ = channel.results.send(outcome);
    });
    Ok(Started {
        start: LoginStart {
            login_id: login_id.to_string(),
            flow: LoginFlow::LocalCallback,
            auth_url,
            user_code: None,
            verification_uri: None,
            interval_secs: None,
            expires_at: Some(crate::oauth::now_secs() + CALLBACK_TIMEOUT.as_secs() as i64),
        },
        pending,
    })
}

/// 门户授权页。纯函数，测试据此核对参数（PKCE + redirect_from=KiroIDE）。
pub(crate) fn portal_auth_url(redirect_uri: &str, state: &str, challenge: &str) -> String {
    let mut url = Url::parse(PORTAL_ORIGIN).expect("static portal endpoint parses");
    url.query_pairs_mut()
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("redirect_from", "KiroIDE");
    url.to_string()
}

fn receive_portal_code(
    server: LoopbackServer,
    callbacks: mpsc::Receiver<QueryMap>,
    state: &str,
    redirect_uri: &str,
    code_verifier: &str,
    cancel: &CancelSignal,
) -> OAuthResult<LoginOutcome> {
    let query = server.wait_for_query(&callbacks, CALLBACK_TIMEOUT, || cancel.is_cancelled())?;
    let code = code_from_query(&query, state)?;
    exchange_portal_code(&code, code_verifier, redirect_uri)
}

fn exchange_portal_code(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> OAuthResult<LoginOutcome> {
    let body = serde_json::json!({
        "code": code,
        "code_verifier": code_verifier,
        "redirect_uri": redirect_uri,
    });
    let tokens = post_json_tokens(PORTAL_TOKEN_URL, &body, "Kiro token")?;
    outcome_from_tokens(tokens, None)
}

/// 门户令牌的响应只需要三个字段；身份从令牌 claim 里读。
fn outcome_from_tokens(tokens: TokenSet, state: Option<KiroState>) -> OAuthResult<LoginOutcome> {
    let identity = identity_from_tokens(&tokens);
    Ok(LoginOutcome {
        agent: AGENT_ID.to_string(),
        tokens,
        identity,
        provider_state: state.and_then(|state| state.to_value()),
    })
}

// --- IdC 设备码 ------------------------------------------------------------

fn start_idc(login_id: &str, region: &str) -> OAuthResult<Started> {
    let region = normalize_region(Some(region))?;
    let base = oidc_base(&region);
    let registered = post_json(
        &format!("{base}/client/register"),
        &serde_json::json!({
            "clientName": "SkillStar Kiro",
            "clientType": "public",
            "scopes": IDC_SCOPES,
            "grantTypes": ["urn:ietf:params:oauth:grant-type:device_code", "refresh_token"],
            "issuerUrl": BUILDER_ID_START_URL,
        }),
        "Kiro IDC register",
    )?;
    let client_id = text_field(&registered, &["clientId", "client_id"])
        .ok_or_else(|| OAuthError::Rejected("AWS OIDC 注册响应缺少 clientId".to_string()))?;
    let client_secret = text_field(&registered, &["clientSecret", "client_secret"])
        .ok_or_else(|| OAuthError::Rejected("AWS OIDC 注册响应缺少 clientSecret".to_string()))?;
    let device = post_json(
        &format!("{base}/device_authorization"),
        &serde_json::json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "startUrl": BUILDER_ID_START_URL,
        }),
        "Kiro IDC device",
    )?;
    let session = DeviceSession::parse(&device)?;
    let state = KiroState {
        client_id: Some(client_id.clone()),
        client_secret: Some(client_secret.clone()),
        region: Some(region.clone()),
        start_url: Some(BUILDER_ID_START_URL.to_string()),
        profile_arn: None,
    };
    let (pending, channel) = PendingLogin::pair(None);
    let interval_secs = session.interval_secs;
    let expires_in = session.expires_in;
    let user_code = session.user_code.clone();
    let verification_uri = session.verification_uri.clone();
    std::thread::spawn(move || {
        let outcome = poll_idc_tokens(&base, &session, &channel.cancel)
            .and_then(|tokens| outcome_from_tokens(tokens, Some(state)));
        let _ = channel.results.send(outcome);
    });
    Ok(Started {
        start: LoginStart {
            login_id: login_id.to_string(),
            flow: LoginFlow::RemotePoll,
            auth_url: verification_uri.clone(),
            user_code: Some(user_code),
            verification_uri: Some(verification_uri),
            interval_secs: Some(interval_secs),
            expires_at: Some(crate::oauth::now_secs() + expires_in as i64),
        },
        pending,
    })
}

/// 一次设备授权。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceSession {
    pub(crate) device_code: String,
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
    pub(crate) interval_secs: u64,
    pub(crate) expires_in: u64,
}

impl DeviceSession {
    fn parse(device: &Value) -> OAuthResult<Self> {
        let device_code = text_field(device, &["deviceCode", "device_code"])
            .ok_or_else(|| OAuthError::Rejected("AWS 设备授权响应缺少 device_code".to_string()))?;
        let user_code = text_field(device, &["userCode", "user_code"])
            .ok_or_else(|| OAuthError::Rejected("AWS 设备授权响应缺少 user_code".to_string()))?;
        let verification_uri =
            text_field(device, &["verificationUriComplete", "verification_uri_complete"])
                .or_else(|| text_field(device, &["verificationUri", "verification_uri"]))
                .filter(|uri| uri.starts_with("https://"))
                .ok_or_else(|| {
                    OAuthError::Rejected("AWS 设备授权响应缺少 https 验证地址".to_string())
                })?;
        Ok(Self {
            device_code,
            user_code,
            verification_uri,
            interval_secs: json_u64(device, &["interval"])
                .filter(|seconds| *seconds > 0)
                .unwrap_or(DEFAULT_IDC_INTERVAL),
            expires_in: json_u64(device, &["expiresIn", "expires_in"])
                .filter(|seconds| *seconds > 0)
                .unwrap_or(DEFAULT_IDC_EXPIRES),
        })
    }
}

/// 一次 IdC 轮询的回答。
#[derive(Debug)]
pub(crate) enum IdcPoll {
    Pending { interval: u64 },
    Tokens(TokenSet),
}

fn poll_idc_tokens(
    base: &str,
    session: &DeviceSession,
    cancel: &CancelSignal,
) -> OAuthResult<TokenSet> {
    let deadline = Instant::now() + Duration::from_secs(session.expires_in);
    let mut interval = session.interval_secs;
    loop {
        if wait_interval(interval, cancel) {
            return Err(OAuthError::Rejected("登录已取消".to_string()));
        }
        if Instant::now() >= deadline {
            return Err(OAuthError::Rejected(
                "IAM Identity Center 设备授权已过期，请重新发起登录".to_string(),
            ));
        }
        let (status, body) = post_form(
            &format!("{base}/token"),
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", session.device_code.as_str()),
            ],
        )?;
        match interpret_idc_poll(status, &body, interval)? {
            IdcPoll::Pending { interval: next } => interval = next,
            IdcPoll::Tokens(tokens) => return Ok(tokens),
        }
    }
}

/// IdC 轮询：等待/放慢继续，过期/拒绝结束。403 不是"凭据失效"，是拒绝。
pub(crate) fn interpret_idc_poll(status: u16, body: &str, interval: u64) -> OAuthResult<IdcPoll> {
    if (200..300).contains(&status) {
        let tokens = tokens_from_body(&parse_json(body, "Kiro IDC token")?)?;
        return Ok(IdcPoll::Tokens(tokens));
    }
    match oauth_error_code(body).as_deref() {
        Some("authorization_pending") => Ok(IdcPoll::Pending { interval }),
        Some("slow_down") => Ok(IdcPoll::Pending {
            interval: interval.saturating_add(5),
        }),
        Some("expired_token") => Err(OAuthError::Rejected(
            "IAM Identity Center 设备授权已过期，请重新发起登录".to_string(),
        )),
        Some("access_denied") => Err(OAuthError::Rejected(
            "IAM Identity Center 登录被用户拒绝".to_string(),
        )),
        _ => Err(token::classify_status(status, body)),
    }
}

// ---------------------------------------------------------------------------
// 刷新
// ---------------------------------------------------------------------------

/// 刷新 Kiro 凭据：有 IdC 注册走 IdC，否则走门户。
pub(crate) fn refresh(
    tokens: &TokenSet,
    provider_state: Option<&Value>,
) -> OAuthResult<TokenSet> {
    let refresh_token = crate::oauth::agents::refresh_token_of(AGENT_ID, tokens)?;
    let state = KiroState::parse(provider_state);
    let refreshed = if state.has_idc_client() {
        let region = state.region_or_default();
        let grant = FormGrant {
            url: format!("{}/token", oidc_base(&region)),
            client_id: state.client_id.clone().unwrap_or_default(),
            client_secret: state.client_secret.clone(),
            refresh_token: Some(refresh_token),
            ..FormGrant::default()
        };
        token::post_token(&grant, GrantKind::RefreshToken)?
    } else {
        post_json_tokens(
            PORTAL_REFRESH_URL,
            &serde_json::json!({ "refreshToken": refresh_token }),
            "Kiro refresh",
        )?
    };
    Ok(token::carry_over(tokens, refreshed))
}

// ---------------------------------------------------------------------------
// 区域与端点
// ---------------------------------------------------------------------------

/// us-east-1、eu-central-1、us-gov-west-1：两字母地理前缀 + 至少一段，最后
/// 一段必须是数字。单独一个连字符不够——builder-id 是登录腿，不是区域。
pub(crate) fn is_aws_region(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.len() > 32 {
        return false;
    }
    let mut parts = value.split('-');
    let Some(geo) = parts.next() else {
        return false;
    };
    if geo.len() != 2 || !geo.chars().all(|ch| ch.is_ascii_lowercase()) {
        return false;
    }
    let mut last = "";
    let mut named = false;
    for part in parts {
        if part.is_empty()
            || !part
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        {
            return false;
        }
        last = part;
        named = true;
    }
    named && last.chars().all(|ch| ch.is_ascii_digit())
}

/// 回调区域 → profile ARN 的区域 → 默认 us-east-1。
pub(crate) fn resolve_region(callback: Option<&str>, profile_arn: Option<&str>) -> String {
    if let Some(region) = callback.map(str::trim).filter(|value| is_aws_region(value)) {
        return region.to_string();
    }
    if let Some(region) = profile_arn.and_then(profile_arn_region)
        && is_aws_region(&region)
    {
        return region;
    }
    DEFAULT_REGION.to_string()
}

/// arn:aws:codewhisperer:us-east-1:123456789012:profile/X 里的区域。
pub(crate) fn profile_arn_region(profile_arn: &str) -> Option<String> {
    let mut segments = profile_arn.split(':');
    if !segments.next()?.trim().eq_ignore_ascii_case("arn") {
        return None;
    }
    let _partition = segments.next()?;
    let _service = segments.next()?;
    let region = segments.next()?.trim();
    (!region.is_empty()).then(|| region.to_string())
}

/// CodeWhisperer 运行时主机；未知区域落回 us-east-1。
pub(crate) fn runtime_endpoint(region: &str) -> String {
    match region.trim() {
        "eu-central-1" => "https://q.eu-central-1.amazonaws.com".to_string(),
        "us-gov-east-1" => "https://q-fips.us-gov-east-1.amazonaws.com".to_string(),
        "us-gov-west-1" => "https://q-fips.us-gov-west-1.amazonaws.com".to_string(),
        "us-iso-east-1" => "https://q.us-iso-east-1.c2s.ic.gov".to_string(),
        "us-isob-east-1" => "https://q.us-isob-east-1.sc2s.sgov.gov".to_string(),
        "us-isof-south-1" => "https://q.us-isof-south-1.csp.hci.ic.gov".to_string(),
        "us-isof-east-1" => "https://q.us-isof-east-1.csp.hci.ic.gov".to_string(),
        _ => "https://q.us-east-1.amazonaws.com".to_string(),
    }
}

pub(crate) fn oidc_base(region: &str) -> String {
    format!("https://oidc.{region}.amazonaws.com")
}

fn normalize_region(region: Option<&str>) -> OAuthResult<String> {
    match region.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(DEFAULT_REGION.to_string()),
        Some(value) if is_aws_region(value) => Ok(value.to_string()),
        Some(_) => Err(OAuthError::Rejected("AWS Region 格式无效".to_string())),
    }
}

// ---------------------------------------------------------------------------
// 令牌与身份
// ---------------------------------------------------------------------------

/// Kiro 的令牌响应：camelCase 为主，也接受 snake_case 与 data 包装。
fn tokens_from_body(value: &Value) -> OAuthResult<TokenSet> {
    let value = value
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(value);
    let access_token = text_field(value, &["accessToken", "access_token"])
        .ok_or_else(|| OAuthError::Rejected("Kiro 令牌响应缺少 accessToken".to_string()))?;
    let expires_at = json_u64(value, &["expiresIn", "expires_in"])
        .filter(|seconds| *seconds > 0)
        .map(|seconds| crate::oauth::now_secs() + seconds as i64);
    Ok(TokenSet {
        access_token,
        refresh_token: text_field(value, &["refreshToken", "refresh_token"]),
        id_token: text_field(value, &["idToken", "id_token"]),
        expires_at,
        token_type: text_field(value, &["tokenType", "token_type"])
            .unwrap_or_else(|| "Bearer".to_string()),
    })
}

/// Kiro 没有 userinfo：身份来自访问令牌的 JWT claim（email / sub / userId）。
pub(crate) fn identity_from_tokens(tokens: &TokenSet) -> Identity {
    let access = tokens.access_token.as_str();
    let id_token = tokens.id_token.as_deref();
    let email = id_token
        .and_then(crate::identity::jwt_email)
        .or_else(|| crate::identity::jwt_email(access));
    let account_id = id_token
        .and_then(|token| crate::identity::jwt_claim(token, "sub"))
        .or_else(|| crate::identity::jwt_claim(access, "sub"))
        .or_else(|| crate::identity::jwt_claim(access, "userId"));
    Identity {
        display_name: email.clone(),
        email,
        plan: None,
        account_id,
    }
}

fn parse_json(body: &str, label: &str) -> OAuthResult<Value> {
    serde_json::from_str(body)
        .map_err(|error| OAuthError::Rejected(format!("{label} 响应不是合法 JSON: {error}")))
}

fn post_json(url: &str, body: &Value, label: &str) -> OAuthResult<Value> {
    let url = url.to_string();
    let body = body.clone();
    let (status, text) = rt::block_on(async move {
        let client = ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))?;
        let response = client
            .post(&url)
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| OAuthError::Transient(format!("{label} 请求失败: {error}")))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|error| OAuthError::Transient(format!("{label} 响应读取失败: {error}")))?;
        Ok::<_, OAuthError>((status, text))
    })??;
    if !(200..300).contains(&status) {
        return Err(token::classify_status(status, &text));
    }
    parse_json(&text, label)
}

fn post_json_tokens(url: &str, body: &Value, label: &str) -> OAuthResult<TokenSet> {
    let value = post_json(url, body, label)?;
    tokens_from_body(&value)
}

fn post_form(url: &str, pairs: &[(&str, &str)]) -> OAuthResult<(u16, String)> {
    let url = url.to_string();
    let body: Vec<(String, String)> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    rt::block_on(async move {
        let client = ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))?;
        let response = client
            .post(&url)
            .header("Accept", "application/json")
            .form(&body)
            .send()
            .await
            .map_err(|error| OAuthError::Transient(format!("Kiro 请求失败: {error}")))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|error| OAuthError::Transient(format!("Kiro 响应读取失败: {error}")))?;
        Ok::<_, OAuthError>((status, text))
    })?
}

fn wait_interval(seconds: u64, cancel: &CancelSignal) -> bool {
    for _ in 0..seconds.max(1).saturating_mul(5) {
        if cancel.is_cancelled() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    cancel.is_cancelled()
}

fn text_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    })
}

fn json_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        let value = value.get(*key)?;
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
    })
}

fn oauth_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(|code| code.trim().to_ascii_lowercase())
}

/// 门户回调端口：Kiro 用随机端口，显式请求优先。
fn ports(opts: &StartOptions) -> Vec<u16> {
    opts.prefer_port.into_iter().collect()
}

#[cfg(test)]
#[path = "kiro_tests.rs"]
mod kiro_tests;
