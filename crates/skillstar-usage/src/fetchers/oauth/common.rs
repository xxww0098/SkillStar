//! Shared boilerplate for the OAuth fetchers in this module.
//!
//! Every provider's `finalize` builds a fresh [`Subscription`] after login;
//! ~20 of its ~27 fields are always the same default (`auth_mode: OAuth`,
//! every non-OAuth-mode field `None`/`0`, timestamps `now`, etc). Only
//! `catalog_id`, `display_name`, `currency`, the token trio, and (for some
//! providers) `id_token_encrypted`/`oauth_account_id` vary.
//! [`SubscriptionBuilder`] centralizes the constant part; it changes no
//! field's value, only where the ~20 identical lines live.
//!
//! `cursor.rs` is intentionally excluded (see the crate rule in `CLAUDE.md`)
//! and keeps its own hand-written literal.

use chrono::Utc;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::token_refresh;
use crate::subscription::{BillingCycle, Subscription};

/// True when `s` looks like an email suitable for a card title.
pub fn looks_like_email(s: &str) -> bool {
    let s = s.trim();
    s.contains('@') && s.len() > 3 && !s.contains(' ')
}

/// Best-effort email claim from an OIDC access/id token.
pub fn email_from_jwt(token: &str) -> Option<String> {
    token_refresh::jwt_string(token, &["email"])
        .filter(|s| looks_like_email(s))
        .or_else(|| {
            token_refresh::jwt_string(token, &["preferred_username"])
                .filter(|s| looks_like_email(s))
        })
}

/// Upgrade placeholder / `Brand · …` / WorkOS-style titles to a real email.
/// Custom renames (anything that already looks intentional) are left alone.
pub fn prefer_email_title(
    current: &str,
    candidate: Option<&str>,
    catalog_placeholders: &[&str],
) -> Option<String> {
    let email = candidate.map(str::trim).filter(|s| looks_like_email(s))?;
    let current = current.trim();
    if current == email {
        return None;
    }
    let is_placeholder = current.is_empty()
        || current.starts_with("user_")
        || current.contains("|user_")
        || catalog_placeholders
            .iter()
            .any(|p| current.eq_ignore_ascii_case(p) || current.starts_with(&format!("{p} · ")));
    if is_placeholder {
        return Some(email.to_string());
    }
    None
}

