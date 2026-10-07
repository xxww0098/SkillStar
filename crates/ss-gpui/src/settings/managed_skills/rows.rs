//! The per-Agent managed-skills row: the switch, its pending/disabled state,
//! the status line, and the notice it shows.

use std::collections::HashSet;

use gpui_kit::*;
use ss_skills::agents::AgentProfile;
use ss_skills::workflows::agent_managed_skills::AgentManagedSkillsState;

use super::SettingsPage;
use super::state::{PauseAction, PauseSnapshot, PauseStatus, global_skills_target_key};
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::i18n::{t, tf};
use crate::theme::palette;

impl SettingsPage {
    pub(crate) fn managed_skills_row(&self, profile: &AgentProfile, view: WeakEntity<Self>) -> Div {
        let key = global_skills_target_key(&profile.global_skills_dir);
        let pending = self.managed.pending.contains(&key);
        let snapshot = pause_snapshot(self.managed.states.get(&key));
        let disabled =
            pending || matches!(snapshot.status, PauseStatus::Loading | PauseStatus::Empty);
        let tinted = matches!(snapshot.status, PauseStatus::Paused | PauseStatus::Partial);
        let status = managed_status_text(pending, &snapshot);
        let emphasized = matches!(snapshot.status, PauseStatus::Paused | PauseStatus::Partial);
        let agent_id = profile.id.clone();
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().fg))
                    .child(t("settings.managedSkills")),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(rgb(if emphasized {
                        palette().accent
                    } else {
                        palette().fg_muted
                    }))
                    .child(status),
            )
            .child(managed_switch(
                &format!("managed-skills-{}", profile.id),
                snapshot.checked,
                disabled,
                tinted,
                view,
                move |this, cx| this.toggle_managed_skills(&agent_id, cx),
            ))
    }

    pub(crate) fn managed_target_pending(&self, profile: &AgentProfile) -> bool {
        profile.has_global_skills()
            && self
                .managed
                .pending
                .contains(&global_skills_target_key(&profile.global_skills_dir))
    }
}

fn managed_switch(
    id: &str,
    checked: bool,
    disabled: bool,
    tinted: bool,
    view: WeakEntity<SettingsPage>,
    apply: impl Fn(&mut SettingsPage, &mut Context<SettingsPage>) + 'static,
) -> impl IntoElement {
    let track = if checked {
        palette().accent
    } else if tinted {
        palette().accent_soft
    } else {
        palette().border
    };
    let mut switch = div()
        .flex_shrink_0()
        .w(px(36.0))
        .h(px(20.0))
        .rounded_full()
        .flex()
        .items_center()
        .bg(rgb(track))
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .rounded_full()
                .bg(rgb(0xffffff))
                .ml(px(if checked { 18.0 } else { 2.0 })),
        );
    if tinted && !checked {
        switch = switch
            .border_1()
            .border_color(rgb(palette().accent_soft_edge));
    }
    if disabled {
        switch = switch.opacity(0.5);
    } else {
        switch = switch.cursor_pointer();
    }
    switch
        .id(ElementId::Name(id.to_string().into()))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            if disabled {
                return;
            }
            let _ = view.update(cx, |this, cx| {
                apply(this, cx);
                cx.notify();
            });
        })
        .interaction_spring(
            id.to_string(),
            !disabled,
            MotionPaint::new().opacity(1.0),
            MotionPaint::new().opacity(1.0),
        )
}

fn managed_status_text(pending: bool, snapshot: &PauseSnapshot) -> SharedString {
    if pending {
        return t("settings.managedSkillsUpdating");
    }
    match snapshot.status {
        PauseStatus::Loading => t("settings.managedSkillsChecking"),
        PauseStatus::Empty => t("settings.noManagedSkills"),
        PauseStatus::Paused => tf(
            "settings.managedSkillsPausedCount",
            &[("count", &snapshot.suspended.len().to_string())],
        ),
        PauseStatus::Partial => tf(
            "settings.managedSkillsPartialCount",
            &[
                ("paused", &snapshot.suspended.len().to_string()),
                ("active", &snapshot.active.len().to_string()),
            ],
        ),
        PauseStatus::Active => tf(
            "settings.managedSkillsActiveCount",
            &[("count", &snapshot.active.len().to_string())],
        ),
    }
}

pub(super) fn pause_snapshot(state: Option<&AgentManagedSkillsState>) -> PauseSnapshot {
    let Some(state) = state else {
        return PauseSnapshot {
            status: PauseStatus::Loading,
            active: Vec::new(),
            suspended: Vec::new(),
            action: None,
            checked: false,
        };
    };
    let active = unique_names(&state.active_skill_names);
    let suspended = unique_names(&state.suspended_skill_names);
    let has_active = !active.is_empty();
    let has_suspended = !suspended.is_empty();
    let status = if has_suspended {
        if has_active {
            PauseStatus::Partial
        } else {
            PauseStatus::Paused
        }
    } else if has_active {
        PauseStatus::Active
    } else {
        PauseStatus::Empty
    };
    PauseSnapshot {
        action: if has_suspended {
            Some(PauseAction::Restore)
        } else if has_active {
            Some(PauseAction::Pause)
        } else {
            None
        },
        checked: status == PauseStatus::Active,
        status,
        active,
        suspended,
    }
}

fn unique_names(names: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for raw in names {
        let name = raw.trim();
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        unique.push(name.to_string());
    }
    unique
}
