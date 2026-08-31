//! v4 store operations: provider rows, and the split binding commands.
//!
//! ## Why `activate_tool` became three commands
//!
//! v3's `activate_tool` did three unrelated jobs behind one name: append a new
//! provider to an agent's list, change which bound provider is active, and
//! rewrite an existing entry's model or settings. Because it was the only entry
//! point, moving the active pointer meant re-supplying the model *and* the
//! settings; a caller that had only the pointer to change had to first read the
//! entry back so as not to clobber the rest of it. `set_active_binding` already
//! existed for exactly that but no command layer reached it.
//!
//! v4 splits them: [`bind_provider`] adds, [`set_active_binding`] points, and
//! [`update_binding_entry`] edits. Each one names what it does and touches
//! nothing else.
//!
//! ## Why `deactivate_tool` became two
//!
//! v3's `deactivate_tool` was not the inverse of `activate_tool`: it emptied
//! the agent's *entire* entry list. Once agents could hold several providers,
//! every per-row "unbind" button in the UI was wired to it, so unbinding one of
//! three providers silently unbound all three. [`unbind_provider`] removes one
//! row and [`unbind_agent`] clears the agent; the destructive one now has to be
//! asked for by name.

use super::binding::{AgentBinding, BindingEntry, ProvidersStoreV4};
use super::provider::{Provider, RequiredWire};
use anyhow::{Context, Result, bail};
use url::Url;

// ---------------------------------------------------------------------------
// Provider rows
// ---------------------------------------------------------------------------

/// Validate an optional endpoint URL. `None` and empty are both fine — "this
/// host does not offer that protocol" is a normal state, not an error.
fn validate_endpoint(url: Option<&String>) -> Result<()> {
    let Some(url) = url
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(());
    };
    let parsed = Url::parse(url).with_context(|| format!("Invalid URL format: '{url}'"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => bail!("URL must use http or https scheme, got: '{scheme}'"),
    }
}

/// Validate every endpoint on a row.
pub fn validate_provider(provider: &Provider) -> Result<()> {
    if provider.name.trim().is_empty() {
        bail!("Provider name must not be empty");
    }
    validate_endpoint(provider.endpoints.openai_chat.as_ref())?;
    validate_endpoint(provider.endpoints.openai_responses.as_ref())?;
    validate_endpoint(provider.endpoints.anthropic_messages.as_ref())?;
    validate_endpoint(provider.endpoints.models_list.as_ref())?;
    Ok(())
}

/// Insert a new provider row.
///
/// The caller owns the id: native-login rows keep their fixed slug, everything
/// else arrives with a fresh UUID. v3 minted the UUID here and overwrote
/// whatever the caller passed, which made seeding a fixed-id row a special case
/// threaded through a whitelist.
pub fn create_provider(store: &mut ProvidersStoreV4, mut provider: Provider) -> Result<Provider> {
    validate_provider(&provider)?;
    if store.providers.iter().any(|p| p.id == provider.id) {
        bail!("Provider '{}' already exists", provider.id);
    }

    if provider.created_at_ms.is_none() {
        provider.created_at_ms = Some(now_ms());
    }
    provider.sort_index = if store.providers.is_empty() {
        0
    } else {
        store
            .providers
            .iter()
            .map(|p| p.sort_index)
            .max()
            .unwrap_or(0)
            + 1
    };

    store.providers.push(provider.clone());
    Ok(provider)
}

/// Replace a provider row in place, keeping its position in the list.
pub fn replace_provider(store: &mut ProvidersStoreV4, provider: Provider) -> Result<Provider> {
    validate_provider(&provider)?;
    let slot = store
        .providers
        .iter_mut()
        .find(|p| p.id == provider.id)
        .with_context(|| format!("Provider '{}' not found", provider.id))?;
    *slot = provider.clone();
    Ok(provider)
}

/// Delete a provider and every reference to it.
///
/// References outlive the row in three places, and v3 pruned only the first
/// two: the entry list, the active pointer, and the role map. A role left
/// pointing at a deleted provider writes a dangling `skillstar_<id>/<model>`
/// into the agent's config, which the agent then fails to resolve.
pub fn delete_provider(store: &mut ProvidersStoreV4, id: &str) -> Result<()> {
    let idx = store
        .providers
        .iter()
        .position(|p| p.id == id)
        .with_context(|| format!("Provider '{id}' not found"))?;
    store.providers.remove(idx);

    for binding in store.bindings.values_mut() {
        drop_entry(binding, id);
        binding.roles.retain(|_, target| target.provider_id != id);
    }
    Ok(())
}

