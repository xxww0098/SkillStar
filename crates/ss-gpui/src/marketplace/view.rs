//! Render regions for the Marketplace page: the tab rail, the sort and view
//! toggles, the snapshot banner, the loading skeletons, and the empty state.
//!
//! The page module keeps the snapshot state machine and the frame; these
//! helpers only read it.

use gpui_kit::assets::IconName;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::types::{MarketplaceTab, icon};
use super::{MarketSort, MarketplacePage};
use crate::chrome::{
    InteractionSpring, MotionPaint, icon_spin, pulse, segment_tab, segment_track,
    view_toggle_button,
};
use crate::skill_card::{CARD_GAP, CardFace, card_placeholder, card_rows};
use crate::theme::palette;

impl MarketplacePage {
    pub(super) fn render_sort(&self, view: WeakEntity<Self>) -> Div {
        let stars = view.clone();
        let updated = view;
        segment_track()
            .child(
                segment_tab(
                    "mk-sort-stars",
                    crate::i18n::t("toolbar.stars"),
                    self.sort == MarketSort::Stars,
                )
                .on_click(move |_, _, cx| {
                    let _ = stars.update(cx, |this, cx| {
                        this.sort = MarketSort::Stars;
                        this.apply_sort();
                        this.revise(cx);
                    });
                }),
            )
            .child(
                segment_tab(
                    "mk-sort-updated",
                    crate::i18n::t("toolbar.updated"),
                    self.sort == MarketSort::Updated,
                )
                .on_click(move |_, _, cx| {
                    let _ = updated.update(cx, |this, cx| {
                        this.sort = MarketSort::Updated;
                        this.apply_sort();
                        this.revise(cx);
                    });
                }),
            )
    }

