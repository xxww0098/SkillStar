//! Pasted refresh token / JSON, and the on-disk Kiro login.
//!
//! Local import reads `~/.aws/sso/cache/kiro-auth-token.json` plus the IdC
//! registration file named by `clientIdHash`. `profile.json` and
//! `state.vscdb` fill identity when those files exist. Paths go through
//! `tool_paths`, so `SKILLSTAR_TOOL_SYNC_HOME` is the only home.

use serde_json::Value;

use super::http::TokenGrant;
use super::{CATALOG_ID, DEFAULT_REGION, KiroState, resolve_region, string_field};
use crate::catalog::AuthMode;
use crate::fetchers::oauth::common::{self, reauth_target};
use crate::oauth::token_refresh;
use crate::subscription::{BillingCycle, Subscription};
use crate::token_import::ImportedToken;
use crate::{UsageError, UsageResult};

const MIN_SECRET_LEN: usize = 20;
const USAGE_DB_KEY: &str = "kiro.kiroAgent";

pub(crate) fn import_from_token(payload: &str) -> UsageResult<ImportedToken> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Err(UsageError::Other("Kiro 令牌为空".into()));
    }
    if trimmed.starts_with(['{', '[', '"']) {
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|_| UsageError::Other("Kiro 凭据 JSON 无法解析".into()))?;
        return match value {
            Value::Object(_) => imported_from_object(&value),
            Value::String(text) => bare_secret(&text),
            _ => Err(UsageError::Other(
                "Kiro 凭据 JSON 须是对象或令牌字符串".into(),
            )),
        };
    }
    bare_secret(trimmed)
}

pub(crate) fn import_from_local() -> UsageResult<ImportedToken> {
    let path = auth_token_path();
    if !path.exists() {
        return Err(UsageError::Other(
            "未在本机找到 Kiro 登录信息（~/.aws/sso/cache/kiro-auth-token.json）".into(),
        ));
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| UsageError::Other(format!("读取 Kiro 本地授权文件失败: {err}")))?;
    let auth: Value = serde_json::from_str(&raw)
        .map_err(|_| UsageError::Other("Kiro 本地授权文件无法解析".into()))?;
    if !auth.is_object() {
        return Err(UsageError::Other("Kiro 本地授权文件须是 JSON 对象".into()));
    }
    let registration = read_registration(&auth)?;
    let profile = read_optional_json(profile_path().as_deref())?;
    let usage = read_usage_snapshot()?;
    imported_from_sources(
        &auth,
        registration.as_ref(),
        profile.as_ref(),
        usage.as_ref(),
    )
}

pub(super) fn imported_from_grant(
    grant: &TokenGrant,
    mut state: KiroState,
    email: Option<String>,
    user_id: Option<String>,
) -> ImportedToken {
    state.overlay_token(&grant.raw);
    if state
        .region
        .as_deref()
        .is_none_or(|region| !super::is_aws_region(region))
    {
        state.region = Some(resolve_region(None, state.profile_arn.as_deref()));
    }
    let email = email
        .or_else(|| string_field(Some(&grant.raw), &["email", "userEmail"]))
        .or_else(|| email_from_jwt(&grant.access_token));
    let user_id = user_id
        .or_else(|| string_field(Some(&grant.raw), &["userId", "user_id", "sub"]))
        .or_else(|| user_id_from_jwt(&grant.access_token));
    token_row(
        grant.access_token.clone(),
        grant.refresh_token.clone(),
        expires_from_grant(grant),
        email,
        user_id,
        state,
    )
}

