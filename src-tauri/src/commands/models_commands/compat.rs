//! The v4 ↔ v3 wire shim the renderer still talks through.
//!
//! ## Why this exists, and when it goes away
//!
//! WP-2A moves the on-disk store and every writer to v4. It deliberately does
//! **not** rewrite the Models UI — that is WP-4, and doing both at once would
//! mean a change nobody can review in which a store bug and a rendering bug
//! look the same. So the store is v4 and the IPC surface is still the v3 shape,
//! and this module is the one place that translates.
//!
//! ## Why the projection is not just a `From` impl
//!
//! Two of the mappings lose information, and both losses are one-directional:
//!
//! - **Endpoints.** v4 distinguishes "this host has no Anthropic endpoint"
//!   (`None`) from "the field is blank" (`Some("")`); v3 spells both as `""`.
//!   Projecting down is total; projecting back up is not, which is why the
//!   write path *patches* an existing v4 row rather than rebuilding one from
//!   the v3 shape. Rebuilding would silently reset `caps`, `headers`, the
//!   failover key chain, and `ext` on every save.
//! - **Credentials.** v4's [`Credential`] union can hold an env var name, a
//!   file path or a command; v3 has one plaintext string. Only the literal
//!   variant survives the trip down, and a save that carries a key back up
//!   replaces just that variant.
//!
//! ## The plaintext key
//!
//! `ProviderEntryFlat.api_key` still crosses to the renderer here. That is not
//! an endorsement — WP-1 built [`ProviderDto`] precisely so it would not have
//! to, and B3 in the redesign's breaking-change list is the UI work that lets
//! the key stop travelling. Cutting it here without that UI would leave the
//! provider editor unable to show or change a key at all, which is a
//! regression, not a fix. It goes when WP-4 lands the editor that replaces it.
//!
//! [`ProviderDto`]: skillstar_models::providers::Provider
//! [`Credential`]: skillstar_models::providers::Credential

use skillstar_models::providers::{
    AgentBinding, Credential, Endpoints, ModelRef, Provider, ProviderEntryFlat, ProviderPatchFlat,
    ProvidersStoreV4, ToolActivation, ToolBinding, derive_responses_endpoint,
};

/// Project one v4 row into the v3 shape the renderer reads.
pub fn provider_to_flat(provider: &Provider) -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: provider.id.clone(),
        name: provider.name.clone(),
        base_url_openai: provider.endpoints.openai_chat.clone().unwrap_or_default(),
        base_url_anthropic: provider
            .endpoints
            .anthropic_messages
            .clone()
            .unwrap_or_default(),
        models_url: provider.endpoints.models_list.clone().unwrap_or_default(),
        api_key: provider
            .credential
            .literal_secret()
            .unwrap_or_default()
            .to_string(),
        models: provider.models.clone(),
        default_model: provider.default_model.clone().unwrap_or_default(),
        sort_index: provider.sort_index,
        preset_id: provider.preset_id.clone(),
        icon_color: provider.icon_color.clone(),
        notes: provider.notes.clone(),
        created_at: provider.created_at_ms,
        meta: provider.ext.clone(),
        // Reported, never chosen. The renderer still renders a Codex column and
        // needs to know what will be written; it no longer gets to pick.
        codex_wire_api: skillstar_models::tool_sync::CODEX_WIRE_API.to_string(),
        codex_auth_mode: String::new(),
    }
}

/// Project one v4 binding into the v3 shape.
///
/// `roles` are folded back into the settings bag under the key v3 used, so the
/// OMP role panel and the Claude mapping panel keep reading what they always
/// read. The values gain nothing and lose nothing: v3's `thinking` string is
/// v4's `effort`, spelled the way OMP spells it.
pub fn binding_to_flat(binding: &AgentBinding) -> ToolBinding {
    let mut settings = binding.settings.clone();
    if !binding.roles.is_empty() {
        let roles: serde_json::Map<String, serde_json::Value> = binding
            .roles
            .iter()
            .map(|(role, target)| (role.clone(), role_to_flat(target)))
            .collect();
        let bag = settings.get_or_insert_with(|| serde_json::Value::Object(Default::default()));
        if let Some(object) = bag.as_object_mut() {
            object.insert("roles".to_string(), serde_json::Value::Object(roles));
        }
    }
    ToolBinding {
        entries: binding
            .entries
            .iter()
            .map(|entry| ToolActivation {
                provider_id: entry.provider_id.clone(),
                model: entry.model.clone(),
                settings: entry.settings.clone(),
                // Back to seconds: the renderer's conflict banner compares this
                // against a file mtime, which it reads in seconds.
                last_sync_at: entry.last_sync_at_ms.map(|ms| ms / 1000),
            })
            .collect(),
        active_index: binding.active_index,
        settings,
    }
}

