//! The Models page's three columns.
//!
//! A row is an id and a display name. Endpoints, credentials, and the gateway
//! listen address stay out of this DTO, so the page cannot show a vendor key
//! or a vendor URL.

use serde::{Deserialize, Serialize};
use skillstar_models::providers::{Provider, StoreError, load_store};
use skillstar_models::tool_sync::agent_specs;
use ts_rs::TS;

/// One Agents, Providers, or Gateway row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "ModelsBoardRowDto.ts")]
pub struct ModelsBoardRowDto {
    pub id: String,
    pub name: String,
}

/// The three columns, left to right: Agents, Providers, Gateway.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "ModelsBoardDto.ts")]
pub struct ModelsBoardDto {
    pub agents: Vec<ModelsBoardRowDto>,
    pub providers: Vec<ModelsBoardRowDto>,
    /// Empty until recent calls have a home. This loader does not read the
    /// listen address.
    pub gateway: Vec<ModelsBoardRowDto>,
}

/// Agents from the registry, provider names from the store, gateway empty.
///
/// Uses [`load_store`], not the repair path: a missing store is an empty list
/// and writes nothing. A v4 file is read as it is.
pub fn load_models_board() -> Result<ModelsBoardDto, StoreError> {
    let loaded = load_store()?;
    Ok(board_from_providers(&loaded.store.providers))
}

fn board_from_providers(providers: &[Provider]) -> ModelsBoardDto {
    ModelsBoardDto {
        agents: agent_specs()
            .iter()
            .map(|spec| ModelsBoardRowDto {
                id: spec.id.to_string(),
                name: spec.display_name.to_string(),
            })
            .collect(),
        providers: providers
            .iter()
            .map(|provider| ModelsBoardRowDto {
                id: provider.id.clone(),
                name: provider.name.clone(),
            })
            .collect(),
        gateway: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_models::providers::Credential;

    #[test]
    fn board_dto_omits_secret_and_vendor_host() {
        let mut provider = Provider::new("p1", "DeepSeek");
        provider.credential = Credential::single_key("k", "sk-secret-value");
        provider.endpoints.openai_chat = Some("https://vendor.example/v1".to_string());

        let board = board_from_providers(std::slice::from_ref(&provider));
        let value = serde_json::to_value(&board).unwrap();
        let row = &value["providers"][0];
        assert_eq!(row["id"], "p1");
        assert_eq!(row["name"], "DeepSeek");
        assert!(row.get("credential").is_none());
        assert!(row.get("endpoints").is_none());
        assert!(value["gateway"].as_array().unwrap().is_empty());

        let text = value.to_string();
        assert!(!text.contains("sk-secret-value"));
        assert!(!text.contains("vendor.example"));
        assert!(!text.contains("https://"));
    }

    #[tokio::test]
    async fn board_load_does_not_touch_agent_files() {
        let _lock = ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let data = temp.path().join("data");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        let config = home.join(".codex").join("config.toml");
        let sentinel = b"model = \"user-owned\"\n";
        std::fs::write(&config, sentinel).unwrap();
        let _env = EnvGuard::set(&[
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("SKILLSTAR_DATA_DIR", &data),
            ("SKILLSTAR_TOOL_SYNC_HOME", &home),
        ]);

        let board = load_models_board().unwrap();
        assert!(board.gateway.is_empty());
        assert!(board.providers.is_empty());
        assert!(!board.agents.is_empty());
        assert_eq!(std::fs::read(&config).unwrap(), sentinel);
        let mut tops: Vec<_> = std::fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        tops.sort();
        assert_eq!(tops, vec![std::ffi::OsString::from(".codex")]);
        assert!(!data.join("config").join("model_providers.json").exists());
    }
}
