//! Update checks and batch updates for the page. The check runs in the
//! background after the list loads (throttled) and on demand from the toolbar;
//! every update outcome is summarized in one toast.

use std::time::{Duration, Instant};

use crate::notify::Notice;
use gpui_kit::*;
use ss_core::types::skill::SkillType;
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::skill_update::SkillUpdateReport;

use super::MySkillsPage;
use crate::spawn_domain;

/// Minimum spacing between two background checks in one session. The check
/// already bounds its own concurrency and honors GitHub's rate-limit reset.
const BACKGROUND_CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);

impl MySkillsPage {
    /// Whether this card's update pill should play the updating ellipsis.
    ///
    /// A single-card update stores the skill name in `busy`. The toolbar and
    /// the selection bar store a batch token instead, and the pill still has
    /// to animate for every skill that click included.
    pub(super) fn skill_update_in_flight(&self, skill: &ss_core::types::skill::Skill) -> bool {
        match self.busy.as_deref() {
            Some("update_all" | "batch_update") => self.updating_batch.contains(&skill.name),
            Some(name) => name == skill.name,
            None => false,
        }
    }

    /// Record the click-time names and mark the page busy. Empty input and a
    /// second click while a mutation is already running do nothing.
    fn arm_batch_update(&mut self, names: &[String], busy: &str) -> bool {
        if names.is_empty() || self.busy.is_some() {
            return false;
        }
        self.updating_batch = names.iter().cloned().collect();
        self.busy = Some(busy.to_string());
        true
    }

    /// Update every Skill the last check marked as updatable.
    pub fn update_all_pending(&mut self, cx: &mut Context<Self>) {
        let names: Vec<String> = self
            .skills
            .iter()
            .filter(|skill| skill.update_available)
            .map(|skill| skill.name.clone())
            .collect();
        self.run_batch_update(names, "update_all", cx);
    }

    /// Update the selected Skills that have an update.
    pub fn batch_update_selected(&mut self, cx: &mut Context<Self>) {
        let names: Vec<String> = self
            .selected_batch
            .iter()
            .filter(|name| {
                self.skills
                    .iter()
                    .any(|skill| &skill.name == *name && skill.update_available)
            })
            .cloned()
            .collect();
        self.run_batch_update(names, "batch_update", cx);
    }

    fn run_batch_update(&mut self, names: Vec<String>, busy: &str, cx: &mut Context<Self>) {
        // One mutation at a time: a second click while busy is ignored.
        if !self.arm_batch_update(&names, busy) {
            return;
        }
        self.revise(cx);
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    GitSkillFacade::from_file_store().update_skills(&names)
                })
                .await
                .map_err(anyhow::Error::from)
            },
            |this, cx, res| {
                this.busy = None;
                this.updating_batch.clear();
                match res {
                    Ok(report) => {
                        if let Some(note) = report_notification(&report) {
                            crate::notify::toast(note, cx);
                        }
                    }
                    Err(err) => crate::notify::toast(
                        Notice::error(format!(
                            "{}: {err:#}",
                            crate::i18n::t("mySkills.updateFailed")
                        )),
                        cx,
                    ),
                }
                this.refresh(cx);
            },
        );
    }

    /// Compare installed Skills with upstream. `manual` reports the outcome;
    /// the background check stays silent and only refreshes the badges.
    pub fn check_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.checking_updates {
            return;
        }
        self.checking_updates = true;
        self.last_update_check = Some(Instant::now());
        self.revise(cx);
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                GitSkillFacade::from_file_store()
                    .refresh_skill_updates()
                    .await
            },
            move |this, cx, res| {
                this.checking_updates = false;
                match res {
                    Ok(states) if manual => {
                        let pending = states.iter().filter(|state| state.update_available).count();
                        let note = if pending == 0 {
                            Notice::success(crate::i18n::t("mySkills.checkUpdatesNone"))
                        } else {
                            Notice::info(crate::i18n::tf(
                                "mySkills.checkUpdatesFound",
                                &[("count", &pending.to_string())],
                            ))
                        };
                        crate::notify::toast(note, cx);
                    }
                    Ok(_) => {}
                    Err(err) if manual => crate::notify::toast(
                        Notice::error(format!(
                            "{}: {err:#}",
                            crate::i18n::t("mySkills.checkUpdatesFailed")
                        )),
                        cx,
                    ),
                    Err(err) => {
                        tracing::debug!(target: "skill_updates", "background update check failed: {err:#}");
                    }
                }
                this.refresh(cx);
            },
        );
    }

    /// Called after every list load: start a silent check when the last one
    /// is old enough and there is something to check.
    pub(super) fn maybe_check_updates(&mut self, cx: &mut Context<Self>) {
        let has_upstream = self
            .skills
            .iter()
            .any(|skill| skill.skill_type != SkillType::Local);
        let due = self
            .last_update_check
            .is_none_or(|at| at.elapsed() >= BACKGROUND_CHECK_INTERVAL);
        if has_upstream && due && self.busy.is_none() {
            self.check_updates(false, cx);
        }
    }
}

