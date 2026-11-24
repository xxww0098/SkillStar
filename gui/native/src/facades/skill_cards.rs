//! Deck-page facade — the skill-cards vertical slice owns this file
//! (`skill_cards/mod.rs` + `group_card.rs` + `query.rs` seam: deck
//! listing, deck cards, and the keep-alive query/refresh model).
//!
//! Registered empty so parallel slice workers never touch a shared registry
//! file: methods below surface in TSX as `SkillCards.*`. Every call is a
//! thin pass-through to `ss_skills` (the same functions the GPUI page's
//! `spawn_domain` jobs run, skill_cards/mod.rs:35-39 and :135-222), with the
//! UI-thread courtesy the GPUI side keeps: the link job tries the update
//! transaction lock with a zero wait instead of blocking on it.
//!
//! Async exports follow nsh's async form (a task of the core, Promise on the
//! TS side). The domain calls themselves are small local-file reads/writes
//! (groups.json, the agents registry, one skills dir listing), so the task
//! runs them inline — no separate blocking pool, mirroring how cheap the
//! GPUI page treats these jobs.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use ss_skills::agents::AgentProfile;
use ss_skills::installed_skill;
use ss_skills::installer;
use ss_skills::skill_group;
use ss_skills::workflows::agent_links::AgentLinkReport;
use ss_skills::workflows::skill_group_links;

// The crate cannot depend on serde_json directly (its manifest is frozen
// for the parallel waves), so the JSON builders here go through what
// nsh-core exports: `JsonValue` (serde_json::Value) and the doc-hidden
// `JsonMap` re-export its own generated code uses.
use nsh_core::element::__private::JsonMap;

fn obj(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
    let mut map = JsonMap::new();
    for (key, value) in pairs {
        map.insert(key.to_owned(), value);
    }
    JsonValue::Object(map)
}

fn str(value: &str) -> JsonValue {
    JsonValue::String(value.to_owned())
}

fn num(value: usize) -> JsonValue {
    JsonValue::from(value as u64)
}

/// The deck-page seam (`SkillCards` in TSX).
#[module]
pub struct SkillCards;

#[module]
impl SkillCards {
    /// One deck-page snapshot in a single call — the data `SkillCardsPage`
    /// loads on mount and on every refresh (skill_cards/mod.rs:61-98):
    /// `decks` from `skill_group::list_groups`, `installedSkills` from
    /// `installer::installed_names`, and `agents`, the targetable carousel
    /// set (`targetable_agent_profiles`: a global skills directory AND
    /// enabled in Settings). Read-only.
    pub async fn snapshot() -> Result<JsonValue, String> {
        blocking(snapshot_body)
            .await
            .map_err(|join| format!("snapshot task failed: {join}"))?
    }

    /// Duplicate a deck (`skill_group::duplicate_group`, the deck card's
    /// 「复制卡组」 command). The copy lands beside its source with the same
    /// members; the page refreshes on resolve.
    pub async fn duplicate_deck(deck_id: String) -> Result<JsonValue, String> {
        run_group_job(move || skill_group::duplicate_group(&deck_id).map(|group| str(&group.id)))
            .await
    }

    /// Delete a deck (`skill_group::delete_group`, the deck card's
    /// 「删除卡组…」 command after the name-confirm dialog). Hub skills stay
    /// intact — the domain only drops the group record.
    pub async fn delete_deck(deck_id: String) -> Result<(), String> {
        run_group_job(move || skill_group::delete_group(&deck_id).map(|_| ()))
            .await
    }

    /// Toggle a whole deck's link to one agent
    /// (`skill_group_links::set_deck_agent`, a carousel slot click on the
    /// deck card). Like the GPUI rail job, the update transaction lock is
    /// tried with a zero wait — a running Skill transaction fails fast with
    /// its message instead of freezing the rail. Resolves with the link
    /// report `{applied: [[agent, skill]…], skipped: […], failed: […]}` so
    /// the page can build the partial-failure notice.
    pub async fn toggle_deck_agent(
        deck_id: String,
        agent_id: String,
        enable: bool,
    ) -> Result<JsonValue, String> {
        run_link_job(move || skill_group_links::set_deck_agent(&deck_id, &agent_id, enable)).await
    }
}

