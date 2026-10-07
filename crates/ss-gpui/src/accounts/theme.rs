//! Account workbench colors.
//!
//! Mixed the same way as `dsh-plugin-oauth-subs` `.osubs` tokens:
//! `color-mix(in oklab, currentColor N%, transparent)` over the shell
//! panel (`palette().panel` / `--color-sidebar`). Quota fills lerp in HSL so
//! the midpoint stays amber.

use crate::theme::{Palette, mix_hsl};

/// Account surfaces mirror the reference workbench; shell colors stay unchanged.
pub(super) fn palette() -> Palette {
    let mut palette = *crate::theme::palette();
    palette.panel = palette.os_surface;
    palette.fg = palette.os_ink;
    palette
}

/// Panel surface. Legend titles paint this to knock out the card border.
/// `--osubs-line` 16%.
/// `--osubs-edge` 30%.
/// `--osubs-hair` 10%.
/// `--osubs-fill` 6%.
/// `--osubs-fill-2` 12%.
/// `--osubs-muted` 66% ink.
/// `--osubs-faint` 64% ink.
/// Tag ink, 75%.
/// Active account border, 60% ink over the panel.
/// `--osubs-ok` `#2f9e44` @ 65% with ink.
/// `--osubs-warn` `#b45309` @ 70% with ink.
/// `--osubs-bad` `#e5484d` @ 62% with ink.

/// Caption color. Healthy windows stay ink; color is reserved for a warning.
pub fn quota_tone(remaining: f32) -> u32 {
    if remaining <= 15.0 {
        palette().os_bad
    } else if remaining <= 40.0 {
        palette().os_warn
    } else {
        palette().fg
    }
}

/// Bar fill. 100% remaining = ok, 50% = warn, 0% = bad.
pub fn quota_fill(remaining: f32) -> u32 {
    let pct = remaining.clamp(0.0, 100.0);
    if pct >= 50.0 {
        mix_hsl(palette().os_warn, palette().os_ok, (pct - 50.0) / 50.0)
    } else {
        mix_hsl(palette().os_bad, palette().os_warn, pct / 50.0)
    }
}
