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

/// `gitOperationErrorMessage` — React keeps these as English `defaultValue`s
/// (no catalog keys), so the literal strings here are byte-identical output.
pub(super) fn git_error_message(raw: &str) -> String {
    const MAP: &[(&str, &str)] = &[
        (
            "token_expired:",
            "Your GitHub session expired. Refresh it in Settings, then retry.",
        ),
        (
            "not_authenticated:",
            "Sign in to GitHub in Settings, then retry this private repository.",
        ),
        (
            "credential_unavailable:",
            "Unlock the system credential store, then retry.",
        ),
        (
            "unauthorized:",
            "The signed-in GitHub user does not have access to this repository.",
        ),
        (
            "app_not_installed:",
            "Install or authorize the SkillStar GitHub App for this repository, then retry.",
        ),
        (
            "network:",
            "GitHub could not be reached. Check the SkillStar proxy and your network, then retry.",
        ),
        ("cancelled:", "The repository operation was cancelled."),
        (
            "unsafe_remote:",
            "Remove credentials from the repository URL and use SkillStar GitHub login instead.",
        ),
    ];
    MAP.iter()
        .find(|(code, _)| raw.contains(code))
        .map(|(_, msg)| msg.to_string())
        .unwrap_or_else(|| raw.to_string())
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
        assert_eq!(
            git_error_message("network: dns broke"),
            "GitHub could not be reached. Check the SkillStar proxy and your network, then retry."
        );
        assert_eq!(git_error_message("anything else"), "anything else");
    }
}
