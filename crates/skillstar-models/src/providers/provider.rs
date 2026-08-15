//! v4 provider row: one writable connection target.
//!
//! ## Why this replaces `ProviderEntryFlat`
//!
//! v3 encoded a provider as a flat bag of two URL strings plus two
//! Codex-specific strings. Three things went wrong with that shape and all
//! three are fixed here rather than papered over:
//!
//! 1. **Protocol is a property of the endpoint, not of the provider.** A relay
//!    routinely exposes `/v1/chat/completions` and `/anthropic` at once (the
//!    `deepseek` preset does exactly that), so [`Endpoints`] is a record of
//!    optional URLs, one per protocol, instead of two hardcoded fields.
//! 2. **"Not probed" and "not supported" were indistinguishable.** v3 inferred
//!    Codex's wire protocol from whether a field still equalled its serde
//!    default, so a user who deliberately picked a value looked identical to a
//!    user who never touched it. [`Tri`] makes "unknown" a value you can hold.
//! 3. **Time units lived in the reader's head.** v3 stored `created_at` in
//!    milliseconds and `last_sync_at` in seconds, in the same file, with
//!    nothing in either name to say so. Every v4 timestamp carries `_ms`.
//!
//! The credential is deliberately *not* a string on this struct — see
//! [`crate::providers::credential`] for why it is a discriminated union.

use super::credential::Credential;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// A provider = one writable connection target.
///
/// Invariant, copied from Cherry Studio's model: **one provider instance = one
/// API host**. A vendor that runs an official host and a relay is two rows,
/// associated through `preset_id` and folded together by the UI — never one row
/// holding two hosts. Anything else makes "which URL failed" unanswerable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    /// Stable id. Third-party rows get a UUIDv4; native-login rows keep a fixed
    /// slug (`claude-official`). Never changes once written: the ids are what
    /// agent bindings point at, so regenerating one silently unbinds the user.
    pub id: String,

    /// Display name. User-editable.
    pub name: String,

    /// Link into the preset registry. `None` = fully custom.
    ///
    /// A preset supplies *defaults*, never unoverridable facts — the row on
    /// disk is the overlay, the preset stays in code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,

    /// Which protocol endpoints this host offers.
    #[serde(default)]
    pub endpoints: Endpoints,

    /// How to authenticate. See [`Credential`].
    #[serde(default = "Credential::unknown_local")]
    pub credential: Credential,

    /// Custom request headers, projected into each agent's own spelling
    /// (OpenCode `options.headers`, Codex `http_headers`, Crush
    /// `extra_headers`, Claude `ANTHROPIC_CUSTOM_HEADERS`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    /// Capability bits: answers "can this host be bound to Codex / to Claude
    /// Code at all". Codex ≥0.95 speaks only the Responses API, so a host that
    /// does not implement `/v1/responses` is not bindable to it — that is a
    /// fact about the host, and it belongs here.
    #[serde(default)]
    pub caps: ProviderCaps,

    /// Model ids the user has **adopted** — a whitelist, not a discovery
    /// result. Relays return hundreds of models; the discovered set lives in
    /// the catalog cache, and only what the user kept lands here.
    #[serde(default)]
    pub models: Vec<String>,

    /// Fallback model used when a binding does not name one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    #[serde(default)]
    pub sort_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// Creation time in **milliseconds**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_ms: Option<u64>,

    /// The one remaining schema-free pocket.
    ///
    /// v3's `meta` carried five unrelated concerns with no central definition.
    /// Four of them are named fields in v4; this holds only what genuinely
    /// cannot be predicted, and [`crate::providers::migrate`] parks unknown
    /// v3 `meta` keys here rather than guessing at them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

