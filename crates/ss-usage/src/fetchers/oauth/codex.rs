//! Codex (ChatGPT/OpenAI) OAuth fetcher.
//!
//! Mirrors the official Codex CLI PKCE + localhost callback flow:
//! 1. Generate PKCE pair + state.
//! 2. Bind `http://localhost:{port}/auth/callback` (1455, fallback 1457).
//! 3. Open `https://auth.openai.com/oauth/authorize` with Codex-specific params.
//! 4. Local server catches `?code=...&state=...`.
//! 5. POST `https://auth.openai.com/oauth/token` (form-encoded) to swap.
//! 6. `id_token` JWT carries `chatgpt_plan_type`. `access_token` is used to
//!    call `https://chatgpt.com/backend-api/wham/usage` (with header
//!    `ChatGPT-Account-Id` extracted from JWT).

#[path = "codex_reset.rs"]
mod reset_cards;
pub(crate) use reset_cards::consume as consume_reset;

use chrono::Utc;
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;

use super::common::SubscriptionBuilder;
use crate::crypto;
use crate::local_import::upsert_oauth_subscription;
use crate::oauth::local_server::{self, CallbackSession};
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_endpoint::{self, TokenResponse};
use crate::oauth::token_refresh;
use crate::oauth_clients;
use crate::storage;
use crate::subscription::{
    CODEX_CREDITS, CODEX_CREDITS_UNLIMITED, CreditInfo, Subscription, SubscriptionUsage, UsageUnit,
    UsageWindow,
};
use crate::tool_paths::codex_auth_path;
use crate::urlencode;
use crate::{UsageError, UsageResult};

static CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    oauth_clients::client_id!(
        "codex",
        "SKILLSTAR_CODEX_CLIENT_ID",
        "app_EMoamEEZ73f0CkXaXp7hrann"
    )
});
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const DEFAULT_CALLBACK_PORT: u16 = 1455;
const FALLBACK_CALLBACK_PORT: u16 = 1457;
const SCOPES: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const ORIGINATOR: &str = "codex_cli_rs";

#[derive(Debug, Deserialize, Default)]
struct UsageResponse {
    #[serde(default)]
    plan_type: Option<String>,
    #[serde(default)]
    rate_limit: Option<RateLimit>,
    #[serde(default)]
    credits: Option<CreditsBalance>,
}

#[derive(Debug, Deserialize, Default)]
struct CreditsBalance {
    #[serde(default)]
    unlimited: bool,
    #[serde(default)]
    balance: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
struct RateLimit {
    #[serde(default)]
    primary_window: Option<Window>,
    #[serde(default)]
    secondary_window: Option<Window>,
}

#[derive(Debug, Deserialize, Default)]
struct Window {
    #[serde(default)]
    used_percent: Option<i32>,
    #[serde(default)]
    limit_window_seconds: Option<i64>,
    #[serde(default)]
    reset_at: Option<i64>,
}

pub async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::OAuthStartInfo> {
    let pkce = PkcePair::generate();
    let state = crate::oauth::pkce::random_state();
    let session = local_server::start_session(DEFAULT_CALLBACK_PORT, Some(FALLBACK_CALLBACK_PORT))?;
    let port = session.port;
    let redirect_uri = format!("http://localhost:{port}/auth/callback");
    let auth_url = build_authorize_url(&redirect_uri, &pkce, &state);

    let pending_id = crate::oauth::pending_state::register_with_callback_port(
        "codex",
        None,
        auth_url.clone(),
        Some(port),
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );

    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let redirect = redirect_uri.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let target_subscription_id = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(
            session,
            state_for_task,
            verifier,
            redirect,
            target_subscription_id,
        )
        .await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id))
}