/// Mutate `subscription.display_name` when `email` is better than a placeholder.
pub fn apply_email_title(
    subscription: &mut Subscription,
    email: Option<&str>,
    catalog_placeholders: &[&str],
) {
    if let Some(better) =
        prefer_email_title(&subscription.display_name, email, catalog_placeholders)
    {
        subscription.display_name = better;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefer_email_title_upgrades_placeholders_only() {
        assert_eq!(
            prefer_email_title("Codex", Some("a@b.com"), &["Codex"]).as_deref(),
            Some("a@b.com")
        );
        assert_eq!(
            prefer_email_title("Grok · a@b.com", Some("a@b.com"), &["Grok"]).as_deref(),
            Some("a@b.com")
        );
        assert_eq!(
            prefer_email_title("user_01ABC", Some("a@b.com"), &["Cursor"]).as_deref(),
            Some("a@b.com")
        );
        assert_eq!(
            prefer_email_title("My Work", Some("a@b.com"), &["Codex"]),
            None
        );
        assert_eq!(
            prefer_email_title("Codex", Some("user_01"), &["Codex"]),
            None
        );
    }
}

/// Builds a fresh OAuth [`Subscription`], applying every default every
/// provider's literal used to repeat: `plan_tier`/`monthly_price`: `None`,
/// `billing_cycle`: `Monthly`, `start_date`/`renew_date`: `0`, `auto_renew`:
/// `false`, `api_key_encrypted`/`platform_token_encrypted`: `None`,
/// `requires_reauth`: `false`,
/// `cookie_jar_encrypted`/`cookie_session_expires_at`: `None`,
/// `manual_quota`/`note`: `None`, `sort_index`: `0`, `created_at` ==
/// `updated_at` == now. `id_token_encrypted`/`oauth_account_id`
/// default to `None`, opt-in via their setters. `oauth_region` is always
/// `None` (no catalog currently needs region-scoped OAuth).
pub struct SubscriptionBuilder {
    catalog_id: &'static str,
    display_name: String,
    currency: &'static str,
    access_token: String,
    refresh_token: Option<String>,
    access_token_expires_at: Option<i64>,
    id_token: Option<String>,
    oauth_account_id: Option<String>,
}

impl SubscriptionBuilder {
    /// Fields every provider must supply: `catalog_id`, display name,
    /// currency, the (plaintext) access token, and its expiry.
    pub fn new(
        catalog_id: &'static str,
        display_name: impl Into<String>,
        currency: &'static str,
        access_token: impl Into<String>,
        access_token_expires_at: Option<i64>,
    ) -> Self {
        Self {
            catalog_id,
            display_name: display_name.into(),
            currency,
            access_token: access_token.into(),
            refresh_token: None,
            access_token_expires_at,
            id_token: None,
            oauth_account_id: None,
        }
    }

    /// Plaintext refresh token; encrypted in [`Self::build`].
    pub fn refresh_token(mut self, token: Option<String>) -> Self {
        self.refresh_token = token;
        self
    }

    /// Plaintext OIDC `id_token`; encrypted in [`Self::build`]. Codex only.
    pub fn id_token(mut self, token: Option<String>) -> Self {
        self.id_token = token;
        self
    }

    /// `oauth_account_id` (Codex account id / Antigravity email / xAI subject-or-email).
    pub fn oauth_account_id(mut self, account_id: Option<String>) -> Self {
        self.oauth_account_id = account_id;
        self
    }

    /// Finalize into a [`Subscription`] with a fresh `id` and `created_at ==
    /// updated_at == now`.
    pub fn build(self) -> Subscription {
        let now = Utc::now().timestamp();
        Subscription {
            id: uuid::Uuid::new_v4().to_string(),
            catalog_id: self.catalog_id.to_string(),
            display_name: self.display_name,
            auth_mode: AuthMode::OAuth,
            plan_tier: None,
            monthly_price: None,
            currency: self.currency.to_string(),
            billing_cycle: BillingCycle::Monthly,
            start_date: 0,
            renew_date: 0,
            auto_renew: false,
            api_key_encrypted: None,
            platform_token_encrypted: None,
            access_token_encrypted: Some(crypto::encrypt(&self.access_token)),
            refresh_token_encrypted: self.refresh_token.as_deref().map(crypto::encrypt),
            access_token_expires_at: self.access_token_expires_at,
            id_token_encrypted: self.id_token.as_deref().map(crypto::encrypt),
            oauth_account_id: self.oauth_account_id,
            oauth_region: None,
            requires_reauth: false,
            cookie_jar_encrypted: None,
            cookie_session_expires_at: None,
            manual_quota: None,
            note: None,
            sort_index: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Generates the `pub async fn fetch(subscription: &mut Subscription)`
/// wrapper every OAuth fetcher repeats verbatim: delegate to the module's
/// private `fetch_inner`.
///
/// A macro, not a generic fn: each call site's `fetch_inner(subscription)`
/// is a distinct async fn borrowing `subscription` mutably, and expanding
/// inline avoids fighting HRTBs for a one-line body. Same
/// `macro_rules!` + `pub(crate) use` convention as `oauth_clients::client_id!`.
macro_rules! impl_oauth_fetch {
    () => {
        pub async fn fetch(
            subscription: &mut crate::subscription::Subscription,
        ) -> crate::UsageResult<crate::subscription::SubscriptionUsage> {
            fetch_inner(subscription).await
        }
    };
}

pub(crate) use impl_oauth_fetch;