    pub(super) fn render_view_toggle(&self, view: WeakEntity<Self>) -> Div {
        let grid = view.clone();
        let list = view;
        let list_mode = self.view_list;
        segment_track()
            .child(
                view_toggle_button("mk-view-grid", IconName::LayoutGrid, !list_mode).on_click(
                    move |_, _, cx| {
                        let _ = grid.update(cx, |this, cx| {
                            this.view_list = false;
                            this.reset_list_scroll();
                            this.revise(cx);
                        });
                    },
                ),
            )
            .child(
                view_toggle_button("mk-view-list", IconName::List, list_mode).on_click(
                    move |_, _, cx| {
                        let _ = list.update(cx, |this, cx| {
                            this.view_list = true;
                            this.reset_list_scroll();
                            this.revise(cx);
                        });
                    },
                ),
            )
    }
    pub(super) fn render_tabs(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let mut track = div()
            .flex()
            .items_center()
            .h(px(32.0))
            .gap(px(2.0))
            .px(px(2.0))
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().well))
            .occlude();

        for tab in MarketplaceTab::ALL {
            let active = tab == self.tab;
            let v = view.clone();
            let search = self.search.clone();
            track = track.child(
                div()
                    .id(ElementId::Name(format!("mk-tab-{tab:?}").into()))
                    .h_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_full()
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(rgb(if active {
                        palette().on_accent
                    } else {
                        palette().fg_muted
                    }))
                    .when(active, |d| d.bg(rgb(palette().accent)).shadow_sm())
                    .child(tab.label())
                    .on_click(move |_, window, cx| {
                        let _ = v.update(cx, |this, cx| {
                            this.tab = tab;
                            this.query.clear();
                            this.snapshot_error = None;
                            if tab == MarketplaceTab::Official {
                                this.detail.clear();
                            }
                            if let Some(search) = &search {
                                search.update(cx, |s, cx| s.clean(window, cx));
                            }
                            this.load_now(cx, true);
                        });
                    })
                    .interaction_spring(
                        format!("mk-tab-{tab:?}"),
                        true,
                        if active {
                            MotionPaint::new()
                                .bg(rgb(palette().accent))
                                .fg(rgb(palette().on_accent))
                        } else {
                            MotionPaint::new().fg(rgb(palette().fg_muted))
                        },
                        if active {
                            MotionPaint::new()
                                .bg(rgb(palette().accent))
                                .fg(rgb(palette().on_accent))
                        } else {
                            MotionPaint::new()
                                .bg(rgb(palette().card_hover))
                                .fg(rgb(palette().fg))
                        },
                    ),
            );
        }
        track
    }

    pub(super) fn render_snapshot_banner(&self, view: WeakEntity<Self>) -> Option<Div> {
        let err = self.snapshot_error.as_ref()?;
        let v = view.clone();
        let refreshing = self.refreshing;

        Some(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .px_4()
                .py_2()
                .rounded_lg()
                .bg(rgb(palette().danger_bg))
                .border_1()
                .border_color(rgb(palette().danger_border))
                .text_xs()
                .text_color(rgb(palette().danger_fg))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(icon(IconName::TriangleAlert, 14.0, palette().danger))
                        .child(div().truncate().child(format!("Snapshot warning: {err}"))),
                )
                .child(
                    div()
                        .id("mk-banner-retry")
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(rgb(palette().card))
                        .border_1()
                        .border_color(rgb(palette().border))
                        .text_color(rgb(palette().fg))
                        .child(icon_spin(
                            "mk-banner-retry-spin",
                            IconName::RefreshCw,
                            11.0,
                            palette().fg,
                            refreshing,
                        ))
                        .child(if refreshing { "Retrying…" } else { "Retry" })
                        .on_click(move |_, _, cx| {
                            let _ = v.update(cx, |this, cx| this.sync_snapshot(cx));
                        })
                        .interaction_spring(
                            "mk-banner-retry",
                            !refreshing,
                            MotionPaint::new().bg(rgb(palette().card)),
                            MotionPaint::new().bg(rgb(palette().card_hover)),
                        ),
                ),
        )
    }

    /// Placeholder tiles on the same tracks the loaded grid uses.
    pub(super) fn render_skeletons(&self, columns: usize) -> impl IntoElement {
        let mut tiles = Vec::with_capacity(6);
        for index in 0..6 {
            tiles.push(
                card_placeholder(
                    ElementId::Name(format!("mk-skel-{index}").into()),
                    CardFace::Market,
                )
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            pulse(format!("mk-skel-{index}-mark"))
                                .size(px(36.0))
                                .rounded_lg(),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .flex_1()
                                .child(pulse(format!("mk-skel-{index}-title")).h_4().w(px(120.0)))
                                .child(
                                    pulse(format!("mk-skel-{index}-sub"))
                                        .h_3()
                                        .w(px(80.0))
                                        .secondary(),
                                ),
                        ),
                )
                .child(
                    pulse(format!("mk-skel-{index}-line"))
                        .h_3()
                        .w(px(180.0))
                        .secondary(),
                )
                .into_any_element(),
            );
        }
        card_rows(columns, CARD_GAP, tiles)
    }

    pub(super) fn render_empty(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let is_search = !self.query.trim().is_empty();
        let query = self.query.clone();
        let search = self.search.clone();
        let v = view.clone();

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .py_16()
            .child(icon(IconName::Search, 32.0, palette().fg_muted))
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette().fg))
                    .child(if is_search {
                        format!("No skills matching \"{query}\"")
                    } else {
                        "No items found in this section".to_string()
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .child(if is_search {
                        "Try a different keyword or clear your search query."
                    } else {
                        "Check your network connection or try refreshing the snapshot."
                    }),
            )
            .when(is_search, |d| {
                d.child(
                    div()
                        .id("mk-empty-clear")
                        .mt_2()
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(rgb(palette().accent))
                        .text_xs()
                        .text_color(rgb(palette().on_accent))
                        .child(crate::i18n::t("marketplace.clearSearch"))
                        .on_click(move |_, window, cx| {
                            let _ = v.update(cx, |this, cx| {
                                this.query.clear();
                                if let Some(search) = &search {
                                    search.update(cx, |s, cx| s.clean(window, cx));
                                }
                                this.load_now(cx, true);
                            });
                        })
                        .interaction_spring(
                            "mk-empty-clear",
                            true,
                            MotionPaint::new().opacity(1.0),
                            MotionPaint::new().opacity(0.9),
                        ),
                )
            })
    }
}
