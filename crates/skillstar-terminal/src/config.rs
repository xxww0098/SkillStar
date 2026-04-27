use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::registry::find_cli_binary;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SplitDirection {
    #[serde(rename = "h")]
    H,
    #[serde(rename = "v")]
    V,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LayoutNode {
    #[serde(rename = "split")]
    Split {
        direction: SplitDirection,
        ratio: f64,
        children: Box<[LayoutNode; 2]>,
    },
    #[serde(rename = "pane")]
    Pane {
        id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "providerId")]
        provider_id: Option<String>,
        #[serde(rename = "providerName")]
        provider_name: Option<String>,
        #[serde(rename = "modelId", default, skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
        #[serde(rename = "safeMode")]
        safe_mode: bool,
        #[serde(rename = "extraArgs")]
        extra_args: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    #[serde(rename = "projectName")]
    pub project_name: String,
    pub mode: LaunchMode,
    #[serde(rename = "singleLayout", default = "default_layout_node")]
    pub single_layout: LayoutNode,
    #[serde(rename = "multiLayout", default = "default_layout_node")]
    pub multi_layout: LayoutNode,
    #[serde(rename = "updatedAt")]
    pub updated_at: u64,
}

fn config_path() -> PathBuf {
    skillstar_infra::paths::config_dir().join("launch_configs.json")
}

fn default_layout_node() -> LayoutNode {
    LayoutNode::Pane {
        id: "pane-1".to_string(),
        agent_id: String::new(),
        provider_id: None,
        provider_name: None,
        model_id: None,
        safe_mode: false,
        extra_args: vec![],
    }
}

fn load_all() -> HashMap<String, LaunchConfig> {
    let path = config_path();
    if !path.exists() {
        return HashMap::new();
    }
    let Ok(data) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(&data).unwrap_or_default();
    let mut resolved = HashMap::new();

    for (k, mut val) in map {
        if let Some(obj) = val.as_object_mut() {
            if obj.contains_key("layout") && !obj.contains_key("singleLayout") {
                let layout = obj.remove("layout").unwrap();
                obj.insert("singleLayout".to_string(), layout.clone());
                obj.insert("multiLayout".to_string(), layout);
            }
        }
        if let Ok(cfg) = serde_json::from_value(val) {
            resolved.insert(k, cfg);
        }
    }
    resolved
}

fn save_all(configs: &HashMap<String, LaunchConfig>) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create dir for {}", path.display()))?;
    }
    let data =
        serde_json::to_string_pretty(configs).context("Failed to serialize launch configs")?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &data).with_context(|| format!("Failed to write {}", tmp.display()))?;
    #[cfg(windows)]
    let _ = std::fs::remove_file(&path);
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("Failed to rename {} → {}", tmp.display(), path.display()))?;
    Ok(())
}

pub fn load_config(project_name: &str) -> Option<LaunchConfig> {
    load_all().remove(project_name)
}

pub fn save_config(config: &LaunchConfig) -> Result<()> {
    let mut all = load_all();
    all.insert(config.project_name.clone(), config.clone());
    save_all(&all)
}

pub fn delete_config(project_name: &str) -> Result<()> {
    let mut all = load_all();
    all.remove(project_name);
    save_all(&all)
}