fn build_authorize_url(redirect_uri: &str, pkce: &PkcePair, state: &str) -> String {
    let params = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID.as_str()),
        ("redirect_uri", redirect_uri),
        ("scope", SCOPES),
        ("code_challenge", pkce.challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", ORIGINATOR),
    ];
    let qs = params
        .into_iter()
        .map(|(k, v)| format!("{k}={}", urlencode::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{AUTHORIZE_URL}?{qs}")
}

async fn drive_login(
    session: CallbackSession,
    state: String,
    verifier: String,
    redirect_uri: String,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let params = local_server::wait(session, state, Some(Duration::from_secs(300))).await?;
    let code = local_server::callback_code(&params)?;
    let tokens = exchange_code(&code, &verifier, &redirect_uri).await?;
    // Hold the catalog lock across the whole write so a queued Codex refresh
    // cannot patch stale credentials over the pair we just minted.
    crate::refresh_guard::with_catalog_lock("codex", || async {
        finalize(tokens, target_subscription_id.as_deref()).await
    })
    .await?
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> UsageResult<TokenResponse> {
    token_endpoint::post_token(
        TOKEN_URL,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLIENT_ID.as_str()),
            ("code_verifier", verifier),
        ],
        "Codex token",
    )
    .await
}

async fn finalize(
    tokens: TokenResponse,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let access_token = tokens
        .access_token()
        .ok_or_else(|| UsageError::Other("Codex 缺少 access_token".into()))?
        .to_string();
    let id_token = tokens
        .id_token()
        .ok_or_else(|| UsageError::Other("Codex 缺少 id_token".into()))?
        .to_string();
    let account_id = account_id_from_id_token(&id_token);

    // Card title = login email (logo already says Codex). Fallback only when claim missing.
    // `oauth_account_id` stays the ChatGPT account id used by the usage API header.
    let email = super::common::email_from_jwt(&id_token);
    let display_name = email.unwrap_or_else(|| "Codex".to_string());
    let mut sub = SubscriptionBuilder::new(
        "codex",
        display_name,
        "USD",
        &access_token,
        tokens.expires_at(),
    )
    .refresh_token(tokens.refresh_token().map(str::to_string))
    .id_token(Some(id_token))
    .oauth_account_id(account_id.clone())
    .build();

    // Re-authorizing an existing card must land back on that card, not add a
    // second one next to it (docs/features/usage/README.md).
    if let Some(existing) = super::common::reauth_target("codex", target_subscription_id) {
        super::common::carry_over_user_metadata(&mut sub, &existing, CODEX_TITLE_PLACEHOLDERS);
    }

    if let Ok(usage) = fetch_with_token(&sub.id, &access_token, account_id.as_deref()).await {
        storage::save_usage_snapshot(usage).ok();
    }
    let saved = storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Codex 订阅保存失败：{}", e)))?;
    Ok(saved)
}

/// ChatGPT account id from the OIDC id token, trying the namespaced claim
/// first and falling back to the bare claim then `sub`.
fn account_id_from_id_token(id_token: &str) -> Option<String> {
    token_refresh::jwt_string(
        id_token,
        &["https://api.openai.com/auth", "chatgpt_account_id"],
    )
    .or_else(|| token_refresh::jwt_string(id_token, &["chatgpt_account_id"]))
    .or_else(|| token_refresh::jwt_string(id_token, &["sub"]))
}

super::common::impl_oauth_fetch!();

const CODEX_TITLE_PLACEHOLDERS: &[&str] = &["Codex"];

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    if token_refresh::needs_refresh(subscription.access_token_expires_at) {
        refresh_codex_tokens(subscription).await?;
    }
    maybe_upgrade_codex_title(subscription);
    let access_token =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    let account_id = subscription.oauth_account_id.clone();
    match fetch_with_token(&subscription.id, &access_token, account_id.as_deref()).await {
        Err(UsageError::AuthRequired) => {
            refresh_codex_tokens(subscription).await?;
            maybe_upgrade_codex_title(subscription);
            let access_token = crate::fetchers::decrypt_required(
                &subscription.access_token_encrypted,
                "access_token",
            )?;
            fetch_with_token(
                &subscription.id,
                &access_token,
                subscription.oauth_account_id.as_deref(),
            )
            .await
        }
        other => other,
    }
}

