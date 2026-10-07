//! SchemePaste login.
//!
//! `start_login` registers `OAuthFlow::SchemePaste { scheme_prefix: "zcode://" }`.
//! The pasted URL is delivered on the manual inbox. Only the two cockpit hosts
//! are accepted, and the host must belong to the upstream that started the
//! login. The code is exchanged at `zcode.z.ai`; nothing is fetched from
//! `zcode://`.

use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;
use url::Url;

use super::http::{self, TokenEnvelope};
use super::{
    BIGMODEL_AUTHORIZE, BIGMODEL_REDIRECT, CALLBACK_ROUTES, CATALOG_ID, Endpoints,
    PLACEHOLDER_NAME, SCHEME_PREFIX, ZAI_AUTHORIZE, ZAI_CLIENT_ID, ZAI_REDIRECT, callback_host,
    nonempty, normalize_provider, provider_state_json,
};
use crate::fetchers::oauth::OAuthStartInfo;
use crate::fetchers::oauth::common::{carry_over_user_metadata, reauth_target};
use crate::subscription::Subscription;
use crate::token_import::ImportedToken;
use crate::{UsageError, UsageResult};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ExchangedGrant {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub jwt: String,
    pub expires_at: Option<i64>,
    pub user_info: serde_json::Value,
}

pub(crate) async fn start_login(
    region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<OAuthStartInfo> {
    let provider = normalize_provider(region)?;
    let state = generate_state();
    let auth_url = build_authorize_url(provider, &state)?;
    let (pending_id, inbox) = crate::oauth::pending_state::register_scheme_paste(
        CATALOG_ID,
        Some(provider),
        auth_url.clone(),
        SCHEME_PREFIX,
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );
    let pid = pending_id.clone();
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(provider, state, inbox, target).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });
    Ok(OAuthStartInfo::scheme_paste(
        auth_url,
        pending_id,
        SCHEME_PREFIX.to_string(),
    ))
}

pub(super) fn build_authorize_url(provider: &str, state: &str) -> UsageResult<String> {
    if provider == "zai" {
        let mut url = Url::parse(ZAI_AUTHORIZE)
            .map_err(|error| UsageError::Other(format!("Z.ai 授权地址无效: {error}")))?;
        url.query_pairs_mut()
            .append_pair("redirect_uri", ZAI_REDIRECT)
            .append_pair("response_type", "code")
            .append_pair("client_id", ZAI_CLIENT_ID)
            .append_pair("state", state);
        return Ok(url.to_string());
    }
    let mut url = Url::parse(BIGMODEL_AUTHORIZE)
        .map_err(|error| UsageError::Other(format!("BigModel 授权地址无效: {error}")))?;
    url.query_pairs_mut()
        .append_pair("redirect", BIGMODEL_REDIRECT)
        .append_pair("appId", "zcode")
        .append_pair("state", state);
    Ok(url.to_string())
}

/// Host whitelist is [`CALLBACK_ROUTES`]. Anything else, including a
/// path-shaped `zcode:///oauth/callback`, is rejected. The host must also be
/// the one registered for `provider`.
pub(super) fn parse_callback(
    callback: &str,
    provider: &str,
    expected_state: &str,
) -> UsageResult<String> {
    let url = Url::parse(callback.trim())
        .map_err(|error| UsageError::Other(format!("ZCode 回调链接无效: {error}")))?;
    if url.scheme() != "zcode" {
        return Err(UsageError::Other("ZCode 回调协议必须是 zcode://".into()));
    }
    let registered = CALLBACK_ROUTES
        .iter()
        .any(|(host, path)| url.host_str() == Some(*host) && url.path() == *path);
    if !registered {
        return Err(UsageError::Other(
            "ZCode 回调地址不在白名单（只接受 zcode://oauth/callback 与 zcode://zai-auth/callback）"
                .into(),
        ));
    }
    if url.host_str() != Some(callback_host(provider)) || url.path() != "/callback" {
        return Err(UsageError::Other("ZCode 回调地址与登录上游不匹配".into()));
    }
    let state = query_value(&url, "state")
        .ok_or_else(|| UsageError::Other("ZCode 回调缺少 state".into()))?;
    if state != expected_state {
        return Err(UsageError::Other("ZCode OAuth state 不匹配或已过期".into()));
    }
    if let Some(error) = query_value(&url, "error") {
        return Err(UsageError::Other(format!("ZCode 授权失败: {error}")));
    }
    query_first(&url, &["code", "authCode"])
        .ok_or_else(|| UsageError::Other("ZCode 回调缺少 code".into()))
}

