//! Domain types for the subscription/usage tracker.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// Optional OAuth region code for region-aware providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_region: Option<String>,
    #[serde(default)]
    pub requires_reauth: bool,

    /// Provider-private versioned JSON ciphertext (AES-GCM).
    /// The plaintext shape belongs to the provider (`{"v":1,...}`).
    /// Never copied into DTOs or logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_state_encrypted: Option<String>,

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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum BillingCycle {
    /// Calendar subscription billed every month.
    #[default]
    Monthly,
    /// Calendar subscription billed yearly (price is annual total).
    Annual,
    /// One-shot purchase (price is a lump sum, not amortized).
    OneTime,
    /// Prepaid / pay-as-you-go API key balance (quota-based, no renew cycle).
    /// Serde tag: `"api-key"`.
    ApiKey,
}

/// Manual-mode quota the user maintains by hand (e.g. for Kimi Coding Plan,
/// Xiaomi MiMo, Tencent Hy3 etc.).
//
// `#[ts(type = "number")]` here and on every other 64-bit field in this file:
// ts-rs maps i64/u64 to `bigint`, but these values only ever cross the wire as
// JSON through serde_json + Tauri IPC + `JSON.parse`, none of which round-trip
// a real bigint — they all produce a plain JS `number`. Epoch seconds and token
// counters are nowhere near 2^53.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ManualQuota {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub total_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub used_tokens: Option<i64>,
    /// Human-readable window label (e.g. "本月" / "5h" / "周").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_label: Option<String>,
}

/// A single usage snapshot returned by a fetcher.
///
/// Each fetcher fills whichever fields apply to that provider — UI renders
/// any present field, hides any absent one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct SubscriptionUsage {
    pub subscription_id: String,
    #[ts(type = "number")]
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

    /// Legacy snapshot compatibility; no current fetcher populates this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(type = "unknown[]")]
    pub api_keys: Vec<serde_json::Value>,

    /// Legacy snapshot compatibility; retained for existing struct constructors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown")]
    pub deepseek_analytics: Option<serde_json::Value>,
}

impl SubscriptionUsage {
    /// True when this snapshot carries anything a card can actually render.
    pub fn has_quota_data(&self) -> bool {
        self.plan_name.is_some()
            || self.hourly.is_some()
            || self.weekly.is_some()
            || self.monthly.is_some()
            || self.balance.is_some()
            || !self.credits.is_empty()
            || !self.api_keys.is_empty()
            || self.deepseek_analytics.is_some()
    }

    /// The snapshot to persist after a failed refresh.
    ///
    /// A `transient` failure (429 / 5xx / network) says nothing about the
    /// account's quota, so the last good snapshot survives with the error
    /// annotated on top and its original `fetched_at` intact — the card keeps
    /// showing real numbers, honestly dated as stale. Any other failure
    /// replaces the snapshot, because the previous numbers may no longer
    /// describe the account at all.
    pub fn from_refresh_error(
        subscription_id: &str,
        previous: Option<&SubscriptionUsage>,
        message: String,
        transient: bool,
    ) -> Self {
        let carried = previous
            .filter(|_| transient)
            .filter(|p| p.has_quota_data());
        match carried {
            Some(previous) => SubscriptionUsage {
                subscription_id: subscription_id.to_string(),
                error: Some(message),
                ..previous.clone()
            },
            None => SubscriptionUsage {
                subscription_id: subscription_id.to_string(),
                fetched_at: Utc::now().timestamp(),
                error: Some(message),
                ..Default::default()
            },
        }
    }
}

/// What `used` / `total` count. Absent on old snapshots, which are counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum UsageUnit {
    /// Tokens, requests, or credits. The meter compacts these (`45K`, `1.2M`).
    #[default]
    Count,
    /// US cents. The meter shows dollars (`$36.00`), not a compacted count.
    UsdCents,
}

