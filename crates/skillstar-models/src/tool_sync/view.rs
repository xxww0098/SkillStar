//! Reading a v4 [`Provider`] the way a config writer needs it.
//!
//! Every agent's config file wants plain strings: a base URL, a key, a model
//! id. v4 stores those as `Option<String>` and as a [`Credential`] union,
//! because "absent" and "empty" are different facts about a provider and the
//! store is where that distinction has to survive. The writers do not need the
//! distinction — for them an absent endpoint and an empty one both mean "skip
//! this row" — so the projection happens here, once, instead of in every
//! writer.
//!
//! Keeping it in one place is also what makes the byte-for-byte comparison
//! against v3's output checkable: each function below is the v4 spelling of
//! exactly one v3 field, and the correspondence is the whole contract.
//!
//! [`Credential`]: crate::providers::Credential

use crate::providers::{Provider, RequiredWire};

/// v3's `base_url_openai`.
pub(crate) fn openai_base(provider: &Provider) -> &str {
    provider.endpoints.openai_chat.as_deref().unwrap_or("")
}

/// v3's `base_url_anthropic`.
pub(crate) fn anthropic_base(provider: &Provider) -> &str {
    provider
        .endpoints
        .anthropic_messages
        .as_deref()
        .unwrap_or("")
}

/// The `/v1/responses` endpoint, the only one Codex ≥0.95 can use.
pub(crate) fn responses_base(provider: &Provider) -> &str {
    provider.endpoints.openai_responses.as_deref().unwrap_or("")
}

/// v3's `api_key`.
///
/// Only literal keys resolve. `EnvVar` / `File` / `Command` yield an empty
/// string here and are carried to the agent as a pointer by the writer that
/// knows how to spell one (Codex `env_key`, OpenCode `{env:NAME}`); expanding
/// them here would bake this machine's environment into the config file.
pub(crate) fn api_key(provider: &Provider) -> &str {
    provider.credential.literal_secret().unwrap_or("")
}

/// v3's `default_model`.
pub(crate) fn default_model(provider: &Provider) -> &str {
    provider.default_model.as_deref().unwrap_or("")
}

/// Whether this row can be projected to an agent speaking `wire` at all.
///
/// A row with no endpoint for the protocol is skipped rather than written with
/// an empty URL — an agent config naming a provider with `base_url = ""` fails
/// at request time with an error that points nowhere near the cause.
pub(crate) fn serves(provider: &Provider, wire: RequiredWire) -> bool {
    !provider.endpoint_for(wire).unwrap_or("").trim().is_empty()
}
