//! Compact marketplace tiles. The scroll parent owns `flex_1`; this wrap
//! and the tiles must not, or a row stretches into a full-height panel.

use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_marketplace::OfficialPublisher;

use super::MarketplacePage;
use super::types::{avatar_palette, icon};
use crate::nav::SelectPublisher;
use crate::skill_card::{
    CARD_GAP, CARD_H, CardFace, CardShell, CardWidth, card_row, card_shell, grid_columns,
};
use crate::theme::palette;

/// One virtual-list row of equal card tracks. Tiles fill their track; a short
/// row is padded, so its cards keep a complete row's width.
///
/// The bottom padding is the gap before the next row; `uniform_list` does not
/// insert gaps itself.
pub(crate) fn tile_row(
    columns: usize,
    stride: f32,
    tiles: impl IntoIterator<Item = impl IntoElement>,
) -> Div {
    let tiles = tiles
        .into_iter()
        .map(|tile| tile.into_any_element())
        .collect::<Vec<_>>();
    card_row(columns, CARD_GAP, tiles)
        .h(px(stride))
        .pb(px(CARD_GAP))
}

pub(crate) fn scroll_pane(id: &'static str, child: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .id(id)
        .child(child)
}

pub(crate) fn card_grid() -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_start()
        .content_start()
        .gap_4()
        .w_full()
}

/// Tile that fills the track `tile_row` hands it. The leaderboard, list mode,
/// and the publisher-detail skill grid all use it, so a market-card is the
/// same width on every marketplace surface.
pub(crate) fn market_tile_box(
    id: impl Into<ElementId>,
    selected: bool,
) -> crate::chrome::MotionDiv {
    tile_shell(id, CardWidth::Fill, selected)
}

/// Tile pinned to the fixed card box: a lone tile outside a card row keeps
/// CARD_W instead of a track.
pub(crate) fn market_tile(id: impl Into<ElementId>, selected: bool) -> crate::chrome::MotionDiv {
    tile_shell(id, CardWidth::Fixed, selected)
}

fn tile_shell(
    id: impl Into<ElementId>,
    width: CardWidth,
    selected: bool,
) -> crate::chrome::MotionDiv {
    card_shell(CardShell {
        id: id.into(),
        width,
        face: CardFace::Market,
        selected,
    })
}

