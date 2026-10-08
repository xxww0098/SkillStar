//! skill-card body. Clicks emit [`SkillCardEvent`]; they do not touch a page.
//! The outer box is `crate::skill_card::card_shell`.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::*;
use ss_core::types::skill::{SkillType, UpstreamChange};

use super::avatar::{avatar_look, cached_owner_avatar};
use super::{SkillCardEvent, SkillCardProps};
use crate::skill_card::{
    AgentRailSlot, CardFace, CardShell, CardWidth, agent_footer_bar, agent_rail, card_shell,
    skill_source_chip,
};

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

pub type SkillCardEmit = Rc<dyn Fn(SkillCardEvent, &mut App)>;

pub fn render_skill_card(props: SkillCardProps, emit: SkillCardEmit, cx: &App) -> impl IntoElement {
    let SkillCardProps {
        scope,
        skill,
        agents,
        selected,
        highlighted,
        updating,
        installing,
        selectable,
        library,
        translate_override,
    } = props;
    let name = skill.name.clone();

    // Width comes from the container: a grid track (技能 / 市场 share one
    // pitch) or the whole pane in list mode. The card only fills its box.
    let mut card = card_shell(CardShell {
        id: eid(scope, "card", &name),
        width: CardWidth::Fill,
        face: CardFace::Skill,
        selected: selected || highlighted,
    })
    .flex()
    .flex_col()
    .justify_between()
    .flex_shrink_0();

    card = card.child(card_body(
        scope,
        &skill,
        selected,
        updating,
        installing,
        selectable,
        library,
        translate_override,
        emit.clone(),
        cx,
    ));
    if let Some(footer) = card_footer(scope, &skill, &agents, library, emit.clone()) {
        card = card.child(footer);
    }

    let open = emit;
    let selector = name.clone();
    let card = card
        .debug_selector(move || format!("skill-card-{selector}"))
        .on_click(move |_, _, cx| open(SkillCardEvent::Open, cx));
    #[cfg(test)]
    let card = {
        let hovered_name = name;
        card.on_hover(move |hovered, _, _| note_skill_card_hover(&hovered_name, *hovered))
    };
    card
}

fn card_body(
    scope: &'static str,
    skill: &ss_core::types::skill::Skill,
    selected: bool,
    updating: bool,
    installing: bool,
    selectable: bool,
    library: bool,
    translate_override: Option<bool>,
    emit: SkillCardEmit,
    cx: &App,
) -> impl IntoElement {
    let look = avatar_look(skill);
    let mut header = div()
        .flex()
        .items_start()
        .gap(px(10.0))
        .min_w_0()
        .flex_shrink_0()
        .child(avatar(
            scope,
            skill,
            &look,
            selected,
            selectable,
            emit.clone(),
        ));

    let mut title = div().flex().flex_col().flex_1().min_w_0().gap(px(4.0));
    let mut title_row = div().flex().items_center().gap_1().min_w_0();
    if !library && skill.rank.is_some_and(|rank| rank <= 100) {
        title_row = title_row.child(
            div()
                .flex_shrink_0()
                .px_1()
                .rounded_sm()
                .bg(rgb(palette().warn_bg))
                .border_1()
                .border_color(rgb(palette().warn_border))
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(palette().warn))
                .child(format!("#{}", skill.rank.unwrap_or(0))),
        );
    }
    title_row = title_row.child(
        div()
            .text_sm()
            .font_bold()
            .text_color(rgb(palette().fg))
            .truncate()
            .child(skill.name.clone()),
    );
    title = title.child(title_row);
    if let Some(badge) = source_badge(scope, skill, emit.clone(), cx) {
        title = title.child(badge);
    }
    header = header.child(title);
    if let Some(status) = status_action(scope, skill, updating, installing, library, emit, cx) {
        header = header.child(status);
    }

    let (desc, translated) = match super::super::description_source(skill) {
        None => (crate::i18n::t("skillCard.noDescription").to_string(), false),
        Some(source) => {
            let shown = match translate_override {
                // The drawer's button chose for its opening; the card follows.
                Some(on) => crate::translation::display_when(source, on),
                None => {
                    crate::translation::display(source, crate::translation::Surface::Description)
                }
            };
            let translated = shown != source;
            (shown, translated)
        }
    };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_2()
        .p(px(14.0))
        .pb(px(8.0))
        .overflow_hidden()
        .child(header)
        .child({
            let desc_selector = skill.name.clone();
            div()
                .flex_shrink_0()
                .flex_grow_0()
                .overflow_hidden()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .line_clamp(2)
                .debug_selector(move || {
                    format!(
                        "skill-card-{}-{desc_selector}",
                        if translated { "translation" } else { "desc" }
                    )
                })
                .child(desc)
        })
}

