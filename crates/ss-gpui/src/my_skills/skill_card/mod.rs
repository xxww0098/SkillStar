//! skill-card for the skills page.
//!
//! The shared box lives in `crate::skill_card`. This module owns the body,
//! the agent rail, and the commands a click asks for. Market-card and
//! group-card do not import it.

mod avatar;
mod render;

pub(super) use avatar::prefetch_skill_avatar;
pub(super) use render::{SkillCardEmit, render_skill_card};
#[cfg(test)]
pub(super) use render::{reset_skill_card_hover, skill_card_is_hovered};

use std::collections::HashSet;

use ss_core::types::skill::Skill;
use ss_skills::agents::AgentProfile;
use ss_skills::deployment::{SKIP_CANONICAL_ROOT_AGENT, ToggleSkillOutcome};

use crate::i18n::{t, tf};
use crate::skill_card::AgentRailSlot;

/// Inputs for one card. `library` hides rank, stars, and the Install label
/// the way the React card does for My Skills.
///
/// The card fills whatever box its container gives it. Grid surfaces hand it a
/// track from [`card_rows`]; a list column hands it the whole pane.
#[derive(Clone)]
pub struct SkillCardProps {
    pub scope: &'static str,
    pub skill: Skill,
    pub agents: Vec<AgentRailSlot>,
    pub selected: bool,
    pub highlighted: bool,
    pub updating: bool,
    pub installing: bool,
    pub selectable: bool,
    pub library: bool,
    /// Description translation remembered for this skill by the drawer's
    /// button. `None` follows the Settings switch; a translated card keeps
    /// its choice after the drawer moves to another skill.
    pub translate_override: Option<bool>,
}

/// UI event. Domain work is [`SkillCardEvent::command`].
#[derive(Clone, Debug)]
pub enum SkillCardEvent {
    Open,
    Select,
    Update,
    Install {
        url: String,
        agent_id: Option<String>,
    },
    ToggleAgent {
        agent_id: String,
        enable: bool,
    },
    OpenFolder,
    OpenLink(String),
}

/// Blocking domain call behind a card click. Hosts run it off the UI thread.
#[derive(Clone, Debug)]
pub enum SkillCardCommand {
    Update {
        name: String,
    },
    Install {
        url: String,
        name: String,
        agent_id: Option<String>,
    },
    ToggleAgent {
        name: String,
        agent_id: String,
        enable: bool,
    },
    OpenFolder {
        name: String,
    },
}

impl SkillCardEvent {
    /// Same branch as the React carousel: a linked icon unlinks, a git source
    /// installs onto that agent, anything else toggles the link on.
    pub fn carousel(linked: bool, git_url: &str, agent_id: &str) -> Self {
        if linked {
            Self::ToggleAgent {
                agent_id: agent_id.to_string(),
                enable: false,
            }
        } else if !git_url.is_empty() {
            Self::Install {
                url: git_url.to_string(),
                agent_id: Some(agent_id.to_string()),
            }
        } else {
            Self::ToggleAgent {
                agent_id: agent_id.to_string(),
                enable: true,
            }
        }
    }

    pub fn command(&self, skill_name: &str) -> Option<SkillCardCommand> {
        match self {
            Self::Update => Some(SkillCardCommand::Update {
                name: skill_name.to_string(),
            }),
            Self::Install { url, agent_id } => Some(SkillCardCommand::Install {
                url: url.clone(),
                name: skill_name.to_string(),
                agent_id: agent_id.clone(),
            }),
            Self::ToggleAgent { agent_id, enable } => Some(SkillCardCommand::ToggleAgent {
                name: skill_name.to_string(),
                agent_id: agent_id.clone(),
                enable: *enable,
            }),
            Self::OpenFolder => Some(SkillCardCommand::OpenFolder {
                name: skill_name.to_string(),
            }),
            Self::Open | Self::Select | Self::OpenLink(_) => None,
        }
    }
}

/// Agent-specific clicks on a skill already in the hub only link it.
/// A missing skill, or an install with no agent, still fetches the source.
pub(crate) fn fetches_before_link(in_hub: bool, agent_specific: bool) -> bool {
    !(in_hub && agent_specific)
}

