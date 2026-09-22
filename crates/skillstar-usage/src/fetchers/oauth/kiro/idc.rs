//! AWS IAM Identity Center device flow.
//!
//! `oidc.{region}.amazonaws.com/client/register` → device authorization →
//! poll `/token` with the device-code grant. `authorization_pending` keeps
//! the interval; `slow_down` adds five seconds. `clientId` and `clientSecret`
//! are returned to the caller so they can be stored — without them refresh
//! has no IDC leg.
//!
//! The on-disk registration name is cockpit `idc_client_registration_path`:
//! a 40-char lowercase hex `clientIdHash` plus `.json` under the AWS SSO
//! cache. Cockpit fills that hash with SHA-1 of the start URL
//! (`compute_idc_client_id_hash`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::http::{self, TokenGrant};
use super::{BUILDER_ID_START_URL, CATALOG_ID, DEFAULT_REGION, KiroState, is_aws_region};
use crate::{UsageError, UsageResult};

const HASH_LEN: usize = 40;
const IDC_SCOPES: &[&str] = &[
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
    "codewhisperer:transformations",
    "codewhisperer:taskassist",
];

#[derive(Debug, Clone)]
pub(super) struct DeviceSession {
    pub region: String,
    pub start_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval_seconds: u64,
    pub deadline: Instant,
}

#[derive(Debug)]
pub(super) enum DevicePoll {
    Pending { interval: u64 },
    Tokens(TokenGrant),
}

pub(super) async fn start_idc(
    region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let region = normalize_region(region)?;
    let client = crate::fetchers::http_client()?;
    let base = oidc_base(&region);
    let session = begin_device_flow(&client, &base, &region, BUILDER_ID_START_URL).await?;
    let interval = u32::try_from(session.interval_seconds).ok();
    let pending_id = crate::oauth::pending_state::register_with_flow(
        CATALOG_ID,
        Some(&region),
        session.verification_uri.clone(),
        super::super::OAuthFlow::RemotePoll,
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );
    let pid = pending_id.clone();
    let user_code = session.user_code.clone();
    let verification_uri = session.verification_uri.clone();
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_idc(session, pid.clone(), target).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });
    Ok(super::super::OAuthStartInfo::device(
        pending_id,
        verification_uri,
        user_code,
        interval,
    ))
}

pub(super) fn oidc_base(region: &str) -> String {
    format!("https://oidc.{region}.amazonaws.com")
}

pub(super) async fn begin_device_flow(
    client: &reqwest::Client,
    oidc_base: &str,
    region: &str,
    start_url: &str,
) -> UsageResult<DeviceSession> {
    let region = normalize_region(Some(region))?;
    let register_url = format!("{}/client/register", oidc_base.trim_end_matches('/'));
    let registered = http::post_json(
        client,
        &register_url,
        &json!({
            "clientName": "SkillStar Kiro",
            "clientType": "public",
            "scopes": IDC_SCOPES,
            "grantTypes": ["urn:ietf:params:oauth:grant-type:device_code", "refresh_token"],
            "issuerUrl": start_url,
        }),
        "Kiro IDC register",
    )
    .await?;
    let (client_id, client_secret) = client_credentials(&registered)?;
    let device_url = format!("{}/device_authorization", oidc_base.trim_end_matches('/'));
    let device = http::post_json(
        client,
        &device_url,
        &json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "startUrl": start_url,
        }),
        "Kiro IDC device",
    )
    .await?;
    let device_code = super::string_field(Some(&device), &["deviceCode", "device_code"])
        .ok_or_else(|| UsageError::Fetcher("AWS 设备授权响应缺少 device_code".into()))?;
    let user_code = super::string_field(Some(&device), &["userCode", "user_code"])
        .ok_or_else(|| UsageError::Fetcher("AWS 设备授权响应缺少 user_code".into()))?;
    let verification_uri = super::string_field(
        Some(&device),
        &["verificationUriComplete", "verification_uri_complete"],
    )
    .or_else(|| super::string_field(Some(&device), &["verificationUri", "verification_uri"]))
    .ok_or_else(|| UsageError::Fetcher("AWS 设备授权响应缺少 verification_uri".into()))?;
    let expires_in = json_u64(&device, &["expiresIn", "expires_in"])
        .unwrap_or(600)
        .max(1);
    let interval_seconds = json_u64(&device, &["interval"]).unwrap_or(5);
    Ok(DeviceSession {
        region,
        start_url: start_url.to_string(),
        client_id,
        client_secret,
        device_code,
        user_code,
        verification_uri,
        interval_seconds,
        deadline: Instant::now() + Duration::from_secs(expires_in),
    })
}

