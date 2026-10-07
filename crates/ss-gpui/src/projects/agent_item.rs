//! Agent item row inside project detail — collapsible skills,
//! deploy mode switch (symlink vs copy), and activation toggle.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::agents::AgentProfile;
use ss_skills::projects::ProjectDeployMode;

use super::ProjectsPage;
use crate::chrome::icon;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

/// Pill-style Switch component for toggling agent activation.
pub fn render_switch(
    id: &str,
    enabled: bool,
    view: WeakEntity<ProjectsPage>,
    agent_id: String,
) -> impl IntoElement {
    div()
        .id(ElementId::Name(id.to_string().into()))
        .w(px(36.0))
        .h(px(20.0))
        .rounded_full()
        .cursor_pointer()
        .bg(rgb(if enabled {
            palette().accent
        } else {
            palette().border
        }))
        .flex()
        .items_center()
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .rounded_full()
                .bg(rgb(0xffffff))
                .ml(px(if enabled { 18.0 } else { 2.0 })),
        )
        .on_click(move |_, _, cx| {
            let agent_id = agent_id.clone();
            let _ = view.update(cx, |this, cx| {
                this.toggle_agent(&agent_id, cx);
            });
        })
        .interaction_spring(
            id.to_string(),
            true,
            MotionPaint::new().opacity(1.0),
            MotionPaint::new().opacity(1.0),
        )
}

