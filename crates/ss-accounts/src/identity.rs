//! Identity strings and token fingerprints.
//!
//! A stored account and the credential a client is serving are matched on
//! identity: an email, the provider's own account id, or - when the client
//! exposes neither - a fingerprint of the token it holds. Fingerprints are
//! SHA-256 prefixes, so no plaintext token ever becomes a comparison key, and
//! nothing here can print one.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// How many bytes of the digest a fingerprint keeps (16 hex characters).
const FINGERPRINT_BYTES: usize = 8;

/// A stable, non-reversible fingerprint of a token.
///
/// Used to answer "is the client holding the token this row owns?" without
/// keeping a comparison-safe copy of the token anywhere.
pub fn fingerprint(token: &str) -> String {
    let digest = Sha256::digest(token.trim().as_bytes());
    digest
        .iter()
        .take(FINGERPRINT_BYTES)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Normalise an identity string for comparison: trimmed, lowercased, non-empty.
pub fn normalize(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

/// Whether two identity strings name the same account. A missing side never
/// matches: "we do not know" is not "the same".
pub fn same_identity(left: Option<&str>, right: Option<&str>) -> bool {
    match (left.and_then(normalize), right.and_then(normalize)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// The claims of a JWT, without verifying its signature.
///
/// Signature verification is the provider's job; here the payload is only read
/// for identity hints (subject, email, expiry) that the provider already
/// attested by handing the token over in a login we performed.
pub fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?.trim_end_matches('=');
    if payload.is_empty() {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&decoded).ok()
}

/// One top-level string claim.
pub fn jwt_claim(token: &str, key: &str) -> Option<String> {
    jwt_claims(token)?
        .get(key)?
        .as_str()
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

/// A string claim nested one object deep (Codex keeps its account id under
/// "https://api.openai.com/auth").
pub fn jwt_nested_claim(token: &str, parent: &str, child: &str) -> Option<String> {
    jwt_claims(token)?
        .get(parent)?
        .get(child)?
        .as_str()
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

/// A JWT's exp claim, in unix seconds.
pub fn jwt_exp(token: &str) -> Option<i64> {
    jwt_claims(token)?.get("exp")?.as_i64()
}

/// A JWT's email claim, trimmed.
pub fn jwt_email(token: &str) -> Option<String> {
    jwt_claim(token, "email").map(|value| value.trim().to_string())
}

/// The tail of a JWT subject: Cursor's sub is "<user>|<auth id>" and only the
/// part after the bar is the account id its client stores as cursorAuth/authId.
pub fn jwt_subject_tail(token: &str) -> Option<String> {
    let subject = jwt_claim(token, "sub")?;
    let tail = subject.rsplit('|').next()?.trim();
    if tail.is_empty() {
        None
    } else {
        Some(tail.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fingerprint, jwt_claim, jwt_email, jwt_exp, jwt_nested_claim, jwt_subject_tail, normalize,
        same_identity,
    };
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

    /// A token shaped like the ones providers hand out: header.payload.signature.
    fn token_with_claims(claims: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        format!("{header}.{payload}.signature")
    }

    #[test]
    fn fingerprints_are_stable_and_do_not_contain_the_token() {
        let token = "sk-live-token";
        let first = fingerprint(token);
        assert_eq!(first, fingerprint(token));
        assert_ne!(first, fingerprint("sk-other-token"));
        assert_eq!(first.len(), 16);
        assert!(!first.contains(token));
    }

    #[test]
    fn identity_comparison_ignores_case_and_space() {
        assert!(same_identity(Some(" Me@Example.com "), Some("me@example.com")));
        assert!(!same_identity(Some("me@example.com"), Some("other@example.com")));
        assert!(!same_identity(None, Some("me@example.com")));
        assert!(!same_identity(Some("   "), Some("   ")));
        assert_eq!(normalize("  A@B "), Some("a@b".to_string()));
        assert_eq!(normalize("  "), None);
    }

    #[test]
    fn reads_claims_from_a_jwt_payload() {
        let token = token_with_claims(serde_json::json!({
            "sub": "user_01|6f1e2d3c",
            "email": "dev@example.com",
            "exp": 1_800_000_000_i64,
            "https://api.openai.com/auth": { "chatgpt_account_id": "acc-123" }
        }));
        assert_eq!(jwt_claim(&token, "email").as_deref(), Some("dev@example.com"));
        assert_eq!(jwt_email(&token).as_deref(), Some("dev@example.com"));
        assert_eq!(jwt_exp(&token), Some(1_800_000_000));
        assert_eq!(jwt_subject_tail(&token).as_deref(), Some("6f1e2d3c"));
        assert_eq!(
            jwt_nested_claim(&token, "https://api.openai.com/auth", "chatgpt_account_id").as_deref(),
            Some("acc-123")
        );
    }

    #[test]
    fn opaque_tokens_have_no_claims() {
        assert!(jwt_claim("not-a-jwt", "sub").is_none());
        assert!(jwt_exp("sk-123").is_none());
        assert!(jwt_subject_tail("sk-123").is_none());
        assert!(super::jwt_claims("header.%%%.signature").is_none());
    }
}