pub(crate) fn oauth_row_from_imported(imported: ImportedToken) -> UsageResult<Subscription> {
    let has_refresh = imported
        .refresh_token
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let provider_state = imported
        .provider_state
        .filter(|value| !value.trim().is_empty());
    if imported.access_token.trim().is_empty() && provider_state.is_none() && !has_refresh {
        return Err(UsageError::Other("Kiro 导入没有可用凭据".into()));
    }
    let now = chrono::Utc::now().timestamp();
    Ok(Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: CATALOG_ID.to_string(),
        display_name: imported.display_name,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: imported.currency.unwrap_or_else(|| "USD".to_string()),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: (!imported.access_token.trim().is_empty())
            .then(|| crate::crypto::encrypt(&imported.access_token)),
        refresh_token_encrypted: imported
            .refresh_token
            .filter(|value| !value.trim().is_empty())
            .map(|value| crate::crypto::encrypt(&value)),
        access_token_expires_at: imported.expires_at,
        id_token_encrypted: None,
        oauth_account_id: imported.oauth_account_id,
        oauth_region: imported.oauth_region,
        requires_reauth: false,
        provider_state_encrypted: provider_state.as_deref().map(crate::crypto::encrypt),
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    })
}

pub(super) async fn finish_login(
    grant: TokenGrant,
    state: KiroState,
    email: Option<String>,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let user_id = user_id_from_jwt(&grant.access_token);
    let imported = imported_from_grant(&grant, state, email, user_id);
    let target = target_subscription_id.map(str::to_string);
    crate::refresh_guard::with_catalog_lock(CATALOG_ID, || async {
        let mut sub = oauth_row_from_imported(imported)?;
        if let Some(existing) = reauth_target(CATALOG_ID, target.as_deref()) {
            common::carry_over_user_metadata(&mut sub, &existing, &["Kiro"]);
        }
        if let Ok(usage) = super::quota::fetch_quota(&mut sub).await {
            crate::storage::save_usage_snapshot(usage).ok();
        }
        crate::storage::upsert_subscription(sub)
            .map_err(|err| UsageError::Other(format!("Kiro 订阅保存失败：{err}")))
    })
    .await?
}

fn imported_from_object(value: &Value) -> UsageResult<ImportedToken> {
    let combined = combine_object(value);
    let access = string_field(Some(&combined), &["accessToken", "access_token", "token"]);
    let refresh = string_field(Some(&combined), &["refreshToken", "refresh_token"]);
    if access
        .as_deref()
        .is_none_or(|token| !looks_like_secret(token))
        && refresh
            .as_deref()
            .is_none_or(|token| !looks_like_secret(token))
    {
        return Err(UsageError::Other("Kiro 凭据 JSON 缺少可用令牌".into()));
    }
    let mut state = state_from_json(&combined);
    if let Some(nested) = value
        .get("clientRegistration")
        .or_else(|| value.get("registration"))
    {
        state.overlay_token(nested);
    }
    if state.region.is_none() {
        state.region = Some(resolve_region(None, state.profile_arn.as_deref()));
    }
    let email = string_field(Some(&combined), &["email", "userEmail"])
        .or_else(|| access.as_deref().and_then(email_from_jwt));
    let user_id = string_field(Some(&combined), &["userId", "user_id", "sub", "accountId"])
        .or_else(|| access.as_deref().and_then(user_id_from_jwt));
    Ok(token_row(
        access.unwrap_or_default(),
        refresh,
        expires_from_json(&combined),
        email,
        user_id,
        state,
    ))
}

