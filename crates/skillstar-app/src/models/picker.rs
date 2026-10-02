//! Model ids the picker can show.
//!
//! Catalog rows come from the gateway's typed parse of the models.dev
//! cache; group rows come from `model_gateway.json`. This module parses no
//! catalog bytes of its own. Neither source contributes a secret or a
//! vendor URL.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One picker row. `id` is `provider/model` or `group/<id>`.
/// `label` is the display name, or the same id when none was saved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "ModelChoiceDto.ts")]
pub struct ModelChoiceDto {
    pub id: String,
    pub label: String,
}

/// Catalog entries this agent is shown, then saved groups. A missing cache is empty.
pub fn load_model_choices(agent: &str) -> Vec<ModelChoiceDto> {
    project_named(
        &skillstar_gateway::catalog_ids(),
        &skillstar_gateway::stored_group_ids(),
        &skillstar_gateway::stored_model_names(),
    )
    .into_iter()
    .filter(|choice| skillstar_gateway::model_shown(agent, &choice.id))
    .collect()
}

fn project_named(
    ids: &[String],
    group_ids: &[String],
    names: &BTreeMap<String, String>,
) -> Vec<ModelChoiceDto> {
    let mut choices = ids
        .iter()
        .map(|id| ModelChoiceDto {
            id: id.clone(),
            label: skillstar_gateway::model_label(id, names),
        })
        .collect::<Vec<_>>();
    for id in group_ids {
        if id.is_empty() || id.contains('/') {
            continue;
        }
        let id = format!("group/{id}");
        let label = skillstar_gateway::model_label(&id, names);
        choices.push(ModelChoiceDto { id, label });
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_name_changes_the_label_only() {
        let mut names = BTreeMap::new();
        names.insert("openai/gpt-test".to_string(), "实验".to_string());
        let choices = project_named(&["openai/gpt-test".to_string()], &[], &names);
        assert_eq!(
            choices,
            vec![ModelChoiceDto {
                id: "openai/gpt-test".to_string(),
                label: "实验".to_string(),
            }]
        );
    }

    #[test]
    fn a_missing_catalog_still_lists_groups() {
        let choices = project_named(&[], &["fast".to_string()], &BTreeMap::new());
        assert_eq!(
            choices,
            vec![ModelChoiceDto {
                id: "group/fast".to_string(),
                label: "group/fast".to_string(),
            }]
        );
    }
}

#[cfg(test)]
mod store_tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use super::load_model_choices;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_gateway::save_group;

    /// Env-sandboxed scratch roots: `home` and `data` are fresh per call.
    /// Dropping restores every storage-root variable and deletes the roots.
    /// The env lock is held for as long as the sandbox lives, so concurrent
    /// tests cannot swap each other's storage root mid-flight.
    struct Sandbox {
        _lock: tokio::sync::MutexGuard<'static, ()>,
        _env: EnvGuard,
        root: PathBuf,
        home: PathBuf,
        data: PathBuf,
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    async fn sandbox(label: &str) -> Sandbox {
        let lock = ENV_LOCK.lock().await;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "skillstar-picker-{label}-{}-{nanos}",
            std::process::id()
        ));
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let addr = PathBuf::from("127.0.0.1:21847");
        let env = EnvGuard::set(&[
            ("HOME", &home),
            ("USERPROFILE", &home),
            ("SKILLSTAR_TOOL_SYNC_HOME", &home),
            ("SKILLSTAR_DATA_DIR", &data),
            ("SKILLSTAR_GATEWAY_ADDR", &addr),
        ]);
        Sandbox {
            _lock: lock,
            _env: env,
            root,
            home,
            data,
        }
    }

    fn write_catalog(data: &Path, body: &[u8]) {
        let dir = data.join("cache").join("gateway-catalog");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("models.dev.json"), body).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn choices_are_provider_or_group_ids_and_carry_no_secret() {
        let box_ = sandbox("secret").await;
        write_catalog(
            &box_.data,
            br#"{"openai":{"id":"openai","base_url":"https://api.example/v1","api_key":"sk-secret-value","models":{"gpt-test":{"id":"gpt-test","url":"https://vendor.example/m"}}}}"#,
        );
        save_group("fast", &["openai/gpt-test"]).unwrap();
        let choices = load_model_choices("opencode");
        assert_eq!(
            choices,
            vec![
                super::ModelChoiceDto {
                    id: "openai/gpt-test".to_string(),
                    label: "openai/gpt-test".to_string(),
                },
                super::ModelChoiceDto {
                    id: "group/fast".to_string(),
                    label: "group/fast".to_string(),
                },
            ]
        );
        let text = serde_json::to_string(&choices).unwrap();
        assert!(!text.contains("sk-secret-value"));
        assert!(!text.contains("https://"));
        assert!(!text.contains("api.example"));
        assert!(!text.contains("vendor.example"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saving_a_group_id_uses_the_existing_writer() {
        let box_ = sandbox("writer").await;
        write_catalog(
            &box_.data,
            br#"{"openai":{"models":{"gpt-test":{"id":"gpt-test"}}}}"#,
        );
        save_group("fast", &["openai/gpt-test"]).unwrap();
        let ids: Vec<_> = load_model_choices("opencode")
            .into_iter()
            .map(|choice| choice.id)
            .collect();
        assert_eq!(ids, vec!["openai/gpt-test".to_string(), "group/fast".to_string()]);

        crate::models::save_agent("opencode", "group/fast").unwrap();
        let text = fs::read_to_string(box_.home.join(".config/opencode/opencode.json")).unwrap();
        assert!(text.contains("\"group/fast\""), "{text}");
        assert!(text.contains("http://127.0.0.1:21847/v1"), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");

        let err = crate::models::save_agent("goose", "group/fast").unwrap_err();
        assert_eq!(err.to_string(), "agent_not_managed");
        assert!(!box_.home.join(".config/goose").exists());
    }
}
