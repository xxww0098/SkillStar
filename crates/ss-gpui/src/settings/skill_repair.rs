//! Storage health preview and repair. The plan only changes entries SkillStar
//! can prove it owns; a user's own Agent folder is reported and left in place.

use gpui_kit::*;

use super::SettingsPage;
use crate::i18n::{t, tf};
use crate::spawn_domain;

impl SettingsPage {
    pub(super) fn preview_skill_repair(&mut self, cx: &mut Context<Self>) {
        if self.storage_busy_repair() {
            return;
        }
        self.previewing = true;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            ss_app::storage_maintenance::preview_skill_repair(),
            |this, cx, result| {
                this.previewing = false;
                this.storage_status = Some(match result {
                    Ok(preview) => {
                        let mut details = preview.steps.join("\n");
                        if !preview.untouched.is_empty() {
                            if !details.is_empty() {
                                details.push_str("\n");
                            }
                            details.push_str(&preview.untouched.join("\n"));
                        }
                        tf(
                            "settings.repairPreviewBody",
                            &[
                                ("issues", &preview.issues.to_string()),
                                ("steps", &preview.steps.len().to_string()),
                                ("untouched", &preview.untouched.len().to_string()),
                                ("details", &details),
                            ],
                        )
                        .to_string()
                    }
                    Err(error) => {
                        tf("settings.repairFailed", &[("error", &error.to_string())]).to_string()
                    }
                });
                this.load_storage(cx);
            },
        );
        cx.notify();
    }

    pub(super) fn repair_skills(&mut self, cx: &mut Context<Self>) {
        if self.storage_busy_repair() {
            return;
        }
        self.repairing = true;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            ss_app::storage_maintenance::apply_skill_repair(),
            |this, cx, result| {
                this.repairing = false;
                this.storage_status = Some(match result {
                    Ok(report) if !report.failed.is_empty() => tf(
                        "settings.repairPartial",
                        &[
                            ("count", &report.applied.to_string()),
                            ("details", &report.failed.join("\n")),
                        ],
                    )
                    .to_string(),
                    Ok(report) if report.applied == 0 => t("settings.repairNone").to_string(),
                    Ok(report) => tf(
                        "settings.repairDone",
                        &[("count", &report.applied.to_string())],
                    )
                    .to_string(),
                    Err(error) => {
                        tf("settings.repairFailed", &[("error", &error.to_string())]).to_string()
                    }
                });
                this.load_storage(cx);
            },
        );
        cx.notify();
    }

    pub(super) fn preview_agent_intake(&mut self, cx: &mut Context<Self>) {
        if self.storage_busy_repair() {
            return;
        }
        self.previewing_intake = true;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            ss_app::storage_maintenance::preview_agent_intake(),
            |this, cx, result| {
                this.previewing_intake = false;
                this.storage_status = Some(match result {
                    Ok(preview) => {
                        let mut details = preview.steps.join("\n");
                        if !preview.reported.is_empty() {
                            if !details.is_empty() {
                                details.push('\n');
                            }
                            details.push_str(&preview.reported.join("\n"));
                        }
                        tf(
                            "settings.intakePreviewBody",
                            &[
                                ("steps", &preview.would_apply.to_string()),
                                ("reported", &preview.reported.len().to_string()),
                                ("details", &details),
                            ],
                        )
                        .to_string()
                    }
                    Err(error) => {
                        tf("settings.intakeFailed", &[("error", &error.to_string())]).to_string()
                    }
                });
                this.load_storage(cx);
            },
        );
        cx.notify();
    }

    pub(super) fn adopt_agent_skills(&mut self, cx: &mut Context<Self>) {
        if self.storage_busy_repair() {
            return;
        }
        self.adopting = true;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            ss_app::storage_maintenance::apply_agent_intake(),
            |this, cx, result| {
                this.adopting = false;
                this.storage_status = Some(match result {
                    Ok(report) if !report.failed.is_empty() => tf(
                        "settings.intakePartial",
                        &[
                            ("count", &report.applied.to_string()),
                            ("details", &report.failed.join("\n")),
                        ],
                    )
                    .to_string(),
                    Ok(report) if report.applied == 0 => t("settings.intakeNone").to_string(),
                    Ok(report) => tf(
                        "settings.intakeDone",
                        &[("count", &report.applied.to_string())],
                    )
                    .to_string(),
                    Err(error) => {
                        tf("settings.intakeFailed", &[("error", &error.to_string())]).to_string()
                    }
                });
                this.load_storage(cx);
            },
        );
        cx.notify();
    }

    pub(super) fn storage_busy_repair(&self) -> bool {
        self.repairing || self.previewing || self.adopting || self.previewing_intake
    }
}
