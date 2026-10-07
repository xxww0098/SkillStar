//! Per-directory pause and restore for the skills an enabled Agent already has.
//!
//! React source: `Settings.tsx` `handleToggleAllSkills` and
//! `src/features/settings/lib/agentSkillSync.ts`. The switch does not enable
//! the Agent and does not deploy the Hub. One physical Global skills directory
//! shares its journal, pending flag, and linked names across every profile
//! that resolves to it.
//!
//! `state.rs` owns the cached journal, `rows.rs` the row, `notice.rs` the
//! result copy.

mod notice;
mod rows;
mod state;

use std::collections::HashSet;
use std::path::Path;

use gpui_kit::*;
use ss_skills::agents;
use ss_skills::deployment;
use ss_skills::workflows::agent_managed_skills::{self, AgentManagedSkillsState};

use crate::i18n::{t, tf};
use crate::spawn_domain;

use self::notice::{NoticeKind, cleared_links_notice, notice_from_report, show_notice};
use self::state::AgentNotice;
use self::state::{ManagedJob, ReadPurpose};
pub(crate) use self::state::{ManagedSkillsUi, global_skills_target_key};
use super::SettingsPage;

impl SettingsPage {
    pub(crate) fn preload_managed_skills(&mut self, cx: &mut Context<Self>) {
        let mut seen = HashSet::new();
        let mut jobs = Vec::new();
        for profile in &self.profiles {
            if !profile.enabled || !profile.has_global_skills() {
                continue;
            }
            let key = global_skills_target_key(&profile.global_skills_dir);
            if !seen.insert(key.clone()) {
                continue;
            }
            let (epoch, request) = self.managed.begin(&key);
            jobs.push(ManagedJob {
                key,
                epoch,
                request,
                agent_id: profile.id.clone(),
                purpose: ReadPurpose::Preload,
            });
        }
        self.spawn_managed_reads(jobs, cx);
    }