fn avatar(
    scope: &'static str,
    skill: &ss_core::types::skill::Skill,
    look: &super::avatar::AvatarLook,
    selected: bool,
    selectable: bool,
    emit: SkillCardEmit,
) -> impl IntoElement {
    // Overflow clips to a rectangle, so the photo and the fallback both carry
    // their own radius. 16px on a 32px box is a circle.
    let hover = SharedString::from(format!("{scope}-avatar-{}", skill.name));
    let mut face = div()
        .group(hover.clone())
        .relative()
        .flex_shrink_0()
        .size(px(32.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(32.0))
                .overflow_hidden()
                .rounded(px(16.0))
                .bg(rgb(look.bg))
                .border_1()
                .border_color(rgb(if selected {
                    palette().accent
                } else {
                    look.border
                }))
                .child(
                    Icon::new(look.icon)
                        .with_size(px(16.0))
                        .text_color(rgb(look.fg)),
                ),
        );
    if let Some(path) = cached_owner_avatar(skill) {
        face = face.child(
            img(path)
                .absolute()
                .inset_0()
                .size(px(32.0))
                .rounded(px(16.0))
                .object_fit(ObjectFit::Cover)
                .border_1()
                .border_color(rgb(if selected {
                    palette().accent
                } else {
                    palette().border
                })),
        );
    }
    if selectable {
        face = face.child(selection_overlay(scope, &skill.name, selected, hover, emit));
    }
    face
}

fn selection_overlay(
    scope: &'static str,
    name: &str,
    selected: bool,
    group: SharedString,
    emit: SkillCardEmit,
) -> AnyElement {
    let mut overlay = div()
        .id(eid(scope, "check", name))
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(16.0))
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            emit(SkillCardEvent::Select, cx);
        });
    if selected {
        overlay = overlay.bg(rgb(palette().accent)).child(
            Icon::new(IconName::Check)
                .with_size(px(16.0))
                .text_color(rgb(palette().on_accent)),
        );
    } else {
        // Only the avatar group, so moving across the title or description
        // does not reveal the checkbox.
        overlay = overlay
            .opacity(0.0)
            .group_hover(group, |style| style.opacity(1.0))
            .bg(rgb(palette().scrim))
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(palette().edge))
                    .bg(rgb(palette().card)),
            );
    }
    overlay.into_any_element()
}

fn source_badge(
    scope: &'static str,
    skill: &ss_core::types::skill::Skill,
    emit: SkillCardEmit,
    cx: &App,
) -> Option<AnyElement> {
    if skill.skill_type == SkillType::Local {
        return Some(
            div()
                .id(eid(scope, "local", &skill.name))
                .flex()
                .items_center()
                .gap_1()
                .self_start()
                .px_1p5()
                .py(px(1.0))
                .rounded(px(6.0))
                .bg(rgb(palette().ok_bg))
                .border_1()
                .border_color(rgb(palette().ok_border))
                .text_size(px(10.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().ok))
                .cursor_pointer()
                .child(Icon::new(IconName::HardDrive).with_size(px(10.0)))
                .child(crate::i18n::t("mySkills.scopeLocal"))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    emit(SkillCardEvent::OpenFolder, cx);
                })
                .interaction_spring(
                    format!("{scope}-local-{}", skill.name),
                    true,
                    MotionPaint::new().bg(rgb(palette().ok_bg)),
                    MotionPaint::new().bg(rgb(palette().ok_hover)),
                )
                .into_any_element(),
        );
    }
    let chip = skill_source_chip(scope, skill)?;
    Some(
        chip.render(cx, move |url, app| {
            emit(SkillCardEvent::OpenLink(url.to_string()), app);
        })
        .into_any_element(),
    )
}