pub(super) async fn exchange_code(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    provider: &str,
    code: &str,
    state: &str,
) -> UsageResult<ExchangedGrant> {
    let redirect = if provider == "zai" {
        ZAI_REDIRECT
    } else {
        BIGMODEL_REDIRECT
    };
    let token_body = http::request_json(
        client,
        reqwest::Method::POST,
        &endpoints.token,
        &[("accept", "application/json".into())],
        Some(&json!({
            "provider": provider,
            "code": code,
            "redirect_uri": redirect,
            "state": state,
        })),
        "ZCode OAuth Token",
    )
    .await?;
    let envelope = http::require_token_envelope(provider, &token_body)?;
    grant_from_envelope(client, endpoints, provider, envelope).await
}

async fn grant_from_envelope(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    provider: &str,
    envelope: TokenEnvelope,
) -> UsageResult<ExchangedGrant> {
    if provider == "zai" {
        let business = http::request_json(
            client,
            reqwest::Method::POST,
            &endpoints.zai_business,
            &[("accept", "application/json".into())],
            Some(&json!({ "token": envelope.provider_access_token })),
            "Z.ai 业务 Token",
        )
        .await?;
        let access_token = http::require_zai_business_token(&business)?;
        let expires_at = envelope
            .expires_in
            .filter(|seconds| *seconds > 0)
            .map(|seconds| chrono::Utc::now().timestamp().saturating_add(seconds));
        return Ok(ExchangedGrant {
            access_token,
            refresh_token: None,
            jwt: envelope.jwt,
            expires_at,
            user_info: envelope.user_info,
        });
    }
    Ok(ExchangedGrant {
        access_token: envelope.provider_access_token,
        refresh_token: envelope.refresh_token,
        jwt: envelope.jwt,
        expires_at: None,
        user_info: envelope.user_info,
    })
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    for byte in &mut bytes {
        *byte = rand::random::<u8>();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
}

fn query_first(url: &Url, keys: &[&str]) -> Option<String> {
    url.query_pairs().find_map(|(name, value)| {
        let text = value.into_owned();
        if keys.contains(&name.as_ref()) && !text.trim().is_empty() {
            Some(text)
        } else {
            None
        }
    })
}

async fn drive_login(
    provider: &'static str,
    state: String,
    mut inbox: mpsc::UnboundedReceiver<String>,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let pasted = match tokio::time::timeout(LOGIN_TIMEOUT, inbox.recv()).await {
        Ok(Some(value)) => value,
        Ok(None) => return Err(UsageError::Other("ZCode 登录已取消".into())),
        Err(_) => return Err(UsageError::Other("ZCode 登录已超时".into())),
    };
    let code = parse_callback(&pasted, provider, &state)?;
    let client = crate::fetchers::http_client()?;
    let endpoints = Endpoints::production();
    let grant = exchange_code(&client, &endpoints, provider, &code, &state).await?;
    crate::refresh_guard::with_catalog_lock(CATALOG_ID, || async {
        finalize(
            provider,
            grant,
            &client,
            &endpoints,
            target_subscription_id.as_deref(),
        )
        .await
    })
    .await?
}

async fn finalize(
    provider: &str,
    grant: ExchangedGrant,
    client: &reqwest::Client,
    endpoints: &Endpoints,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let mut sub = super::import::oauth_row_from_imported(imported_from_grant(provider, &grant))?;
    if let Some(existing) = reauth_target(CATALOG_ID, target_subscription_id) {
        carry_over_user_metadata(&mut sub, &existing, &[PLACEHOLDER_NAME]);
    }
    if let Ok(usage) = super::quota::read_quota(client, endpoints, &mut sub).await {
        crate::storage::save_usage_snapshot(usage).ok();
    } else if user_info_empty(&grant.user_info)
        && let Ok(profile) =
            super::quota::fetch_profile(client, endpoints, provider, &grant.access_token).await
    {
        super::quota::apply_identity(&mut sub, &profile);
    }
    crate::storage::upsert_subscription(sub)
        .map_err(|error| UsageError::Other(format!("ZCode 订阅保存失败：{error}")))
}

fn imported_from_grant(provider: &str, grant: &ExchangedGrant) -> ImportedToken {
    let user_id = http::pick_string(
        &grant.user_info,
        &[&["user_id"], &["id"], &["customerNumber"], &["sub"]],
    );
    let email = http::pick_string(&grant.user_info, &[&["email"]]);
    let name = http::pick_string(
        &grant.user_info,
        &[
            &["name"],
            &["displayName"],
            &["username"],
            &["nickName"],
            &["customerName"],
        ],
    );
    ImportedToken {
        display_name: email
            .clone()
            .filter(|value| crate::fetchers::oauth::common::looks_like_email(value))
            .or(name)
            .or(user_id.clone())
            .unwrap_or_else(|| PLACEHOLDER_NAME.to_string()),
        access_token: grant.access_token.clone(),
        refresh_token: grant.refresh_token.clone(),
        expires_at: grant.expires_at,
        oauth_account_id: user_id,
        provider_state: Some(provider_state_json("oauth")),
        currency: None,
        oauth_region: Some(provider.to_string()),
        id_token: nonempty(Some(grant.jwt.as_str())),
        api_key: None,
    }
}

fn user_info_empty(value: &serde_json::Value) -> bool {
    match value.as_object() {
        Some(object) => object.is_empty(),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetchers::oauth::OAuthFlow;

    fn query(url: &str, key: &str) -> String {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(name, _)| name == key)
            .unwrap()
            .1
            .into_owned()
    }

    #[test]
    fn authorize_urls_follow_cockpit() {
        let zai = build_authorize_url("zai", "state-1").unwrap();
        let zai_url = Url::parse(&zai).unwrap();
        assert_eq!(zai_url.host_str(), Some("chat.z.ai"));
        assert_eq!(zai_url.path(), "/api/oauth/authorize");
        assert_eq!(query(&zai, "redirect_uri"), ZAI_REDIRECT);
        assert_eq!(query(&zai, "response_type"), "code");
        assert_eq!(query(&zai, "client_id"), ZAI_CLIENT_ID);
        assert_eq!(query(&zai, "state"), "state-1");

        let bigmodel = build_authorize_url("bigmodel", "state-2").unwrap();
        let bigmodel_url = Url::parse(&bigmodel).unwrap();
        assert_eq!(bigmodel_url.host_str(), Some("bigmodel.cn"));
        assert_eq!(bigmodel_url.path(), "/login");
        assert_eq!(query(&bigmodel, "redirect"), BIGMODEL_REDIRECT);
        assert_eq!(query(&bigmodel, "appId"), "zcode");
        assert_eq!(query(&bigmodel, "state"), "state-2");
        assert!(
            bigmodel_url
                .query_pairs()
                .all(|(key, _)| key != "client_id")
        );
    }

    #[test]
    fn callback_whitelist_is_the_two_hosts_and_rejects_the_other_shape() {
        assert_eq!(
            parse_callback("zcode://oauth/callback?code=ok&state=s", "bigmodel", "s",).unwrap(),
            "ok"
        );
        assert_eq!(
            parse_callback(
                "zcode://zai-auth/callback?code=zai&state=state%2B1",
                "zai",
                "state+1",
            )
            .unwrap(),
            "zai"
        );
        assert_eq!(
            parse_callback(
                "zcode://oauth/callback?authCode=alt&state=s",
                "bigmodel",
                "s",
            )
            .unwrap(),
            "alt"
        );

        let unlisted =
            parse_callback("zcode://evil/callback?code=1&state=s", "zai", "s").unwrap_err();
        assert!(unlisted.to_string().contains("白名单"), "{unlisted}");
        let path_shaped =
            parse_callback("zcode:///oauth/callback?code=1&state=s", "bigmodel", "s").unwrap_err();
        assert!(path_shaped.to_string().contains("白名单"), "{path_shaped}");
        let https =
            parse_callback("https://oauth/callback?code=1&state=s", "bigmodel", "s").unwrap_err();
        assert!(https.to_string().contains("zcode://"), "{https}");
        let other_path =
            parse_callback("zcode://oauth/other?code=1&state=s", "bigmodel", "s").unwrap_err();
        assert!(other_path.to_string().contains("白名单"), "{other_path}");
        let mismatch =
            parse_callback("zcode://oauth/callback?code=1&state=s", "zai", "s").unwrap_err();
        assert!(mismatch.to_string().contains("不匹配"), "{mismatch}");
        let state =
            parse_callback("zcode://zai-auth/callback?code=1&state=nope", "zai", "s").unwrap_err();
        assert!(state.to_string().contains("state"), "{state}");
        let missing = parse_callback("zcode://zai-auth/callback?code=1", "zai", "s").unwrap_err();
        assert!(missing.to_string().contains("state"), "{missing}");
        let denied = parse_callback(
            "zcode://zai-auth/callback?error=access_denied&state=s",
            "zai",
            "s",
        )
        .unwrap_err();
        assert!(denied.to_string().contains("access_denied"), "{denied}");
        assert!(!denied.to_string().contains("code="), "{denied}");
    }

    #[test]
    fn generated_state_is_32_byte_hex() {
        let state = generate_state();
        assert_eq!(state.len(), 64);
        assert!(state.chars().all(|value| value.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn start_login_registers_scheme_paste_and_rejects_a_bad_paste() {
        let info = start_login(Some("zai"), None).await.unwrap();
        assert!(matches!(
            info.flow,
            OAuthFlow::SchemePaste { ref scheme_prefix } if scheme_prefix == "zcode://"
        ));
        assert!(info.auth_url.contains("chat.z.ai"));
        let rx = crate::oauth::pending_state::take_receiver(&info.pending_id).unwrap();
        crate::oauth::manual_callback::deliver_manual_input(
            &info.pending_id,
            "zcode://evil/callback?code=1&state=nope",
        )
        .await
        .unwrap();
        let error = rx.await.unwrap().unwrap_err();
        assert!(error.to_string().contains("白名单"), "{error}");
        assert!(!error.to_string().contains("zcode.z.ai"), "{error}");
        crate::oauth::pending_state::remove(&info.pending_id);

        let bigmodel = start_login(Some("BIGMODEL"), None).await.unwrap();
        assert!(bigmodel.auth_url.contains("bigmodel.cn"));
        let _ = crate::oauth::pending_state::cancel(&bigmodel.pending_id);
        let unknown = start_login(Some("openai"), None).await.unwrap_err();
        assert!(unknown.to_string().contains("不支持"), "{unknown}");
    }

    #[tokio::test]
    async fn exchange_uses_the_scripted_host_for_both_upstreams() {
        use super::super::http::scripted::ScriptedHttp;

        let zai = ScriptedHttp::start(vec![
            (
                200,
                r#"{"code":0,"data":{"token":"jwt-z","zai":{"access_token":"oauth-z"},"expires_in":60,"user":{"email":"a@b.c","user_id":"u1"}}}"#.into(),
            ),
            (
                200,
                r#"{"code":200,"data":{"access_token":"business-z"}}"#.into(),
            ),
        ]);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap();
        let endpoints = Endpoints::from_base(&zai.base);
        let grant = exchange_code(&client, &endpoints, "zai", "code-z", "state-z")
            .await
            .unwrap();
        assert_eq!(grant.access_token, "business-z");
        assert_eq!(grant.jwt, "jwt-z");
        assert!(grant.refresh_token.is_none());
        assert_eq!(grant.user_info["email"], "a@b.c");
        let seen = zai.seen();
        assert_eq!(seen[0].path, "/api/v1/oauth/token");
        assert_eq!(seen[1].path, "/api/auth/z/login");
        assert!(seen.iter().all(|item| !item.path.contains("zcode.z.ai")));

        let bigmodel = ScriptedHttp::start(vec![(
            200,
            r#"{"data":{"token":"jwt-b","bigmodel":{"access_token":"access-b","refresh_token":"refresh-b"}}}"#.into(),
        )]);
        let endpoints = Endpoints::from_base(&bigmodel.base);
        let grant = exchange_code(&client, &endpoints, "bigmodel", "code-b", "state-b")
            .await
            .unwrap();
        assert_eq!(grant.access_token, "access-b");
        assert_eq!(grant.refresh_token.as_deref(), Some("refresh-b"));
        assert_eq!(grant.jwt, "jwt-b");
        assert_eq!(bigmodel.seen()[0].method, "POST");

        let forbidden = ScriptedHttp::start(vec![(403, r#"{"msg":"nope"}"#.into())]);
        let endpoints = Endpoints::from_base(&forbidden.base);
        let error = exchange_code(&client, &endpoints, "bigmodel", "c", "s")
            .await
            .unwrap_err();
        assert!(matches!(error, UsageError::Fetcher(_)), "{error:?}");
        assert!(!error.is_transient());
    }
}