pub fn validate(config: &LaunchConfig) -> Result<(), Vec<String>> {
    let mut errors = vec![];
    let panes = collect_leaf_panes(deployable_layout(config));

    if panes.is_empty() {
        errors.push("No panes configured".to_string());
        return Err(errors);
    }

    for (id, agent_id) in &panes {
        if agent_id.is_empty() {
            errors.push(format!("Pane '{}' has no agent assigned", id));
            continue;
        }
        if find_cli_binary(agent_id).is_none() {
            errors.push(format!("{} CLI not installed", agent_id));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(dead_code)]
pub fn count_panes(node: &LayoutNode) -> usize {
    match node {
        LayoutNode::Pane { .. } => 1,
        LayoutNode::Split { children, .. } => count_panes(&children[0]) + count_panes(&children[1]),
    }
}

pub fn collect_leaf_panes(node: &LayoutNode) -> Vec<(String, String)> {
    let mut result = vec![];
    collect_panes_recursive(node, &mut result);
    result
}

fn layout_has_assigned_agent(node: &LayoutNode) -> bool {
    collect_leaf_panes(node)
        .into_iter()
        .any(|(_, agent_id)| !agent_id.is_empty())
}

pub fn deployable_layout(config: &LaunchConfig) -> &LayoutNode {
    if layout_has_assigned_agent(&config.single_layout) {
        &config.single_layout
    } else {
        &config.multi_layout
    }
}

fn collect_panes_recursive(node: &LayoutNode, out: &mut Vec<(String, String)>) {
    match node {
        LayoutNode::Pane { id, agent_id, .. } => {
            out.push((id.clone(), agent_id.clone()));
        }
        LayoutNode::Split { children, .. } => {
            collect_panes_recursive(&children[0], out);
            collect_panes_recursive(&children[1], out);
        }
    }
}

#[allow(dead_code)]
pub fn default_config(project_name: &str) -> LaunchConfig {
    let default_pane = default_layout_node();
    LaunchConfig {
        project_name: project_name.to_string(),
        mode: LaunchMode::Single,
        single_layout: default_pane.clone(),
        multi_layout: default_pane,
        updated_at: now_epoch_ms(),
    }
}

#[allow(dead_code)]
fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pane(id: &str, agent: &str) -> LayoutNode {
        LayoutNode::Pane {
            id: id.to_string(),
            agent_id: agent.to_string(),
            provider_id: None,
            provider_name: None,
            model_id: None,
            safe_mode: false,
            extra_args: vec![],
        }
    }

    fn sample_split(dir: SplitDirection, ratio: f64, a: LayoutNode, b: LayoutNode) -> LayoutNode {
        LayoutNode::Split {
            direction: dir,
            ratio,
            children: Box::new([a, b]),
        }
    }

    #[test]
    fn test_count_panes_single() {
        let node = sample_pane("1", "claude");
        assert_eq!(count_panes(&node), 1);
    }

    #[test]
    fn test_count_panes_complex() {
        let tree = sample_split(
            SplitDirection::H,
            0.5,
            sample_split(
                SplitDirection::V,
                0.5,
                sample_pane("a", "claude"),
                sample_pane("b", "codex"),
            ),
            sample_pane("c", "gemini"),
        );
        assert_eq!(count_panes(&tree), 3);
    }

    #[test]
    fn test_collect_leaf_panes() {
        let tree = sample_split(
            SplitDirection::H,
            0.6,
            sample_pane("1", "claude"),
            sample_pane("2", "codex"),
        );
        let panes = collect_leaf_panes(&tree);
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0], ("1".to_string(), "claude".to_string()));
        assert_eq!(panes[1], ("2".to_string(), "codex".to_string()));
    }

    #[test]
    fn test_default_config() {
        let config = default_config("myproject");
        assert_eq!(config.project_name, "myproject");
        assert_eq!(config.mode, LaunchMode::Single);
        assert_eq!(count_panes(&config.single_layout), 1);
        assert_eq!(count_panes(&config.multi_layout), 1);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let layout_tree = sample_split(
            SplitDirection::H,
            0.5,
            sample_pane("a", "claude"),
            sample_pane("b", "codex"),
        );
        let config = LaunchConfig {
            project_name: "test".to_string(),
            mode: LaunchMode::Multi,
            single_layout: sample_pane("c", "gemini"),
            multi_layout: layout_tree,
            updated_at: 12345,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: LaunchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.project_name, "test");
        assert_eq!(count_panes(&restored.multi_layout), 2);
        assert_eq!(count_panes(&restored.single_layout), 1);
    }
}
