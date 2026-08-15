//! Reference type pointing from `AiConfig` into the model provider store.
//!
//! `AiConfig` itself lives in `skillstar-models::ai_provider` because it carries
//! inference-time state (api_format / model / preset etc.), but its provider
//! pointer belongs to the models domain, so we keep the type here and re-export
//! it from `skillstar-models::ai_provider` for backward compatibility.
//!
//! ## Why the field is `agent_id` and not `app_id`
//!
//! v3 called it `app_id` and accepted exactly two values, `"claude"` and
//! `"codex"` — a private two-element id space that overlapped the agent
//! registry's without matching it (`"claude"` vs `"claude-code"`). It was the
//! fifth id space in the models domain, and the only reason it existed was that
//! nobody had reconciled it with the fourth. Everything downstream had to know
//! which spelling it was holding.
//!
//! v4 uses agent ids. `"claude"` migrates to `"claude-code"`, `"codex"` stays
//! as it is, and the resolver looks the wire protocol up in the agent registry
//! instead of matching on a hardcoded pair.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AiProviderRef {
    /// An id from the agent registry (`claude-code`, `codex`, …).
    ///
    /// `alias` accepts the v3 spelling so an `ai.json` written by an older
    /// build still parses; [`AiProviderRef::normalized_agent_id`] maps its
    /// values forward. Migrating a one-field pointer through serde is cheaper
    /// and less breakable than a file-format migration for it.
    #[serde(default, alias = "app_id")]
    pub agent_id: String,
    #[serde(default)]
    pub provider_id: String,
}

impl AiProviderRef {
    /// The agent id, with v3's two legacy spellings mapped forward.
    pub fn normalized_agent_id(&self) -> &str {
        normalize_agent_id(&self.agent_id)
    }
}

/// Map a v3 `app_id` onto the agent registry's id space.
///
/// Only `"claude"` needs mapping; `"codex"` was already the registry's
/// spelling. Anything else is passed through, because a value this function
/// does not recognise is a value it has no business rewriting.
pub fn normalize_agent_id(app_or_agent_id: &str) -> &str {
    match app_or_agent_id.trim() {
        "claude" => "claude-code",
        other => other,
    }
}
