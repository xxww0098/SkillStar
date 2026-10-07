//! `SelectSkillsPhase` — repo source row, filter, plugin hint, checkbox
//! list, install bar. One skill row per `DiscoveredSkill`, matching React's
//! `import-modal/SelectSkillsPhase.tsx` semantics (fresh-first selection,
//! advisory vs blocking frontmatter badges, hover-reveal reinstall).

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::repo_scanner::DiscoveredSkill;

use super::ImportDialog;
use super::phases::count_pill;
use crate::chrome::icon;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

impl ImportDialog {
    pub(super) fn render_select(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Some(scan) = &self.scan else {
            return div();
        };
        let view = cx.entity().downgrade();
        let query = self.filter_query.to_lowercase();
        let filtered: Vec<&DiscoveredSkill> = scan
            .skills
            .iter()
            .filter(|s| {
                query.is_empty()
                    || s.id.to_lowercase().contains(&query)
                    || s.description.to_lowercase().contains(&query)
            })
            .collect();

        // Fresh installs are the toggle target; when everything installable
        // is already installed, installed skills become the toggle targets
        // (React: `selectableFiltered` falls back to the installable set).
        let fresh: Vec<&DiscoveredSkill> = filtered
            .iter()
            .copied()
            .filter(|s| s.installable && !s.already_installed)
            .collect();
        let selectable: Vec<&DiscoveredSkill> = if fresh.is_empty() {
            filtered.iter().copied().filter(|s| s.installable).collect()
        } else {
            fresh
        };
        let all_selected =
            !selectable.is_empty() && selectable.iter().all(|s| self.selected.contains(&s.id));
        // Owned ids — the click closure is 'static and cannot borrow `scan`.
        let selectable_ids: Vec<String> = selectable.iter().map(|s| s.id.clone()).collect();
        let all_installed =
            !scan.skills.is_empty() && scan.skills.iter().all(|s| s.already_installed);
        let list_max_h = window.viewport_size().height * 0.38;

        // ── Header: source + count | quick-pack + select-all ──────
        let pack_view = view.clone();
        let all_view = view.clone();
        let refresh_view = view.clone();
        let skill_count = scan.skills.len();
        let head = div()
            .flex()
            .flex_col()
            .gap_3()
            .px(px(24.0))
            .pt(px(16.0))
            .pb_2()
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(icon(IconName::GitBranch, 14.0, palette().fg_muted))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(palette().fg_muted))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(scan.spec.short.clone()),
                            )
                            .child(count_pill(format!(
                                "{skill_count} skill{}",
                                if skill_count == 1 { "" } else { "s" }
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .flex_shrink_0()
                            .when(!self.selected.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("import-quick-pack")
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(palette().warn))
                                        .bg(rgb(palette().warn).alpha(0.10))
                                        .whitespace_nowrap()
                                        .child(icon(IconName::Package, 14.0, palette().warn))
                                        .child(crate::i18n::t("githubImportModal.quickPack"))
                                        .on_click(move |_, _window, cx| {
                                            let _ = pack_view.update(cx, |this, cx| {
                                                this.install_selected(true, cx)
                                            });
                                        })
                                        .interaction_spring(
                                            "import-quick-pack",
                                            true,
                                            MotionPaint::new().bg(rgb(palette().warn).alpha(0.10)),
                                            MotionPaint::new().bg(rgb(palette().warn).alpha(0.20)),
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .id("import-select-all")
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(palette().accent))
                                    .child(if all_selected {
                                        crate::i18n::t("common.deselectAll")
                                    } else {
                                        crate::i18n::t("common.selectAll")
                                    })
                                    .on_click(move |_, _window, cx| {
                                        let ids = selectable_ids.clone();
                                        let _ = all_view.update(cx, |this, cx| {
                                            if all_selected {
                                                for id in ids {
                                                    this.selected.remove(&id);
                                                }
                                            } else {
                                                for id in ids {
                                                    this.selected.insert(id);
                                                }
                                            }
                                            cx.notify();
                                        });
                                    })
                                    .interaction_spring(
                                        "import-select-all",
                                        true,
                                        MotionPaint::new().fg(rgb(palette().accent)),
                                        MotionPaint::new().fg(rgb(palette().accent_hover)),
                                    ),
                            ),
                    ),
            )
            .child(
                div().mt_3().child(
                    Input::new(&self.filter)
                        .prefix(div().pl(px(4.0)).flex().items_center().child(icon(
                            IconName::Search,
                            14.0,
                            palette().fg_muted,
                        )))
                        .cleanable(true),
                ),
            );

        // ── Cache source and explicit upstream refresh ──────────────
        let mut col = div().flex().flex_col().child(head);
        if let Some(fetched_at) = &scan.cached_at {
            let time = chrono::DateTime::parse_from_rfc3339(fetched_at)
                .map(|time| {
                    time.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_else(|_| fetched_at.clone());
            col = col.child(
                div()
                    .px(px(24.0))
                    .pb_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::tf(
                        if scan.cache_hit {
                            "githubImportModal.cachedSource"
                        } else {
                            "githubImportModal.fetchedSource"
                        },
                        &[("time", &time)],
                    ))
                    .child(
                        div()
                            .id("import-refresh-upstream")
                            .cursor_pointer()
                            .text_color(rgb(palette().accent))
                            .child(crate::i18n::t("githubImportModal.refreshUpstream"))
                            .on_click(move |_, _, cx| {
                                let _ = refresh_view.update(cx, |this, cx| {
                                    this.scan_with_refresh(
                                        this.scan_input.clone(),
                                        this.full_depth,
                                        true,
                                        cx,
                                    );
                                });
                            })
                            .interaction_spring(
                                "import-refresh-upstream",
                                true,
                                MotionPaint::new().fg(rgb(palette().accent)).opacity(1.0),
                                MotionPaint::new()
                                    .fg(rgb(palette().accent_hover))
                                    .opacity(1.0),
                            ),
                    ),
            );
        }

        // ── Plugin hint ────────────────────────────────────────────
        if scan.plugin.is_some() {
            col = col.child(
                div().px(px(24.0)).pb_2().flex_shrink_0().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_lg()
                        .bg(rgb(palette().well).alpha(0.60))
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(icon(IconName::Info, 14.0, palette().fg_muted))
                        .flex_shrink_0()
                        .child(crate::i18n::t("githubImportModal.claudePluginHint")),
                ),
            );
        }

        // ── Skill rows ─────────────────────────────────────────────
        let mut rows = div().flex().flex_col().gap(px(2.0));
        if filtered.is_empty() {
            rows = rows.child(
                div()
                    .py(px(32.0))
                    .text_center()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t(if scan.skills.is_empty() {
                        "githubImportModal.noSkillsFound"
                    } else {
                        "common.noResults"
                    })),
            );
        }
        for skill in filtered {
            rows = rows.child(self.skill_row(skill, view.clone()));
        }
        col = col.child(
            div()
                .px(px(24.0))
                .pb_2()
                .max_h(list_max_h)
                .overflow_y_scrollbar()
                .child(rows),
        );

        // ── Install bar ────────────────────────────────────────────
        let install_view = view.clone();
        let deep_view = view.clone();
        let reinstall_view = view.clone();
        let selected_count = self.selected.len();
        let full_depth = self.full_depth;
        let installable_ids: Vec<String> = scan
            .skills
            .iter()
            .filter(|s| s.installable)
            .map(|s| s.id.clone())
            .collect();
        col.child(
            div()
                .px(px(24.0))
                .py(px(14.0))
                .border_t_1()
                .border_color(rgb(palette().border_soft))
                .flex()
                .items_center()
                .justify_between()
                .flex_shrink_0()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(crate::i18n::tf(
                            "githubImportModal.selected",
                            &[("count", &selected_count.to_string())],
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("import-deep-scan")
                                .h(px(28.0))
                                .px_3()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(palette().border))
                                .cursor_pointer()
                                .text_xs()
                                .whitespace_nowrap()
                                .text_color(rgb(palette().fg))
                                .interaction_spring(
                                    "import-deep-scan",
                                    true,
                                    MotionPaint::new(),
                                    MotionPaint::new().bg(rgb(palette().panel_hover)),
                                )
                                .child(icon(IconName::ScanSearch, 14.0, palette().fg_muted))
                                .child(if full_depth {
                                    crate::i18n::t("githubImportModal.rescanFullDepth")
                                } else {
                                    crate::i18n::t("githubImportModal.fullDepthLabel")
                                })
                                .on_click(move |_, _window, cx| {
                                    let _ = deep_view.update(cx, |this, cx| this.deep_scan(cx));
                                }),
                        )
                        .when(all_installed, |d| {
                            d.child(
                                div()
                                    .id("import-reinstall-all")
                                    .h(px(28.0))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .text_color(rgb(palette().fg))
                                    .interaction_spring(
                                        "import-reinstall-all",
                                        true,
                                        MotionPaint::new(),
                                        MotionPaint::new().bg(rgb(palette().panel_hover)),
                                    )
                                    .child(icon(IconName::RotateCcw, 12.0, palette().fg_muted))
                                    .child(crate::i18n::t("githubImportModal.reinstallAll"))
                                    .on_click(move |_, _window, cx| {
                                        let ids = installable_ids.clone();
                                        let _ = reinstall_view.update(cx, |this, cx| {
                                            // `selectAll` filters to
                                            // installable ids in React.
                                            this.selected = ids.into_iter().collect();
                                            cx.notify();
                                        });
                                    }),
                            )
                        })
                        .child(
                            div()
                                .id("import-install")
                                .h(px(28.0))
                                .px(px(20.0))
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .rounded_md()
                                .bg(rgb(palette().accent))
                                .cursor_pointer()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(palette().on_accent))
                                .when(selected_count == 0, |d| d.opacity(0.5))
                                .child(icon(IconName::Download, 14.0, palette().on_accent))
                                .child(crate::i18n::t("githubImportModal.install"))
                                .on_click(move |_, _window, cx| {
                                    if selected_count == 0 {
                                        return;
                                    }
                                    let _ = install_view
                                        .update(cx, |this, cx| this.install_selected(false, cx));
                                })
                                .interaction_spring(
                                    "import-install",
                                    selected_count > 0,
                                    MotionPaint::new().bg(rgb(palette().accent)),
                                    MotionPaint::new().bg(rgb(if selected_count > 0 {
                                        palette().accent_hover
                                    } else {
                                        palette().accent
                                    })),
                                ),
                        ),
                ),
        )
    }

    /// One skill row — checkbox + id + badges + description, plus the
    /// hover-reveal reinstall glyph React shows on installed rows.
    fn skill_row(
        &self,
        skill: &DiscoveredSkill,
        view: WeakEntity<Self>,
    ) -> crate::chrome::MotionDiv {
        let id = skill.id.clone();
        let row_id: SharedString = format!("import-skill-{}", skill.folder_path).into();
        let selected = skill.installable && self.selected.contains(&skill.id);
        let installed = skill.already_installed;
        let issues: Vec<String> = skill
            .frontmatter_issues
            .iter()
            .map(|issue| {
                crate::i18n::t(&format!("githubImportModal.frontmatterIssues.{issue}")).to_string()
            })
            .collect();
        let issue_hint = (!issues.is_empty()).then(|| issues.join("; "));
        let blocking = !issues.is_empty() && !skill.installable;
        let group: SharedString = format!("import-row-{}", skill.folder_path).into();
        let toggle = view;

        div()
            .id(ElementId::Name(row_id.clone()))
            .group(group.clone())
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .py_2()
            .rounded_xl()
            .when(selected, |d| d.bg(rgb(palette().accent).alpha(0.05)))
            .when(!skill.installable, |d| d.opacity(0.7))
            .interaction_spring(
                row_id,
                skill.installable,
                if selected {
                    MotionPaint::new().bg(rgb(palette().accent).alpha(0.05))
                } else {
                    MotionPaint::new()
                },
                if selected || !skill.installable {
                    if selected {
                        MotionPaint::new().bg(rgb(palette().accent).alpha(0.05))
                    } else {
                        MotionPaint::new()
                    }
                } else {
                    MotionPaint::new().bg(rgb(palette().panel_hover))
                },
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_1()
                    .min_w_0()
                    // Checkbox — `w-4 h-4 rounded border-[1.5px]`
                    .child(
                        div()
                            .size(px(16.0))
                            .flex_shrink_0()
                            .rounded(px(4.0))
                            .border(px(1.5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(selected, |d| {
                                d.bg(rgb(palette().accent))
                                    .border_color(rgb(palette().accent))
                            })
                            .when(installed && !selected, |d| {
                                d.bg(rgb(palette().ok).alpha(0.20))
                                    .border_color(rgb(palette().ok).alpha(0.40))
                            })
                            .when(!selected && !installed, |d| {
                                d.border_color(rgb(palette().fg_muted).alpha(0.30))
                            })
                            .when(selected || installed, |d| {
                                d.child(icon(
                                    IconName::Check,
                                    10.0,
                                    if selected {
                                        palette().on_accent
                                    } else {
                                        palette().ok
                                    },
                                ))
                            }),
                    )
                    // Name + badges + description
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pr_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(if selected {
                                                palette().accent
                                            } else {
                                                palette().fg
                                            }))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(skill.id.clone()),
                                    )
                                    .when(installed && !selected, |d| {
                                        d.child(super::phases::badge(
                                            crate::i18n::t("githubImportModal.installed"),
                                            palette().ok,
                                            palette().ok_bg,
                                        ))
                                    })
                                    .when(selected && installed, |d| {
                                        d.child(super::phases::badge(
                                            crate::i18n::t("detailPanel.reinstall"),
                                            palette().warn,
                                            palette().warn_bg,
                                        ))
                                    })
                                    .when_some(issue_hint, |d, hint| {
                                        d.child(
                                            div()
                                                .id(ElementId::Name(
                                                    format!("import-issue-{}", skill.folder_path)
                                                        .into(),
                                                ))
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .px(px(6.0))
                                                .py(px(2.0))
                                                .rounded_full()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .flex_shrink_0()
                                                .bg(rgb(if blocking {
                                                    palette().danger_bg
                                                } else {
                                                    palette().warn_bg
                                                }))
                                                .text_color(rgb(if blocking {
                                                    palette().danger
                                                } else {
                                                    palette().warn
                                                }))
                                                .child(icon(
                                                    IconName::TriangleAlert,
                                                    10.0,
                                                    if blocking {
                                                        palette().danger
                                                    } else {
                                                        palette().warn
                                                    },
                                                ))
                                                .child(if blocking {
                                                    crate::i18n::t(
                                                        "githubImportModal.metadataIssue",
                                                    )
                                                } else {
                                                    crate::i18n::t(
                                                        "githubImportModal.compatibilityWarning",
                                                    )
                                                })
                                                .tooltip(move |window, cx| {
                                                    crate::chrome::tooltip(hint.clone())
                                                        .build(window, cx)
                                                }),
                                        )
                                    }),
                            )
                            .when(!skill.description.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(palette().fg_muted))
                                        .mt(px(2.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(skill.description.clone()),
                                )
                            }),
                    ),
            )
            // Hover-reveal reinstall button for already-installed rows.
            .when(installed, |d| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .opacity(if selected { 1.0 } else { 0.0 })
                        .group_hover(group, |s| s.opacity(1.0))
                        .child(icon(IconName::RotateCcw, 12.0, palette().fg_muted)),
                )
            })
            .when(skill.installable, |d| {
                d.cursor_pointer().on_click(move |_, _window, cx| {
                    let id = id.clone();
                    let _ = toggle.update(cx, |this, cx| {
                        if !this.selected.remove(&id) {
                            this.selected.insert(id);
                        }
                        cx.notify();
                    });
                })
            })
    }
}
