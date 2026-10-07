//! Static widths for the window, skill cards, and quota cards.
//!
//! One row is the shared measure: three skill cards and two quota cards
//! occupy the same width. The default window is that row, plus the expanded
//! rail, the accounts page padding, and a little slack for the scrollbar.
//! Cards do not grow when the window does. A wider pane adds another
//! column. The pitch is `skill_card/grid.rs`; this file only holds the numbers.

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

/// Accounts page horizontal padding, each side.
pub const ACCOUNTS_PAGE_PAD: f32 = 24.0;
/// Skills and market page padding. `p_5` at the default scale.
pub const SKILL_PAGE_PAD: f32 = 20.0;

/// Scrollbar and panel-border slack so the second quota card stays on the row.
const ROW_SLACK: f32 = 16.0;

/// Default window. Fits two quota cards and three skill cards with the rail open.
pub const WINDOW_W: f32 = SHELL_CHROME_W + ACCOUNTS_PAGE_PAD * 2.0 + CARD_ROW_W + ROW_SLACK;
pub const WINDOW_H: f32 = 800.0;

/// Still holds one quota card and two skill cards.
pub const WINDOW_MIN_W: f32 = 960.0;
pub const WINDOW_MIN_H: f32 = 600.0;

const _: () = {
    assert!(QUOTA_CARD_W * 2.0 + CARD_GAP == CARD_ROW_W);

    let accounts = WINDOW_W - SHELL_CHROME_W - PANEL_BORDER_X - ACCOUNTS_PAGE_PAD * 2.0;
    assert!(accounts >= CARD_ROW_W);
    let skills = WINDOW_W - SHELL_CHROME_W - PANEL_BORDER_X - SKILL_PAGE_PAD * 2.0;
    assert!(skills >= CARD_ROW_W);
    assert!(skills < SKILL_CARD_W * 4.0 + CARD_GAP * 3.0);

    let min_accounts = WINDOW_MIN_W - SHELL_CHROME_W - PANEL_BORDER_X - ACCOUNTS_PAGE_PAD * 2.0;
    assert!(min_accounts >= QUOTA_CARD_W);
    assert!(min_accounts < QUOTA_CARD_W * 2.0 + CARD_GAP);
    let min_skills = WINDOW_MIN_W - SHELL_CHROME_W - PANEL_BORDER_X - SKILL_PAGE_PAD * 2.0;
    assert!(min_skills >= SKILL_CARD_W * 2.0 + CARD_GAP);
    assert!(min_skills < CARD_ROW_W);
};
