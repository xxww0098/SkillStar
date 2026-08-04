use std::collections::HashMap;

use super::*;
use crate::test_support::{ENV_LOCK, EnvGuard};
use skillstar_usage::subscription::{SubscriptionUsage, UsageWindow};

// ── fixtures ──────────────────────────────────────────────────────────

fn stored(id: &str, catalog_id: &str) -> Subscription {
    Subscription {
        id: id.into(),
        catalog_id: catalog_id.into(),
        display_name: "账号一".into(),
        auth_mode: AuthMode::ApiKey,
        plan_tier: None,
        monthly_price: None,
        currency: "CNY".into(),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: None,
        refresh_token_encrypted: None,
        access_token_expires_at: None,
        id_token_encrypted: None,
        oauth_account_id: None,
        oauth_region: None,
        requires_reauth: false,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: 0,
        updated_at: 0,
    }
}

fn create_input(catalog_id: &str, auth_mode: AuthMode) -> CreateSubscriptionInput {
    CreateSubscriptionInput {
        catalog_id: catalog_id.into(),
        display_name: None,
        auth_mode,
        plan_tier: None,
        monthly_price: None,
        currency: None,
        billing_cycle: None,
        start_date: None,
        renew_date: None,
        auto_renew: None,
        api_key: None,
        platform_token: None,
        oauth_region: None,
        manual_quota: None,
        note: None,
        cookie_header: None,
    }
}

fn empty_update() -> UpdateSubscriptionInput {
    UpdateSubscriptionInput {
        display_name: None,
        plan_tier: None,
        monthly_price: None,
        currency: None,
        billing_cycle: None,
        start_date: None,
        renew_date: None,
        auto_renew: None,
        api_key: None,
        platform_token: None,
        clear_platform_token: false,
        manual_quota: None,
        note: None,
        cookie_header: None,
    }
}

fn snapshot_with_window(sub_id: &str, label: &str, used_percent: i32) -> SubscriptionUsage {
    SubscriptionUsage {
        subscription_id: sub_id.into(),
        fetched_at: Utc::now().timestamp(),
        monthly: Some(UsageWindow {
            label: label.into(),
            used: 0,
            total: None,
            percent: Some(used_percent),
            reset_at: None,
            breakdown: Vec::new(),
        }),
        ..Default::default()
    }
}

// ── pure helpers (no storage) ─────────────────────────────────────────

#[test]
fn network_hint_targets_point_grok_transport_failures_at_xai() {
    assert_eq!(
        network_hint_targets("Grok token: error sending request for url (https://auth.x.ai)"),
        "x.ai / Grok"
    );
    assert_eq!(
        network_hint_targets("dns error: failed to lookup address"),
        "Google / GitHub 等海外服务"
    );
}

#[test]
fn non_network_and_already_hinted_errors_pass_through_unchanged() {
    // Domain errors (auth, quota, HTTP status) must not gain proxy guidance.
    let auth_error = "HTTP 401 Unauthorized: token revoked".to_string();
    assert_eq!(append_network_hint(auth_error.clone()), auth_error);

    // A message that already carries proxy guidance must not be double-hinted.
    let hinted = "connection refused。请在设置 > 网络代理检查配置".to_string();
    assert_eq!(append_network_hint(hinted.clone()), hinted);
}

#[test]
fn mark_credentials_rotated_clears_auth_expired_latch() {
    let mut sub = stored("s1", "kimi");
    sub.requires_reauth = true;
    sub.cookie_session_expires_at = Some(123);

    mark_credentials_rotated(&mut sub);

    assert!(!sub.requires_reauth);
    assert_eq!(sub.cookie_session_expires_at, None);
}

#[test]
fn fill_active_flags_only_the_pinned_row_of_its_catalog() {
    let dto = || SubscriptionDto::from_parts(stored("s1", "kimi"), None);

    let pinned = HashMap::from([("kimi".to_string(), "s1".to_string())]);
    assert!(fill_active(dto(), &pinned).is_active);

    let sibling_pinned = HashMap::from([("kimi".to_string(), "s2".to_string())]);
    assert!(!fill_active(dto(), &sibling_pinned).is_active);

    assert!(!fill_active(dto(), &HashMap::new()).is_active);
}