/// One toast for a batch update: what was updated, then everything that needs
/// the user (failures, skips, renames, unrefreshed Agent or Project copies).
fn report_notification(report: &SkillUpdateReport) -> Option<Notice> {
    let lines = report_lines(report);
    if lines.is_empty() {
        return None;
    }
    let updated = report.updated.len();
    let failed = report.failed.len();
    let text = lines.join("\n");
    Some(if failed > 0 && updated == 0 {
        Notice::error(text)
    } else if failed == 0 && lines.len() == 1 && updated > 0 {
        Notice::success(text)
    } else {
        Notice::warning(text)
    })
}

fn report_lines(report: &SkillUpdateReport) -> Vec<String> {
    let updated = report.updated.len();
    let failed = report.failed.len();
    let mut lines = Vec::new();
    if failed == 0 {
        if updated > 0 {
            lines.push(
                crate::i18n::tf(
                    "mySkills.batchUpdateSuccess",
                    &[("count", &updated.to_string())],
                )
                .to_string(),
            );
        }
    } else {
        lines.push(
            crate::i18n::tf(
                "mySkills.batchUpdatePartial",
                &[
                    ("success", &updated.to_string()),
                    ("failed", &failed.to_string()),
                ],
            )
            .to_string(),
        );
        lines.extend(
            report
                .failed
                .iter()
                .map(|failure| format!("{}: {}", failure.name, failure.error)),
        );
    }
    if !report.skipped.is_empty() {
        lines.push(
            crate::i18n::tf(
                "mySkills.updateSkippedToast",
                &[("names", &report.skipped.join(", "))],
            )
            .to_string(),
        );
    }
    for change in &report.identity_changed {
        lines.push(
            crate::i18n::tf(
                "mySkills.updateIdentityChanged",
                &[("name", &change.name), ("upstream", &change.upstream_name)],
            )
            .to_string(),
        );
    }
    if !report.channel_managed.is_empty() {
        lines.push(
            crate::i18n::tf(
                "mySkills.batchUpdateChannelManaged",
                &[("count", &report.channel_managed.len().to_string())],
            )
            .to_string(),
        );
    }
    if !report.not_updatable.is_empty() {
        lines.push(
            crate::i18n::tf(
                "mySkills.updateNotUpdatable",
                &[("names", &report.not_updatable.join(", "))],
            )
            .to_string(),
        );
    }
    let link_failures: Vec<String> = report
        .updated
        .iter()
        .flat_map(|result| result.agent_link_failures.iter().cloned())
        .collect();
    if !link_failures.is_empty() {
        lines.push(format!(
            "{} {}",
            crate::i18n::t("mySkills.agentRelinkFailed"),
            link_failures.join(", ")
        ));
    }
    if !report.project_failures.is_empty() {
        lines.push(format!(
            "{} {}",
            crate::i18n::t("mySkills.updateProjectFailed"),
            report.project_failures.join(", ")
        ));
    }
    lines
}