    pub(crate) fn toggle_agent_enabled(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        match agents::toggle_profile(agent_id) {
            Ok(enabled) => {
                let mut refresh = None;
                if let Some(profile) = self
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id == agent_id)
                {
                    profile.enabled = enabled;
                    if enabled && profile.has_global_skills() {
                        refresh = Some(profile.global_skills_dir.clone());
                    }
                }
                if let Some(path) = refresh {
                    self.refresh_managed(agent_id, &path, cx);
                }
                // KeepAlive rails snapshot profiles at construction. Emit
                // before notify so the carousel SVG is in the next paint,
                // not after an unrelated skills refresh.
                cx.emit(crate::nav::AgentsChanged);
            }
            Err(_) => {
                show_notice(
                    AgentNotice::error(t("settings.toggleFailed")),
                    NoticeKind::Result,
                    agent_id,
                    cx,
                );
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_agent_expand(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        if self.expanded_agent.as_deref() == Some(agent_id) {
            self.expanded_agent = None;
            cx.notify();
            return;
        }
        self.expanded_agent = Some(agent_id.to_string());
        let profile = self
            .profiles
            .iter()
            .find(|profile| profile.id == agent_id)
            .cloned();
        if let Some(profile) = profile.filter(|profile| profile.has_global_skills()) {
            self.refresh_managed(&profile.id, &profile.global_skills_dir, cx);
        } else if !self.linked_skills.contains_key(agent_id) {
            match deployment::list_linked_skills(agent_id) {
                Ok(names) => {
                    self.linked_skills.insert(agent_id.to_string(), names);
                }
                Err(_) => {
                    show_notice(
                        AgentNotice::error(t("settings.listLinkedFailed")),
                        NoticeKind::Result,
                        agent_id,
                        cx,
                    );
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn unlink_linked_skill(
        &mut self,
        agent_id: &str,
        skill: &str,
        cx: &mut Context<Self>,
    ) {
        let global = self
            .profiles
            .iter()
            .find(|profile| profile.id == agent_id)
            .filter(|profile| profile.has_global_skills())
            .map(|profile| global_skills_target_key(&profile.global_skills_dir));
        if let Some(key) = &global {
            if self.managed.pending.contains(key) {
                return;
            }
            self.managed.invalidate(key);
        }
        if deployment::unlink_skill_from_agent(skill, agent_id).is_err() {
            show_notice(
                AgentNotice::error(t("settings.unlinkFailed")),
                NoticeKind::Result,
                agent_id,
                cx,
            );
            if let Some(key) = global {
                self.refresh_managed_key(&key, ReadPurpose::Refresh, cx);
            }
            cx.notify();
            return;
        }
        self.forget_linked_skill(agent_id, skill);
        if let Some(key) = global {
            self.refresh_managed_key(&key, ReadPurpose::Refresh, cx);
        }
        cx.notify();
    }

    /// One-click clear for the expanded agent's linked cards. Sweeps the
    /// physical Global skills directory through `unlink_all_skills_from_agent`,
    /// so every profile sharing that directory loses the same links. Unmanaged
    /// real directories are left in place by the domain layer.
    pub(crate) fn unlink_all_linked_skills(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        let Some(profile) = self
            .profiles
            .iter()
            .find(|profile| profile.id == agent_id)
            .filter(|profile| profile.has_global_skills())
            .cloned()
        else {
            return;
        };
        let key = global_skills_target_key(&profile.global_skills_dir);
        if self.managed.pending.contains(&key) {
            return;
        }
        self.managed.invalidate(&key);
        self.managed.pending.insert(key.clone());
        let id = profile.id.clone();
        let name = profile.display_name.clone();
        let pending_key = key.clone();
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    deployment::unlink_all_skills_from_agent(&id).map_err(|err| err.to_string())
                })
                .await
                .map_err(|err| err.to_string())
                .and_then(|removed| removed)
            },
            move |this, cx, removed| {
                this.managed.pending.remove(&pending_key);
                let notice = match removed {
                    Ok(removed) => {
                        this.forget_all_linked_skills(&pending_key);
                        cleared_links_notice(removed, &name)
                    }
                    Err(_) => AgentNotice::error(t("settings.unlinkAllFromAgentFailed")),
                };
                show_notice(notice, NoticeKind::Result, &pending_key, cx);
                this.refresh_managed_key(&pending_key, ReadPurpose::Refresh, cx);
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(crate) fn toggle_managed_skills(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        let Some(profile) = self
            .profiles
            .iter()
            .find(|profile| profile.id == agent_id)
            .cloned()
        else {
            return;
        };
        if !profile.enabled || !profile.has_global_skills() {
            return;
        }
        let key = global_skills_target_key(&profile.global_skills_dir);
        if self.managed.pending.contains(&key) {
            return;
        }
        self.managed.invalidate(&key);
        self.managed.pending.insert(key.clone());
        let id = profile.id.clone();
        let name = profile.display_name.clone();
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    agent_managed_skills::toggle_agent_managed_skills(&id)
                        .map_err(|err| err.to_string())
                })
                .await
                .map_err(|err| err.to_string())
                .and_then(|report| report)
            },
            move |this, cx, report| {
                this.managed.pending.remove(&key);
                let notice = match report {
                    Ok(report) => {
                        this.apply_managed_state(&key, report.state.clone());
                        notice_from_report(&name, &report)
                    }
                    Err(_) => AgentNotice::error(t("settings.managedSkillsOperationFailed")),
                };
                show_notice(notice, NoticeKind::Result, &key, cx);
                this.refresh_managed_key(&key, ReadPurpose::Refresh, cx);
                cx.notify();
            },
        );
        cx.notify();
    }

    fn refresh_managed(&mut self, agent_id: &str, path: &Path, cx: &mut Context<Self>) {
        let key = global_skills_target_key(path);
        let (epoch, request) = self.managed.begin(&key);
        self.spawn_managed_reads(
            vec![ManagedJob {
                key,
                epoch,
                request,
                agent_id: agent_id.to_string(),
                purpose: ReadPurpose::Refresh,
            }],
            cx,
        );
    }

    fn refresh_managed_key(&mut self, key: &str, purpose: ReadPurpose, cx: &mut Context<Self>) {
        let Some(agent_id) = self
            .profiles
            .iter()
            .find(|profile| {
                profile.has_global_skills()
                    && global_skills_target_key(&profile.global_skills_dir) == key
            })
            .map(|profile| profile.id.clone())
        else {
            return;
        };
        let (epoch, request) = self.managed.begin(key);
        self.spawn_managed_reads(
            vec![ManagedJob {
                key: key.to_string(),
                epoch,
                request,
                agent_id,
                purpose,
            }],
            cx,
        );
    }

    fn spawn_managed_reads(&mut self, jobs: Vec<ManagedJob>, cx: &mut Context<Self>) {
        if jobs.is_empty() {
            return;
        }
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    jobs.into_iter()
                        .map(|job| {
                            let result =
                                agent_managed_skills::get_agent_managed_skills_state(&job.agent_id)
                                    .map_err(|err| err.to_string());
                            (job, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .map_err(|err| err.to_string())
            },
            |this, cx, joined| {
                let Ok(rows) = joined else {
                    return;
                };
                for (job, result) in rows {
                    if !this.managed.accepts(&job.key, job.epoch, job.request) {
                        continue;
                    }
                    match result {
                        Ok(state) => this.apply_managed_state(&job.key, state),
                        Err(err) => this.note_managed_read_error(job.purpose, &job.key, &err, cx),
                    }
                }
                cx.notify();
            },
        );
    }

    fn apply_managed_state(&mut self, key: &str, state: AgentManagedSkillsState) {
        let names = state.active_skill_names.clone();
        let members: Vec<String> = self
            .profiles
            .iter()
            .filter(|profile| {
                profile.has_global_skills()
                    && global_skills_target_key(&profile.global_skills_dir) == key
            })
            .map(|profile| profile.id.clone())
            .collect();
        self.managed.states.insert(key.to_string(), state);
        let count = names.len() as u32;
        for id in members {
            self.linked_skills.insert(id.clone(), names.clone());
            if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
                profile.synced_count = count;
            }
        }
    }

    fn forget_linked_skill(&mut self, agent_id: &str, skill: &str) {
        let key = self
            .profiles
            .iter()
            .find(|profile| profile.id == agent_id && profile.has_global_skills())
            .map(|profile| global_skills_target_key(&profile.global_skills_dir));
        let ids: Vec<String> = if let Some(key) = &key {
            self.profiles
                .iter()
                .filter(|profile| {
                    profile.has_global_skills()
                        && &global_skills_target_key(&profile.global_skills_dir) == key
                })
                .map(|profile| profile.id.clone())
                .collect()
        } else {
            vec![agent_id.to_string()]
        };
        for id in &ids {
            if let Some(list) = self.linked_skills.get_mut(id) {
                list.retain(|name| name != skill);
            }
            if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == *id) {
                profile.synced_count = profile.synced_count.saturating_sub(1);
            }
        }
        if let Some(key) = key {
            if let Some(state) = self.managed.states.get_mut(&key) {
                state.active_skill_names.retain(|name| name != skill);
            }
        }
    }

