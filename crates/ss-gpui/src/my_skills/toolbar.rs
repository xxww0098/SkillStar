//! My Skills toolbar. One row, matching `Toolbar` + `PageToolbar`:
//! title and scope icons, search, agent filter, compact source menu, then
//! attention / import / refresh / view. Refresh also checks upstream.

use std::rc::Rc;

mod origin_menu;

use gpui_kit::assets::IconName;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_skills::agents::AgentProfile;

use super::MySkillsPage;
use super::import_modal::ImportDialog;
use super::types::{MySkillsScope, SourceFilter};
use crate::chrome::{
    InteractionSpring, MotionPaint, SliderGeometry, SliderSegment, bar_count, bar_icon_button,
    bar_refresh_button, icon, icon_spin, page_toolbar, segment_tab_compact, segment_track,
    slider_segmented, toolbar_search,
};
use crate::theme::palette;
use origin_menu::{origin_menu_button, repo_sources};

/// One brand-glyph slot in the toolbar agent filter, and how many the
/// lane holds. Fixed geometry: the lane never scrolls and never widens.
const AGENT_FILTER_SLOT_W: f32 = 22.0;
const AGENT_FILTER_SLOTS: usize = 6;

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

        if all_attention > 0 || self.only_updates {
            bar = bar.action(self.render_attention(view.clone(), attention, pending_updates));
        }

        let import = view.clone();
        bar = bar.action(
            bar_icon_button("my-skills-import", IconName::Download, false)
                .tooltip(|window, cx| {
                    crate::chrome::tooltip(crate::i18n::t("toolbar.import")).build(window, cx)
                })
                .on_click(move |_, window, cx| open_import(import.clone(), window, cx)),
        );

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

        bar.action(
            TabBar::new("my-skills-view")
                .segmented()
                .selected_index(if self.view_list { 1 } else { 0 })
                .on_click(move |ix, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.view_list = *ix == 1;
                        this.reset_list_scroll();
                        this.revise(cx);
                    });
                })
                .child(
                    Tab::new()
                        .icon(IconName::LayoutGrid)
                        .flex_1()
                        .tooltip(|window, cx| {
                            crate::chrome::tooltip(crate::i18n::t("toolbar.viewGrid"))
                                .build(window, cx)
                        }),
                )
                .child(
                    Tab::new()
                        .icon(IconName::List)
                        .flex_1()
                        .tooltip(|window, cx| {
                            crate::chrome::tooltip(crate::i18n::t("toolbar.viewList"))
                                .build(window, cx)
                        }),
                ),
        )
        .build()
    }

    /// Icon-only Local / Shared / Remote, same order as
    /// `MySkillsScopeSwitch`. The sliding thumb is the shared segmented
    /// control (`chrome::segmented`), the same one the sidebar mode switcher
    /// uses.
    fn render_scope_switch(&self, view: WeakEntity<Self>) -> AnyElement {
        let selected = match self.scope {
            MySkillsScope::Local => 0,
            MySkillsScope::Channels => 1,
            MySkillsScope::Remote => 2,
        };
        let segments = vec![
            SliderSegment {
                id: "my-skills-scope-local",
                icon: Some(IconName::Laptop),
                label: None,
            },
            SliderSegment {
                id: "my-skills-scope-channels",
                icon: Some(IconName::UsersRound),
                label: None,
            },
            SliderSegment {
                id: "my-skills-scope-remote",
                icon: Some(IconName::Server),
                label: None,
            },
        ];
        slider_segmented(
            "my-skills-scope-motion",
            &segments,
            selected,
            // Toolbar height is 32px: pad 2 per side plus the border leaves
            // 26px slots, matching `segment_track`'s density.
            SliderGeometry {
                slot_w: 28.0,
                slot_h: 26.0,
                pad: 2.0,
                icon_size: 14.0,
            },
            false,
            Rc::new(move |ix, _, cx| {
                let scope = match ix {
                    1 => MySkillsScope::Channels,
                    2 => MySkillsScope::Remote,
                    _ => MySkillsScope::Local,
                };
                let _ = view.update(cx, |this, cx| {
                    this.scope = scope;
                    this.revise(cx);
                });
            }),
        )
    }

    /// "All" plus one brand glyph per enabled agent, six slots at most.
    /// The glyph lane holds a fixed six-slot width whether the roster is
    /// shorter or longer, so this filter never changes the toolbar length;
    /// glyphs past the sixth don't render. Clicking the active glyph clears
    /// the filter.
    fn render_agent_filter(&self, view: WeakEntity<Self>, profiles: &[&AgentProfile]) -> Div {
        let all = view.clone();
        let active_all = self.agent_filter.is_none();
        let mut icons = div()
            .flex()
            .items_center()
            .gap_0()
            .h_full()
            // Six fixed slots: the track keeps this length for any roster
            // size instead of widening over the search field or clipping a
            // seventh glyph at a scroll edge. Space pressure goes to the
            // search field, never here.
            .flex_shrink_0()
            .w(px(AGENT_FILTER_SLOT_W * AGENT_FILTER_SLOTS as f32));
        for profile in profiles.iter().take(AGENT_FILTER_SLOTS) {
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
                    .w(px(AGENT_FILTER_SLOT_W))
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

    /// One warn capsule, in the card update pill's palette (`warn_bg` fill,
    /// `warn_border` outline). The filter half reads like `bar_count` — glyph
    /// plus count — and stays quiet; the solid half is the update action.
    /// `segment_track` geometry: a 2px inset and each half's own `rounded_md`
    /// keep the fills concentric with the outline.
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
            .border_color(rgb(palette().warn_border))
            .bg(rgb(palette().warn_bg))
            .p(px(2.0))
            .gap(px(2.0))
            .occlude()
            .child(
                div()
                    .id("filter-updates-only")
                    .flex()
                    .items_center()
                    .h_full()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(palette().warn))
                    // The glyph slot swaps to X instead of appending one, so
                    // the count keeps its lane and the row stays two elements.
                    .child(icon(
                        if pressed {
                            IconName::X
                        } else {
                            IconName::ListFilter
                        },
                        14.0,
                        palette().warn,
                    ))
                    .child(attention.to_string())
                    .tooltip(move |window, cx| {
                        crate::chrome::tooltip(crate::i18n::t(if pressed {
                            "toolbar.showAllSkills"
                        } else {
                            "toolbar.filterUpdatesHint"
                        }))
                        .build(window, cx)
                    })
                    .when(pressed, |d| d.bg(rgb(palette().warn_hover)))
                    .on_click(move |_, _, cx| {
                        let _ = toggle.update(cx, |this, cx| {
                            this.only_updates = !this.only_updates;
                            this.revise(cx);
                        });
                    })
                    .interaction_spring(
                        "filter-updates-only",
                        true,
                        MotionPaint::new().bg(rgb(if pressed {
                            palette().warn_hover
                        } else {
                            palette().warn_bg
                        })),
                        MotionPaint::new().bg(rgb(palette().warn_hover)),
                    ),
            );
        if pending > 0 {
            group = group.child(
                div()
                    .id("my-skills-update-all")
                    .flex()
                    .items_center()
                    .h_full()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .bg(rgb(palette().warn))
                    .text_color(rgb(palette().on_warn))
                    .when(updating, |d| d.opacity(0.8))
                    .child(icon_spin(
                        IconName::CircleArrowUp,
                        14.0,
                        palette().on_warn,
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
                    // Solid fill, so the hover dims it like `bar_primary`
                    // instead of layering another tint over `warn`.
                    .interaction_spring(
                        "my-skills-update-all",
                        !updating,
                        MotionPaint::new().opacity(1.0),
                        MotionPaint::new().opacity(0.9),
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

#[cfg(test)]
mod scope_switch_tests {
    use gpui_kit::component::Root;
    use gpui_kit::{AppContext, point, px, size};

    use super::super::test_support::IsolatedDataDir;
    use super::MySkillsPage;
    use crate::my_skills::types::MySkillsScope;

    fn paint(cx: &mut gpui_kit::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn click(cx: &mut gpui_kit::VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} missing"));
        cx.simulate_click(
            point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            ),
            Default::default(),
        );
    }

    /// The scope switch rides the shared sliding thumb (`chrome::segmented`):
    /// the three pills mount in the toolbar and clicking one moves the page
    /// scope, the state the thumb then springs to.
    #[gpui_kit::test]
    fn scope_switch_clicks_move_the_page_scope(cx: &mut gpui_kit::TestAppContext) {
        let _dir = IsolatedDataDir::new();
        crate::init_test(cx);
        let page = cx.new(|cx| MySkillsPage::new(cx));
        let shown = page.clone();
        let (_root, cx) =
            cx.add_window_view(move |window, cx| Root::new(shown.clone(), window, cx));
        cx.simulate_resize(size(px(1200.), px(800.)));
        paint(cx);

        assert!(
            cx.debug_bounds("my-skills-scope-channels").is_some(),
            "scope pills missing from the toolbar"
        );
        click(cx, "my-skills-scope-channels");
        paint(cx);
        cx.update(|_, cx| {
            assert_eq!(page.read(cx).scope, MySkillsScope::Channels);
        });

        click(cx, "my-skills-scope-remote");
        paint(cx);
        cx.update(|_, cx| {
            assert_eq!(page.read(cx).scope, MySkillsScope::Remote);
        });
    }
}
