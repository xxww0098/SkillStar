//! Pure helpers of the import dialog — free functions split out of the
//! state-heavy `ImportDialog` so they stay unit-testable without a GPUI
//! runtime. They mirror the React free functions in `ImportModal.tsx` and
//! `src/lib/shareCode.ts`.

/// `deckNameFromRepoSource` — repo name after the last slash.
pub(super) fn deck_name_from_source(source: &str) -> String {
    source
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// `looksLikeShareCode` + `extractShareCode` — prefix + length, with the
/// embedded-token fallback for pasted share messages.
pub(super) fn looks_like_share_code(text: &str) -> bool {
    let trimmed = text.trim();
    if (trimmed.starts_with("ags-") || trimmed.starts_with("agd-")) && trimmed.len() > 30 {
        return true;
    }
    let extracted = extract_share_token(trimmed);
    extracted != trimmed && looks_like_share_code(extracted)
}

fn extract_share_token(text: &str) -> &str {
    let Some(start) = text.find("ags-").or_else(|| text.find("agd-")) else {
        return text;
    };
    let rest = &text[start..];
    let end = rest
        .find(
            |c: char| !matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '+' | '/' | '=' | '_' | '-'),
        )
        .unwrap_or(rest.len());
    &rest[..end]
}

/// `gitOperationErrorMessage`. Known transport codes use `mySkills.git*`.
/// Anything else is returned unchanged so the raw detail still shows.
pub(super) fn git_error_message(raw: &str) -> String {
    let key = if raw.contains("token_expired:") {
        "mySkills.gitTokenExpired"
    } else if raw.contains("not_authenticated:") {
        "mySkills.gitNotAuthenticated"
    } else if raw.contains("credential_unavailable:") {
        "mySkills.gitCredentialUnavailable"
    } else if raw.contains("unauthorized:") {
        "mySkills.gitUnauthorized"
    } else if raw.contains("app_not_installed:") {
        "mySkills.gitAppNotInstalled"
    } else if raw.contains("network:") {
        "mySkills.gitNetwork"
    } else if raw.contains("cancelled:") {
        "mySkills.gitCancelled"
    } else if raw.contains("unsafe_remote:") {
        "mySkills.gitUnsafeRemote"
    } else {
        return raw.to_string();
    };
    crate::i18n::t(key).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_names_come_from_the_last_path_segment() {
        assert_eq!(deck_name_from_source("anthropics/skills"), "skills");
        assert_eq!(
            deck_name_from_source("https://github.com/anthropics/skills.git/"),
            "skills"
        );
        assert_eq!(deck_name_from_source("  skills "), "skills");
        assert_eq!(deck_name_from_source(""), "");
    }

    #[test]
    fn share_codes_are_detected_bare_and_embedded() {
        let code = format!("agd-{}", "A".repeat(64));
        assert!(looks_like_share_code(&code));
        // A pasted share message carries the token mid-text.
        assert!(looks_like_share_code(&format!(
            "DecksName: pack\nsee {code} thanks"
        )));
        // Short prefixes and plain repo inputs stay on the scan path.
        assert!(!looks_like_share_code("ags-short"));
        assert!(!looks_like_share_code("anthropics/skills"));
        assert!(!looks_like_share_code(""));
    }

    #[test]
    fn embedded_tokens_stop_at_the_first_non_code_character() {
        let code = format!("ags-{}", "B".repeat(64));
        let message = format!("prefix {code}, suffix");
        let extracted = extract_share_token(&message);
        assert_eq!(extracted, code);
    }

    #[test]
    fn git_errors_map_to_actionable_text() {
        let lang = crate::i18n::set_language_for_test("en");
        assert_eq!(
            git_error_message("network: dns broke"),
            "GitHub could not be reached. Check the SkillStar proxy and your network, then retry."
        );
        assert_eq!(git_error_message("anything else"), "anything else");
        lang.set("zh-CN");
        assert_eq!(
            git_error_message("network: dns broke"),
            "连不上 GitHub。请检查 SkillStar 的代理和网络后再试。"
        );
    }
}
