//! App entry for pointing a file-based agent at the local gateway.
//!
//! The home directory is resolved inside the gateway. This module does not
//! read Usage or the provider store.

use skillstar_gateway::ApplyError;

/// Write `agent_id`'s loopback files, or restore them when `model_ref` is empty.
///
/// Codex keeps its own writer: its config takeover is field-level, not
/// whole-file, so it goes through the codex module with the model selected.
/// `claude-code` is the board's spelling of the file agent `claude`; the
/// gateway maps it.
pub fn save_agent(agent_id: &str, model_ref: &str) -> Result<(), ApplyError> {
    if agent_id == "codex" {
        return crate::models::save_codex_model(model_ref);
    }
    skillstar_gateway::apply_gateway(agent_id, model_ref)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::save_agent;
    use crate::test_support::{ENV_LOCK, EnvGuard};

    #[tokio::test(flavor = "current_thread")]
    async fn save_agent_writes_opencode_loopback() {
        let _lock = ENV_LOCK.lock().await;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "skillstar-save-agent-{}-{nanos}",
            std::process::id()
        ));
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let addr = PathBuf::from("127.0.0.1:21847");
        let _env = EnvGuard::set(&[
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("SKILLSTAR_TOOL_SYNC_HOME", &home),
            ("SKILLSTAR_DATA_DIR", &data),
            ("SKILLSTAR_GATEWAY_ADDR", &addr),
        ]);

        save_agent("opencode", "deepseek/pro").unwrap();
        let text = fs::read_to_string(home.join(".config/opencode/opencode.json")).unwrap();
        assert!(
            text.contains("\"baseURL\": \"http://127.0.0.1:21847/v1\""),
            "{text}"
        );
        assert!(text.contains("\"apiKey\": \"skillstar\""), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");

        let err = save_agent("goose", "deepseek/pro").unwrap_err();
        assert_eq!(err.to_string(), "agent_not_managed");
        assert!(!home.join(".config/goose").exists());

        save_agent("opencode", "").unwrap();
        assert!(!home.join(".config/opencode/opencode.json").exists());
    }
}
