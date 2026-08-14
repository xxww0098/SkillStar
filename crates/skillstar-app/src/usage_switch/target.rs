//! What a CLI has to tell the custody engine — and nothing more.
//!
//! Under symlink custody the engine never owns credentials, so a target does
//! not need a two-phase commit, a lease protocol or a private session store.
//! It answers five questions: where is the live file, which access token does
//! a file root carry, whose account is it, how do I build one from a
//! `Subscription` row, and how do I copy a rotated one back into that row.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use skillstar_usage::crypto;
use skillstar_usage::oauth::token_refresh;
use skillstar_usage::subscription::Subscription;

use super::error::{CustodyResult, ExternalStoreError, MaterializeError};

pub(super) mod codex;
pub(super) mod grok;
pub(super) mod opencode;

/// A CLI credential file SkillStar can put under custody.
///
/// "Root" always means the **whole file** the CLI reads, not one account's
/// slice of it: a snapshot is a byte-for-byte stand-in for that file, which is
/// what lets the live path become a symlink to it.
pub(super) trait CliCredentialTarget: Send + Sync {
    /// Usage catalog this target serves.
    fn catalog_id(&self) -> &'static str;

    /// Tool id reported in [`super::SwitchOutcome`] (what the UI names).
    fn tool_id(&self) -> &'static str;

    /// The file the CLI actually reads.
    fn live_path(&self) -> CustodyResult<PathBuf>;

    /// The lock the CLI itself takes while rotating tokens.
    ///
    /// Symlinks remove the *stale copy* problem, never the "refresh tokens are
    /// single-use, whoever spends first revokes the other" problem — so this
    /// stays. Targets whose CLI has no official lock get a private one, which
    /// at least serialises SkillStar against itself across processes.
    fn lock_path(&self, live: &Path) -> PathBuf {
        let mut name = live.file_name().unwrap_or_default().to_os_string();
        name.push(".skillstar.lock");
        live.with_file_name(name)
    }

    /// Whether the lock file carries a `PID:unix_seconds` holder line. Grok's
    /// stale-lock recovery reads it and would otherwise recreate the lock
    /// inode underneath a live SkillStar lease.
    fn lock_holder_line(&self) -> bool {
        false
    }

    /// The bearer credential inside a file root. `None` = nobody is logged in.
    ///
    /// This single string is the whole basis of reconciliation (§ [`super::custody`]):
    /// never compare whole JSON, because every CLI adds fields of its own.
    fn access_token(&self, root: &Value) -> Option<String>;

    /// When the credential in `root` stops working, as unix seconds.
    fn expires_at(&self, root: &Value) -> Option<i64>;

    /// Who the credential in `root` belongs to. Empty identity = unknowable
    /// (OpenCode API keys carry no claims), which the engine treats as
    /// "cannot conflict", not as "matches everyone".
    fn identity(&self, root: &Value) -> AccountIdentity;

    /// Build a file root for `sub` from scratch.
    ///
    /// `base` is the file the CLI has now (or this account's previous
    /// snapshot) so sibling material survives — Grok's legacy sign-in block,
    /// OpenCode's other providers, Codex's unknown keys.
    fn materialize(
        &self,
        sub: &Subscription,
        base: Option<&Value>,
    ) -> Result<Value, MaterializeError>;

    /// Copy the credential fields out of `root` into the subscription row.
    /// Only credentials: user-owned metadata is never touched here.
    fn absorb(&self, root: &Value, sub: &mut Subscription);

    /// A second, higher-priority store the symlink cannot reach — macOS
    /// Codex reads its keychain first and treats `auth.json` as a fallback.
    ///
    /// `live` is passed in rather than re-resolved: the engine already holds
    /// the resolved path, and a target that resolved it again could disagree
    /// with the file custody is actually holding.
    fn external_root(&self, _live: &Path) -> Option<Value> {
        None
    }

    /// Push `root` into that second store. Returns whether anything was written.
    fn publish_external(&self, _live: &Path, _root: &Value) -> Result<bool, ExternalStoreError> {
        Ok(false)
    }
}

// ── identity ─────────────────────────────────────────────────────────────

/// Two independent channels for "who is this". Multiple values inside one
/// channel means the material is ambiguous, and ambiguity always fails closed:
/// it neither matches nor is safe to overwrite.
#[derive(Debug, Default, Clone)]
pub(super) struct AccountIdentity {
    subjects: HashSet<String>,
    emails: HashSet<String>,
}

impl AccountIdentity {
    pub(super) fn is_empty(&self) -> bool {
        self.subjects.is_empty() && self.emails.is_empty()
    }

