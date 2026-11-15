use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub skills: Vec<String>,
    #[serde(default)]
    pub skill_sources: std::collections::HashMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_agent: String,
    /// Agent ids this deck is explicitly linked to, owned by the deck itself.
    ///
    /// A fresh deck starts empty: creating a deck never claims an Agent, even
    /// when its Skills are already linked there by the install-time global
    /// deploy. `None` means the deck never recorded a rail. The deck page shows
    /// that the same way as an empty set: unlinked.
    #[serde(default)]
    pub agent_links: Option<Vec<String>>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GroupStore {
    groups: Vec<SkillGroup>,
}

fn store_path() -> PathBuf {
    ss_core::infra::paths::groups_path()
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

pub fn list_groups() -> Vec<SkillGroup> {
    load_store().groups
}

pub fn create_group(
    name: String,
    description: String,
    icon: String,
    skills: Vec<String>,
    skill_sources: std::collections::HashMap<String, String>,
) -> Result<SkillGroup> {
    let mut store = load_store();

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
        // A new deck claims no Agent until the user lights one up on the rail.
        agent_links: Some(Vec::new()),
        created_at: now.clone(),
        updated_at: now,
    };

    store.groups.push(group.clone());
    save_store(&store)?;

    Ok(group)
}

/// What happened when an import tried to create the deck for members that are
/// already installed. Member install is not undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckAttach {
    Created,
    /// A deck with this name already exists. The import itself succeeded.
    NameTaken,
    /// No deck name, or no member names. Nothing was written.
    Skipped,
}

/// Create the deck for a finished member install.
///
/// A blank name uses `fallback_name`. Still blank, or no member names, skips
/// the write. A duplicate name is [`DeckAttach::NameTaken`], not an error.
/// Any other store failure is returned after the members are left in place.
pub fn attach_imported_deck(
    name: &str,
    fallback_name: &str,
    description: String,
    icon: String,
    skills: Vec<String>,
    skill_sources: std::collections::HashMap<String, String>,
) -> Result<DeckAttach> {
    if skills.is_empty() {
        return Ok(DeckAttach::Skipped);
    }
    let resolved = if name.trim().is_empty() {
        fallback_name.trim()
    } else {
        name.trim()
    };
    if resolved.is_empty() {
        return Ok(DeckAttach::Skipped);
    }
    if list_groups().iter().any(|group| group.name == resolved) {
        return Ok(DeckAttach::NameTaken);
    }
    let icon = if icon.trim().is_empty() {
        "📦".to_string()
    } else {
        icon
    };
    match create_group(
        resolved.to_string(),
        description,
        icon,
        skills,
        skill_sources,
    ) {
        Ok(_) => Ok(DeckAttach::Created),
        Err(error) if error.to_string().contains("already exists") => Ok(DeckAttach::NameTaken),
        Err(error) => Err(error),
    }
}

pub fn update_group(
    id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    skills: Option<Vec<String>>,
    skill_sources: Option<std::collections::HashMap<String, String>>,
    agent_links: Option<Vec<String>>,
) -> Result<SkillGroup> {
    let mut store = load_store();
    if let Some(ref new_name) = name
        && store
            .groups
            .iter()
            .any(|g| g.id != id && &g.name == new_name)
    {
        anyhow::bail!("A group with the name '{}' already exists", new_name);
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
    if let Some(v) = agent_links {
        group.agent_links = Some(dedupe_agent_links(v));
    }
    group.updated_at = chrono::Utc::now().to_rfc3339();

    let updated = group.clone();
    save_store(&store)?;
    Ok(updated)
}

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

    // A copy holds the same Skills, so it is on the same Agents as its source.
    let source_links = source.agent_links.clone();
    let copy = create_group(
        new_name,
        source.description.clone(),
        source.icon.clone(),
        source.skills.clone(),
        source.skill_sources.clone(),
    )?;

    match source_links {
        Some(links) if !links.is_empty() => {
            update_group(copy.id.clone(), None, None, None, None, None, Some(links))
        }
        // `None` means the source never recorded a rail. `create_group` starts
        // a deck with an empty rail, so put the copy back to `None`. The deck
        // page treats both as unlinked.
        Some(_) => Ok(copy),
        None => clear_agent_links(&copy.id).map(|_| copy),
    }
}

