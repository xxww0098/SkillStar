//! Read side of installed skills under the D-081 canonical layout.
//!
//! Skills live as real copies in `~/.agents/skills/<name>`; provenance comes
//! from the vercel lock (`skill_lock`). Update badges are a `update_state`
//! projection refreshed by comparing lock hashes against upstream trees.

use crate::agents::{self as agent_profile, AgentProfile};
pub use crate::update_state::SkillUpdateState;
use crate::{local_skill, skill_lock, update_state};
use anyhow::Result;
use skillstar_core::types::{
    Skill, SkillCategory, SkillType, UpstreamChange, extract_github_source_from_url,
    extract_skill_description,
};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};
use tracing::warn;

static SKILL_CACHE: LazyLock<RwLock<Option<Vec<Skill>>>> = LazyLock::new(|| RwLock::new(None));

pub fn invalidate_cache() {
    if let Ok(mut cache) = SKILL_CACHE.write() {
        *cache = None;
    }
}

pub fn installed_snapshot_markers() -> HashSet<String> {
    let mut markers = HashSet::new();
    for name in crate::installer::installed_names() {
        markers.insert(name.to_ascii_lowercase());
    }
    for (name, entry) in &skill_lock::load().skills {
        markers.insert(name.to_ascii_lowercase());
        if let Some(source) = extract_github_source_from_url(&entry.source_url)
            && !source.is_empty()
        {
            markers.insert(format!("{}/{}", source.to_ascii_lowercase(), name.to_ascii_lowercase()));
        }
    }
    markers
}

fn apply_cached_update_states(mut skills: Vec<Skill>) -> Vec<Skill> {
    update_state::apply_to(&mut skills);
    let policy = crate::skill_mutation::policy();
    let mut lookup_failures = 0usize;
    let mut first_failure: Option<String> = None;
    for skill in &mut skills {
        match policy.managed_repository_for_skill(&skill.name) {
            // A shared channel owns it: its updates come from the channel flow,
            // never from the generic update badge.
            Ok(Some(_)) => {
                skill.update_available = false;
                skill.upstream_change = None;
            }
            Ok(None) => {}
            // Ownership unknown (corrupt or future-versioned registry). The
            // badge is display-only and write paths re-check the gate, so the
            // computed value stays; zeroing every badge silently did real harm.
            Err(error) => {
                lookup_failures += 1;
                first_failure.get_or_insert_with(|| format!("{error:#}"));
            }
        }
    }
    if let Some(error) = first_failure {
        warn!(
            target: "skills",
            failures = lookup_failures,
            "Could not read shared-channel ownership while listing Skills; update badges are left as computed: {error}"
        );
    }
    skills
}

pub async fn list_installed_skills() -> Result<Vec<Skill>> {
    if let Ok(cache) = SKILL_CACHE.read()
        && let Some(skills) = &*cache
    {
        return Ok(apply_cached_update_states(skills.clone()));
    }

    // Local creations surface through canonical links like every other skill.
    local_skill::reconcile_hub_symlinks();

    let lock = skill_lock::load();
    let profiles: Arc<[AgentProfile]> = Arc::from(agent_profile::list_profiles());
    let names = crate::installer::installed_names();

    let built: Vec<Skill> = names
        .iter()
        .map(|name| build_installed_skill(name, lock.skills.get(name), &profiles))
        .collect::<Result<Vec<_>, _>>()?;

    let mut skills = apply_cached_update_states(built);
    skills.sort_by(|left, right| left.name.cmp(&right.name));

    if let Ok(mut cache) = SKILL_CACHE.write() {
        *cache = Some(skills.clone());
    }

    Ok(skills)
}

