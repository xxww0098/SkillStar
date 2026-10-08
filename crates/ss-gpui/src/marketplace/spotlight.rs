//! Spotlight search — toolbar glyph and ⌘F trigger opening a suggestion
//! panel over the current list. Split out of `mod.rs` to keep that file
//! under the line cap.

use gpui_kit::assets::IconName;
use gpui_kit::component::Selectable;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::MarketplacePage;
use super::types::icon;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

/// Suggestion rows that fit before the spotlight panel has to scroll. A
/// `max_h` on the scrollable does not create wheel overflow — the wrapper
/// leaves it on the content, which is then clamped to the viewport height.
const SPOTLIGHT_VISIBLE: usize = 9;
/// Suggestion-list viewport height (the panel's old 360px max minus the
/// padding, the input, and the gaps), same caveat as [`SPOTLIGHT_VISIBLE`].
const SPOTLIGHT_LIST_H: f32 = 300.0;

impl MarketplacePage {
    /// Spotlight trigger: search glyph and ⌘F, or the active query. Picking a
    /// row sets the query, because this shell has no skill detail drawer.
    pub(super) fn render_spotlight(
        &self,
        search: &Entity<InputState>,
        view: WeakEntity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let query = self.query.trim().to_string();
        let active = !query.is_empty();
        let needle = query.to_lowercase();
        let names: Vec<String> = self
            .skills
            .iter()
            .filter(|skill| needle.is_empty() || skill.name.to_lowercase().contains(&needle))
            .take(8)
            .map(|skill| skill.name.clone())
            .collect();
        let focus = search.read(cx).focus_handle(cx);
        let input = search.clone();
        let menu_view = view.clone();
        let menu_search = search.clone();
        let clear_view = view;
        let clear_search = search.clone();

        div()
            .flex()
            .items_center()
            .h(px(32.0))
            .flex_shrink_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if active {
                palette().accent
            } else {
                palette().border
            }))
            .bg(rgb(if active {
                palette().accent_soft
            } else {
                palette().bg
            }))
            .occlude()
            .child(
                Popover::new("mk-spotlight")
                    .anchor(Anchor::TopLeft)
                    .offset(px(6.0))
                    .appearance(false)
                    .track_focus(&focus)
                    .trigger(SpotlightTrigger {
                        query: SharedString::from(query),
                        active,
                        open: false,
                    })
                    .content(move |popover, _window, cx| {
                        let dismiss = cx.entity().downgrade();
                        let _ = popover;
                        let panel = div()
                            .w(px(320.0))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2()
                            .rounded_xl()
                            .border_1()
                            .border_color(rgb(palette().border))
                            .bg(rgb(palette().card))
                            .shadow_lg()
                            .child(Input::new(&input));
                        // More names than the viewport holds scroll inside a
                        // fixed height; fewer render at their natural height.
                        // A `max_h` on the scrollable does not create wheel
                        // overflow — the wrapper leaves it on the content.
                        let mut list = div().flex().flex_col().gap_1();
                        for (index, name) in names.iter().enumerate() {
                            let pick = name.clone();
                            let view = menu_view.clone();
                            let search = menu_search.clone();
                            let dismiss = dismiss.clone();
                            list = list.child(
                                div()
                                    .id(ElementId::Name(format!("mk-spot-{index}").into()))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .text_color(rgb(palette().fg))
                                    .interaction_spring(
                                        format!("mk-spot-{index}"),
                                        true,
                                        MotionPaint::new(),
                                        MotionPaint::new().bg(rgb(palette().panel_hover)),
                                    )
                                    .child(name.clone())
                                    .on_click(move |_, window, app| {
                                        app.stop_propagation();
                                        let pick = pick.clone();
                                        let _ = view.update(app, |this, cx| {
                                            this.query = pick.clone();
                                            search.update(cx, |state, cx| {
                                                state.set_value(pick.clone(), window, cx);
                                            });
                                            this.load_now(cx, false);
                                        });
                                        let _ = dismiss
                                            .update(app, |state, cx| state.dismiss(window, cx));
                                    }),
                            );
                        }
                        panel.child(if names.len() > SPOTLIGHT_VISIBLE {
                            div()
                                .h(px(SPOTLIGHT_LIST_H))
                                .w_full()
                                .min_w_0()
                                .flex_shrink_0()
                                .child(
                                    div()
                                        .id("mk-spotlight-list")
                                        .overflow_y_scrollbar()
                                        .child(list),
                                )
                                .into_any_element()
                        } else {
                            list.into_any_element()
                        })
                    }),
            )
            .when(active, |row| {
                row.child(
                    div()
                        .id("mk-spotlight-clear")
                        .p(px(4.0))
                        .mr(px(4.0))
                        .rounded_sm()
                        .cursor_pointer()
                        .child(icon(IconName::X, 12.0, palette().accent))
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            let _ = clear_view.update(cx, |this, cx| {
                                this.query.clear();
                                clear_search.update(cx, |state, cx| state.clean(window, cx));
                                this.load_now(cx, true);
                            });
                        })
                        .interaction_spring(
                            "mk-spotlight-clear",
                            true,
                            MotionPaint::new(),
                            MotionPaint::new().bg(rgb(palette().panel_hover)),
                        ),
                )
            })
    }
}

struct SpotlightTrigger {
    query: SharedString,
    active: bool,
    open: bool,
}

impl Selectable for SpotlightTrigger {
    fn selected(self, _: bool) -> Self {
        self
    }

    fn is_selected(&self) -> bool {
        self.active
    }

    fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

impl IntoElement for SpotlightTrigger {
    type Element = crate::chrome::MotionDiv;

    fn into_element(self) -> Self::Element {
        let ink = if self.active {
            palette().accent
        } else {
            palette().fg
        };
        let mut row = div()
            .id("mk-spotlight-trigger")
            .flex()
            .items_center()
            .h(px(30.0))
            .gap(px(6.0))
            .px(px(10.0))
            .cursor_pointer()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(ink))
            .child(icon(IconName::Search, 14.0, ink));
        if self.active {
            row = row.child(div().max_w(px(88.0)).overflow_hidden().child(self.query));
        } else {
            row = row.child(
                div()
                    .px(px(6.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette().border))
                    .bg(rgb(palette().well))
                    .text_size(px(10.0))
                    .font_family("monospace")
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette().fg))
                    .child("⌘F"),
            );
        }
        let _ = self.open;
        row.interaction_spring(
            "mk-spotlight-trigger",
            true,
            MotionPaint::new().fg(rgb(ink)),
            MotionPaint::new()
                .bg(rgb(palette().panel_hover))
                .fg(rgb(ink)),
        )
    }
}
