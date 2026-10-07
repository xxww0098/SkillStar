//! Leaderboard body and detail column, replayed across toolbar animation frames.
//!
//! The refresh icon and the snapshot-banner retry live on the page. Their
//! frames notify the page and must not rebuild the virtual list. Scroll,
//! hover, and the loading pulse notify this board instead.

use gpui_kit::*;

use super::detail_drawer;
use super::skill_blurb;
use super::{MarketplacePage, MarketplaceTab, scroll_pane};
use crate::chrome::{back_to_top_button, is_list_scrolled_down};
use crate::skill_card::grid_columns;

pub(super) struct MarketBoard {
    page: Entity<MarketplacePage>,
    list_epoch: u64,
    translation_epoch: u64,
    _observe: Subscription,
}

impl MarketBoard {
    fn new(
        page: Entity<MarketplacePage>,
        list_epoch: u64,
        translation_epoch: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        // `page` is leased by the render that constructs this view.
        let observe = cx.observe(&page, |this, page, cx| {
            let (list, translation) = page.read(cx).replay_epochs();
            if list != this.list_epoch || translation != this.translation_epoch {
                cx.notify();
            }
        });
        Self {
            page,
            list_epoch,
            translation_epoch,
            _observe: observe,
        }
    }
}

impl Render for MarketBoard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page.clone();
        let board = page.update(cx, |page, cx| {
            page.render_board(window, cx).into_any_element()
        });
        let (list, translation) = self.page.read(cx).replay_epochs();
        self.list_epoch = list;
        self.translation_epoch = translation;
        board
    }
}

pub(super) struct MarketDetail {
    page: Entity<MarketplacePage>,
    list_epoch: u64,
    translation_epoch: u64,
    _observe: Subscription,
}

impl MarketDetail {
    fn new(
        page: Entity<MarketplacePage>,
        list_epoch: u64,
        translation_epoch: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        // `page` is leased by the render that constructs this view.
        let observe = cx.observe(&page, |this, page, cx| {
            let (list, translation) = page.read(cx).replay_epochs();
            if list != this.list_epoch || translation != this.translation_epoch {
                cx.notify();
            }
        });
        Self {
            page,
            list_epoch,
            translation_epoch,
            _observe: observe,
        }
    }
}

impl Render for MarketDetail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page.clone();
        let column = page.update(cx, |page, cx| {
            if let Some(summary) = page.detail_summary() {
                crate::translation::schedule(
                    &cx.entity(),
                    [summary],
                    crate::translation::Surface::Description,
                    cx,
                );
            }
            page.render_detail_column(cx)
        });
        let (list, translation) = self.page.read(cx).replay_epochs();
        self.list_epoch = list;
        self.translation_epoch = translation;
        column
    }
}

impl MarketplacePage {
    pub(super) fn ensure_board(&mut self, cx: &mut Context<Self>) -> Entity<MarketBoard> {
        if let Some(board) = &self.board {
            return board.clone();
        }
        let page = cx.entity();
        let (list_epoch, translation_epoch) = self.replay_epochs();
        let board = cx.new(|cx| MarketBoard::new(page, list_epoch, translation_epoch, cx));
        self.board = Some(board.clone());
        board
    }

    pub(super) fn ensure_detail(&mut self, cx: &mut Context<Self>) -> Entity<MarketDetail> {
        if let Some(detail) = &self.detail_view {
            return detail.clone();
        }
        let page = cx.entity();
        let (list_epoch, translation_epoch) = self.replay_epochs();
        let detail = cx.new(|cx| MarketDetail::new(page, list_epoch, translation_epoch, cx));
        self.detail_view = Some(detail.clone());
        detail
    }

    pub(super) fn detail_summary(&self) -> Option<String> {
        self.detail.phase().and_then(|phase| match phase {
            detail_drawer::DetailPhase::Ready(details) => details
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string),
            _ => None,
        })
    }

    pub(super) fn render_board(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        crate::translation::schedule(
            &cx.entity(),
            self.skills.iter().filter_map(skill_blurb),
            crate::translation::Surface::Description,
            cx,
        );
        let layout_columns = grid_columns(detail_drawer::market_grid_width(
            f32::from(window.viewport_size().width),
            self.detail.is_open(),
        ));
        self.columns = layout_columns;
        let columns = if self.view_list { 1 } else { layout_columns };
        self.watch_bounds(window, cx);
        let view = cx.entity().downgrade();

        let body = if self.loading {
            scroll_pane("mk-scroll", self.render_skeletons(columns)).into_any_element()
        } else if (self.tab == MarketplaceTab::Official && self.publishers.is_empty())
            || (self.tab != MarketplaceTab::Official && self.skills.is_empty())
        {
            scroll_pane("mk-scroll", self.render_empty(view.clone())).into_any_element()
        } else if self.tab == MarketplaceTab::Official {
            self.publisher_list(columns, cx).into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(self.skill_list(columns, cx))
                .into_any_element()
        };

        let mut col = div().relative().size_full().flex().flex_col().child(body);

        let scrolled_down = if self.tab == MarketplaceTab::Official {
            !self.publishers.is_empty() && is_list_scrolled_down(&self.publishers_scroll)
        } else {
            !self.skills.is_empty() && is_list_scrolled_down(&self.skills_scroll)
        };
        if scrolled_down {
            let skills_scroll = self.skills_scroll.clone();
            let publishers_scroll = self.publishers_scroll.clone();
            let view = view.clone();
            col = col.child(back_to_top_button(
                "mk-back-top",
                crate::i18n::t("marketplace.backToTop"),
                move |_, _, cx| {
                    cx.stop_propagation();
                    skills_scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    publishers_scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    let _ = view.update(cx, |this, cx| this.revise(cx));
                },
            ));
        }
        col
    }
}
