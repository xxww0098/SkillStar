//! Wire the gateway's account book into Usage's account system, and the
//! upstream env into the listener (specs/usage-models-evolution slice 10).
//!
//! `account()` is live-first (slice 02): custody decides who the CLI is
//! serving right now, and that live/snapshot credential is the signing
//! source of truth; the stored row is only a fallback for when the CLI has
//! no usable credential. `allowance()` still reads the usage snapshots —
//! not a secret, not part of upstream signing, so its stored-read behaviour
//! is unchanged.
//!
//! `upstream_env` is the same seam from the routing side: the provider
//! store supplies candidate endpoints, the account book signs them, and the
//! winner's ledger attribution reads the subscription the credential
//! belongs to. The gateway crate never sees a store (README D7); it only
//! sees the closures handed to it here.

use skillstar_core::providers::identity::identity_for_preset;
use skillstar_gateway::key_fingerprint;
use skillstar_gateway::{AccountBook, AccountSnapshot, AllowanceSnapshot, Upstream, UpstreamEnv};
use skillstar_models::providers::load_store;
use skillstar_models::providers::Provider;
use skillstar_usage::storage;
use skillstar_usage::subscription::{Subscription, SubscriptionUsage};
use skillstar_usage::usage_switch::signing_material;

/// The generic provider-key signing shape: a literal api key as a bearer,
/// the shape sign.rs gives the key-only catalogs. A custom provider row has
/// no usage catalog of its own, so it signs through this shape; its ledger
/// attribution still carries the row's real catalog.
const PROVIDER_KEY_SHAPE: &str = "gemini-cli";

/// The env the CLI `gateway serve` and the desktop listener share. Every
/// turn calls `resolve` once and `attribute` at most once (for the winner).
pub fn upstream_env() -> UpstreamEnv {
    UpstreamEnv {
        resolve: Box::new(resolve_upstreams),
        book: Box::new(UsageAccountBook),
        attribute: Box::new(attribute_candidate),
    }
}

/// Candidate upstreams for one model ref: the provider rows whose adopted
/// model list names the ref (whole, or its model half), in store order. A
/// group ref expands to its members first. Rows without a usable OpenAI
/// endpoint — native-login seeds, most of all — contribute nothing.
pub fn resolve_upstreams(model_ref: &str) -> Vec<Upstream> {
    let refs = candidate_refs(model_ref);
    let providers = stored_providers();
    let mut candidates = Vec::new();
    for model_ref in refs {
        for provider in &providers {
            if let Some(upstream) = candidate_of(provider, &model_ref) {
                candidates.push(upstream);
            }
        }
    }
    candidates
}

/// Ledger attribution of one candidate id: the catalog column (the usage
/// catalog a preset maps to, else the row id) and the account label (the
/// winning credential's subscription id, or the row key's fingerprint).
pub fn attribute_candidate(id: &str) -> (String, String) {
    let providers = stored_providers();
    let Some(provider) = providers.iter().find(|provider| provider.id == id) else {
        return (String::new(), String::new());
    };
    let catalog = ledger_catalog(provider);
    let account = match signing_shape(provider) {
        // An account catalog attributes through the live credential.
        Shape::Account(catalog) => signing_material(&catalog)
            .and_then(|material| material.subscription_id)
            .unwrap_or_default(),
        // A provider-row key never lands itself; its fingerprint does.
        Shape::ProviderKey(key) => key.as_deref().map(key_fingerprint).unwrap_or_default(),
    };
    (catalog, account)
}

/// The refs one turn routes for: a saved group expands to its members
/// (auto groups need a model list this read does not have); anything else
/// routes as the single ref it already is.
fn candidate_refs(model_ref: &str) -> Vec<String> {
    let trimmed = model_ref.trim();
    if trimmed.starts_with("group/") {
        let members = skillstar_gateway::expand_group(trimmed, &[]);
        if !members.is_empty() {
            return members;
        }
    }
    vec![trimmed.to_string()]
}

fn stored_providers() -> Vec<Provider> {
    load_store()
        .map(|loaded| loaded.store.providers)
        .unwrap_or_default()
}

