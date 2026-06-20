//! Balance / usage-quota query specs for API-key providers.
//!
//! A [`BalanceSpec`] captures the parts of a balance query that are pure data —
//! the endpoint, how the API key is presented, and any provider-specific auth
//! hint. Response *parsing* is deliberately NOT modelled here: the four
//! providers return materially different shapes (monetary balance vs. rate-limit
//! windows), so each fetcher keeps its own parse step while sharing this spec
//! for everything that is genuinely common.

/// How an API key is attached to the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthScheme {
    /// `Authorization: Bearer <key>`.
    Bearer,
    /// A raw header with the key as its verbatim value, e.g. GLM uses
    /// `Authorization: <key>` with no `Bearer ` prefix.
    RawHeader(&'static str),
}

/// Pure-data description of a provider's balance/usage endpoint.
#[derive(Debug, Clone, Copy)]
pub struct BalanceSpec {
    /// Catalog id this spec belongs to (e.g. `"deepseek"`).
    pub catalog_id: &'static str,
    /// Human-readable name used in error messages (e.g. `"DeepSeek"`).
    pub display_name: &'static str,
    /// Full URL the balance query hits.
    pub endpoint: &'static str,
    /// How to present the API key.
    pub auth: AuthScheme,
    /// When set, an HTTP 401 surfaces this message instead of the generic
    /// "auth required" error (MiniMax needs the user to use a Token Plan Key).
    pub auth_error_hint: Option<&'static str>,
}

pub const DEEPSEEK: BalanceSpec = BalanceSpec {
    catalog_id: "deepseek",
    display_name: "DeepSeek",
    endpoint: "https://api.deepseek.com/user/balance",
    auth: AuthScheme::Bearer,
    auth_error_hint: None,
};

pub const KIMI: BalanceSpec = BalanceSpec {
    catalog_id: "kimi",
    display_name: "Kimi",
    endpoint: "https://api.moonshot.cn/v1/users/me/balance",
    auth: AuthScheme::Bearer,
    auth_error_hint: None,
};

pub const GLM: BalanceSpec = BalanceSpec {
    catalog_id: "glm",
    display_name: "GLM",
    endpoint: "https://open.bigmodel.cn/api/monitor/usage/quota/limit",
    // GLM expects the raw token in `Authorization`, NOT `Bearer <token>`.
    auth: AuthScheme::RawHeader("Authorization"),
    auth_error_hint: None,
};

pub const MINIMAX: BalanceSpec = BalanceSpec {
    catalog_id: "minimax",
    display_name: "MiniMax",
    endpoint: "https://www.minimax.io/v1/token_plan/remains",
    auth: AuthScheme::Bearer,
    auth_error_hint: Some(
        "MiniMax 401：请确认填的是 Token Plan Key（订阅管理 → Token Plan），\
         而非普通按量 API Key。",
    ),
};

pub const ZCODE: BalanceSpec = BalanceSpec {
    catalog_id: "zcode",
    display_name: "ZCode",
    endpoint: "https://zcode.z.ai/api/v1/zcode-plan/billing/balance?app_version=3.0.0",
    auth: AuthScheme::Bearer,
    auth_error_hint: Some(
        "ZCode 401：请粘贴从 ZCode 登录后获得的有效 access_token（\
         见 ~/.zcode/v2/logs 里的 zai.access_token），而非 GLM API Key。",
    ),
};

/// All API-key balance specs, in catalog order.
pub const API_KEY_BALANCE_SPECS: &[BalanceSpec] = &[DEEPSEEK, KIMI, GLM, MINIMAX, ZCODE];

/// Look up a balance spec by catalog id.
pub fn find(catalog_id: &str) -> Option<&'static BalanceSpec> {
    API_KEY_BALANCE_SPECS
        .iter()
        .find(|s| s.catalog_id == catalog_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique() {
        let mut ids: Vec<_> = API_KEY_BALANCE_SPECS.iter().map(|s| s.catalog_id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), API_KEY_BALANCE_SPECS.len());
    }

    #[test]
    fn find_resolves_known_and_rejects_unknown() {
        assert_eq!(
            find("deepseek").map(|s| s.endpoint),
            Some(DEEPSEEK.endpoint)
        );
        assert!(find("not-a-provider").is_none());
    }

    #[test]
    fn glm_uses_raw_authorization_header() {
        assert_eq!(GLM.auth, AuthScheme::RawHeader("Authorization"));
    }
}
