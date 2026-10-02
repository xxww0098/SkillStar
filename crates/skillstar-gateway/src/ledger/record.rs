//! One ledger line: the fixed shape of a finished turn.
//!
//! Hard cut (spec D6): the schema is set once, snake_case keys, no migration
//! scaffolding. No field here may hold a secret. `account` is a subscription
//! id or `key:` plus the first eight hex chars of a presented key's sha256
//! (the custody orphan-id precedent); the key itself never reaches a line, a
//! log, or a Debug output.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Token counts of one turn, both vendor vocabularies folded together.
/// A field the response did not name is zero; absence is not unknown data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

/// Why a finished turn went wrong. `Quota` and `Verify` are distinguished by
/// the turn state machine of the upstream-wiring slice, which sees the word
/// lists and the verification links; the classification here is what the
/// status and the body alone settle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    RateLimit,
    Quota,
    Credit,
    Verify,
    Auth,
    Upstream,
    BadBody,
    Rejected,
}

impl ErrorKind {
    /// `None` while the turn succeeded (any status below 400). `local` marks
    /// a reply the gateway generated itself, which is how a 400 from a body
    /// that would not translate (`BadBody`) parts from a 400 the upstream
    /// handed back (`Rejected`).
    pub fn classify(status: u16, body: &[u8], local: bool) -> Option<Self> {
        if (100..400).contains(&status) {
            return None;
        }
        // Credit words outrank the status, the way rest::kind reads it: a 429
        // with `insufficient_quota` is out of credit, not out of rate.
        if status == 402 || contains(body, b"insufficient_quota") {
            return Some(Self::Credit);
        }
        match status {
            401 | 403 => Some(Self::Auth),
            429 => Some(Self::RateLimit),
            400 if local => Some(Self::BadBody),
            400..=499 => Some(Self::Rejected),
            _ => Some(Self::Upstream),
        }
    }
}

/// One finished turn, one JSONL line. `at` is the dispatch-entry clock in
/// Unix milliseconds, not the moment the reply left.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub at: i64,
    pub agent: String,
    pub session: String,
    pub model_asked: String,
    pub model_answered: String,
    pub catalog: String,
    pub account: String,
    pub tokens: TokenCounts,
    pub status: u16,
    pub latency_ms: u64,
    pub error_kind: Option<ErrorKind>,
    pub endpoint: String,
}

/// The ledger-side account label for a presented key: `key:` plus the first
/// eight hex chars of its sha256. Stable for one key, useless for guessing
/// the key back.
pub fn key_fingerprint(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let hex: String = digest
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("key:{hex}")
}

/// Which account a turn is charged to, from the credential slots the request
/// presented. The attribution and selection channels (`skillstar-<agent>`,
/// `skillstar/<model>`) and the bare placeholder name no account, so the
/// label is empty; any other presented key is fingerprinted. Until routing is
/// wired this is the only account signal a turn has — the winning candidate's
/// subscription id lands with the upstream-wiring slice.
pub(crate) fn account_of(
    authorization: &str,
    api_key: &str,
    goog_key: &str,
    query_key: &str,
) -> String {
    let bearer = authorization.trim();
    let bearer = bearer.strip_prefix("Bearer ").unwrap_or(bearer).trim();
    let Some(key) = [bearer, api_key.trim(), goog_key.trim(), query_key.trim()]
        .into_iter()
        .find(|slot| !slot.is_empty())
    else {
        return String::new();
    };
    if key == crate::PLACEHOLDER_BEARER
        || key.starts_with("skillstar-")
        || key.starts_with("skillstar/")
    {
        return String::new();
    }
    key_fingerprint(key)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_key_plus_eight_hex_chars() {
        let first = key_fingerprint("sk-live-abcdef");
        let again = key_fingerprint("sk-live-abcdef");
        assert_eq!(first, again, "stable for the same key");
        assert_eq!(first.len(), 4 + 8, "key: plus 8 hex chars: {first}");
        let hex = &first[4..];
        assert!(
            hex.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "lowercase hex: {first}"
        );
        assert_ne!(first, key_fingerprint("sk-live-abcdeg"));
    }

    #[test]
    fn account_of_fingerprints_only_real_keys() {
        // Channels, the placeholder, and nothing at all name no account.
        assert_eq!(account_of("Bearer skillstar-codex", "", "", ""), "");
        assert_eq!(account_of("skillstar/gpt-4o", "", "", ""), "");
        assert_eq!(account_of("Bearer skillstar", "", "", ""), "");
        assert_eq!(account_of("", "", "", ""), "");
        // A presented key in any slot becomes the same fingerprint.
        let fingerprint = key_fingerprint("sk-secret-value");
        assert_eq!(
            account_of("Bearer sk-secret-value", "", "", ""),
            fingerprint
        );
        assert_eq!(account_of("", "sk-secret-value", "", ""), fingerprint);
        assert_eq!(account_of("", "", "sk-secret-value", ""), fingerprint);
        assert_eq!(account_of("", "", "", "sk-secret-value"), fingerprint);
    }

    #[test]
    fn classification_follows_status_and_body() {
        assert_eq!(ErrorKind::classify(200, b"{}", false), None);
        assert_eq!(ErrorKind::classify(301, b"{}", false), None);
        assert_eq!(
            ErrorKind::classify(401, b"{}", false),
            Some(ErrorKind::Auth)
        );
        assert_eq!(
            ErrorKind::classify(403, b"{}", false),
            Some(ErrorKind::Auth)
        );
        assert_eq!(
            ErrorKind::classify(402, b"{}", false),
            Some(ErrorKind::Credit)
        );
        assert_eq!(
            ErrorKind::classify(429, br#"{"error":{"code":"insufficient_quota"}}"#, false),
            Some(ErrorKind::Credit)
        );
        assert_eq!(
            ErrorKind::classify(429, b"{}", false),
            Some(ErrorKind::RateLimit)
        );
        assert_eq!(
            ErrorKind::classify(400, b"{}", true),
            Some(ErrorKind::BadBody)
        );
        assert_eq!(
            ErrorKind::classify(400, b"{}", false),
            Some(ErrorKind::Rejected)
        );
        assert_eq!(
            ErrorKind::classify(404, b"{}", true),
            Some(ErrorKind::Rejected)
        );
        assert_eq!(
            ErrorKind::classify(500, b"{}", false),
            Some(ErrorKind::Upstream)
        );
        assert_eq!(
            ErrorKind::classify(502, b"{}", true),
            Some(ErrorKind::Upstream)
        );
    }

    #[test]
    fn records_never_debug_a_secret() {
        let account = account_of("Bearer sk-secret-value", "", "", "");
        let record = Record {
            at: 1_700_000_000_000,
            agent: "codex".to_string(),
            session: "s1".to_string(),
            model_asked: "m1".to_string(),
            model_answered: "m1".to_string(),
            catalog: String::new(),
            account,
            tokens: TokenCounts {
                input: 10,
                output: 5,
                ..TokenCounts::default()
            },
            status: 200,
            latency_ms: 12,
            error_kind: None,
            endpoint: "/v1/chat/completions".to_string(),
        };
        let text = format!("{record:?}");
        assert!(!text.contains("sk-secret-value"), "{text}");
        assert!(
            text.contains("key:"),
            "the account label is the fingerprint: {text}"
        );
        let tokens = format!("{:?}", record.tokens);
        assert!(!tokens.contains("sk-"), "{tokens}");
    }
}
