//! User-defined custom agents and their `AgentSpec` impl + write operations.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::profile_storage::PrefsStore;
use super::spec::AgentSpec;
use super::validation::validate_project_skills_rel;

/// A user-defined agent template, persisted in `ProfilePrefs::custom_profiles`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomProfileDef {
    pub id: String,
    pub display_name: String,
    pub global_skills_dir: String,
    pub project_skills_rel: String,
    pub icon_data_uri: Option<String>,
}

/// `AgentSpec` view over one custom agent definition.
pub(crate) struct CustomSpec<'a>(pub &'a CustomProfileDef);

impl AgentSpec for CustomSpec<'_> {
    fn id(&self) -> &str {
        &self.0.id
    }
    fn display_name(&self) -> &str {
        &self.0.display_name
    }
    fn icon(&self) -> &str {
        self.0
            .icon_data_uri
            .as_deref()
            .unwrap_or("agents/openclaw.svg")
    }
    fn resolve_global_dir(&self, home: &Path) -> PathBuf {
        let path_str = &self.0.global_skills_dir;
        // Handle both ~/ (Unix) and ~\ (Windows) prefixes.
        if let Some(stripped) = path_str
            .strip_prefix("~/")
            .or_else(|| path_str.strip_prefix("~\\"))
        {
            home.join(stripped)
        } else {
            PathBuf::from(path_str)
        }
    }
    fn project_skills_rel(&self) -> Option<&str> {
        let rel = self.0.project_skills_rel.as_str();
        if rel.is_empty() { None } else { Some(rel) }
    }
}

/// Add (or replace by id) a custom agent and enable it.
pub(crate) fn add(def: CustomProfileDef, store: &dyn PrefsStore) -> Result<()> {
    let normalized_project_rel = validate_project_skills_rel(&def.project_skills_rel)?;

    let mut prefs = store.load();

    let mut new_def = def;
    new_def.project_skills_rel = normalized_project_rel;
    if new_def.id.is_empty() {
        new_def.id = format!(
            "custom_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
    }

    prefs.custom_profiles.retain(|p| p.id != new_def.id);
    prefs.enabled.insert(new_def.id.clone(), true);
    prefs.custom_profiles.push(new_def);
    store.save(&prefs)
}

/// Remove a custom agent by id.
pub(crate) fn remove(id: &str, store: &dyn PrefsStore) -> Result<()> {
    let mut prefs = store.load();
    prefs.custom_profiles.retain(|p| p.id != id);
    prefs.enabled.remove(id);
    store.save(&prefs)
}
