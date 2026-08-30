//! Frontend projections of the v4 provider types.
//!
//! ## Why a projection rather than `#[derive(TS)]` on `Provider`
//!
//! One reason, and it is not stylistic: [`Provider`] holds the plaintext API
//! key. A type that derives `TS` is a type whose serialized form crosses into
//! the renderer, and the renderer is where a key can leak into a devtools
//! panel, a React state dump, a query cache persisted to disk, or an error
//! report. The projection makes it impossible to send the secret by accident,
//! because the DTO has nowhere to put it.
//!
//! What the frontend gets instead is enough to render and edit the connection
//! without ever holding the value: which *kind* of credential it is, a masked
//! or symbolic summary of it, and whether one is set at all. Changing a key is
//! a write, not a read-modify-write, so nothing is lost.
//!
//! Per D-034 the `From` impls destructure completely rather than using `..`:
//! adding a field upstream then fails to compile here, which forces a decision
//! about whether it belongs in the frontend contract instead of letting it
//! silently not arrive.

use serde::{Deserialize, Serialize};
use skillstar_models::providers::{
    AgentBinding, BindingEntry, CapSource, Credential, CredentialKind, Endpoints, ModelRef,
    Provider, ProviderCaps, ProvidersStoreV4,
};
use std::collections::{BTreeMap, HashMap};
use ts_rs::TS;

/// A provider as the renderer sees it: everything except the secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "ProviderDto.ts")]
pub struct ProviderDto {
    pub id: String,
    pub name: String,
    pub preset_id: Option<String>,
    pub endpoints: Endpoints,
    /// Which credential mechanism is configured.
    pub credential_kind: CredentialKind,
    /// Masked (`sk-a••••wxyz`) for literal keys, or the pointer itself
    /// (`$OPENAI_API_KEY`, a path, a command) for the indirect variants. Safe
    /// to render; never the secret.
    pub credential_summary: String,
    /// Whether any secret material is configured at all. The UI needs this to
    /// tell "no key yet" from "key set, value hidden" — a masked empty string
    /// looks like both.
    pub has_secret: bool,
    pub headers: BTreeMap<String, String>,
    pub caps: ProviderCaps,
    pub cap_source: CapSource,
    pub models: Vec<String>,
    pub default_model: Option<String>,
    #[ts(type = "number")]
    pub sort_index: u32,
    pub icon_color: Option<String>,
    pub notes: Option<String>,
    #[ts(type = "number | null")]
    pub created_at_ms: Option<u64>,
    /// True when this row's credentials live in another CLI's store — the
    /// "Official / native login" rows. The UI hides the key field entirely
    /// rather than showing an empty one.
    pub is_external_cli: bool,
}

impl From<Provider> for ProviderDto {
    fn from(provider: Provider) -> Self {
        // Complete destructuring, per D-034: a new field on `Provider` breaks
        // this line until someone decides where it goes.
        let Provider {
            id,
            name,
            preset_id,
            endpoints,
            credential,
            headers,
            caps,
            models,
            default_model,
            sort_index,
            icon_color,
            notes,
            created_at_ms,
            // Deliberately not projected: `ext` is the unschema'd pocket for
            // values the backend itself does not interpret. Forwarding it would
            // put arbitrary, possibly sensitive JSON into the renderer for no
            // stated consumer.
            ext: _,
        } = provider;

        let cap_source = caps.source;
        Self {
            id,
            name,
            preset_id,
            endpoints,
            credential_kind: credential.kind(),
            credential_summary: credential.summary(),
            has_secret: credential.has_secret(),
            is_external_cli: matches!(credential, Credential::ExternalCli { .. }),
            headers,
            caps,
            cap_source,
            models,
            default_model,
            sort_index,
            icon_color,
            notes,
            created_at_ms,
        }
    }
}

/// An agent binding as the renderer sees it. No secrets here, so this is a
/// straight shape-for-shape copy; it exists so the frontend has one generated
/// type instead of reaching for the domain type's serde form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "AgentBindingDto.ts")]
pub struct AgentBindingDto {
    pub entries: Vec<BindingEntryDto>,
    #[ts(type = "number")]
    pub active_index: u32,
    pub roles: BTreeMap<String, ModelRef>,
    #[ts(type = "unknown | null")]
    pub settings: Option<serde_json::Value>,
}

