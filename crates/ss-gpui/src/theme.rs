//! Light/dark palettes and the toggle that applies them.
//!
//! Two `Palette` consts — `DARK` (OLED slate navy) and `LIGHT` (cool
//! porcelain) — hold every color the shell paints. Views read
//! [`palette()`] inside `render`, so a single `cx.refresh_windows()`
//! repaint picks up a mode flip. `install` seeds the mode from
//! `gui_prefs.json` (`background_style`: `"current"` = dark, `"paper"` =
//! light, matching `src/lib/backgroundStyle`); [`toggle`] flips it,
//! persists the pref, and re-applies.
//!
//! The same palette is projected onto `gpui_kit::component::theme::Theme`
//! in [`apply`], so kit widgets (Input, scrollbar, switch) follow the app
//! instead of the kit's stock theme. Setting `Theme::mode` reloads the
//! registered config's colors, so colors are re-asserted in a second
//! `Theme::update` — see `gpui-component` docs on `Theme::update`.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::component::theme::{Theme, ThemeColor, ThemeMode};
use gpui_kit::*;

/// Every color the shell paints, for one mode. `u32` is `0xRRGGBB` (the
/// `rgb()` literal format used everywhere). Prompt dialogs do not paint
/// `scrim`: they share [`prompt_veil`], a translucent wash that leaves the
/// page visible.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Canvas behind the floating panels (`--color-background`).
    pub bg: u32,
    /// Sidebar rail / page panel (`--color-sidebar`).
    pub panel: u32,
    /// Row hover on the panel (`--color-sidebar-hover` premixed).
    pub panel_hover: u32,
    /// Active row / segmented fill (`--color-sidebar-active` premixed).
    pub panel_active: u32,
    /// Raised card (`--color-card`).
    pub card: u32,
    /// Card hover (`--color-card-hover`).
    pub card_hover: u32,
    /// Selected card fill.
    pub card_active: u32,
    /// Selected card hover.
    pub card_active_hover: u32,
    /// Sunken fills — card footers, segmented tracks, neutral chips.
    pub well: u32,
    /// Recessed input / code-field background.
    pub input: u32,
    /// Opaque dark plate for a local cover, such as a card selection mask.
    /// Prompt dialogs use [`prompt_veil`] instead of this.
    pub scrim: u32,
    /// Hover tip surface. Black in both modes.
    pub tip_bg: u32,
    /// Text on [`Self::tip_bg`].
    pub tip_fg: u32,

    /// Primary text (`--color-foreground`).
    pub fg: u32,
    /// Secondary text (`--color-muted-foreground`).
    pub fg_muted: u32,
    /// Weakest text tier — hints, timestamps.
    pub fg_faint: u32,
    /// Small labels and tag ink — between `fg` and `fg_muted`.
    pub tag: u32,
    /// Text/icons on `accent` fills.
    pub on_accent: u32,
    /// Text/icons on `warn` fills.
    pub on_warn: u32,

    /// Default 1px line (`--color-border` premixed).
    pub border: u32,
    /// Hairline — inner dividers, subtle rims.
    pub border_soft: u32,
    /// Stronger line — emphasized rims, scrollbar thumbs.
    pub edge: u32,

    /// Brand accent — links, primary buttons, selection (`--color-primary`).
    pub accent: u32,
    /// Hover on accent fills (`--color-primary-hover`).
    pub accent_hover: u32,
    /// Accent-colored text on surfaces (`--color-accent-foreground`).
    pub accent_fg: u32,
    /// Accent-tinted wells — selected rows, icon wells (`--color-accent`).
    pub accent_soft: u32,
    /// Border of `accent_soft` fills.
    pub accent_soft_edge: u32,

    /// Success — text/fill, soft bg, border, hover bg.
    pub ok: u32,
    pub ok_bg: u32,
    pub ok_border: u32,
    pub ok_hover: u32,
    /// Warning — text/fill, soft bg, border, hover bg.
    pub warn: u32,
    pub warn_bg: u32,
    pub warn_border: u32,
    pub warn_hover: u32,
    /// Destructive — text/fill, soft bg, border, badge text, hover bg.
    pub danger: u32,
    pub danger_bg: u32,
    pub danger_border: u32,
    pub danger_fg: u32,
    pub danger_hover: u32,
    /// Informational blue — linked/deploy chips.
    pub info: u32,
    pub info_bg: u32,
    pub info_border: u32,
    /// AI/violet accent — fill, text.
    pub violet: u32,
    pub violet_fg: u32,

    /// Accounts workbench tokens — the `dsh-plugin-oauth-subs` `.osubs`
    /// ramp, premixed `currentColor @ N%` over `panel` for each mode.
    /// `os_line`/`os_edge`/`os_hair` are line strengths; `os_fill*` are
    /// tinted wells; `os_*` text tones and status colors follow.
    pub os_surface: u32,
    pub os_ink: u32,
    pub os_line: u32,
    pub os_edge: u32,
    pub os_hair: u32,
    pub os_fill: u32,
    pub os_fill_2: u32,
    pub os_muted: u32,
    pub os_faint: u32,
    pub os_tag: u32,
    pub os_active_edge: u32,
    pub os_ok: u32,
    pub os_warn: u32,
    pub os_bad: u32,
}