/// Render a single Agent row inside the project detail view.
pub fn render_agent_item(
    profile: &AgentProfile,
    page: &ProjectsPage,
    view: WeakEntity<ProjectsPage>,
) -> impl IntoElement {
    let agent_id = profile.id.clone();
    let is_enabled = page.agent_skills.contains_key(&agent_id);
    let is_expanded = page.expanded_agent.as_deref() == Some(&agent_id);
    let skills = page
        .agent_skills
        .get(&agent_id)
        .cloned()
        .unwrap_or_default();

    let rel_path = profile.project_skills_rel.clone();
    let deploy_mode = page
        .deploy_modes
        .get(&rel_path)
        .copied()
        .unwrap_or(ProjectDeployMode::Symlink);
    let is_copy = deploy_mode == ProjectDeployMode::Copy;

    let mut row = div()
        .w_full()
        .flex_grow_0()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(rgb(if is_expanded {
            palette().accent
        } else {
            palette().edge
        }))
        .bg(rgb(palette().card));

    let expand_view = view.clone();
    let agent_id_for_expand = agent_id.clone();

    let top = div()
        .id(ElementId::Name(format!("agent-top-{agent_id}").into()))
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_2()
        .cursor_pointer()
        .interaction_spring(
            format!("agent-top-{agent_id}"),
            true,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().card_hover)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .flex_1()
                .child(icon(
                    if is_expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    },
                    14.0,
                    palette().fg_muted,
                ))
                .child(icon(
                    IconName::Bot,
                    16.0,
                    if is_enabled {
                        palette().accent_fg
                    } else {
                        palette().fg_muted
                    },
                ))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if is_enabled {
                            palette().fg
                        } else {
                            palette().fg_muted
                        }))
                        .child(profile.display_name.clone()),
                )
                .child(
                    div()
                        .px_1_5()
                        .py(px(1.0))
                        .rounded_sm()
                        .bg(rgb(palette().well))
                        .text_xs()
                        .font_family("JetBrains Mono")
                        .text_color(rgb(palette().fg_muted))
                        .child(rel_path.clone()),
                )
                .when(is_enabled, |d| {
                    d.child(
                        div()
                            .px_2()
                            .py(px(1.0))
                            .rounded_full()
                            .bg(rgb(palette().info_bg))
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(palette().accent_fg))
                            .child(format!("{} skills", skills.len())),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .when(is_enabled, |d| {
                    let mode_view = view.clone();
                    let rel_path_clone = rel_path.clone();
                    d.child(
                        div()
                            .id(ElementId::Name(format!("mode-toggle-{agent_id}").into()))
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py(px(2.0))
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(if is_copy {
                                palette().warn_border
                            } else {
                                palette().info_border
                            }))
                            .bg(rgb(if is_copy {
                                palette().warn_bg
                            } else {
                                palette().info_bg
                            }))
                            .text_xs()
                            .text_color(rgb(if is_copy {
                                palette().warn
                            } else {
                                palette().accent_fg
                            }))
                            .cursor_pointer()
                            .interaction_spring(
                                format!("mode-toggle-{agent_id}"),
                                true,
                                MotionPaint::new()
                                    .bg(rgb(if is_copy {
                                        palette().warn_bg
                                    } else {
                                        palette().info_bg
                                    }))
                                    .opacity(1.0),
                                MotionPaint::new()
                                    .bg(rgb(if is_copy {
                                        palette().warn_bg
                                    } else {
                                        palette().info_bg
                                    }))
                                    .opacity(0.9),
                            )
                            .child(icon(
                                if is_copy {
                                    IconName::Copy
                                } else {
                                    IconName::Link2
                                },
                                11.0,
                                if is_copy {
                                    palette().warn
                                } else {
                                    palette().accent_fg
                                },
                            ))
                            .child(if is_copy { "Copy" } else { "Symlink" })
                            .on_click(move |_, _, cx| {
                                let rel = rel_path_clone.clone();
                                let _ = mode_view.update(cx, |this, cx| {
                                    this.toggle_deploy_mode(&rel, cx);
                                });
                            }),
                    )
                })
                .child(render_switch(
                    &format!("switch-{agent_id}"),
                    is_enabled,
                    view.clone(),
                    agent_id.clone(),
                )),
        )
        .on_click(move |_, _, cx| {
            let id = agent_id_for_expand.clone();
            let _ = expand_view.update(cx, |this, cx| {
                this.expanded_agent = if this.expanded_agent.as_deref() == Some(&id) {
                    None
                } else {
                    Some(id)
                };
                cx.notify();
            });
        });

    row = row.child(top);

    if is_expanded && is_enabled {
        let mut detail = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .border_t_1()
            .border_color(rgb(palette().edge))
            .bg(rgb(palette().panel));

        if skills.is_empty() {
            detail = detail.child(
                div()
                    .text_xs()
                    .italic()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t("projects.noSkillsAssigned")),
            );
        } else {
            let mut chips = div().flex().flex_wrap().gap_1_5();
            for skill_name in &skills {
                let remove_view = view.clone();
                let aid = agent_id.clone();
                let sname = skill_name.clone();

                chips = chips.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py(px(3.0))
                        .rounded_md()
                        .bg(rgb(palette().card))
                        .border_1()
                        .border_color(rgb(palette().border))
                        .text_xs()
                        .text_color(rgb(palette().fg))
                        .child(skill_name.clone())
                        .child(
                            div()
                                .id(ElementId::Name(format!("rem-btn-{aid}-{sname}").into()))
                                .p(px(1.0))
                                .rounded_sm()
                                .cursor_pointer()
                                .interaction_spring(
                                    format!("rem-btn-{aid}-{sname}"),
                                    true,
                                    MotionPaint::new().opacity(1.0),
                                    MotionPaint::new().fg(rgb(palette().danger)).opacity(1.0),
                                )
                                .child(icon(IconName::X, 11.0, palette().fg_muted))
                                .on_click(move |_, _, cx| {
                                    let aid = aid.clone();
                                    let sname = sname.clone();
                                    let _ = remove_view.update(cx, |this, cx| {
                                        this.remove_skill_from_agent(&aid, &sname, cx);
                                    });
                                }),
                        ),
                );
            }
            detail = detail.child(chips);
        }

        let available_skills: Vec<&String> = page
            .hub_skills
            .iter()
            .filter(|s| !skills.contains(s))
            .collect();

        if !available_skills.is_empty() {
            let mut add_section = div()
                .flex()
                .flex_col()
                .gap_1_5()
                .pt_2()
                .border_t_1()
                .border_color(rgb(palette().well));

            let add_all_view = view.clone();
            let aid_for_add_all = agent_id.clone();
            let all_available_names: Vec<String> =
                available_skills.iter().map(|s| (*s).clone()).collect();

            let add_header = div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(palette().fg_muted))
                        .child(crate::i18n::t("projects.addFromHub")),
                )
                .child(
                    div()
                        .id(ElementId::Name(
                            format!("add-all-btn-{aid_for_add_all}").into(),
                        ))
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(rgb(palette().accent_fg))
                        .cursor_pointer()
                        .interaction_spring(
                            format!("add-all-btn-{aid_for_add_all}"),
                            true,
                            MotionPaint::new().fg(rgb(palette().accent_fg)).opacity(1.0),
                            MotionPaint::new().fg(rgb(palette().fg)).opacity(1.0),
                        )
                        .child(crate::i18n::t("projects.addAllAvailable"))
                        .on_click(move |_, _, cx| {
                            let aid = aid_for_add_all.clone();
                            let names = all_available_names.clone();
                            let _ = add_all_view.update(cx, |this, cx| {
                                for name in names {
                                    this.add_skill_to_agent(&aid, name, cx);
                                }
                            });
                        }),
                );

            add_section = add_section.child(add_header);

            let mut add_chips = div().flex().flex_wrap().gap_1();
            for skill_name in available_skills {
                let add_view = view.clone();
                let aid = agent_id.clone();
                let sname = (*skill_name).clone();

                add_chips = add_chips.child(
                    div()
                        .id(ElementId::Name(format!("add-chip-{aid}-{sname}").into()))
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py(px(2.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette().border))
                        .bg(rgb(palette().card))
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .cursor_pointer()
                        .interaction_spring(
                            format!("add-chip-{aid}-{sname}"),
                            true,
                            MotionPaint::new()
                                .bg(rgb(palette().card))
                                .fg(rgb(palette().fg_muted)),
                            MotionPaint::new()
                                .bg(rgb(palette().card_hover))
                                .fg(rgb(palette().accent_fg)),
                        )
                        .child(icon(IconName::Plus, 10.0, palette().accent))
                        .child(skill_name.clone())
                        .on_click(move |_, _, cx| {
                            let aid = aid.clone();
                            let sname = sname.clone();
                            let _ = add_view.update(cx, |this, cx| {
                                this.add_skill_to_agent(&aid, sname, cx);
                            });
                        }),
                );
            }

            add_section = add_section.child(add_chips);
            detail = detail.child(add_section);
        }

        row = row.child(detail);
    }

    row
}
