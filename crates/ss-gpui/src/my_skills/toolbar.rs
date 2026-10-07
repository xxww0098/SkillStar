//! My Skills toolbar. One row, matching `Toolbar` + `PageToolbar`:
//! title and scope icons, search, agent filter, compact source menu, then
//! import / attention / refresh / view. Refresh also checks upstream.

mod origin_menu;

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_skills::agents::AgentProfile;

use super::MySkillsPage;
use super::import_modal::ImportDialog;
use super::types::{MySkillsScope, SourceFilter};
use crate::chrome::{
    InteractionSpring, MotionPaint, bar_count, bar_icon_button, bar_refresh_button, icon,
    icon_spin, page_toolbar, segment_tab_compact, segment_track, toolbar_search,
    view_toggle_button,
};
use crate::theme::palette;
use origin_menu::{origin_menu_button, repo_sources};

fn needs_attention(skill: &Skill) -> bool {
    skill.update_available || skill.upstream_change.is_some()
}

impl MySkillsPage {
    pub fn render_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let view = cx.entity().downgrade();
        let scoped = self.scoped_skills();
        let filtered_count = if self.only_updates {
            scoped.iter().filter(|s| needs_attention(s)).count()
        } else {
            scoped.len()
        };
        let attention = scoped.iter().filter(|s| needs_attention(s)).count();
        let all_attention = self.skills.iter().filter(|s| needs_attention(s)).count();
        let pending_updates = self.skills.iter().filter(|s| s.update_available).count();
        let local_count = self
            .skills
            .iter()
            .filter(|s| s.skill_type == ss_core::types::skill::SkillType::Local)
            .count();
        let repos = repo_sources(&self.skills);
        let filtering = self.source_filter != SourceFilter::All || self.repo_filter.is_some();
        let show_origin = local_count > 0 || !repos.is_empty() || filtering;

        let mut bar = page_toolbar(crate::i18n::t("sidebar.skills"))
            .drag_id("my-skills-toolbar-drag")
            .extra(self.render_scope_switch(view.clone()));

        if let Some(search) = &self.search {
            bar = bar.search(toolbar_search(search, 200.0));
        }

        let enabled: Vec<&AgentProfile> = self.profiles.iter().filter(|p| p.enabled).collect();
        if !enabled.is_empty() {
            bar = bar.filter(self.render_agent_filter(view.clone(), &enabled));
        }

        if show_origin {
            bar = bar.filter(origin_menu_button(
                view.clone(),
                self,
                filtered_count,
                repos,
            ));
        } else {
            bar = bar.filter(bar_count(IconName::Layers, filtered_count.to_string()));
        }

        let import = view.clone();
        bar = bar.action(
            bar_icon_button("my-skills-import", IconName::Download, false)
                .tooltip(|window, cx| {
                    crate::chrome::tooltip(crate::i18n::t("toolbar.import")).build(window, cx)
                })
                .on_click(move |_, window, cx| open_import(import.clone(), window, cx)),
        );

        if all_attention > 0 || self.only_updates {
            bar = bar.action(self.render_attention(view.clone(), attention, pending_updates));
        }

        // One control: a manual check toasts, then reloads the list. The
        // silent background check still runs after ordinary list loads.
        let refresh = view.clone();
        let syncing = self.checking_updates || self.loading;
        bar = bar.action(
            bar_refresh_button("my-skills-refresh", syncing)
                .tooltip(move |window, cx| {
                    crate::chrome::tooltip(crate::i18n::t(if syncing {
                        "toolbar.checkingUpdates"
                    } else {
                        "toolbar.refreshAndCheck"
                    }))
                    .build(window, cx)
                })
                .on_click(move |_, _, cx| {
                    let _ = refresh.update(cx, |this, cx| {
                        if this.checking_updates || this.loading {
                            return;
                        }
                        this.loading = true;
                        this.revise(cx);
                        this.check_updates(true, cx);
                    });
                }),
        );

