//! Device login: machine info → selectAccounts URL → poll `deviceToken/poll`.
//!
//! There is no user code. The panel opens the link and waits. Poll sends the
//! same Cosy headers as quota. 404 and an empty token stay pending. Cancel
//! drops the pending session; the loop notices and stops.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use url::Url;

use super::http::{self, DeviceGrant, PollBody};
use super::{CATALOG_ID, CHALLENGE_METHOD, LOGIN_URL, OPENAPI_BASE, QoderMachine, REDIRECT_URI};
use crate::UsageError;
use crate::UsageResult;
use crate::oauth::pkce::PkcePair;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);
const POLL_PATH: &str = "/api/v1/deviceToken/poll";

pub(super) struct PreparedLogin {
    pub auth_url: String,
    pub nonce: String,
    pub verifier: String,
    pub machine: QoderMachine,
    pub openapi_base: String,
}

pub(super) struct PollClock {
    pub interval: Duration,
    pub deadline: Instant,
}

pub(super) struct InstalledMachine {
    pub machine: QoderMachine,
    pub login_machine_id: Option<String>,
}

pub(crate) async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let installed = load_installed_machine();
    let prepared = prepare_login(
        installed.machine,
        installed.login_machine_id.as_deref(),
        LOGIN_URL,
        OPENAPI_BASE,
    )?;
    let auth_url = prepared.auth_url.clone();
    let pending_id = crate::oauth::pending_state::register_with_flow(
        CATALOG_ID,
        None,
        auth_url.clone(),
        super::super::OAuthFlow::RemotePoll,
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );
    let pid = pending_id.clone();
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(prepared, &pid, target.as_deref()).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });
    Ok(super::super::OAuthStartInfo::remote_poll(
        auth_url,
        pending_id,
        Some(u32::try_from(POLL_INTERVAL.as_secs()).unwrap_or(1)),
    ))
}

pub(super) fn prepare_login(
    machine: QoderMachine,
    login_machine_id: Option<&str>,
    login_base: &str,
    openapi_base: &str,
) -> UsageResult<PreparedLogin> {
    let pkce = PkcePair::generate();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let auth_url = build_login_url(login_base, &nonce, &pkce.challenge, login_machine_id)?;
    Ok(PreparedLogin {
        auth_url,
        nonce,
        verifier: pkce.verifier,
        machine,
        openapi_base: openapi_base.trim_end_matches('/').to_string(),
    })
}

/// Cockpit puts the machine *token* in the `machine_id` query when it has one,
/// and only then the `cache/id` file. The Cosy machine id header is separate.
pub(super) fn build_login_url(
    login_base: &str,
    nonce: &str,
    challenge: &str,
    machine_id: Option<&str>,
) -> UsageResult<String> {
    let mut url = Url::parse(login_base)
        .map_err(|err| UsageError::Other(format!("Qoder 登录地址无效: {err}")))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("nonce", nonce);
        query.append_pair("challenge", challenge);
        query.append_pair("challenge_method", CHALLENGE_METHOD);
        query.append_pair("redirect_uri", REDIRECT_URI);
        if let Some(machine_id) = machine_id.map(str::trim).filter(|value| !value.is_empty()) {
            query.append_pair("machine_id", machine_id);
        }
    }
    Ok(url.to_string())
}

