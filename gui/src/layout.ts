// Static widths mirror of crates/ss-gpui/src/layout.rs. That file's compile-
// time `assert!` block becomes `LAYOUT_ASSERTIONS` below: run from a
// self-check test, so a width change that breaks the grid math fails loudly.
//
// One row is the shared measure: three skill cards and two quota cards
// occupy the same width. The default window is picked so that row fills the
// pane exactly with the rail expanded; the detail column reserves one card
// plus one gap, so opening it removes exactly one column.

/** Gap between cards in a row (layout.rs CARD_GAP). */
export const CARD_GAP = 16;
/** Painted skill-card width; grid tracks pin to it (layout.rs SKILL_CARD_W). */
export const SKILL_CARD_W = 336;
/** Painted skill-card height (layout.rs SKILL_CARD_H). */
export const SKILL_CARD_H = 160;
/** Painted quota-card width (layout.rs QUOTA_CARD_W). */
export const QUOTA_CARD_W = 512;

/** Three skill cards, or two quota cards, including the gaps. */
export const CARD_ROW_W = SKILL_CARD_W * 3 + CARD_GAP * 2;

/** Track the detail column reserves beside the grid: one card plus one gap. */
export const DETAIL_COLUMN_W = SKILL_CARD_W + CARD_GAP;

/** Canvas gutter around the rail and the main panel (SHELL_GAP). */
export const SHELL_GAP = 8;
/** Expanded sidebar (RAIL_W). */
export const RAIL_W = 180;
/** Collapsed sidebar (RAIL_COLLAPSED_W). */
export const RAIL_COLLAPSED_W = 56;

/** Left gutter + expanded rail + gutter + right gutter: the main panel's border box. */
export const SHELL_CHROME_W = SHELL_GAP * 3 + RAIL_W;
/** Main panel border, left plus right (Taffy sizes borders inside the box). */
export const PANEL_BORDER_X = 2;

/** Page horizontal padding, each side (PAGE_PAD). */
export const PAGE_PAD = 20;
/** Page toolbar band height, every page (TOOLBAR_H). */
export const TOOLBAR_H = 56;

/** Vertical padding of the skills page scroll region. */
export const SKILL_SCROLL_PT = 12;
export const SKILL_SCROLL_PB = 10;

/** Default window: the pane is one exact card row (WINDOW_W / WINDOW_H). */
export const WINDOW_W = SHELL_CHROME_W + PANEL_BORDER_X + PAGE_PAD * 2 + CARD_ROW_W;
export const WINDOW_H = 800;

/** Still holds one quota card and two skill cards. */
export const WINDOW_MIN_W = 960;
export const WINDOW_MIN_H = 600;

/** SkillStar `--radius-xl`: panel corner radius (shell.rs PANEL_RADIUS_PX). */
export const PANEL_RADIUS = 16;

/** The nav selection box travels through rows of this stride (32 + 2 gap). */
export const NAV_ROW_H = 32;
export const NAV_GAP = 2;
export const NAV_STRIDE = NAV_ROW_H + NAV_GAP;

// layout.rs's const-assertion block, as data: [description, passed].
// `checkLayoutInvariants()` runs it; the app self-check test calls it.
export const LAYOUT_ASSERTIONS: [string, boolean][] = [
  ["quota pair spans the card row", QUOTA_CARD_W * 2 + CARD_GAP === CARD_ROW_W],
  ["detail column is one card plus one gap", DETAIL_COLUMN_W === SKILL_CARD_W + CARD_GAP],
  [
    "default window: pane is the exact card row",
    WINDOW_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2 === CARD_ROW_W,
  ],
  [
    "default window with detail open holds two columns",
    WINDOW_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2 - DETAIL_COLUMN_W ===
      SKILL_CARD_W * 2 + CARD_GAP,
  ],
  [
    "minimum window keeps two skill columns",
    WINDOW_MIN_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2 >= SKILL_CARD_W * 2 + CARD_GAP,
  ],
  [
    "minimum window is under the three-column row",
    WINDOW_MIN_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2 < CARD_ROW_W,
  ],
  [
    "minimum window keeps one quota card",
    WINDOW_MIN_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2 >= QUOTA_CARD_W &&
      WINDOW_MIN_W - SHELL_CHROME_W - PANEL_BORDER_X - PAGE_PAD * 2 <
        QUOTA_CARD_W * 2 + CARD_GAP,
  ],
  [
    "default window shows four card rows without scrolling",
    WINDOW_H - SHELL_GAP * 2 - PANEL_BORDER_X - TOOLBAR_H - SKILL_SCROLL_PT - SKILL_SCROLL_PB ===
      (SKILL_CARD_H + CARD_GAP) * 4,
  ],
];

export function checkLayoutInvariants(): string[] {
  return LAYOUT_ASSERTIONS.filter(([, ok]) => !ok).map(([label]) => label);
}
