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
    /// DeepSeek platform session token for `platform.deepseek.com` usage APIs
    /// (separate from the API Key balance endpoint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_token_encrypted: Option<String>,

    // -- OAuth mode --
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_encrypted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_encrypted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_expires_at: Option<i64>,
    /// OpenID Connect `id_token` (JWT), encrypted. Required by Codex CLI
    /// account switching — `~/.codex/auth.json` needs the full `tokens` block
    /// including `id_token`, not just access/refresh tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token_encrypted: Option<String>,
    /// Free-form extra — historically held Codex `ChatGPT-Account-Id`; now
    /// the canonical Codex account id (extracted from `id_token` JWT).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_account_id: Option<String>,
    /// Region code for Trae (`cn` / `sg` / `us` / `ttp`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_region: Option<String>,
    #[serde(default)]
    pub requires_reauth: bool,

    /// Optional fingerprint id (see `skillstar-fingerprint`). When `Some`,
    /// the fetcher dispatcher resolves the fingerprint from the store and
    /// builds a `FingerprintAwareClient`; when `None`, falls back to the
    /// reqwest-default client used by SkillStar before v0.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_id: Option<String>,

    // -- Cookie mode --
    /// JSON-serialised Vec<CookieEntry> encrypted with AES-256-GCM.
    /// Cookies are parsed from the raw `Cookie:` header the user pastes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_jar_encrypted: Option<String>,
    /// Epoch seconds after which the session is assumed dead (user must re-paste).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_session_expires_at: Option<i64>,

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

    /// Credits visible in paid tiers (e.g. Antigravity paidTier.availableCredits).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<CreditInfo>,

    /// Set when the fetch failed but the subscription is still valid.
    pub error: Option<String>,

    /// OpenCode API keys discovered from the control panel (display only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_keys: Vec<OpenCodeApiKey>,

    /// DeepSeek platform usage analytics (model tokens + daily trend).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deepseek_analytics: Option<DeepSeekAnalytics>,
}

/// Per-model and daily usage from DeepSeek platform internal APIs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekAnalytics {
    pub month_cost: f64,
    pub today_cost: f64,
    #[serde(default)]
    pub models: Vec<DeepSeekModelUsage>,
    #[serde(default)]
    pub daily: Vec<DeepSeekDailyUsage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekModelUsage {
    pub key: String,
    pub name: String,
    pub total_tokens: u64,
    pub request_count: u64,
    pub cache_hit_tokens: u64,
    pub cache_miss_tokens: u64,
    pub response_tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekDailyUsage {
    pub date: String,
    pub flash_tokens: u64,
    pub flash_cache_hit: u64,
    pub flash_cache_miss: u64,
    pub flash_response: u64,
    pub pro_tokens: u64,
    pub pro_cache_hit: u64,
    pub pro_cache_miss: u64,
    pub pro_response: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
}

/// Display-only metadata for an OpenCode API key. The full key is **never**
/// stored here — it lives encrypted on the [`Subscription`] itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeApiKey {
    pub id: String,
    pub name: String,
    /// Masked display like `"sk-ZFY8...9ESf"`.
    pub display: String,
    pub email: Option<String>,
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
    /// Nested sub-quotas (e.g. Cursor's Auto+Composer / API split under Total).
    /// The UI renders these inside a visual container beneath the main bar.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakdown: Vec<UsageWindow>,
}

/// Credit info extracted from paid tiers (e.g. Antigravity paidTier.availableCredits).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditInfo {
    pub credit_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit_amount: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_credit_amount_for_usage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonetaryBalance {
    pub currency: String,
    pub total: f64,
    #[serde(default)]
    pub granted: f64,
    #[serde(default)]
    pub topped_up: f64,
    /// Provider-specific availability flag (e.g. DeepSeek `is_available`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_available: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deepseek_analytics_serializes_empty_arrays() {
        let value = serde_json::to_value(DeepSeekAnalytics::default()).unwrap();

        assert_eq!(value["models"], json!([]));
        assert_eq!(value["daily"], json!([]));
    }
}