/// One provider row as one gateway candidate, when the row serves the ref
/// and names an endpoint the gateway's `/v1` paths can join onto.
fn candidate_of(provider: &Provider, model_ref: &str) -> Option<Upstream> {
    if !serves_ref(provider, model_ref) {
        return None;
    }
    let endpoint = base_origin(provider)?;
    let (catalog_id, key) = match signing_shape(provider) {
        Shape::Account(catalog) => (catalog, None),
        Shape::ProviderKey(key) => (PROVIDER_KEY_SHAPE.to_string(), key),
    };
    Some(Upstream {
        id: provider.id.clone(),
        catalog_id,
        endpoint,
        provider: key.map(|key| skillstar_gateway::ProviderSnapshot {
            api_key: Some(key),
        }),
    })
}

/// The adopted-model whitelist decides. A `provider/model` ref matches the
/// same string or its model half, so both spellings a writer may have put
/// in a file serve the row.
fn serves_ref(provider: &Provider, model_ref: &str) -> bool {
    let model_ref = model_ref.trim();
    if model_ref.is_empty() {
        return false;
    }
    let model = model_ref.split_once('/').map_or(model_ref, |(_, half)| half);
    provider
        .models
        .iter()
        .any(|listed| listed == model_ref)
        || provider.models.iter().any(|listed| listed == model)
}

/// The OpenAI endpoint as the origin the gateway appends `/v1` paths to. A
/// row that already ends in its own `/v1` has that suffix trimmed; a root
/// without one is used as it is. Rows with other shapes (a vendor's `/v4`
/// root, an Anthropic-only host) are a later mapping — the honest minimal
/// wiring only joins the OpenAI family.
fn base_origin(provider: &Provider) -> Option<String> {
    let raw = provider.endpoints.openai_chat.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    let raw = raw.trim_end_matches('/');
    let origin = raw.strip_suffix("/v1").unwrap_or(raw);
    Some(origin.to_string())
}

/// How a row signs: a native-login row whose preset maps to a usage catalog
/// signs from the account book; every other row sends its own literal key
/// through the generic provider-key shape.
fn signing_shape(provider: &Provider) -> Shape {
    if provider.is_external_cli()
        && let Some(catalog) = provider
            .preset_id
            .as_deref()
            .and_then(identity_for_preset)
            .and_then(|identity| identity.catalog_id)
    {
        return Shape::Account(catalog.to_string());
    }
    Shape::ProviderKey(
        provider
            .credential
            .literal_secret()
            .map(str::to_string),
    )
}

/// One row's signing shape: an account catalog the book answers for, or the
/// row's own api key.
enum Shape {
    Account(String),
    ProviderKey(Option<String>),
}

/// The ledger catalog of a row: the usage catalog its preset maps to, the
/// identity's canonical id for models-only presets, else the row id.
fn ledger_catalog(provider: &Provider) -> String {
    provider
        .preset_id
        .as_deref()
        .and_then(identity_for_preset)
        .map(|identity| identity.catalog_id.unwrap_or(identity.canonical_id).to_string())
        .unwrap_or_else(|| provider.id.clone())
}

/// One process-wide view of the subscriptions Usage has already saved.
#[derive(Debug, Default, Clone, Copy)]
pub struct UsageAccountBook;

impl AccountBook for UsageAccountBook {
    fn account(&self, catalog_id: &str) -> Option<AccountSnapshot> {
        // Live-first: material and attribution (Freshness / subscription_id)
        // are decided by the seam. AccountSnapshot's fields and the sign.rs
        // trait contract are untouched; ledger-side attribution signals (the
        // key fingerprint, say) will be derived from subscription_id by the
        // later ledger slices.
        let material = signing_material(catalog_id)?;
        Some(AccountSnapshot {
            access_token: material.access_token,
            account_id: material.account_id,
            api_key: material.api_key,
        })
    }

    fn allowance(&self, catalog_id: &str) -> Option<AllowanceSnapshot> {
        let row = stored_row(catalog_id)?;
        let snapshots = storage::list_usage_snapshots().ok()?;
        let usage = snapshots.get(&row.id)?;
        written_used(usage).map(|used| AllowanceSnapshot { used })
    }
}

fn stored_row(catalog_id: &str) -> Option<Subscription> {
    let rows = storage::list_subscriptions().ok()?;
    let active = storage::get_active_subscription(catalog_id)
        .ok()
        .flatten();
    pick(&rows, catalog_id, active.as_deref()).cloned()
}

