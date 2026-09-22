//! Remote-poll login.
//!
//! `POST /v2/plugin/auth/state?platform=ide` returns `state` + `authUrl`.
//! The browser opens that URL with `loginSessionId`. The server is polled at
//! `GET /v2/plugin/auth/token?state=` until it returns access + refresh.
//! There is no user code and nothing to paste.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use url::Url;

use super::http::{self, IssuedToken, PollBody};
use super::{
    ACCOUNT_PATH, AUTH_STATE_PATH, AUTH_TOKEN_PATH, Host, LoginIdentity, PLATFORM, POLL_INTERVAL,
    POLL_INTERVAL_SECS, endpoint, host_for, pick_string,
};
use crate::fetchers::oauth::OAuthStartInfo;
use crate::{UsageError, UsageResult};

pub(crate) async fn start_login(
    catalog_id: &str,
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<OAuthStartInfo> {
    let host = host_for(catalog_id).ok_or_else(|| crate::fetchers::unsupported(catalog_id))?;
    let client = crate::fetchers::http_client()?;
    let begun = request_state(
        &client,
        host.origin,
        &format!("{} auth/state", host.display_name),
    )
    .await?;
    let auth_url = begun.auth_url.clone();
    let pending_id = crate::oauth::pending_state::register_with_flow(
        host.catalog_id,
        Some(host.oauth_region),
        auth_url.clone(),
        crate::fetchers::oauth::OAuthFlow::RemotePoll,
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );
    let pid = pending_id.clone();
    let state = begun.state;
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(host, state, &pid, target).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });
    Ok(start_info(auth_url, pending_id))
}

pub(super) fn start_info(auth_url: String, pending_id: String) -> OAuthStartInfo {
    OAuthStartInfo::remote_poll(auth_url, pending_id, Some(POLL_INTERVAL_SECS))
}

#[derive(Debug)]
pub(super) struct BegunLogin {
    pub state: String,
    pub auth_url: String,
}

