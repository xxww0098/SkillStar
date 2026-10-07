//! group-card. One expandable row per skill group, with its agent links,
//! duplicate/delete commands, and the skill rows it reveals.
//!
//! The outer box is `crate::skill_card::card_shell`. The page module keeps
//! state and commands; this file paints one card.

use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::skill_group::{SkillGroup, delete_group, duplicate_group};

use super::SkillCardsPage;
use crate::chrome::icon;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::layout::CARD_ROW_W;
use crate::skill_card::{CardFace, CardShell, CardWidth, card_shell};
use crate::theme::palette;

impl SkillCardsPage {
    pub(super) fn render_group_card(
        &self,
        group: &SkillGroup,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let id = group.id.clone();
        let expanded = self.expanded.as_deref() == Some(id.as_str());

        let glyph = group
            .icon
            .chars()
            .next()
            .or_else(|| group.name.chars().next())
            .unwrap_or('📦')
            .to_string();

        let total_skills = group.skills.len();
        let installed_count = group
            .skills
            .iter()
            .filter(|s| self.installed_skills.contains(*s))
            .count();
        let missing_count = total_skills.saturating_sub(installed_count);

        let dup_id = group.id.clone();
        let del_id = group.id.clone();
        let group_name = group.name.clone();

        let header_row = div()
            .flex()
            .items_start()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(44.0))
                    .rounded_xl()
                    .bg(rgb(palette().well))
                    .border_1()
                    .border_color(rgb(palette().edge))
                    .text_xl()
                    .child(glyph),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette().fg))
                            .truncate()
                            .child(group.name.clone()),
                    )
                    .child(if !group.description.is_empty() {
                        div()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .line_clamp(2)
                            .child(group.description.clone())
                    } else {
                        div()
                            .text_xs()
                            .italic()
                            .text_color(rgb(palette().fg_muted))
                            .opacity(0.6)
                            .child(crate::i18n::t("skillCards.noDescription"))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id(ElementId::Name(format!("dup-btn-{id}").into()))
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .interaction_spring(
                                format!("dup-btn-{id}"),
                                true,
                                MotionPaint::new(),
                                MotionPaint::new().bg(rgb(palette().card)),
                            )
                            .child(icon(IconName::Copy, 14.0, palette().fg_muted))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        if let Ok(_) = duplicate_group(&dup_id) {
                                            this.refresh(cx);
                                        }
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .id(ElementId::Name(format!("del-btn-{id}").into()))
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .interaction_spring(
                                format!("del-btn-{id}"),
                                true,
                                MotionPaint::new(),
                                MotionPaint::new().bg(rgb(palette().danger_hover)),
                            )
                            .child(icon(IconName::Trash, 14.0, palette().danger))
                            .on_click({
                                let view = view.clone();
                                let group_name = group_name.clone();
                                move |_, window, cx| {
                                    let view = view.clone();
                                    let del_id = del_id.clone();
                                    let group_name = group_name.clone();
                                    crate::chrome::open_confirm(
                                        window,
                                        cx,
                                        format!("Delete \"{group_name}\"?"),
                                        crate::i18n::t("skillCards.deleteDeckHint"),
                                        crate::i18n::t("common.delete"),
                                        true,
                                        move |_, cx| {
                                            let _ = view.update(cx, |this, cx| {
                                                if let Ok(()) = delete_group(&del_id) {
                                                    if this.expanded.as_deref() == Some(&del_id) {
                                                        this.expanded = None;
                                                    }
                                                    this.refresh(cx);
                                                }
                                            });
                                            true
                                        },
                                    );
                                }
                            }),
                    ),
            );

        let meta_row = div()
            .flex()
            .items_center()
            .justify_between()
            .pt_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py(px(2.0))
                    .rounded_md()
                    .bg(rgb(palette().card_hover))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().accent_fg))
                    .child(icon(IconName::Layers, 12.0, palette().accent))
                    .child(format!("{total_skills} skills")),
            )
            .when(missing_count > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .text_xs()
                        .text_color(rgb(palette().warn))
                        .child(icon(IconName::TriangleAlert, 12.0, palette().warn))
                        .child(format!("{missing_count} missing")),
                )
            });

        let max_visible_collapsed = 4;
        let visible_skills: Vec<&String> = if expanded {
            group.skills.iter().collect()
        } else {
            group.skills.iter().take(max_visible_collapsed).collect()
        };

        let mut chips = div().flex().flex_wrap().gap_1().py_1();
        for skill_name in visible_skills {
            let is_installed = self.installed_skills.contains(skill_name);
            chips = chips.child(
                div()
                    .px_2()
                    .py(px(2.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if is_installed {
                        palette().border
                    } else {
                        palette().warn_border
                    }))
                    .bg(rgb(if is_installed {
                        palette().card_hover
                    } else {
                        palette().warn_bg
                    }))
                    .text_xs()
                    .text_color(rgb(if is_installed {
                        palette().fg
                    } else {
                        palette().warn
                    }))
                    .child(skill_name.clone()),
            );
        }
        if !expanded && total_skills > max_visible_collapsed {
            chips = chips.child(
                div()
                    .px_1()
                    .py(px(2.0))
                    .rounded_md()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(format!("+{} more", total_skills - max_visible_collapsed)),
            );
        }

        let group_links: HashSet<String> = group
            .agent_links
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let mut agent_row = div()
            .mt_1()
            .pt_2()
            .border_t_1()
            .border_color(rgb(palette().border))
            .flex()
            .flex_col()
            .gap_1_5();

        let header_deploy = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t("skillCards.deployToAgent")),
            )
            .child(
                div()
                    .id(ElementId::Name(format!("deploy-all-{id}").into()))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py(px(2.0))
                    .rounded_md()
                    .bg(rgb(palette().accent))
                    .text_color(rgb(palette().on_accent))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .interaction_spring(
                        format!("deploy-all-{id}"),
                        true,
                        MotionPaint::new().opacity(1.0),
                        MotionPaint::new().opacity(0.9),
                    )
                    .child(icon(IconName::Rocket, 11.0, palette().on_accent))
                    .child(crate::i18n::t("skillCards.deployAll"))
                    .on_click({
                        let view = view.clone();
                        let group_clone = group.clone();
                        move |_, _, cx| {
                            let _ = view
                                .update(cx, |this, cx| this.deploy_all_for_deck(&group_clone, cx));
                        }
                    }),
            );

        agent_row = agent_row.child(header_deploy);

        let mut agent_chips = div().flex().flex_wrap().gap_1();
        for profile in &self.profiles {
            if !profile.enabled {
                continue;
            }
            let agent_id = profile.id.clone();
            let is_linked = group_links.contains(&agent_id);

            let pill = div()
                .id(ElementId::Name(
                    format!("agent-link-{id}-{agent_id}").into(),
                ))
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(2.0))
                .rounded_md()
                .border_1()
                .border_color(rgb(if is_linked {
                    palette().accent
                } else {
                    palette().border
                }))
                .bg(rgb(if is_linked {
                    palette().accent_soft
                } else {
                    palette().card_hover
                }))
                .text_xs()
                .cursor_pointer()
                .text_color(rgb(if is_linked {
                    palette().fg
                } else {
                    palette().fg_muted
                }))
                .interaction_spring(
                    format!("agent-link-{id}-{agent_id}"),
                    true,
                    MotionPaint::new()
                        .bg(rgb(if is_linked {
                            palette().accent_soft
                        } else {
                            palette().card_hover
                        }))
                        .fg(rgb(if is_linked {
                            palette().fg
                        } else {
                            palette().fg_muted
                        }))
                        .opacity(1.0),
                    MotionPaint::new()
                        .bg(rgb(if is_linked {
                            palette().accent_soft
                        } else {
                            palette().card_hover
                        }))
                        .fg(rgb(palette().fg))
                        .opacity(1.0),
                )
                .when(is_linked, |d| {
                    d.child(icon(IconName::Check, 10.0, palette().ok))
                })
                .child(profile.display_name.clone())
                .on_click({
                    let view = view.clone();
                    let group_clone = group.clone();
                    let agent_id = agent_id.clone();
                    move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.toggle_agent_for_deck(&group_clone, &agent_id, cx)
                        });
                    }
                });

            agent_chips = agent_chips.child(pill);
        }

        agent_row = agent_row.child(agent_chips);

        card_shell(CardShell {
            id: ElementId::Name(format!("deck-{id}").into()),
            width: if self.view_list {
                CardWidth::FillMax(CARD_ROW_W)
            } else {
                CardWidth::Fixed
            },
            face: CardFace::Deck,
            selected: expanded,
        })
        .self_start()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .cursor_pointer()
        .child(header_row.flex_shrink_0().px_3().pt_3())
        .child(
            div()
                .id(ElementId::Name(format!("deck-scroll-{id}").into()))
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .pb_3()
                .child(meta_row)
                .child(chips)
                .child(agent_row),
        )
        .on_click(move |_, _, cx| {
            let id = id.clone();
            let _ = view.update(cx, |this, cx| {
                this.expanded = if this.expanded.as_deref() == Some(id.as_str()) {
                    None
                } else {
                    Some(id)
                };
                cx.notify();
            });
        })
    }
}
