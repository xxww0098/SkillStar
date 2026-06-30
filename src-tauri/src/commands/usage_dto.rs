//! Frontend-facing DTOs for the `/usage` page.
//!
//! These wrap the `skillstar-usage` domain types and **never** expose raw
//! encrypted secrets — `api_key`/`access_token`/`refresh_token` ciphertexts
//! are stripped before serialization.

use serde::{Deserialize, Serialize};
use skillstar_usage::catalog::{AuthMode, CatalogEntry, CatalogTier};
use skillstar_usage::subscription::{
    AlertKind, AlertSeverity, BillingCycle, ManualQuota, Subscription, SubscriptionAlert,
    SubscriptionUsage,
};

#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntryDto {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub tier: CatalogTier,
    pub auth_modes: Vec<AuthMode>,
    pub brand_color: String,
    pub default_currency: String,
    pub subscription_url: String,
    pub warning: Option<String>,
    pub regions: Vec<String>,
}

impl From<CatalogEntry> for CatalogEntryDto {
    fn from(e: CatalogEntry) -> Self {
        Self {
            id: e.id.to_string(),
            display_name: e.display_name.to_string(),
            description: e.description.to_string(),
            tier: e.tier,
            auth_modes: e.auth_modes.to_vec(),
            brand_color: e.brand_color.to_string(),
            default_currency: e.default_currency.to_string(),
            subscription_url: e.subscription_url.to_string(),
            warning: e.warning.map(|s| s.to_string()),
            regions: e.regions.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionDto {
    pub id: String,
    pub catalog_id: String,
    pub display_name: String,
    pub auth_mode: AuthMode,
    pub plan_tier: Option<String>,
    pub monthly_price: Option<f64>,
    pub currency: String,
    pub billing_cycle: BillingCycle,
    pub start_date: i64,
    pub renew_date: i64,
    pub auto_renew: bool,
    /// `true` when ApiKey/OAuth credentials are present (without revealing them).
    pub has_credential: bool,
    /// DeepSeek platform session token configured (usage charts).
    #[serde(default)]
    pub has_platform_token: bool,
    pub requires_reauth: bool,
    /// Fingerprint bound to this subscription (id in the fingerprint store).
    /// `None` → behaves identically to pre-fingerprint SkillStar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_id: Option<String>,
    /// `true` when this subscription is the active account for its
    /// catalog_id (see Phase 7 multi-account support). At most one
    /// row per catalog has `is_active = true`.
    #[serde(default)]
    pub is_active: bool,
    pub manual_quota: Option<ManualQuota>,
    pub note: Option<String>,
    pub sort_index: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub usage: Option<SubscriptionUsage>,
    /// Outcome of the last CLI account-switch attempt (set by
    /// `set_active_subscription` when it also pushes credentials to the CLI).
    /// Absent when no switch was attempted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_result: Option<skillstar_app::usage_switch::SwitchOutcome>,
    /// Whether this catalog maps to a CLI whose credentials SkillStar can
    /// switch (codex / opencode / grok). IDE-only catalogs (cursor, trae, …)
    /// are `false` — the UI hides the "sync to CLI" affordance for them.
    #[serde(default)]
    pub supports_cli_switch: bool,
}

impl SubscriptionDto {
    pub fn from_parts(sub: Subscription, usage: Option<SubscriptionUsage>) -> Self {
        let has_credential = sub
            .api_key_encrypted
            .as_ref()
            .is_some_and(|s| !s.is_empty())
            || sub
                .access_token_encrypted
                .as_ref()
                .is_some_and(|s| !s.is_empty())
            || sub
                .cookie_jar_encrypted
                .as_ref()
                .is_some_and(|s| !s.is_empty());
        let has_platform_token = sub
            .platform_token_encrypted
            .as_ref()
            .is_some_and(|s| !s.is_empty());
        let supports_cli = skillstar_app::usage_switch::supports_cli_switch(&sub.catalog_id);
        Self {
            id: sub.id,
            catalog_id: sub.catalog_id,
            display_name: sub.display_name,
            auth_mode: sub.auth_mode,
            plan_tier: sub.plan_tier,
            monthly_price: sub.monthly_price,
            currency: sub.currency,
            billing_cycle: sub.billing_cycle,
            start_date: sub.start_date,
            renew_date: sub.renew_date,
            auto_renew: sub.auto_renew,
            has_credential,
            has_platform_token,
            requires_reauth: sub.requires_reauth,
            fingerprint_id: sub.fingerprint_id,
            // Will be filled by the command layer (which consults the
            // active-per-catalog store). The pure-data DTO can't know.
            is_active: false,
            manual_quota: sub.manual_quota,
            note: sub.note,
            sort_index: sub.sort_index,
            created_at: sub.created_at,
            updated_at: sub.updated_at,
            usage,
            switch_result: None,
            supports_cli_switch: supports_cli,
        }
    }

    /// Attach the outcome of a CLI account-switch attempt (used by
    /// `set_active_subscription` after it pushes credentials).
    pub fn with_switch_result(mut self, outcome: skillstar_app::usage_switch::SwitchOutcome) -> Self {
        self.switch_result = Some(outcome);
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSubscriptionInput {
    pub catalog_id: String,
    pub display_name: Option<String>,
    pub auth_mode: AuthMode,
    pub plan_tier: Option<String>,
    pub monthly_price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<BillingCycle>,
    pub start_date: Option<i64>,
    pub renew_date: Option<i64>,
    pub auto_renew: Option<bool>,
    /// Plaintext API key (encrypted server-side before storage).
    pub api_key: Option<String>,
    /// DeepSeek platform session token for usage analytics (encrypted server-side).
    pub platform_token: Option<String>,
    pub oauth_region: Option<String>,
    pub manual_quota: Option<ManualQuota>,
    pub note: Option<String>,
    /// Raw `Cookie:` header string pasted by the user (Cookie mode only).
    /// Parsed and encrypted server-side into `cookie_jar_encrypted`.
    pub cookie_header: Option<String>,
    /// Optional fingerprint binding when creating the subscription.
    pub fingerprint_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSubscriptionInput {
    pub display_name: Option<String>,
    pub plan_tier: Option<String>,
    pub monthly_price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<BillingCycle>,
    pub start_date: Option<i64>,
    pub renew_date: Option<i64>,
    pub auto_renew: Option<bool>,
    /// Send only when rotating; absent => keep existing.
    pub api_key: Option<String>,
    /// DeepSeek platform session token (send when rotating).
    pub platform_token: Option<String>,
    /// When `true`, clear any stored DeepSeek platform token.
    #[serde(default, rename = "clearPlatformToken")]
    pub clear_platform_token: bool,
    pub manual_quota: Option<ManualQuota>,
    pub note: Option<String>,
    /// Raw `Cookie:` header string to replace existing cookies (Cookie mode only).
    pub cookie_header: Option<String>,
    /// Bind this subscription to a stored fingerprint id.
    /// Absent → leave existing binding unchanged. Use [`clear_fingerprint`]
    /// to explicitly remove the binding.
    pub fingerprint_id: Option<String>,
    /// When `true`, drop the existing fingerprint binding regardless of
    /// [`fingerprint_id`]. Frontend sends `{ clearFingerprint: true }`
    /// when the user picks "无（默认）" from the picker.
    #[serde(default, rename = "clearFingerprint")]
    pub clear_fingerprint: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionAlertDto {
    pub id: String,
    pub subscription_id: String,
    pub severity: AlertSeverity,
    pub kind: AlertKind,
    pub message: String,
}

impl From<SubscriptionAlert> for SubscriptionAlertDto {
    fn from(a: SubscriptionAlert) -> Self {
        Self {
            id: a.id,
            subscription_id: a.subscription_id,
            severity: a.severity,
            kind: a.kind,
            message: a.message,
        }
    }
}

/// Header summary for the usage page.
#[derive(Debug, Clone, Serialize, Default)]
pub struct UsageSummary {
    /// Per-currency monthly spend (folded by billing cycle).
    pub monthly_spend: Vec<MonthlySpendEntry>,
    pub total_subscriptions: usize,
    pub alert_count: usize,
    pub reauth_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlySpendEntry {
    pub currency: String,
    pub amount: f64,
}

/// Returned by `start_oauth_login`.
#[derive(Debug, Clone, Serialize)]
pub struct OAuthStartDto {
    pub pending_id: String,
    pub auth_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri: Option<String>,
}

// Re-export inner types used by handler signatures so the lib.rs `#[command]`
// metadata generator can see them.
