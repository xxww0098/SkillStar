//! Project Detail view — headers, detected rule chips, per-agent
//! switch toggles, deploy modes, and Save & Sync footer.

use std::path::Path;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::projects::{ProjectEntry, remove_project, update_project_path};

use super::{ProjectsPage, render_agent_item};
use crate::chrome::icon;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

#[derive(Clone, Debug)]
pub struct DetectedRule {
    pub name: String,
}

/// Inspect the project directory for standard agent rule files and configs.
pub fn scan_project_rules(project_path: &str) -> Vec<DetectedRule> {
    let root = Path::new(project_path);
    let mut rules = Vec::new();

    let candidates = [
        ("CLAUDE.md", "CLAUDE.md"),
        (".cursorrules", ".cursorrules"),
        (".cursor/rules", ".cursor/rules"),
        (".windsurfrules", ".windsurfrules"),
        (".windsurf/rules", ".windsurf/rules"),
        ("AGENTS.md", "AGENTS.md"),
        (".agents/skills", ".agents/skills"),
        (".claude/skills", ".claude/skills"),
        (".github/copilot-instructions.md", "copilot-instructions.md"),
        (".gemini", ".gemini"),
        (".vscode", ".vscode"),
    ];

    for (rel_path, display_name) in candidates {
        if root.join(rel_path).exists() {
            rules.push(DetectedRule {
                name: display_name.to_string(),
            });
        }
    }

    rules
}