impl MarketplacePage {
    pub(super) fn render_publisher_card(
        &self,
        pub_: &OfficialPublisher,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let p = pub_.clone();
        let (bg_color, fg_color) = avatar_palette(&pub_.name);
        let initial = pub_
            .name
            .chars()
            .next()
            .unwrap_or('P')
            .to_uppercase()
            .to_string();

        market_tile_box(
            ElementId::Name(format!("pub-card-{}", pub_.name).into()),
            false,
        )
        .flex()
        .items_center()
        .gap_3()
        .p_3()
        .cursor_pointer()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(36.0))
                .rounded_xl()
                .bg(rgb(bg_color))
                .border_1()
                .border_color(rgb(palette().border))
                .flex_shrink_0()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(fg_color))
                        .child(initial),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(palette().fg))
                                .truncate()
                                .child(pub_.name.clone()),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .px_1()
                                .rounded_sm()
                                .bg(rgb(palette().ok_bg))
                                .text_size(px(10.0))
                                .text_color(rgb(palette().ok))
                                .child(crate::i18n::t("marketplace.officialBadge")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(icon(IconName::Folder, 12.0, palette().fg_muted))
                                .child(format!("{} repos", pub_.repo_count)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(icon(IconName::Package, 12.0, palette().fg_muted))
                                .child(format!("{} skills", pub_.skill_count)),
                        ),
                ),
        )
        .child(icon(IconName::ChevronRight, 14.0, palette().fg_muted))
        .on_click(move |_, _, cx| {
            let p = p.clone();
            let _ = view.update(cx, |_, cx| {
                cx.emit(SelectPublisher { publisher: p });
            });
        })
    }

    pub(super) fn render_market_card(
        &self,
        skill: &Skill,
        view: WeakEntity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let busy = self.busy.as_deref() == Some(skill.name.as_str());
        let installed = skill.installed;
        let highlighted = self.detail.matches(skill);
        let action_name = skill.name.clone();
        let action_url = skill.git_url.clone();
        let clicked = skill.clone();
        let open_view = view.clone();
        super::market_card::render_market_card(
            skill,
            busy,
            highlighted,
            "mk",
            cx,
            move |_, _, cx| {
                cx.stop_propagation();
                let _ = view.update(cx, |this, cx| {
                    this.set_installed(action_url.clone(), action_name.clone(), !installed, cx);
                });
            },
        )
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let skill = clicked.clone();
            let _ = open_view.update(cx, |this, cx| this.open_market_skill(&skill, cx));
        })
    }

    pub(super) fn watch_bounds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._bounds_watch.is_some() {
            return;
        }
        self._bounds_watch = Some(cx.observe_window_bounds(window, |this, window, cx| {
            let columns = grid_columns(super::detail_drawer::market_grid_width(
                f32::from(window.viewport_size().width),
                this.detail.is_open(),
            ));
            if columns != this.columns {
                this.columns = columns;
                this.revise(cx);
            }
        }));
    }

    /// Visible rows only. Scroll and hover both notify this view; building
    /// the full leaderboard here is what made the page stutter.
    pub(super) fn skill_list(&self, columns: usize, cx: &mut Context<Self>) -> UniformList {
        let columns = columns.max(1);
        let rows = self.skills.len().div_ceil(columns);
        let list_id = SharedString::from(format!(
            "mk-skills-{}-{}",
            self.tab.label(),
            if self.view_list { "list" } else { "grid" }
        ));
        uniform_list(
            list_id,
            rows,
            cx.processor(move |this, range: Range<usize>, _window, cx| {
                let view = cx.entity().downgrade();
                let mut lines = Vec::with_capacity(range.end.saturating_sub(range.start));
                for row in range {
                    let start = row.saturating_mul(columns);
                    if start >= this.skills.len() {
                        break;
                    }
                    let end = (start + columns).min(this.skills.len());
                    let skills = this.skills[start..end].to_vec();
                    let tiles = skills
                        .iter()
                        .map(|skill| {
                            this.render_market_card(skill, view.clone(), cx)
                                .into_any_element()
                        })
                        .collect::<Vec<_>>();
                    lines.push(tile_row(columns, CARD_H + CARD_GAP, tiles));
                }
                lines
            }),
        )
        .track_scroll(&self.skills_scroll)
        .flex_1()
        .min_h_0()
        .w_full()
    }

    pub(super) fn publisher_list(
        &self,
        columns: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let columns = columns.max(1);
        let rows = self.publishers.len().div_ceil(columns);
        let list = uniform_list(
            SharedString::from(if self.view_list {
                "mk-publishers-list"
            } else {
                "mk-publishers-grid"
            }),
            rows,
            cx.processor(move |this, range: Range<usize>, _window, cx| {
                let view = cx.entity().downgrade();
                let mut lines = Vec::with_capacity(range.end.saturating_sub(range.start));
                for row in range {
                    let start = row.saturating_mul(columns);
                    if start >= this.publishers.len() {
                        break;
                    }
                    let end = (start + columns).min(this.publishers.len());
                    let publishers = this.publishers[start..end].to_vec();
                    let tiles = publishers
                        .iter()
                        .map(|pub_| {
                            this.render_publisher_card(pub_, view.clone())
                                .into_any_element()
                        })
                        .collect::<Vec<_>>();
                    lines.push(tile_row(columns, CARD_H + CARD_GAP, tiles));
                }
                lines
            }),
        )
        .track_scroll(&self.publishers_scroll)
        .flex_1()
        .min_h_0()
        .w_full();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_4()
            .w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .min_w_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette().fg))
                                    .child(crate::i18n::t("marketplace.officialPublishersTitle")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(palette().fg_muted))
                                    .child(self.tab.subtitle()),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_2()
                            .py(px(2.0))
                            .rounded_full()
                            .bg(rgb(palette().card))
                            .border_1()
                            .border_color(rgb(palette().border))
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .child(format!("{} publishers", self.publishers.len())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(list),
            )
    }
}