/// Reorder by assigning `sort_index = position`. Rows not named keep theirs.
pub fn reorder_providers(store: &mut ProvidersStoreV4, ordered_ids: &[String]) -> Result<()> {
    for id in ordered_ids {
        if !store.providers.iter().any(|p| p.id == *id) {
            bail!("Provider '{id}' not found in store");
        }
    }
    for (index, id) in ordered_ids.iter().enumerate() {
        if let Some(provider) = store.providers.iter_mut().find(|p| p.id == *id) {
            provider.sort_index = index as u32;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Binding commands
// ---------------------------------------------------------------------------

/// Why a provider cannot be bound to an agent.
///
/// A typed reason rather than a formatted string: the UI has to distinguish
/// "this will never work" (no such endpoint) from "we have not checked yet"
/// (`Tri::Unknown` plus a probe button), and matching on prose is not a
/// distinction, it is a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindRefusal {
    /// The host exposes no endpoint for the protocol this agent speaks.
    NoEndpoint {
        wire: RequiredWire,
        provider_name: String,
    },
    /// A probe established that the host does not speak this protocol.
    CapabilityDenied {
        wire: RequiredWire,
        provider_name: String,
    },
}

impl std::fmt::Display for BindRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindRefusal::NoEndpoint {
                wire,
                provider_name,
            } => write!(
                f,
                "Provider '{provider_name}' has no {} endpoint, which this agent requires",
                wire_label(*wire)
            ),
            BindRefusal::CapabilityDenied {
                wire,
                provider_name,
            } => write!(
                f,
                "Provider '{provider_name}' was probed and does not support {}",
                wire_label(*wire)
            ),
        }
    }
}

fn wire_label(wire: RequiredWire) -> &'static str {
    match wire {
        RequiredWire::OpenaiChat => "an OpenAI-compatible /v1/chat/completions",
        RequiredWire::OpenaiResponses => "the OpenAI /v1/responses",
        RequiredWire::AnthropicMessages => "an Anthropic /v1/messages",
    }
}

/// Whether a provider can serve an agent that speaks `wire`.
///
/// Native-login rows are exempt: their endpoints are empty *on purpose*, and
/// syncing one means clearing SkillStar's managed fields so the agent's own
/// login takes over. Gating them on an endpoint would make "use my ChatGPT
/// login" unbindable.
///
/// `Tri::Unknown` never refuses. Migration writes `Unknown` for every row, so
/// treating it as a denial would disable every existing binding on upgrade.
pub fn check_bindable(provider: &Provider, wire: RequiredWire) -> Result<(), BindRefusal> {
    if provider.is_external_cli() {
        return Ok(());
    }
    let cap = match wire {
        RequiredWire::OpenaiChat => provider.caps.models_list, // never consulted; see below
        RequiredWire::OpenaiResponses => provider.caps.responses_api,
        RequiredWire::AnthropicMessages => provider.caps.anthropic_messages,
    };
    // `OpenaiChat` has no capability bit of its own: every OpenAI-compatible
    // host implements it by definition, so the endpoint's presence is the whole
    // question. The read above is discarded for that arm.
    if !matches!(wire, RequiredWire::OpenaiChat) && cap.is_denied() {
        return Err(BindRefusal::CapabilityDenied {
            wire,
            provider_name: provider.name.clone(),
        });
    }
    if provider
        .endpoint_for(wire)
        .is_none_or(|u| u.trim().is_empty())
    {
        return Err(BindRefusal::NoEndpoint {
            wire,
            provider_name: provider.name.clone(),
        });
    }
    Ok(())
}

