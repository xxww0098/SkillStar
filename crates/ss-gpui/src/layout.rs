//! Static widths for the window, skill cards, and quota cards.
//!
//! One row is the shared measure: three skill cards and two quota cards
//! occupy the same width. The default window is picked so that row fills
//! the pane exactly, with the rail expanded — no trailing slack, no
//! clipped fourth column. The detail column reserves one card plus one
//! gap, so opening it removes exactly one column: three columns closed,
//! two open. Cards do not grow when the window does. A wider pane adds
//! another column. The pitch is `skill_card/grid.rs`; this file only
//! holds the numbers.

/// Gap between cards in a row. Accounts `gap` and the skill grid use this.
pub const CARD_GAP: f32 = 16.0;

/// Painted skill-card width. Grid tracks pin to this; they do not stretch.
pub const SKILL_CARD_W: f32 = 336.0;
/// Painted skill-card height. Marketplace virtual rows copy this number.
pub const SKILL_CARD_H: f32 = 160.0;

/// Painted quota-card width. Same-provider cards sit two-up at this size.
pub const QUOTA_CARD_W: f32 = 512.0;

/// Three skill cards, or two quota cards, including the gaps between them.
pub const CARD_ROW_W: f32 = SKILL_CARD_W * 3.0 + CARD_GAP * 2.0;

/// Track the detail column reserves beside the grid: one card plus one
/// gap. Subtracting it from the pane removes exactly one grid column, so
/// the default window holds three columns closed and two with the column
/// open, both without remainder.
pub const DETAIL_COLUMN_W: f32 = SKILL_CARD_W + CARD_GAP;

/// Canvas gutter around the rail and the main panel.
pub const SHELL_GAP: f32 = 8.0;
/// Expanded sidebar.
pub const RAIL_W: f32 = 180.0;
/// Collapsed sidebar.
pub const RAIL_COLLAPSED_W: f32 = 56.0;

/// Left gutter + expanded rail + gutter + right gutter.
/// This is the main panel's border box. The 1px border on each side sits
/// inside it; card columns use [`PANEL_BORDER_X`].
pub const SHELL_CHROME_W: f32 = SHELL_GAP * 3.0 + RAIL_W;

/// Main panel border, left plus right. Taffy sizes borders inside the box,
/// so the card pane is this much narrower than [`SHELL_CHROME_W`] alone.
pub const PANEL_BORDER_X: f32 = 2.0;

/// Page horizontal padding, each side, shared by the skills, market, and
/// accounts pages (`p_5` at the default scale). One inset lets the default
/// window hold one exact card row on every page.
pub const PAGE_PAD: f32 = 20.0;

/// Page toolbar band height, every page. The band carries the traffic-light
/// clearance (overlay titlebar), so it starts at the panel's top border.
pub const TOOLBAR_H: f32 = 56.0;

/// Vertical padding of the skills page scroll region. The pair is not the
/// page `PAGE_PAD`: it is the remainder that keeps four card strides inside
/// the default window's track, so the last row lands fully in view without
/// scrolling.
pub const SKILL_SCROLL_PT: f32 = 12.0;
pub const SKILL_SCROLL_PB: f32 = 10.0;

/// Default window. With the rail expanded the pane is one exact card row:
/// three skill columns, two quota columns, no remainder.
pub const WINDOW_W: f32 = SHELL_CHROME_W + PANEL_BORDER_X + PAGE_PAD * 2.0 + CARD_ROW_W;
pub const WINDOW_H: f32 = 800.0;

/// Still holds one quota card and two skill cards.
pub const WINDOW_MIN_W: f32 = 960.0;
pub const WINDOW_MIN_H: f32 = 600.0;

const _: () = {
    assert!(QUOTA_CARD_W * 2.0 + CARD_GAP == CARD_ROW_W);
    assert!(DETAIL_COLUMN_W == SKILL_CARD_W + CARD_GAP);

    // Default window: the pane is the row exactly, closed and with the
    // detail column open. One page padding for skills and accounts is what
    // makes both pages exact at the same width.
    let pane = WINDOW_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2.0;
    assert!(pane == CARD_ROW_W);
    assert!(pane - DETAIL_COLUMN_W == SKILL_CARD_W * 2.0 + CARD_GAP);

    // Minimum window keeps two skill columns and one quota card.
    let min_pane = WINDOW_MIN_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2.0;
    assert!(min_pane >= SKILL_CARD_W * 2.0 + CARD_GAP);
    assert!(min_pane < CARD_ROW_W);
    assert!(min_pane >= QUOTA_CARD_W);
    assert!(min_pane < QUOTA_CARD_W * 2.0 + CARD_GAP);

    // Vertical lock at the default window: shell insets and borders 18,
    // toolbar 56, scroll padding 22, four card strides 704 — no remainder,
    // so the fourth card row is fully visible without scrolling. The
    // marketplace and other pages keep their own padding and may scroll.
    let track =
        WINDOW_H - SHELL_GAP * 2.0 - PANEL_BORDER_X - TOOLBAR_H - SKILL_SCROLL_PT - SKILL_SCROLL_PB;
    assert!(track == (SKILL_CARD_H + CARD_GAP) * 4.0);
};