/// Refresh update badges by comparing the lock's recorded folder hashes with
/// upstream trees (D-081): grouped by source+ref, GitHub API first, temp
/// clone fallback. Local/bundle sources and channel-owned skills are skipped.
pub async fn refresh_skill_updates_in_session(
    _session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<SkillUpdateState>> {
    let policy = crate::skill_mutation::policy();
    let lock = skill_lock::load();
    let mut entries: Vec<(String, skill_lock::SkillLockEntry)> = Vec::new();
    for (name, entry) in lock.skills {
        if matches!(
            entry.source_type,
            skill_lock::SourceType::Local | skill_lock::SourceType::Bundle
        ) {
            continue;
        }
        match policy.managed_repository_for_skill(&name) {
            Ok(Some(_)) => continue, // channel flow owns it
            Ok(None) => {}
            Err(error) => {
                warn!(
                    target: "skills",
                    "Could not read shared-channel ownership for '{name}'; skipped its update check: {error:#}"
                );
                continue;
            }
        }
        entries.push((name, entry));
    }

    let verdicts = crate::update::check_upstream(&entries, crate::update_api::optional_github_api_token().as_deref()).await;

    let mut states = Vec::new();
    for (name, entry) in &entries {
        let state = match verdicts.get(name) {
            Some(crate::update::Upstream::Hash(hash)) => {
                let changed = entry.skill_folder_hash.as_deref() != Some(hash.as_str());
                update_state::set_stamped(name, changed);
                SkillUpdateState {
                    name: name.clone(),
                    update_available: changed,
                    upstream_change: None,
                }
            }
            Some(crate::update::Upstream::Removed) => {
                update_state::set_stamped(name, false);
                SkillUpdateState {
                    name: name.clone(),
                    update_available: false,
                    upstream_change: Some(UpstreamChange::Removed {
                        suggested_local_name: crate::skill_update::divergence::suggested_local_name(name),
                        successor: None,
                    }),
                }
            }
            // Unknown (network/API failure) keeps the previous badge.
            Some(crate::update::Upstream::Unknown) | None => SkillUpdateState {
                name: name.clone(),
                update_available: update_state::get(name).unwrap_or(false),
                upstream_change: None,
            },
        };
        states.push(state);
    }
    Ok(states)
}

fn build_installed_skill(
    name: &str,
    entry: Option<&skill_lock::SkillLockEntry>,
    profiles: &[AgentProfile],
) -> Result<Skill> {
    let dir = skillstar_core::infra::paths::agents_skill_dir(name);
    let description = extract_skill_description(&dir);
    let agent_links = detect_agent_links(name, profiles);
    let is_local = local_skill::is_local_skill(name)
        || entry.is_some_and(|entry| matches!(entry.source_type, skill_lock::SourceType::Local));
    Ok(Skill {
        name: name.to_string(),
        description,
        localized_description: None,
        skill_type: if is_local { SkillType::Local } else { SkillType::Hub },
        stars: 0,
        installed: true,
        update_available: false,
        upstream_change: None,
        last_updated: entry
            .map(|entry| entry.updated_at.clone())
            .unwrap_or_default(),
        git_url: entry
            .map(|entry| entry.source_url.clone())
            .unwrap_or_default(),
        tree_hash: entry.and_then(|entry| entry.skill_folder_hash.clone()),
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(agent_links),
        rank: None,
        source: Some(
            entry
                .map(|entry| entry.source.clone())
                .unwrap_or_else(|| name.to_string()),
        ),
    })
}

/// Agent links for one Skill, read from disk.
///
/// A freshly installed `Skill` carries no links yet, so callers that deploy and
/// then return the Skill must re-read them here instead of guessing.
pub fn agent_links_for(skill_name: &str) -> Vec<String> {
    detect_agent_links(skill_name, &agent_profile::list_profiles())
}

fn detect_agent_links(skill_name: &str, profiles: &[AgentProfile]) -> Vec<String> {
    let mut links = Vec::with_capacity(2); // most skills link to 1-2 agents
    for profile in profiles {
        if !profile.has_global_skills() {
            continue;
        }
        let link_path = profile.global_skills_dir.join(skill_name);
        // Check symlinks/junctions: is_link() AND exists() (follows target — broken = false)
        if skillstar_core::infra::fs_ops::is_link(&link_path) && link_path.exists() {
            links.push(profile.display_name.clone());
        } else if link_path.is_dir() && link_path.join("SKILL.md").exists() {
            // Also detect copy-based deployment (Windows fallback)
            links.push(profile.display_name.clone());
        }
    }
    links
}

#[allow(dead_code)]
fn skill_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
}

/// Path of one canonical skill folder, when it exists (test/patrol helper).
#[allow(dead_code)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_skills_carry_the_recorded_update_state() {
        let _guard = crate::lock_test_env();
        update_state::reset_for_test();

        update_state::commit_scan(
            update_state::stamp(),
            &[
                SkillUpdateState {
                    name: "recorded-update".to_string(),
                    update_available: true,
                    upstream_change: None,
                },
                SkillUpdateState {
                    name: "recorded-current".to_string(),
                    update_available: false,
                    upstream_change: None,
                },
            ],
        );

        let mut unknown = skillstar_core::types::Skill {
            name: "unknown".to_string(),
            description: String::new(),
            localized_description: None,
            skill_type: SkillType::Hub,
            stars: 0,
            installed: true,
            update_available: false,
            upstream_change: None,
            last_updated: String::new(),
            git_url: String::new(),
            tree_hash: None,
            category: SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(Vec::new()),
            rank: None,
            source: None,
        };
        unknown.name = "unknown".to_string();
        let skills = apply_cached_update_states(vec![
            {
                let mut s = unknown.clone();
                s.name = "recorded-update".to_string();
                s
            },
            unknown,
        ]);

        assert!(skills[0].update_available);
        assert!(
            !skills[1].update_available,
            "unscanned skills stay as built"
        );
    }
}
