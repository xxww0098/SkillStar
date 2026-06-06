//! Canonical provider-identity map reconciling the two domain id-spaces.
//!
//! SkillStar keeps two intentionally separate provider lists (see crate docs):
//! the usage-side subscription catalog (`skillstar-usage::catalog`) and the
//! models-side routing presets (`skillstar-models::providers::get_all_presets_flat`).
//! Their ids do not line up 1:1:
//!
//! - the catalog id `glm` corresponds to **two** preset variants `glm` + `glm-coding`;
//! - the catalog id `xiaomi-mimo` corresponds to the preset id `mimo`;
//! - the catalog id `kimi` corresponds to presets `kimi` + `kimi-coding`;
//! - many catalog ids (OAuth IDEs) have no preset, and many presets
//!   (`qwen`, `openrouter`, …) have no subscription catalog entry.
//!
//! Rather than merge the two lists into one sparse mega-table — which would pull
//! both domains' concerns into this leaf crate — we record the correspondence
//! here as a small, tested map. Conformance tests in the consuming crates assert
//! that every real catalog id and every real preset id resolves to exactly one
//! identity, so the mapping can never silently drift.

/// One logical provider and the ids each domain uses to refer to it.
#[derive(Debug, Clone, Copy)]
pub struct ProviderIdentity {
    /// Stable canonical id for this provider across SkillStar.
    pub canonical_id: &'static str,
    /// Human-readable name.
    pub display_name: &'static str,
    /// The id used by the usage-side `catalog()` row, if this provider has a
    /// subscription-account presence. `None` for models-only providers.
    pub catalog_id: Option<&'static str>,
    /// The model-routing preset ids (`get_all_presets_flat`). Empty for
    /// subscription-only providers; multiple when a provider exposes endpoint
    /// variants (e.g. GLM = `glm` + `glm-coding`).
    pub preset_ids: &'static [&'static str],
}

