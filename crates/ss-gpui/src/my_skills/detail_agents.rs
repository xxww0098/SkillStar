//! Master switch for the skill detail column's agent deployment list.

use gpui_kit::*;
use ss_skills::agents::AgentProfile;

use super::MySkillsPage;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

impl MySkillsPage {
    /// One switch for every row below it. Off unless every listed agent is
    /// linked: sliding it on links the rest, sliding it off unlinks them all.
    pub(super) fn master_agent_switch(
        &self,
        skill_name: &str,
        profiles: &[&AgentProfile],
        agent_links: &[String],
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let pairs: Vec<(String, bool)> = profiles
            .iter()
            .map(|profile| (profile.id.clone(), profile_is_linked(profile, agent_links)))
            .collect();
        let all_on = pairs.iter().all(|(_, on)| *on);
        let enable = super::commands::master_switch_enable(&pairs).unwrap_or(false);
        let ids = super::commands::master_switch_agent_ids(&pairs, enable);
        let busy = pairs
            .iter()
            .any(|(id, _)| self.pending_agents.contains(&format!("{skill_name}::{id}")));
        let s_name = skill_name.to_string();
        let toggle_key = format!("drawer-toggle-all-{skill_name}");
        let row_key = format!("drawer-agent-all-{skill_name}");

        div()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1p5()
            .rounded_md()
            .border_b_1()
            .border_color(rgb(palette().border))
            .id(ElementId::Name(row_key.clone().into()))
            .interaction_spring(
                row_key,
                true,
                MotionPaint::new(),
                MotionPaint::new().bg(rgb(palette().card_hover)),
            )
            .child(
                div()
                    .min_w_0()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().fg))
                    .child(crate::i18n::t("detailPanel.allAgents").to_string()),
            )
            .child(
                {
                    let switch = slide_switch(&toggle_key, all_on);
                    if busy { switch.opacity(0.55) } else { switch }
                }
                .tooltip(move |window, cx| {
                    crate::chrome::tooltip(crate::i18n::t(if all_on {
                        "detailPanel.unlinkAll"
                    } else {
                        "detailPanel.linkAll"
                    }))
                    .build(window, cx)
                })
                .on_click(move |_, _, cx| {
                    let s_name = s_name.clone();
                    let ids = ids.clone();
                    let _ = view.update(cx, |this, cx| {
                        this.set_skill_agent_links(&s_name, &ids, enable, cx);
                    });
                })
                .interaction_spring(
                    toggle_key,
                    true,
                    MotionPaint::new().opacity(1.0),
                    MotionPaint::new().opacity(1.0),
                ),
            )
    }
}

pub(super) fn profile_is_linked(profile: &AgentProfile, links: &[String]) -> bool {
    links
        .iter()
        .any(|link| link == &profile.display_name || link == &profile.id)
}

/// The same pill the per-agent rows use, before the click and hover spring.
pub(super) fn slide_switch(id: &str, on: bool) -> Stateful<Div> {
    div()
        .id(ElementId::Name(id.to_string().into()))
        .w(px(34.0))
        .h(px(18.0))
        .rounded_full()
        .cursor_pointer()
        .flex_shrink_0()
        .bg(rgb(if on {
            palette().accent
        } else {
            palette().border
        }))
        .flex()
        .items_center()
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .rounded_full()
                .bg(rgb(palette().on_accent))
                .ml(px(if on { 17.0 } else { 2.0 })),
        )
}