fn pick<'a>(
    rows: &'a [Subscription],
    catalog_id: &str,
    active: Option<&str>,
) -> Option<&'a Subscription> {
    if let Some(active) = active
        && let Some(row) = rows
            .iter()
            .find(|row| row.id == active && row.catalog_id == catalog_id)
    {
        return Some(row);
    }
    rows.iter().find(|row| row.catalog_id == catalog_id)
}

/// Largest percent Usage already wrote. Missing percents stay unknown.
fn written_used(usage: &SubscriptionUsage) -> Option<f64> {
    [&usage.hourly, &usage.weekly, &usage.monthly]
        .into_iter()
        .filter_map(|window| window.as_ref().and_then(|window| window.percent))
        .max()
        .map(|percent| percent as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_gateway::key_fingerprint;
    use skillstar_gateway::{ProviderSnapshot, SignInput, sign_upstream};
    use skillstar_models::providers::{Credential, Provider};
    use skillstar_usage::crypto;
    use skillstar_usage::subscription::{BillingCycle, UsageWindow};
    use skillstar_usage::usage_switch::activate_subscription;
    use skillstar_usage::{AuthMode, Subscription};

    // Minimal JWT (sub=acct-1 / email=dana@example.com) used as the row's
    // id_token so a codex row can be activated (materialize requires one).
    // sub equals row 1's oauth_account_id — in a real login the id_token's
    // sub and tokens.account_id are the same account id, and a fixture that
    // is not self-consistent sends identity attribution down the wrong
    // channel.
    const ACCT_ID_TOKEN: &str = concat!(
        "e30.",
        "eyJlbWFpbCI6ImRhbmFAZXhhbXBsZS5jb20iLCJzdWIiOiJhY2N0LTEiLCJleHAiOjE5OTk5OTk5OTl9",
        "."
    );

    fn row(id: &str, token: &str, account_id: &str, refresh: &str) -> Subscription {
        Subscription {
            id: id.to_string(),
            catalog_id: "codex".to_string(),
            display_name: id.to_string(),
            auth_mode: AuthMode::OAuth,
            plan_tier: None,
            monthly_price: None,
            currency: "USD".to_string(),
            billing_cycle: BillingCycle::Monthly,
            start_date: 0,
            renew_date: 0,
            auto_renew: false,
            api_key_encrypted: None,
            platform_token_encrypted: None,
            access_token_encrypted: Some(crypto::encrypt(token)),
            refresh_token_encrypted: Some(crypto::encrypt(refresh)),
            access_token_expires_at: None,
            id_token_encrypted: Some(crypto::encrypt(ACCT_ID_TOKEN)),
            oauth_account_id: Some(account_id.to_string()),
            oauth_region: None,
            requires_reauth: false,
            provider_state_encrypted: None,
            cookie_jar_encrypted: None,
            cookie_session_expires_at: None,
            manual_quota: None,
            note: None,
            sort_index: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn window(percent: i32) -> UsageWindow {
        UsageWindow {
            label: "7d".to_string(),
            used: i64::from(percent),
            total: Some(100),
            percent: Some(percent),
            reset_at: None,
            breakdown: Vec::new(),
        }
    }

    /// With no live credential (CLI not installed / logged out) custody
    /// reports Missing and account() falls back to the stored row — the
    /// pre-re-routing pinned semantics carry over unchanged.
    #[tokio::test(flavor = "current_thread")]
    async fn usage_account_book_reads_the_pinned_row() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        storage::upsert_subscription(row("codex-1", "access-1", "acct-1", "refresh-1")).unwrap();
        storage::upsert_subscription(row("codex-2", "access-2", "acct-2", "refresh-2")).unwrap();
        storage::set_active_subscription("codex", "codex-2").unwrap();
        let first = SubscriptionUsage {
            subscription_id: "codex-1".to_string(),
            weekly: Some(window(90)),
            ..Default::default()
        };
        let pinned = SubscriptionUsage {
            subscription_id: "codex-2".to_string(),
            weekly: Some(window(40)),
            ..Default::default()
        };
        storage::save_usage_snapshot(first).unwrap();
        storage::save_usage_snapshot(pinned).unwrap();

        let book = UsageAccountBook;
        let account = book.account("codex").expect("pinned codex row");
        assert_eq!(account.access_token.as_deref(), Some("access-2"));
        assert_eq!(account.account_id.as_deref(), Some("acct-2"));
        assert!(account.api_key.is_none());
        let rendered = format!("{account:?}");
        assert!(!rendered.contains("refresh-1"));
        assert!(!rendered.contains("refresh-2"));
        assert_eq!(
            book.allowance("codex"),
            Some(AllowanceSnapshot { used: 40.0 })
        );
        assert!(book.account("gemini-cli").is_none());
        assert!(book.allowance("gemini-cli").is_none());

        let mut asked = 0u32;
        let signed = sign_upstream(
            &book,
            &SignInput {
                catalog_id: "codex",
                provider: None,
                body: b"{}",
            },
            &mut |_url| asked += 1,
        );
        assert_eq!(asked, 0);
        assert_eq!(
            signed.headers,
            vec![
                ("Authorization".to_string(), "Bearer access-2".to_string()),
                ("Accept".to_string(), "application/json".to_string()),
            ]
        );

        let provider = ProviderSnapshot {
            api_key: Some("sk-provider".to_string()),
        };
        let api_only = sign_upstream(
            &book,
            &SignInput {
                catalog_id: "gemini-cli",
                provider: Some(&provider),
                body: b"{}",
            },
            &mut |_url| asked += 1,
        );
        assert_eq!(asked, 0);
        assert_eq!(
            api_only.headers,
            vec![(
                "Authorization".to_string(),
                "Bearer sk-provider".to_string()
            )]
        );
    }

    /// When custody reports LinkedTo, signing material is read straight from
    /// the live credential: the pin and the row are only caches — the token
    /// the CLI is actually sending is the source of truth.
    #[tokio::test(flavor = "current_thread")]
    async fn usage_account_book_prefers_the_live_credential() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        storage::upsert_subscription(row("codex-1", "access-1", "acct-1", "refresh-1")).unwrap();
        storage::set_active_subscription("codex", "codex-1").unwrap();
        activate_subscription("codex-1").await.unwrap();

        let book = UsageAccountBook;
        let account = book.account("codex").expect("live codex credential");
        // activate absorbed the same token into the row; this matches the
        // pre-re-routing (row-read) output, but from here on any CLI-side
        // rotation is followed live instead of being dragged back to the
        // row's older generation.
        assert_eq!(account.access_token.as_deref(), Some("access-1"));
        assert_eq!(account.account_id.as_deref(), Some("acct-1"));

        // The CLI clobbered the link via rename() and rotated the token:
        // signing must follow live.
        let live = root.path().join(".codex").join("auth.json");
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&live).unwrap()).unwrap();
        json["tokens"]["access_token"] = serde_json::json!("rotated-in-the-cli");
        std::fs::remove_file(&live).unwrap();
        std::fs::write(&live, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

        let rotated = book.account("codex").expect("rotated live credential");
        assert_eq!(
            rotated.access_token.as_deref(),
            Some("rotated-in-the-cli"),
            "live-first: the token the CLI is sending wins over the stored row"
        );

        let mut asked = 0u32;
        let signed = sign_upstream(
            &book,
            &SignInput {
                catalog_id: "codex",
                provider: None,
                body: b"{}",
            },
            &mut |_url| asked += 1,
        );
        assert_eq!(asked, 0);
        assert_eq!(
            signed.headers,
            vec![
                (
                    "Authorization".to_string(),
                    "Bearer rotated-in-the-cli".to_string()
                ),
                ("Accept".to_string(), "application/json".to_string()),
            ]
        );
    }

    /// A terminal CLI login (attributable to nobody) must still sign:
    /// Diverged serves the orphan material, just without a subscription
    /// attribution.
    #[tokio::test(flavor = "current_thread")]
    async fn usage_account_book_serves_an_orphan_login() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        storage::upsert_subscription(row("codex-1", "access-1", "acct-1", "refresh-1")).unwrap();
        let live = root.path().join(".codex").join("auth.json");
        std::fs::create_dir_all(live.parent().unwrap()).unwrap();
        std::fs::write(
            &live,
            br#"{"tokens":{"access_token":"terminal-login-token"}}"#,
        )
        .unwrap();

        let account = UsageAccountBook
            .account("codex")
            .expect("orphan material still signs");
        assert_eq!(account.access_token.as_deref(), Some("terminal-login-token"));
    }

    /// The upstream resolve half of the seam: provider rows become gateway
    /// candidates by their adopted-model whitelist, their OpenAI root joins
    /// onto the gateway's `/v1` paths, and an api-key row signs through the
    /// generic provider-key shape while the ledger attribution still names
    /// the row's real catalog.
    #[tokio::test(flavor = "current_thread")]
    async fn upstreams_resolve_from_the_provider_rows_by_adopted_models() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        let mut deepseek = Provider::new("deepseek", "DeepSeek");
        deepseek.preset_id = Some("deepseek".to_string());
        deepseek.endpoints.openai_chat = Some("https://api.deepseek.com/v1".to_string());
        deepseek.credential = Credential::single_key("k1", "sk-deepseek-secret");
        deepseek.models = vec!["deepseek-chat".to_string()];
        let mut relay = Provider::new("my-relay", "My Relay");
        relay.endpoints.openai_chat = Some("https://relay.example".to_string());
        relay.credential = Credential::single_key("k1", "sk-relay-secret");
        relay.models = vec!["m-9".to_string()];
        let mut unused = Provider::new("quiet", "Quiet");
        unused.endpoints.openai_chat = Some("https://quiet.example".to_string());
        unused.models = vec!["other-model".to_string()];
        let mut official = Provider::new("codex-official", "Codex");
        official.preset_id = Some("codex-official".to_string());
        official.credential = Credential::ExternalCli {
            surface: "codex".to_string(),
        };
        official.models = vec!["gpt-5".to_string()];
        write_store(vec![deepseek, relay, unused, official]);

        // A bare model name and a provider-qualified ref both match the row
        // whose whitelist names them.
        let candidates = super::resolve_upstreams("deepseek-chat");
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].id, "deepseek");
        assert_eq!(candidates[0].catalog_id, "gemini-cli");
        assert_eq!(candidates[0].endpoint, "https://api.deepseek.com");
        assert_eq!(
            candidates[0].provider.as_ref().and_then(|row| row.api_key.as_deref()),
            Some("sk-deepseek-secret")
        );

        let candidates = super::resolve_upstreams("my-relay/m-9");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "my-relay");
        assert_eq!(candidates[0].endpoint, "https://relay.example");

        assert!(
            super::resolve_upstreams("not-adopted-anywhere").is_empty(),
            "no row names it"
        );
        assert!(
            super::resolve_upstreams("gpt-5").is_empty(),
            "a native-login seed has no endpoint to forward to"
        );

        // Ledger attribution: an api-key row's catalog is its identity (or
        // its id), and its account is the key's fingerprint — the key itself
        // never appears.
        let (catalog, account) = super::attribute_candidate("deepseek");
        assert_eq!(catalog, "deepseek");
        assert_eq!(account, key_fingerprint("sk-deepseek-secret"));
        let (catalog, account) = super::attribute_candidate("my-relay");
        assert_eq!(catalog, "my-relay");
        assert_eq!(account, key_fingerprint("sk-relay-secret"));
        assert_ne!(account, "sk-relay-secret");
        assert_eq!(
            super::attribute_candidate("no-such-row"),
            (String::new(), String::new())
        );
    }

    /// Ledger attribution of an account-catalog row reads the subscription
    /// the signing material belongs to: a stored codex row attributes its
    /// own id, and no live credential means the row fallback answers.
    #[tokio::test(flavor = "current_thread")]
    async fn account_rows_attribute_through_the_stored_subscription() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);
        storage::upsert_subscription(row("codex-1", "access-1", "acct-1", "refresh-1")).unwrap();

        let mut official = Provider::new("codex-official", "Codex");
        official.preset_id = Some("codex-official".to_string());
        official.credential = Credential::ExternalCli {
            surface: "codex".to_string(),
        };
        official.endpoints.openai_chat = Some("https://chatgpt.example/v1".to_string());
        official.models = vec!["gpt-5".to_string()];
        write_store(vec![official]);

        let candidates = super::resolve_upstreams("gpt-5");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].catalog_id, "codex", "an account catalog signs from the book");
        assert!(candidates[0].provider.is_none(), "the row key is not used here");
        let (catalog, account) = super::attribute_candidate("codex-official");
        assert_eq!(catalog, "codex");
        assert_eq!(account, "codex-1", "the stored row the material fell back to");
    }

    fn write_store(providers: Vec<Provider>) {
        let store = skillstar_models::providers::ProvidersStoreV4 {
            providers,
            ..Default::default()
        };
        skillstar_models::providers::save_store(&store).unwrap();
    }
}
