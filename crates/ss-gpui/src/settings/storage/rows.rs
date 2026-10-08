//! The storage card: the path hint, the per-root rows, and the maintenance
//! actions they offer.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_app::storage_maintenance::StorageOverview;

use super::SettingsPage;
use super::parts::{clean_button, delete_button, folder_button};
use crate::chrome::{InteractionSpring, MotionPaint, icon_spin};
use crate::i18n::{t, tf};
use crate::settings::{DeleteTarget, card, format_bytes};
use crate::theme::palette;

impl SettingsPage {
    pub(super) fn path_hint(&self, view: WeakEntity<Self>) -> Div {
        let open = self.path_structure_open;
        let v = view;
        div()
            .mb_2()
            .px_1()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .whitespace_normal()
                    .child(t("settings.storagePathModelHint")),
            )
            .child(
                div()
                    .id("storage-path-toggle")
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(if open {
                        t("common.hide")
                    } else {
                        t("settings.viewPathStructure")
                    })
                    .child(
                        Icon::new(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(14.0))
                        .text_color(rgb(palette().fg_muted)),
                    )
                    .on_click(move |_, _, cx| {
                        let _ = v.update(cx, |this, cx| {
                            this.path_structure_open = !this.path_structure_open;
                            cx.notify();
                        });
                    })
                    .interaction_spring(
                        "storage-path-toggle",
                        true,
                        MotionPaint::new().fg(rgb(palette().fg_muted)),
                        MotionPaint::new().fg(rgb(palette().fg)),
                    ),
            )
    }

    pub(super) fn storage_card(&self, view: WeakEntity<Self>) -> Div {
        let cleaning = self.cleaning;
        let loading = self.storage_loading;
        let total = self.storage.as_ref().filter(|_| !loading).map(|overview| {
            format_bytes(
                overview.config_bytes
                    + overview.hub_bytes
                    + overview.local_bytes
                    + overview.cache_bytes,
            )
        });
        let v = view.clone();
        let mut title = div().flex().flex_col().gap(px(2.0)).child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(palette().fg))
                .child(t("settings.storageTotal")),
        );
        if let Some(total) = total {
            title = title.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(total),
            );
        }
        let mut card = card().child(
            div()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .border_b_1()
                .border_color(rgb(palette().border))
                .bg(rgb(palette().well))
                .child(title)
                .child(clean_button(cleaning, loading, v)),
        );

        if let Some(overview) = &self.storage {
            if !loading {
                if overview.broken_count > 0
                    || overview.health_issue_count > 0
                    || !overview.intake.is_empty()
                {
                    card = card.child(self.broken_row(overview, view.clone()));
                }
                card = card.child(self.hub_row(overview, view.clone()));
                card = card.child(self.cache_row(overview, view.clone()));
                card = card.child(self.plain_row(
                    "local",
                    IconName::FolderOpen,
                    palette().violet_fg,
                    t("settings.storageLocal"),
                    Some(t("settings.storageSourceHub")),
                    if overview.local_count > 0 {
                        format!(
                            "{} · {}",
                            format_bytes(overview.local_bytes),
                            tf(
                                "settings.storageLocalCount",
                                &[("count", &overview.local_count.to_string())]
                            )
                        )
                    } else {
                        t("settings.storageEmpty").to_string()
                    },
                    false,
                    &overview.local_path,
                    None,
                    view.clone(),
                ));
                card = card.child(self.plain_row(
                    "config",
                    IconName::Globe,
                    palette().violet,
                    t("settings.storageConfig"),
                    Some(t("settings.storageSourceData")),
                    format_bytes(overview.config_bytes),
                    false,
                    &overview.config_path,
                    None,
                    view.clone(),
                ));
                card = card.child(self.plain_row(
                    "history",
                    IconName::FileClock,
                    palette().ok,
                    t("settings.storageHistory"),
                    Some(t("settings.storageSourceData")),
                    if overview.history_count > 0 {
                        tf(
                            "settings.storageHistoryCount",
                            &[("count", &overview.history_count.to_string())],
                        )
                        .to_string()
                    } else {
                        t("settings.storageEmpty").to_string()
                    },
                    overview.history_count > 0,
                    &overview.config_path,
                    None,
                    view,
                ));
            }
        }
        if loading {
            card = card.child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(icon_spin(IconName::Loader, 12.0, palette().fg_muted, true))
                    .child(t("common.loading")),
            );
        }
        card
    }

    fn broken_row(&self, overview: &StorageOverview, view: WeakEntity<Self>) -> Div {
        let busy = self.storage_busy_repair();
        let preview = view.clone();
        let repair = view.clone();
        let intake_view = view.clone();
        let actionable = overview
            .intake
            .iter()
            .filter(|item| item.kind == "adopt" || item.kind == "relink")
            .count();
        let reported = overview.intake.len().saturating_sub(actionable);
        let mut detail = tf(
            "settings.healthSummary",
            &[
                ("count", &overview.health_issue_count.to_string()),
                ("repairable", &overview.health_repairable.to_string()),
            ],
        )
        .to_string();
        if !overview.intake.is_empty() {
            detail.push_str(" · ");
            detail.push_str(&tf(
                "settings.intakeSummary",
                &[
                    ("actionable", &actionable.to_string()),
                    ("reported", &reported.to_string()),
                ],
            ));
        }
        let mut actions = div().flex().items_center().gap_1();
        if overview.broken_count > 0 || overview.health_issue_count > 0 {
            actions = actions
                .child(health_button(
                    "storage-repair-preview",
                    if self.previewing {
                        t("settings.repairPreviewing")
                    } else {
                        t("settings.repairPreview")
                    },
                    false,
                    busy,
                    move |_, _, cx| {
                        let _ = preview.update(cx, |this, cx| this.preview_skill_repair(cx));
                    },
                ))
                .child(health_button(
                    "storage-repair",
                    if self.repairing {
                        t("settings.repairing")
                    } else {
                        t("settings.repairAll")
                    },
                    true,
                    busy,
                    move |_, _, cx| {
                        let _ = repair.update(cx, |this, cx| this.repair_skills(cx));
                    },
                ));
        }
        let row = self.plain_row(
            "health",
            IconName::Stethoscope,
            palette().warn,
            t("settings.skillHealth"),
            Some(t("settings.repairHint")),
            detail,
            overview.health_issue_count > 0 || actionable > 0,
            &overview.hub_path,
            Some(actions.into_any_element()),
            view,
        );
        let mut block = div().flex().flex_col().child(row);
        if !overview.intake.is_empty() {
            block = block.child(self.intake_lines(overview, intake_view));
        }
        block
    }

    fn intake_lines(&self, overview: &StorageOverview, view: WeakEntity<Self>) -> Div {
        let mut lines = div()
            .px_4()
            .pb_2()
            .flex()
            .flex_col()
            .gap_1()
            .text_xs()
            .text_color(rgb(palette().fg_muted))
            .child(t("settings.intakeHint"));
        for item in overview.intake.iter().take(8) {
            let note = intake_note(&item.kind).to_string();
            lines = lines.child(div().text_color(rgb(palette().fg)).child(tf(
                "settings.intakeLine",
                &[
                    ("agent", &item.agent_id),
                    ("skill", &item.skill),
                    ("note", &note),
                ],
            )));
        }
        if overview.intake.len() > 8 {
            lines = lines.child(tf(
                "settings.intakeMore",
                &[("count", &(overview.intake.len() - 8).to_string())],
            ));
        }
        let busy = self.storage_busy_repair();
        let preview = view.clone();
        let apply = view;
        let actionable = overview
            .intake
            .iter()
            .filter(|item| item.kind == "adopt" || item.kind == "relink")
            .count();
        let mut buttons = div().flex().items_center().gap_1().mt_1();
        buttons = buttons.child(health_button(
            "storage-intake-preview",
            if self.previewing_intake {
                t("settings.intakePreviewing")
            } else {
                t("settings.intakePreview")
            },
            false,
            busy,
            move |_, _, cx| {
                let _ = preview.update(cx, |this, cx| this.preview_agent_intake(cx));
            },
        ));
        if actionable > 0 {
            buttons = buttons.child(health_button(
                "storage-intake",
                if self.adopting {
                    t("settings.intakeApplying")
                } else {
                    t("settings.intakeApply")
                },
                true,
                busy,
                move |_, _, cx| {
                    let _ = apply.update(cx, |this, cx| this.adopt_agent_skills(cx));
                },
            ));
        }
        lines.child(buttons)
    }

    fn hub_row(&self, overview: &StorageOverview, view: WeakEntity<Self>) -> Div {
        let mut detail = format!(
            "{} · {}",
            format_bytes(overview.hub_bytes),
            tf(
                "settings.storageHubCount",
                &[("count", &overview.hub_count.to_string())]
            )
        );
        if overview.broken_count > 0 {
            detail.push_str(" (");
            detail.push_str(&tf(
                "settings.healthBroken",
                &[("count", &overview.broken_count.to_string())],
            ));
            detail.push(')');
        }
        self.plain_row(
            "hub",
            IconName::Database,
            palette().accent,
            t("settings.storageHub"),
            Some(t("settings.storageSourceHub")),
            detail,
            overview.broken_count > 0,
            &overview.hub_path,
            Some(
                delete_button("storage-del-hub", DeleteTarget::Hub, self, view.clone())
                    .into_any_element(),
            ),
            view,
        )
    }

    fn cache_row(&self, overview: &StorageOverview, view: WeakEntity<Self>) -> Div {
        let detail = if overview.cache_count > 0 {
            let mut text = format!(
                "{} · {}",
                format_bytes(overview.cache_bytes),
                tf(
                    "settings.cacheRepos",
                    &[("count", &overview.cache_count.to_string())]
                )
            );
            if overview.cache_unused_count > 0 {
                text.push_str(" · ");
                text.push_str(&tf(
                    "settings.cacheUnused",
                    &[("count", &overview.cache_unused_count.to_string())],
                ));
                text.push_str(" (");
                text.push_str(&format_bytes(overview.cache_unused_bytes));
                text.push(')');
            }
            text
        } else {
            t("settings.storageEmpty").to_string()
        };
        self.plain_row(
            "cache",
            IconName::FolderGit2,
            palette().warn,
            t("settings.repoCache"),
            Some(t("settings.storageSourceHub")),
            detail,
            overview.cache_unused_count > 0,
            &overview.cache_path,
            Some(
                delete_button("storage-del-cache", DeleteTarget::Cache, self, view.clone())
                    .into_any_element(),
            ),
            view,
        )
    }

    fn plain_row(
        &self,
        id: &str,
        icon: IconName,
        icon_color: u32,
        label: SharedString,
        source: Option<SharedString>,
        detail: String,
        highlight: bool,
        path: &str,
        extra: Option<AnyElement>,
        view: WeakEntity<Self>,
    ) -> Div {
        let mut actions = div().flex().items_center().gap_1().flex_shrink_0();
        if let Some(extra) = extra {
            actions = actions.child(extra);
        }
        actions = actions.child(folder_button(id, path, view));
        div()
            .px_4()
            .py(px(10.0))
            .flex()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(rgb(palette().border))
            .child(
                div()
                    .size(px(24.0))
                    .flex_shrink_0()
                    .rounded_md()
                    .bg(rgb(palette().well))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(icon).size(px(14.0)).text_color(rgb(icon_color))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(palette().fg))
                                    .child(label),
                            )
                            .when_some(source, |row, source| {
                                row.child(
                                    div()
                                        .px(px(6.0))
                                        .py(px(1.0))
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(rgb(palette().border))
                                        .bg(rgb(palette().well))
                                        .text_xs()
                                        .text_color(rgb(palette().fg_muted))
                                        .child(source),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .text_xs()
                            .truncate()
                            .text_color(rgb(if highlight {
                                palette().warn
                            } else {
                                palette().fg_muted
                            }))
                            .child(detail),
                    ),
            )
            .child(actions)
    }
}

