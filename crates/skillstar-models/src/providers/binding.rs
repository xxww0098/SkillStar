//! v4 bindings: which providers an agent uses, and which model plays which role.
//!
//! ## What changed and why
//!
//! v3 called the map `tool_activations` but stored `ToolBinding` values — the
//! key said "activation", the type said "binding", and they had disagreed for
//! three schema versions. v4 calls it `bindings` and calls the entry a
//! [`BindingEntry`], matching the doc comment v3 already had.
//!
//! The substantive change is [`AgentBinding::roles`]. In v3 the same concept
//! lived in two places: Claude's tiered models sat in `provider.meta` as
//! `claude_haiku_model` / `claude_sonnet_model` / `claude_opus_model`, while
//! OMP's role routing sat in the binding's untyped `settings` bag. Neither
//! location was reachable by the other agent, so role routing could never be
//! generalised. v4 promotes it to a first-class field on the binding, which is
//! the only level that can express the thing OMP actually needs: a role may
//! point at a *different provider* than the active one.
//!
//! `roles` is an open `BTreeMap`, not an enum. A closed enum forces a
//! three-place edit to add a role and drifts the moment one place is missed;
//! an open map costs nothing and cannot drift.

use super::provider::Tri;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use ts_rs::TS;

/// On-disk schema version of the v4 provider store.
pub const STORE_VERSION_V4: u32 = 4;

/// The one model-reference shape used across the whole domain.
///
/// A model reference is a **triple**, not a pair. OMP writes
/// `provider/model:level`, Crush writes `{provider, model, reasoning_effort}`,
/// Codex writes `model` + `model_provider` + `model_reasoning_effort`: three
/// projects, three spellings, the same three values. Any abstraction that
/// carries only `(provider, model)` drops the reasoning tier at write time,
/// silently, and the user's chosen effort setting stops taking effect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export, export_to = "ModelRef.ts")]
pub struct ModelRef {
    /// SkillStar's own provider id — **not** the on-disk `skillstar_*` key.
    /// That key is derived at write time so it always tracks the managed-key
    /// rule; storing it here would freeze a stale copy.
    pub provider_id: String,
    pub model: String,
    /// Canonical reasoning tier, narrowed per target at write time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Agent-specific extras that are not the effort tier (Claude's
    /// `fallback_model`, for instance). Sparse; read through typed accessors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown | null")]
    pub ext: Option<serde_json::Value>,
}

impl ModelRef {
    pub fn new(provider_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: model.into(),
            effort: None,
            ext: None,
        }
    }

    /// Whether this reference names an actual model. An incomplete role must
    /// never overwrite what the user already has on disk.
    pub fn is_complete(&self) -> bool {
        !self.model.trim().is_empty() && !self.provider_id.trim().is_empty()
    }
}