fn maybe_upgrade_codex_title(subscription: &mut Subscription) {
    let email = subscription
        .id_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|s| !s.is_empty())
        .and_then(|jwt| super::common::email_from_jwt(&jwt))
        .or_else(|| {
            subscription
                .oauth_account_id
                .as_deref()
                .filter(|s| super::common::looks_like_email(s))
                .map(str::to_string)
        });
    super::common::apply_email_title(subscription, email.as_deref(), CODEX_TITLE_PLACEHOLDERS);
}

async fn refresh_codex_tokens(subscription: &mut Subscription) -> UsageResult<()> {
    let rt_cipher = subscription
        .refresh_token_encrypted
        .as_deref()
        .ok_or(UsageError::AuthRequired)?;
    let refresh = crypto::decrypt(rt_cipher);
    if refresh.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    // `post_token` is what keeps an OpenAI 5xx or 429 from being read as a
    // revoked grant: only 401 / `invalid_grant` reaches `AuthRequired`.
    let tokens = token_endpoint::post_token(
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", CLIENT_ID.as_str()),
        ],
        "Codex refresh",
    )
    .await?;
    let access_token = tokens.access_token().ok_or(UsageError::AuthRequired)?;
    subscription.access_token_encrypted = Some(crypto::encrypt(access_token));
    if let Some(rt) = tokens.refresh_token() {
        subscription.refresh_token_encrypted = Some(crypto::encrypt(rt));
    }
    subscription.access_token_expires_at = tokens.expires_at();
    if let Some(id_token) = tokens.id_token() {
        subscription.id_token_encrypted = Some(crypto::encrypt(id_token));
        if let Some(email) = super::common::email_from_jwt(id_token) {
            super::common::apply_email_title(subscription, Some(&email), CODEX_TITLE_PLACEHOLDERS);
        }
        if let Some(account_id) = account_id_from_id_token(id_token) {
            subscription.oauth_account_id = Some(account_id);
        }
    }
    Ok(())
}

async fn fetch_with_token(
    subscription_id: &str,
    access_token: &str,
    account_id: Option<&str>,
) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;
    let mut req = client.get(USAGE_URL).bearer_auth(access_token);
    if let Some(account) = account_id {
        req = req.header("ChatGPT-Account-Id", account);
    }
    let resp = req
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::transport("Codex wham/usage", e))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::http_status(
            "Codex wham/usage",
            status.as_u16(),
            &body,
        ));
    }
    let body: UsageResponse = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Codex 解析 usage: {}", e)))?;

    let plan_name = body.plan_type.clone().unwrap_or_else(|| "FREE".to_string());
    let (hourly, weekly) = body
        .rate_limit
        .as_ref()
        .map(rate_limit_windows)
        .unwrap_or((None, None));

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(plan_name),
        hourly,
        weekly,
        monthly: None,
        balance: None,
        credits: {
            let mut credits = credit_balance(body.credits.as_ref())
                .into_iter()
                .collect::<Vec<_>>();
            credits.extend(
                reset_cards::fetch(access_token, account_id)
                    .await
                    .unwrap_or_default(),
            );
            credits
        },
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    })
}

#[derive(Debug, Deserialize, Default)]
struct CodexAuthFile {
    #[serde(default)]
    tokens: Option<CodexAuthTokens>,
}

#[derive(Debug, Deserialize, Default)]
struct CodexAuthTokens {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

pub(crate) async fn import_from_local() -> UsageResult<Subscription> {
    let path = codex_auth_path();
    if !path.exists() {
        return Err(UsageError::Other("未找到 ~/.codex/auth.json".into()));
    }
    let content = std::fs::read_to_string(&path).map_err(UsageError::Io)?;
    let auth: CodexAuthFile = serde_json::from_str(&content).map_err(UsageError::Serde)?;

    let tokens = auth.tokens.ok_or_else(|| {
        UsageError::Other("auth.json 缺少 tokens，请先在 Codex CLI 完成 OAuth".into())
    })?;
    let access_token = tokens
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UsageError::Other("auth.json tokens 缺少 access_token".into()))?;