    fn is_ambiguous(&self) -> bool {
        self.subjects.len() > 1 || self.emails.len() > 1
    }

    /// Same account, provable. Subjects outrank emails: an email is reusable
    /// across tenants, a subject is not.
    pub(super) fn matches(&self, other: &Self) -> bool {
        if self.is_ambiguous() || other.is_ambiguous() {
            return false;
        }
        if !self.subjects.is_empty() && !other.subjects.is_empty() {
            return self.subjects.iter().any(|s| other.subjects.contains(s));
        }
        !self.emails.is_empty()
            && !other.emails.is_empty()
            && self.emails.iter().any(|e| other.emails.contains(e))
    }

    /// Provably *different* accounts. An empty identity never conflicts — it
    /// is unknown, not wrong.
    pub(super) fn conflicts(&self, other: &Self) -> bool {
        if self.is_ambiguous() || other.is_ambiguous() {
            return true;
        }
        if !self.subjects.is_empty() && !other.subjects.is_empty() {
            return !self.subjects.iter().any(|s| other.subjects.contains(s));
        }
        !self.emails.is_empty()
            && !other.emails.is_empty()
            && !self.emails.iter().any(|e| other.emails.contains(e))
    }

    pub(super) fn push_subject(&mut self, raw: Option<&str>) {
        push_normalized(&mut self.subjects, raw);
    }

    pub(super) fn push_email(&mut self, raw: Option<&str>) {
        push_normalized(&mut self.emails, raw);
    }

    pub(super) fn email(&self) -> Option<&str> {
        self.emails.iter().next().map(String::as_str)
    }

    pub(super) fn subject(&self) -> Option<&str> {
        self.subjects.iter().next().map(String::as_str)
    }
}

fn push_normalized(values: &mut HashSet<String>, raw: Option<&str>) {
    if let Some(value) = raw {
        let value = value.trim().to_ascii_lowercase();
        if !value.is_empty() {
            values.insert(value);
        }
    }
}

/// Identity claims carried by a JWT-shaped credential. Shared by every target
/// whose tokens are JWTs (Codex id_token, Grok access token).
pub(super) fn identity_from_jwt(token: &str) -> AccountIdentity {
    let mut identity = AccountIdentity::default();
    let Some(claims) = token_refresh::decode_jwt_payload(token) else {
        return identity;
    };
    for field in ["sub", "user_id", "principal_id"] {
        identity.push_subject(claims.get(field).and_then(Value::as_str));
    }
    identity.push_email(claims.get("email").and_then(Value::as_str));
    identity
}

/// Identity of a stored subscription row.
///
/// The access token is authoritative; an id_token may survive from an older
/// authorization, so it only fills channels the access token left empty.
/// `oauth_account_id` is the last fallback, never a peer that can make two
/// conflicting identities appear to match.
pub(super) fn subscription_identity(sub: &Subscription) -> AccountIdentity {
    let mut identity = AccountIdentity::default();
    for encrypted in [
        sub.access_token_encrypted.as_deref(),
        sub.id_token_encrypted.as_deref(),
    ] {
        let Some(token) = encrypted.map(crypto::decrypt).filter(|t| !t.is_empty()) else {
            continue;
        };
        let claims = identity_from_jwt(&token);
        if identity.subjects.is_empty() {
            identity.subjects = claims.subjects;
        }
        if identity.emails.is_empty() {
            identity.emails = claims.emails;
        }
    }
    if let Some(account_id) = sub.oauth_account_id.as_deref() {
        if account_id.contains('@') {
            if identity.emails.is_empty() {
                identity.push_email(Some(account_id));
            }
        } else if identity.subjects.is_empty() {
            identity.push_subject(Some(account_id));
        }
    }
    identity
}

/// Decrypt a subscription field, treating empty as absent.
pub(super) fn secret(field: Option<&str>) -> Option<String> {
    field.map(crypto::decrypt).filter(|value| !value.is_empty())
}

/// Effective expiry: the stored value and the JWT `exp` clipped to whichever
/// runs out first, so a stale stored value can never claim a dead token is live.
pub(super) fn subscription_expiry(sub: &Subscription) -> Option<i64> {
    let jwt = secret(sub.access_token_encrypted.as_deref())
        .and_then(|token| token_refresh::jwt_exp(&token));
    match (sub.access_token_expires_at, jwt) {
        (Some(stored), Some(jwt)) => Some(stored.min(jwt)),
        (stored, jwt) => stored.or(jwt),
    }
}
