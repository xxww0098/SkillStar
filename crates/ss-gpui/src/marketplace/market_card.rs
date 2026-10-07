//! market-card. The leaderboard and the publisher skill grid share this body.
//!
//! The outer box is crate::skill_card::card_shell. The page that owns the
//! card decides whether it is selected and handles the click.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::{Skill, SkillCategory};

use super::card::market_tile_box;
use super::types::{avatar_palette, format_installs, icon, skill_blurb};
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::skill_card::skill_source_chip;
use crate::theme::palette;

/// Identity, one evidence line, and Install. Rank and HOT only when the
/// snapshot has them. An empty description still takes a line.
pub(crate) fn render_market_card(
    skill: &Skill,
    busy: bool,
    selected: bool,
    id_prefix: &str,
    cx: &App,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::chrome::MotionDiv {
    let (bg_color, fg_color) = avatar_palette(&skill.name);
    let initial = skill
        .name
        .chars()
        .next()
        .unwrap_or('S')
        .to_uppercase()
        .to_string();
    let installed = skill.installed;
    // The description row always renders. Empty copy falls back to the
    // shared no-description text, so the footer stays put in the fixed box.
    let original = skill_blurb(skill)
        .map(str::to_string)
        .unwrap_or_else(|| crate::i18n::t("skillCard.noDescription").to_string());
    let blurb = if skill_blurb(skill).is_some() {
        crate::translation::display(&original, crate::translation::Surface::Description)
    } else {
        original.clone()
    };
    let translated = blurb != original;
    let stars = skill.stars;

    let mut tile = market_tile_box(
        ElementId::Name(format!("{id_prefix}-card-{}", skill.name).into()),
        selected,
    )
    .flex()
    .flex_col()
    .gap_2()
    .p_3()
    .child(
        div()
            .flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(36.0))
                    .rounded_lg()
                    .bg(rgb(bg_color))
                    .border_1()
                    .border_color(rgb(palette().border))
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(fg_color))
                            .child(initial),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .id(ElementId::Name(
                                format!("{id_prefix}-name-{}", skill.name).into(),
                            ))
                            .flex()
                            .items_center()
                            .gap_1()
                            .min_w_0()
                            // Both the name and the source line below can
                            // truncate; the tooltip carries each in full.
                            .tooltip({
                                let name = skill.name.clone();
                                let source = skill.source.clone().unwrap_or_default();
                                move |window, cx| {
                                    crate::chrome::tooltip(format!("{name}\n{source}"))
                                        .build(window, cx)
                                }
                            })
                            .when(skill.rank.is_some_and(|rank| rank <= 100), |d| {
                                let rank = skill.rank.unwrap_or(0);
                                d.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded_sm()
                                        .bg(rgb(palette().warn_bg))
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(palette().warn))
                                        .child(format!("#{rank}")),
                                )
                            })
                            .when(skill.category == SkillCategory::Hot, |d| {
                                d.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded_sm()
                                        .bg(rgb(palette().danger_bg))
                                        .text_size(px(9.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(palette().danger))
                                        .child(crate::i18n::t("marketplace.hot")),
                                )
                            })
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette().fg))
                                    .truncate()
                                    .child(skill.name.clone()),
                            ),
                    )
                    // The source line is the shared chip — the same control
                    // skill-card paints — so both surfaces name a repo the same
                    // way instead of one of them falling back to bare text.
                    .when_some(skill_source_chip(id_prefix, skill), |d, chip| {
                        d.child(chip.render(cx, |url, app| app.open_url(&url)))
                    }),
            )
            .child(install_action(
                id_prefix,
                &skill.name,
                busy,
                installed,
                on_toggle,
            )),
    );

    tile = tile.child({
        let row = div()
            .text_xs()
            .text_color(rgb(palette().fg_muted))
            .line_clamp(2)
            .child(blurb);
        if translated {
            crate::translation::paint_card(row)
        } else {
            row.into_any_element()
        }
    });
    if stars > 0 {
        tile = tile.child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(rgb(palette().warn))
                .child(icon(IconName::Star, 12.0, palette().warn))
                .child(format_installs(stars)),
        );
    }

    if matches!(
        skill.upstream_change,
        Some(ss_core::types::UpstreamChange::Removed { .. })
    ) {
        tile = tile.child(exception_chip(
            "Removed",
            palette().danger,
            palette().danger_bg,
        ));
    } else if skill.update_available {
        tile = tile.child(exception_chip("Update", palette().warn, palette().warn_bg));
    }

    tile
}

fn exception_chip(label: &'static str, fg: u32, bg: u32) -> Div {
    div()
        .self_start()
        .px_1()
        .rounded_sm()
        .bg(rgb(bg))
        .text_size(px(10.0))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(fg))
        .child(label)
}

fn install_action(
    id_prefix: &str,
    name: &str,
    busy: bool,
    installed: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::chrome::MotionDiv {
    let key: SharedString = format!("{id_prefix}-act-{name}").into();
    let (rest_bg, hover_bg, fg) = if busy {
        (
            palette().card_hover,
            palette().card_hover,
            palette().fg_muted,
        )
    } else if installed {
        (
            palette().danger_bg,
            palette().danger_hover,
            palette().danger,
        )
    } else {
        (
            palette().accent,
            palette().accent_hover,
            palette().on_accent,
        )
    };
    let mut rest = MotionPaint::new().bg(rgb(rest_bg)).fg(rgb(fg));
    let mut hover = MotionPaint::new().bg(rgb(hover_bg)).fg(rgb(fg));
    if !busy && installed {
        let edge = rgb(palette().danger_border);
        rest = rest.border(edge);
        hover = hover.border(edge);
    }
    div()
        .id(ElementId::Name(key.clone()))
        .flex()
        .items_center()
        .gap_1()
        .flex_shrink_0()
        .px_2()
        .py(px(3.0))
        .rounded_md()
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .bg(rgb(rest_bg))
        .text_color(rgb(fg))
        .when(!busy && installed, |d| {
            d.border_1().border_color(rgb(palette().danger_border))
        })
        .when(busy, |d| d.child("…"))
        .when(!busy && installed, |d| {
            d.child(icon(IconName::Trash, 11.0, palette().danger))
                .child(crate::i18n::t("common.uninstall"))
        })
        .when(!busy && !installed, |d| {
            d.child(icon(IconName::Download, 11.0, palette().on_accent))
                .child(crate::i18n::t("common.install"))
        })
        .on_click(move |event, window, app| {
            app.stop_propagation();
            on_toggle(event, window, app);
        })
        .interaction_spring(key, !busy, rest, hover)
}
