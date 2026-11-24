//! The credential a quota read needs: the OAuth tokens plus whatever
//! provider-private context the endpoint wants.

use serde::{Deserialize, Serialize};
use crate::oauth::TokenSet;

/// One account's credentials, as handed to a fetcher.
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    /// The tokens to authenticate the read with.
    pub tokens: TokenSet,
    /// The provider account id stored on the account record, when known.
    pub account_id: Option<String>,
    /// Provider-private state the account layer stores encrypted (an account
    /// id, a machine id, a plan hint). Fetchers read only their own keys.
    pub provider_state: Option<serde_json::Value>,
    /// Whether the account still needs a provider profile to identify it.
    pub needs_identity: bool,
}

/// What a read produced, including tokens the read rotated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageOutcome {
    pub snapshot: crate::usage::UsageSnapshot,
    /// The tokens to persist: the refreshed set when the read refreshed, else
    /// the ones it was given.
    pub tokens: TokenSet,
    /// A rotated provider state, when the refresh changed one.
    #[serde(default)]
    pub provider_state: Option<serde_json::Value>,
    /// Fresh identity learned while reading the profile/quota endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<crate::oauth::Identity>,
}