/// Bind a provider to an agent, and make it the active one.
///
/// Adding a provider to an agent's list *is* choosing it — a bind that left the
/// pointer elsewhere would need a second click to do the thing the first click
/// asked for. Re-binding an already-bound provider updates its model and points
/// at it, which is what the UI's "switch to this row" affordance means; the
/// pointer-only and edit-only operations have their own commands.
pub fn bind_provider(
    store: &mut ProvidersStoreV4,
    agent_id: &str,
    provider_id: &str,
    model: Option<&str>,
    settings: Option<serde_json::Value>,
) -> Result<BindingEntry> {
    let provider = store
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .with_context(|| format!("Provider '{provider_id}' not found"))?;

    let wire = crate::tool_sync::agent_spec(agent_id)
        .map(|spec| spec.required_wire)
        // An unknown agent id keeps the safest assumption rather than binding
        // to nothing: chat is the protocol every OpenAI-compatible host has.
        .unwrap_or(RequiredWire::OpenaiChat);
    check_bindable(provider, wire).map_err(|refusal| anyhow::anyhow!("{refusal}"))?;

    let resolved_model = match model {
        Some(m) if !m.trim().is_empty() => m.to_string(),
        _ => provider.default_model.clone().unwrap_or_default(),
    };
    let multi = agent_supports_multiple_providers(agent_id);
    let external_cli = provider.is_external_cli();

    let existing_settings = store
        .bindings
        .get(agent_id)
        .and_then(|b| b.entries.iter().find(|e| e.provider_id == provider_id))
        .and_then(|e| e.settings.clone());

    let mut entry = BindingEntry {
        provider_id: provider_id.to_string(),
        model: resolved_model,
        settings: settings.or(existing_settings),
        last_sync_at_ms: None,
    };
    apply_codex_auth_default(&mut entry, agent_id, external_cli);

    let binding = store.bindings.entry(agent_id.to_string()).or_default();
    if multi {
        match binding
            .entries
            .iter()
            .position(|e| e.provider_id == provider_id)
        {
            Some(pos) => {
                binding.entries[pos] = entry.clone();
                binding.active_index = pos;
            }
            None => {
                binding.entries.push(entry.clone());
                binding.active_index = binding.entries.len() - 1;
            }
        }
    } else {
        binding.entries = vec![entry.clone()];
        binding.active_index = 0;
    }
    Ok(entry)
}

/// Codex Official binds a ChatGPT OAuth session, so its entry must never carry
/// an auth mode that would write `OPENAI_API_KEY` over the user's token.
fn apply_codex_auth_default(entry: &mut BindingEntry, agent_id: &str, external_cli: bool) {
    if agent_id != "codex" || !external_cli {
        return;
    }
    let settings = entry
        .settings
        .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let Some(object) = settings.as_object_mut() {
        object.insert(
            "auth_mode".to_string(),
            serde_json::Value::String(crate::tool_sync::CODEX_AUTH_MODE_OAUTH.to_string()),
        );
    }
}

/// Move an agent's active pointer to an already-bound provider. Nothing else
/// changes — not the model, not the settings, not the order.
pub fn set_active_binding(
    store: &mut ProvidersStoreV4,
    agent_id: &str,
    provider_id: &str,
) -> Result<BindingEntry> {
    let binding = store
        .bindings
        .get_mut(agent_id)
        .filter(|b| !b.is_empty())
        .with_context(|| format!("Agent '{agent_id}' has no bindings"))?;
    let pos = binding
        .entries
        .iter()
        .position(|e| e.provider_id == provider_id)
        .with_context(|| format!("Provider '{provider_id}' is not bound to agent '{agent_id}'"))?;
    binding.active_index = pos;
    Ok(binding.entries[pos].clone())
}

/// Edit one bound entry's model and/or settings without moving the pointer.
///
/// `None` means "leave it alone", which is what lets a caller change the model
/// without having to read the settings back first and hand them in unchanged.
pub fn update_binding_entry(
    store: &mut ProvidersStoreV4,
    agent_id: &str,
    provider_id: &str,
    model: Option<&str>,
    settings: Option<serde_json::Value>,
) -> Result<BindingEntry> {
    let binding = store
        .bindings
        .get_mut(agent_id)
        .with_context(|| format!("Agent '{agent_id}' has no bindings"))?;
    let entry = binding
        .entries
        .iter_mut()
        .find(|e| e.provider_id == provider_id)
        .with_context(|| format!("Provider '{provider_id}' is not bound to agent '{agent_id}'"))?;

    if let Some(model) = model {
        entry.model = model.to_string();
    }
    if let Some(settings) = settings {
        entry.settings = if settings.is_null() {
            None
        } else {
            Some(settings)
        };
    }
    Ok(entry.clone())
}

