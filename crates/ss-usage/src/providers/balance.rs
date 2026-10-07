//! Balance / usage-quota query specs for API-key providers.
//!
//! A [`BalanceSpec`] captures the parts of a balance query that are pure data —
//! the endpoint, how the API key is presented, and any provider-specific auth
//! hint. Response *parsing* is deliberately NOT modelled here: the API-key
//! providers return materially different shapes (monetary balance vs. rate-limit
//! windows), so each fetcher keeps its own parse step while sharing this spec
//! for everything that is genuinely common.

/// How an API key is attached to the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthScheme {
    /// `Authorization: Bearer <key>`.
    Bearer,
}

/// Pure-data description of a provider's balance/usage endpoint.
#[derive(Debug, Clone, Copy)]
pub struct BalanceSpec {
    /// Catalog id this spec belongs to (e.g. `"deepseek"`).
    #[cfg(test)]
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

pub const KIMI: BalanceSpec = BalanceSpec {
    #[cfg(test)]
    catalog_id: "kimi",
    display_name: "Kimi",
    endpoint: "https://api.kimi.com/coding/v1/usages",
    auth: AuthScheme::Bearer,
    auth_error_hint: None,
};

pub const OLLAMA: BalanceSpec = BalanceSpec {
    #[cfg(test)]
    catalog_id: "ollama",
    display_name: "Ollama Cloud",
    endpoint: "https://ollama.com/api/usage",
    auth: AuthScheme::Bearer,
    auth_error_hint: Some(
        "Ollama Cloud 401：请填 ollama.com → Settings → Keys 的 Cloud API Key，\
         不是本机 localhost:11434。",
    ),
};

/// All API-key balance specs, in catalog order.
#[cfg(test)]
pub const API_KEY_BALANCE_SPECS: &[BalanceSpec] = &[KIMI, OLLAMA];

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
}