fn snapshot_body() -> Result<JsonValue, String> {
    let decks = skill_group::list_groups();
    let installed = installer::installed_names();
    let profiles = ss_skills::agents::list_profiles();
    let targetable: Vec<&AgentProfile> = profiles
        .iter()
        .filter(|profile| profile.has_global_skills() && profile.enabled)
        .collect();
    let installed_links: HashMap<String, Vec<String>> = installed
        .iter()
        .map(|name| {
            let links = installed_skill::agent_links_for(name)
                .into_iter()
                .map(|raw| canonical_agent_id(&raw, &profiles))
                .collect::<Vec<_>>();
            (name.trim().to_ascii_lowercase(), links)
        })
        .collect();
    let agents = targetable
        .iter()
        .map(|profile| {
            obj(vec![
                ("id", str(&profile.id)),
                ("displayName", str(&profile.display_name)),
                ("icon", str(&profile.icon)),
            ])
        })
        .collect::<Vec<_>>();
    Ok(obj(vec![
        (
            "decks",
            JsonValue::Array(
                decks
                    .iter()
                    .map(|group| deck_json_with_members(group, &installed_links, &targetable))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "installedSkills",
            JsonValue::Array(installed.iter().map(|name| str(name)).collect()),
        ),
        ("agents", JsonValue::Array(agents)),
    ]))
}

fn canonical_agent_id(raw: &str, profiles: &[AgentProfile]) -> String {
    let trimmed = raw.trim();
    profiles
        .iter()
        .find(|profile| {
            profile.id.eq_ignore_ascii_case(trimmed)
                || profile.display_name.eq_ignore_ascii_case(trimmed)
        })
        .map(|profile| profile.id.clone())
        .unwrap_or_else(|| trimmed.to_owned())
}

fn matches_agent_ref(links: &[String], profile: &AgentProfile) -> bool {
    links.iter().any(|link| {
        let trimmed = link.trim();
        trimmed.eq_ignore_ascii_case(&profile.id)
            || trimmed.eq_ignore_ascii_case(&profile.display_name)
    })
}

fn compute_deck_agent_states(
    group: &skill_group::SkillGroup,
    installed_links: &HashMap<String, Vec<String>>,
    targetable: &[&AgentProfile],
) -> (JsonMap<String, JsonValue>, JsonMap<String, JsonValue>) {
    let mut member_links_map = JsonMap::new();
    let mut seen_members = HashSet::new();
    let mut installed_member_links: Vec<&[String]> = Vec::new();
    for skill_name in &group.skills {
        let lower = skill_name.trim().to_ascii_lowercase();
        if !seen_members.insert(lower.clone()) {
            continue;
        }
        if let Some(links) = installed_links.get(&lower) {
            member_links_map.insert(
                skill_name.clone(),
                JsonValue::Array(links.iter().map(|id| str(id)).collect()),
            );
            installed_member_links.push(links.as_slice());
        }
    }

    let mut state_map = JsonMap::new();
    let declared_links = group.agent_links.as_deref().unwrap_or(&[]);
    for profile in targetable {
        let declared = matches_agent_ref(declared_links, profile);
        let state = if !declared {
            "none"
        } else if installed_member_links.is_empty() {
            "linked"
        } else {
            let linked_count = installed_member_links
                .iter()
                .filter(|links| matches_agent_ref(links, profile))
                .count();
            if linked_count == installed_member_links.len() {
                "linked"
            } else {
                "mixed"
            }
        };
        state_map.insert(profile.id.clone(), str(state));
    }
    (member_links_map, state_map)
}

/// The group-command tail every deck job shares (skill_cards/mod.rs
/// `run_group_job`): run on the blocking pool, surface the domain error chain as
/// the UI notice string. The error stays generic (`{err:#}` keeps
/// anyhow's alternate-chain formatting when the domain hands one over).
async fn run_group_job<T: Send + 'static, E: std::fmt::Display + Send + 'static>(
    job: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> Result<T, String> {
    blocking(move || job().map_err(|err| format!("{err:#}")))
        .await
        .map_err(|join| format!("group task failed: {join}"))?
}

/// The link-command tail (skill_cards/mod.rs `run_link_job`): acquire the
/// update transaction lock with a zero wait on the blocking pool (do not wait
/// on a Skill transaction or block the JS/async thread), then run the link
/// job and shape the report as JSON.
async fn run_link_job<E: std::fmt::Display + Send + 'static>(
    job: impl FnOnce() -> Result<AgentLinkReport, E> + Send + 'static,
) -> Result<JsonValue, String> {
    blocking(move || link_job_body(job))
        .await
        .map_err(|join| format!("link task failed: {join}"))?
}

fn link_job_body<E: std::fmt::Display>(
    job: impl FnOnce() -> Result<AgentLinkReport, E>,
) -> Result<JsonValue, String> {
    let _guard = ss_skills::skill_update::try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    let report = job().map_err(|err| format!("{err:#}"))?;
    Ok(report_to_json(&report))
}

fn report_to_json(report: &AgentLinkReport) -> JsonValue {
    let applied = report
        .applied
        .iter()
        .map(|(agent, skill)| JsonValue::Array(vec![str(agent), str(skill)]))
        .collect::<Vec<_>>();
    let skipped = report
        .skipped
        .iter()
        .map(|issue| link_issue_json(issue))
        .collect::<Vec<_>>();
    let failed = report
        .failed
        .iter()
        .map(|issue| link_issue_json(issue))
        .collect::<Vec<_>>();
    obj(vec![
        ("applied", JsonValue::Array(applied)),
        ("skipped", JsonValue::Array(skipped)),
        ("failed", JsonValue::Array(failed)),
    ])
}

fn link_issue_json(issue: &ss_skills::workflows::agent_links::AgentLinkIssue) -> JsonValue {
    obj(vec![
        ("agentId", str(&issue.agent_id)),
        ("skill", str(&issue.skill)),
        ("reason", str(&issue.reason)),
    ])
}

/// One deck as the page's TS deck record: the SkillGroup fields the card
/// and the query predicate read (skill_cards/query.rs + group_card.rs).
/// The struct is Serialize, so this mirrors serde's field names exactly.
fn deck_json_with_members(
    group: &skill_group::SkillGroup,
    installed_links: &HashMap<String, Vec<String>>,
    targetable: &[&AgentProfile],
) -> JsonValue {
    let (member_agent_links, agent_link_states) =
        compute_deck_agent_states(group, installed_links, targetable);
    obj(vec![
        ("id", str(&group.id)),
        ("name", str(&group.name)),
        ("description", str(&group.description)),
        ("icon", str(&group.icon)),
        (
            "skills",
            JsonValue::Array(group.skills.iter().map(|s| str(s)).collect()),
        ),
        (
            "skillSources",
            JsonValue::Object({
                let mut map = JsonMap::new();
                for (skill, source) in &group.skill_sources {
                    map.insert(skill.clone(), str(source));
                }
                map
            }),
        ),
        ("defaultAgent", str(&group.default_agent)),
        (
            "agentLinks",
            match &group.agent_links {
                Some(links) => JsonValue::Array(links.iter().map(|id| str(id)).collect()),
                // None and an empty set read the same on the rail: unlinked
                // (skill_group.rs AgentLinks doc).
                None => JsonValue::Array(vec![]),
            },
        ),
        ("memberAgentLinks", JsonValue::Object(member_agent_links)),
        ("agentLinkStates", JsonValue::Object(agent_link_states)),
        ("createdAt", str(&group.created_at)),
        ("updatedAt", str(&group.updated_at)),
        ("memberCount", num(group.skills.len())),
    ])
}

#[cfg(test)]
mod tests {
    use ss_skills::skill_group;

    /// The GPUI suites' IsolatedDataDir pattern (ss-gpui test_support.rs):
    /// keep test writes off the real `~/.skillstar` by re-rooting
    /// `SKILLSTAR_DATA_DIR` for the duration. The guard borrows the shared
    /// TEST_ENV_LOCK (facades/mod.rs) so every facade suite that flips the
    /// variable serializes against each other — module-local locks cannot,
    /// and the racing threads abort the test binary.
    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let previous = [
                ("HOME", "home"),
                ("USERPROFILE", "home"),
                ("SKILLSTAR_TOOL_SYNC_HOME", "home"),
                ("SKILLSTAR_DATA_DIR", "data"),
            ]
            .into_iter()
            .map(|(key, sub)| {
                let saved = std::env::var_os(key);
                let dir = root.join(sub);
                std::fs::create_dir_all(&dir).unwrap();
                unsafe { std::env::set_var(key, dir) };
                (key, saved)
            })
            .collect();
            ss_skills::deployment::invalidate_profile_cache();
            Self { _lock: lock, previous, root }
        }
    }

    impl Drop for IsolatedDataDir {
        fn drop(&mut self) {
            unsafe {
                for (key, saved) in self.previous.drain(..) {
                    match saved {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
            ss_skills::deployment::invalidate_profile_cache();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn snapshot_lists_decks_installed_and_targetable_agents() {
        let _iso = IsolatedDataDir::new();
        skill_group::create_group(
            "Rust Starter".into(),
            "Compiler tooling".into(),
            "🦀".into(),
            vec!["cargo-doc".into()],
            Default::default(),
        )
        .unwrap();

        let snapshot = super::snapshot_body().expect("snapshot over an isolated data dir");
        let decks = snapshot["decks"].as_array().expect("decks is an array");
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0]["name"].as_str(), Some("Rust Starter"));
        assert_eq!(
            decks[0]["skills"].as_array().map(Vec::len),
            Some(1),
            "member skill names ride along as strings"
        );
        // Isolated dir has no agents.json and no installed skills: both
        // arrays exist and are empty (a fresh install's shape).
        assert_eq!(
            snapshot["installedSkills"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(snapshot["agents"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn duplicate_then_delete_round_trip_the_deck_list() {
        let _iso = IsolatedDataDir::new();
        let group = skill_group::create_group(
            "Frontend".into(),
            String::new(),
            "🎨".into(),
            vec!["react-hooks".into()],
            Default::default(),
        )
        .unwrap();

        let copy_id = skill_group::duplicate_group(&group.id)
            .map(|g| super::str(&g.id))
            .map_err(|err| format!("{err:#}"))
            .expect("duplicate in the isolated store");
        let copy_id = copy_id.as_str().expect("duplicate returns the new id").to_owned();
        assert_ne!(copy_id, group.id);

        let names: Vec<String> = skill_group::list_groups()
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|n| n.starts_with("Frontend (Copy")));

        skill_group::delete_group(&copy_id).expect("delete the copy");
        assert_eq!(skill_group::list_groups().len(), 1);
    }

    #[test]
    fn missing_deck_jobs_surface_the_domain_error() {
        let _iso = IsolatedDataDir::new();
        let err = skill_group::duplicate_group("no-such-deck")
            .map_err(|err| format!("{err:#}"))
            .expect_err("duplicating a missing deck fails");
        assert!(err.contains("no-such-deck"), "error names the deck: {err}");

        let err = skill_group::delete_group("no-such-deck")
            .map_err(|err| format!("{err:#}"))
            .expect_err("deleting a missing deck fails");
        assert!(err.contains("no-such-deck"), "error names the deck: {err}");
    }

    #[test]
    fn toggle_reports_structured_link_outcome() {
        let _iso = IsolatedDataDir::new();
        let group = skill_group::create_group(
            "Empty Deck".into(),
            String::new(),
            "📦".into(),
            vec![],
            Default::default(),
        )
        .unwrap();
        // No member skills and no agent profile: the domain toggles nothing,
        // the report stays empty, and the JSON shape is what the page reads.
        let report = super::link_job_body(|| {
            ss_skills::workflows::skill_group_links::set_deck_agent(&group.id, "claude", true)
        })
        .expect("toggle on an empty deck is not an error");
        assert_eq!(report["applied"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["skipped"].as_array().map(Vec::len), Some(0));
        assert_eq!(report["failed"].as_array().map(Vec::len), Some(0));

        // A missing deck is a domain error, surfaced as the notice string.
        let err = super::link_job_body(|| {
            ss_skills::workflows::skill_group_links::set_deck_agent("no-such-deck", "claude", true)
        })
        .expect_err("toggling a missing deck fails");
        assert!(err.contains("no-such-deck"), "error names the deck: {err}");
    }

    #[test]
    fn deck_agent_states_detect_mixed_linked_and_display_name_matches() {
        let profiles = ss_skills::agents::list_profiles();
        let claude = profiles
            .iter()
            .find(|p| p.id == "claude")
            .expect("claude profile");
        let codex = profiles
            .iter()
            .find(|p| p.id == "codex")
            .expect("codex profile");
        let cursor = profiles
            .iter()
            .find(|p| p.id == "cursor")
            .expect("cursor profile");
        let targetable = vec![claude, codex, cursor];

        let group = skill_group::SkillGroup {
            id: "d1".into(),
            name: "Deck".into(),
            description: String::new(),
            icon: "📦".into(),
            skills: vec!["Alpha".into(), "Beta".into()],
            skill_sources: Default::default(),
            default_agent: String::new(),
            agent_links: Some(vec!["Claude Code".into(), "codex".into()]),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let mut installed_links = std::collections::HashMap::new();
        // cursor is linked on both members, but NOT declared in deck.agent_links -> stays "none".
        installed_links.insert(
            "alpha".to_owned(),
            vec!["Claude Code".to_owned(), "codex".to_owned(), "cursor".to_owned()],
        );
        installed_links.insert(
            "beta".to_owned(),
            vec!["codex".to_owned(), "cursor".to_owned()],
        );

        let (members, states) =
            super::compute_deck_agent_states(&group, &installed_links, &targetable);
        assert_eq!(
            members["Alpha"].as_array().map(Vec::len),
            Some(3),
            "installed member links recorded"
        );
        assert_eq!(states["claude"].as_str(), Some("mixed"));
        assert_eq!(states["codex"].as_str(), Some("linked"));
        assert_eq!(states["cursor"].as_str(), Some("none"));
    }
}