fn imported_from_sources(
    auth: &Value,
    registration: Option<&Value>,
    profile: Option<&Value>,
    usage: Option<&Value>,
) -> UsageResult<ImportedToken> {
    let access = string_field(
        Some(auth),
        &[
            "accessToken",
            "access_token",
            "token",
            "idToken",
            "id_token",
        ],
    );
    let refresh = string_field(Some(auth), &["refreshToken", "refresh_token"]);
    if access.as_deref().is_none_or(|token| token.is_empty())
        && refresh.as_deref().is_none_or(|token| token.is_empty())
    {
        return Err(UsageError::Other(
            "Kiro 本地授权信息缺少 access token".into(),
        ));
    }
    let mut state = state_from_json(auth);
    if let Some(registration) = registration {
        if state.client_id.is_none() {
            state.client_id = string_field(Some(registration), &["clientId", "client_id"]);
        }
        if state.client_secret.is_none() {
            state.client_secret =
                string_field(Some(registration), &["clientSecret", "client_secret"]);
        }
    }
    if state.profile_arn.is_none() {
        state.profile_arn = string_field(profile, &["arn", "profileArn", "profile_arn"])
            .or_else(|| nested_profile_arn(profile))
            .or_else(|| string_field(usage, &["profileArn", "profile_arn", "arn"]));
    }
    if state
        .region
        .as_deref()
        .is_none_or(|region| !super::is_aws_region(region))
    {
        state.region = Some(resolve_region(
            state.region.as_deref(),
            state.profile_arn.as_deref(),
        ));
    }
    if state.start_url.is_none() {
        state.start_url = string_field(
            Some(auth),
            &["issuerUrl", "issuer_url", "issuer", "startUrl", "start_url"],
        );
    }
    let email = string_field(profile, &["email"])
        .or_else(|| string_field(Some(auth), &["email", "userEmail"]))
        .or_else(|| usage_user_string(usage, &["email"]))
        .or_else(|| access.as_deref().and_then(email_from_jwt));
    let user_id = string_field(profile, &["userId", "user_id", "id", "sub"])
        .or_else(|| string_field(Some(auth), &["userId", "user_id", "sub"]))
        .or_else(|| usage_user_string(usage, &["userId", "user_id", "sub"]))
        .or_else(|| access.as_deref().and_then(user_id_from_jwt));
    Ok(token_row(
        access.unwrap_or_default(),
        refresh,
        expires_from_json(auth),
        email,
        user_id,
        state,
    ))
}

fn bare_secret(raw: &str) -> UsageResult<ImportedToken> {
    let token = raw.trim();
    if !looks_like_secret(token) {
        return Err(UsageError::Other("Kiro 令牌格式无法识别".into()));
    }
    let state = KiroState {
        region: Some(DEFAULT_REGION.to_string()),
        ..KiroState::default()
    };
    Ok(token_row(
        String::new(),
        Some(token.to_string()),
        None,
        None,
        None,
        state,
    ))
}

fn token_row(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    email: Option<String>,
    user_id: Option<String>,
    state: KiroState,
) -> ImportedToken {
    let email = email.filter(|value| common::looks_like_email(value));
    let display_name = email
        .clone()
        .or_else(|| user_id.clone())
        .unwrap_or_else(|| "Kiro".to_string());
    let oauth_region = state
        .region
        .clone()
        .filter(|region| super::is_aws_region(region));
    ImportedToken {
        display_name,
        access_token,
        refresh_token,
        expires_at,
        oauth_account_id: user_id.filter(|value| !value.is_empty()),
        provider_state: state.to_json(),
        currency: None,
        oauth_region,
        id_token: None,
        api_key: None,
    }
}

fn state_from_json(value: &Value) -> KiroState {
    let mut state = KiroState::default();
    state.overlay_token(value);
    state
}

fn combine_object(value: &Value) -> Value {
    let Some(map) = value.as_object() else {
        return value.clone();
    };
    let mut base = map
        .get("kiro_auth_token_raw")
        .or_else(|| map.get("authToken"))
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(map.clone()));
    if let Some(obj) = base.as_object_mut() {
        for (key, item) in map {
            if key == "kiro_auth_token_raw" || key == "authToken" {
                continue;
            }
            obj.entry(key.clone()).or_insert_with(|| item.clone());
        }
    }
    base
}

fn looks_like_secret(token: &str) -> bool {
    token.len() >= MIN_SECRET_LEN
        && !token.contains("://")
        && token.chars().all(|ch| ch.is_ascii_graphic())
}

fn expires_from_grant(grant: &TokenGrant) -> Option<i64> {
    grant
        .expires_in
        .filter(|seconds| *seconds > 0)
        .map(|seconds| chrono::Utc::now().timestamp() + seconds)
        .or_else(|| token_refresh::jwt_exp(&grant.access_token))
}

