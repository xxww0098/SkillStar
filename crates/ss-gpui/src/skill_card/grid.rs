//! One card pitch for skill cards and quota cards.
//!
//! `columns_for` turns a pane width and a card width into a track count.
//! `tracks` packs cells onto those tracks. A track is that card width and
//! does not grow, so a wider pane adds a column instead of stretching a card.
//! A short row is padded with spacers; the cards on it stay the same width.
//!
//! Skill, market, and publisher grids pass [`CARD_W`]. Quota cards pass
//! [`crate::layout::QUOTA_CARD_W`] and paint their own legend face.

use gpui_kit::*;

use super::{CARD_GAP, CARD_W};
use crate::layout::{PANEL_BORDER_X, SHELL_CHROME_W, SKILL_PAGE_PAD};

/// How many fixed-width tracks of `card_w` fit in `content_width`.
pub const fn columns_for(card_w: f32, gap: f32, content_width: f32) -> usize {
    if content_width < card_w {
        return 1;
    }
    let pitch = card_w + gap;
    ((content_width + gap) / pitch).floor() as usize
}

/// Skill-card columns. Quota cards call [`columns_for`] with their own width.
pub const fn grid_columns(content_width: f32) -> usize {
    columns_for(CARD_W, CARD_GAP, content_width)
}

/// Pane width inside the expanded rail, the panel border, and the page padding.
///
/// A collapsed rail is wider than this, so it can fit one more column.
/// Tracks stay `card_w`, so that extra column does not resize the cards.
/// The panel border is inside the shell inset; leaving it in the pane makes
/// the last card's border land past the scroller and get clipped.
pub const fn pane_width(viewport_width: f32, page_pad: f32, card_w: f32) -> f32 {
    (viewport_width - SHELL_CHROME_W - PANEL_BORDER_X - page_pad * 2.0).max(card_w)
}

/// Skill-card pane. Quota cards call [`pane_width`] with their padding and width.
pub const fn card_content_width(viewport_width: f32) -> f32 {
    pane_width(viewport_width, SKILL_PAGE_PAD, CARD_W)
}

const _: () = assert!(grid_columns(card_content_width(crate::layout::WINDOW_W)) == 3);

/// Lock one element to `width`. Tracks and a lone fixed card share this so
/// the two cannot drift.
pub(super) fn pin_width<E: Styled>(element: E, width: f32) -> E {
    element
        .w(px(width))
        .min_w(px(width))
        .max_w(px(width))
        .flex_grow_0()
        .flex_shrink_0()
}

fn pinned_track(card_w: f32, tile: impl IntoElement) -> Div {
    pin_width(div(), card_w).child(tile)
}

fn track_row(card_w: f32, columns: usize, gap: f32, tiles: Vec<AnyElement>) -> Div {
    let columns = columns.max(1);
    let filled = tiles.len();
    let mut row = div().w_full().flex().flex_row().items_start().gap(px(gap));
    for tile in tiles {
        row = row.child(pinned_track(card_w, tile));
    }
    for _ in filled..columns {
        row = row.child(div().flex_1().min_w_0().h(px(0.0)));
    }
    row
}

/// Skill-card row. The tiles fill [`CARD_W`] tracks.
pub fn card_row(columns: usize, gap: f32, tiles: Vec<AnyElement>) -> Div {
    track_row(CARD_W, columns, gap, tiles)
}

/// Rows of `card_w` tracks, chunked at `columns`.
pub fn tracks(
    card_w: f32,
    columns: usize,
    gap: f32,
    tiles: impl IntoIterator<Item = impl IntoElement>,
) -> Div {
    let columns = columns.max(1);
    let mut column = div().w_full().flex().flex_col().gap(px(gap));
    let mut row = Vec::with_capacity(columns);
    for tile in tiles {
        row.push(tile.into_any_element());
        if row.len() == columns {
            column = column.child(track_row(card_w, columns, gap, std::mem::take(&mut row)));
        }
    }
    if !row.is_empty() {
        column = column.child(track_row(card_w, columns, gap, row));
    }
    column
}

/// Skill-card rows. Quota cards call [`tracks`] with their own width.
pub fn card_rows(
    columns: usize,
    gap: f32,
    tiles: impl IntoIterator<Item = impl IntoElement>,
) -> Div {
    tracks(CARD_W, columns, gap, tiles)
}

#[cfg(test)]
mod tests {
    use super::{CARD_GAP, CARD_W};
    use super::{card_content_width, columns_for, grid_columns, pane_width};
    use crate::layout::{ACCOUNTS_PAGE_PAD, QUOTA_CARD_W, WINDOW_MIN_W, WINDOW_W};

    #[test]
    fn columns_follow_the_card_pitch() {
        let w = CARD_W;
        let g = CARD_GAP;
        assert_eq!(grid_columns(0.0), 1);
        assert_eq!(grid_columns(w), 1);
        assert_eq!(grid_columns(w + g + (w - 1.0)), 1);
        assert_eq!(grid_columns(w + g + w), 2);
        assert_eq!(grid_columns(w * 3.0 + g * 2.0), 3);
        assert_eq!(
            columns_for(w, g, w * 3.0 + g * 2.0),
            grid_columns(w * 3.0 + g * 2.0)
        );
    }

    #[test]
    fn content_width_never_drops_below_one_card() {
        assert_eq!(card_content_width(0.0), CARD_W);
        let default = card_content_width(WINDOW_W);
        assert_eq!(grid_columns(default), 3);
        assert!(
            grid_columns(card_content_width(1600.0)) >= grid_columns(card_content_width(1200.0)),
            "a wider viewport never offers fewer columns"
        );
    }

    #[test]
    fn quota_cards_use_the_same_pitch() {
        let wide = pane_width(WINDOW_W, ACCOUNTS_PAGE_PAD, QUOTA_CARD_W);
        let narrow = pane_width(WINDOW_MIN_W, ACCOUNTS_PAGE_PAD, QUOTA_CARD_W);
        assert_eq!(columns_for(QUOTA_CARD_W, CARD_GAP, wide), 2);
        assert_eq!(columns_for(QUOTA_CARD_W, CARD_GAP, narrow), 1);
        assert!(
            columns_for(QUOTA_CARD_W, CARD_GAP, wide + QUOTA_CARD_W)
                >= columns_for(QUOTA_CARD_W, CARD_GAP, wide)
        );
    }
}
