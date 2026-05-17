//! Token expiry checks and JWT `exp` extraction.
//!
//! Each OAuth fetcher uses this to decide when to call its `refresh` flow
//! BEFORE issuing a quota request, so we don't burn an extra 401 round trip.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde::Deserialize;

/// Refresh tokens this many seconds before their stated expiry.
pub const TOKEN_REFRESH_SKEW_SECONDS: i64 = 300;

/// True if the token expires within `TOKEN_REFRESH_SKEW_SECONDS`.
pub fn needs_refresh(expires_at: Option<i64>) -> bool {
    match expires_at {
        Some(exp) => exp - Utc::now().timestamp() < TOKEN_REFRESH_SKEW_SECONDS,
        None => false,
    }
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    #[serde(default)]
    exp: Option<i64>,
}

/// Decode the JWT *payload* (no signature verification — we already trust the
/// HTTPS-fetched token). Returns `None` for malformed tokens.
pub fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Extract `exp` claim from a JWT.
pub fn jwt_exp(token: &str) -> Option<i64> {
    let claims: JwtClaims =
        serde_json::from_value(decode_jwt_payload(token)?).ok()?;
    claims.exp
}

/// Extract a string-valued claim by JSON path (dot-separated).
pub fn jwt_string(token: &str, path: &[&str]) -> Option<String> {
    let mut value = decode_jwt_payload(token)?;
    for key in path {
        value = value.get(*key)?.clone();
    }
    value.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_refresh_logic() {
        let now = Utc::now().timestamp();
        assert!(!needs_refresh(None));
        assert!(!needs_refresh(Some(now + 3600)));
        assert!(needs_refresh(Some(now + 60)));
        assert!(needs_refresh(Some(now - 10)));
    }

    #[test]
    fn jwt_exp_parses() {
        // {"alg":"none"}.{"exp":1700000000}.
        let token = "eyJhbGciOiJub25lIn0.eyJleHAiOjE3MDAwMDAwMDB9.";
        assert_eq!(jwt_exp(token), Some(1_700_000_000));
    }

    #[test]
    fn jwt_exp_handles_malformed() {
        assert_eq!(jwt_exp("not-a-jwt"), None);
        assert_eq!(jwt_exp(""), None);
    }
}
