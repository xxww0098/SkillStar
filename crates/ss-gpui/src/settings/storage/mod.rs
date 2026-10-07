//! Storage overview + maintenance. React source:
//! `src/features/settings/sections/StorageSection.tsx`.
//!
//! `rows.rs` owns the card body, `parts.rs` the buttons and path cards.

mod parts;
mod rows;

use gpui_kit::*;
use ss_app::storage_maintenance;

use crate::i18n::{t, tf};
use crate::spawn_domain;
use crate::theme::palette;

use self::parts::path_structure;
use super::{DeleteTarget, SettingsPage, SettingsSection, section_shell};

impl SettingsPage {
    pub(crate) fn load_storage(&mut self, cx: &mut Context<Self>) {
        self.storage_loading = true;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            storage_maintenance::get_storage_overview(),
            |this, cx, result| {
                this.storage_loading = false;
                match result {
                    Ok(overview) => this.storage = Some(overview),
                    Err(error) => {
                        this.storage_status = Some(
                            tf(
                                "settings.connectionFailed",
                                &[("error", &error.to_string())],
                            )
                            .to_string(),
                        );
                    }
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    /// First click arms the row; the second click runs it. `parts.rs` renders
    /// the armed label from `storage_confirm`, and the footer line explains
    /// what the delete does.
    pub(crate) fn arm_delete(&mut self, target: DeleteTarget, cx: &mut Context<Self>) {
        if self.storage_busy.is_some() || self.cleaning {
            return;
        }
        if self.storage_confirm == Some(target) {
            self.storage_action(target, cx);
            return;
        }
        if target == DeleteTarget::Hub {
            self.preview_hub_delete(cx);
            return;
        }
        self.storage_confirm = Some(target);
        self.storage_status = Some(t(confirm_key(target)).to_string());
        cx.notify();
    }

    /// Read the names the hub reset will uninstall, then arm the second click.
    /// The list is the same selection `force_delete_installed_skills` uses.
    fn preview_hub_delete(&mut self, cx: &mut Context<Self>) {
        self.storage_confirm = None;
        self.storage_busy = Some("hub");
        self.storage_status = None;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            storage_maintenance::preview_force_delete_installed_skills(),
            |this, cx, result| {
                this.storage_busy = None;
                match result {
                    Err(_) => {
                        this.storage_status = Some(t("settings.forceDeleteFailed").to_string());
                    }
                    Ok(names) if names.is_empty() => {
                        this.storage_status = Some(t("settings.forceDeleteHubEmpty").to_string());
                    }
                    Ok(names) => {
                        this.storage_confirm = Some(DeleteTarget::Hub);
                        this.storage_status = Some(hub_delete_confirm(&names));
                    }
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    fn storage_action(&mut self, target: DeleteTarget, cx: &mut Context<Self>) {
        self.storage_confirm = None;
        self.storage_busy = Some(match target {
            DeleteTarget::Hub => "hub",
            DeleteTarget::Cache => "cache",
        });
        let entity = cx.entity();
        match target {
            DeleteTarget::Hub => spawn_domain(
                &entity,
                cx,
                storage_maintenance::force_delete_installed_skills(),
                |this, cx, result| {
                    this.storage_busy = None;
                    this.storage_status = Some(match result {
                        Err(_) => t("settings.forceDeleteFailed").to_string(),
                        Ok(report) => hub_delete_status(&report),
                    });
                    this.load_storage(cx);
                    cx.notify();
                },
            ),
            DeleteTarget::Cache => spawn_domain(
                &entity,
                cx,
                storage_maintenance::force_delete_repo_caches(),
                |this, cx, result| {
                    this.storage_busy = None;
                    this.storage_status = Some(match result {
                        Err(_) => t("settings.forceDeleteFailed").to_string(),
                        Ok(0) => t("settings.forceDeleteCacheEmpty").to_string(),
                        Ok(count) => tf(
                            "settings.forceDeleteCacheDone",
                            &[("count", &count.to_string())],
                        )
                        .to_string(),
                    });
                    this.load_storage(cx);
                    cx.notify();
                },
            ),
        }
        cx.notify();
    }

    pub(crate) fn clean_caches(&mut self, cx: &mut Context<Self>) {
        if self.cleaning {
            return;
        }
        self.cleaning = true;
        self.storage_confirm = None;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            storage_maintenance::clear_all_caches(),
            |this, cx, result| {
                this.cleaning = false;
                this.storage_status = Some(match result {
                    Err(_) => t("settings.cacheCleanFailed").to_string(),
                    Ok(cleaned) if cleaned.repos_removed + cleaned.history_cleared == 0 => {
                        t("settings.cacheEmpty").to_string()
                    }
                    Ok(cleaned) => tf(
                        "settings.cacheCleanDone",
                        &[(
                            "count",
                            &(cleaned.repos_removed + cleaned.history_cleared).to_string(),
                        )],
                    )
                    .to_string(),
                });
                this.load_storage(cx);
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(crate) fn render_storage(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let mut column = div().flex().flex_col().w_full();
        column = column.child(self.path_hint(view.clone()));
        if self.path_structure_open {
            if let Some(overview) = &self.storage {
                if !self.storage_loading {
                    column = column.child(path_structure(overview));
                }
            }
        }
        column = column.child(self.storage_card(view));
        if let Some(status) = &self.storage_status {
            let confirm = self.storage_confirm.is_some()
                || status == &t("settings.confirmForceDelete").to_string();
            let failed = status == &t("settings.forceDeleteFailed").to_string()
                || status == &t("settings.cacheCleanFailed").to_string();
            column = column.child(
                div()
                    .mt_2()
                    .px_1()
                    .text_xs()
                    .whitespace_normal()
                    .text_color(rgb(if confirm {
                        palette().warn
                    } else if failed {
                        palette().danger
                    } else {
                        palette().fg_muted
                    }))
                    .child(status.clone()),
            );
        }
        section_shell(SettingsSection::Storage, None, None, column)
    }
}

fn confirm_key(target: DeleteTarget) -> &'static str {
    match target {
        DeleteTarget::Hub => "settings.confirmForceDeleteHub",
        DeleteTarget::Cache => "settings.confirmForceDelete",
    }
}

fn hub_delete_confirm(names: &[String]) -> String {
    let sep = t("settings.skillNameSeparator").to_string();
    let listed = names.join(&sep);
    tf("settings.confirmForceDeleteHub", &[("names", &listed)]).to_string()
}

fn hub_delete_status(report: &storage_maintenance::ForceDeleteSkillsReport) -> String {
    let count = report.removed.len().to_string();
    if !report.failed.is_empty() {
        let failed = report.failed.len().to_string();
        return tf(
            "settings.forceDeleteHubPartial",
            &[("count", &count), ("failed", &failed)],
        )
        .to_string();
    }
    if report.removed.is_empty() && report.kept.is_empty() {
        return t("settings.forceDeleteHubEmpty").to_string();
    }
    if !report.kept.is_empty() {
        let kept = report.kept.len().to_string();
        return tf(
            "settings.forceDeleteHubKept",
            &[("count", &count), ("kept", &kept)],
        )
        .to_string();
    }
    tf("settings.forceDeleteHubDone", &[("count", &count)]).to_string()
}