pub(super) async fn poll_device_token(
    client: &reqwest::Client,
    token_url: &str,
    session: &DeviceSession,
    sleep: bool,
    cancelled: impl Fn() -> bool,
) -> UsageResult<TokenGrant> {
    let mut interval = session.interval_seconds;
    loop {
        if cancelled() {
            return Err(UsageError::Other("用户取消登录".into()));
        }
        if Instant::now() >= session.deadline {
            return Err(UsageError::Other(
                "IAM Identity Center 设备授权已过期，请重新发起登录".into(),
            ));
        }
        if sleep && interval > 0 {
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
        if cancelled() {
            return Err(UsageError::Other("用户取消登录".into()));
        }
        let (status, body) = form_token_raw(
            client,
            token_url,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", session.device_code.as_str()),
                ("client_id", session.client_id.as_str()),
                ("client_secret", session.client_secret.as_str()),
            ],
        )
        .await?;
        match interpret_device_poll(status, &body, interval)? {
            DevicePoll::Pending { interval: next } => interval = next,
            DevicePoll::Tokens(grant) => return Ok(grant),
        }
    }
}

pub(super) async fn refresh_idc_token(
    client: &reqwest::Client,
    region: &str,
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> UsageResult<TokenGrant> {
    let url = format!("{}/token", oidc_base(region));
    http::post_form_token(
        client,
        &url,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ],
        "Kiro IDC refresh",
    )
    .await
}

/// Copied from cockpit `kiro_account.rs` `idc_client_registration_path`.
/// The hash must already be 40 lowercase hex characters; this does not scan
/// the cache directory.
pub(super) fn idc_client_registration_path(
    cache_dir: &Path,
    client_id_hash: &str,
) -> UsageResult<PathBuf> {
    let hash = client_id_hash;
    let is_lower_hex = hash
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if hash.len() != HASH_LEN || !is_lower_hex {
        return Err(UsageError::Other(
            "Kiro 本地授权文件中的 clientIdHash 格式无效".into(),
        ));
    }
    Ok(cache_dir.join(format!("{hash}.json")))
}

