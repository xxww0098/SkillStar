//! Fixed catalog of supported providers.
//!
//! Users can only create subscriptions from this list — there is no
//! "custom provider" escape hatch in v1. Missing providers should be added
//! by extending this catalog rather than letting users free-text input.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuthMode {
    ApiKey,
    OAuth,
    Cookie,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogTier {
    /// OAuth — v1 implements 6 of these.
    OAuth,
    /// Public API-key endpoint — v1 implements 4.
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
    /// Available regions for region-aware providers (Trae).
    pub regions: &'static [&'static str],
}

const NO_REGIONS: &[&str] = &[];
const TRAE_REGIONS: &[&str] = &["cn", "sg", "us", "ttp"];

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

// Authentication-mode tuples (cannot reference statics inside a const struct
// literal directly across all toolchains, so define as `&[AuthMode]` consts).
const OAUTH_ONLY: &[AuthMode] = &[AuthMode::OAuth];
const APIKEY_ONLY: &[AuthMode] = &[AuthMode::ApiKey];
const COOKIE_MANUAL: &[AuthMode] = &[AuthMode::Cookie, AuthMode::Manual];

/// Returns the full fixed catalog.
pub fn catalog() -> Vec<CatalogEntry> {
    vec![
        // ── Tier 1: OAuth (6) ──────────────────────────────────────────
        CatalogEntry {
            id: "cursor",
            display_name: "Cursor",
            description: "AI Code Editor",
            tier: CatalogTier::OAuth,
            auth_modes: OAUTH_ONLY,
            brand_color: "00E5BC",
            default_currency: "USD",
            subscription_url: "https://cursor.com/settings",
            warning: None,
            regions: NO_REGIONS,
        },
        CatalogEntry {
            id: "codex",
            display_name: "Codex",
            description: "OpenAI Codex CLI",
            tier: CatalogTier::OAuth,
            auth_modes: OAUTH_ONLY,
            brand_color: "10A37F",
            default_currency: "USD",
            subscription_url: "https://chat.openai.com/codex",
            warning: None,
            regions: NO_REGIONS,
        },
        CatalogEntry {
            id: "antigravity",
            display_name: "Antigravity",
            description: "Google AI IDE",
            tier: CatalogTier::OAuth,
            auth_modes: OAUTH_ONLY,
            brand_color: "4285F4",
            default_currency: "USD",
            subscription_url: "https://antigravity.google",
            warning: None,
            regions: NO_REGIONS,
        },
        CatalogEntry {
            id: "trae",
            display_name: "Trae",
            description: "字节系 AI IDE",
            tier: CatalogTier::OAuth,
            auth_modes: OAUTH_ONLY,
            brand_color: "FF7A45",
            default_currency: "CNY",
            subscription_url: "https://trae.ai",
            warning: None,
            regions: TRAE_REGIONS,
        },
        CatalogEntry {
            id: "qoder",
            display_name: "Qoder",
            description: "AI Coding Agent",
            tier: CatalogTier::OAuth,
            auth_modes: OAUTH_ONLY,
            brand_color: "7C3AED",
            default_currency: "CNY",
            subscription_url: "https://qoder.com",
            warning: None,
            regions: NO_REGIONS,
        },
        entry(
            "xai",
            "Grok",
            "xAI Grok CLI",
            CatalogTier::OAuth,
            OAUTH_ONLY,
            "111111",
            "USD",
            "https://x.ai",
        ),
        // ── Tier 2: API Key (4) ────────────────────────────────────────
        entry(
            "deepseek",
            "DeepSeek",
            "API Key 余额",
            CatalogTier::ApiKey,
            APIKEY_ONLY,
            "1A56DB",
            "CNY",
            "https://platform.deepseek.com/usage",
        ),
        entry(
            "glm",
            "智谱 GLM",
            "Coding Plan",
            CatalogTier::ApiKey,
            APIKEY_ONLY,
            "4A90E2",
            "CNY",
            "https://bigmodel.cn/usercenter/order",
        ),
        entry(
            "kimi",
            "Kimi",
            "Moonshot",
            CatalogTier::ApiKey,
            APIKEY_ONLY,
            "F5B400",
            "CNY",
            "https://platform.moonshot.cn",
        ),
        entry(
            "minimax",
            "MiniMax",
            "Token Plan",
            CatalogTier::ApiKey,
            APIKEY_ONLY,
            "9333EA",
            "CNY",
            "https://platform.minimaxi.com/user-center/basic-information/interface-key",
        ),
        // ── Tier 3: Cookie (2) + Manual ────────────────────────────────
        CatalogEntry {
            id: "stepfun",
            display_name: "阶跃 Step",
            description: "账户余额 / 消费",
            tier: CatalogTier::Cookie,
            auth_modes: COOKIE_MANUAL,
            brand_color: "00B5A9",
            default_currency: "CNY",
            subscription_url: "https://platform.stepfun.com/account-overview",
            warning: Some(
                "阶跃官方未提供余额查询 API：请使用 Cookie 模式，登录 platform.stepfun.com 后，\
                 从浏览器开发者工具复制包含 Oasis-Token 的 Cookie。",
            ),
            regions: NO_REGIONS,
        },
        // ── OpenCode first-party services ──────────────────────────────
        CatalogEntry {
            id: "opencode",
            display_name: "OpenCode",
            description: "$10/月 Go 订阅 · Zen 按量付费",
            tier: CatalogTier::Cookie,
            auth_modes: COOKIE_MANUAL,
            brand_color: "6366F1",
            default_currency: "USD",
            subscription_url: "https://opencode.ai/workspace",
            warning: Some(
                "OpenCode 官方 OAuth token 无法读取控制台用量；请使用 Cookie 模式，从 opencode.ai 控制台请求中复制 Cookie。",
            ),
            regions: NO_REGIONS,
        },
    ]
}