impl Provider {
    /// A minimal row: id + name, everything else at its default.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            preset_id: None,
            endpoints: Endpoints::default(),
            credential: Credential::unknown_local(),
            headers: BTreeMap::new(),
            caps: ProviderCaps::default(),
            models: Vec::new(),
            default_model: None,
            sort_index: 0,
            icon_color: None,
            notes: None,
            created_at_ms: None,
            ext: None,
        }
    }

    /// Whether this row is a native-login row whose credentials live in some
    /// other CLI's store.
    ///
    /// This replaces v3's `is_native_official_preset_id` id whitelist. The
    /// whitelist had to be consulted in six places because "official" was not
    /// representable in the data; here it is one match on the credential.
    pub fn is_external_cli(&self) -> bool {
        matches!(self.credential, Credential::ExternalCli { .. })
    }

    /// The endpoint an agent needs, given which wire protocol it speaks.
    pub fn endpoint_for(&self, wire: RequiredWire) -> Option<&str> {
        match wire {
            RequiredWire::OpenaiChat => self.endpoints.openai_chat.as_deref(),
            RequiredWire::OpenaiResponses => self.endpoints.openai_responses.as_deref(),
            RequiredWire::AnthropicMessages => self.endpoints.anthropic_messages.as_deref(),
        }
    }
}

/// Which protocol an agent requires from the provider it is bound to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "RequiredWire.ts")]
pub enum RequiredWire {
    OpenaiChat,
    OpenaiResponses,
    AnthropicMessages,
}

/// The protocol endpoints one host exposes.
///
/// Every field is optional because "this host does not offer that protocol" is
/// a normal state, not an error. An empty string in v3 meant the same thing but
/// could not be told apart from "not yet filled in".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export, export_to = "Endpoints.ts")]
pub struct Endpoints {
    /// `/v1/chat/completions` — OpenCode / Pi / OMP / Crush enter here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_chat: Option<String>,
    /// `/v1/responses` — **the only entry Codex ≥0.95 accepts**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_responses: Option<String>,
    /// `/v1/messages` — the only entry Claude Code accepts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_messages: Option<String>,
    /// Model-discovery endpoint. v3's `models_url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_list: Option<String>,
}

impl Endpoints {
    /// Whether every endpoint is absent — the shape a native-login row has.
    pub fn is_empty(&self) -> bool {
        self.openai_chat.is_none()
            && self.openai_responses.is_none()
            && self.anthropic_messages.is_none()
            && self.models_list.is_none()
    }
}

/// Three-state capability bit.
///
/// `Unknown` is the initial value and the only honest one before a probe has
/// run. Migration writes `Unknown`, never `No`: recording "never probed" as
/// "unsupported" would disable a user's working binding on upgrade with no
/// obvious way back.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "Tri.ts")]
pub enum Tri {
    #[default]
    Unknown,
    Yes,
    No,
}

impl Tri {
    /// Whether this capability is known to be absent. Only an explicit `No`
    /// blocks a binding — `Unknown` means "ask the user to probe", not "deny".
    pub fn is_denied(self) -> bool {
        matches!(self, Tri::No)
    }
}

/// Where a capability bit's current value came from.
///
/// The UI is required to show this: a bit that came from a preset default and
/// a bit that came from a live probe warrant different user confidence, and
/// hiding the difference is how "it says supported but it doesn't work"
/// becomes unexplainable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "CapSource.ts")]
pub enum CapSource {
    /// P0 — declared by the preset the row was created from.
    #[default]
    Preset,
    /// P1 — observed by probing the endpoint.
    Probe,
    /// P2 — set by the user in the advanced section.
    UserOverride,
}

/// What this host can actually do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export, export_to = "ProviderCaps.ts")]
pub struct ProviderCaps {
    /// Speaks `/v1/responses`. Gates the Codex column.
    #[serde(default)]
    pub responses_api: Tri,
    /// Speaks the Anthropic Messages protocol. Gates the Claude Code column.
    ///
    /// Anthropic's published position is that routing Claude Code to
    /// non-Claude models through a gateway is unsupported, so this bit must
    /// come from an actual probe rather than from an assumption, and the UI
    /// has to say that it is an undocumented use.
    #[serde(default)]
    pub anthropic_messages: Tri,
    /// `/v1/models` answers. Gates the "fetch models" button.
    #[serde(default)]
    pub models_list: Tri,
    #[serde(default)]
    pub source: CapSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub probed_at_ms: Option<u64>,
}

impl ProviderCaps {
    /// All bits `Unknown`, attributed to the preset level. What migration
    /// writes for every row it has not probed — which is all of them, because
    /// migration is a pure offline function.
    pub fn unknown() -> Self {
        Self::default()
    }
}
