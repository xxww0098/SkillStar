//! Resolving a provider's credential inside the backend, by id.
//!
//! ## Why the diagnostics commands stopped taking a key
//!
//! `test_provider_connection`, `fetch_provider_models`, `query_provider_balance`
//! and `test_endpoints_latency` all used to take the plaintext key as a
//! parameter. That made the renderer the *source* of the credential for every
//! probe: it had to hold the key, keep it in a query cache, and hand it back
//! across the IPC boundary on each call. Every one of those is a place the key
//! can be observed, and none of them buys anything — the backend already has
//! the store the key came from.
//!
//! Taking `provider_id` instead inverts it: the renderer names a row, the
//! backend looks up what that row needs. The key never leaves the process that
//! owns it. This also removes a whole class of bug where the frontend passes a
//! key that has drifted from the one on disk.
//!
//! An empty resolved key is not an error here. Plenty of endpoints are
//! reachable without one (local services, native-login rows), and the probe
//! itself is what should report an authentication failure — refusing to probe
//! would replace a specific `401` with a generic complaint.

use super::provider::Provider;
use super::store::{flat_store_path, read_flat_store};
use super::types::ProviderEntryFlat;
use anyhow::{Context, Result, bail};
use std::path::Path;

/// The connection facts a diagnostics probe needs about one provider.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedConnection {
    pub provider_id: String,
    /// May be empty — see the module note.
    pub api_key: String,
    pub base_url_openai: String,
    pub base_url_anthropic: String,
    pub models_url: String,
    pub preset_id: Option<String>,
    /// Alternate URLs for the endpoint speed test, from the preset.
    pub endpoint_candidates: Vec<String>,
}

impl ResolvedConnection {
    fn from_v3(entry: &ProviderEntryFlat) -> Self {
        let endpoint_candidates = entry
            .preset_id
            .as_deref()
            .and_then(|pid| {
                super::presets::get_all_presets_flat()
                    .into_iter()
                    .find(|p| p.id == pid)
            })
            .map(|p| p.endpoint_candidates)
            .unwrap_or_default();
        Self {
            provider_id: entry.id.clone(),
            api_key: entry.api_key.clone(),
            base_url_openai: entry.base_url_openai.clone(),
            base_url_anthropic: entry.base_url_anthropic.clone(),
            models_url: entry.models_url.clone(),
            preset_id: entry.preset_id.clone(),
            endpoint_candidates,
        }
    }

    /// Project a v4 row. Used once the store is v4; kept beside the v3 reader
    /// so the two derivations of the same facts stay visibly parallel.
    pub fn from_v4(provider: &Provider) -> Self {
        Self {
            provider_id: provider.id.clone(),
            // Only literal keys resolve here. `EnvVar` / `File` / `Command`
            // deliberately yield an empty string: resolving those belongs to
            // the writer at sync time, where the machine's environment is the
            // right context, not to a probe.
            api_key: provider
                .credential
                .literal_secret()
                .unwrap_or_default()
                .to_string(),
            base_url_openai: provider
                .endpoints
                .openai_chat
                .clone()
                .unwrap_or_default(),
            base_url_anthropic: provider
                .endpoints
                .anthropic_messages
                .clone()
                .unwrap_or_default(),
            models_url: provider.endpoints.models_list.clone().unwrap_or_default(),
            preset_id: provider.preset_id.clone(),
            endpoint_candidates: Vec::new(),
        }
    }

    /// The URL a discovery probe should call, falling back to the OpenAI base
    /// plus `/models` the way the frontend used to.
    pub fn models_endpoint(&self) -> String {
        if !self.models_url.trim().is_empty() {
            return self.models_url.trim().to_string();
        }
        let base = self.base_url_openai.trim().trim_end_matches('/');
        if base.is_empty() {
            String::new()
        } else {
            format!("{base}/models")
        }
    }
}

/// Look up one provider's connection facts from the live store.
pub fn resolve_connection(provider_id: &str) -> Result<ResolvedConnection> {
    resolve_connection_at(&flat_store_path(), provider_id)
}