#[cfg(test)]
mod card_motion {
    use gpui_kit::AppContext as _;
    use ss_core::types::skill::Skill;

    use super::super::MySkillsPage;

    fn skill(name: &str, update_available: bool) -> Skill {
        let mut skill =
            Skill::from_skills_sh(name.into(), String::new(), 0, "acme".into(), String::new());
        skill.update_available = update_available;
        skill
    }

    /// The toolbar "更新" and the selection-bar batch both leave `busy` as a
    /// token, not a skill name. Pending cards still have to mount the
    /// ellipsis; that is the only animation on the card's update pill.
    #[gpui_kit::test]
    fn batch_update_mounts_the_card_update_animation(cx: &mut gpui_kit::TestAppContext) {
        crate::init_test(cx);
        let page = cx.new(|cx| MySkillsPage::new(cx));
        cx.update(|cx| {
            page.update(cx, |page, _| {
                let pending = skill("alpha", true);
                let other = skill("beta", true);
                let current = skill("gamma", false);

                assert!(
                    page.arm_batch_update(
                        &[pending.name.clone(), other.name.clone()],
                        "update_all",
                    )
                );
                assert!(
                    page.skill_card_props(&pending).updating,
                    "toolbar update should start the pending card's ellipsis"
                );
                assert!(page.skill_card_props(&other).updating);
                assert!(
                    !page.skill_card_props(&current).updating,
                    "a card outside the click snapshot has no update pill"
                );
                assert!(
                    !page.arm_batch_update(&[pending.name.clone()], "update_all"),
                    "a second click while the batch is running is ignored"
                );

                page.busy = None;
                page.updating_batch.clear();
                page.selected_batch.insert(other.name.clone());
                assert!(page.arm_batch_update(&[pending.name.clone()], "batch_update"));
                page.selected_batch.clear();
                assert!(
                    page.skill_card_props(&pending).updating,
                    "clearing the selection does not stop the in-flight card"
                );
                assert!(!page.skill_card_props(&other).updating);

                page.busy = None;
                page.updating_batch.clear();
                page.busy = Some(pending.name.clone());
                assert!(page.skill_card_props(&pending).updating);
                assert!(!page.skill_card_props(&other).updating);

                page.busy = None;
                assert!(!page.skill_card_props(&pending).updating);
            });
        });
    }
}

#[cfg(test)]
mod tests {
    // Explicit imports: `use super::*` would pull GPUI's `test` macro through
    // the parent `use gpui_kit::*` and shadow Rust's `#[test]`.
    use super::report_lines;
    use ss_core::types::skill::Skill;
    use ss_skills::skill_update::{SkillUpdateFailure, SkillUpdateReport, UpdateResult};

    #[test]
    fn partial_updates_name_every_copy_left_behind() {
        let report = SkillUpdateReport {
            updated: vec![UpdateResult {
                skill: Skill::from_skills_sh(
                    "alpha".into(),
                    String::new(),
                    0,
                    "acme".into(),
                    String::new(),
                ),
                siblings_cleared: Vec::new(),
                agent_link_failures: vec!["codex".into()],
            }],
            failed: vec![SkillUpdateFailure {
                name: "beta".into(),
                error: "network".into(),
            }],
            project_failures: vec!["proj-a: locked".into()],
            not_updatable: vec!["mine".into()],
            ..SkillUpdateReport::default()
        };
        let text = report_lines(&report).join("\n");
        for needle in ["beta: network", "codex", "proj-a: locked", "mine"] {
            assert!(text.contains(needle), "{needle} missing from {text}");
        }
        assert!(report_lines(&SkillUpdateReport::default()).is_empty());
    }
}