/// Enabled agents with a global skills directory, in profile order.
/// `pending` keys are `{skill}::{agent id}`. A profile disabled in Settings
/// stays off the rail even while a skill still links it — matching the React
/// `selectTargetableAgentProfiles` branch this card mirrors.
pub fn skill_card_agents(
    skill: &Skill,
    profiles: &[AgentProfile],
    pending: &HashSet<String>,
) -> Vec<AgentRailSlot> {
    let links = skill.agent_links.as_deref().unwrap_or(&[]);
    crate::skill_card::targetable_agent_profiles(profiles)
        .map(|profile| AgentRailSlot {
            id: profile.id.clone(),
            linked: skill.installed && links.iter().any(|link| link == &profile.display_name),
            pending: pending.contains(&format!("{}::{}", skill.name, profile.id)),
        })
        .collect()
}

pub fn run_skill_card_command(command: SkillCardCommand) -> anyhow::Result<()> {
    match command {
        SkillCardCommand::Update { name } => {
            ss_skills::git_skill::GitSkillFacade::from_file_store()
                .update_skill(&name)
                .map(|_| ())
                .map_err(|err| anyhow::anyhow!("{err:#}"))
        }
        SkillCardCommand::Install {
            url,
            name,
            agent_id,
        } => {
            // A skill already in the hub only needs a symlink. Re-fetching the
            // git source here is what made a carousel click wait on the network.
            let in_hub = ss_skills::installer::canonical_skill_name(&name).is_ok_and(|folder| {
                ss_core::infra::paths::hub_skills_dir()
                    .join(folder)
                    .is_dir()
            });
            if !fetches_before_link(in_hub, agent_id.is_some()) {
                let id = agent_id.unwrap_or_default();
                return ss_skills::workflows::global_deploy::deploy_to_selected_global_agents(
                    std::slice::from_ref(&name),
                    std::slice::from_ref(&id),
                )
                .map(|_| ())
                .map_err(anyhow::Error::msg);
            }
            let facade = ss_skills::git_skill::GitSkillFacade::from_file_store();
            let skill = match agent_id.as_deref() {
                Some(id) => facade
                    .install_skill_for_agent(url, Some(name), id)
                    .map_err(|err| anyhow::anyhow!("{err:#}"))?,
                None => facade
                    .install_skill(url, Some(name))
                    .map_err(anyhow::Error::msg)?,
            };
            if let Some(id) = agent_id {
                ss_skills::workflows::global_deploy::deploy_to_selected_global_agents(
                    std::slice::from_ref(&skill.name),
                    &[id],
                )
                .map(|_| ())
                .map_err(anyhow::Error::msg)?;
            }
            Ok(())
        }
        SkillCardCommand::ToggleAgent {
            name,
            agent_id,
            enable,
        } => {
            // Callers run this on the blocking pool (`dispatch_card_command`),
            // never inside the click callback. A zero wait surfaces
            // OperationInProgress instead of sitting on another install.
            let _guard = ss_skills::skill_update::try_acquire_update_transaction_lock(
                std::time::Duration::ZERO,
            )?;
            match ss_skills::deployment::toggle_skill_for_agent(&name, &agent_id, enable)? {
                ToggleSkillOutcome::Applied => Ok(()),
                ToggleSkillOutcome::Skipped { code, path, .. } => {
                    Err(anyhow::anyhow!(toggle_skip_message(&name, &code, &path)))
                }
            }
        }
        SkillCardCommand::OpenFolder { name } => {
            ss_skills::content::open_skill_folder(&name).map_err(|err| anyhow::anyhow!("{err:#}"))
        }
    }
}