pub(super) struct PollClock {
    pub interval: Duration,
    pub deadline: Instant,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AccountProfile {
    pub uid: Option<String>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub enterprise_id: Option<String>,
    pub enterprise_name: Option<String>,
}

pub(super) async fn request_state(
    client: &reqwest::Client,
    origin: &str,
    label: &str,
) -> UsageResult<BegunLogin> {
    let url = state_url(origin)?;
    let (status, body) = http::execute(
        client,
        reqwest::Method::POST,
        &url,
        &http::anonymous_headers(),
        Some(&json!({})),
        label,
    )
    .await?;
    let value = http::classify_exchange(status, &body, label)?;
    begun_from_body(origin, &value)
}

pub(super) fn begun_from_body(origin: &str, body: &Value) -> UsageResult<BegunLogin> {
    let data = body
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(body);
    let state = pick_string(data, &["state"])
        .ok_or_else(|| UsageError::Fetcher("CodeBuddy auth/state 响应缺少 state".into()))?;
    let raw_url = pick_string(data, &["authUrl", "auth_url", "url"]).unwrap_or_default();
    let base = if raw_url.is_empty() {
        format!("{}/login?state={state}", origin.trim_end_matches('/'))
    } else {
        raw_url
    };
    let session = uuid::Uuid::new_v4().to_string();
    Ok(BegunLogin {
        state,
        auth_url: decorate_login_url(&base, &session),
    })
}

pub(super) fn decorate_login_url(raw_url: &str, login_session_id: &str) -> String {
    let Ok(mut url) = Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    url.query_pairs_mut()
        .append_pair("loginSessionId", login_session_id);
    url.to_string()
}

pub(super) async fn poll_until(
    client: &reqwest::Client,
    origin: &str,
    state: &str,
    clock: &PollClock,
    mut cancelled: impl FnMut() -> bool,
) -> UsageResult<IssuedToken> {
    let mut last_transient: Option<UsageError> = None;
    loop {
        if cancelled() {
            return Err(cancelled_error());
        }
        if Instant::now() >= clock.deadline {
            return Err(last_transient.unwrap_or_else(timeout_error));
        }
        match poll_once(client, origin, state).await {
            Ok(PollBody::Ready(token)) => return Ok(token),
            Ok(PollBody::Pending) => {}
            Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
            Err(err) if err.is_transient() => last_transient = Some(err),
            Err(err) => return Err(err),
        }
        if cancelled() {
            return Err(cancelled_error());
        }
        if Instant::now() >= clock.deadline {
            return Err(last_transient.unwrap_or_else(timeout_error));
        }
        if !clock.interval.is_zero() {
            tokio::time::sleep(clock.interval).await;
        }
    }
}

pub(super) async fn poll_and_profile(
    client: &reqwest::Client,
    origin: &str,
    state: &str,
    clock: &PollClock,
    cancelled: impl FnMut() -> bool,
) -> UsageResult<(IssuedToken, AccountProfile)> {
    let token = poll_until(client, origin, state, clock, cancelled).await?;
    let profile = fetch_profile(client, origin, state, &token).await;
    Ok((token, profile))
}

async fn drive_login(
    host: &'static Host,
    state: String,
    pending_id: &str,
    target_subscription_id: Option<String>,
) -> UsageResult<crate::subscription::Subscription> {
    let client = crate::fetchers::http_client()?;
    let clock = PollClock {
        interval: POLL_INTERVAL,
        deadline: Instant::now() + super::LOGIN_TIMEOUT,
    };
    let (grant, profile) = poll_and_profile(&client, host.origin, &state, &clock, || {
        crate::oauth::pending_state::flow(pending_id).is_none()
    })
    .await?;
    let identity = login_identity(grant, profile);
    let mut sub =
        super::import::oauth_row(host, super::import::imported_from_login(host, &identity))?;
    let usage = match super::quota::fetch_quota(host, &mut sub, false).await {
        Ok(usage) => Some(usage),
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(_) => None,
    };
    let placeholders = [host.display_name];
    crate::refresh_guard::with_catalog_lock(host.catalog_id, || async {
        if let Some(existing) = crate::fetchers::oauth::common::reauth_target(
            host.catalog_id,
            target_subscription_id.as_deref(),
        ) {
            crate::fetchers::oauth::common::carry_over_user_metadata(
                &mut sub,
                &existing,
                &placeholders,
            );
        }
        if let Some(mut usage) = usage {
            usage.subscription_id = sub.id.clone();
            if usage.has_quota_data() {
                crate::storage::save_usage_snapshot(usage).ok();
            }
        }
        crate::storage::upsert_subscription(sub)
            .map_err(|err| UsageError::Other(format!("{} 订阅保存失败：{err}", host.display_name)))
    })
    .await?
}

fn login_identity(grant: IssuedToken, profile: AccountProfile) -> LoginIdentity {
    let mut enterprise = grant.enterprise();
    if let Some(id) = profile.enterprise_id.clone() {
        enterprise.enterprise_id = Some(id);
    }
    if let Some(name) = profile.enterprise_name.clone() {
        enterprise.enterprise_name = Some(name);
    }
    LoginIdentity {
        access_token: grant.access_token,
        refresh_token: grant.refresh_token,
        expires_at: grant.expires_at,
        uid: profile.uid.or(grant.uid),
        email: profile.email,
        nickname: profile.nickname,
        enterprise,
    }
}

async fn poll_once(client: &reqwest::Client, origin: &str, state: &str) -> UsageResult<PollBody> {
    let url = token_url(origin, state)?;
    let (status, body) = http::execute(
        client,
        reqwest::Method::GET,
        &url,
        &http::anonymous_headers(),
        None,
        "CodeBuddy auth/token",
    )
    .await?;
    http::interpret_poll(status, &body)
}

async fn fetch_profile(
    client: &reqwest::Client,
    origin: &str,
    state: &str,
    token: &IssuedToken,
) -> AccountProfile {
    let Ok(url) = account_url(origin, state) else {
        return AccountProfile::default();
    };
    let Ok((status, body)) = http::execute(
        client,
        reqwest::Method::GET,
        &url,
        &http::account_headers(&token.access_token, token.domain.as_deref()),
        None,
        "CodeBuddy account",
    )
    .await
    else {
        return AccountProfile::default();
    };
    let Ok(value) = http::classify_exchange(status, &body, "CodeBuddy account") else {
        return AccountProfile::default();
    };
    profile_from_body(&value)
}

pub(super) fn profile_from_body(body: &Value) -> AccountProfile {
    let data = body
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(body);
    AccountProfile {
        uid: pick_string(data, &["uid", "userId", "user_id"]),
        email: pick_string(data, &["email"]),
        nickname: pick_string(data, &["nickname", "name"]),
        enterprise_id: pick_string(data, &["enterpriseId", "enterprise_id"]),
        enterprise_name: pick_string(data, &["enterpriseName", "enterprise_name"]),
    }
}

pub(super) fn token_url(origin: &str, state: &str) -> UsageResult<String> {
    with_query(&endpoint(origin, AUTH_TOKEN_PATH), &[("state", state)])
}

fn state_url(origin: &str) -> UsageResult<String> {
    with_query(
        &endpoint(origin, AUTH_STATE_PATH),
        &[("platform", PLATFORM)],
    )
}

fn account_url(origin: &str, state: &str) -> UsageResult<String> {
    with_query(&endpoint(origin, ACCOUNT_PATH), &[("state", state)])
}

pub(super) fn with_query(url: &str, pairs: &[(&str, &str)]) -> UsageResult<String> {
    let mut parsed =
        Url::parse(url).map_err(|err| UsageError::Other(format!("CodeBuddy 地址无效: {err}")))?;
    {
        let mut query = parsed.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
    }
    Ok(parsed.to_string())
}

fn timeout_error() -> UsageError {
    UsageError::Other("CodeBuddy 登录已超时".into())
}

fn cancelled_error() -> UsageError {
    UsageError::Other("用户取消登录".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client")
    }

    fn clock(interval: Duration) -> PollClock {
        PollClock {
            interval,
            deadline: Instant::now() + Duration::from_secs(5),
        }
    }

    #[test]
    fn remote_poll_start_has_no_user_code() {
        let info = start_info(
            "https://www.codebuddy.ai/login?state=s".into(),
            "pending".into(),
        );
        assert_eq!(info.flow, crate::fetchers::oauth::OAuthFlow::RemotePoll);
        assert!(info.user_code.is_none());
        assert!(info.verification_uri.is_none());
        assert_eq!(info.interval_secs, Some(POLL_INTERVAL_SECS));
    }

    #[test]
    fn login_url_uses_the_returned_auth_url_and_a_fallback() {
        let session = "session-1";
        let decorated = decorate_login_url("https://www.codebuddy.ai/login?state=abc", session);
        let parsed = Url::parse(&decorated).unwrap();
        let pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        assert!(pairs.contains(&("state".into(), "abc".into())));
        assert!(pairs.contains(&("loginSessionId".into(), session.into())));
        assert!(!pairs.iter().any(|(key, _)| key == "version"));

        let begun = begun_from_body(
            "https://www.codebuddy.cn",
            &json!({"code":0,"data":{"state":"st"}}),
        )
        .unwrap();
        assert_eq!(begun.state, "st");
        let parsed = Url::parse(&begun.auth_url).unwrap();
        assert_eq!(parsed.host_str(), Some("www.codebuddy.cn"));
        assert_eq!(parsed.path(), "/login");
        assert!(
            parsed
                .query_pairs()
                .any(|(key, value)| key == "state" && value == "st")
        );
        assert!(parsed.query_pairs().any(|(key, _)| key == "loginSessionId"));
    }

    #[test]
    fn state_urls_for_both_hosts_share_the_path() {
        let global = with_query(
            &endpoint(super::super::GLOBAL.origin, AUTH_STATE_PATH),
            &[("platform", PLATFORM)],
        )
        .unwrap();
        let cn = with_query(
            &endpoint(super::super::CN.origin, AUTH_STATE_PATH),
            &[("platform", PLATFORM)],
        )
        .unwrap();
        assert!(global.contains("https://www.codebuddy.ai/v2/plugin/auth/state?"));
        assert!(cn.contains("https://www.codebuddy.cn/v2/plugin/auth/state?"));
        assert!(global.contains("platform=ide"));
        assert!(cn.contains("platform=ide"));
        assert_eq!(
            global.trim_start_matches(super::super::GLOBAL.origin),
            cn.trim_start_matches(super::super::CN.origin)
        );
    }

    #[tokio::test]
    async fn state_then_poll_then_account_sends_the_cockpit_requests() {
        let server = super::http::scripted::ScriptedHttp::start(vec![
            (
                200,
                r#"{"code":0,"data":{"state":"abc","authUrl":"https://www.codebuddy.ai/login?state=abc"}}"#.into(),
            ),
            (200, r#"{"code":8,"message":"waiting"}"#.into()),
            (
                200,
                r#"{"code":0,"data":{"accessToken":"access-token-value-0123456789","refreshToken":"refresh-token-value-0123456789","expiresIn":60,"domain":"team.example"}}"#.into(),
            ),
            (
                200,
                r#"{"code":0,"data":{"uid":"uid-1","email":"ada@example.com","nickname":"Ada","enterpriseId":"ent-1","enterpriseName":"Acme"}}"#.into(),
            ),
        ]);
        let begun = request_state(&client(), &server.base, "CodeBuddy auth/state")
            .await
            .expect("state");
        assert_eq!(begun.state, "abc");
        let parsed = Url::parse(&begun.auth_url).unwrap();
        assert_eq!(parsed.path(), "/login");
        assert!(parsed.query_pairs().any(|(key, _)| key == "loginSessionId"));

        let (grant, profile) = poll_and_profile(
            &client(),
            &server.base,
            &begun.state,
            &clock(Duration::ZERO),
            || false,
        )
        .await
        .expect("profile");
        assert_eq!(grant.access_token, "access-token-value-0123456789");
        assert_eq!(
            grant.refresh_token.as_deref(),
            Some("refresh-token-value-0123456789")
        );
        assert_eq!(grant.domain.as_deref(), Some("team.example"));
        assert_eq!(profile.uid.as_deref(), Some("uid-1"));
        assert_eq!(profile.email.as_deref(), Some("ada@example.com"));
        assert_eq!(profile.enterprise_id.as_deref(), Some("ent-1"));
        assert_eq!(profile.enterprise_name.as_deref(), Some("Acme"));

        let seen = server.seen();
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[0].method, "POST");
        assert!(
            seen[0].path.starts_with("/v2/plugin/auth/state?"),
            "{}",
            seen[0].path
        );
        assert!(seen[0].path.contains("platform=ide"), "{}", seen[0].path);
        assert_eq!(
            super::http::scripted::header(&seen[0], "user-agent"),
            Some(super::super::USER_AGENT)
        );
        assert_eq!(
            super::http::scripted::header(&seen[0], "x-no-authorization"),
            Some("true")
        );
        assert!(
            seen[0].body.contains("{}") || seen[0].body.contains("{\n") || !seen[0].body.is_empty()
        );
        for request in &seen[1..3] {
            assert_eq!(request.method, "GET");
            assert!(
                request.path.starts_with("/v2/plugin/auth/token?"),
                "{}",
                request.path
            );
            assert!(request.path.contains("state=abc"), "{}", request.path);
            assert!(super::http::scripted::header(request, "authorization").is_none());
            assert_eq!(
                super::http::scripted::header(request, "user-agent"),
                Some(super::super::USER_AGENT)
            );
        }
        assert_eq!(seen[3].method, "GET");
        assert!(
            seen[3].path.starts_with("/v2/plugin/login/account?"),
            "{}",
            seen[3].path
        );
        assert_eq!(
            super::http::scripted::header(&seen[3], "authorization"),
            Some("Bearer access-token-value-0123456789")
        );
        assert_eq!(
            super::http::scripted::header(&seen[3], "x-domain"),
            Some("team.example")
        );
    }

    #[tokio::test]
    async fn poll_timeout_cancel_auth_and_ua_reject() {
        let expired = poll_until(
            &client(),
            "http://127.0.0.1:1",
            "n",
            &PollClock {
                interval: Duration::ZERO,
                deadline: Instant::now() - Duration::from_secs(1),
            },
            || false,
        )
        .await
        .expect_err("timeout");
        assert!(expired.to_string().contains("超时"), "{expired}");

        let hits = AtomicUsize::new(0);
        let server =
            super::http::scripted::ScriptedHttp::start(vec![(200, r#"{"code":8}"#.into())]);
        let cancelled = poll_until(&client(), &server.base, "n", &clock(Duration::ZERO), || {
            hits.fetch_add(1, Ordering::SeqCst) > 0
        })
        .await
        .expect_err("cancel");
        assert!(cancelled.to_string().contains("取消"), "{cancelled}");

        let denied = super::http::scripted::ScriptedHttp::start(vec![(401, "nope".into())]);
        let auth = poll_until(&client(), &denied.base, "n", &clock(Duration::ZERO), || {
            false
        })
        .await
        .expect_err("401");
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let ua = super::http::scripted::ScriptedHttp::start(vec![(
            200,
            r#"{"code":10085,"message":"missing user agent"}"#.into(),
        )]);
        let rejected = poll_until(&client(), &ua.base, "n", &clock(Duration::ZERO), || false)
            .await
            .expect_err("ua");
        assert!(matches!(rejected, UsageError::Fetcher(_)), "{rejected:?}");
        assert!(rejected.to_string().contains("10085"), "{rejected}");

        let state_err = super::http::scripted::ScriptedHttp::start(vec![(
            200,
            r#"{"code":10085,"message":"missing user agent"}"#.into(),
        )]);
        let begin = request_state(&client(), &state_err.base, "CodeBuddy auth/state")
            .await
            .expect_err("state");
        assert!(matches!(begin, UsageError::Fetcher(_)), "{begin:?}");
        assert!(!begin.is_transient());
    }
}