    let display_name = tokens
        .id_token
        .as_deref()
        .and_then(|jwt| token_refresh::jwt_string(jwt, &["email", "preferred_username"]))
        .unwrap_or_else(|| "Codex".to_string());

    let expires_at = tokens
        .id_token
        .as_deref()
        .and_then(token_refresh::jwt_exp)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    upsert_oauth_subscription(
        "codex",
        display_name,
        access_token,
        tokens.refresh_token,
        expires_at,
        "USD",
        None,
    )
    .await
}

/// Slot the two wham windows onto hourly/weekly by their reported length:
/// ChatGPT Pro accounts serve the 7-day window as `primary` with a null
/// `secondary`, so slotting by position would label a window that resets in
/// days as "5h". Windows without `limit_window_seconds` keep the legacy
/// positional slots (primary=hourly, secondary=weekly).
fn rate_limit_windows(rl: &RateLimit) -> (Option<UsageWindow>, Option<UsageWindow>) {
    let mut hourly = None;
    let mut weekly = None;
    for (raw, fallback_label) in [
        (rl.primary_window.as_ref(), "5h"),
        (rl.secondary_window.as_ref(), "7d"),
    ] {
        let Some(raw) = raw else { continue };
        let (preferred, other) = if raw.limit_window_seconds.is_some_and(|s| s >= 86_400) {
            (&mut weekly, &mut hourly)
        } else {
            (&mut hourly, &mut weekly)
        };
        if preferred.is_none() {
            *preferred = window(raw, fallback_label);
        } else if other.is_none() {
            *other = window(raw, fallback_label);
        }
    }
    (hourly, weekly)
}

/// Prepaid Codex credits from the same usage payload. Reset cards stay on
/// their own endpoint. An empty balance with `unlimited: false` adds no row.
fn credit_balance(credits: Option<&CreditsBalance>) -> Option<CreditInfo> {
    let credits = credits?;
    if credits.unlimited {
        return Some(CreditInfo {
            credit_type: CODEX_CREDITS.to_string(),
            credit_amount: Some(CODEX_CREDITS_UNLIMITED.to_string()),
            minimum_credit_amount_for_usage: None,
        });
    }
    let balance = credits.balance.as_ref().and_then(balance_text)?;
    Some(CreditInfo {
        credit_type: CODEX_CREDITS.to_string(),
        credit_amount: Some(balance),
        minimum_credit_amount_for_usage: None,
    })
}

