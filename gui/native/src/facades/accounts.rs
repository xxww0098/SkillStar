//! Accounts page facade — the native half of `src/pages/accounts/bridge.ts`
//! (`Accounts` in TSX). The domain is `ss-accounts`, whose facade is:
//! `list` / `refresh` / `activate` / `remove` / `reconcile` /
//! `add_from_token` / `add_from_local` / `start_login` /
//! `submit_login_callback` / `finish_login` / `cancel_login`. That crate owns
//! the account record, the encrypted store, the per-agent client switch
//! writers and the login/quota orchestration (including OAuth and usage);
//! this file only narrows to JSON.
//!
//! Credentials never cross the wire: the UI sees `AccountView` (serde
//! camelCase, carrying its `UsageSnapshot`) and `AgentLive`, nothing else.
//!
//! UI-thread courtesy follows the other facades here: the cheap local read
//! (`list`) runs inline; the network and client-credential work (`refresh`,
//! `activate`, `delete`, `finish_login`, both imports) rides the blocking
//! pool — the same work the GPUI page's `spawn_domain` jobs ran.
//!
//! `start_login` and `submit_oauth_callback` are the deliberate exceptions:
//! both settle their validation branches (unknown agent, unknown pending id)
//! **before** any pool hop or socket, so a bad call rejects on the first poll
//! — pinned by the tests at the bottom of this file.
//!
//! Errors reject as the domain's own message (`err.to_string()`), exactly
//! like every other facade in this crate.

use nsh_core::JsonValue;
use nsh_core::element::__private::JsonMap;
use nsh_core::prelude::*;
use ss_accounts::{AccountError, AccountResult, AccountView, Agent, StartOptions};

/// 单个受管 agent 的展示信息与登录能力。
fn agent_row(agent: Agent) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert("id".to_owned(), JsonValue::String(agent.id().to_owned()));
    map.insert(
        "displayName".to_owned(),
        JsonValue::String(agent.display_name().to_owned()),
    );
    map.insert(
        "supportsOauth".to_owned(),
        JsonValue::Bool(agent.supports_oauth()),
    );
    map.insert(
        "localSource".to_owned(),
        JsonValue::String(agent.local_source().to_owned()),
    );
    if let Some(modes) = oauth_modes(agent) {
        map.insert("oauthModes".to_owned(), modes);
    }
    JsonValue::Object(map)
}

fn oauth_modes(agent: Agent) -> Option<JsonValue> {
    match agent {
        Agent::Kiro => Some(serde_json::json!([
            { "id": "portal", "label": "Kiro Portal", "optionKey": "leg", "fields": [] },
            { "id": "idc", "label": "AWS IdC", "optionKey": "leg", "fields": ["idcRegion", "idcStartUrl"] }
        ])),
        Agent::Zcode => Some(serde_json::json!([
            { "id": "zai", "label": "Z.ai", "optionKey": "provider", "fields": [] },
            { "id": "bigmodel", "label": "BigModel", "optionKey": "provider", "fields": [] }
        ])),
        _ => None,
    }
}

fn option_text(map: &JsonMap<String, JsonValue>, keys: &[&str]) -> AccountResult<Option<String>> {
    for key in keys {
        let Some(value) = map.get(*key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let Some(raw) = value.as_str() else {
            return Err(AccountError::Client(format!(
                "start_login option `{key}` must be a string"
            )));
        };
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_owned()));
        }
    }
    Ok(None)
}

fn parse_start_options(agent: Agent, options: Option<JsonValue>) -> AccountResult<StartOptions> {
    let Some(value) = options else {
        return Ok(StartOptions::default());
    };
    if value.is_null() {
        return Ok(StartOptions::default());
    }
    let JsonValue::Object(map) = value else {
        return Err(AccountError::Client(
            "start_login options must be a JSON object".to_owned(),
        ));
    };
    let mut parsed = StartOptions::default();
    if let Some(port_val) = map.get("callbackPort").or_else(|| map.get("port"))
        && !port_val.is_null()
    {
        let Some(raw) = port_val.as_u64().and_then(|n| u16::try_from(n).ok()) else {
            return Err(AccountError::Client(
                "start_login option `callbackPort` must be a valid TCP port".to_owned(),
            ));
        };
        parsed.prefer_port = Some(raw);
    }
    match agent {
        Agent::Kiro => {
            if let Some(leg) = option_text(&map, &["leg", "mode"])? {
                let normalized = leg.to_ascii_lowercase();
                match normalized.as_str() {
                    "portal" | "social" | "builder-id" => {
                        parsed.leg = Some("portal".to_owned());
                        return Ok(parsed);
                    }
                    "idc" | "aws-idc" | "awsidc" => parsed.leg = Some("idc".to_owned()),
                    _ => {
                        return Err(AccountError::Client(format!(
                            "不支持的 Kiro 登录方式：{leg}（请选择 portal 或 idc）"
                        )));
                    }
                }
            }
            parsed.region = option_text(&map, &["idcRegion", "idc_region", "region"])?;
            if let Some(url) = option_text(&map, &["idcStartUrl", "startUrl", "start_url"])? {
                if !url.starts_with("https://")
                    || url.len() <= "https://".len()
                    || url.contains(char::is_whitespace)
                {
                    return Err(AccountError::Client(
                        "IAM Identity Center Start URL 必须是有效的 https:// 地址".to_owned(),
                    ));
                }
                parsed.start_url = Some(url);
            }
        }
        Agent::Zcode => {
            if let Some(provider) = option_text(&map, &["provider", "mode", "region"])? {
                let normalized = provider.to_ascii_lowercase();
                if normalized != "zai" && normalized != "bigmodel" {
                    return Err(AccountError::Client(
                        "不支持的 ZCode 上游，请选择 zai 或 bigmodel".to_owned(),
                    ));
                }
                parsed.region = Some(normalized);
            }
        }
        _ => {
            parsed.region = option_text(&map, &["region"])?;
        }
    }
    Ok(parsed)
}