        let grid = view.clone();
        let list = view;
        bar.action(
            segment_track()
                .child(
                    view_toggle_button(
                        "my-skills-view-grid",
                        IconName::LayoutGrid,
                        !self.view_list,
                    )
                    .on_click(move |_, _, cx| {
                        let _ = grid.update(cx, |this, cx| {
                            this.view_list = false;
                            this.reset_list_scroll();
                            this.revise(cx);
                        });
                    }),
                )
                .child(
                    view_toggle_button("my-skills-view-list", IconName::List, self.view_list)
                        .on_click(move |_, _, cx| {
                            let _ = list.update(cx, |this, cx| {
                                this.view_list = true;
                                this.reset_list_scroll();
                                this.revise(cx);
                            });
                        }),
                ),
        )
        .build()
    }

    /// Icon-only Local / Shared / Remote, same order and active tint as
    /// `MySkillsScopeSwitch` (`bg-primary/20` + primary glyph).
    fn render_scope_switch(&self, view: WeakEntity<Self>) -> Div {
        let order = [
            (MySkillsScope::Local, IconName::Laptop, "scope-local"),
            (
                MySkillsScope::Channels,
                IconName::UsersRound,
                "scope-shared",
            ),
            (MySkillsScope::Remote, IconName::Server, "scope-remote"),
        ];
        let mut track = div()
            .flex()
            .items_center()
            .p(px(2.0))
            .rounded_lg()
            .border_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().bg))
            .occlude();
        for (scope, glyph, id) in order {
            let active = self.scope == scope;
            let v = view.clone();
            track = track.child(
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(if active {
                        palette().accent_soft_edge
                    } else {
                        palette().bg
                    }))
                    .child(icon(
                        glyph,
                        14.0,
                        if active {
                            palette().accent
                        } else {
                            palette().fg_muted
                        },
                    ))
                    .when(active, |d| d.bg(rgb(palette().accent_soft)))
                    .on_click(move |_, _, cx| {
                        let _ = v.update(cx, |this, cx| {
                            this.scope = scope;
                            this.revise(cx);
                        });
                    })
                    .interaction_spring(
                        id,
                        true,
                        if active {
                            MotionPaint::new().bg(rgb(palette().accent_soft))
                        } else {
                            MotionPaint::new()
                        },
                        if active {
                            MotionPaint::new().bg(rgb(palette().accent_soft))
                        } else {
                            MotionPaint::new().bg(rgb(palette().panel_hover))
                        },
                    ),
            );
        }
        track
    }

    /// "All" plus one brand glyph per enabled agent. Clicking the active
    /// glyph clears the filter, matching `AgentFilterPill`.
    ///
    /// The glyph strip scrolls like the skill-card carousel
    /// (`overflow_x_scroll`): a vertical mouse wheel maps onto the
    /// horizontal axis. `overflow_x_scrollbar` locks that cross-axis, so
    /// the same wheel would not move these icons.
    fn render_agent_filter(&self, view: WeakEntity<Self>, profiles: &[&AgentProfile]) -> Div {
        let all = view.clone();
        let active_all = self.agent_filter.is_none();
        let mut icons = div()
            .id("my-skills-agent-filter-icons")
            .flex()
            .items_center()
            .gap_0()
            .h_full()
            .min_w_0()
            // Five slots. More agents scroll inside the strip instead of
            // widening the track over the search field.
            .max_w(px(22.0 * 5.0))
            .overflow_x_scroll();
        for profile in profiles {
            let id = profile.id.clone();
            let active = self.agent_filter.as_deref() == Some(profile.id.as_str());
            let v = view.clone();
            let pick = id.clone();
            icons = icons.child(
                div()
                    .id(ElementId::Name(format!("agent-filter-{id}").into()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(22.0))
                    .h_full()
                    .rounded_md()
                    .cursor_pointer()
                    .flex_shrink_0()
                    .when(active, |d| d.bg(rgb(palette().accent_soft)))
                    .child(
                        img(crate::agent_icons::agent_icon_path(&id))
                            .w(px(14.0))
                            .h(px(14.0))
                            .when(!active, |d| d.opacity(0.6)),
                    )
                    .on_click(move |_, _, cx| {
                        let pick = pick.clone();
                        let _ = v.update(cx, |this, cx| {
                            this.agent_filter =
                                if this.agent_filter.as_deref() == Some(pick.as_str()) {
                                    None
                                } else {
                                    Some(pick)
                                };
                            this.revise(cx);
                        });
                    })
                    .interaction_spring(
                        format!("agent-filter-{id}"),
                        true,
                        if active {
                            MotionPaint::new().bg(rgb(palette().accent_soft))
                        } else {
                            MotionPaint::new()
                        },
                        if active {
                            MotionPaint::new().bg(rgb(palette().accent_soft))
                        } else {
                            MotionPaint::new().bg(rgb(palette().panel_hover))
                        },
                    ),
            );
        }
        segment_track()
            .rounded_lg()
            .gap_0()
            .px(px(1.0))
            .child(
                segment_tab_compact(
                    "agent-filter-all",
                    crate::i18n::t("toolbar.all"),
                    active_all,
                )
                .on_click(move |_, _, cx| {
                    let _ = all.update(cx, |this, cx| {
                        this.agent_filter = None;
                        this.revise(cx);
                    });
                }),
            )
            .child(icons)
    }

    fn render_attention(&self, view: WeakEntity<Self>, attention: usize, pending: usize) -> Div {
        let pressed = self.only_updates;
        let updating = self.busy.as_deref() == Some("update_all");
        let toggle = view.clone();
        let update = view;
        let mut group = div()
            .flex()
            .items_center()
            .h(px(32.0))
            .flex_shrink_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(palette().warn))
            .overflow_hidden()
            .occlude()
            .child(
                div()
                    .id("filter-updates-only")
                    .flex()
                    .items_center()
                    .h_full()
                    .gap(px(6.0))
                    .px(px(10.0))
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(if pressed {
                        palette().on_warn
                    } else {
                        palette().fg
                    }))
                    .when(pressed, |d| d.bg(rgb(palette().warn)))
                    .child(icon(
                        IconName::ListFilter,
                        14.0,
                        if pressed {
                            palette().on_warn
                        } else {
                            palette().warn
                        },
                    ))
                    .child(attention.to_string())
                    .tooltip({
                        let pressed = pressed;
                        move |window, cx| {
                            crate::chrome::tooltip(crate::i18n::t(if pressed {
                                "toolbar.showAllSkills"
                            } else {
                                "toolbar.filterUpdatesHint"
                            }))
                            .build(window, cx)
                        }
                    })
                    .when(pressed, |d| {
                        d.child(icon(IconName::X, 12.0, palette().on_warn))
                    })
                    .on_click(move |_, _, cx| {
                        let _ = toggle.update(cx, |this, cx| {
                            this.only_updates = !this.only_updates;
                            this.revise(cx);
                        });
                    })
                    .interaction_spring(
                        "filter-updates-only",
                        true,
                        if pressed {
                            MotionPaint::new().bg(rgb(palette().warn))
                        } else {
                            MotionPaint::new()
                        },
                        if pressed {
                            MotionPaint::new().bg(rgb(palette().warn))
                        } else {
                            MotionPaint::new().bg(rgb(palette().card_hover))
                        },
                    ),
            );
        if pending > 0 {
            group = group
                .child(div().w(px(1.0)).h(px(16.0)).bg(rgb(palette().warn)))
                .child(
                    div()
                        .id("my-skills-update-all")
                        .flex()
                        .items_center()
                        .h_full()
                        .gap(px(6.0))
                        .px(px(10.0))
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(palette().warn))
                        .child(icon_spin(
                            "my-skills-update-all-spin",
                            IconName::CircleArrowUp,
                            14.0,
                            palette().warn,
                            updating,
                        ))
                        .child(crate::i18n::t("common.update"))
                        .tooltip({
                            let count = pending.to_string();
                            move |window, cx| {
                                let tip = if updating {
                                    crate::i18n::t("common.updating")
                                } else {
                                    crate::i18n::tf(
                                        "toolbar.updateAllHint",
                                        &[("count", count.as_str())],
                                    )
                                };
                                crate::chrome::tooltip(tip).build(window, cx)
                            }
                        })
                        .on_click(move |_, _, cx| {
                            let _ = update.update(cx, |this, cx| this.update_all_pending(cx));
                        })
                        .interaction_spring(
                            "my-skills-update-all",
                            !updating,
                            MotionPaint::new(),
                            MotionPaint::new().bg(rgb(palette().warn_hover)),
                        ),
                );
        }
        group
    }
}