fn role_to_flat(target: &ModelRef) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert(
        "provider_id".to_string(),
        serde_json::Value::String(target.provider_id.clone()),
    );
    object.insert(
        "model".to_string(),
        serde_json::Value::String(target.model.clone()),
    );
    if let Some(effort) = target.effort {
        object.insert(
            "thinking".to_string(),
            serde_json::Value::String(effort.as_omp_thinking().to_string()),
        );
    }
    serde_json::Value::Object(object)
}

/// Read a v3 `roles` bag back into the typed map.
pub fn roles_from_flat(
    settings: &serde_json::Value,
) -> std::collections::BTreeMap<String, ModelRef> {
    let mut roles = std::collections::BTreeMap::new();
    let Some(map) = settings.get("roles").and_then(|v| v.as_object()) else {
        return roles;
    };
    for (role, target) in map {
        roles.insert(
            role.clone(),
            ModelRef {
                provider_id: target
                    .get("provider_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                model: target
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                effort: target
                    .get("thinking")
                    .and_then(|v| v.as_str())
                    .and_then(skillstar_models::providers::Effort::from_omp_thinking),
                ext: None,
            },
        );
    }
    roles
}

/// Strip the roles key back out of a v3 settings bag, returning what is left.
///
/// An object whose only key was `roles` becomes `None` rather than `{}` — an
/// empty bag and no bag mean the same thing, and writing one of them makes the
/// store file noisier for no reader's benefit.
pub fn settings_without_roles(settings: &serde_json::Value) -> Option<serde_json::Value> {
    let mut value = settings.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("roles");
        if object.is_empty() {
            return None;
        }
    }
    if value.is_null() { None } else { Some(value) }
}

/// Build a fresh v4 row from the v3 creation payload.
///
/// Creation is the one direction where rebuilding is correct: there is no
/// existing row whose `caps` / `headers` / `ext` could be clobbered.
pub fn provider_from_flat_new(entry: &ProviderEntryFlat) -> Provider {
    let mut provider = Provider::new(
        if entry.id.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            entry.id.clone()
        },
        entry.name.clone(),
    );
    provider.preset_id = entry.preset_id.clone();
    provider.endpoints = Endpoints {
        openai_chat: non_empty(&entry.base_url_openai),
        openai_responses: derive_responses_endpoint(&entry.base_url_openai),
        anthropic_messages: non_empty(&entry.base_url_anthropic),
        models_list: non_empty(&entry.models_url),
    };
    provider.credential = credential_from_key(&entry.api_key);
    if provider.endpoints.openai_responses.is_some() {
        provider.caps.responses_api = skillstar_models::providers::Tri::Yes;
    }
    provider.models = entry.models.clone();
    provider.default_model = non_empty(&entry.default_model);
    provider.icon_color = entry.icon_color.clone();
    provider.notes = entry.notes.clone();
    provider.created_at_ms = entry.created_at;
    provider.ext = entry.meta.clone();
    provider
}