pub(super) async fn poll_until(
    client: &reqwest::Client,
    openapi_base: &str,
    nonce: &str,
    verifier: &str,
    machine: &QoderMachine,
    clock: &PollClock,
    mut cancelled: impl FnMut() -> bool,
) -> UsageResult<DeviceGrant> {
    let mut last_transient: Option<UsageError> = None;
    loop {
        if cancelled() {
            return Err(cancelled_error());
        }
        if Instant::now() >= clock.deadline {
            return Err(last_transient.unwrap_or_else(timeout_error));
        }
        match poll_once(client, openapi_base, nonce, verifier, machine).await {
            Ok(PollBody::Ready(grant)) => return Ok(grant),
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

async fn drive_login(
    prepared: PreparedLogin,
    pending_id: &str,
    target_subscription_id: Option<&str>,
) -> UsageResult<crate::subscription::Subscription> {
    let client = crate::fetchers::http_client()?;
    let clock = PollClock {
        interval: POLL_INTERVAL,
        deadline: Instant::now() + LOGIN_TIMEOUT,
    };
    let grant = poll_until(
        &client,
        &prepared.openapi_base,
        &prepared.nonce,
        &prepared.verifier,
        &prepared.machine,
        &clock,
        || crate::oauth::pending_state::flow(pending_id).is_none(),
    )
    .await?;
    finish_login(
        &client,
        &prepared.openapi_base,
        grant,
        prepared.machine,
        target_subscription_id,
    )
    .await
}

pub(super) async fn finish_login(
    client: &reqwest::Client,
    openapi_base: &str,
    grant: DeviceGrant,
    machine: QoderMachine,
    target_subscription_id: Option<&str>,
) -> UsageResult<crate::subscription::Subscription> {
    let profile =
        super::quota::read_profile(client, openapi_base, &grant.token, &machine, false).await?;
    let identity = super::quota::identity_of(&profile);
    let imported = super::import::from_grant(grant, machine, identity);
    let target = target_subscription_id.map(str::to_string);
    crate::refresh_guard::with_catalog_lock(CATALOG_ID, || async {
        let mut sub = super::import::oauth_row_from_imported(imported)?;
        if let Some(existing) =
            crate::fetchers::oauth::common::reauth_target(CATALOG_ID, target.as_deref())
        {
            crate::fetchers::oauth::common::carry_over_user_metadata(
                &mut sub,
                &existing,
                &["Qoder"],
            );
        }
        let usage = super::quota::usage_from_profile(&sub.id, &profile);
        if usage.has_quota_data() {
            crate::storage::save_usage_snapshot(usage).ok();
        }
        crate::storage::upsert_subscription(sub)
            .map_err(|err| UsageError::Other(format!("Qoder 订阅保存失败：{err}")))
    })
    .await?
}

async fn poll_once(
    client: &reqwest::Client,
    openapi_base: &str,
    nonce: &str,
    verifier: &str,
    machine: &QoderMachine,
) -> UsageResult<PollBody> {
    let url = poll_url(openapi_base, nonce, verifier)?;
    let response = http::apply_headers(client.get(&url), None, machine)
        .send()
        .await
        .map_err(|err| UsageError::transport("Qoder deviceToken", err))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    http::interpret_poll(status, &body)
}

pub(super) fn poll_url(openapi_base: &str, nonce: &str, verifier: &str) -> UsageResult<String> {
    let mut url = Url::parse(&http::endpoint(openapi_base, POLL_PATH))
        .map_err(|err| UsageError::Other(format!("Qoder 轮询地址无效: {err}")))?;
    url.query_pairs_mut()
        .append_pair("nonce", nonce)
        .append_pair("verifier", verifier)
        .append_pair("challenge_method", CHALLENGE_METHOD);
    Ok(url.to_string())
}

pub(super) fn load_installed_machine() -> InstalledMachine {
    let Some(db) = crate::tool_paths::qoder_state_db_path() else {
        return InstalledMachine {
            machine: QoderMachine::default(),
            login_machine_id: None,
        };
    };
    load_machine_at(&user_data_root(&db))
}

pub(super) fn load_machine_at(user_data: &Path) -> InstalledMachine {
    let cache = user_data.join("SharedClientCache").join("cache");
    let mut machine = std::fs::read_to_string(cache.join("machine_token.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .map(|value: serde_json::Value| QoderMachine::from_cache_value(&value))
        .unwrap_or_default();
    let file_id = std::fs::read_to_string(cache.join("id"))
        .ok()
        .and_then(|text| super::nonempty(Some(text.trim())));
    if machine.machine_id.is_none() {
        machine.machine_id = file_id.clone();
    }
    let login_machine_id = machine.machine_token.clone().or(file_id);
    InstalledMachine {
        machine,
        login_machine_id,
    }
}

pub(super) fn user_data_root(db: &Path) -> PathBuf {
    let mut path = db.to_path_buf();
    if path.file_name().is_some_and(|name| name == "state.vscdb") {
        path.pop();
    }
    if path.file_name().is_some_and(|name| name == "globalStorage") {
        path.pop();
    }
    if path.file_name().is_some_and(|name| name == "User") {
        path.pop();
    }
    path
}

fn cancelled_error() -> UsageError {
    UsageError::Other("用户取消登录".into())
}

fn timeout_error() -> UsageError {
    UsageError::Other("Qoder 登录已超时，请重试".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetchers::oauth::{OAuthFlow, OAuthStartInfo};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn machine() -> QoderMachine {
        QoderMachine {
            machine_token: Some("machine-token".into()),
            machine_id: Some("machine-id".into()),
            machine_type: Some("machine-type".into()),
            machine_code: Some("machine-code".into()),
            hostname: Some("machine-hostname".into()),
            os: Some("aarch64_darwin".into()),
            cosy_version: Some("1.27.1".into()),
        }
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client")
    }

    #[test]
    fn qoder_login_url_matches_the_cockpit_select_accounts_link() {
        let url = build_login_url(
            LOGIN_URL,
            "test-nonce",
            "test-challenge",
            Some("machine-token"),
        )
        .expect("url");
        let parsed = Url::parse(&url).expect("parse");
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("qoder.com"));
        assert_eq!(parsed.path(), "/device/selectAccounts");
        let query: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        assert!(query.contains(&("nonce".into(), "test-nonce".into())));
        assert!(query.contains(&("challenge".into(), "test-challenge".into())));
        assert!(query.contains(&("challenge_method".into(), CHALLENGE_METHOD.into())));
        assert!(query.contains(&("redirect_uri".into(), REDIRECT_URI.into())));
        assert!(query.contains(&("machine_id".into(), "machine-token".into())));
        assert!(!query.iter().any(|(key, _)| key == "client_id"));
        assert!(!query.iter().any(|(key, _)| key == "user_code"));

        let bare = build_login_url(LOGIN_URL, "n", "c", None).unwrap();
        let parsed = Url::parse(&bare).unwrap();
        assert!(!parsed.query_pairs().any(|(key, _)| key == "machine_id"));
    }

    #[test]
    fn qoder_remote_poll_start_has_no_user_code() {
        let info = OAuthStartInfo::remote_poll(LOGIN_URL.to_string(), "pending".into(), Some(1));
        assert_eq!(info.flow, OAuthFlow::RemotePoll);
        assert!(info.user_code.is_none());
        assert!(info.verification_uri.is_none());
        assert_eq!(info.interval_secs, Some(1));
        assert_eq!(info.auth_url, LOGIN_URL);
    }

    #[test]
    fn qoder_machine_cache_prefers_the_token_for_the_login_query() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("SharedClientCache").join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("machine_token.json"),
            r#"{"token":"mt","id":"from-json","type":"ide","code":"c","hostname":"h","os":"aarch64_darwin","version":"1.0.0"}"#,
        )
        .unwrap();
        std::fs::write(cache.join("id"), "from-file").unwrap();
        let loaded = load_machine_at(dir.path());
        assert_eq!(loaded.login_machine_id.as_deref(), Some("mt"));
        assert_eq!(loaded.machine.machine_id.as_deref(), Some("from-json"));
        assert_eq!(loaded.machine.cosy_version.as_deref(), Some("1.0.0"));

        std::fs::write(cache.join("machine_token.json"), "{}").unwrap();
        let loaded = load_machine_at(dir.path());
        assert_eq!(loaded.login_machine_id.as_deref(), Some("from-file"));
        assert_eq!(loaded.machine.machine_id.as_deref(), Some("from-file"));
    }

    #[tokio::test]
    async fn qoder_poll_pending_then_ready_sends_the_cosy_headers() {
        let server = super::http::scripted::ScriptedHttp::start(vec![
            (404, String::new()),
            (
                200,
                r#"{"token":"user-token-value-0123456789","user_id":"user-1","refresh_token":"refresh-token-value-0123456789","expires_at":"1700000000"}"#.into(),
            ),
        ]);
        let clock = PollClock {
            interval: Duration::ZERO,
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let grant = poll_until(
            &client(),
            &server.base,
            "nonce-1",
            "verifier-1",
            &machine(),
            &clock,
            || false,
        )
        .await
        .expect("ready");
        assert_eq!(grant.token, "user-token-value-0123456789");
        assert_eq!(grant.user_id.as_deref(), Some("user-1"));
        assert_eq!(
            grant.refresh_token.as_deref(),
            Some("refresh-token-value-0123456789")
        );
        assert_eq!(grant.expires_at, Some(1_700_000_000));

        let seen = server.seen();
        assert_eq!(seen.len(), 2);
        for request in &seen {
            assert_eq!(request.method, "GET");
            assert!(
                request.path.starts_with("/api/v1/deviceToken/poll?"),
                "{}",
                request.path
            );
            assert!(request.path.contains("nonce=nonce-1"), "{}", request.path);
            assert!(
                request.path.contains("verifier=verifier-1"),
                "{}",
                request.path
            );
            assert!(
                request.path.contains("challenge_method=S256"),
                "{}",
                request.path
            );
            assert!(
                super::http::scripted::header(request, "authorization").is_none(),
                "poll has no user token yet"
            );
            for name in super::http::OFFICIAL_COSY_HEADERS {
                let value = super::http::scripted::header(request, name);
                assert!(value.is_some(), "missing {name}");
            }
            assert_eq!(
                super::http::scripted::header(request, "Cosy-MachineToken"),
                Some("machine-token")
            );
            assert_eq!(
                super::http::scripted::header(request, "Cosy-ClientType"),
                Some("0")
            );
        }
    }

    #[tokio::test]
    async fn qoder_poll_timeout_cancel_and_auth_stop() {
        let expired = poll_until(
            &client(),
            "http://127.0.0.1:1",
            "n",
            "v",
            &QoderMachine::default(),
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
        let server = super::http::scripted::ScriptedHttp::start(vec![(404, String::new())]);
        let cancelled = poll_until(
            &client(),
            &server.base,
            "n",
            "v",
            &machine(),
            &PollClock {
                interval: Duration::ZERO,
                deadline: Instant::now() + Duration::from_secs(5),
            },
            || hits.fetch_add(1, Ordering::SeqCst) > 0,
        )
        .await
        .expect_err("cancel");
        assert!(cancelled.to_string().contains("取消"), "{cancelled}");
        assert_eq!(server.seen().len(), 1, "one pending poll then cancel");

        let immediate = poll_until(
            &client(),
            "http://127.0.0.1:1",
            "n",
            "v",
            &QoderMachine::default(),
            &PollClock {
                interval: Duration::ZERO,
                deadline: Instant::now() + Duration::from_secs(5),
            },
            || true,
        )
        .await
        .expect_err("immediate cancel");
        assert!(immediate.to_string().contains("取消"), "{immediate}");

        let denied = super::http::scripted::ScriptedHttp::start(vec![(401, "nope".into())]);
        let auth = poll_until(
            &client(),
            &denied.base,
            "n",
            "v",
            &machine(),
            &PollClock {
                interval: Duration::ZERO,
                deadline: Instant::now() + Duration::from_secs(5),
            },
            || false,
        )
        .await
        .expect_err("401");
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let business = super::http::scripted::ScriptedHttp::start(vec![(
            200,
            r#"{"code":1001,"message":"no"}"#.into(),
        )]);
        let err = poll_until(
            &client(),
            &business.base,
            "n",
            "v",
            &machine(),
            &PollClock {
                interval: Duration::ZERO,
                deadline: Instant::now() + Duration::from_secs(5),
            },
            || false,
        )
        .await
        .expect_err("business");
        assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
        assert!(!err.is_transient());

        let flaky = super::http::scripted::ScriptedHttp::start(vec![(503, "down".into())]);
        let transient = poll_until(
            &client(),
            &flaky.base,
            "n",
            "v",
            &machine(),
            &PollClock {
                interval: Duration::from_millis(20),
                deadline: Instant::now() + Duration::from_millis(50),
            },
            || false,
        )
        .await
        .expect_err("503 then deadline");
        assert!(
            matches!(transient, UsageError::Transient(_)),
            "{transient:?}"
        );
    }
}