fn expires_from_json(value: &Value) -> Option<i64> {
    let raw = value.get("expiresAt").or_else(|| value.get("expires_at"))?;
    if let Some(seconds) = raw.as_i64() {
        return normalize_epoch(seconds);
    }
    if let Some(text) = raw.as_str() {
        if let Ok(seconds) = text.trim().parse::<i64>() {
            return normalize_epoch(seconds);
        }
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text.trim()) {
            return Some(parsed.timestamp());
        }
    }
    None
}

fn normalize_epoch(raw: i64) -> Option<i64> {
    if raw <= 0 {
        None
    } else if raw > 10_000_000_000 {
        Some(raw / 1000)
    } else {
        Some(raw)
    }
}

fn email_from_jwt(token: &str) -> Option<String> {
    token_refresh::jwt_string(token, &["email"]).filter(|value| common::looks_like_email(value))
}

fn user_id_from_jwt(token: &str) -> Option<String> {
    token_refresh::jwt_string(token, &["sub"])
        .or_else(|| token_refresh::jwt_string(token, &["user_id"]))
        .filter(|value| !value.is_empty())
}

fn nested_profile_arn(profile: Option<&Value>) -> Option<String> {
    profile
        .and_then(|value| value.get("profile"))
        .and_then(|value| string_field(Some(value), &["arn", "profileArn"]))
}

fn usage_user_string(usage: Option<&Value>, keys: &[&str]) -> Option<String> {
    usage
        .and_then(|value| value.get("userInfo"))
        .and_then(|info| string_field(Some(info), keys))
        .or_else(|| string_field(usage, keys))
}

fn auth_token_path() -> std::path::PathBuf {
    crate::tool_paths::aws_sso_cache_dir().join("kiro-auth-token.json")
}

fn profile_path() -> Option<std::path::PathBuf> {
    crate::tool_paths::kiro_data_dir().map(|root| {
        root.join("User")
            .join("globalStorage")
            .join("kiro.kiroagent")
            .join("profile.json")
    })
}

fn state_db_path() -> Option<std::path::PathBuf> {
    crate::tool_paths::kiro_data_dir()
        .map(|root| root.join("User").join("globalStorage").join("state.vscdb"))
}

fn read_registration(auth: &Value) -> UsageResult<Option<Value>> {
    // Explicit `clientIdHash` wins. Otherwise the cache file is SHA-1 of the
    // start URL, the same name cockpit writes.
    let Some(hash) = string_field(Some(auth), &["clientIdHash", "client_id_hash"]).or_else(|| {
        string_field(
            Some(auth),
            &["startUrl", "start_url", "issuerUrl", "issuer_url", "issuer"],
        )
        .map(|start_url| super::idc::compute_idc_client_id_hash(&start_url))
    }) else {
        return Ok(None);
    };
    let cache = crate::tool_paths::aws_sso_cache_dir();
    let path = super::idc::idc_client_registration_path(&cache, &hash)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|err| UsageError::Other(format!("读取 Kiro IdC 客户端注册文件失败: {err}")))?;
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|_| UsageError::Other("Kiro IdC 客户端注册文件无法解析".into()))?;
    if !parsed.is_object() {
        return Err(UsageError::Other(
            "Kiro IdC 客户端注册文件须是 JSON 对象".into(),
        ));
    }
    Ok(Some(parsed))
}

fn read_optional_json(path: Option<&std::path::Path>) -> UsageResult<Option<Value>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|err| UsageError::Other(format!("读取 Kiro profile.json 失败: {err}")))?;
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|_| UsageError::Other("Kiro profile.json 无法解析".into()))?;
    Ok(Some(parsed))
}

