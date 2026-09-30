//! Model ids the picker can show.
//!
//! Catalog rows come from the models.dev cache. Group rows come from
//! `model_gateway.json`. Neither source contributes a secret or a vendor URL.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
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
        &skillstar_gateway::models_dev_load(),
        &skillstar_gateway::stored_group_ids(),
        &skillstar_gateway::stored_model_names(),
    )
    .into_iter()
    .filter(|choice| skillstar_gateway::model_shown(agent, &choice.id))
    .collect()
}

#[cfg(test)]
fn project_choices(catalog: &[u8], group_ids: &[String]) -> Vec<ModelChoiceDto> {
    project_named(catalog, group_ids, &BTreeMap::new())
}

fn project_named(
    catalog: &[u8],
    group_ids: &[String],
    names: &BTreeMap<String, String>,
) -> Vec<ModelChoiceDto> {
    let mut choices = catalog_ids(catalog, names);
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

fn catalog_ids(body: &[u8], names: &BTreeMap<String, String>) -> Vec<ModelChoiceDto> {
    let Ok(Value::Object(providers)) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let mut choices = Vec::new();
    for (provider, entry) in providers {
        if provider.is_empty() || provider.contains('/') {
            continue;
        }
        let Some(models) = entry.get("models").and_then(Value::as_object) else {
            continue;
        };
        for model in models.keys() {
            if model.is_empty() || model.contains('/') {
                continue;
            }
            let id = format!("{provider}/{model}");
            let label = skillstar_gateway::model_label(&id, names);
            choices.push(ModelChoiceDto { id, label });
        }
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_are_provider_or_group_ids_and_carry_no_secret() {
        let catalog = br#"{"openai":{"id":"openai","base_url":"https://api.example/v1","api_key":"sk-secret-value","models":{"gpt-test":{"id":"gpt-test","url":"https://vendor.example/m"}}}}"#;
        let choices = project_choices(catalog, &["fast".to_string()]);
        assert_eq!(
            choices,
            vec![
                ModelChoiceDto {
                    id: "openai/gpt-test".to_string(),
                    label: "openai/gpt-test".to_string(),
                },
                ModelChoiceDto {
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

    #[test]
    fn a_stored_name_changes_the_label_only() {
        let catalog = br#"{"openai":{"models":{"gpt-test":{"id":"gpt-test","url":"https://vendor.example/m"}}}}"#;
        let mut names = BTreeMap::new();
        names.insert("openai/gpt-test".to_string(), "实验".to_string());
        let choices = project_named(catalog, &[], &names);
        assert_eq!(choices[0].id, "openai/gpt-test");
        assert_eq!(choices[0].label, "实验");
        let text = serde_json::to_string(&choices).unwrap();
        assert!(!text.contains("https://"));
        assert!(!text.contains("vendor.example"));
    }

    #[test]
    fn a_missing_catalog_still_lists_groups() {
        let choices = project_choices(b"", &["fast".to_string()]);
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
mod save_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::load_model_choices;
    use crate::models::save_agent;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_gateway::save_group;

    #[tokio::test(flavor = "current_thread")]
    async fn saving_a_group_id_uses_the_existing_writer() {
        let _lock = ENV_LOCK.lock().await;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("skillstar-picker-{}-{nanos}", std::process::id()));
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

        fs::write(
            data.join("cache/gateway-catalog/models.dev.json"),
            br#"{"openai":{"models":{"gpt-test":{"id":"gpt-test"}}}}"#,
        )
        .unwrap_or_else(|_| {
            fs::create_dir_all(data.join("cache/gateway-catalog")).unwrap();
            fs::write(
                data.join("cache/gateway-catalog/models.dev.json"),
                br#"{"openai":{"models":{"gpt-test":{"id":"gpt-test"}}}}"#,
            )
            .unwrap();
        });
        save_group("fast", &["openai/gpt-test"]).unwrap();
        let ids: Vec<_> = load_model_choices("opencode")
            .into_iter()
            .map(|choice| choice.id)
            .collect();
        assert_eq!(ids, vec!["openai/gpt-test".to_string(), "group/fast".to_string()]);

        save_agent("opencode", "group/fast").unwrap();
        let text = fs::read_to_string(home.join(".config/opencode/opencode.json")).unwrap();
        assert!(text.contains("\"group/fast\""), "{text}");
        assert!(text.contains("http://127.0.0.1:21847/v1"), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");

        let err = save_agent("goose", "group/fast").unwrap_err();
        assert_eq!(err.to_string(), "agent_not_managed");
        assert!(!home.join(".config/goose").exists());
    }
}