/// A toggle that left the Agent untouched is reported, not shown as done.
fn toggle_skip_message(name: &str, code: &str, path: &str) -> String {
    if code == SKIP_CANONICAL_ROOT_AGENT {
        return t("skillToggle.servedByCanonicalRoot").to_string();
    }
    tf(
        "skillToggle.skipUnmanagedDirItem",
        &[("name", name), ("path", path)],
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        AgentProfile, Skill, SkillCardCommand, SkillCardEvent, run_skill_card_command,
        skill_card_agents,
    };
    use std::collections::HashSet;

    #[test]
    fn carousel_unlink_install_or_link() {
        match SkillCardEvent::carousel(true, "https://github.com/a/b", "cursor") {
            SkillCardEvent::ToggleAgent { enable: false, .. } => {}
            other => panic!("unlink, got {other:?}"),
        }
        match SkillCardEvent::carousel(false, "https://github.com/a/b", "cursor") {
            SkillCardEvent::Install {
                agent_id: Some(id), ..
            } => assert_eq!(id, "cursor"),
            other => panic!("install, got {other:?}"),
        }
        match SkillCardEvent::carousel(false, "", "cursor") {
            SkillCardEvent::ToggleAgent { enable: true, .. } => {}
            other => panic!("link, got {other:?}"),
        }
    }

    #[test]
    fn installed_agent_click_skips_fetch() {
        assert!(!super::fetches_before_link(true, true));
        assert!(super::fetches_before_link(false, true));
        assert!(super::fetches_before_link(true, false));
    }

    fn profile(id: &str, enabled: bool) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            display_name: id.to_string(),
            icon: String::new(),
            global_skills_dir: std::path::PathBuf::from("/tmp/agent-skills"),
            project_skills_rel: String::new(),
            installed: enabled,
            enabled,
            synced_count: 0,
        }
    }

    #[test]
    fn rail_skips_disabled_profiles_even_when_linked() {
        let mut skill = Skill::from_skills_sh(
            "demo".into(),
            "desc".into(),
            0,
            "author".into(),
            "https://github.com/a/b".into(),
        );
        skill.installed = true;
        skill.agent_links = Some(vec!["off".into()]);

        let agents = skill_card_agents(
            &skill,
            &[profile("on", true), profile("off", false)],
            &HashSet::new(),
        );

        assert_eq!(
            agents.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            vec!["on"],
            "a disabled profile must not occupy a carousel slot"
        );
    }

    #[test]
    fn targetable_profiles_are_the_carousel_set() {
        // The filter itself lives in `crate::skill_card::agent_rail`; this
        // suite keeps the cards-side behavior: a disabled profile never
        // occupies a slot.
        let mut no_global = profile("no-global", true);
        no_global.global_skills_dir = std::path::PathBuf::new();
        let profiles = [profile("on", true), profile("off", false), no_global];

        let mut skill = Skill::from_skills_sh(
            "demo".into(),
            "desc".into(),
            0,
            "author".into(),
            "https://github.com/a/b".into(),
        );
        skill.installed = true;

        let ids: Vec<_> = skill_card_agents(&skill, &profiles, &HashSet::new())
            .into_iter()
            .map(|slot| slot.id)
            .collect();

        assert_eq!(ids, vec!["on".to_string()]);
    }

    /// The toggle path must not block when another Skill write holds the
    /// transaction. The click handler never calls this itself; the command
    /// runs on the blocking pool and reports OperationInProgress.
    #[test]
    fn toggle_reports_operation_in_progress_without_waiting() {
        let _env = CardEnv::new();
        let _held = ss_skills::skill_update::acquire_update_transaction_lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = run_skill_card_command(SkillCardCommand::ToggleAgent {
                name: "demo".into(),
                agent_id: "claude".into(),
                enable: true,
            });
            let _ = tx.send(result.map_err(|err| err.to_string()));
        });
        let err = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("toggle waited on the skill transaction")
            .expect_err("a busy transaction must surface to the user");
        assert!(err.contains("Another SkillStar operation"), "{err}");
    }

    struct CardEnv {
        /// Same lock as the drawer/reader translation suites: `CardEnv`
        /// redirects the data root, so it must not race another suite's
        /// `SKILLSTAR_DATA_DIR` read or restore.
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
        root: std::path::PathBuf,
    }

    impl CardEnv {
        fn new() -> Self {
            let lock = super::super::test_support::data_dir_lock();
            let root = std::env::temp_dir().join(format!(
                "skillstar-card-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let home = root.join("home");
            std::fs::create_dir_all(&home).unwrap();
            let assignments = [
                ("HOME", Some(home.clone())),
                ("USERPROFILE", Some(home)),
                ("SKILLSTAR_DATA_DIR", Some(root.join("data"))),
                ("SKILLSTAR_HUB_DIR", Some(root.join("hub"))),
                ("SKILLSTAR_TOOL_SYNC_HOME", Some(root.join("tool-home"))),
                ("XDG_STATE_HOME", None),
                ("CLAUDE_CONFIG_DIR", None),
            ];
            let previous = assignments
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in assignments {
                    match value {
                        Some(path) => std::env::set_var(key, path),
                        None => std::env::remove_var(key),
                    }
                }
            }
            Self {
                _lock: lock,
                previous,
                root,
            }
        }
    }

    impl Drop for CardEnv {
        fn drop(&mut self) {
            unsafe {
                for (key, previous) in self.previous.drain(..).rev() {
                    match previous {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
