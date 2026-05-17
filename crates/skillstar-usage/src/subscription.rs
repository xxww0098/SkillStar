//! Domain types for the subscription/usage tracker.

use serde::{Deserialize, Serialize};

use crate::catalog::AuthMode;

/// A user-tracked subscription (one row in the usage panel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub catalog_id: String,
    pub display_name: String,
    pub auth_mode: AuthMode,

    /// User-entered plan tier override (Manual mode only). For ApiKey/OAuth
    /// modes the live plan tier lives in [`SubscriptionUsage::plan_name`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_tier: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthly_price: Option<f64>,

    #[serde(default = "default_currency")]
    pub currency: String,

    #[serde(default)]
    pub billing_cycle: BillingCycle,

    /// Subscription start date (epoch seconds, 0 if unset).
    #[serde(default)]
    pub start_date: i64,

    /// Next renewal date (epoch seconds, 0 if unset).
    #[serde(default)]
    pub renew_date: i64,

    #[serde(default)]
    pub auto_renew: bool,

    // -- ApiKey mode --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_encrypted: Option<String>,

    // -- OAuth mode --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_encrypted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_encrypted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_expires_at: Option<i64>,
    /// Free-form extra (e.g. Codex `ChatGPT-Account-Id`, JWT `id_token`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_account_id: Option<String>,
    /// Region code for Trae (`cn` / `sg` / `us` / `ttp`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_region: Option<String>,
    #[serde(default)]
    pub requires_reauth: bool,

    // -- Manual mode --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_quota: Option<ManualQuota>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// Grid order for drag-and-drop UI (lower first).
    #[serde(default)]
    pub sort_index: i32,

    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

fn default_currency() -> String {
    "CNY".to_string()
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BillingCycle {
    #[default]
    Monthly,
    Annual,
    OneTime,
}

/// Manual-mode quota the user maintains by hand (e.g. for Kimi Coding Plan,
/// Xiaomi MiMo, Tencent Hy3 etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualQuota {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_tokens: Option<i64>,
    /// Human-readable window label (e.g. "本月" / "5h" / "周").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_label: Option<String>,
}

/// A single usage snapshot returned by a fetcher.
///
/// Each fetcher fills whichever fields apply to that provider — UI renders
/// any present field, hides any absent one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubscriptionUsage {
    pub subscription_id: String,
    pub fetched_at: i64,

    /// Plan tier ("PRO" / "PLUS" / "ULTRA" / "PAYG" / "FREE" / free text).
    /// **All auto-sync fetchers must populate this** (see plan doc).
    pub plan_name: Option<String>,

    pub hourly: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    pub monthly: Option<UsageWindow>,
    pub balance: Option<MonetaryBalance>,

    /// Set when the fetch failed but the subscription is still valid.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWindow {
    /// Display label like `"5h"`, `"7d"`, `"30d"`, `"本月"`.
    pub label: String,
    pub used: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    /// 0-100; computed by fetcher if both `used` and `total` known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<i32>,
    /// Epoch seconds at which this window resets (if known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonetaryBalance {
    pub currency: String,
    pub total: f64,
    #[serde(default)]
    pub granted: f64,
    #[serde(default)]
    pub topped_up: f64,
}

/// Computed alert (banner / toast trigger) — never persisted, recomputed each refresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionAlert {
    /// Stable id (subscription_id + kind) so dismiss is idempotent.
    pub id: String,
    pub subscription_id: String,
    pub severity: AlertSeverity,
    pub kind: AlertKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AlertKind {
    QuotaLow,
    QuotaCritical,
    RenewSoon,
    Expired,
    NeedsReauth,
}