/// OLED slate navy. Canvas sits near black so the rail and cards lift
/// cleanly off it; hairlines stay one step below `fg_muted` so structure
/// reads without competing with text.
const DARK: Palette = Palette {
    bg: 0x070b13,
    panel: 0x0e1626,
    panel_hover: 0x182234,
    panel_active: 0x1d2c4e,
    card: 0x1a2338,
    card_hover: 0x222c46,
    card_active: 0x1e2d52,
    card_active_hover: 0x243559,
    well: 0x141c30,
    input: 0x101a2e,
    scrim: 0x060a12,
    tip_bg: 0x000000,
    tip_fg: 0xffffff,

    fg: 0xeef2fa,
    fg_muted: 0x93a1b8,
    fg_faint: 0x6f7d96,
    tag: 0xbcc3d3,
    on_accent: 0xffffff,
    on_warn: 0x451a03,

    border: 0x2a3549,
    border_soft: 0x202a3f,
    edge: 0x42506c,

    accent: 0x3b82f6,
    accent_hover: 0x60a5fa,
    accent_fg: 0x93c5fd,
    accent_soft: 0x1c2b50,
    accent_soft_edge: 0x3a5aa8,

    ok: 0x34d399,
    ok_bg: 0x0d3529,
    ok_border: 0x146c50,
    ok_hover: 0x124637,
    warn: 0xfbbf24,
    warn_bg: 0x3a2a12,
    warn_border: 0xb45309,
    warn_hover: 0x4a3414,
    danger: 0xf87171,
    danger_bg: 0x431722,
    danger_border: 0x8d2f42,
    danger_fg: 0xfda4af,
    danger_hover: 0x522030,
    info: 0x60a5fa,
    info_bg: 0x1b2a4e,
    info_border: 0x3d5aa0,
    violet: 0x8b5cf6,
    violet_fg: 0xc4b5fd,

    os_surface: 0x0e1626,
    os_ink: 0xeef2fa,
    os_line: 0x333d50,
    os_edge: 0x525b6d,
    os_hair: 0x252d3f,
    os_fill: 0x1c2537,
    os_fill_2: 0x2a3345,
    os_muted: 0xa3abb9,
    os_faint: 0x9ea6b4,
    os_tag: 0xbfc5d1,
    os_active_edge: 0x97a0ae,
    os_ok: 0x74bf85,
    os_warn: 0xc88955,
    os_bad: 0xef9094,
};

