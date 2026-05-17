//! Fixed catalog of 18 supported providers.
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
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogTier {
    /// OAuth — v1 implements 5 of these.
    OAuth,
    /// Public API-key endpoint — v1 implements 4.
    ApiKey,
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
const OAUTH_ONLY: &[AuthMode] = &[AuthMode::OAuth, AuthMode::Manual];
const APIKEY_ONLY: &[AuthMode] = &[AuthMode::ApiKey, AuthMode::Manual];
const MANUAL_ONLY: &[AuthMode] = &[AuthMode::Manual];

/// Returns the full fixed catalog (18 entries).
pub fn catalog() -> Vec<CatalogEntry> {
    vec![
        // ── Tier 1: OAuth (5) ──────────────────────────────────────────
        CatalogEntry {
            id: "cursor",
            display_name: "Cursor",
            description: "AI Code Editor",
            tier: CatalogTier::OAuth,
            auth_modes: OAUTH_ONLY,
            brand_color: "000000",
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
        // ── Tier 2: API Key (4) ────────────────────────────────────────
        entry(
            "deepseek",
            "DeepSeek",
            "Coding Plan",
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
        // ── Tier 3: Manual (9) ─────────────────────────────────────────
        entry(
            "xiaomi-mimo",
            "小米 MiMo",
            "Token Plan",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "FF6700",
            "CNY",
            "https://platform.xiaomimimo.com/console/plan-manage",
        ),
        CatalogEntry {
            id: "volc-ark",
            display_name: "火山方舟",
            description: "豆包 Coding Plan",
            tier: CatalogTier::Manual,
            auth_modes: MANUAL_ONLY,
            brand_color: "1664FF",
            default_currency: "CNY",
            subscription_url: "https://www.volcengine.com/product/ark",
            warning: Some(
                "服务条款禁止直接 API 调用 Coding Plan key，请在控制台查看后手动维护。",
            ),
            regions: NO_REGIONS,
        },
        entry(
            "tencent-hy3",
            "腾讯 Hy3",
            "Token Plan",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "00A4FF",
            "CNY",
            "https://hunyuan.tencent.com",
        ),
        entry(
            "stepfun",
            "阶跃 Step",
            "Step Plan",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "00B5A9",
            "CNY",
            "https://platform.stepfun.com/account-overview",
        ),
        entry(
            "alibaba-bailian",
            "阿里百炼",
            "通义 / Bailian",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "FF6A00",
            "CNY",
            "https://bailian.console.aliyun.com",
        ),
        entry(
            "iflytek-spark",
            "讯飞星火",
            "Astron Coding Plan",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "1E88E5",
            "CNY",
            "https://console.xfyun.cn",
        ),
        entry(
            "baichuan",
            "百川 Baichuan",
            "Baichuan API",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "FFB400",
            "CNY",
            "https://platform.baichuan-ai.com",
        ),
        entry(
            "lingyi",
            "零一万物",
            "Yi / 01.AI",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "0EA5E9",
            "CNY",
            "https://platform.lingyiwanwu.com",
        ),
        entry(
            "shangtang",
            "商汤日日新",
            "SenseChat",
            CatalogTier::Manual,
            MANUAL_ONLY,
            "EF4444",
            "CNY",
            "https://console.sensecore.cn",
        ),
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
    fn catalog_has_18_entries() {
        assert_eq!(catalog().len(), 18);
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
        let manual = c.iter().filter(|e| e.tier == CatalogTier::Manual).count();
        assert_eq!(oauth, 5);
        assert_eq!(api_key, 4);
        assert_eq!(manual, 9);
    }

    #[test]
    fn trae_has_regions() {
        let trae = find("trae").expect("trae catalog entry");
        assert_eq!(trae.regions.len(), 4);
    }

    #[test]
    fn volc_ark_has_warning() {
        let volc = find("volc-ark").expect("volc-ark catalog entry");
        assert!(volc.warning.is_some());
    }
}