/// Render the detail panel for the currently selected project.
pub fn render_project_detail(
    project: &ProjectEntry,
    page: &ProjectsPage,
    view: WeakEntity<ProjectsPage>,
) -> impl IntoElement {
    let project_name = project.name.clone();
    let project_path = project.path.clone();

    let mut col = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .h_full();

    // ── Header Bar ───────────────────────────────────────────────────
    let change_view = view.clone();
    let remove_view = view.clone();
    let proj_name_for_dialog = project_name.clone();

    let header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .p_4()
        .border_b_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .size(px(40.0))
                        .rounded_xl()
                        .bg(rgb(palette().accent_soft))
                        .border_1()
                        .border_color(rgb(palette().accent))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(IconName::FolderOpen, 20.0, palette().accent_fg)),
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
                                .child(project_name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(palette().fg_muted))
                                .font_family("JetBrains Mono")
                                .truncate()
                                .child(project_path.clone()),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    // "Change Path" button
                    div()
                        .id("proj-change-path")
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_3()
                        .py(px(5.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette().border))
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .cursor_pointer()
                        .interaction_spring(
                            "proj-change-path",
                            true,
                            MotionPaint::new().fg(rgb(palette().fg_muted)),
                            MotionPaint::new()
                                .bg(rgb(palette().card_hover))
                                .fg(rgb(palette().fg)),
                        )
                        .child(icon(IconName::Folder, 12.0, palette().fg_muted))
                        .child(crate::i18n::t("projects.changePath"))
                        .on_click({
                            let project_name = project_name.clone();
                            let current_path = project_path.clone();
                            move |_, window, cx| {
                                let view = change_view.clone();
                                let project_name = project_name.clone();
                                let current_path = current_path.clone();
                                let path_input = cx.new(|cx| {
                                    InputState::new(window, cx).default_value(current_path.clone())
                                });
                                let submitted = path_input.clone();
                                // The browse button fills the input on a later
                                // paint, so it carries its own handle.
                                let browse_input = path_input.clone();
                                crate::chrome::open_form_dialog(
                                    window,
                                    cx,
                                    crate::i18n::t("projects.changePathTitle"),
                                    crate::i18n::t("common.update"),
                                    false,
                                    move |_, _| {
                                        let browse = div()
                                            .debug_selector(|| "project-change-path-browse".into())
                                            .child({
                                                let path_input = browse_input.clone();
                                                Button::new("project-change-path-browse")
                                                    .label(crate::i18n::t("projects.browseFolder"))
                                                    .on_click(move |_, window, cx| {
                                                        let receiver = cx.prompt_for_paths(
                                                            PathPromptOptions {
                                                                files: false,
                                                                directories: true,
                                                                multiple: false,
                                                                prompt: Some(crate::i18n::t(
                                                                    "projects.chooseDir",
                                                                )),
                                                            },
                                                        );
                                                        let path_input = path_input.clone();
                                                        window
                                                            .spawn(cx, async move |cx| {
                                                                let picked = receiver
                                                                    .await
                                                                    .ok()
                                                                    .and_then(|r| r.ok())
                                                                    .flatten()
                                                                    .and_then(|p| {
                                                                        p.into_iter().next()
                                                                    });
                                                                if let Some(path) = picked {
                                                                    let _ =
                                                                        cx.update(|window, cx| {
                                                                            let text = path
                                                                                .to_string_lossy()
                                                                                .to_string();
                                                                            path_input.update(
                                                                                cx,
                                                                                |state, cx| {
                                                                                    state.set_value(
                                                                                        text,
                                                                                        window, cx,
                                                                                    )
                                                                                },
                                                                            );
                                                                            window.refresh();
                                                                        });
                                                                }
                                                            })
                                                            .detach();
                                                    })
                                            });
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .w_full()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .gap_2()
                                                    .w_full()
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .child(Input::new(&path_input)),
                                                    )
                                                    .child(browse),
                                            )
                                            .into_any_element()
                                    },
                                    move |_, cx| {
                                        let new_path =
                                            submitted.read(cx).value().trim().to_string();
                                        if new_path.is_empty() {
                                            return false;
                                        }
                                        let _ = view.update(cx, |this, cx| {
                                            if let Ok(_) =
                                                update_project_path(&project_name, &new_path)
                                            {
                                                this.refresh(cx);
                                            }
                                        });
                                        true
                                    },
                                );
                            }
                        }),
                )
                .child(
                    // "Remove Project" button
                    div()
                        .id("proj-remove")
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py(px(5.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette().danger_border))
                        .text_xs()
                        .text_color(rgb(palette().danger))
                        .cursor_pointer()
                        .interaction_spring(
                            "proj-remove",
                            true,
                            MotionPaint::new(),
                            MotionPaint::new().bg(rgb(palette().danger_hover)),
                        )
                        .child(icon(IconName::Trash, 12.0, palette().danger))
                        .tooltip(|window, cx| {
                            crate::chrome::tooltip(crate::i18n::t("projects.removeProject"))
                                .build(window, cx)
                        })
                        .on_click(move |_, window, cx| {
                            let view = remove_view.clone();
                            let name = proj_name_for_dialog.clone();
                            crate::chrome::open_confirm(
                                window,
                                cx,
                                crate::i18n::tf(
                                    "projects.removeConfirmTitle",
                                    &[("name", name.as_str())],
                                ),
                                crate::i18n::t("projects.unregisterHint"),
                                crate::i18n::t("projects.removeProject"),
                                true,
                                move |_, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        if let Ok(()) = remove_project(&name) {
                                            if this
                                                .selected_project
                                                .as_ref()
                                                .map(|p| p.name.as_str())
                                                == Some(&name)
                                            {
                                                this.selected_project = None;
                                            }
                                            this.refresh(cx);
                                        }
                                    });
                                    true
                                },
                            );
                        }),
                ),
        );

    col = col.child(header);

    // Scroll is the flex_1 pane. Sections live in a content-sized column
    // so a single rules/agent block cannot stretch to the pane height.
    let mut body = div()
        .w_full()
        .flex_grow_0()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .flex_nowrap()
        .justify_start()
        .gap_4()
        .p_4();

    // ── Section 1: Per-Agent Skill Management (the page's primary task) ──
    let mut agents_card = div()
        .debug_selector(|| "agents-card".into())
        .w_full()
        .flex_grow_0()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded_xl()
        .bg(rgb(palette().card))
        .border_1()
        .border_color(rgb(palette().border));

    // Same targetable set as the card rails: only agents the user enabled in
    // Settings, further limited to the ones that accept project skills.
    let visible_agents: Vec<_> = page
        .profiles
        .iter()
        .filter(|profile| profile.has_project_skills() && profile.enabled)
        .collect();

    // The working state counts agents switched on for this project, while the
    // list shows every candidate, so the counter names both numbers.
    let enabled_agents_count = page.agent_skills.len();
    agents_card =
        agents_card.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(palette().fg))
                        .child(crate::i18n::t("projects.perAgentTitle")),
                )
                .when(!visible_agents.is_empty(), |d| {
                    d.child(div().text_xs().text_color(rgb(palette().fg_muted)).child(
                        crate::i18n::tf(
                            "projects.agentsEnabledTotal",
                            &[
                                ("enabled", &enabled_agents_count.to_string()),
                                ("total", &visible_agents.len().to_string()),
                            ],
                        ),
                    ))
                }),
        );

    if visible_agents.is_empty() {
        agents_card = agents_card.child(
            div()
                .text_xs()
                .italic()
                .text_color(rgb(palette().fg_muted))
                .child(crate::i18n::t("projects.noAgentsEnabled")),
        );
    }
    let mut agent_list = div()
        .w_full()
        .flex()
        .flex_col()
        .flex_nowrap()
        .justify_start()
        .gap_2();
    for profile in visible_agents {
        let agent_row = render_agent_item(profile, page, view.clone());
        agent_list = agent_list.child(agent_row);
    }

    agents_card = agents_card.child(agent_list);
    body = body.child(agents_card);

    // ── Section 2: Deploy Mode Legend ────────────────────────────────────
    // Static explainer for the per-row mode capsules; the current mode is
    // always visible on the rows themselves, so nothing is summarized here.
    let mode_row =
        |icon_name: IconName, icon_color: u32, name_key: &'static str, hint_key: &'static str| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(icon(icon_name, 13.0, icon_color))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(palette().fg))
                        .child(crate::i18n::t(name_key)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(crate::i18n::t(hint_key)),
                )
        };
    let mode_card = div()
        .w_full()
        .flex_grow_0()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("projects.deployMode")),
        )
        .child(mode_row(
            IconName::Link2,
            palette().accent_fg,
            "projects.deploySymlink",
            "projects.deployModeSymlinkHint",
        ))
        .child(mode_row(
            IconName::Copy,
            palette().warn,
            "projects.deployCopy",
            "projects.deployModeCopyHint",
        ));

    body = body.child(mode_card);

    // ── Section 3: Detected Agent Rule Files Chips ───────────────────────
    let mut rules_card = div()
        .w_full()
        .flex_grow_0()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded_xl()
        .bg(rgb(palette().card))
        .border_1()
        .border_color(rgb(palette().border));

    let rules_header = div()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(icon(IconName::Scan, 14.0, palette().accent_fg))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(palette().fg))
                        .child(crate::i18n::t("projects.rulesTitle")),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .child(crate::i18n::tf(
                    "projects.rulesFound",
                    &[("count", &page.detected_rules.len().to_string())],
                )),
        );

    rules_card = rules_card.child(rules_header);

    if page.detected_rules.is_empty() {
        rules_card = rules_card.child(
            div()
                .text_xs()
                .italic()
                .text_color(rgb(palette().fg_muted))
                .child(crate::i18n::t("projects.noRules")),
        );
    } else {
        let mut chips_row = div().flex().flex_wrap().gap_1();
        for rule in &page.detected_rules {
            chips_row = chips_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py(px(3.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette().edge))
                    .bg(rgb(palette().well))
                    .text_xs()
                    .child(icon(IconName::FileText, 12.0, palette().accent_fg))
                    .child(
                        div()
                            .font_family("JetBrains Mono")
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(rule.name.clone()),
                    ),
            );
        }
        rules_card = rules_card.child(chips_row);
    }

    body = body.child(rules_card);

    col = col.child(
        div()
            .flex()
            .flex_col()
            .justify_start()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scrollbar()
            .child(body),
    );

    // ── Section 4: Apply & Save Footer ────────────────────────────────
    let total_assigned_skills: usize = page.agent_skills.values().map(|v| v.len()).sum();
    let enabled_agents_count = page.agent_skills.len();

    let save_view = view.clone();
    let footer = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .p_4()
        .border_t_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(palette().fg))
                                .child(crate::i18n::tf(
                                    "projects.footerSummary",
                                    &[
                                        ("skills", &total_assigned_skills.to_string()),
                                        ("agents", &enabled_agents_count.to_string()),
                                    ],
                                )),
                        )
                        .when(page.dirty, |d| {
                            d.child(
                                div()
                                    .px_1()
                                    .py(px(1.0))
                                    .rounded_sm()
                                    .bg(rgb(palette().warn_bg))
                                    .border_1()
                                    .border_color(rgb(palette().warn_border))
                                    .text_xs()
                                    .text_color(rgb(palette().warn))
                                    .child(crate::i18n::t("projects.unsavedBadge")),
                            )
                        }),
                )
                .when_some(page.status_message.as_ref(), |d, msg| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(rgb(palette().accent_fg))
                            .child(msg.clone()),
                    )
                }),
        )
        .child(if page.dirty {
            div()
                .id("proj-save-sync")
                .debug_selector(|| "proj-save-sync".into())
                .flex()
                .items_center()
                .gap_1()
                .px_4()
                .py(px(7.0))
                .rounded_lg()
                .bg(rgb(palette().accent))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().on_accent))
                .cursor_pointer()
                .interaction_spring(
                    "proj-save-sync",
                    true,
                    MotionPaint::new().opacity(1.0),
                    MotionPaint::new().opacity(0.9),
                )
                .child(icon(IconName::Check, 14.0, palette().on_accent))
                .child(crate::i18n::t("projects.saveAndSync"))
                .on_click(move |_, _, cx| {
                    let _ = save_view.update(cx, |this, cx| {
                        this.save_and_sync_current(cx);
                    });
                })
                .into_any_element()
        } else {
            // Nothing to apply yet: keep the label visible but quiet and inert.
            div()
                .flex()
                .items_center()
                .gap_1()
                .px_4()
                .py(px(7.0))
                .rounded_lg()
                .border_1()
                .border_color(rgb(palette().border))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().fg_muted))
                .child(icon(IconName::Check, 14.0, palette().fg_muted))
                .child(crate::i18n::t("projects.saveAndSync"))
                .into_any_element()
        });

    col = col.child(footer);
    col
}

#[cfg(test)]
mod tests {
    use super::scan_project_rules;

    #[test]
    fn test_scan_project_rules_detected() {
        let temp = std::env::temp_dir().join(format!("test_rules_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp);
        let _ = std::fs::write(temp.join("CLAUDE.md"), "# Claude");
        let _ = std::fs::create_dir_all(temp.join(".cursor/rules"));

        let rules = scan_project_rules(temp.to_str().unwrap());
        let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"CLAUDE.md"));
        assert!(names.contains(&".cursor/rules"));

        let _ = std::fs::remove_dir_all(&temp);
    }
}
