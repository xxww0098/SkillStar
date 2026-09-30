//! The Models page's three columns.
//!
//! A row is an id and a display name. Provider rows also carry
//! [`Credential::summary`](skillstar_models::providers::Credential::summary):
//! a masked key or a pointer, never the secret. An agent row may carry the
//! loopback host:port already written into that agent's file. Vendor
//! endpoints stay out of this DTO.

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
    /// Masked credential line for a provider. Empty on Agents and Gateway.
    pub credential_summary: String,
    /// Loopback host:port already written for this agent, such as `127.0.0.1:21847`.
    /// Empty when nothing loopback has been written, and on Providers and Gateway.
    pub loopback_label: String,
}

/// The three columns, left to right: Agents, Providers, Gateway.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "ModelsBoardDto.ts")]
pub struct ModelsBoardDto {
    pub agents: Vec<ModelsBoardRowDto>,
    pub providers: Vec<ModelsBoardRowDto>,
    /// Recent calls are a separate query. This list stays empty, and this
    /// loader does not read the listen address.
    pub gateway: Vec<ModelsBoardRowDto>,
}

/// Agents from the registry, provider names from the store, gateway empty.
///
/// Uses [`load_store`], not the repair path: a missing store is an empty list
/// and writes nothing. A v4 file is read as it is.
pub fn load_models_board() -> Result<ModelsBoardDto, StoreError> {
    let loaded = load_store()?;
    let mut board = board_from_providers(&loaded.store.providers);
    fill_loopback_labels(&mut board);
    Ok(board)
}

fn fill_loopback_labels(board: &mut ModelsBoardDto) {
    for agent in &mut board.agents {
        agent.loopback_label = skillstar_gateway::written_loopback_label(&agent.id);
    }
}

fn board_from_providers(providers: &[Provider]) -> ModelsBoardDto {
    ModelsBoardDto {
        agents: agent_specs()
            .iter()
            .map(|spec| ModelsBoardRowDto {
                id: spec.id.to_string(),
                name: spec.display_name.to_string(),
                credential_summary: String::new(),
                loopback_label: String::new(),
            })
            .collect(),
        providers: providers
            .iter()
            .map(|provider| ModelsBoardRowDto {
                id: provider.id.clone(),
                name: provider.name.clone(),
                credential_summary: provider.credential.summary(),
                loopback_label: String::new(),
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
        assert_eq!(row["credential_summary"], "sk-s••••alue");
        assert!(row.get("credential").is_none());
        assert!(row.get("endpoints").is_none());
        assert!(row.get("api_key").is_none());
        assert_eq!(value["agents"][0]["credential_summary"], "");
        assert_eq!(value["agents"][0]["loopback_label"], "");
        assert_eq!(value["providers"][0]["loopback_label"], "");
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
        assert!(board.agents.iter().all(|agent| agent.loopback_label.is_empty()));
        assert_eq!(std::fs::read(&config).unwrap(), sentinel);
        let mut tops: Vec<_> = std::fs::read_dir(&home)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        tops.sort();
        assert_eq!(tops, vec![std::ffi::OsString::from(".codex")]);
        assert!(!data.join("config").join("model_providers.json").exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_row_shows_the_written_loopback_not_the_vendor_endpoint() {
        let _lock = ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let data = temp.path().join("data");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let addr = std::path::PathBuf::from("127.0.0.1:21847");
        let _env = EnvGuard::set(&[
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("SKILLSTAR_DATA_DIR", &data),
            ("SKILLSTAR_TOOL_SYNC_HOME", &home),
            ("SKILLSTAR_GATEWAY_ADDR", &addr),
        ]);

        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex").join("config.toml"),
            "openai_base_url = \"https://api.openai.com/v1\"\n",
        )
        .unwrap();
        crate::models::save_agent("opencode", "group/fast").unwrap();

        let mut provider = Provider::new("p1", "DeepSeek");
        provider.credential = Credential::single_key("k", "sk-secret-value");
        provider.endpoints.openai_chat = Some("https://api.openai.com/v1".to_string());
        let mut board = board_from_providers(std::slice::from_ref(&provider));
        fill_loopback_labels(&mut board);

        let opencode = board.agents.iter().find(|agent| agent.id == "opencode").unwrap();
        assert_eq!(opencode.loopback_label, "127.0.0.1:21847");
        let codex = board.agents.iter().find(|agent| agent.id == "codex").unwrap();
        assert_eq!(codex.loopback_label, "");
        let text = serde_json::to_string(&board).unwrap();
        assert!(text.contains("127.0.0.1:21847"), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");
        assert!(!text.contains("https://"), "{text}");
        assert!(!text.contains("sk-secret-value"), "{text}");

        crate::models::save_codex(
            crate::models::CodexRoute::Api,
            "http://127.0.0.1:21847",
        )
        .unwrap();
        let mut board = board_from_providers(&[]);
        fill_loopback_labels(&mut board);
        let codex = board.agents.iter().find(|agent| agent.id == "codex").unwrap();
        assert_eq!(codex.loopback_label, "127.0.0.1:21847");
        let file = std::fs::read_to_string(home.join(".codex").join("config.toml")).unwrap();
        assert!(file.contains("http://127.0.0.1:21847"), "{file}");
    }
}
