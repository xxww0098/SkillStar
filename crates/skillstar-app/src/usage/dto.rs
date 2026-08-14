//! Frontend-facing DTOs for the `/usage` page.
//!
//! These wrap the `skillstar-usage` domain types and **never** expose raw
//! encrypted secrets — `api_key`/`access_token`/`refresh_token` ciphertexts
//! are stripped before serialization.
//!
//! Every type here derives [`TS`] and is exported to `src/types/generated/`
//! by `bun run types:gen`; `src/features/usage/types.ts` only re-exports
//! them. Field-level drift is therefore a build failure, not a silent
//! runtime mismatch — do not hand-write the TypeScript shapes.
//!
//! Names are `#[ts(rename = ...)]`d to drop the `Dto` suffix: the suffix
//! marks the Rust-side layering boundary, and on the TypeScript side these
//! *are* the only shapes, so the plain name is the honest one.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use skillstar_usage::catalog::{AuthMode, CatalogEntry, CatalogTier};
use skillstar_usage::subscription::{
    AlertKind, AlertSeverity, BillingCycle, ManualQuota, Subscription, SubscriptionAlert,
    SubscriptionUsage,
};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "CatalogEntry.ts", rename = "CatalogEntry")]
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

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "Subscription.ts", rename = "Subscription")]
pub struct SubscriptionDto {
    pub id: String,
    pub catalog_id: String,
    pub display_name: String,
    pub auth_mode: AuthMode,
    pub plan_tier: Option<String>,
    pub monthly_price: Option<f64>,
    pub currency: String,
    pub billing_cycle: BillingCycle,
    #[ts(type = "number")]
    pub start_date: i64,
    #[ts(type = "number")]
    pub renew_date: i64,
    pub auto_renew: bool,
    /// `true` when ApiKey/OAuth credentials are present (without revealing them).
    pub has_credential: bool,
    /// DeepSeek platform session token configured (usage charts).
    #[serde(default)]
    pub has_platform_token: bool,
    pub requires_reauth: bool,
    /// Region this OAuth account was authorized against, for region-aware
    /// providers (empty for everyone else). Exposed so the edit dialog can
    /// round-trip the stored region instead of silently re-defaulting it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_region: Option<String>,
    /// `true` when this subscription is the active account for its
    /// catalog_id (see Phase 7 multi-account support). At most one
    /// row per catalog has `is_active = true`.
    #[serde(default)]
    pub is_active: bool,
    pub manual_quota: Option<ManualQuota>,
    pub note: Option<String>,
    pub sort_index: i32,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    pub usage: Option<SubscriptionUsage>,
    /// Outcome of the last CLI account-switch attempt (set by
    /// `set_active_subscription` when it also pushes credentials to the CLI).
    /// Absent when no switch was attempted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub switch_result: Option<SwitchOutcomeDto>,
    /// Whether this catalog maps to a CLI whose credentials SkillStar can
    /// switch (codex / opencode / grok). IDE-only catalogs (cursor, …)
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
        let supports_cli = crate::usage_switch::supports_cli_switch(&sub.catalog_id);
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
            oauth_region: sub.oauth_region,
            // Will be filled by the application service (which consults the
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
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "CreateSubscriptionInput.ts")]
#[ts(optional_fields = nullable)]
pub struct CreateSubscriptionInput {
    pub catalog_id: String,
    pub display_name: Option<String>,
    pub auth_mode: AuthMode,
    pub plan_tier: Option<String>,
    pub monthly_price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<BillingCycle>,
    #[ts(type = "number", optional)]
    pub start_date: Option<i64>,
    #[ts(type = "number", optional)]
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
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "UpdateSubscriptionInput.ts")]
#[ts(optional_fields = nullable)]
pub struct UpdateSubscriptionInput {
    pub display_name: Option<String>,
    pub plan_tier: Option<String>,
    pub monthly_price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<BillingCycle>,
    #[ts(type = "number", optional)]
    pub start_date: Option<i64>,
    #[ts(type = "number", optional)]
    pub renew_date: Option<i64>,
    pub auto_renew: Option<bool>,
    /// Send only when rotating; absent => keep existing.
    pub api_key: Option<String>,
    /// DeepSeek platform session token (send when rotating).
    pub platform_token: Option<String>,
    /// When `Some(true)`, clear any stored DeepSeek platform token. Absent —
    /// the usual case — leaves it alone; `Option` rather than a defaulted
    /// `bool` so the generated TypeScript can say `clearPlatformToken?`
    /// instead of forcing every caller to spell out `false`.
    #[serde(default, rename = "clearPlatformToken")]
    pub clear_platform_token: Option<bool>,
    pub manual_quota: Option<ManualQuota>,
    pub note: Option<String>,
    /// Raw `Cookie:` header string to replace existing cookies (Cookie mode only).
    pub cookie_header: Option<String>,
}