fn is_count_unit(unit: &UsageUnit) -> bool {
    *unit == UsageUnit::Count
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UsageWindow {
    /// Display label like `"5h"`, `"7d"`, `"30d"`, `"本月"`.
    pub label: String,
    #[ts(type = "number")]
    pub used: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub total: Option<i64>,
    /// 0-100; computed by fetcher if both `used` and `total` known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<i32>,
    /// Epoch seconds at which this window resets (if known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub reset_at: Option<i64>,
    /// Nested sub-quotas (Cursor Auto+Composer / API) **or** per-model request
    /// counts (Ollama Cloud `limits.*.models`). Request-count rows omit
    /// `percent`/`total`; the UI lists them instead of drawing quota bars.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakdown: Vec<UsageWindow>,
    /// Defaults to [`UsageUnit::Count`] so older snapshots keep their numbers.
    #[serde(default, skip_serializing_if = "is_count_unit")]
    pub unit: UsageUnit,
}

/// Codex prepaid balance from `wham/usage` `credits`. Not a reset card.
/// `credit_amount` is the upstream balance text, or [`CODEX_CREDITS_UNLIMITED`].
pub const CODEX_CREDITS: &str = "codex-credits";
/// Sentinel stored in [`CODEX_CREDITS`] when `credits.unlimited` is true.
pub const CODEX_CREDITS_UNLIMITED: &str = "unlimited";

/// Card text for a Codex `credits.balance`. Storage keeps the upstream
/// string; this trims trailing zeros and keeps at most two decimal places.
/// Non-numeric text is returned trimmed.
pub fn format_codex_credit_points(raw: &str) -> String {
    let trimmed = raw.trim();
    let Ok(value) = trimmed.parse::<f64>() else {
        return trimmed.to_string();
    };
    if !value.is_finite() {
        return trimmed.to_string();
    }
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 && rounded.abs() < 1e15 {
        return format!("{}", rounded as i64);
    }
    format!("{rounded:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// Count of live Grok reset cards. `credit_amount` is the decimal count.
/// The React footer reads this row; the GPUI reset stack prefers
/// [`GROK_RESET_CARD`] when those rows are present.
pub const GROK_RESET_CREDITS: &str = "grok-reset-credits";
/// One live Grok reset card. `credit_amount` is the expiry, epoch seconds.
/// The token id stays on the provider and is never written here.
pub const GROK_RESET_CARD: &str = "grok-reset-card";

/// The quota window consumed by one reset card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ResetWindow {
    FiveHour,
    #[default]
    Weekly,
}

impl ResetWindow {
    pub fn key(self) -> &'static str {
        match self {
            Self::FiveHour => "FIVE_HOUR",
            Self::Weekly => "WEEK",
        }
    }

    /// Persisted count/expiry keys; IDs used for redemption are never stored.
    pub fn credit_keys(self, catalog: &str) -> Option<(&'static str, &'static str)> {
        match (catalog, self) {
            ("xai", Self::Weekly) => Some((GROK_RESET_CREDITS, GROK_RESET_CARD)),
            ("codex", Self::Weekly) => Some(("codex-reset-credits", "codex-reset-card")),
            ("zcode", Self::FiveHour) => Some(("glm-five-reset-credits", "glm-five-reset-card")),
            ("zcode", Self::Weekly) => Some(("glm-week-reset-credits", "glm-week-reset-card")),
            _ => None,
        }
    }

    pub fn for_catalog(catalog: &str) -> &'static [Self] {
        match catalog {
            "xai" | "codex" => &[Self::Weekly],
            "zcode" => &[Self::FiveHour, Self::Weekly],
            _ => &[],
        }
    }
}

/// Credit info extracted from paid tiers (e.g. Antigravity paidTier.availableCredits).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreditInfo {
    pub credit_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit_amount: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_credit_amount_for_usage: Option<String>,
}

impl CreditInfo {
    pub fn is_reset_card(&self) -> bool {
        ["xai", "codex", "zcode"].iter().any(|catalog| {
            ResetWindow::for_catalog(catalog).iter().any(|window| {
                window.credit_keys(catalog).is_some_and(|(count, card)| {
                    self.credit_type == count || self.credit_type == card
                })
            })
        })
    }