/// Apply a v3 patch onto an existing v4 row.
///
/// Only the fields the patch names are touched. Everything v3 cannot express —
/// capability bits, custom headers, the key failover chain beyond its head, the
/// `ext` pocket — survives untouched, which is the whole reason this is a patch
/// and not a rebuild.
pub fn apply_flat_patch(provider: &mut Provider, patch: &ProviderPatchFlat) {
    if let Some(name) = &patch.name {
        provider.name = name.clone();
    }
    if let Some(url) = &patch.base_url_openai {
        provider.endpoints.openai_chat = non_empty(url);
        // The Responses endpoint tracks the chat one for the single host we can
        // infer it for. A probe (WP-2B) is what sets it for anyone else; until
        // then leaving a stale value would re-enable a Codex binding the user's
        // edit just invalidated.
        provider.endpoints.openai_responses = derive_responses_endpoint(url);
        provider.caps.responses_api = if provider.endpoints.openai_responses.is_some() {
            skillstar_models::providers::Tri::Yes
        } else {
            skillstar_models::providers::Tri::Unknown
        };
    }
    if let Some(url) = &patch.base_url_anthropic {
        provider.endpoints.anthropic_messages = non_empty(url);
    }
    if let Some(url) = &patch.models_url {
        provider.endpoints.models_list = non_empty(url);
    }
    if let Some(api_key) = &patch.api_key {
        provider.credential = replace_literal_key(&provider.credential, api_key);
    }
    if let Some(models) = &patch.models {
        provider.models = models.clone();
    }
    if let Some(default_model) = &patch.default_model {
        provider.default_model = non_empty(default_model);
    }
    if let Some(sort_index) = patch.sort_index {
        provider.sort_index = sort_index;
    }
    if let Some(preset_id) = &patch.preset_id {
        provider.preset_id = Some(preset_id.clone());
    }
    if let Some(icon_color) = &patch.icon_color {
        provider.icon_color = Some(icon_color.clone());
    }
    if let Some(notes) = &patch.notes {
        provider.notes = Some(notes.clone());
    }
    if let Some(meta) = &patch.meta {
        provider.ext = Some(meta.clone());
    }
    // `codex_wire_api` and `codex_auth_mode` are deliberately ignored: the
    // first no longer names a choice that exists, and the second moved to the
    // binding entry where it applies to the one agent that has the concept.
}

/// Replace only the literal key, keeping the rest of the credential's shape.
fn replace_literal_key(current: &Credential, api_key: &str) -> Credential {
    if api_key.trim().is_empty() {
        // Clearing the key is not the same as declaring a native login, so an
        // external-CLI row keeps its variant rather than being downgraded.
        return match current {
            Credential::ExternalCli { .. } => current.clone(),
            _ => Credential::unknown_local(),
        };
    }
    match current {
        Credential::ApiKey { keys } if !keys.is_empty() => {
            let mut keys = keys.clone();
            // The head of the failover chain is what v3 could see and therefore
            // what it can be editing; the fallbacks are none of its business.
            keys[0].secret = api_key.to_string();
            keys[0].enabled = true;
            Credential::ApiKey { keys }
        }
        _ => Credential::single_key(uuid::Uuid::new_v4().to_string(), api_key),
    }
}

fn credential_from_key(api_key: &str) -> Credential {
    if api_key.trim().is_empty() {
        Credential::unknown_local()
    } else {
        Credential::single_key(uuid::Uuid::new_v4().to_string(), api_key)
    }
}

fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Project the whole store.
pub fn store_to_flat(store: &ProvidersStoreV4) -> super::FlatProvidersResponse {
    super::FlatProvidersResponse {
        version: store.version,
        providers: store.providers.iter().map(provider_to_flat).collect(),
        tool_activations: store
            .bindings
            .iter()
            .map(|(agent_id, binding)| (agent_id.clone(), binding_to_flat(binding)))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_models::providers::{ApiKeyEntry, BindingEntry, Tri};

    fn relay() -> Provider {
        let mut p = Provider::new("p1", "Relay");
        p.endpoints.openai_chat = Some("https://relay.example.com/v1".to_string());
        p.credential = Credential::single_key("k1", "sk-primary");
        p
    }

    #[test]
    fn a_patch_preserves_everything_v3_cannot_express() {
        let mut provider = relay();
        provider
            .headers
            .insert("X-Team".to_string(), "platform".to_string());
        provider.caps.anthropic_messages = Tri::Yes;
        provider.credential = Credential::ApiKey {
            keys: vec![
                ApiKeyEntry {
                    id: "k1".into(),
                    secret: "sk-primary".into(),
                    label: None,
                    enabled: true,
                },
                ApiKeyEntry {
                    id: "k2".into(),
                    secret: "sk-fallback".into(),
                    label: Some("backup".into()),
                    enabled: true,
                },
            ],
        };

        apply_flat_patch(
            &mut provider,
            &ProviderPatchFlat {
                api_key: Some("sk-rotated".to_string()),
                name: Some("Relay (prod)".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(provider.name, "Relay (prod)");
        assert_eq!(provider.headers.get("X-Team").unwrap(), "platform");
        assert_eq!(
            provider.caps.anthropic_messages,
            Tri::Yes,
            "a probe result must survive an unrelated edit"
        );
        let Credential::ApiKey { keys } = &provider.credential else {
            panic!("credential shape changed");
        };
        assert_eq!(keys[0].secret, "sk-rotated");
        assert_eq!(
            keys[1].secret, "sk-fallback",
            "the failover chain is invisible to v3 and must not be truncated by it"
        );
    }

    #[test]
    fn editing_the_openai_url_away_from_openai_withdraws_the_responses_endpoint() {
        let mut provider = Provider::new("p1", "OpenAI");
        provider.endpoints.openai_chat = Some("https://api.openai.com/v1".to_string());
        provider.endpoints.openai_responses = Some("https://api.openai.com/v1".to_string());
        provider.caps.responses_api = Tri::Yes;

        apply_flat_patch(
            &mut provider,
            &ProviderPatchFlat {
                base_url_openai: Some("https://relay.example.com/v1".to_string()),
                ..Default::default()
            },
        );

        assert!(
            provider.endpoints.openai_responses.is_none(),
            "pointing the row at a relay must not leave it claiming Responses support"
        );
        assert_eq!(provider.caps.responses_api, Tri::Unknown);
    }

    #[test]
    fn roles_round_trip_through_the_v3_settings_bag() {
        use skillstar_models::providers::Effort;
        let mut binding = AgentBinding::default();
        binding.entries.push(BindingEntry::new("p1", "model-a"));
        binding.roles.insert(
            "fast".to_string(),
            ModelRef {
                provider_id: "p1".into(),
                model: "model-b".into(),
                effort: Some(Effort::Xhigh),
                ext: None,
            },
        );
        binding.settings = Some(serde_json::json!({ "other": 1 }));

        let flat = binding_to_flat(&binding);
        let bag = flat.settings.expect("settings bag");

        assert_eq!(bag.get("other").unwrap(), 1);
        assert_eq!(roles_from_flat(&bag), binding.roles);
        assert_eq!(
            settings_without_roles(&bag),
            Some(serde_json::json!({ "other": 1 }))
        );
    }

    /// The middle link of the chain 00 §1.3 found broken.
    ///
    /// This is the literal payload the Claude mapping panel sends — canonical
    /// role ids, one entry per tier — and what it has to become is a role map
    /// the Claude writer recognises. The renderer's end is
    /// `ClaudeMappingPanel.persist.test.tsx`; the disk end is
    /// `tool_sync::tests::roles::claude_role_mapping_lands_in_the_env_block`.
    /// Between them the tier models now reach `~/.claude/settings.json`, which
    /// they had not done in three schema versions.
    #[test]
    fn the_claude_panels_payload_becomes_roles_the_writer_recognises() {
        let payload = serde_json::json!({
            "roles": {
                "default": { "provider_id": "p1", "model": "big-model" },
                "fast":    { "provider_id": "p1", "model": "haiku-fast" },
                "sonnet":  { "provider_id": "p1", "model": "sonnet-mid" },
                "opus":    { "provider_id": "p1", "model": "opus-deep" },
                "subagent":{ "provider_id": "p1", "model": "subagent-model" },
            }
        });

        let roles = roles_from_flat(&payload);
        assert_eq!(roles.len(), 5);

        // Every key must be one Claude Code declares, or the writer would drop
        // it — the panel offering a row the backend has no env key for is the
        // silent-discard defect in a different costume.
        let declared: Vec<&str> = skillstar_models::tool_sync::agent_spec("claude-code")
            .unwrap()
            .roles
            .iter()
            .map(|def| def.id)
            .collect();
        for role in roles.keys() {
            assert!(
                declared.contains(&role.as_str()),
                "the panel sends `{role}`, which claude-code does not declare"
            );
        }
        assert_eq!(roles["fast"].model, "haiku-fast");
        assert_eq!(roles["subagent"].provider_id, "p1");

        // And the bag holds nothing else, so no stray agent settings are
        // invented by a panel that only edits roles.
        assert_eq!(settings_without_roles(&payload), None);
    }

    #[test]
    fn a_bag_holding_only_roles_projects_back_to_no_bag_at_all() {
        let bag = serde_json::json!({ "roles": { "fast": { "provider_id": "p1", "model": "m" } } });
        assert_eq!(settings_without_roles(&bag), None);
    }
}