/// Same, against an explicit store path (what the tests use).
pub fn resolve_connection_at(path: &Path, provider_id: &str) -> Result<ResolvedConnection> {
    let store = read_flat_store(path)
        .with_context(|| format!("failed to read provider store at {}", path.display()))?;
    let Some(entry) = store.providers.iter().find(|p| p.id == provider_id) else {
        // Naming the id matters: the usual cause is a stale frontend cache
        // pointing at a row that was deleted, and "provider not found" without
        // the id sends the reader looking in the wrong place.
        bail!("provider '{provider_id}' is not in the store");
    };
    Ok(ResolvedConnection::from_v3(entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::credential::Credential;
    use crate::providers::types::FlatProvidersStore;

    fn store_file(dir: &Path, entries: Vec<ProviderEntryFlat>) -> std::path::PathBuf {
        let path = dir.join("model_providers.json");
        let store = FlatProvidersStore {
            version: 3,
            providers: entries,
            tool_activations: Default::default(),
        };
        std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).unwrap();
        path
    }

    fn entry(id: &str) -> ProviderEntryFlat {
        ProviderEntryFlat {
            id: id.to_string(),
            name: "Relay".to_string(),
            base_url_openai: "https://relay.example.com/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: String::new(),
            api_key: "sk-from-disk".to_string(),
            models: vec![],
            default_model: String::new(),
            sort_index: 0,
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
            codex_wire_api: "chat".to_string(),
            codex_auth_mode: "third_party".to_string(),
        }
    }

    #[test]
    fn the_key_comes_from_disk_not_from_the_caller() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = store_file(tmp.path(), vec![entry("p1")]);

        let resolved = resolve_connection_at(&path, "p1").unwrap();

        assert_eq!(
            resolved.api_key, "sk-from-disk",
            "the whole point of R-8: the renderer never supplies this"
        );
        assert_eq!(resolved.base_url_openai, "https://relay.example.com/v1");
    }

    #[test]
    fn an_unknown_provider_id_names_itself_in_the_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = store_file(tmp.path(), vec![entry("p1")]);

        let err = resolve_connection_at(&path, "ghost").unwrap_err().to_string();

        assert!(err.contains("ghost"), "{err}");
    }

    #[test]
    fn a_missing_models_url_falls_back_to_the_openai_base() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = store_file(tmp.path(), vec![entry("p1")]);

        let resolved = resolve_connection_at(&path, "p1").unwrap();

        assert_eq!(
            resolved.models_endpoint(),
            "https://relay.example.com/v1/models",
            "the frontend used to build this URL; the rule moved with the key"
        );
    }

    #[test]
    fn an_explicit_models_url_wins_over_the_fallback() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut e = entry("p1");
        e.models_url = "https://relay.example.com/custom/models".to_string();
        let path = store_file(tmp.path(), vec![e]);

        let resolved = resolve_connection_at(&path, "p1").unwrap();

        assert_eq!(
            resolved.models_endpoint(),
            "https://relay.example.com/custom/models"
        );
    }

    #[test]
    fn a_v4_row_resolves_only_literal_keys() {
        let mut provider = Provider::new("p1", "Relay");
        provider.credential = Credential::single_key("k1", "sk-literal");
        assert_eq!(ResolvedConnection::from_v4(&provider).api_key, "sk-literal");

        // A pointer is not a key. Expanding `$NAME` here would bake this
        // machine's environment into a probe that may run elsewhere.
        provider.credential = Credential::EnvVar {
            name: "OPENAI_API_KEY".to_string(),
        };
        assert_eq!(ResolvedConnection::from_v4(&provider).api_key, "");
    }

    #[test]
    fn an_empty_key_resolves_rather_than_failing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut e = entry("local");
        e.api_key = String::new();
        let path = store_file(tmp.path(), vec![e]);

        let resolved = resolve_connection_at(&path, "local").unwrap();

        assert_eq!(
            resolved.api_key, "",
            "local services need no key; the probe reports auth failures, not this"
        );
    }
}
