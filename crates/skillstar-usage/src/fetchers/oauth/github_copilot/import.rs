//! Paste parser for a GitHub Copilot token.
//!
//! The paste stays an opaque string: only a recognized token (and, from JSON,
//! an optional login) is copied onto [`ImportedToken`]. Nothing here is logged.

use serde_json::Value;

use crate::token_import::ImportedToken;
use crate::{UsageError, UsageResult};

const PLACEHOLDER_NAME: &str = "GitHub Copilot";

/// Accept a bare `gho_` / `ghp_` / `github_pat_` token, or JSON that carries one.
pub(crate) fn import_from_token(payload: &str) -> UsageResult<ImportedToken> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Err(UsageError::Other("GitHub Copilot 令牌为空".into()));
    }
    let (token, login) = if looks_like_json(trimmed) {
        token_from_json(trimmed)?
    } else {
        (require_github_token(trimmed)?, None)
    };
    Ok(ImportedToken {
        display_name: login
            .clone()
            .unwrap_or_else(|| PLACEHOLDER_NAME.to_string()),
        access_token: token,
        refresh_token: None,
        expires_at: None,
        oauth_account_id: login,
        provider_state: None,
        currency: None,
        oauth_region: None,
    })
}

fn looks_like_json(payload: &str) -> bool {
    matches!(payload.as_bytes().first(), Some(b'{' | b'[' | b'"'))
}

fn token_from_json(payload: &str) -> UsageResult<(String, Option<String>)> {
    let value: Value = serde_json::from_str(payload)
        .map_err(|_| UsageError::Other("GitHub Copilot 凭据 JSON 无法解析".into()))?;
    find_token(&value)
        .ok_or_else(|| UsageError::Other("GitHub Copilot 凭据 JSON 缺少可用令牌".into()))
}

fn find_token(value: &Value) -> Option<(String, Option<String>)> {
    match value {
        Value::String(text) => {
            let token = require_github_token(text.trim()).ok()?;
            Some((token, None))
        }
        Value::Array(items) => items.iter().find_map(find_token),
        Value::Object(map) => {
            let login = ["github_login", "login"].iter().find_map(|key| {
                map.get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|login| !login.is_empty())
                    .map(str::to_string)
            });
            for key in [
                "github_access_token",
                "access_token",
                "token",
                "githubToken",
            ] {
                if let Some(text) = map.get(key).and_then(Value::as_str)
                    && let Ok(token) = require_github_token(text.trim())
                {
                    return Some((token, login));
                }
            }
            for key in ["account", "github"] {
                if let Some((token, nested_login)) = map.get(key).and_then(find_token) {
                    return Some((token, nested_login.or(login)));
                }
            }
            None
        }
        _ => None,
    }
}

fn require_github_token(token: &str) -> UsageResult<String> {
    if is_github_token(token) {
        Ok(token.to_string())
    } else {
        Err(UsageError::Other("GitHub Copilot 令牌格式无法识别".into()))
    }
}

/// `gho_` (OAuth), `ghp_` (classic PAT), `github_pat_` (fine-grained PAT).
fn is_github_token(token: &str) -> bool {
    let rest = token
        .strip_prefix("github_pat_")
        .or_else(|| token.strip_prefix("gho_"))
        .or_else(|| token.strip_prefix("ghp_"));
    match rest {
        Some(rest) if !rest.is_empty() => rest
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_github_token_prefixes_are_accepted() {
        for token in ["gho_example", "ghp_example", "github_pat_11_example"] {
            let imported = import_from_token(token).unwrap();
            assert_eq!(imported.access_token, token);
            assert_eq!(imported.display_name, PLACEHOLDER_NAME);
            assert!(imported.oauth_account_id.is_none());
            assert!(imported.refresh_token.is_none());
            assert!(imported.provider_state.is_none());
            assert!(imported.expires_at.is_none());
        }
    }

    #[test]
    fn json_token_keeps_login_and_drops_the_paste() {
        let imported = import_from_token(
            r#"{"github_login":"octocat","github_access_token":"gho_fromjson","copilot_token":"short"}"#,
        )
        .unwrap();
        assert_eq!(imported.access_token, "gho_fromjson");
        assert_eq!(imported.oauth_account_id.as_deref(), Some("octocat"));
        assert_eq!(imported.display_name, "octocat");
        assert!(imported.provider_state.is_none());
    }

    #[test]
    fn bad_json_and_random_text_are_rejected_without_echoing_the_paste() {
        let secret = "gho_supersecretvalue";
        let broken = format!(r#"{{"access_token":"{secret}""#);
        let err = match import_from_token(&broken) {
            Err(err) => err,
            Ok(_) => panic!("broken json must be rejected"),
        };
        assert!(err.to_string().contains("无法解析"), "{err}");
        assert!(!err.to_string().contains(secret), "{err}");

        let err = match import_from_token(r#"{"access_token":"not-a-github-token"}"#) {
            Err(err) => err,
            Ok(_) => panic!("non-github token must be rejected"),
        };
        assert!(err.to_string().contains("缺少可用令牌"), "{err}");

        let err = match import_from_token("not a token") {
            Err(err) => err,
            Ok(_) => panic!("random text must be rejected"),
        };
        assert!(err.to_string().contains("无法识别"), "{err}");
        assert!(!is_github_token("gho_"));
        assert!(!is_github_token("ghu_user_to_server"));
    }
}
