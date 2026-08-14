//! Percent-encoding for query-string and form values.
//!
//! Five fetchers each carried their own copy of this loop. Four encoded
//! `s.bytes()` (correct); the GLM copy encoded `s.chars()` and emitted
//! `%{codepoint:02X}` for anything non-ASCII, which is not valid
//! percent-encoding. One byte-oriented implementation removes both the
//! duplication and that latent bug.

/// Percent-encode `s` keeping only the RFC 3986 unreserved set
/// (`A-Z a-z 0-9 - _ . ~`).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_unreserved_and_escapes_everything_else() {
        assert_eq!(encode("aZ0-_.~"), "aZ0-_.~");
        assert_eq!(
            encode("http://localhost:1455/auth/callback"),
            "http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"
        );
        assert_eq!(encode("a b"), "a%20b");
    }

    #[test]
    fn encodes_non_ascii_as_utf8_bytes() {
        // The old GLM copy produced "%4E2D" here (codepoint, not bytes).
        assert_eq!(encode("中"), "%E4%B8%AD");
    }
}
