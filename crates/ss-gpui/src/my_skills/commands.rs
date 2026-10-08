//! Domain commands for the page: update, uninstall, batch selection, and
//! agent links. Every one of them talks to `ss_skills` and reports back
//! through page state, so the render tree never mutates domain data itself.

use gpui_kit::*;
use ss_core::types::skill::Skill;

use super::skill_card::{
    SkillCardCommand, SkillCardEvent, SkillCardProps, run_skill_card_command, skill_card_agents,
};
use super::{AgentLinkSnap, MySkillsPage};
use crate::spawn_domain;

impl MySkillsPage {
    /// Update a single skill.
    pub fn update_skill(&mut self, name: &str, cx: &mut Context<Self>) {
        self.dispatch_card_command(
            SkillCardCommand::Update {
                name: name.to_string(),
            },
            cx,
        );
    }

    /// Uninstall a skill.
    pub fn uninstall_skill(&mut self, name: &str, cx: &mut Context<Self>) {
        let name = name.to_string();
        self.busy = Some(name.clone());
        self.selected_batch.remove(&name);
        if self.selected_skill.as_deref() == Some(&name) {
            self.select_detail(None);
        }

        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    ss_skills::skill_install::uninstall_skill(&name)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r)
            },
            |this, cx, res| {
                this.busy = None;
                if let Err(err) = res {
                    this.error = Some(
                        crate::i18n::tf("mySkills.uninstallFailedReason", &[("err", &err)])
                            .to_string(),
                    );
                }
                this.refresh(cx);
            },
        );
    }

    /// Batch uninstall all selected skills.
    pub fn batch_uninstall_selected(&mut self, cx: &mut Context<Self>) {
        let names: Vec<String> = self.selected_batch.iter().cloned().collect();
        if names.is_empty() {
            return;
        }

        self.busy = Some("batch_uninstall".to_string());
        self.selected_batch.clear();
        self.select_detail(None);

        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    for name in names {
                        let _ = ss_skills::skill_install::uninstall_skill(&name);
                    }
                })
                .await
                .map_err(|e| e.to_string())
            },
            |this, cx, res| {
                this.busy = None;
                if let Err(err) = res {
                    this.error = Some(
                        crate::i18n::tf("mySkills.batchUninstallFailedReason", &[("err", &err)])
                            .to_string(),
                    );
                }
                this.refresh(cx);
            },
        );
    }

    /// Toggle deployment of a skill for an agent profile.
    pub fn toggle_skill_agent(
        &mut self,
        skill_name: &str,
        agent_id: &str,
        enable: bool,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_card_command(
            SkillCardCommand::ToggleAgent {
                name: skill_name.to_string(),
                agent_id: agent_id.to_string(),
                enable,
            },
            cx,
        );
    }

    pub(super) fn on_skill_card(
        &mut self,
        name: &str,
        event: SkillCardEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SkillCardEvent::Open => {
                self.select_detail(if self.selected_skill.as_deref() == Some(name) {
                    None
                } else {
                    Some(name.to_string())
                });
                self.revise(cx);
            }
            SkillCardEvent::Select => {
                if !self.selected_batch.insert(name.to_string()) {
                    self.selected_batch.remove(name);
                }
                self.revise(cx);
            }
            SkillCardEvent::OpenLink(_) => {}
            other => {
                if let Some(command) = other.command(name) {
                    self.dispatch_card_command(command, cx);
                }
            }
        }
    }

    /// One domain path for the card and the detail drawer.
    fn dispatch_card_command(&mut self, command: SkillCardCommand, cx: &mut Context<Self>) {
        let pending_key = match &command {
            SkillCardCommand::ToggleAgent { name, agent_id, .. } => {
                Some(format!("{name}::{agent_id}"))
            }
            SkillCardCommand::Install {
                name,
                agent_id: Some(agent_id),
                ..
            } => Some(format!("{name}::{agent_id}")),
            _ => None,
        };
        if let Some(key) = &pending_key {
            if !self.pending_agents.insert(key.clone()) {
                return;
            }
        }
        let installed = matches!(
            &command,
            SkillCardCommand::Install {
                name,
                agent_id: Some(_),
                ..
            } if self.skills.iter().any(|skill| skill.name == *name && skill.installed)
        );
        let snap = match &command {
            SkillCardCommand::ToggleAgent {
                name,
                agent_id,
                enable,
            } => self.flip_agent_link(name, agent_id, *enable),
            SkillCardCommand::Install {
                name,
                agent_id: Some(agent_id),
                ..
            } if installed => self.flip_agent_link(name, agent_id, true),
            _ => None,
        };
        let busy_name = match &command {
            SkillCardCommand::Update { name } => Some(name.clone()),
            SkillCardCommand::Install {
                name,
                agent_id: None,
                ..
            } => Some(name.clone()),
            _ => None,
        };
        if let Some(name) = busy_name {
            self.busy = Some(name);
        }
        let fail = match &command {
            SkillCardCommand::Update { .. } => crate::i18n::t("mySkills.updateFailed").to_string(),
            SkillCardCommand::Install { .. } => {
                crate::i18n::t("mySkills.installFailed").to_string()
            }
            SkillCardCommand::ToggleAgent { .. } | SkillCardCommand::OpenFolder { .. } => {
                crate::i18n::t("common.failed").to_string()
            }
        };
        let clear_busy = matches!(
            &command,
            SkillCardCommand::Update { .. } | SkillCardCommand::Install { agent_id: None, .. }
        );
        let pending_done = pending_key;
        let view = cx.entity();
        self.revise(cx);
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || run_skill_card_command(command))
                    .await
                    .map_err(|err| err.to_string())
                    .and_then(|result| result.map_err(|err| format!("{err:#}")))
            },
            move |this, cx, res| {
                if let Some(key) = pending_done {
                    this.pending_agents.remove(&key);
                }
                if clear_busy {
                    this.busy = None;
                }
                if let Err(err) = res {
                    if let Some(snap) = snap {
                        this.restore_agent_link(snap);
                    }
                    this.error = Some(format!("{fail}: {err}"));
                }
                this.refresh(cx);
            },
        );
    }

    /// Paint the icon immediately. `display_name` is what the rail compares.
    fn flip_agent_link(
        &mut self,
        name: &str,
        agent_id: &str,
        enable: bool,
    ) -> Option<AgentLinkSnap> {
        let display_name = self
            .profiles
            .iter()
            .find(|profile| profile.id == agent_id)?
            .display_name
            .clone();
        let skill = self.skills.iter_mut().find(|skill| skill.name == name)?;
        let links = skill.agent_links.get_or_insert_with(Vec::new);
        let was_linked = links.iter().any(|link| link == &display_name);
        if enable && !was_linked {
            links.push(display_name.clone());
        } else if !enable {
            links.retain(|link| link != &display_name);
        }
        Some(AgentLinkSnap {
            name: name.to_string(),
            display_name,
            was_linked,
        })
    }

    /// Link or unlink one skill for every listed agent. The drawer master
    /// switch sends only the agents whose current link differs from `enable`.
    pub(super) fn set_skill_agent_links(
        &mut self,
        skill_name: &str,
        agent_ids: &[String],
        enable: bool,
        cx: &mut Context<Self>,
    ) {
        if agent_ids.is_empty() {
            return;
        }
        let prefix = format!("{skill_name}::");
        if self
            .pending_agents
            .iter()
            .any(|key| key.starts_with(&prefix))
        {
            return;
        }
        let keys: Vec<String> = agent_ids.iter().map(|id| format!("{prefix}{id}")).collect();
        let mut snaps = Vec::new();
        for id in agent_ids {
            if let Some(snap) = self.flip_agent_link(skill_name, id, enable) {
                snaps.push(snap);
            }
        }
        for key in &keys {
            self.pending_agents.insert(key.clone());
        }
        let name = skill_name.to_string();
        let agent_ids = agent_ids.to_vec();
        let pending_keys = keys;
        let view = cx.entity();
        self.revise(cx);
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || toggle_listed_agents(&name, &agent_ids, enable))
                    .await
                    .map_err(|err| err.to_string())
                    .and_then(|result| result)
            },
            move |this, cx, res| {
                for key in &pending_keys {
                    this.pending_agents.remove(key);
                }
                match res {
                    Ok(Some(notice)) => {
                        crate::notify::toast(crate::notify::Notice::warning(notice), cx);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        for snap in snaps {
                            this.restore_agent_link(snap);
                        }
                        crate::notify::toast(
                            crate::notify::Notice::warning(format!(
                                "{}: {err}",
                                crate::i18n::t("common.failed")
                            )),
                            cx,
                        );
                    }
                }
                this.refresh(cx);
            },
        );
    }

    fn restore_agent_link(&mut self, snap: AgentLinkSnap) {
        let Some(skill) = self.skills.iter_mut().find(|skill| skill.name == snap.name) else {
            return;
        };
        let links = skill.agent_links.get_or_insert_with(Vec::new);
        links.retain(|link| link != &snap.display_name);
        if snap.was_linked {
            links.push(snap.display_name);
        }
    }

    pub(super) fn skill_card_props(&self, skill: &Skill) -> SkillCardProps {
        SkillCardProps {
            scope: "my-skills",
            skill: skill.clone(),
            agents: skill_card_agents(skill, &self.profiles, &self.pending_agents),
            selected: self.selected_batch.contains(&skill.name),
            highlighted: self.selected_skill.as_deref() == Some(skill.name.as_str()),
            updating: self.skill_update_in_flight(skill),
            installing: false,
            selectable: true,
            library: true,
            translate_override: self.description_choices.get(&skill.name).copied(),
        }
    }

    /// Batch link selected skills to an agent.
    pub fn batch_link_to_agent(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        let names: Vec<String> = self.selected_batch.iter().cloned().collect();
        let agent_id = agent_id.to_string();

        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    ss_skills::deployment::batch_link_skills_to_agent(&names, &agent_id)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()))
            },
            |this, cx, res| {
                if let Err(err) = res {
                    this.error = Some(
                        crate::i18n::tf("mySkills.batchLinkFailedReason", &[("err", &err)])
                            .to_string(),
                    );
                }
                this.selected_batch.clear();
                this.refresh(cx);
            },
        );
    }

    /// Batch unlink selected skills from all agents.
    pub fn batch_unlink_selected(&mut self, cx: &mut Context<Self>) {
        let names: Vec<String> = self.selected_batch.iter().cloned().collect();

        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    for name in names {
                        let _ = ss_skills::deployment::remove_skill_from_all_agents(&name);
                    }
                })
                .await
                .map_err(|e| e.to_string())
            },
            |this, cx, res| {
                if let Err(err) = res {
                    this.error = Some(
                        crate::i18n::tf("mySkills.batchUnlinkFailedReason", &[("err", &err)])
                            .to_string(),
                    );
                }
                this.selected_batch.clear();
                this.refresh(cx);
            },
        );
    }
}