/// Cool porcelain — a blue-tinted desk, not inverted dark. The canvas is
/// a soft slate so pure-white cards read as lifted; accent drops to
/// `#2563eb` so filled buttons keep AA contrast on white.
const LIGHT: Palette = Palette {
    bg: 0xe6eaf2,
    panel: 0xf7f9fc,
    panel_hover: 0xe9edf5,
    panel_active: 0xdfe8fa,
    card: 0xffffff,
    card_hover: 0xf1f4f9,
    card_active: 0xe4ecfd,
    card_active_hover: 0xd8e4fb,
    well: 0xeceff5,
    input: 0xffffff,
    scrim: 0x16202f,
    tip_bg: 0x000000,
    tip_fg: 0xffffff,

    fg: 0x16213c,
    fg_muted: 0x52617e,
    fg_faint: 0x8693ab,
    tag: 0x4d5c78,
    on_accent: 0xffffff,
    on_warn: 0xffffff,

    border: 0xd6dcea,
    border_soft: 0xe3e8f1,
    edge: 0xaebad0,

    accent: 0x2563eb,
    accent_hover: 0x1d4ed8,
    accent_fg: 0x1d4ed8,
    accent_soft: 0xe3ecfd,
    accent_soft_edge: 0xb6c9f5,

    ok: 0x15803d,
    ok_bg: 0xdcf5e4,
    ok_border: 0x8ed4a2,
    ok_hover: 0xc7eed4,
    warn: 0xb45309,
    warn_bg: 0xfdf0d0,
    warn_border: 0xefcd77,
    warn_hover: 0xfbe6b3,
    danger: 0xdc2626,
    danger_bg: 0xfbe7e9,
    danger_border: 0xefaab0,
    danger_fg: 0xb91c1c,
    danger_hover: 0xf6d3d7,
    info: 0x1d4ed8,
    info_bg: 0xe1eafc,
    info_border: 0xaec3f2,
    violet: 0x7c3aed,
    violet_fg: 0x6d28d9,

    os_surface: 0xffffff,
    os_ink: 0x101216,
    os_line: 0xd9d9da,
    os_edge: 0xb7b7b9,
    os_hair: 0xe7e7e7,
    os_fill: 0xf1f1f1,
    os_fill_2: 0xe9e9e9,
    os_muted: 0x66676a,
    os_faint: 0x66676a,
    os_tag: 0x48494b,
    os_active_edge: 0x78797c,
    os_ok: 0x266a34,
    os_warn: 0x8a411f,
    os_bad: 0x943335,
};

/// `true` while the light palette is active. Mirrors `UiLang`: a plain
/// atomic so `palette()` stays a free function callable from any render
/// or helper without threading `cx` through.
static LIGHT_MODE: AtomicBool = AtomicBool::new(false);

/// Active palette — `DARK` or `LIGHT` by the current mode.
pub fn palette() -> &'static Palette {
    if is_light() { &LIGHT } else { &DARK }
}

pub fn is_light() -> bool {
    LIGHT_MODE.load(Ordering::Relaxed)
}

/// Wash behind every prompt dialog.
///
/// gpui-pre 0.3.8 cannot sample the framebuffer, so this is a light tint
/// rather than a gaussian backdrop blur. Alpha stays low enough that the
/// page underneath remains visible. Kit dialogs read the same color from
/// `ThemeColor.overlay`; in-page prompts paint [`prompt_veil`] directly.
pub fn prompt_veil() -> Hsla {
    rgb(0x000000).alpha(prompt_veil_alpha(is_light())).into()
}

fn prompt_veil_alpha(light: bool) -> f32 {
    if light { 0.18 } else { 0.28 }
}

fn gpui_mode() -> ThemeMode {
    if is_light() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    }
}

/// Seed the mode from `gui_prefs.json` and paint the kit theme before
/// the first frame. Called once from `run()`.
pub fn install(cx: &mut App) {
    set_mode(
        prefs_style_is_light(&crate::prefs::load().background_style),
        cx,
    );
}

/// `background_style` pref → mode. `"paper"` is light; anything else
/// follows the React default, dark (`"current"`).
fn prefs_style_is_light(style: &str) -> bool {
    style == "paper"
}

/// Flip light ↔ dark, persist `background_style`, repaint.
pub fn toggle(cx: &mut App) {
    set_mode(!is_light(), cx);
}