    pub(crate) fn reset_record(kind: &str, amount: i64) -> Self {
        Self {
            credit_type: kind.into(),
            credit_amount: Some(amount.to_string()),
            minimum_credit_amount_for_usage: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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
// Not `TS`-derived: the frontend contract for an alert is
// `usage::SubscriptionAlertDto`, which owns the generated `SubscriptionAlert.ts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionAlert {
    /// Stable id (subscription_id + kind) so dismiss is idempotent.
    pub id: String,
    pub subscription_id: String,
    pub severity: AlertSeverity,
    pub kind: AlertKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
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

    fn good_snapshot() -> SubscriptionUsage {
        SubscriptionUsage {
            subscription_id: "sub-1".into(),
            fetched_at: 1_700_000_000,
            plan_name: Some("PRO".into()),
            weekly: Some(UsageWindow {
                label: "7d".into(),
                used: 61,
                total: Some(100),
                percent: Some(61),
                reset_at: None,
                breakdown: Vec::new(),

                unit: UsageUnit::Count,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn transient_failure_keeps_the_last_good_quota_and_its_timestamp() {
        let previous = good_snapshot();

        let kept = SubscriptionUsage::from_refresh_error(
            "sub-1",
            Some(&previous),
            "Codex refresh 状态码 503: upstream".into(),
            true,
        );

        assert_eq!(kept.plan_name.as_deref(), Some("PRO"));
        assert_eq!(kept.weekly.as_ref().and_then(|w| w.percent), Some(61));
        assert_eq!(
            kept.fetched_at, previous.fetched_at,
            "stale data must keep its real age, not claim to be fresh"
        );
        assert!(kept.error.as_deref().unwrap().contains("503"));
    }

    #[test]
    fn non_transient_failure_replaces_the_snapshot() {
        let previous = good_snapshot();

        let replaced = SubscriptionUsage::from_refresh_error(
            "sub-1",
            Some(&previous),
            "登录已失效，请重新授权。".into(),
            false,
        );

        assert_eq!(replaced.plan_name, None);
        assert!(replaced.weekly.is_none());
        assert!(!replaced.has_quota_data());
        assert_eq!(replaced.subscription_id, "sub-1");
    }

    #[test]
    fn old_rows_without_provider_state_deserialize_and_omit_the_field_when_empty() {
        let raw = r#"{
            "id":"row",
            "catalog_id":"codex",
            "display_name":"Codex",
            "auth_mode":"o-auth",
            "currency":"USD"
        }"#;
        let sub: Subscription = serde_json::from_str(raw).unwrap();
        assert!(sub.provider_state_encrypted.is_none());
        let value = serde_json::to_value(&sub).unwrap();
        assert!(value.get("provider_state_encrypted").is_none());

        let mut with_blob = sub;
        with_blob.provider_state_encrypted = Some("cipher".into());
        let value = serde_json::to_value(&with_blob).unwrap();
        assert_eq!(value["provider_state_encrypted"], "cipher");
        let roundtrip: Subscription = serde_json::from_value(value).unwrap();
        assert_eq!(
            roundtrip.provider_state_encrypted.as_deref(),
            Some("cipher")
        );
    }

    #[test]
    fn codex_credit_points_keep_two_decimals() {
        assert_eq!(format_codex_credit_points("58654.7347730000"), "58654.73");
        assert_eq!(format_codex_credit_points("25"), "25");
        assert_eq!(format_codex_credit_points("0"), "0");
        assert_eq!(format_codex_credit_points("12.5"), "12.5");
        assert_eq!(format_codex_credit_points("12.50"), "12.5");
        assert_eq!(format_codex_credit_points("not-a-number"), "not-a-number");
    }

    #[test]
    fn transient_failure_without_prior_data_still_reports_the_error() {
        let empty = SubscriptionUsage {
            subscription_id: "sub-1".into(),
            fetched_at: 1,
            error: Some("旧错误".into()),
            ..Default::default()
        };

        let fresh =
            SubscriptionUsage::from_refresh_error("sub-1", Some(&empty), "429 太频繁".into(), true);

        assert!(!fresh.has_quota_data());
        assert_eq!(fresh.error.as_deref(), Some("429 太频繁"));
        assert!(fresh.fetched_at > 1, "an error-only card is re-stamped");
    }
}
