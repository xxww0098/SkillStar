//! Settings → Skill updates.
//!
//! One switch owns the mode: on is manual, off is automatic. Automatic mode
//! shows the check frequency. The background monitor (`ss-app::skill_wake`)
//! reads the same config file on every wake, so this section never schedules
//! or runs an update itself.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use ss_core::config::skill_updates::{self, INTERVAL_CHOICES_MINUTES};

use super::{SettingsPage, SettingsSection, card, choice_pills, field_label, section_shell};
use crate::i18n::t;
use crate::theme::palette;

impl SettingsPage {
    pub(crate) fn render_skill_updates(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let manual = !self.skill_updates.auto_update;
        let mode = t(if manual {
            "settings.skillUpdatesManual"
        } else {
            "settings.skillUpdatesAuto"
        });
        section_shell(
            SettingsSection::SkillUpdates,
            None,
            None,
            card().child(
                div()
                    .flex()
                    .flex_col()
                    .child(Self::mode_row(manual, mode, view.clone()))
                    .when(!manual, |column| {
                        column.child(Self::interval_row(
                            self.skill_updates.interval_minutes,
                            view,
                        ))
                    }),
            ),
        )
    }

    fn mode_row(manual: bool, mode: SharedString, view: WeakEntity<Self>) -> gpui_kit::Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .px_4()
            .py_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(t("settings.skillUpdatesMode")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .child(mode),
                    ),
            )
            .child(Self::toggle(
                "skill-update-mode-toggle",
                manual,
                view,
                |this, cx| {
                    this.skill_updates.auto_update = !this.skill_updates.auto_update;
                    this.save_skill_updates(cx);
                },
            ))
    }

    fn interval_row(minutes: u64, view: WeakEntity<Self>) -> gpui_kit::Div {
        let choices = [
            ("15", t("settings.skillUpdatesEvery15")),
            ("30", t("settings.skillUpdatesEvery30")),
            ("60", t("settings.skillUpdatesEvery60")),
            ("360", t("settings.skillUpdatesEvery360")),
            ("1440", t("settings.skillUpdatesEveryDay")),
        ];
        let current = minutes.to_string();
        debug_assert!(INTERVAL_CHOICES_MINUTES.contains(&minutes));
        div()
            .px_4()
            .pt_3()
            .pb_4()
            .border_t_1()
            .border_color(rgb(palette().border))
            .child(field_label(
                t("settings.skillUpdatesInterval"),
                choice_pills(
                    "skill-update-interval",
                    &choices,
                    &current,
                    view,
                    |this, id, cx| {
                        let Ok(picked) = id.parse::<u64>() else {
                            return;
                        };
                        this.skill_updates.interval_minutes =
                            skill_updates::resolve_interval_minutes(picked);
                        this.save_skill_updates(cx);
                    },
                ),
            ))
    }

    /// Persist the mode and interval. The background monitor re-reads the
    /// file on every wake, so a change takes effect within a minute.
    pub(crate) fn save_skill_updates(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = skill_updates::save_config(&self.skill_updates) {
            tracing::warn!("failed to save skill update prefs: {error}");
        }
        cx.notify();
    }
}
