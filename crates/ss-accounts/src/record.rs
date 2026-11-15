//! One account as the store keeps it.
//!
//! Credentials live here only as ciphertext; every accessor that returns a
//! plaintext token is crate-private, and nothing in this struct prints one.

use serde::{Deserialize, Serialize};
use crate::oauth::{Identity, TokenSet};
use crate::usage::UsageSnapshot;

/// A stored account row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    /// The ss-skills agent id this account logs in to.
    pub agent: String,
    /// User-visible name (an email, a login, or a rename).
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    /// The provider's own account id, as its client shows it (Codex
    /// tokens.account_id, Cursor's JWT subject tail). Identity metadata, not a
    /// secret: a live client credential is matched against it.
    #[serde(default)]
    pub account_id: Option<String>,
    /// AES-256-GCM ciphertext of the JSON token set.
    pub tokens_encrypted: String,
    /// AES-256-GCM ciphertext of the provider-private state, when there is one.
    #[serde(default)]
    pub provider_state_encrypted: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// Set when the provider rejected the credential; cleared by a new login.
    #[serde(default)]
    pub requires_reauth: bool,
    /// The last quota read, denormalised next to the record for a cheap list.
    #[serde(default)]
    pub usage: Option<UsageSnapshot>,
}

impl Account {
    /// Build a record from a finished login.
    pub(crate) fn from_login(
        agent: &str,
        identity: &Identity,
        tokens: &TokenSet,
        provider_state: Option<&serde_json::Value>,
        now: i64,
    ) -> crate::AccountResult<Self> {
        let tokens_json = serde_json::to_string(tokens)?;
        let mut record = Self {
            id: uuid::Uuid::new_v4().to_string(),
            agent: agent.to_string(),
            display_name: identity
                .display_name
                .clone()
                .or_else(|| identity.email.clone())
                .unwrap_or_else(|| agent.to_string()),
            email: identity.email.clone(),
            plan: identity.plan.clone(),
            account_id: identity.account_id.clone(),
            tokens_encrypted: crate::crypto::encrypt(&tokens_json),
            provider_state_encrypted: None,
            created_at: now,
            last_used_at: None,
            requires_reauth: false,
            usage: None,
        };
        record.set_provider_state(provider_state)?;
        Ok(record)
    }

    /// The identity this row is matched on: the provider's account id first,
    /// then the email. Never a token.
    pub fn identity(&self) -> Option<&str> {
        match self.account_id.as_deref() {
            Some(account_id) if !account_id.trim().is_empty() => Some(account_id),
            _ => self.email.as_deref(),
        }
    }

    /// Fingerprint of the stored refresh token, for matching a client that
    /// exposes no identity. Decrypts, so callers keep it off the cheap path.
    pub fn refresh_fingerprint(&self) -> Option<String> {
        let tokens = self.tokens().ok()?;
        tokens
            .refresh_token
            .as_deref()
            .map(crate::identity::fingerprint)
    }

    /// Fingerprint of the stored access token: a client may have rotated the
    /// refresh token, but a switch wrote this access token a moment ago.
    pub fn access_fingerprint(&self) -> Option<String> {
        let tokens = self.tokens().ok()?;
        Some(crate::identity::fingerprint(&tokens.access_token))
    }

    /// Replace the provider-private state (encrypted, never plaintext on disk).
    pub fn set_provider_state(
        &mut self,
        state: Option<&serde_json::Value>,
    ) -> crate::AccountResult<()> {
        self.provider_state_encrypted = match state {
            Some(value) => Some(crate::crypto::encrypt(&serde_json::to_string(value)?)),
            None => None,
        };
        Ok(())
    }

    /// The provider-private state, decrypted.
    pub(crate) fn provider_state(&self) -> crate::AccountResult<Option<serde_json::Value>> {
        match self.provider_state_encrypted.as_deref() {
            Some(sealed) => {
                let plaintext =
                    crate::crypto::decrypt(sealed).map_err(|error| crate::AccountError::Domain(error.to_string()))?;
                Ok(serde_json::from_str(&plaintext).ok())
            }
            None => Ok(None),
        }
    }

    /// The token set, decrypted.
    pub(crate) fn tokens(&self) -> crate::AccountResult<TokenSet> {
        let plaintext = crate::crypto::decrypt(&self.tokens_encrypted)
            .map_err(|error| crate::AccountError::AuthRequired(error.to_string()))?;
        Ok(serde_json::from_str(&plaintext)?)
    }

    /// Replace the token set.
    pub(crate) fn set_tokens(&mut self, tokens: &TokenSet) -> crate::AccountResult<()> {
        self.tokens_encrypted = crate::crypto::encrypt(&serde_json::to_string(tokens)?);
        Ok(())
    }
}