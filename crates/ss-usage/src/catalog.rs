//! Fixed catalog of supported providers.
//!
//! Users can only create subscriptions from this list — there is no
//! "custom provider" escape hatch in v1. Missing providers should be added
//! by extending this catalog rather than letting users free-text input.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AuthMode {
    ApiKey,
    OAuth,
    Cookie,
    Manual,
    /// Pasted credential. The generic create/update form must not write this mode.
    TokenImport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogTier {
    /// OAuth — v1 implements 6 of these.
    OAuth,
    /// Public API-key endpoint — v1 implements 5.
    ApiKey,
    /// Cookie-based web session.
    Cookie,
    /// Manual entry only.
    Manual,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    /// Optional sub-label (e.g. "Coding Plan" / "Token Plan").
    pub description: &'static str,
    pub tier: CatalogTier,
    /// Authentication modes this provider supports, in preferred order.
    pub auth_modes: &'static [AuthMode],
    /// Hex color (without `#`) for the SVG logo / badge.
    pub brand_color: &'static str,
    pub default_currency: &'static str,
    /// External URL the "续费" button opens.
    pub subscription_url: &'static str,
    /// Special warning shown in the create dialog (e.g. terms-of-use).
    pub warning: Option<&'static str>,
    /// Available regions for region-aware providers (empty for most).
    pub regions: &'static [&'static str],
}

const NO_REGIONS: &[&str] = &[];
// A flat positional builder keeps the static catalog table below compact and
// readable; a struct-with-builder would bloat each of the ~20 rows.
#[allow(clippy::too_many_arguments)]
const fn entry(
    id: &'static str,
    display_name: &'static str,
    description: &'static str,
    tier: CatalogTier,
    auth_modes: &'static [AuthMode],
    brand_color: &'static str,
    default_currency: &'static str,
    subscription_url: &'static str,
) -> CatalogEntry {
    CatalogEntry {
        id,
        display_name,
        description,
        tier,
        auth_modes,
        brand_color,
        default_currency,
        subscription_url,
        warning: None,
        regions: NO_REGIONS,
    }
}

pub(crate) const OAUTH_TOKEN_IMPORT: &[AuthMode] = &[AuthMode::OAuth, AuthMode::TokenImport];

/// Accounts follows the DSH OAuth registry plus its OpenCode Go API-key family. IDs already persisted remain stable.
pub fn catalog() -> Vec<CatalogEntry> {
    use AuthMode::{ApiKey, OAuth, TokenImport};
    use CatalogTier::{ApiKey as KeyTier, OAuth as OAuthTier};
    let mut rows = vec![
        entry(
            "codex",
            "Codex",
            "OpenAI Codex CLI",
            OAuthTier,
            &[OAuth],
            "10A37F",
            "USD",
            "https://chatgpt.com/codex",
        ),
        entry(
            "chatgpt",
            "ChatGPT",
            "Sign in with ChatGPT",
            OAuthTier,
            &[TokenImport],
            "10A37F",
            "USD",
            "https://chatgpt.com/settings/usage",
        ),
        entry(
            "xai",
            "Grok",
            "xAI Grok CLI",
            OAuthTier,
            &[OAuth],
            "111111",
            "USD",
            "https://x.ai",
        ),
        entry(
            "zcode",
            "GLM",
            "Z.ai / BigModel",
            OAuthTier,
            OAUTH_TOKEN_IMPORT,
            "000000",
            "USD",
            "https://zcode.z.ai",
        ),
        entry(
            "kiro",
            "Kiro",
            "Amazon Kiro",
            OAuthTier,
            OAUTH_TOKEN_IMPORT,
            "14B8A6",
            "USD",
            "https://app.kiro.dev/signin",
        ),
        entry(
            "antigravity",
            "Antigravity",
            "Google AI IDE",
            OAuthTier,
            &[OAuth],
            "4285F4",
            "USD",
            "https://antigravity.google",
        ),
        entry(
            "cursor",
            "Cursor",
            "AI Code Editor",
            OAuthTier,
            &[OAuth],
            "00E5BC",
            "USD",
            "https://cursor.com/settings",
        ),
        entry(
            "ollama",
            "Ollama",
            "Cloud",
            KeyTier,
            &[ApiKey],
            "111111",
            "USD",
            "https://ollama.com/settings",
        ),
        entry(
            "kimi",
            "Kimi",
            "Kimi Code",
            KeyTier,
            &[ApiKey],
            "F5B400",
            "USD",
            "https://www.kimi.com/code/console",
        ),
        entry(
            "github-copilot",
            "GitHub Copilot",
            "Suggestions & chat",
            OAuthTier,
            OAUTH_TOKEN_IMPORT,
            "24292F",
            "USD",
            "https://github.com/settings/copilot",
        ),
        entry(
            "devin-desktop",
            "Devin",
            "Cognition Devin",
            OAuthTier,
            OAUTH_TOKEN_IMPORT,
            "09B6A2",
            "USD",
            "https://devin.ai",
        ),
        entry(
            "cline",
            "Cline",
            "ClinePass / Credits",
            OAuthTier,
            &[TokenImport],
            "111111",
            "USD",
            "https://app.cline.bot",
        ),
        entry(
            "opencode-go",
            "OpenCode Go",
            "Go subscription",
            KeyTier,
            &[ApiKey],
            "111111",
            "USD",
            "https://opencode.ai/console",
        ),
        entry(
            "command-code",
            "Command Code",
            "Command Code CLI",
            KeyTier,
            &[ApiKey],
            "111111",
            "USD",
            "https://commandcode.ai",
        ),
    ];
    rows.iter_mut().find(|r| r.id == "zcode").unwrap().regions = &["zai", "bigmodel"];
    rows.iter_mut().find(|r| r.id == "chatgpt").unwrap().warning = Some(
        "导入 Sign in with ChatGPT 会话 JSON；此家族没有额度查询接口，用量请到 ChatGPT Settings → Usage 查看。",
    );
    rows.iter_mut().find(|r| r.id == "cline").unwrap().warning =
        Some("导入 Cline accessToken 或会话 JSON。");
    rows
}

pub fn find(id: &str) -> Option<CatalogEntry> {
    catalog().into_iter().find(|entry| entry.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_match_dsh_without_duplicate_or_extra_accounts() {
        let ids: Vec<_> = catalog().into_iter().map(|entry| entry.id).collect();
        // DSH: grok=xai, glm=zcode, copilot=github-copilot, devin=devin-desktop.
        assert_eq!(
            ids,
            [
                "codex",
                "chatgpt",
                "xai",
                "zcode",
                "kiro",
                "antigravity",
                "cursor",
                "ollama",
                "kimi",
                "github-copilot",
                "devin-desktop",
                "cline",
                "opencode-go",
                "command-code"
            ]
        );
        for id in [
            "anthropic",
            "qoder",
            "codebuddy",
            "codebuddy-cn",
            "trae",
            "trae-solo",
            "trae-cn",
            "trae-solo-cn",
            "zed",
            "deepseek",
            "minimax",
            "opencode",
        ] {
            assert!(find(id).is_none(), "{id}");
            assert!(!crate::local_import::local_import_supported(id), "{id}");
            assert!(!crate::token_import::token_import_supported(id), "{id}");
            assert!(!crate::usage_switch::supports_switch(id), "{id}");
        }
    }
}