/// Canonical reasoning tier.
///
/// The value set is the widest superset in play (models.dev's), so that
/// narrowing happens at projection time where the target's limits are known:
/// Crush accepts only low/medium/high and needs the nearest value, Codex takes
/// a free string, OMP remaps freely. Narrowing early would lose information
/// that a different target could have used.
///
/// Note that "reasoning off" and "reasoning tier = none" are different
/// requests: the former omits the reasoning block entirely (`effort: None`),
/// the latter sends an explicit `none` (`effort: Some(Effort::None)`).
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "Effort.ts")]
pub enum Effort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    /// Parse one of OMP's nine thinking levels into the canonical tier.
    ///
    /// `inherit` and `auto` deliberately map to `None` (the outer `Option`,
    /// i.e. "do not set an effort"): they are instructions to defer, not
    /// tiers, and writing them as a tier would pin a value the user asked to
    /// leave floating. Anything unrecognised also yields `None` rather than a
    /// guess.
    pub fn from_omp_thinking(level: &str) -> Option<Self> {
        match level.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Effort::None),
            "minimal" => Some(Effort::Minimal),
            "low" => Some(Effort::Low),
            "medium" => Some(Effort::Medium),
            "high" => Some(Effort::High),
            "xhigh" => Some(Effort::Xhigh),
            "max" => Some(Effort::Max),
            // "inherit" | "auto" | anything else
            _ => None,
        }
    }

    /// Render back into OMP's spelling, for writers that still speak it.
    pub fn as_omp_thinking(self) -> &'static str {
        match self {
            Effort::None => "off",
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

/// Everything one agent is bound to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentBinding {
    #[serde(default)]
    pub entries: Vec<BindingEntry>,
    /// Which entry owns the agent's "active" pointer on disk. A stale index is
    /// clamped rather than treated as an error, preserving v3's behaviour.
    #[serde(default)]
    pub active_index: usize,

    /// Role → model routing, keyed by canonical role id (see the agent
    /// registry) with unknown keys preserved verbatim as extra roles.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, ModelRef>,

    /// Agent-private configuration that is **not** role routing. With roles
    /// lifted out, very little is left here, which is the point: the two
    /// settings bags (this one and [`BindingEntry::settings`]) stopped being
    /// easy to confuse once each had one job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}

impl AgentBinding {
    /// A binding holding exactly one entry.
    pub fn single(entry: BindingEntry) -> Self {
        Self {
            entries: vec![entry],
            active_index: 0,
            roles: BTreeMap::new(),
            settings: None,
        }
    }

    /// The active entry, clamping a stale `active_index` to the last entry.
    pub fn active(&self) -> Option<&BindingEntry> {
        if self.entries.is_empty() {
            return None;
        }
        let idx = self.active_index.min(self.entries.len() - 1);
        self.entries.get(idx)
    }

    /// Mutable access to the active entry, clamped like [`Self::active`].
    pub fn active_mut(&mut self) -> Option<&mut BindingEntry> {
        if self.entries.is_empty() {
            return None;
        }
        let idx = self.active_index.min(self.entries.len() - 1);
        self.entries.get_mut(idx)
    }

    /// Whether any entry binds the given provider.
    pub fn binds_provider(&self, provider_id: &str) -> bool {
        self.entries.iter().any(|e| e.provider_id == provider_id)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One provider+model pair an agent can use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BindingEntry {
    pub provider_id: String,
    pub model: String,
    /// Per-entry settings: "how this provider behaves under this agent".
    /// Codex's `auth_mode` lives here in v4 rather than on the provider row,
    /// where it applied to agents that have no such concept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    /// **Milliseconds** — v3 stored this one in seconds while storing
    /// `created_at` in milliseconds, in the same file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at_ms: Option<u64>,
}

impl BindingEntry {
    pub fn new(provider_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: model.into(),
            settings: None,
            last_sync_at_ms: None,
        }
    }
}

/// Root of `model_providers.json` at v4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvidersStoreV4 {
    pub version: u32,
    #[serde(default)]
    pub providers: Vec<super::provider::Provider>,
    /// Keyed by agent id. Renamed from v3's `tool_activations`.
    #[serde(default)]
    pub bindings: HashMap<String, AgentBinding>,
}

impl Default for ProvidersStoreV4 {
    fn default() -> Self {
        Self {
            version: STORE_VERSION_V4,
            providers: Vec::new(),
            bindings: HashMap::new(),
        }
    }
}

impl ProvidersStoreV4 {
    pub fn provider(&self, id: &str) -> Option<&super::provider::Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Whether any binding can still reach the given provider through an
    /// endpoint the agent's wire protocol requires.
    pub fn provider_caps_deny(&self, provider_id: &str, wire: super::provider::RequiredWire) -> bool {
        let Some(provider) = self.provider(provider_id) else {
            return false;
        };
        let cap = match wire {
            super::provider::RequiredWire::OpenaiResponses => provider.caps.responses_api,
            super::provider::RequiredWire::AnthropicMessages => provider.caps.anthropic_messages,
            super::provider::RequiredWire::OpenaiChat => Tri::Yes,
        };
        cap.is_denied()
    }
}
