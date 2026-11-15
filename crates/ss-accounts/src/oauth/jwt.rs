//! Reading a JWT payload without verifying its signature.
//!
//! The claims we want (email, plan, the WorkOS user id) live in a token the
//! provider just handed us over TLS, so the payload is the provider's own
//! statement about the account. Nothing here is a gate: a missing or unreadable
//! claim means "identity unknown", never "login failed".

use base64::{
    Engine,
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
};
use serde_json::Value;

/// The decoded payload of `token`, or `None` when it is not a JWT.
pub(crate) fn payload(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let encoded = parts.next()?;
    if encoded.is_empty() {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .or_else(|_| URL_SAFE.decode(encoded))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The non-empty, trimmed string at `path` inside `value`.
pub(crate) fn claim(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(key)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Build an unsigned JWT-shaped token carrying `claims`.
///
/// Test-only: the crate never mints tokens, it only reads the ones a provider
/// just sent back.
#[cfg(test)]
pub(crate) fn test_token(claims: &Value) -> String {
    let encode = |bytes: &[u8]| URL_SAFE_NO_PAD.encode(bytes);
    format!(
        "{}.{}.{}",
        encode(br#"{"alg":"none"}"#),
        encode(claims.to_string().as_bytes()),
        encode(b"signature")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make(claims: &Value) -> String {
        test_token(claims)
    }

    #[test]
    fn payload_reads_claims() {
        let token = make(&json!({ "email": "a@b.c", "chatgpt_plan_type": "plus" }));
        let value = payload(&token).expect("payload");
        assert_eq!(claim(&value, &["email"]).as_deref(), Some("a@b.c"));
    }

    #[test]
    fn payload_reads_nested_claims() {
        let token = make(&json!({ "https://api.openai.com/auth": { "chatgpt_plan_type": "pro" } }));
        let value = payload(&token).expect("payload");
        assert_eq!(
            claim(&value, &["https://api.openai.com/auth", "chatgpt_plan_type"]).as_deref(),
            Some("pro")
        );
    }

    #[test]
    fn payload_rejects_non_jwt() {
        assert!(payload("not-a-token").is_none());
        assert!(payload("a..c").is_none());
        assert!(payload("").is_none());
    }

    #[test]
    fn claim_skips_blank_values() {
        let value = json!({ "email": "   ", "plan": null });
        assert_eq!(claim(&value, &["email"]), None);
        assert_eq!(claim(&value, &["plan"]), None);
        assert_eq!(claim(&value, &["missing"]), None);
    }
}