/// React `ImportModal` — the entity renders its own `ModalHeader` + per-phase
/// bodies, so the `Dialog` only supplies overlay, panel chrome (X, Escape,
/// backdrop) and width. `p_0` strips the component padding; our phases apply
/// the React `px-6` rhythm themselves.
fn open_import(view: WeakEntity<MySkillsPage>, window: &mut Window, cx: &mut App) {
    // `open_dialog`'s builder runs on every dialog-layer paint. Creating the
    // view inside it discards the URL field (and anything typed into it) each
    // frame, so the caret never stays and the box looks dead.
    let entity = cx.new(|cx| ImportDialog::new(view, window, cx));
    let weak = entity.downgrade();
    crate::chrome::open_centered(
        window,
        cx,
        420.0,
        crate::chrome::DialogChrome::Flush,
        move |dialog, frame, _, _| {
            // `.modal-surface`: rounded-xl, sidebar fill (card on paper).
            // The kit already animates the drop shadow; don't replace it.
            let surface = if crate::theme::is_light() {
                palette().card
            } else {
                palette().panel
            };
            dialog
                .w(px(512.0))
                .p_0()
                .rounded(px(12.0))
                .bg(rgb(surface))
                .border_color(rgb(palette().border))
                .child(frame.measure(entity.clone()))
                // Escape/backdrop/X all run `on_close`; `Drop` on the entity is
                // the belt-and-suspenders cancel for in-flight git work.
                .on_close({
                    let weak = weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |this, _| this.cancel_active());
                    }
                })
        },
    );
}