/// `AccountView` straight through (its serde is the UI contract).
fn account_json(view: &AccountView) -> Result<JsonValue, String> {
    serde_json::to_value(view).map_err(|err| err.to_string())
}

/// The accounts facade (`Accounts` in TSX).
#[module]
pub struct Accounts;

#[module]
impl Accounts {
    /// Every supported stored account, oldest first (`ss_accounts::list`).
    ///
    /// Blocking pool, not inline: the domain's list reads the account index
    /// *and* every client's own credential (a SQLite store for one agent), so
    /// holding the engine thread here would stall the first frame and eat the
    /// keystrokes that arrive before it (measured: the marketplace tests' mod+F
    /// was lost while this ran inline).
    pub async fn list() -> Result<JsonValue, String> {
        blocking(|| {
            let rows = ss_accounts::list().map_err(|err| err.to_string())?;
            serde_json::to_value(&rows).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("list task failed: {join}"))?
    }

    /// What each client is serving right now (`ss_accounts::reconcile`): one
    /// row per supported agent. An unreadable client reports an empty
    /// identity, never an error, so the page still renders. Blocking pool for
    /// the same reason as `list` (it reads the same credential files).
    pub async fn reconcile() -> Result<JsonValue, String> {
        blocking(|| {
            let rows = ss_accounts::reconcile().map_err(|err| err.to_string())?;
            serde_json::to_value(&rows).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("reconcile task failed: {join}"))?
    }

    /// Refresh one account's tokens and quota, then its fresh view
    /// (`ss_accounts::refresh`). Provider work: blocking pool.
    pub async fn refresh(id: String) -> Result<JsonValue, String> {
        blocking(move || {
            let view = ss_accounts::refresh(&id).map_err(|err| err.to_string())?;
            account_json(&view)
        })
        .await
        .map_err(|join| format!("refresh task failed: {join}"))?
    }

    /// Make the account's client serve it (`ss_accounts::activate`). Writes
    /// the client's own credential store: blocking pool.
    pub async fn activate(id: String) -> Result<JsonValue, String> {
        blocking(move || {
            let outcome = ss_accounts::activate(&id).map_err(|err| err.to_string())?;
            serde_json::to_value(&outcome).map_err(|err| err.to_string())
        })
        .await
        .map_err(|join| format!("activate task failed: {join}"))?
    }

    /// Forget one account, leaving the client as it is (`ss_accounts::remove`).
    /// Resolves `null`.
    pub async fn delete(id: String) -> Result<JsonValue, String> {
        blocking(move || ss_accounts::remove(&id).map_err(|err| err.to_string()))
            .await
            .map_err(|join| format!("delete task failed: {join}"))??;
        Ok(JsonValue::Null)
    }

    /// 受管 Agent 元数据列表，
    /// 顺序与 `ss_accounts::Agent::supported()` 一致。Pure.
    pub fn agents() -> JsonValue {
        JsonValue::Array(
            Agent::supported()
                .iter()
                .map(|agent| agent_row(*agent))
                .collect(),
        )
    }

    /// Start an OAuth login for the agent (`ss_accounts::start_login`).
    /// Resolves with the pending session the dialog renders
    /// (`loginId`/`flow`/`authUrl`/`userCode`/`verificationUri`).
    ///
    /// Optional `options`:
    /// - Kiro: `{ leg: "portal" | "idc", idcRegion?: string, idcStartUrl?: string }`
    /// - ZCode: `{ provider: "zai" | "bigmodel" }`
    ///
    /// Both the agent id and the options object are validated synchronously
    /// before any pool hop or socket is opened.
    pub async fn start_login(
        agent: String,
        options: Option<JsonValue>,
    ) -> Result<JsonValue, String> {
        let parsed_agent = Agent::from_id(&agent).map_err(|err| err.to_string())?;
        let opts = parse_start_options(parsed_agent, options).map_err(|err| err.to_string())?;
        blocking(move || start_login_body(&agent, &opts))
            .await
            .map_err(|join| format!("login task failed: {join}"))?
    }

    /// Feed a pasted callback URL or bare code into a pending login
    /// (`ss_accounts::submit_login_callback`). Inline on purpose: the delivery
    /// is a parse plus a channel send (no socket), and an unknown pending id
    /// has to reject on the first poll.
    pub async fn submit_oauth_callback(
        pending_id: String,
        text: String,
    ) -> Result<JsonValue, String> {
        ss_accounts::submit_login_callback(&pending_id, &text).map_err(|err| err.to_string())?;
        Ok(JsonValue::Null)
    }

    /// Wait for the pending login to finish and store the account
    /// (`ss_accounts::finish_login`). Blocks until the provider answers, so it
    /// rides the blocking pool; resolves with the new `AccountView`.
    pub async fn finish_login(pending_id: String) -> Result<JsonValue, String> {
        blocking(move || {
            let view = ss_accounts::finish_login(&pending_id).map_err(|err| err.to_string())?;
            account_json(&view)
        })
        .await
        .map_err(|join| format!("login task failed: {join}"))?
    }

    /// Drop a pending login (`ss_accounts::cancel_login`). Sync; unknown ids
    /// are a no-op in the domain.
    pub fn cancel_oauth_login(pending_id: String) {
        ss_accounts::cancel_login(&pending_id);
    }

    /// Add an account from a pasted token / credential JSON
    /// (`ss_accounts::add_from_token`). Verifies against the provider:
    /// blocking pool.
    pub async fn import_token(agent: String, payload: String) -> Result<JsonValue, String> {
        blocking(move || {
            let view =
                ss_accounts::add_from_token(&agent, &payload).map_err(|err| err.to_string())?;
            account_json(&view)
        })
        .await
        .map_err(|join| format!("import task failed: {join}"))?
    }

    /// Add the account the machine's own client already holds
    /// (`ss_accounts::add_from_local`). Reads the client credential, then
    /// verifies quota: blocking pool.
    pub async fn import_local(agent: String) -> Result<JsonValue, String> {
        blocking(move || {
            let view = ss_accounts::add_from_local(&agent).map_err(|err| err.to_string())?;
            account_json(&view)
        })
        .await
        .map_err(|join| format!("import task failed: {join}"))?
    }

}

/// The login start's pool body (a plain fn so the pool hop stays in the
/// method above, where the agent guard is).
fn start_login_body(agent: &str, opts: &StartOptions) -> Result<JsonValue, String> {
    let started = ss_accounts::start_login(agent, opts).map_err(|err| err.to_string())?;
    serde_json::to_value(&started).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::Accounts;

    /// Poll a facade future once, without a runtime: every branch these tests
    /// pin settles on the first poll (no pool hop, no socket), so a `Pending`
    /// here means the guard moved behind I/O.
    fn first_poll<T>(
        future: std::pin::Pin<&mut impl std::future::Future<Output = T>>,
    ) -> T {
        let mut future = future;
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => panic!("the guard must settle on the first poll"),
        }
    }

    /// `Accounts::agents()` 必须按 `ss_accounts::Agent::supported()` 原序完整透传
    /// 每个字段都来自账号域。Pure — no store, no network.
    #[test]
    fn agents_list_the_supported_rows() {
        let rows = Accounts::agents();
        let rows = rows.as_array().expect("agent rows");
        let expected = ss_accounts::Agent::supported();
        assert_eq!(rows.len(), expected.len());
        for (row, agent) in rows.iter().zip(expected.iter()) {
            assert_eq!(row["id"].as_str(), Some(agent.id()));
            assert_eq!(row["displayName"].as_str(), Some(agent.display_name()));
            assert_eq!(row["supportsOauth"].as_bool(), Some(agent.supports_oauth()));
            let local_source = row["localSource"].as_str().expect("localSource");
            assert_eq!(local_source, agent.local_source());
            assert!(
                !local_source.trim().is_empty(),
                "empty localSource for {}",
                agent.id()
            );
        }
    }

    /// 每个受管客户端都有品牌图标和技能 profile。
    #[test]
    fn every_account_agent_has_dedicated_icon_and_skills_profile() {
        let profiles = ss_skills::agents::list_profiles();
        for agent in ss_accounts::Agent::supported() {
            let id = agent.id();
            assert!(
                crate::agent_icons::has_dedicated_svg(id),
                "missing dedicated SVG asset for managed agent '{id}'"
            );
            assert!(
                ss_skills::agents::find_profile(&profiles, id).is_ok(),
                "missing ss-skills profile for managed agent '{id}'"
            );
        }
    }

    /// An unknown agent rejects on the first poll: `Agent::from_id` runs
    /// before the blocking hop, so no socket is ever opened for a typo.
    #[test]
    fn start_login_rejects_unknown_agent_on_the_first_poll() {
        let mut future = std::pin::pin!(Accounts::start_login("no-such-agent".to_owned(), None));
        let settled = first_poll(future.as_mut());
        let err = match settled {
            Err(err) => err,
            Ok(value) => panic!("unknown agent must reject, got {value:?}"),
        };
        assert!(err.contains("no-such-agent"), "{err}");
        assert!(err.contains("unsupported agent"), "{err}");
    }

    #[test]
    fn start_login_parses_kiro_idc_and_zcode_provider_options() {
        let kiro_idc = super::parse_start_options(
            ss_accounts::Agent::Kiro,
            Some(serde_json::json!({
                "leg": "idc",
                "idcRegion": "eu-west-1",
                "idcStartUrl": "https://d-123.awsapps.com/start"
            })),
        )
        .expect("kiro idc options");
        assert_eq!(kiro_idc.leg.as_deref(), Some("idc"));
        assert_eq!(kiro_idc.region.as_deref(), Some("eu-west-1"));
        assert_eq!(
            kiro_idc.start_url.as_deref(),
            Some("https://d-123.awsapps.com/start")
        );

        let kiro_portal = super::parse_start_options(
            ss_accounts::Agent::Kiro,
            Some(serde_json::json!({ "leg": "portal", "idcRegion": "eu-west-1" })),
        )
        .expect("kiro portal ignores idc params");
        assert_eq!(kiro_portal.leg.as_deref(), Some("portal"));
        assert_eq!(kiro_portal.region, None);

        let zcode_bigmodel = super::parse_start_options(
            ss_accounts::Agent::Zcode,
            Some(serde_json::json!({ "provider": "BigModel" })),
        )
        .expect("zcode bigmodel options");
        assert_eq!(zcode_bigmodel.region.as_deref(), Some("bigmodel"));

        let mut bad_leg = std::pin::pin!(Accounts::start_login(
            "kiro".to_owned(),
            Some(serde_json::json!({ "leg": "invalid" })),
        ));
        assert!(first_poll(bad_leg.as_mut()).is_err());

        let mut bad_url = std::pin::pin!(Accounts::start_login(
            "kiro".to_owned(),
            Some(serde_json::json!({ "leg": "idc", "idcStartUrl": "http://insecure.local" })),
        ));
        assert!(first_poll(bad_url.as_mut()).is_err());

        let mut bad_provider = std::pin::pin!(Accounts::start_login(
            "zcode".to_owned(),
            Some(serde_json::json!({ "provider": "unknown" })),
        ));
        assert!(first_poll(bad_provider.as_mut()).is_err());
    }

    #[test]
    fn agents_include_oauth_modes_metadata_for_multi_leg_providers() {
        let rows = Accounts::agents();
        let rows = rows.as_array().expect("agent rows");
        let kiro = rows
            .iter()
            .find(|row| row["id"].as_str() == Some("kiro"))
            .expect("kiro row");
        assert_eq!(kiro["oauthModes"][0]["id"].as_str(), Some("portal"));
        assert_eq!(kiro["oauthModes"][1]["id"].as_str(), Some("idc"));
        assert_eq!(kiro["oauthModes"][1]["fields"][0].as_str(), Some("idcRegion"));
        assert_eq!(kiro["oauthModes"][1]["fields"][1].as_str(), Some("idcStartUrl"));

        let zcode = rows
            .iter()
            .find(|row| row["id"].as_str() == Some("zcode"))
            .expect("zcode row");
        assert_eq!(zcode["oauthModes"][0]["id"].as_str(), Some("zai"));
        assert_eq!(zcode["oauthModes"][1]["id"].as_str(), Some("bigmodel"));
    }

    /// An unknown pending id rejects instead of resolving: the callback
    /// delivery path is inline, so the rejection lands on the first poll too.
    #[test]
    fn submit_oauth_callback_rejects_unknown_pending_id() {
        let mut future = std::pin::pin!(Accounts::submit_oauth_callback(
            "no-such-pending".to_owned(),
            "http://127.0.0.1:1455/auth/callback?code=abc".to_owned(),
        ));
        let settled = first_poll(future.as_mut());
        let err = settled.err().expect("an unknown pending id cannot resolve");
        assert!(!err.trim().is_empty(), "the page renders this message");
    }
}
