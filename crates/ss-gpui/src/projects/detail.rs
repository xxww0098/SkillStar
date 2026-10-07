//! Project Detail view — headers, detected rule chips, per-agent
//! switch toggles, deploy modes, and Save & Sync footer.

use std::path::Path;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::projects::{ProjectDeployMode, ProjectEntry, remove_project, update_project_path};

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
                                crate::chrome::open_form_dialog(
                                    window,
                                    cx,
                                    "Change Project Path",
                                    crate::i18n::t("common.update"),
                                    false,
                                    160.0,
                                    move |_, _| {
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(Input::new(&path_input))
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
                        .on_click(move |_, window, cx| {
                            let view = remove_view.clone();
                            let name = proj_name_for_dialog.clone();
                            crate::chrome::open_confirm(
                                window,
                                cx,
                                format!("Remove project \"{name}\"?"),
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

    // ── Section 1: Detected Agent Rule Files Chips ───────────────────
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
                .gap_1_5()
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
                .child(format!("{} found", page.detected_rules.len())),
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
        let mut chips_row = div().flex().flex_wrap().gap_1_5();
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
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(rule.name.clone()),
                    ),
            );
        }
        rules_card = rules_card.child(chips_row);
    }

    body = body.child(rules_card);

    // ── Section 2: Deployment Mode Overview ──────────────────────────
    let has_copy_mode = page
        .deploy_modes
        .values()
        .any(|m| *m == ProjectDeployMode::Copy);
    let mode_card = div()
        .w_full()
        .flex_grow_0()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_2()
        .rounded_lg()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(icon(
                    if has_copy_mode { IconName::Copy } else { IconName::Link2 },
                    15.0,
                    if has_copy_mode { palette().warn } else { palette().accent_fg },
                ))
                .child(
                    div()
                        .text_xs()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(palette().fg))
                                .child(if has_copy_mode {
                                    "Deploy Mode: Standalone Copy mode active"
                                } else {
                                    "Deploy Mode: Symlink (Default - live link to Hub)"
                                }),
                        )
                        .child(
                            div()
                                .text_color(rgb(palette().fg_muted))
                                .child(if has_copy_mode {
                                    "Files are physically copied into project directories. Hub updates will not affect project."
                                } else {
                                    "Symlinks automatically reflect upstream skill updates from your local hub."
                                }),
                        ),
                ),
        )
        .child(
            div()
                .px_2()
                .py(px(2.0))
                .rounded_full()
                .border_1()
                .border_color(rgb(if has_copy_mode { palette().warn_border } else { palette().info_border }))
                .bg(rgb(if has_copy_mode { palette().warn_bg } else { palette().info_bg }))
                .text_xs()
                .text_color(rgb(if has_copy_mode { palette().warn } else { palette().accent_fg }))
                .child(if has_copy_mode { "Copy Mode" } else { "Strict Symlink" }),
        );

    body = body.child(mode_card);

    // ── Section 3: Agent Skill Configuration List ────────────────────
    let mut agents_card = div()
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

    agents_card = agents_card.child(
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
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(format!("{} agents configured", page.agent_skills.len())),
            ),
    );

    let mut agent_list = div()
        .w_full()
        .flex()
        .flex_col()
        .flex_nowrap()
        .justify_start()
        .gap_2();
    for profile in &page.profiles {
        if !profile.has_project_skills() {
            continue;
        }
        let agent_row = render_agent_item(profile, page, view.clone());
        agent_list = agent_list.child(agent_row);
    }

    agents_card = agents_card.child(agent_list);
    body = body.child(agents_card);

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
                                .child(format!("{total_assigned_skills} skills across {enabled_agents_count} agents")),
                        )
                        .when(page.dirty, |d| {
                            d.child(
                                div()
                                    .px_1_5()
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
        .child(
            div()
                .id("proj-save-sync")
                .flex()
                .items_center()
                .gap_1_5()
                .px_4()
                .py(px(7.0))
                .rounded_lg()
                .bg(rgb(if page.dirty { palette().accent } else { palette().edge }))
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
                }),
        );

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