// ── behavior against isolated storage ─────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn transport_errors_gain_proxy_guidance_matching_proxy_config() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    // Without a proxy config the hint tells the user the proxy is off.
    let hinted = append_network_hint("Grok token: error sending request".into());
    assert!(hinted.starts_with("Grok token: error sending request。"));
    assert!(hinted.contains("未启用"), "{hinted}");
    assert!(hinted.contains("x.ai / Grok"), "{hinted}");

    // With an enabled proxy the hint names the configured endpoint instead.
    proxy::save_config(&proxy::ProxyConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port: 7897,
        ..Default::default()
    })
    .unwrap();
    let hinted = append_network_hint("connection refused".into());
    assert!(hinted.contains("http://127.0.0.1:7897"), "{hinted}");
    assert!(hinted.contains("Google / GitHub 等海外服务"), "{hinted}");
}

#[tokio::test(flavor = "current_thread")]
async fn create_rejects_unknown_catalog_and_auth_mode_outside_whitelist() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let err = create_subscription(create_input("no-such-provider", AuthMode::Manual)).unwrap_err();
    assert!(err.to_string().contains("unknown catalog id"), "{err}");

    // kimi is API-key only — OAuth must be refused, and nothing persisted.
    let err = create_subscription(create_input("kimi", AuthMode::OAuth)).unwrap_err();
    assert!(err.to_string().contains("不支持"), "{err}");
    assert!(list_subscriptions().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn create_fills_catalog_defaults_and_pins_only_the_first_account() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let mut input = create_input("kimi", AuthMode::ApiKey);
    input.display_name = Some("   ".into()); // blank → catalog display name
    input.api_key = Some("sk-live-1".into());
    let first = create_subscription(input).unwrap();

    assert_eq!(first.display_name, "Kimi");
    assert_eq!(first.currency, "CNY");
    assert_eq!(first.billing_cycle, BillingCycle::Monthly);
    assert!(first.has_credential);
    assert!(first.is_active, "first account of a catalog is auto-pinned");

    // The plaintext key never appears in the DTO but survives a round trip.
    assert_eq!(
        get_subscription_api_key(first.id.clone())
            .unwrap()
            .as_deref(),
        Some("sk-live-1")
    );

    let second = create_subscription(create_input("kimi", AuthMode::ApiKey)).unwrap();
    assert!(!second.is_active, "a later account must not steal the pin");
    assert!(!second.has_credential);
    assert_eq!(get_subscription_api_key(second.id.clone()).unwrap(), None);
    assert_eq!(
        get_active_subscriptions().unwrap().get("kimi"),
        Some(&first.id)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cookie_paste_without_name_value_pairs_is_rejected_with_guidance() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let mut input = create_input("stepfun", AuthMode::Cookie);
    input.cookie_header = Some("Cookie:".into());
    let err = create_subscription(input).unwrap_err();
    assert!(err.to_string().contains("Cookie 解析失败"), "{err}");

    // A real header parses even with the `Cookie:` label still attached.
    let mut input = create_input("stepfun", AuthMode::Cookie);
    input.cookie_header = Some("Cookie: Oasis-Token=tok-1; session=abc".into());
    let dto = create_subscription(input).unwrap();
    assert!(dto.has_credential);
    let saved = storage::get_subscription(&dto.id).unwrap();
    let jar = crypto::decrypt(saved.cookie_jar_encrypted.as_deref().unwrap());
    assert!(jar.contains("Oasis-Token"), "{jar}");
    assert!(!jar.contains("Cookie:"), "label must be stripped: {jar}");
}

#[tokio::test(flavor = "current_thread")]
async fn rotating_api_key_clears_reauth_latch_and_blank_name_is_ignored() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let mut sub = stored("kimi-1", "kimi");
    sub.requires_reauth = true;
    sub.cookie_session_expires_at = Some(99);
    storage::upsert_subscription(sub).unwrap();

    let mut input = empty_update();
    input.display_name = Some("   ".into());
    input.api_key = Some("sk-rotated".into());
    input.note = Some("换了新 key".into());
    let dto = update_subscription("kimi-1".into(), input).await.unwrap();

    assert!(!dto.requires_reauth, "new credentials must drop the latch");
    assert_eq!(dto.display_name, "账号一", "blank rename must be ignored");
    assert_eq!(dto.note.as_deref(), Some("换了新 key"));
    assert_eq!(
        get_subscription_api_key("kimi-1".into())
            .unwrap()
            .as_deref(),
        Some("sk-rotated")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn delete_removes_row_snapshot_and_active_pin_together() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let dto = create_subscription(create_input("kimi", AuthMode::ApiKey)).unwrap();
    storage::save_usage_snapshot(snapshot_with_window(&dto.id, "30d", 50)).unwrap();
    assert!(dto.is_active);

    delete_subscription(dto.id.clone()).await.unwrap();

    assert!(list_subscriptions().unwrap().is_empty());
    assert!(storage::get_usage_snapshot(&dto.id).unwrap().is_none());
    assert!(get_active_subscriptions().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn usage_summary_folds_billing_cycles_into_monthly_spend_per_currency() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let mut monthly_usd = stored("a", "codex");
    monthly_usd.currency = "USD".into();
    monthly_usd.monthly_price = Some(20.0);
    let mut annual_usd = stored("b", "cursor");
    annual_usd.currency = "USD".into();
    annual_usd.monthly_price = Some(120.0);
    annual_usd.billing_cycle = BillingCycle::Annual;
    let mut prepaid_cny = stored("c", "deepseek");
    prepaid_cny.monthly_price = Some(999.0);
    prepaid_cny.billing_cycle = BillingCycle::ApiKey;
    let mut monthly_cny = stored("d", "kimi");
    monthly_cny.monthly_price = Some(50.0);
    monthly_cny.requires_reauth = true;
    for sub in [monthly_usd, annual_usd, prepaid_cny, monthly_cny] {
        storage::upsert_subscription(sub).unwrap();
    }

    let summary = get_usage_summary().unwrap();

    let spend: HashMap<String, f64> = summary
        .monthly_spend
        .iter()
        .map(|entry| (entry.currency.clone(), entry.amount))
        .collect();
    assert_eq!(spend["USD"], 30.0, "annual price must amortize to /12");
    assert_eq!(spend["CNY"], 50.0, "prepaid balance is not monthly burn");
    assert_eq!(summary.total_subscriptions, 4);
    assert_eq!(summary.reauth_count, 1);
    assert_eq!(summary.alert_count, 1, "the reauth row raises one alert");
}

#[tokio::test(flavor = "current_thread")]
async fn dock_menu_lines_order_most_urgent_first_with_catalog_fallback() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let mut roomy = stored("a", "codex");
    roomy.display_name = "Alpha".into();
    let mut nameless = stored("b", "kimi");
    nameless.display_name = "  ".into();
    let no_snapshot = stored("c", "glm");
    for sub in [roomy, nameless, no_snapshot] {
        storage::upsert_subscription(sub).unwrap();
    }
    storage::save_usage_snapshot(snapshot_with_window("a", "30d", 30)).unwrap();
    storage::save_usage_snapshot(snapshot_with_window("b", "30d", 90)).unwrap();

    assert_eq!(
        dock_menu_lines(),
        vec![
            "kimi · 剩余 10%".to_string(),
            "Alpha · 剩余 70%".to_string()
        ],
        "least remaining first; blank names fall back to catalog id; \
         rows without a percent quota are hidden"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn refresh_all_synthesizes_usage_for_manual_accounts_without_network() {
    let _lock = ENV_LOCK.lock().await;
    let data_root = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[("SKILLSTAR_DATA_DIR", data_root.path())]);

    let mut input = create_input("stepfun", AuthMode::Manual);
    input.plan_tier = Some("Pro".into());
    create_subscription(input).unwrap();

    let dtos = refresh_all_subscriptions().await.unwrap();

    assert_eq!(dtos.len(), 1);
    let usage = dtos[0].usage.as_ref().expect("manual rows get a snapshot");
    assert_eq!(usage.plan_name.as_deref(), Some("Pro"));
    assert_eq!(usage.error, None);
}

// ── OAuth plumbing that fails fast without any pending login ──────────

#[tokio::test(flavor = "current_thread")]
async fn await_oauth_completion_rejects_unknown_pending_id() {
    let err = await_oauth_completion("no-such-pending".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("pending_id"), "{err}");
}

#[tokio::test(flavor = "current_thread")]
async fn import_from_local_rejects_catalogs_without_local_credentials() {
    let err = import_subscription_from_local("cursor".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("不支持从本地导入"), "{err}");
}
