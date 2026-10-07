//! Selection pill for batch skill operations (Link, Update, Uninstall, Clear).
//!
//! The bar is a floating pill pinned to the bottom-center of the card pane,
//! not a band: selection is transient context, so it overlays the grid like
//! the back-to-top control and the bottom-center toast instead of pushing
//! content down when the first card is checked.

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::MySkillsPage;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

impl MySkillsPage {
    /// Full-width overlay strip that centers the selection pill on the pane's
    /// bottom edge. Painted inside the scroll region's parent so it hovers
    /// above the cards; the strip itself has no handlers, so clicks and scroll
    /// fall through everywhere except on the pill.
    pub fn render_selection_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_count = self.selected_batch.len();
        if selected_count == 0 {
            return div().into_any_element();
        }

        let filtered_skills = self.filtered_skills();
        let all_selected = !filtered_skills.is_empty()
            && filtered_skills
                .iter()
                .all(|s| self.selected_batch.contains(&s.name));
        let some_selected = selected_count > 0 && !all_selected;

        let view = cx.entity().downgrade();
        let is_busy = self.busy.is_some();
        let updates_in_selection = self
            .skills
            .iter()
            .filter(|s| self.selected_batch.contains(&s.name) && s.update_available)
            .count();

        // The pill floats on the grid. A normal hitbox does not hide the card
        // behind it, so the card would take the hover face. Block that hover
        // and those clicks; the wheel still scrolls the list.
        let pill = div()
            .block_mouse_except_scroll()
            .flex()
            .items_center()
            .gap_1()
            .pl_1p5()
            .pr_2()
            .py_1p5()
            .rounded_full()
            .border_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().card))
            .shadow_lg()
            // Count: the one place accent color earns its emphasis.
            .child(
                div()
                    .px_2p5()
                    .py_0p5()
                    .rounded_full()
                    .bg(rgb(palette().accent))
                    .text_xs()
                    .font_semibold()
                    .text_color(rgb(palette().on_accent))
                    .child(crate::i18n::tf(
                        "selectionBar.selected",
                        &[("count", &selected_count.to_string())],
                    )),
            )
            // Select All / Deselect All stays a quiet toggle, not an action.
            .child(self.render_select_all(view.clone(), all_selected, some_selected))
            .child(pill_divider())
            // Batch commands
            .child(self.render_link_menu(view.clone(), is_busy))
            .child(self.render_update_button(view.clone(), is_busy, updates_in_selection))
            .child(self.render_uninstall_button(view.clone(), is_busy, selected_count))
            .child(pill_divider())
            // Clear selection — icon-only, tooltip names it.
            .child(
                pill_button("selection-clear")
                    .tooltip(move |window, cx| {
                        crate::chrome::tooltip(crate::i18n::t("common.clear")).build(window, cx)
                    })
                    .child(
                        Icon::new(IconName::X)
                            .with_size(px(13.0))
                            .text_color(rgb(palette().fg_muted)),
                    )
                    .on_click({
                        let v = view.clone();
                        move |_, _, cx| {
                            let _ = v.update(cx, |this, cx| {
                                this.selected_batch.clear();
                                this.link_menu_open = false;
                                this.revise(cx);
                            });
                        }
                    }),
            );

        // Overlay fills the pane (no handlers → clicks fall through) and
        // anchors the pill to the bottom-center.
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_end()
            .justify_center()
            .pb_3()
            // Click-away layer for the open agent menu; painted under the pill.
            .when(self.link_menu_open, |d| {
                let v = view.clone();
                d.child(
                    div()
                        .id("selection-menu-dismiss")
                        .absolute()
                        .inset_0()
                        .on_click(move |_, _, cx| {
                            let _ = v.update(cx, |this, cx| {
                                this.link_menu_open = false;
                                this.revise(cx);
                            });
                        }),
                )
            })
            .child(pill)
            .into_any_element()
    }

    /// Quiet tri-state toggle: selects every filtered card, or clears the
    /// batch when all of them are already selected.
    fn render_select_all(
        &self,
        view: WeakEntity<Self>,
        all_selected: bool,
        some_selected: bool,
    ) -> impl IntoElement {
        let names: Vec<String> = self
            .filtered_skills()
            .iter()
            .map(|s| s.name.clone())
            .collect();

        pill_button("select-all-toggle-btn")
            .child(
                Icon::new(if all_selected {
                    IconName::SquareCheck
                } else if some_selected {
                    IconName::SquareMinus
                } else {
                    IconName::Square
                })
                .with_size(px(12.0))
                .text_color(rgb(if all_selected {
                    palette().accent
                } else {
                    palette().fg_muted
                })),
            )
            .child(if all_selected {
                crate::i18n::t("common.deselectAll")
            } else {
                crate::i18n::t("common.selectAll")
            })
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    if all_selected {
                        this.selected_batch.clear();
                    } else {
                        for n in &names {
                            this.selected_batch.insert(n.clone());
                        }
                    }
                    this.revise(cx);
                });
            })
    }

    /// Batch update. Disabled with a count of zero instead of running a no-op.
    fn render_update_button(
        &self,
        view: WeakEntity<Self>,
        is_busy: bool,
        updates_in_selection: usize,
    ) -> impl IntoElement {
        let enabled = !is_busy && updates_in_selection > 0;
        pill_button("batch-update-btn")
            .map(|b| {
                if enabled {
                    b.on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.batch_update_selected(cx);
                        });
                    })
                } else {
                    b.opacity(0.45)
                }
            })
            .child(
                Icon::new(IconName::RefreshCw)
                    .with_size(px(12.0))
                    .text_color(rgb(palette().fg_muted)),
            )
            .child(crate::i18n::t("common.update"))
            .when(updates_in_selection > 0, |b| {
                b.child(
                    div()
                        .px_1p5()
                        .rounded_full()
                        .bg(rgb(palette().accent_soft))
                        .text_xs()
                        .text_color(rgb(palette().accent_fg))
                        .child(updates_in_selection.to_string()),
                )
            })
    }

    /// Destructive batch uninstall — quiet danger styling, confirmed upstream.
    fn render_uninstall_button(
        &self,
        view: WeakEntity<Self>,
        is_busy: bool,
        selected_count: usize,
    ) -> impl IntoElement {
        let enabled = !is_busy;
        pill_button("batch-uninstall-btn")
            .text_color(rgb(palette().danger))
            .map(|b| {
                if enabled {
                    b.on_click(move |_, window, cx| {
                        let v_dialog = view.clone();
                        crate::chrome::open_confirm(
                            window,
                            cx,
                            crate::i18n::tf(
                                if selected_count == 1 {
                                    "uninstallDialog.title_one"
                                } else {
                                    "uninstallDialog.title_other"
                                },
                                &[("count", &selected_count.to_string())],
                            ),
                            crate::i18n::t("uninstallDialog.description"),
                            crate::i18n::t("uninstallDialog.confirmUninstall"),
                            true,
                            move |_, cx| {
                                let _ = v_dialog.update(cx, |this, cx| {
                                    this.batch_uninstall_selected(cx);
                                });
                                true
                            },
                        );
                    })
                } else {
                    b.opacity(0.45)
                }
            })
            .child(
                Icon::new(IconName::Trash)
                    .with_size(px(12.0))
                    .text_color(rgb(palette().danger)),
            )
            .child(crate::i18n::t("common.uninstall"))
    }

    /// Link-to-agent trigger plus the menu it owns. The menu opens upward —
    /// the pill sits on the pane's bottom edge.
    fn render_link_menu(&self, view: WeakEntity<Self>, is_busy: bool) -> impl IntoElement {
        let v = view.clone();
        let menu_open = self.link_menu_open;
        // Same agents as the card carousel: Settings switches decide the list.
        let enabled_profiles: Vec<_> =
            super::skill_card::targetable_agent_profiles(&self.profiles).collect();

        div()
            .relative()
            .child(
                pill_button("batch-link-trigger")
                    .when(menu_open, |b| b.bg(rgb(palette().accent_soft)))
                    .child(
                        Icon::new(IconName::Link2)
                            .with_size(px(12.0))
                            .text_color(rgb(if menu_open {
                                palette().accent_fg
                            } else {
                                palette().fg_muted
                            })),
                    )
                    .child(crate::i18n::t("selectionBar.linkToAgent"))
                    .child(
                        Icon::new(IconName::ChevronUp)
                            .with_size(px(11.0))
                            .text_color(rgb(palette().fg_muted)),
                    )
                    .map(|b| {
                        if is_busy {
                            b.opacity(0.45)
                        } else {
                            b.on_click(move |_, _, cx| {
                                let _ = v.update(cx, |this, cx| {
                                    this.link_menu_open = !this.link_menu_open;
                                    this.revise(cx);
                                });
                            })
                        }
                    }),
            )
            .when(menu_open && !is_busy, |d| {
                // Occlude the whole panel. Menu rows are ordinary hitboxes, and
                // those do not stop the skill card underneath from hovering.
                let mut menu = div()
                    .id("selection-link-menu")
                    .debug_selector(|| "selection-link-menu".into())
                    .occlude()
                    .absolute()
                    .bottom(px(40.0))
                    .left_0()
                    .w(px(224.0))
                    .p_1p5()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(palette().border))
                    .bg(rgb(palette().card))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .font_semibold()
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("selectionBar.linkToAgent")),
                    );

                if enabled_profiles.is_empty() {
                    menu = menu.child(
                        div()
                            .px_2()
                            .py_1p5()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("selectionBar.noAgents")),
                    );
                }

                for profile in enabled_profiles {
                    let agent_id = profile.id.clone();
                    let agent_name = profile.display_name.clone();
                    let icon_path = crate::agent_icons::agent_icon_path(&agent_id);
                    let v_action = view.clone();
                    menu = menu.child(
                        div()
                            .id(ElementId::Name(format!("link-to-{}", profile.id).into()))
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_2()
                            .py_1p5()
                            .rounded_md()
                            .text_xs()
                            .text_color(rgb(palette().fg))
                            .cursor_pointer()
                            .child(img(icon_path).w(px(16.0)).h(px(16.0)).flex_shrink_0())
                            .child(agent_name)
                            .on_click(move |_, _, cx| {
                                let agent_id = agent_id.clone();
                                let _ = v_action.update(cx, |this, cx| {
                                    this.link_menu_open = false;
                                    this.batch_link_to_agent(&agent_id, cx);
                                });
                            })
                            .interaction_spring(
                                format!("link-to-{}", profile.id),
                                true,
                                MotionPaint::new(),
                                MotionPaint::new().bg(rgb(palette().card_hover)),
                            ),
                    );
                }

                // Unlink all option
                let v_unlink = view.clone();
                menu = menu.child(
                    div()
                        .mt_1()
                        .pt_1()
                        .border_t_1()
                        .border_color(rgb(palette().border))
                        .child(
                            div()
                                .id("batch-unlink-all-btn")
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .px_2()
                                .py_1p5()
                                .rounded_md()
                                .text_xs()
                                .font_medium()
                                .text_color(rgb(palette().warn))
                                .cursor_pointer()
                                .child(
                                    Icon::new(IconName::Link2Off)
                                        .with_size(px(12.0))
                                        .text_color(rgb(palette().warn)),
                                )
                                .child(crate::i18n::t("selectionBar.unlinkAll"))
                                .on_click(move |_, _, cx| {
                                    let _ = v_unlink.update(cx, |this, cx| {
                                        this.link_menu_open = false;
                                        this.batch_unlink_selected(cx);
                                    });
                                })
                                .interaction_spring(
                                    "batch-unlink-all-btn",
                                    true,
                                    MotionPaint::new(),
                                    MotionPaint::new().bg(rgb(palette().warn_hover)),
                                ),
                        ),
                );

                d.child(menu)
            })
    }
}

/// One quiet button inside the pill: same 24px frame, icon+label gap and
/// hover treatment for every command, so the row reads as one group.
fn pill_button(id: &'static str) -> crate::chrome::MotionDiv {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_1p5()
        .h_6()
        .px_2()
        .rounded_full()
        .text_xs()
        .font_medium()
        .text_color(rgb(palette().fg))
        .cursor_pointer()
        .interaction_spring(
            id,
            true,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(palette().card_hover)),
        )
}

/// Hairline separating the selection controls from the batch commands.
fn pill_divider() -> Div {
    div().w(px(1.0)).h_4().mx_0p5().bg(rgb(palette().border))
}