    /// Immediate feedback after a directory-wide clear: every profile that
    /// resolves to `key` drops its cached link list before the refresh lands.
    fn forget_all_linked_skills(&mut self, key: &str) {
        let ids: Vec<String> = self
            .profiles
            .iter()
            .filter(|profile| {
                profile.has_global_skills()
                    && global_skills_target_key(&profile.global_skills_dir) == key
            })
            .map(|profile| profile.id.clone())
            .collect();
        for id in &ids {
            if let Some(list) = self.linked_skills.get_mut(id) {
                list.clear();
            }
            if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == *id) {
                profile.synced_count = 0;
            }
        }
        if let Some(state) = self.managed.states.get_mut(key) {
            state.active_skill_names.clear();
        }
    }

    /// A failed read never overwrites the outcome the user is looking at:
    /// `NoticeKind::Read` toasts stack beside `NoticeKind::Result` ones, and
    /// repeats for the same directory replace each other instead of piling up.
    fn note_managed_read_error(
        &mut self,
        purpose: ReadPurpose,
        key: &str,
        err: &str,
        cx: &mut Context<Self>,
    ) {
        match purpose {
            ReadPurpose::Preload => show_notice(
                AgentNotice::error(tf("settings.connectionFailed", &[("error", err)])),
                NoticeKind::Read,
                "managed-skills-preload",
                cx,
            ),
            ReadPurpose::Refresh => show_notice(
                AgentNotice::warn(t("settings.managedSkillsRefreshFailed")),
                NoticeKind::Read,
                key,
                cx,
            ),
        }
    }
}

#[cfg(test)]
mod tests;