/// `Some(true)` links every agent that is still off. `Some(false)` unlinks
/// every listed agent, and only when they are all already linked. `None`
/// when the list is empty.
pub(super) fn master_switch_enable(agents: &[(String, bool)]) -> Option<bool> {
    if agents.is_empty() {
        return None;
    }
    Some(agents.iter().any(|(_, linked)| !linked))
}

/// Agents whose link state is not `enable` yet. Already-correct rows stay put.
pub(super) fn master_switch_agent_ids(agents: &[(String, bool)], enable: bool) -> Vec<String> {
    agents
        .iter()
        .filter(|(_, linked)| *linked != enable)
        .map(|(id, _)| id.clone())
        .collect()
}

/// One transaction for the whole list. A canonical-root agent is not a
/// failure: it already sees every installed skill. Name collisions and hard
/// errors come back as a notice; the caller refreshes from disk.
fn toggle_listed_agents(
    name: &str,
    agent_ids: &[String],
    enable: bool,
) -> Result<Option<String>, String> {
    let _guard =
        ss_skills::skill_update::try_acquire_update_transaction_lock(std::time::Duration::ZERO)
            .map_err(|err| format!("{err:#}"))?;

    let total = agent_ids.len();
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut canonical = 0usize;
    for agent_id in agent_ids {
        match ss_skills::deployment::toggle_skill_for_agent(name, agent_id, enable) {
            Ok(ss_skills::deployment::ToggleSkillOutcome::Applied) => {}
            Ok(ss_skills::deployment::ToggleSkillOutcome::Skipped { code, .. })
                if code == ss_skills::deployment::SKIP_CANONICAL_ROOT_AGENT =>
            {
                canonical += 1;
            }
            Ok(ss_skills::deployment::ToggleSkillOutcome::Skipped { .. }) => skipped += 1,
            Err(err) => {
                if failed == 0 {
                    tracing::warn!(
                        target: "sync",
                        skill = %name,
                        agent_id,
                        error = %err,
                        "master agent toggle failed"
                    );
                }
                failed += 1;
            }
        }
    }
    if failed == 0 && skipped == 0 {
        if canonical == total && total > 0 {
            return Ok(Some(
                crate::i18n::t("skillToggle.servedByCanonicalRoot").to_string(),
            ));
        }
        return Ok(None);
    }
    let total = total.to_string();
    let mut parts = Vec::new();
    if failed > 0 {
        parts.push(
            crate::i18n::tf(
                "skillCards.batchTogglePartialFailed",
                &[("failed", &failed.to_string()), ("total", &total)],
            )
            .to_string(),
        );
    }
    if skipped > 0 {
        parts.push(
            crate::i18n::tf(
                "skillCards.batchTogglePartialSkipped",
                &[("skipped", &skipped.to_string()), ("total", &total)],
            )
            .to_string(),
        );
    }
    Ok(Some(parts.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::{master_switch_agent_ids, master_switch_enable};

    fn rows(pairs: &[(&str, bool)]) -> Vec<(String, bool)> {
        pairs
            .iter()
            .map(|(id, on)| ((*id).to_string(), *on))
            .collect()
    }

    #[test]
    fn master_switch_links_the_rest_until_every_agent_is_on() {
        assert_eq!(master_switch_enable(&[]), None);

        let none = rows(&[("cursor", false), ("codex", false)]);
        assert_eq!(master_switch_enable(&none), Some(true));
        assert_eq!(
            master_switch_agent_ids(&none, true),
            vec!["cursor".to_string(), "codex".to_string()]
        );

        let mixed = rows(&[("cursor", true), ("codex", false)]);
        assert_eq!(master_switch_enable(&mixed), Some(true));
        assert_eq!(
            master_switch_agent_ids(&mixed, true),
            vec!["codex".to_string()]
        );

        let all = rows(&[("cursor", true), ("codex", true)]);
        assert_eq!(master_switch_enable(&all), Some(false));
        assert_eq!(
            master_switch_agent_ids(&all, false),
            vec!["cursor".to_string(), "codex".to_string()]
        );
    }
}