/// SHA-1 hex of the start URL. Cockpit `compute_idc_client_id_hash`.
pub(super) fn compute_idc_client_id_hash(start_url: &str) -> String {
    let digest = Sha1::digest(start_url.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn interpret_device_poll(
    status: u16,
    body: &str,
    interval: u64,
) -> UsageResult<DevicePoll> {
    if (200..300).contains(&status) {
        let grant = http::parse_token_grant(body, "Kiro IDC token")?;
        if grant.access_token.trim().is_empty() {
            return Err(UsageError::AuthRequired);
        }
        return Ok(DevicePoll::Tokens(grant));
    }
    match http::oauth_error_code(body).as_deref() {
        Some("authorization_pending") => return Ok(DevicePoll::Pending { interval }),
        Some("slow_down") => {
            return Ok(DevicePoll::Pending {
                interval: interval.saturating_add(5),
            });
        }
        Some("expired_token") => {
            return Err(UsageError::Other(
                "IAM Identity Center 设备授权已过期，请重新发起登录".into(),
            ));
        }
        Some("access_denied") => {
            return Err(UsageError::Other(
                "IAM Identity Center 登录被用户拒绝".into(),
            ));
        }
        _ => {}
    }
    http::classify_status(status, body, "Kiro IDC token")?;
    Err(UsageError::Fetcher(
        "Kiro IDC token 没有 access_token".into(),
    ))
}

pub(super) fn normalize_region(region: Option<&str>) -> UsageResult<String> {
    match region.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(DEFAULT_REGION.to_string()),
        Some(value) if is_aws_region(value) => Ok(value.to_string()),
        Some(_) => Err(super::invalid_region()),
    }
}

async fn drive_idc(
    session: DeviceSession,
    pending_id: String,
    target_subscription_id: Option<String>,
) -> UsageResult<crate::subscription::Subscription> {
    let client = crate::fetchers::http_client()?;
    let token_url = format!("{}/token", oidc_base(&session.region));
    let grant = poll_device_token(&client, &token_url, &session, true, || {
        crate::oauth::pending_state::flow(&pending_id).is_none()
    })
    .await?;
    let mut state = KiroState {
        client_id: Some(session.client_id.clone()),
        client_secret: Some(session.client_secret.clone()),
        region: Some(session.region.clone()),
        start_url: Some(session.start_url.clone()),
        profile_arn: None,
    };
    state.overlay_token(&grant.raw);
    super::import::finish_login(grant, state, None, target_subscription_id.as_deref()).await
}

async fn form_token_raw(
    client: &reqwest::Client,
    url: &str,
    form: &[(&str, &str)],
) -> UsageResult<(u16, String)> {
    let response = client
        .post(url)
        .form(form)
        .send()
        .await
        .map_err(|err| UsageError::transport("Kiro IDC token", err))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
}

fn client_credentials(value: &Value) -> UsageResult<(String, String)> {
    let client_id = super::string_field(Some(value), &["clientId", "client_id"])
        .ok_or_else(|| UsageError::Fetcher("AWS OIDC 注册响应缺少 clientId".into()))?;
    let client_secret = super::string_field(Some(value), &["clientSecret", "client_secret"])
        .ok_or_else(|| UsageError::Fetcher("AWS OIDC 注册响应缺少 clientSecret".into()))?;
    Ok((client_id, client_secret))
}

fn json_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    let map = value.as_object()?;
    for key in keys {
        let Some(item) = map.get(*key) else {
            continue;
        };
        if let Some(number) = item.as_u64() {
            return Some(number);
        }
        if let Some(number) = item.as_i64().filter(|n| *n >= 0) {
            return Some(number as u64);
        }
        if let Some(text) = item.as_str()
            && let Ok(number) = text.trim().parse::<u64>()
        {
            return Some(number);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiro_idc_registration_path_copies_the_cockpit_hash_gate() {
        let cache = Path::new("/tmp/aws-sso-cache");
        let hash = "eb04fe0de39241e4fa17bec175f7540138ad1bd8";
        let path = idc_client_registration_path(cache, hash).unwrap();
        assert_eq!(path, cache.join(format!("{hash}.json")));
        assert!(idc_client_registration_path(cache, "ABC").is_err());
        assert!(idc_client_registration_path(cache, &"g".repeat(40)).is_err());
        assert!(idc_client_registration_path(cache, &"ab".repeat(19)).is_err());
        assert_eq!(
            compute_idc_client_id_hash(BUILDER_ID_START_URL),
            "cc18142e2bfa693e309f59d910dcef90c3c47767"
        );
    }

    #[test]
    fn kiro_device_poll_pending_and_slow_down_are_not_failures() {
        let pending =
            interpret_device_poll(400, r#"{"error":"authorization_pending"}"#, 5).unwrap();
        assert!(matches!(pending, DevicePoll::Pending { interval: 5 }));
        let slowed = interpret_device_poll(400, r#"{"error":"slow_down"}"#, 5).unwrap();
        assert!(matches!(slowed, DevicePoll::Pending { interval: 10 }));

        let denied = interpret_device_poll(400, r#"{"error":"access_denied"}"#, 1).unwrap_err();
        assert!(denied.to_string().contains("拒绝"), "{denied}");

        let revoked = interpret_device_poll(400, r#"{"error":"invalid_grant"}"#, 1).unwrap_err();
        assert!(matches!(revoked, UsageError::AuthRequired), "{revoked:?}");

        let forbidden = interpret_device_poll(403, r#"{"message":"no"}"#, 1).unwrap_err();
        assert!(matches!(forbidden, UsageError::Fetcher(_)), "{forbidden:?}");

        let down = interpret_device_poll(502, "upstream", 1).unwrap_err();
        assert!(matches!(down, UsageError::Transient(_)), "{down:?}");
    }

    #[tokio::test]
    async fn kiro_device_flow_register_device_then_poll() {
        let server = super::super::http::scripted::ScriptedHttp::start(vec![
            (
                200,
                r#"{"clientId":"cid","clientSecret":"csec"}"#.into(),
            ),
            (
                200,
                r#"{"deviceCode":"dev","userCode":"ABCD-EFGH","verificationUri":"https://device.example/verify","verificationUriComplete":"https://device.example/verify?user_code=ABCD-EFGH","expiresIn":30,"interval":0}"#.into(),
            ),
            (400, r#"{"error":"authorization_pending"}"#.into()),
            (400, r#"{"error":"slow_down"}"#.into()),
            (
                200,
                r#"{"accessToken":"at-1","refreshToken":"rt-1","expiresIn":3600}"#.into(),
            ),
        ]);
        let client = reqwest::Client::new();
        let session = begin_device_flow(&client, &server.base, "us-east-1", BUILDER_ID_START_URL)
            .await
            .expect("begin");
        assert_eq!(session.client_id, "cid");
        assert_eq!(session.client_secret, "csec");
        assert_eq!(session.user_code, "ABCD-EFGH");
        assert_eq!(
            session.verification_uri,
            "https://device.example/verify?user_code=ABCD-EFGH"
        );
        assert_eq!(session.interval_seconds, 0);
        let token_url = format!("{}/token", server.base);
        let grant = poll_device_token(&client, &token_url, &session, false, || false)
            .await
            .expect("poll");
        assert_eq!(grant.access_token, "at-1");
        assert_eq!(grant.refresh_token.as_deref(), Some("rt-1"));
        let seen = server.seen();
        assert_eq!(
            seen,
            vec![
                "POST /client/register".to_string(),
                "POST /device_authorization".to_string(),
                "POST /token".to_string(),
                "POST /token".to_string(),
                "POST /token".to_string(),
            ]
        );

        let state = KiroState {
            client_id: Some(session.client_id),
            client_secret: Some(session.client_secret),
            region: Some(session.region),
            start_url: Some(session.start_url),
            profile_arn: None,
        };
        let imported =
            super::super::import::imported_from_grant(&grant, state, None, Some("user-1".into()));
        let row = super::super::import::oauth_row_from_imported(imported).unwrap();
        assert!(row.platform_token_encrypted.is_none());
        let plain = crate::crypto::decrypt(row.provider_state_encrypted.as_deref().unwrap());
        let json: Value = serde_json::from_str(&plain).unwrap();
        assert_eq!(json["clientId"], "cid");
        assert_eq!(json["clientSecret"], "csec");
        assert_eq!(json["region"], "us-east-1");
        assert_eq!(json["startUrl"], BUILDER_ID_START_URL);
        assert!(json.get("idc").is_none());
    }
}
