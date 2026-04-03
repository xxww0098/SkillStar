use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A reusable bundle of skills that can be deployed to projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillGroup {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable name, e.g. "Rust 全栈", "Frontend Design".
    pub name: String,
    /// Short description of what this group is for.
    pub description: String,
    /// Emoji icon displayed on the card.
    pub icon: String,
    /// Ordered list of skill names included in this group.
    pub skills: Vec<String>,
    /// Map of skill names to their git_urls, for downloading when importing from a share code.
    #[serde(default)]
    pub skill_sources: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_agent: String, // Keeping field with default for backwards compatibility but it is ignored now
    /// ISO 8601 timestamp when the group was created.
    pub created_at: String,
    /// ISO 8601 timestamp when the group was last modified.
    pub updated_at: String,
}

/// Persisted collection of all user-defined groups.
#[derive(Debug, Serialize, Deserialize, Default)]
struct GroupStore {
    groups: Vec<SkillGroup>,
}

/// Path to the JSON file storing groups.
fn store_path() -> PathBuf {
    super::paths::groups_path()
}

fn load_store() -> GroupStore {
    let path = store_path();
    if !path.exists() {
        return GroupStore::default();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return GroupStore::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_store(store: &GroupStore) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(store).context("Failed to serialize group store")?;
    std::fs::write(&path, content).context("Failed to write group store")?;
    Ok(())
}

// ── Public API ──────────────────────────────────────────────────────

/// List all skill groups.
pub fn list_groups() -> Vec<SkillGroup> {
    load_store().groups
}

/// Create a new skill group. Returns the created group.
pub fn create_group(
    name: String,
    description: String,
    icon: String,
    skills: Vec<String>,
    skill_sources: std::collections::HashMap<String, String>,
) -> Result<SkillGroup> {
    let mut store = load_store();

    // Enforce unique name
    if store.groups.iter().any(|g| g.name == name) {
        anyhow::bail!("A group with the name '{}' already exists", name);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let group = SkillGroup {
        id: uuid_v4(),
        name,
        description,
        icon,
        skills,
        skill_sources,
        default_agent: String::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    store.groups.push(group.clone());
    save_store(&store)?;

    Ok(group)
}

/// Update an existing skill group by ID. Returns the updated group.
pub fn update_group(
    id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    skills: Option<Vec<String>>,
    skill_sources: Option<std::collections::HashMap<String, String>>,
) -> Result<SkillGroup> {
    let mut store = load_store();
    if let Some(ref new_name) = name {
        if store
            .groups
            .iter()
            .any(|g| &g.id != &id && &g.name == new_name)
        {
            anyhow::bail!("A group with the name '{}' already exists", new_name);
        }
    }

    let group = store
        .groups
        .iter_mut()
        .find(|g| g.id == id)
        .ok_or_else(|| anyhow::anyhow!("Group '{}' not found", id))?;

    if let Some(v) = name {
        group.name = v;
    }
    if let Some(v) = description {
        group.description = v;
    }
    if let Some(v) = icon {
        group.icon = v;
    }
    if let Some(v) = skills {
        group.skills = v;
    }
    if let Some(v) = skill_sources {
        group.skill_sources = v;
    }
    group.updated_at = chrono::Utc::now().to_rfc3339();

    let updated = group.clone();
    save_store(&store)?;
    Ok(updated)
}

/// Delete a skill group by ID.
pub fn delete_group(id: &str) -> Result<()> {
    let mut store = load_store();
    let before = store.groups.len();
    store.groups.retain(|g| g.id != id);
    if store.groups.len() == before {
        anyhow::bail!("Group '{}' not found", id);
    }
    save_store(&store)?;
    Ok(())
}

/// Duplicate an existing group with a new name.
pub fn duplicate_group(id: &str) -> Result<SkillGroup> {
    let store = load_store();
    let source = store
        .groups
        .iter()
        .find(|g| g.id == id)
        .ok_or_else(|| anyhow::anyhow!("Group '{}' not found", id))?;

    let re = regex::Regex::new(r"^(.*?)(?:\s*\(Copy[\s\d]*\))+$").unwrap();
    let base_name = if let Some(caps) = re.captures(&source.name) {
        if let Some(m) = caps.get(1) {
            m.as_str().trim_end().to_string()
        } else {
            source.name.clone()
        }
    } else {
        source.name.clone()
    };

    let mut counter = 1;
    let mut new_name = format!("{} (Copy{})", base_name, counter);
    while store.groups.iter().any(|g| g.name == new_name) {
        counter += 1;
        new_name = format!("{} (Copy{})", base_name, counter);
    }

    create_group(
        new_name,
        source.description.clone(),
        source.icon.clone(),
        source.skills.clone(),
        source.skill_sources.clone(),
    )
}

/// RFC 4122 UUID v4 generator (cryptographically random).
fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}