/// Set the mode explicitly — the Settings appearance picker and the
/// sidebar button both land here.
pub fn set_mode(light: bool, cx: &mut App) {
    LIGHT_MODE.store(light, Ordering::Relaxed);
    let mut prefs = crate::prefs::load();
    let style = if light { "paper" } else { "current" };
    if prefs.background_style != style {
        prefs.background_style = style.into();
        if let Err(err) = crate::prefs::save(&prefs) {
            tracing::warn!("failed to save gui prefs: {err}");
        }
    }
    apply(cx);
}

/// Push the active palette onto the kit [`Theme`] and repaint every
/// window. `mode` is set first so the kit flips its dark/light branches;
/// a second `update` re-asserts our colors over whatever the registered
/// config loaded, then re-asserts the bundled fonts for the same reason.
pub fn apply(cx: &mut App) {
    Theme::update(cx, |theme| theme.mode = gpui_mode());
    Theme::update(cx, |theme| {
        theme.colors = theme_colors();
        theme.font_family = "DM Sans".into();
        theme.mono_font_family = "JetBrains Mono".into();
    });
}

/// Project the active palette onto `gpui-component`'s `ThemeColor`. Fields
/// the app doesn't map keep the kit's own defaults for the mode, so any
/// component we haven't styled still looks sane.
fn theme_colors() -> ThemeColor {
    let p = palette();
    let c = |hex: u32| -> Hsla { rgb(hex).into() };
    let base = if is_light() {
        ThemeColor::light()
    } else {
        ThemeColor::dark()
    };
    ThemeColor {
        background: c(p.panel),
        foreground: c(p.fg),
        muted: c(p.well),
        muted_foreground: c(p.fg_muted),
        border: c(p.border),
        ring: c(p.accent),
        selection: c(p.accent_soft),
        caret: c(p.fg),

        primary: c(p.accent),
        primary_foreground: c(p.on_accent),
        primary_hover: c(p.accent_hover),
        primary_active: c(p.accent_hover),
        secondary: c(p.well),
        secondary_foreground: c(p.fg),
        secondary_hover: c(p.card_hover),
        secondary_active: c(p.card_active),
        accent: c(p.accent_soft),
        accent_foreground: c(p.accent_fg),

        danger: c(p.danger),
        danger_foreground: c(p.on_accent),
        danger_hover: c(p.danger_hover),
        danger_active: c(p.danger_hover),
        warning: c(p.warn),
        warning_foreground: c(p.on_warn),
        warning_hover: c(p.warn_hover),
        warning_active: c(p.warn_hover),
        success: c(p.ok),
        success_foreground: c(p.on_accent),
        success_hover: c(p.ok_hover),
        success_active: c(p.ok_hover),
        info: c(p.info),
        info_foreground: c(p.on_accent),
        info_hover: c(p.info_bg),
        info_active: c(p.info_bg),

        input: c(p.border),
        popover: c(p.card),
        popover_foreground: c(p.fg),
        group_box: c(p.card),
        group_box_foreground: c(p.fg),
        description_list_label: c(p.well),
        description_list_label_foreground: c(p.fg_muted),
        accordion: c(p.card),

        sidebar: c(p.panel),
        sidebar_foreground: c(p.fg),
        sidebar_border: c(p.border),
        sidebar_accent: c(p.panel_active),
        sidebar_accent_foreground: c(p.accent_fg),
        sidebar_primary: c(p.accent),
        sidebar_primary_foreground: c(p.on_accent),

        list: c(p.panel),
        list_even: c(p.panel),
        list_head: c(p.well),
        list_hover: c(p.panel_hover),
        list_active: c(p.panel_active),
        list_active_border: c(p.accent),

        tab: c(p.panel),
        tab_bar: c(p.panel),
        tab_bar_segmented: c(p.well),
        tab_foreground: c(p.fg_muted),
        tab_active: c(p.panel_active),
        tab_active_foreground: c(p.fg),

        table: c(p.panel),
        table_head: c(p.well),
        table_head_foreground: c(p.fg_muted),
        table_even: c(p.well),
        table_hover: c(p.panel_hover),
        table_active: c(p.panel_active),
        table_active_border: c(p.accent),
        table_row_border: c(p.border),
        table_foot: c(p.well),
        table_foot_foreground: c(p.fg_muted),

        button: c(p.well),
        button_foreground: c(p.fg),
        button_hover: c(p.card_hover),
        button_active: c(p.card_active),
        button_primary: c(p.accent),
        button_primary_foreground: c(p.on_accent),
        button_primary_hover: c(p.accent_hover),
        button_primary_active: c(p.accent_hover),
        button_secondary: c(p.well),
        button_secondary_foreground: c(p.fg),
        button_secondary_hover: c(p.card_hover),
        button_secondary_active: c(p.card_active),
        button_danger: c(p.danger),
        button_danger_foreground: c(p.on_accent),
        button_danger_hover: c(p.danger_hover),
        button_danger_active: c(p.danger_hover),
        button_success: c(p.ok),
        button_success_foreground: c(p.on_accent),
        button_success_hover: c(p.ok_hover),
        button_success_active: c(p.ok_hover),
        button_warning: c(p.warn),
        button_warning_foreground: c(p.on_warn),
        button_warning_hover: c(p.warn_hover),
        button_warning_active: c(p.warn_hover),
        button_info: c(p.info),
        button_info_foreground: c(p.on_accent),
        button_info_hover: c(p.info_bg),
        button_info_active: c(p.info_bg),

        switch: c(p.edge),
        // The thumb must clear the `edge` track in both themes. `panel` is
        // the card surface itself and disappears on it; `on_accent` is white
        // in both, matching the switch this projection was written for.
        switch_thumb: c(p.on_accent),
        slider_bar: c(p.well),
        slider_thumb: c(p.accent),
        progress_bar: c(p.accent),
        skeleton: c(p.well),

        // Translucent thumb on a transparent track — same treatment as
        // `::-webkit-scrollbar` in `src/index.css` and the
        // `install_scrollbar_theme` projection in `lib.rs`.
        scrollbar: c(p.panel).alpha(0.0),
        scrollbar_thumb: c(p.fg).alpha(if is_light() { 0.14 } else { 0.18 }),
        scrollbar_thumb_hover: c(p.fg).alpha(if is_light() { 0.24 } else { 0.32 }),

        link: c(p.accent_fg),
        link_hover: c(p.accent_hover),
        link_active: c(p.accent),

        title_bar: c(p.panel),
        title_bar_border: c(p.border),
        status_bar: c(p.panel),
        status_bar_border: c(p.border),
        window_border: c(p.border),
        drag_border: c(p.accent),
        drop_target: c(p.accent_soft),
        overlay: prompt_veil(),

        chart_1: c(p.accent),
        chart_2: c(p.ok),
        chart_3: c(p.warn),
        chart_4: c(p.violet),
        chart_5: c(p.info),
        chart_bullish: c(p.ok),
        chart_bearish: c(p.danger),
        chart_grid: c(p.border_soft),

        red: c(p.danger),
        red_light: c(p.danger_fg),
        green: c(p.ok),
        green_light: c(p.ok_border),
        blue: c(p.accent),
        blue_light: c(p.accent_fg),
        yellow: c(p.warn),
        yellow_light: c(p.warn_border),
        magenta: c(p.violet),
        magenta_light: c(p.violet_fg),
        ..*base
    }
}