fn intake_note(kind: &str) -> SharedString {
    match kind {
        "adopt" => t("settings.intakeAdopt"),
        "relink" => t("settings.intakeRelink"),
        "conflict" => t("settings.intakeConflict"),
        "excluded" => t("settings.intakeExcluded"),
        "foreign" => t("settings.intakeForeign"),
        _ => t("settings.intakeOccupied"),
    }
}

fn health_button(
    id: &'static str,
    label: SharedString,
    emphasize: bool,
    busy: bool,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::chrome::MotionDiv {
    let (border, color, hover_bg) = if emphasize {
        (palette().warn_border, palette().warn, palette().warn_bg)
    } else {
        (palette().border, palette().fg, palette().panel_hover)
    };
    div()
        .id(id)
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(border))
        .text_xs()
        .text_color(rgb(color))
        .when(!busy, |d| d.cursor_pointer())
        .when(busy, |d| d.opacity(0.6))
        .when(busy && emphasize, |d| {
            d.child(icon_spin(IconName::Loader, 12.0, color, true))
        })
        .when(!busy && emphasize, |d| {
            d.child(
                Icon::new(IconName::Wrench)
                    .size(px(12.0))
                    .text_color(rgb(color)),
            )
        })
        .child(label)
        .on_click(click)
        .interaction_spring(
            id,
            !busy,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(hover_bg)),
        )
}