/// The canonical provider table. Covers every catalog id and every preset id.
pub const PROVIDER_IDENTITIES: &[ProviderIdentity] = &[
    // ── Providers present in BOTH domains (the id-granularity reconciliation) ──
    ProviderIdentity {
        canonical_id: "deepseek",
        display_name: "DeepSeek",
        catalog_id: Some("deepseek"),
        preset_ids: &["deepseek"],
    },
    ProviderIdentity {
        canonical_id: "kimi",
        display_name: "Kimi",
        catalog_id: Some("kimi"),
        preset_ids: &["kimi", "kimi-coding"],
    },
    ProviderIdentity {
        canonical_id: "glm",
        display_name: "智谱 GLM",
        catalog_id: Some("glm"),
        preset_ids: &["glm", "glm-coding"],
    },
    ProviderIdentity {
        canonical_id: "minimax",
        display_name: "MiniMax",
        catalog_id: Some("minimax"),
        preset_ids: &["minimax"],
    },
    ProviderIdentity {
        canonical_id: "xiaomi-mimo",
        display_name: "小米 MiMo",
        catalog_id: Some("xiaomi-mimo"),
        preset_ids: &["mimo"],
    },
    // ── Models-only providers (routing presets with no subscription account) ──
    ProviderIdentity {
        canonical_id: "qwen",
        display_name: "通义千问",
        catalog_id: None,
        preset_ids: &["qwen", "qwen-coding"],
    },
    ProviderIdentity {
        canonical_id: "volcengine",
        display_name: "火山方舟",
        catalog_id: None,
        preset_ids: &["volcengine"],
    },
    ProviderIdentity {
        canonical_id: "openrouter",
        display_name: "OpenRouter",
        catalog_id: None,
        preset_ids: &["openrouter"],
    },
    ProviderIdentity {
        canonical_id: "siliconflow",
        display_name: "SiliconFlow",
        catalog_id: None,
        preset_ids: &["siliconflow"],
    },
    ProviderIdentity {
        canonical_id: "grok",
        display_name: "Grok (xAI)",
        catalog_id: Some("xai"),
        preset_ids: &["grok"],
    },
    // ── Subscription-only providers (OAuth / Cookie / Manual; no routing preset) ──
    ProviderIdentity {
        canonical_id: "cursor",
        display_name: "Cursor",
        catalog_id: Some("cursor"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "codex",
        display_name: "Codex",
        catalog_id: Some("codex"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "antigravity",
        display_name: "Antigravity",
        catalog_id: Some("antigravity"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "trae",
        display_name: "Trae",
        catalog_id: Some("trae"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "qoder",
        display_name: "Qoder",
        catalog_id: Some("qoder"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "kiro",
        display_name: "Kiro",
        catalog_id: Some("kiro"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "windsurf",
        display_name: "Windsurf",
        catalog_id: Some("windsurf"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "github-copilot",
        display_name: "GitHub Copilot",
        catalog_id: Some("github-copilot"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "stepfun",
        display_name: "阶跃 Step",
        catalog_id: Some("stepfun"),
        preset_ids: &[],
    },
    ProviderIdentity {
        canonical_id: "opencode",
        display_name: "OpenCode",
        catalog_id: Some("opencode"),
        preset_ids: &[],
    },
];

/// Resolve the canonical identity for a usage-side catalog id.
pub fn identity_for_catalog(catalog_id: &str) -> Option<&'static ProviderIdentity> {
    PROVIDER_IDENTITIES
        .iter()
        .find(|p| p.catalog_id == Some(catalog_id))
}

/// Resolve the canonical identity for a models-side preset id.
pub fn identity_for_preset(preset_id: &str) -> Option<&'static ProviderIdentity> {
    PROVIDER_IDENTITIES
        .iter()
        .find(|p| p.preset_ids.contains(&preset_id))
}

/// Resolve by canonical id.
pub fn identity(canonical_id: &str) -> Option<&'static ProviderIdentity> {
    PROVIDER_IDENTITIES
        .iter()
        .find(|p| p.canonical_id == canonical_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn canonical_ids_are_unique() {
        let mut seen = BTreeSet::new();
        for p in PROVIDER_IDENTITIES {
            assert!(
                seen.insert(p.canonical_id),
                "duplicate canonical_id: {}",
                p.canonical_id
            );
        }
    }

    #[test]
    fn catalog_ids_are_unique() {
        let mut seen = BTreeSet::new();
        for p in PROVIDER_IDENTITIES.iter().filter_map(|p| p.catalog_id) {
            assert!(seen.insert(p), "duplicate catalog_id mapping: {p}");
        }
    }

    #[test]
    fn preset_ids_are_globally_unique() {
        let mut seen = BTreeSet::new();
        for id in PROVIDER_IDENTITIES.iter().flat_map(|p| p.preset_ids.iter()) {
            assert!(seen.insert(*id), "preset id mapped to two identities: {id}");
        }
    }

    #[test]
    fn granularity_mismatches_are_pinned() {
        // catalog "glm" spans two preset variants.
        assert_eq!(
            identity_for_catalog("glm").unwrap().preset_ids,
            &["glm", "glm-coding"]
        );
        // catalog "xiaomi-mimo" maps to preset "mimo".
        assert_eq!(
            identity_for_preset("mimo").unwrap().canonical_id,
            "xiaomi-mimo"
        );
        // catalog "kimi" spans "kimi" + "kimi-coding".
        assert_eq!(
            identity_for_preset("kimi-coding").unwrap().canonical_id,
            "kimi"
        );
    }

    #[test]
    fn every_api_key_balance_spec_has_an_identity() {
        for spec in crate::balance::API_KEY_BALANCE_SPECS {
            assert!(
                identity_for_catalog(spec.catalog_id).is_some(),
                "balance spec {} has no provider identity",
                spec.catalog_id
            );
        }
    }
}
