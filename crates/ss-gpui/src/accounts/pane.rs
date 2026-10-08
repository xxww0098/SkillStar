//! Quota cards, replayed while the header refresh icon turns.
//!
//! Per-card refresh and the reset-card spring live in this view, so they
//! still lay the pane out. The header spin does not.

use gpui_kit::*;

use super::AccountsPage;
use crate::layout::{CARD_GAP, PAGE_PAD, QUOTA_CARD_W};
use crate::skill_card::{columns_for, pane_width};

pub(super) struct AccountsPane {
    page: Entity<AccountsPage>,
    epoch: u64,
    _observe: Subscription,
}

impl AccountsPane {
    fn new(page: Entity<AccountsPage>, epoch: u64, cx: &mut Context<Self>) -> Self {
        // `page` is leased by the render that constructs this view.
        let observe = cx.observe(&page, |this, page, cx| {
            let epoch = page.read(cx).pane_epoch();
            if epoch != this.epoch {
                cx.notify();
            }
        });
        Self {
            page,
            epoch,
            _observe: observe,
        }
    }
}

impl Render for AccountsPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let columns = columns_for(
            QUOTA_CARD_W,
            CARD_GAP,
            pane_width(
                f32::from(window.viewport_size().width),
                PAGE_PAD,
                QUOTA_CARD_W,
            ),
        );
        let page = self.page.clone();
        let pane = page.update(cx, |page, cx| {
            div()
                .flex_1()
                .min_h_0()
                .size_full()
                .px(px(PAGE_PAD))
                .pb(px(PAGE_PAD))
                .child(page.render_pane(columns, cx.entity().downgrade()))
                .into_any_element()
        });
        self.epoch = self.page.read(cx).pane_epoch();
        pane
    }
}

impl AccountsPage {
    pub(super) fn ensure_pane(&mut self, cx: &mut Context<Self>) -> Entity<AccountsPane> {
        if let Some(pane) = &self.pane {
            return pane.clone();
        }
        let page = cx.entity();
        let epoch = self.pane_epoch();
        let pane = cx.new(|cx| AccountsPane::new(page, epoch, cx));
        self.pane = Some(pane.clone());
        pane
    }
}
