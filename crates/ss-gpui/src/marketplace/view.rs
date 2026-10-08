//! Render regions for the Marketplace page: the tab rail, the sort and view
//! toggles, the snapshot banner, the loading skeletons, and the empty state.
//!
//! The page module keeps the snapshot state machine and the frame; these
//! helpers only read it.

use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::types::{MarketplaceTab, icon};
use super::{MarketSort, MarketplacePage};
use crate::chrome::{InteractionSpring, MotionPaint, icon_spin, pulse};
use crate::skill_card::{CARD_GAP, CardFace, card_placeholder, card_rows};
use crate::theme::palette;

impl MarketplacePage {
    pub(super) fn render_sort(&self, view: WeakEntity<Self>) -> TabBar {
        TabBar::new("mk-sort")
            .segmented()
            .selected_index(if self.sort == MarketSort::Updated {
                1
            } else {
                0
            })
            .on_click(move |ix, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.sort = if *ix == 1 {
                        MarketSort::Updated
                    } else {
                        MarketSort::Stars
                    };
                    this.apply_sort();
                    this.revise(cx);
                });
            })
            .child(Tab::new().label(crate::i18n::t("toolbar.stars")).flex_1())
            .child(Tab::new().label(crate::i18n::t("toolbar.updated")).flex_1())
    }

    pub(super) fn render_view_toggle(&self, view: WeakEntity<Self>) -> TabBar {
        TabBar::new("mk-view")
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
                        crate::chrome::tooltip(crate::i18n::t("toolbar.viewGrid")).build(window, cx)
                    }),
            )
            .child(
                Tab::new()
                    .icon(IconName::List)
                    .flex_1()
                    .tooltip(|window, cx| {
                        crate::chrome::tooltip(crate::i18n::t("toolbar.viewList")).build(window, cx)
                    }),
            )
    }
    pub(super) fn render_tabs(&self, view: WeakEntity<Self>) -> TabBar {
        let search = self.search.clone();
        let selected = MarketplaceTab::ALL
            .iter()
            .position(|tab| *tab == self.tab)
            .unwrap_or(0);
        let mut bar = TabBar::new("mk-tabs")
            .segmented()
            .selected_index(selected)
            .on_click(move |ix, window, cx| {
                let tab = MarketplaceTab::ALL[*ix];
                let _ = view.update(cx, |this, cx| {
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
            });
        for tab in MarketplaceTab::ALL {
            bar = bar.child(Tab::new().label(tab.label()).flex_1());
        }
        bar
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
                        .child(div().truncate().child(crate::i18n::tf(
                            "marketplace.snapshotWarning",
                            &[("err", err.as_str())],
                        ))),
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
                            IconName::RefreshCw,
                            11.0,
                            palette().fg,
                            refreshing,
                        ))
                        .child(if refreshing {
                            crate::i18n::t("marketplace.retrying")
                        } else {
                            crate::i18n::t("common.retry")
                        })
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

        Empty::new()
            .header(
                EmptyHeader::new()
                    .media(EmptyMedia::new().child(icon(
                        IconName::Search,
                        32.0,
                        palette().fg_muted,
                    )))
                    .title(
                        EmptyTitle::new()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .child(if is_search {
                                crate::i18n::tf(
                                    "marketplace.emptySearchTitle",
                                    &[("query", query.as_str())],
                                )
                            } else {
                                crate::i18n::t("marketplace.emptySectionTitle")
                            }),
                    )
                    .description(EmptyDescription::new().child(if is_search {
                        crate::i18n::t("marketplace.emptySearchDescription")
                    } else {
                        crate::i18n::t("marketplace.emptySectionDescription")
                    })),
            )
            .when(is_search, |d| {
                d.child(
                    Button::new("mk-empty-clear")
                        .primary()
                        .small()
                        .label(crate::i18n::t("marketplace.clearSearch"))
                        .on_click(move |_, window, cx| {
                            let _ = v.update(cx, |this, cx| {
                                this.query.clear();
                                if let Some(search) = &search {
                                    search.update(cx, |s, cx| s.clean(window, cx));
                                }
                                this.load_now(cx, true);
                            });
                        }),
                )
            })
    }
}