fn read_usage_snapshot() -> UsageResult<Option<Value>> {
    let Some(path) = state_db_path() else {
        return Ok(None);
    };
    let Some(raw) = crate::vscdb::read_item_string(&path, USAGE_DB_KEY)? else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|_| UsageError::Other("Kiro usage 快照无法解析".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kiro_token_import_accepts_refresh_or_json_and_rejects_garbage() {
        let bare = import_from_token("refresh-token-value-0123456789").unwrap();
        assert!(bare.access_token.is_empty());
        assert_eq!(
            bare.refresh_token.as_deref(),
            Some("refresh-token-value-0123456789")
        );
        assert_eq!(bare.oauth_region.as_deref(), Some(DEFAULT_REGION));
        let row = oauth_row_from_imported(bare).unwrap();
        assert!(row.platform_token_encrypted.is_none());
        assert!(row.refresh_token_encrypted.is_some());

        let imported = import_from_token(
            r#"{"refreshToken":"refresh-token-value-0123456789","clientId":"cid","clientSecret":"sec","region":"eu-central-1","startUrl":"https://view.awsapps.com/start","profileArn":"arn:aws:codewhisperer:eu-central-1:1:profile/p","userId":"user-1"}"#,
        )
        .unwrap();
        assert_eq!(imported.oauth_account_id.as_deref(), Some("user-1"));
        assert_eq!(imported.oauth_region.as_deref(), Some("eu-central-1"));
        let row = oauth_row_from_imported(imported).unwrap();
        assert!(row.platform_token_encrypted.is_none());
        let plain = crate::crypto::decrypt(row.provider_state_encrypted.as_deref().unwrap());
        let state: Value = serde_json::from_str(&plain).unwrap();
        assert_eq!(state["clientId"], "cid");
        assert_eq!(state["clientSecret"], "sec");
        assert_eq!(state["region"], "eu-central-1");
        assert_eq!(state["startUrl"], "https://view.awsapps.com/start");
        assert_eq!(
            state["profileArn"],
            "arn:aws:codewhisperer:eu-central-1:1:profile/p"
        );
        assert!(state.get("idc").is_none());

        assert!(import_from_token("hello").is_err());
        assert!(import_error("{").contains("无法解析"));
        assert!(import_error("{}").contains("缺少可用令牌"));
        assert!(import_from_token("not a token").is_err());
        let secret = "refresh-token-value-0123456789";
        let broken = format!(r#"{{"refreshToken":"{secret}""#);
        let err = import_error(&broken);
        assert!(err.contains("无法解析"), "{err}");
        assert!(!err.contains(secret), "{err}");
    }

    fn import_error(payload: &str) -> String {
        match import_from_token(payload) {
            Err(err) => err.to_string(),
            Ok(_) => panic!("import must fail"),
        }
    }

    #[test]
    fn kiro_local_import_builds_a_row_from_the_sso_cache_pair() {
        let _lock = crate::test_env_lock()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let _env = ToolHome::set(dir.path(), Some(&dir.path().join("data")));
        local_fixture(dir.path());
    }

    struct ToolHome {
        prev_home: Option<std::ffi::OsString>,
        prev_data: Option<std::ffi::OsString>,
    }

    impl ToolHome {
        fn set(home: &std::path::Path, data: Option<&std::path::Path>) -> Self {
            let prev_home = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            let prev_data = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", home);
                if let Some(data) = data {
                    std::env::set_var("SKILLSTAR_DATA_DIR", data);
                }
            }
            Self {
                prev_home,
                prev_data,
            }
        }
    }

    impl Drop for ToolHome {
        fn drop(&mut self) {
            unsafe {
                match self.prev_home.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                    None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                }
                match self.prev_data.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
        }
    }

    fn local_fixture(home: &std::path::Path) {
        let hash = "eb04fe0de39241e4fa17bec175f7540138ad1bd8";
        let cache = home.join(".aws").join("sso").join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("unrelated.json"),
            r#"{"clientId":"wrong-client","clientSecret":"wrong-secret"}"#,
        )
        .unwrap();
        std::fs::write(
            cache.join(format!("{hash}.json")),
            r#"{"clientId":"idc-client","clientSecret":"idc-secret"}"#,
        )
        .unwrap();
        std::fs::write(
            cache.join("kiro-auth-token.json"),
            json!({
                "accessToken": "access-token-value-0123456789",
                "refreshToken": "refresh-token-value-0123456789",
                "clientIdHash": hash,
                "profileArn": "arn:aws:codewhisperer:us-west-2:111122223333:profile/abc"
            })
            .to_string(),
        )
        .unwrap();
        let profile_dir = home
            .join("Library")
            .join("Application Support")
            .join("Kiro")
            .join("User")
            .join("globalStorage")
            .join("kiro.kiroagent");
        if cfg!(target_os = "macos") {
            std::fs::create_dir_all(&profile_dir).unwrap();
            std::fs::write(
                profile_dir.join("profile.json"),
                r#"{"email":"dev@example.com"}"#,
            )
            .unwrap();
        }

        let imported = import_from_local().unwrap();
        assert_eq!(imported.access_token, "access-token-value-0123456789");
        assert_eq!(
            imported.refresh_token.as_deref(),
            Some("refresh-token-value-0123456789")
        );
        assert_eq!(imported.oauth_region.as_deref(), Some("us-west-2"));
        if cfg!(target_os = "macos") {
            assert_eq!(imported.display_name, "dev@example.com");
        }
        let row = oauth_row_from_imported(imported).unwrap();
        assert_eq!(row.catalog_id, CATALOG_ID);
        assert!(row.platform_token_encrypted.is_none());
        assert_eq!(row.oauth_region.as_deref(), Some("us-west-2"));
        let plain = crate::crypto::decrypt(row.provider_state_encrypted.as_deref().unwrap());
        let state: Value = serde_json::from_str(&plain).unwrap();
        assert_eq!(state["clientId"], "idc-client");
        assert_eq!(state["clientSecret"], "idc-secret");
        assert_ne!(state["clientSecret"], "wrong-secret");
        assert_eq!(state["region"], "us-west-2");

        let saved = crate::storage::upsert_subscription(row).unwrap();
        assert_eq!(saved.catalog_id, "kiro");
        assert!(saved.platform_token_encrypted.is_none());
    }

    #[test]
    fn kiro_local_import_reads_user_id_from_state_vscdb() {
        let _lock = crate::test_env_lock()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let _env = ToolHome::set(dir.path(), None);
        vscdb_fixture(dir.path());
    }

    fn vscdb_fixture(home: &std::path::Path) {
        let cache = home.join(".aws").join("sso").join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("kiro-auth-token.json"),
            r#"{"refreshToken":"refresh-token-value-0123456789"}"#,
        )
        .unwrap();
        let db_dir = kiro_global_storage(home);
        std::fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("state.vscdb");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            [USAGE_DB_KEY, r#"{"userInfo":{"userId":"kiro-user"}}"#],
        )
        .unwrap();
        let imported = import_from_local().unwrap();
        assert_eq!(imported.oauth_account_id.as_deref(), Some("kiro-user"));
        assert_eq!(imported.oauth_region.as_deref(), Some(DEFAULT_REGION));
    }

    fn kiro_global_storage(home: &std::path::Path) -> std::path::PathBuf {
        if cfg!(target_os = "macos") {
            home.join("Library")
                .join("Application Support")
                .join("Kiro")
                .join("User")
                .join("globalStorage")
        } else if cfg!(target_os = "windows") {
            home.join("AppData")
                .join("Roaming")
                .join("Kiro")
                .join("User")
                .join("globalStorage")
        } else {
            home.join(".config")
                .join("Kiro")
                .join("User")
                .join("globalStorage")
        }
    }

    #[test]
    fn kiro_local_import_missing_file_names_the_cache() {
        let _lock = crate::test_env_lock()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let _env = ToolHome::set(dir.path(), None);
        let err = match import_from_local() {
            Err(err) => err,
            Ok(_) => panic!("missing auth file must fail"),
        };
        assert!(err.to_string().contains("kiro-auth-token.json"), "{err}");
    }
}