/// Put a copy back to `None`. `create_group` starts a deck with an empty rail.
fn clear_agent_links(id: &str) -> Result<()> {
    let mut store = load_store();
    let Some(group) = store.groups.iter_mut().find(|g| g.id == id) else {
        return Ok(());
    };
    group.agent_links = None;
    save_store(&store)
}

fn dedupe_agent_links(links: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    links
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sandbox the group store so tests never touch the real state dir.
    fn sandbox() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
        let guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        // SAFETY: `lock_test_env` is held for as long as the override is live.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        (guard, temp)
    }

    fn new_deck(name: &str) -> SkillGroup {
        create_group(
            name.to_string(),
            String::new(),
            "💻".to_string(),
            vec!["git-flow".to_string()],
            Default::default(),
        )
        .unwrap()
    }

    #[test]
    fn new_deck_claims_no_agent() {
        let _sandbox = sandbox();
        assert_eq!(new_deck("fresh").agent_links, Some(Vec::new()));
    }

    #[test]
    fn update_replaces_and_dedupes_links() {
        let _sandbox = sandbox();
        let deck = new_deck("editable");
        let updated = update_group(
            deck.id,
            None,
            None,
            None,
            None,
            None,
            Some(vec![
                "claude".to_string(),
                "claude".to_string(),
                "  ".to_string(),
                "codex".to_string(),
            ]),
        )
        .unwrap();
        assert_eq!(
            updated.agent_links,
            Some(vec!["claude".to_string(), "codex".to_string()])
        );
    }

    #[test]
    fn imported_deck_reports_a_taken_name_without_a_second_group() {
        let _sandbox = sandbox();
        new_deck("taken");
        let status = attach_imported_deck(
            "taken",
            "fallback",
            "desc".into(),
            String::new(),
            vec!["member".into()],
            Default::default(),
        )
        .unwrap();
        assert_eq!(status, DeckAttach::NameTaken);
        assert_eq!(list_groups().len(), 1);
        assert_eq!(list_groups()[0].skills, vec!["git-flow".to_string()]);
    }

    #[test]
    fn imported_deck_skips_a_blank_name_and_uses_a_fallback() {
        let _sandbox = sandbox();
        let skipped = attach_imported_deck(
            "  ",
            "",
            "desc".into(),
            String::new(),
            vec!["member".into()],
            Default::default(),
        )
        .unwrap();
        assert_eq!(skipped, DeckAttach::Skipped);
        assert!(list_groups().is_empty());

        let created = attach_imported_deck(
            "",
            "fallback",
            "desc".into(),
            "  ".into(),
            vec!["member".into()],
            Default::default(),
        )
        .unwrap();
        assert_eq!(created, DeckAttach::Created);
        let groups = list_groups();
        assert_eq!(groups[0].name, "fallback");
        assert_eq!(groups[0].icon, "📦");
    }

    #[test]
    fn share_deck_keeps_the_installed_member_when_the_name_is_taken() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        let content = "---\nname: member\ndescription: d\n---\nbody\n";
        let skill = crate::share_install::ShareCodeSkill {
            n: "member".into(),
            u: String::new(),
            c: Some(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                content,
            )),
            p: None,
        };
        let first = crate::share_install::install_share_and_deck(
            true,
            "Deck",
            "d".into(),
            String::new(),
            vec![skill.clone()],
        )
        .unwrap();
        assert_eq!(first.deck, DeckAttach::Created);
        assert_eq!(first.summary.embedded_names, vec!["member".to_string()]);
        let body = std::fs::read_to_string(
            ss_core::infra::paths::local_skills_dir().join("member/SKILL.md"),
        )
        .unwrap();

        let second = crate::share_install::install_share_and_deck(
            true,
            "Deck",
            "d".into(),
            String::new(),
            vec![skill],
        )
        .unwrap();
        assert_eq!(second.deck, DeckAttach::NameTaken);
        assert_eq!(list_groups().len(), 1);
        assert_eq!(
            std::fs::read_to_string(
                ss_core::infra::paths::local_skills_dir().join("member/SKILL.md")
            )
            .unwrap(),
            body
        );
    }

    #[test]
    fn duplicate_inherits_source_links() {
        let _sandbox = sandbox();
        let deck = new_deck("original");
        update_group(
            deck.id.clone(),
            None,
            None,
            None,
            None,
            None,
            Some(vec!["claude".to_string()]),
        )
        .unwrap();

        let copy = duplicate_group(&deck.id).unwrap();
        assert_eq!(copy.agent_links, Some(vec!["claude".to_string()]));
    }
}