/// HSL lerp between two `0xRRGGBB` colors — used by the quota bars so the
/// midpoint stays amber instead of going muddy through sRGB.
pub fn mix_hsl(from: u32, to: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 {
        return from;
    }
    if t == 1.0 {
        return to;
    }
    let (h1, s1, l1) = rgb_to_hsl(from);
    let (h2, s2, l2) = rgb_to_hsl(to);
    hsl_to_rgb(lerp_hue(h1, h2, t), s1 + (s2 - s1) * t, l1 + (l2 - l1) * t)
}

fn rgb_to_hsl(rgb: u32) -> (f32, f32, f32) {
    let r = ((rgb >> 16) & 0xff) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xff) as f32 / 255.0;
    let b = (rgb & 0xff) as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f32::EPSILON {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn lerp_hue(a: f32, b: f32, t: f32) -> f32 {
    let mut d = b - a;
    if d > 0.5 {
        d -= 1.0;
    }
    if d < -0.5 {
        d += 1.0;
    }
    (a + d * t).rem_euclid(1.0)
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> u32 {
    let (r, g, b) = if s <= f32::EPSILON {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        (
            hue_to_rgb(p, q, h + 1.0 / 3.0),
            hue_to_rgb(p, q, h),
            hue_to_rgb(p, q, h - 1.0 / 3.0),
        )
    };
    let ch = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u32;
    (ch(r) << 16) | (ch(g) << 8) | ch(b)
}

#[cfg(test)]
mod tests {
    use super::{DARK, LIGHT, mix_hsl, palette, prompt_veil_alpha};

    fn rel_luminance(hex: u32) -> f32 {
        let f = |v: u32| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(hex >> 16) + 0.7152 * f(hex >> 8 & 0xff) + 0.0722 * f(hex & 0xff)
    }

    fn contrast(a: u32, b: u32) -> f32 {
        let (hi, lo) = (
            rel_luminance(a).max(rel_luminance(b)),
            rel_luminance(a).min(rel_luminance(b)),
        );
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn text_tiers_hold_wcag_aa_on_their_surfaces() {
        for p in [&DARK, &LIGHT] {
            assert!(contrast(p.fg, p.panel) > 4.5, "fg on panel");
            assert!(contrast(p.fg, p.card) > 4.5, "fg on card");
            assert!(contrast(p.fg_muted, p.panel) > 4.5, "muted on panel");
            assert!(contrast(p.fg_muted, p.card) > 4.5, "muted on card");
            assert!(contrast(p.accent_fg, p.panel) > 3.0, "accent text on panel");
            assert!(contrast(p.on_accent, p.accent) > 3.0, "text on accent");
            assert!(contrast(p.danger, p.card) > 3.0, "danger on card");
            assert!(contrast(p.danger_fg, p.danger_bg) > 3.0, "danger badge");
            assert!(contrast(p.warn, p.warn_bg) > 3.0, "warn badge");
            assert!(contrast(p.ok, p.ok_bg) > 3.0, "ok badge");
        }
    }

    #[test]
    fn elevation_ramps_are_monotonic() {
        // Dark: canvas is the deepest, cards sit above panels.
        assert!(rel_luminance(DARK.bg) < rel_luminance(DARK.panel));
        assert!(rel_luminance(DARK.panel) < rel_luminance(DARK.card));
        // Light: canvas is the darkest surface, cards sit above it.
        assert!(rel_luminance(LIGHT.card) > rel_luminance(LIGHT.panel));
        assert!(rel_luminance(LIGHT.panel) > rel_luminance(LIGHT.bg));
    }

    #[test]
    fn default_mode_is_dark_to_match_react() {
        assert_eq!(palette().bg, DARK.bg);
    }

    #[test]
    fn prompt_veil_keeps_the_page_visible() {
        let light = prompt_veil_alpha(true);
        let dark = prompt_veil_alpha(false);
        assert!(light > 0.0 && light < 0.25, "light veil {light}");
        assert!(dark > light && dark < 0.4, "dark veil {dark}");
    }

    #[test]
    fn mix_hsl_endpoints_and_midpoint() {
        assert_eq!(mix_hsl(0x000000, 0xffffff, 0.0), 0x000000);
        assert_eq!(mix_hsl(0x000000, 0xffffff, 1.0), 0xffffff);
        let mid = mix_hsl(0xf87171, 0x34d399, 0.5);
        assert_ne!(mid, 0x000000);
    }
}