impl From<AgentBinding> for AgentBindingDto {
    fn from(binding: AgentBinding) -> Self {
        let AgentBinding {
            entries,
            active_index,
            roles,
            settings,
        } = binding;
        Self {
            entries: entries.into_iter().map(BindingEntryDto::from).collect(),
            // Narrowed to u32 because ts-rs maps usize to bigint while Tauri's
            // IPC hands JavaScript a number; an index is never large enough for
            // the narrowing to matter.
            active_index: active_index.min(u32::MAX as usize) as u32,
            roles,
            settings,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "BindingEntryDto.ts")]
pub struct BindingEntryDto {
    pub provider_id: String,
    pub model: String,
    #[ts(type = "unknown | null")]
    pub settings: Option<serde_json::Value>,
    #[ts(type = "number | null")]
    pub last_sync_at_ms: Option<u64>,
}

impl From<BindingEntry> for BindingEntryDto {
    fn from(entry: BindingEntry) -> Self {
        let BindingEntry {
            provider_id,
            model,
            settings,
            last_sync_at_ms,
        } = entry;
        Self {
            provider_id,
            model,
            settings,
            last_sync_at_ms,
        }
    }
}

/// The whole store, projected. This is what the providers list command returns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "ProvidersStoreDto.ts")]
pub struct ProvidersStoreDto {
    #[ts(type = "number")]
    pub version: u32,
    pub providers: Vec<ProviderDto>,
    pub bindings: HashMap<String, AgentBindingDto>,
}

impl From<ProvidersStoreV4> for ProvidersStoreDto {
    fn from(store: ProvidersStoreV4) -> Self {
        let ProvidersStoreV4 {
            version,
            providers,
            bindings,
        } = store;
        Self {
            version,
            providers: providers.into_iter().map(ProviderDto::from).collect(),
            bindings: bindings
                .into_iter()
                .map(|(k, v)| (k, AgentBindingDto::from(v)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_models::providers::{ApiKeyEntry, NoCredentialReason};

    fn provider_with(credential: Credential) -> Provider {
        let mut p = Provider::new("p1", "Relay");
        p.endpoints = Endpoints {
            openai_chat: Some("https://relay.example.com/v1".to_string()),
            ..Endpoints::default()
        };
        p.credential = credential;
        p
    }

    /// The whole reason this module exists.
    #[test]
    fn provider_dto_never_contains_plaintext_secret() {
        const SECRET: &str = "sk-super-secret-value-9876";

        let cases = vec![
            Credential::single_key("k1", SECRET),
            Credential::ApiKey {
                keys: vec![
                    ApiKeyEntry {
                        id: "k1".to_string(),
                        secret: SECRET.to_string(),
                        label: Some("primary".to_string()),
                        enabled: true,
                    },
                    ApiKeyEntry {
                        id: "k2".to_string(),
                        secret: format!("{SECRET}-fallback"),
                        label: None,
                        enabled: false,
                    },
                ],
            },
        ];

        for credential in cases {
            let dto = ProviderDto::from(provider_with(credential));
            let json = serde_json::to_string(&dto).expect("serialize");
            assert!(
                !json.contains(SECRET),
                "plaintext key reached the DTO: {json}"
            );
            assert!(dto.has_secret, "but the UI must still know one is set");
            assert_eq!(dto.credential_kind, CredentialKind::ApiKey);
        }
    }

    #[test]
    fn the_summary_masks_a_key_without_hiding_which_key_it_is() {
        let dto = ProviderDto::from(provider_with(Credential::single_key(
            "k1",
            "sk-abcdefghijklmnop",
        )));
        assert_eq!(dto.credential_summary, "sk-a••••mnop");

        // A short key is replaced wholesale — showing four of six characters
        // is not masking.
        let short = ProviderDto::from(provider_with(Credential::single_key("k1", "abc123")));
        assert_eq!(short.credential_summary, "••••••••");
        assert!(!short.credential_summary.contains("abc"));
    }

    #[test]
    fn indirect_credentials_summarize_as_the_pointer_not_a_mask() {
        // A variable name is not a secret, and hiding it would leave the user
        // unable to see where their key is being read from.
        let env = ProviderDto::from(provider_with(Credential::EnvVar {
            name: "OPENAI_API_KEY".to_string(),
        }));
        assert_eq!(env.credential_summary, "$OPENAI_API_KEY");
        assert_eq!(env.credential_kind, CredentialKind::EnvVar);
        assert!(env.has_secret);
    }

    #[test]
    fn an_external_cli_row_reports_itself_and_holds_no_secret() {
        let dto = ProviderDto::from(provider_with(Credential::ExternalCli {
            surface: "claude".to_string(),
        }));
        assert!(dto.is_external_cli);
        assert!(!dto.has_secret);
        assert_eq!(dto.credential_kind, CredentialKind::ExternalCli);
    }

    #[test]
    fn a_missing_credential_carries_its_reason_through_the_kind() {
        let local = ProviderDto::from(provider_with(Credential::None {
            reason: NoCredentialReason::LocalService,
        }));
        assert_eq!(local.credential_kind, CredentialKind::None);
        assert!(!local.has_secret);
        assert_eq!(local.credential_summary, "local service");
    }

    #[test]
    fn store_projection_strips_secrets_across_every_row() {
        const SECRET: &str = "sk-store-wide-secret-1234";
        let store = ProvidersStoreV4 {
            version: 4,
            providers: vec![
                provider_with(Credential::single_key("k1", SECRET)),
                provider_with(Credential::EnvVar {
                    name: "X".to_string(),
                }),
            ],
            bindings: HashMap::new(),
        };
        let json = serde_json::to_string(&ProvidersStoreDto::from(store)).unwrap();
        assert!(!json.contains(SECRET), "{json}");
    }
}