/// Look up a single catalog entry by id.
pub fn find(id: &str) -> Option<CatalogEntry> {
    catalog().into_iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_12_entries() {
        assert_eq!(catalog().len(), 12);
    }

    #[test]
    fn catalog_ids_are_unique() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for e in catalog() {
            assert!(seen.insert(e.id), "duplicate catalog id: {}", e.id);
        }
    }

    #[test]
    fn tier_counts_match_plan() {
        let c = catalog();
        let oauth = c.iter().filter(|e| e.tier == CatalogTier::OAuth).count();
        let api_key = c.iter().filter(|e| e.tier == CatalogTier::ApiKey).count();
        let cookie = c.iter().filter(|e| e.tier == CatalogTier::Cookie).count();
        let manual = c.iter().filter(|e| e.tier == CatalogTier::Manual).count();
        assert_eq!(oauth, 6);
        assert_eq!(api_key, 4);
        assert_eq!(cookie, 2);
        assert_eq!(manual, 0);
    }

    #[test]
    fn trae_has_regions() {
        let trae = find("trae").expect("trae catalog entry");
        assert_eq!(trae.regions.len(), 4);
    }

    #[test]
    fn auto_fetch_providers_exclude_manual_auth() {
        for entry in catalog() {
            let auto_fetch = entry.auth_modes.contains(&AuthMode::OAuth)
                || entry.auth_modes.contains(&AuthMode::ApiKey);
            if auto_fetch {
                assert!(
                    !entry.auth_modes.contains(&AuthMode::Manual),
                    "catalog `{}` supports auto fetch but still exposes manual auth",
                    entry.id
                );
            }
        }
    }

    /// Every catalog id must resolve to exactly one canonical provider identity
    /// in `skillstar-providers`. This pins the usage-side half of the
    /// catalog↔preset id reconciliation so the two can never silently drift.
    #[test]
    fn every_catalog_id_resolves_to_a_provider_identity() {
        for entry in catalog() {
            assert!(
                skillstar_providers::identity::identity_for_catalog(entry.id).is_some(),
                "catalog id `{}` has no provider identity in skillstar-providers",
                entry.id
            );
        }
    }
}