fn balance_text(value: &serde_json::Value) -> Option<String> {
    let text = match value {
        serde_json::Value::String(text) => text.trim().to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn window(w: &Window, fallback_label: &str) -> Option<UsageWindow> {
    let percent = w.used_percent?;
    Some(UsageWindow {
        label: window_label(w.limit_window_seconds, fallback_label),
        used: percent as i64,
        total: Some(100),
        percent: Some(percent),
        reset_at: w.reset_at,
        breakdown: Vec::new(),

        unit: UsageUnit::Count,
    })
}

/// Display label from the window length the API reports (`18000` → `5h`,
/// `604800` → `7d`); the positional fallback only covers a missing length.
fn window_label(seconds: Option<i64>, fallback: &str) -> String {
    let Some(seconds) = seconds.filter(|s| *s > 0) else {
        return fallback.to_string();
    };
    if seconds % 86_400 == 0 {
        format!("{}d", seconds / 86_400)
    } else if seconds % 3_600 == 0 {
        format!("{}h", seconds / 3_600)
    } else if seconds % 60 == 0 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::pkce::PkcePair;

    fn parse_windows(raw: &str) -> (Option<UsageWindow>, Option<UsageWindow>) {
        let body: UsageResponse = serde_json::from_str(raw).expect("fixture is valid JSON");
        body.rate_limit
            .as_ref()
            .map(rate_limit_windows)
            .unwrap_or((None, None))
    }

    #[test]
    fn authorize_url_matches_codex_cli_params() {
        let pkce = PkcePair::generate();
        let url = build_authorize_url("http://localhost:1455/auth/callback", &pkce, "state-abc");
        assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
        assert!(url.contains("codex_cli_simplified_flow=true"));
        assert!(url.contains("id_token_add_organizations=true"));
        assert!(url.contains("originator=codex_cli_rs"));
        assert!(url.contains("api.connectors.read"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("state=state-abc"));
    }

    #[test]
    fn weekly_primary_window_lands_in_weekly_slot() {
        // Shape served to ChatGPT Pro accounts (live 2026-10): the 7-day
        // budget is the primary window and there is no secondary.
        let (hourly, weekly) = parse_windows(
            r#"{"rate_limit":{"primary_window":{"used_percent":30,"limit_window_seconds":604800,
                "reset_after_seconds":317427,"reset_at":1791594863},"secondary_window":null}}"#,
        );
        assert!(hourly.is_none());
        let weekly = weekly.expect("weekly window");
        assert_eq!(weekly.label, "7d");
        assert_eq!(weekly.percent, Some(30));
        assert_eq!(weekly.reset_at, Some(1791594863));
    }

    #[test]
    fn five_hour_primary_and_weekly_secondary_keep_their_slots() {
        let (hourly, weekly) = parse_windows(
            r#"{"rate_limit":{"primary_window":{"used_percent":10,"limit_window_seconds":18000},
                "secondary_window":{"used_percent":40,"limit_window_seconds":604800}}}"#,
        );
        assert_eq!(hourly.expect("hourly window").label, "5h");
        assert_eq!(weekly.expect("weekly window").label, "7d");
    }

    #[test]
    fn missing_window_seconds_falls_back_to_positional_labels() {
        let (hourly, weekly) = parse_windows(
            r#"{"rate_limit":{"primary_window":{"used_percent":10},
                "secondary_window":{"used_percent":40}}}"#,
        );
        assert_eq!(hourly.expect("hourly window").label, "5h");
        assert_eq!(weekly.expect("weekly window").label, "7d");
    }

    #[test]
    fn credits_balance_is_a_card_row_and_unlimited_wins() {
        let row = |raw| {
            let body: UsageResponse = serde_json::from_str(raw).expect("fixture");
            credit_balance(body.credits.as_ref())
        };
        assert_eq!(
            row(r#"{"credits":{"has_credits":true,"unlimited":false,"balance":"25"}}"#)
                .unwrap()
                .credit_amount
                .as_deref(),
            Some("25")
        );
        assert_eq!(
            row(r#"{"credits":{"balance":0}}"#)
                .unwrap()
                .credit_amount
                .as_deref(),
            Some("0")
        );
        assert_eq!(
            row(r#"{"credits":{"balance":12.5}}"#)
                .unwrap()
                .credit_amount
                .as_deref(),
            Some("12.5")
        );
        assert_eq!(
            row(
                r#"{"credits":{"has_credits":true,"unlimited":false,"balance":"58654.7347730000"}}"#
            )
            .unwrap()
            .credit_amount
            .as_deref(),
            Some("58654.7347730000")
        );
        assert_eq!(
            row(r#"{"credits":{"unlimited":true,"balance":"0"}}"#)
                .unwrap()
                .credit_amount
                .as_deref(),
            Some(CODEX_CREDITS_UNLIMITED)
        );
        assert!(row(r#"{"credits":{"has_credits":false}}"#).is_none());
        assert!(row("{}").is_none());
        assert!(
            !row(r#"{"credits":{"balance":"9"}}"#)
                .unwrap()
                .is_reset_card()
        );
    }

    #[test]
    fn window_label_formats_reported_durations() {
        assert_eq!(window_label(Some(604_800), "5h"), "7d");
        assert_eq!(window_label(Some(18_000), "5h"), "5h");
        assert_eq!(window_label(Some(900), "5h"), "15m");
        assert_eq!(window_label(Some(45), "5h"), "45s");
        assert_eq!(window_label(None, "7d"), "7d");
        assert_eq!(window_label(Some(0), "7d"), "7d");
    }
}