/// Frontend projection of [`crate::usage_switch::SwitchOutcome`].
///
/// The switch domain owns its own outcome type and is free to reshape it;
/// this DTO is what the UI actually contracts against. The [`From`] impl
/// below is the seam between them — a field added or renamed upstream
/// fails to compile *here*, which is where the frontend contract lives.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SwitchOutcome.ts", rename = "SwitchOutcome")]
pub struct SwitchOutcomeDto {
    /// CLI tool id that was targeted (`"codex"` / `"opencode"` / `"grok"`).
    pub tool_id: String,
    /// Resolved config file that was (or would be) written.
    pub config_path: String,
    /// Path to the rolling backup created before the write, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// `true` on macOS when the keychain entry was updated (Codex only).
    pub keychain_updated: bool,
    /// How the CLI's live path ended up bound; `null` when nothing was bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub link_mode: Option<LinkModeDto>,
    /// `true` when the write fully succeeded.
    pub success: bool,
    /// Human-readable error when `success` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Frontend projection of [`crate::usage_switch::LinkMode`].
///
/// `copy` is a real behaviour difference, not an implementation detail: under
/// `symlink` the CLI's own token rotation writes through into SkillStar's
/// snapshot, and under `copy` it does not. The UI has to be able to say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LinkMode.ts", rename = "LinkMode")]
pub enum LinkModeDto {
    /// The CLI reads through a symlink straight into SkillStar's snapshot.
    Symlink,
    /// Degraded to a byte copy (Windows without symlink privilege): the CLI's
    /// own rotations no longer flow back on their own.
    Copy,
}

impl From<crate::usage_switch::LinkMode> for LinkModeDto {
    fn from(mode: crate::usage_switch::LinkMode) -> Self {
        // Matched exhaustively for the same reason the struct below is
        // destructured: a new binding mode must stop the build here.
        match mode {
            crate::usage_switch::LinkMode::Symlink => Self::Symlink,
            crate::usage_switch::LinkMode::Copy => Self::Copy,
        }
    }
}

impl From<crate::usage_switch::SwitchOutcome> for SwitchOutcomeDto {
    fn from(o: crate::usage_switch::SwitchOutcome) -> Self {
        // Destructured, not `..`-spread: an upstream field addition must
        // land as a compile error here rather than silently vanish from the
        // frontend contract.
        let crate::usage_switch::SwitchOutcome {
            tool_id,
            config_path,
            backup_path,
            keychain_updated,
            link_mode,
            success,
            error,
        } = o;
        Self {
            tool_id,
            config_path,
            backup_path,
            keychain_updated,
            link_mode: link_mode.map(LinkModeDto::from),
            success,
            error,
        }
    }
}

/// Frontend projection of [`crate::usage_switch::CliAccountState`] — which
/// account a CLI is *actually* serving, read back from disk.
///
/// Deliberately three cases rather than a boolean. "Not this account" and
/// "nobody at all" call for different words on the card, and collapsing them
/// is how the badge came to claim an account the CLI had stopped serving.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "CliAccountState.ts", rename = "CliAccountState")]
pub enum CliAccountStateDto {
    /// The CLI is serving this subscription.
    #[serde(rename_all = "camelCase")]
    LinkedTo { subscription_id: String },
    /// Someone is logged in, but no subscription row owns those credentials.
    Diverged,
    /// The CLI has no credential at all.
    Missing,
}

impl From<crate::usage_switch::CliAccountState> for CliAccountStateDto {
    fn from(state: crate::usage_switch::CliAccountState) -> Self {
        use crate::usage_switch::CliAccountState as Domain;
        match state {
            Domain::LinkedTo { subscription_id } => Self::LinkedTo { subscription_id },
            Domain::Diverged => Self::Diverged,
            Domain::Missing => Self::Missing,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "SubscriptionAlert.ts", rename = "SubscriptionAlert")]
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
#[derive(Debug, Clone, Serialize, Default, TS)]
#[ts(export, export_to = "UsageSummary.ts")]
pub struct UsageSummary {
    /// Per-currency monthly spend (folded by billing cycle).
    pub monthly_spend: Vec<MonthlySpendEntry>,
    pub total_subscriptions: usize,
    pub alert_count: usize,
    pub reauth_count: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "MonthlySpendEntry.ts")]
pub struct MonthlySpendEntry {
    pub currency: String,
    pub amount: f64,
}

/// Returned by `start_oauth_login`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "OAuthStart.ts", rename = "OAuthStart")]
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
