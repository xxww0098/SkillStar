//! Publisher repos and skills, replayed while the toolbar refresh icon turns.
//!
//! The install spin lives in the detail column, so that column is its own
//! view. Scroll still rebuilds this board; a refresh frame does not.

use gpui_kit::assets::IconName;
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_marketplace::PublisherRepo;

use super::super::types::skill_blurb;
use super::super::{card_grid, scroll_pane};
use super::PublisherDetailPage;
use crate::skill_card::{CARD_GAP, card_rows, grid_columns};

pub(super) struct PublisherBoard {
    page: Entity<PublisherDetailPage>,
    epoch: u64,
    _observe: Subscription,
}

impl PublisherBoard {
    fn new(page: Entity<PublisherDetailPage>, epoch: u64, cx: &mut Context<Self>) -> Self {
        // `page` is leased by the render that constructs this view.
        let observe = cx.observe(&page, |this, page, cx| {
            let epoch = page.read(cx).body_epoch();
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

impl Render for PublisherBoard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page.clone();
        let list = page.update(cx, |page, cx| {
            page.render_list(window, cx).into_any_element()
        });
        self.epoch = self.page.read(cx).body_epoch();
        list
    }
}

pub(super) struct PublisherDetailColumn {
    page: Entity<PublisherDetailPage>,
    epoch: u64,
    _observe: Subscription,
}

impl PublisherDetailColumn {
    fn new(page: Entity<PublisherDetailPage>, epoch: u64, cx: &mut Context<Self>) -> Self {
        // `page` is leased by the render that constructs this view.
        let observe = cx.observe(&page, |this, page, cx| {
            let epoch = page.read(cx).body_epoch();
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

impl Render for PublisherDetailColumn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page.clone();
        let column = page.update(cx, |page, cx| page.render_market_detail(cx));
        self.epoch = self.page.read(cx).body_epoch();
        column
    }
}

impl PublisherDetailPage {
    pub(super) fn ensure_board(&mut self, cx: &mut Context<Self>) -> Entity<PublisherBoard> {
        if let Some(board) = &self.board {
            return board.clone();
        }
        let page = cx.entity();
        let epoch = self.body_epoch();
        let board = cx.new(|cx| PublisherBoard::new(page, epoch, cx));
        self.board = Some(board.clone());
        board
    }

    pub(super) fn ensure_detail(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Entity<PublisherDetailColumn> {
        if let Some(detail) = &self.detail_view {
            return detail.clone();
        }
        let page = cx.entity();
        let epoch = self.body_epoch();
        let detail = cx.new(|cx| PublisherDetailColumn::new(page, epoch, cx));
        self.detail_view = Some(detail.clone());
        detail
    }

    pub(super) fn render_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let columns = grid_columns(super::super::detail_drawer::market_grid_width(
            f32::from(window.viewport_size().width),
            self.detail.is_open(),
        ));
        let view = cx.entity().downgrade();
        let body = if self.loading_repos || (self.active_repo.is_some() && self.loading_skills) {
            self.render_skeletons(columns).into_any_element()
        } else if self.active_repo.is_some() {
            let query = self.query.to_ascii_lowercase();
            let visible_skills: Vec<&Skill> = self
                .skills
                .iter()
                .filter(|skill| {
                    query.is_empty()
                        || skill.name.to_ascii_lowercase().contains(&query)
                        || skill.description.to_ascii_lowercase().contains(&query)
                })
                .collect();
            if visible_skills.is_empty() {
                super::empty_block(
                    IconName::Package,
                    if !self.query.is_empty() {
                        crate::i18n::t("publisherDetail.noMatch").to_string()
                    } else {
                        crate::i18n::t("publisherDetail.noSkills").to_string()
                    },
                )
                .into_any_element()
            } else {
                crate::translation::schedule(
                    &cx.entity(),
                    visible_skills.iter().copied().filter_map(skill_blurb),
                    crate::translation::Surface::Description,
                    cx,
                );
                let tiles = visible_skills
                    .into_iter()
                    .map(|skill| {
                        self.render_market_card(skill, view.clone(), cx)
                            .into_any_element()
                    })
                    .collect::<Vec<_>>();
                card_rows(columns, CARD_GAP, tiles).into_any_element()
            }
        } else {
            let query = self.query.to_ascii_lowercase();
            let visible_repos: Vec<&PublisherRepo> = self
                .repos
                .iter()
                .filter(|repo| {
                    query.is_empty()
                        || repo.repo.to_ascii_lowercase().contains(&query)
                        || repo.source.to_ascii_lowercase().contains(&query)
                })
                .collect();
            if visible_repos.is_empty() {
                super::empty_block(
                    IconName::Folder,
                    crate::i18n::t("publisherDetail.noReposMatch").to_string(),
                )
                .into_any_element()
            } else {
                let mut grid = card_grid();
                for repo in visible_repos {
                    grid = grid.child(self.render_repo_card(repo, view.clone()));
                }
                grid.into_any_element()
            }
        };
        div().size_full().child(scroll_pane("pd-scroll", body))
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::AppContext as _;

    use super::super::PublisherDetailPage;

    /// `ensure_board` runs inside the page render, which already holds the lease.
    #[gpui_kit::test]
    fn board_can_be_created_while_the_page_is_updating(cx: &mut gpui_kit::TestAppContext) {
        crate::init_test(cx);
        let page = cx.new(|_cx| PublisherDetailPage::new());
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                let _board = page.ensure_board(cx);
                let _detail = page.ensure_detail(cx);
            });
        });
    }
}