fn status_action(
    scope: &'static str,
    skill: &ss_core::types::skill::Skill,
    updating: bool,
    installing: bool,
    library: bool,
    emit: SkillCardEmit,
    cx: &App,
) -> Option<AnyElement> {
    if skill.skill_type == SkillType::Local {
        return None;
    }
    let blocked_label = match &skill.upstream_change {
        Some(UpstreamChange::Removed { .. }) => Some(crate::i18n::t("skillCard.upstreamRemoved")),
        Some(UpstreamChange::IdentityChanged { .. }) => {
            Some(crate::i18n::t("skillCard.upstreamRenamed"))
        }
        Some(UpstreamChange::LocalChanges { .. }) | None => None,
    };
    if let Some(label) = blocked_label {
        return Some(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_1()
                .h(px(24.0))
                .px(px(10.0))
                .rounded_full()
                .bg(rgb(palette().danger_bg))
                .border_1()
                .border_color(rgb(palette().danger_border))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().danger_fg))
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .with_size(px(12.0))
                        .text_color(rgb(palette().danger_fg)),
                )
                .child(label)
                .into_any_element(),
        );
    }
    let overwrites_local = matches!(
        skill.upstream_change,
        Some(UpstreamChange::LocalChanges { .. })
    );
    if skill.update_available {
        let mut button = div()
            .id(eid(scope, "update", &skill.name))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .h(px(24.0))
            .px(px(10.0))
            .rounded_full()
            .bg(rgb(palette().warn_bg))
            .border_1()
            .border_color(rgb(palette().warn_border))
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(palette().warn))
            .cursor_pointer();
        if updating {
            button = button.opacity(0.75).child(updating_label(
                eid(scope, "updating-dots", &skill.name),
                cx.reduce_motion(),
            ));
        } else {
            button = button
                .child(
                    Icon::new(IconName::CircleArrowUp)
                        .with_size(px(12.0))
                        .text_color(rgb(palette().warn)),
                )
                .child(if overwrites_local {
                    crate::i18n::t("skillCard.updateOverwritesLocal")
                } else {
                    crate::i18n::t("common.update")
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    emit(SkillCardEvent::Update, cx);
                });
        }
        return Some(
            button
                .interaction_spring(
                    format!("{scope}-update-{}", skill.name),
                    !updating,
                    MotionPaint::new().bg(rgb(palette().warn_bg)),
                    MotionPaint::new().bg(rgb(if updating {
                        palette().warn_bg
                    } else {
                        palette().warn_hover
                    })),
                )
                .into_any_element(),
        );
    }
    if library || skill.installed {
        if !library && skill.installed {
            return Some(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t("skillCard.installed"))
                    .into_any_element(),
            );
        }
        return None;
    }
    let url = skill.git_url.clone();
    let mut button = div()
        .id(eid(scope, "install", &skill.name))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .h(px(24.0))
        .px_2()
        .rounded_md()
        .bg(rgb(palette().accent))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette().on_accent))
        .cursor_pointer();
    if installing {
        button = button
            .opacity(0.75)
            .child(crate::i18n::t("common.installing"));
    } else {
        button = button
            .child(Icon::new(IconName::Download).with_size(px(12.0)))
            .child(crate::i18n::t("common.install"))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                emit(
                    SkillCardEvent::Install {
                        url: url.clone(),
                        agent_id: None,
                    },
                    cx,
                );
            });
    }
    Some(
        button
            .interaction_spring(
                format!("{scope}-install-{}", skill.name),
                !installing,
                MotionPaint::new().bg(rgb(palette().accent)),
                MotionPaint::new().bg(rgb(if installing {
                    palette().accent
                } else {
                    palette().accent_hover
                })),
            )
            .into_any_element(),
    )
}

fn card_footer(
    scope: &str,
    skill: &ss_core::types::skill::Skill,
    agents: &[AgentRailSlot],
    library: bool,
    emit: SkillCardEmit,
) -> Option<AnyElement> {
    let stars = (!library && skill.stars > 0).then(|| skill.stars);
    if stars.is_none() && agents.is_empty() {
        return None;
    }
    let mut footer = agent_footer_bar();
    if let Some(stars) = stars {
        footer = footer.child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_1()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().warn))
                .child(
                    Icon::new(IconName::Star)
                        .with_size(px(12.0))
                        .text_color(rgb(palette().warn)),
                )
                .child(format_count(stars)),
        );
    }
    if !agents.is_empty() {
        // Same branch as the React carousel: a linked icon unlinks, a git
        // source installs onto that agent, anything else toggles the link on.
        let git_url = skill.git_url.clone();
        footer = footer.child(agent_rail(
            scope,
            &skill.name,
            agents,
            std::rc::Rc::new(move |slot, cx| {
                emit(
                    SkillCardEvent::carousel(slot.linked, &git_url, &slot.id),
                    cx,
                );
            }),
        ));
    }
    Some(footer.into_any_element())
}