/// Replace the active entry's per-provider settings bag.
///
/// Renamed from v3's `update_tool_settings`, whose only distinction from
/// `update_tool_binding_settings` was the word "binding" in the middle of the
/// name. Now one says *entry* and the other says *agent*.
pub fn update_binding_entry_settings(
    store: &mut ProvidersStoreV4,
    agent_id: &str,
    settings: serde_json::Value,
) -> Result<BindingEntry> {
    let binding = store
        .bindings
        .get_mut(agent_id)
        .filter(|b| !b.is_empty())
        .with_context(|| format!("Agent '{agent_id}' is not currently bound"))?;
    let active = binding
        .active_mut()
        .with_context(|| format!("Agent '{agent_id}' has no active entry"))?;
    active.settings = if settings.is_null() {
        None
    } else {
        Some(settings)
    };
    Ok(active.clone())
}

/// Replace the agent-level settings bag (everything that is not role routing).
pub fn update_agent_settings(
    store: &mut ProvidersStoreV4,
    agent_id: &str,
    settings: serde_json::Value,
) -> Result<AgentBinding> {
    let binding = store
        .bindings
        .get_mut(agent_id)
        .filter(|b| !b.is_empty())
        .with_context(|| format!("Agent '{agent_id}' is not currently bound"))?;
    binding.settings = if settings.is_null() {
        None
    } else {
        Some(settings)
    };
    Ok(binding.clone())
}

/// Replace an agent's role → model routing.
pub fn set_agent_roles(
    store: &mut ProvidersStoreV4,
    agent_id: &str,
    roles: std::collections::BTreeMap<String, super::binding::ModelRef>,
) -> Result<AgentBinding> {
    let binding = store
        .bindings
        .get_mut(agent_id)
        .filter(|b| !b.is_empty())
        .with_context(|| format!("Agent '{agent_id}' is not currently bound"))?;
    binding.roles = roles;
    Ok(binding.clone())
}

/// Remove **one** provider from an agent's binding.
///
/// The pointer re-clamps and the provider's role assignments go with it. This
/// is what every per-row unbind button should always have called.
pub fn unbind_provider(
    store: &mut ProvidersStoreV4,
    agent_id: &str,
    provider_id: &str,
) -> Result<Option<BindingEntry>> {
    let binding = store
        .bindings
        .get_mut(agent_id)
        .with_context(|| format!("Agent '{agent_id}' has no bindings"))?;
    if !binding.binds_provider(provider_id) {
        bail!("Provider '{provider_id}' is not bound to agent '{agent_id}'");
    }
    drop_entry(binding, provider_id);
    binding
        .roles
        .retain(|_, target| target.provider_id != provider_id);
    Ok(binding.active().cloned())
}

/// Clear an agent's binding entirely — every entry and every role.
///
/// The destructive sibling of [`unbind_provider`], and now reachable only by
/// asking for it.
pub fn unbind_agent(store: &mut ProvidersStoreV4, agent_id: &str) -> Result<Option<BindingEntry>> {
    let previous = store
        .bindings
        .insert(agent_id.to_string(), AgentBinding::default())
        .and_then(|b| b.active().cloned());
    Ok(previous)
}

/// Drop one entry and re-clamp the active pointer so it still points at
/// something real.
fn drop_entry(binding: &mut AgentBinding, provider_id: &str) {
    let Some(pos) = binding
        .entries
        .iter()
        .position(|e| e.provider_id == provider_id)
    else {
        return;
    };
    binding.entries.remove(pos);
    if binding.active_index >= pos && binding.active_index > 0 {
        binding.active_index -= 1;
    }
}

/// Whether an agent's config format natively holds several providers.
///
/// The answer lives in the agent registry; this is the store layer's single
/// consultation point, and the registry's consistency test pins the two
/// together. Unknown ids are single-provider.
pub fn agent_supports_multiple_providers(agent_id: &str) -> bool {
    crate::tool_sync::agent_spec(agent_id)
        .is_some_and(|spec| matches!(spec.kind, crate::tool_sync::AgentKind::Multi))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
