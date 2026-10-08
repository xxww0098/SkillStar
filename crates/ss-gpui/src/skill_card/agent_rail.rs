//! The agent carousel shared by card footers.
//!
//! One 28px slot per targetable agent brand icon: a linked slot highlights,
//! an unlinked slot grays out, and a click reports back through the caller's
//! callback. The skill-card footer and the deck-card footer both paint this
//! rail inside [`agent_footer_bar`]; what a click means stays with the page.

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::agents::AgentProfile;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

/// One slot in the carousel. Callers map their own link state onto it.
#[derive(Clone, Debug)]
pub struct AgentRailSlot {
    pub id: String,
    pub linked: bool,
    pub pending: bool,
}

/// What a slot click asks for. The caller owns the semantics — the skill
/// card emits install/unlink events, the deck card toggles its own link.
pub type AgentRailClick = Rc<dyn Fn(&AgentRailSlot, &mut App)>;

/// Agents the card rail, the detail deployment switches, and the batch
/// link menu may offer. Same set: a global skills directory, and enabled
/// in Settings. A disabled profile stays off every one of those surfaces.
pub fn targetable_agent_profiles(profiles: &[AgentProfile]) -> impl Iterator<Item = &AgentProfile> {
    profiles
        .iter()
        .filter(|profile| profile.has_global_skills() && profile.enabled)
}

/// The footer strip a rail rides in: 42px, well fill, top hairline. The
/// caller prepends its own lead item (stars, deploy) before the rail.
pub fn agent_footer_bar() -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .w_full()
        .min_w_0()
        .h(px(42.0))
        .flex_shrink_0()
        .px(px(14.0))
        .border_t_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().well))
}

/// The horizontal brand-icon carousel. `key` names the card so element ids
/// and spring keys stay unique across pages.
pub fn agent_rail(
    scope: &str,
    key: &str,
    slots: &[AgentRailSlot],
    on_click: AgentRailClick,
) -> Stateful<Div> {
    let mut rail = div()
        .id(ElementId::Name(format!("{scope}-rail-{key}").into()))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_x_scroll();
    for slot in slots {
        let linked = slot.linked;
        let pending = slot.pending;
        let mut glyph = img(crate::agent_icons::agent_icon_path(&slot.id))
            .w(px(16.0))
            .h(px(16.0))
            .flex_shrink_0()
            .cursor_pointer();
        // Pending only dims. The linked flag already flipped, so the color
        // changes on the click instead of after the filesystem call returns.
        if !linked {
            glyph = glyph.grayscale(true).opacity(0.45);
        }
        if pending {
            glyph = glyph.opacity(0.55);
        }
        let spring_key = format!("{scope}-agent-{}-{key}", slot.id);
        let clicked = slot.clone();
        let on_click = on_click.clone();
        rail = rail.child(
            div()
                .id(ElementId::Name(spring_key.clone().into()))
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.0))
                .flex_shrink_0()
                .flex_grow_0()
                .rounded(px(12.0))
                .cursor_pointer()
                .border_1()
                .when(linked, |button| {
                    button
                        .bg(rgb(palette().info_bg))
                        .border_color(rgb(palette().info_border))
                })
                .when(!linked, |button| button.border_color(rgb(palette().well)))
                .child(glyph)
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if pending {
                        return;
                    }
                    on_click(&clicked, cx);
                })
                .interaction_spring(
                    spring_key,
                    !pending,
                    if linked {
                        MotionPaint::new()
                            .bg(rgb(palette().info_bg))
                            .border(rgb(palette().info_border))
                    } else {
                        MotionPaint::new().border(rgb(palette().well))
                    },
                    if linked || pending {
                        if linked {
                            MotionPaint::new()
                                .bg(rgb(palette().info_bg))
                                .border(rgb(palette().info_border))
                        } else {
                            MotionPaint::new().border(rgb(palette().well))
                        }
                    } else {
                        MotionPaint::new()
                            .bg(rgb(palette().card_hover))
                            .border(rgb(palette().border))
                    },
                ),
        );
    }
    rail
}

#[cfg(test)]
mod tests {
    use super::{AgentProfile, targetable_agent_profiles};

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
    fn targetable_profiles_are_the_carousel_set() {
        let mut no_global = profile("no-global", true);
        no_global.global_skills_dir = std::path::PathBuf::new();
        let profiles = [profile("on", true), profile("off", false), no_global];

        let ids: Vec<_> = targetable_agent_profiles(&profiles)
            .map(|profile| profile.id.as_str())
            .collect();

        assert_eq!(ids, vec!["on"]);
    }
}