/// One ellipsis step. The window rebuilds once per step, not once per refresh.
const ELLIPSIS_STEP: Duration = Duration::from_millis(400);

/// `.` then `..` then `...`, in equal thirds of a cycle.
fn ellipsis_count(delta: f32) -> usize {
    let scaled = (delta.clamp(0.0, 1.0) * 3.0 + 1.0e-4).min(2.999);
    scaled.floor() as usize + 1
}

/// `common.updating` is `更新中...` / `Updating...`. The badge draws the dots
/// itself so the count can change without a second translated fragment.
fn updating_stem() -> SharedString {
    let label = crate::i18n::t("common.updating");
    let stem = stem_without_ellipsis(label.as_ref());
    if stem.len() == label.len() {
        label
    } else {
        stem.to_string().into()
    }
}

fn stem_without_ellipsis(label: &str) -> &str {
    let stem = label.trim_end_matches(['.', '…']);
    if stem.is_empty() { label } else { stem }
}

/// Trailing dots on the update pill. All three marks stay in the layout, so
/// the pill does not change width as they appear. Reduced motion keeps the
/// full ellipsis and does not schedule frames.
fn updating_label(id: ElementId, reduce_motion: bool) -> AnyElement {
    let row = div().flex().items_baseline().child(updating_stem());
    if reduce_motion {
        return row.child(ellipsis_marks(3)).into_any_element();
    }
    let step_secs = ELLIPSIS_STEP.as_secs_f32();
    row.with_animation(
        id,
        Animation::new(ELLIPSIS_STEP * 3)
            .repeat_synced()
            .with_max_fps(1.0 / step_secs),
        |row, delta| row.child(ellipsis_marks(ellipsis_count(delta))),
    )
    .into_any_element()
}

fn ellipsis_marks(count: usize) -> Div {
    let mut marks = div().flex().items_baseline();
    for index in 0..3 {
        marks = marks.child(
            div()
                .opacity(if index < count { 1.0 } else { 0.0 })
                .child("."),
        );
    }
    marks
}

fn format_count(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}m", n as f32 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f32 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn eid(scope: &str, kind: &str, name: &str) -> ElementId {
    ElementId::Name(format!("{scope}-{kind}-{name}").into())
}

#[cfg(test)]
thread_local! {
    static HOVERED_SKILL_CARDS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

#[cfg(test)]
fn note_skill_card_hover(name: &str, hovered: bool) {
    HOVERED_SKILL_CARDS.with(|cards| {
        let mut cards = cards.borrow_mut();
        if hovered {
            cards.insert(name.to_string());
        } else {
            cards.remove(name);
        }
    });
}

#[cfg(test)]
pub(crate) fn reset_skill_card_hover() {
    HOVERED_SKILL_CARDS.with(|cards| cards.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn skill_card_is_hovered(name: &str) -> bool {
    HOVERED_SKILL_CARDS.with(|cards| cards.borrow().contains(name))
}

#[cfg(test)]
mod tests {
    use super::{ellipsis_count, stem_without_ellipsis};

    #[test]
    fn updating_ellipsis_steps_through_one_two_three_dots() {
        assert_eq!(ellipsis_count(0.0), 1);
        assert_eq!(ellipsis_count(1.0 / 3.0 - 0.01), 1);
        assert_eq!(ellipsis_count(1.0 / 3.0), 2);
        assert_eq!(ellipsis_count(0.5), 2);
        assert_eq!(ellipsis_count(2.0 / 3.0), 3);
        assert_eq!(ellipsis_count(0.99), 3);
        assert_eq!(ellipsis_count(1.0), 3);
    }

    #[test]
    fn updating_stem_drops_only_the_trailing_ellipsis() {
        assert_eq!(stem_without_ellipsis("更新中..."), "更新中");
        assert_eq!(stem_without_ellipsis("Updating..."), "Updating");
        assert_eq!(stem_without_ellipsis("Updating…"), "Updating");
        assert_eq!(stem_without_ellipsis("更新中"), "更新中");
    }
}
