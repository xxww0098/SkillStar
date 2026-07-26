//! Multi-window support for the usage page: lightweight "floating card"
//! windows that show a single subscription's quota in a small always-on-top
//! window, mirroring cockpit-tools' per-instance floating card.
//!
//! Each card window is a separate Tauri webview that loads the same
//! `index.html` entry with a `?window=usage-card&id=<subscription_id>` query.
//! The frontend (`main.tsx`) reads the window label / query param and renders
//! a stripped-down `UsageCardWindow` root instead of the full app (first
//! window-label-routed surface in the codebase).
//!
//! Card windows share the usage command surface with the main window; the
//! `capabilities/usage-card.json` capability grants them the same core +
//! usage permissions the main window has.

use std::collections::HashSet;

use skillstar_core::infra::error::AppError;
use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, Position, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

/// Label prefix for usage card windows. The full label is
/// `usage-card-<sanitized-subscription-id>` so each subscription gets its own
/// window and we can look it up / close it by subscription id.
pub const USAGE_CARD_LABEL_PREFIX: &str = "usage-card-";

/// Event broadcast when the active account for a catalog changes, so every
/// open card window can refresh its own `is_active` indicator without polling.
pub const USAGE_ACTIVE_CHANGED_EVENT: &str = "usage://active-changed";

/// Cascade offset (px) between stacked card windows so they don't overlap
/// perfectly when several are open.
const CARD_OFFSET_STEP: i32 = 28;
const CARD_DEFAULT_MARGIN: i32 = 20;
const CARD_WIDTH: f64 = 360.0;
const CARD_HEIGHT: f64 = 480.0;

/// Sanitise an arbitrary string into a window-label-safe segment (alphanumeric
/// + `-`/`_` only), matching Tauri's label constraints.
fn sanitize_label_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').trim_matches('_');
    if trimmed.is_empty() {
        "card".to_string()
    } else {
        trimmed.to_string()
    }
}

fn card_label(subscription_id: &str) -> String {
    format!(
        "{USAGE_CARD_LABEL_PREFIX}{}",
        sanitize_label_segment(subscription_id)
    )
}

/// Open (or focus) a floating card window for `subscription_id`.
///
/// If a window with that label already exists it is just shown + focused
/// (and pulled back on-screen if a previous coordinate bug left it off-display);
/// otherwise a new always-on-top, frameless window is created loading
/// `index.html?window=usage-card&id=<subscription_id>`.
#[tauri::command]
pub fn open_usage_card_window(app: AppHandle, subscription_id: String) -> Result<(), AppError> {
    let label = card_label(&subscription_id);
    if let Some(window) = app.get_webview_window(&label) {
        // Re-clamp if a prior Retina physical/logical mix parked the window
        // off-screen (users then report the button as "dead").
        ensure_card_window_on_screen(&app, &window);
        window
            .show()
            .map_err(|e| AppError::Other(format!("显示用量卡片失败：{e}")))?;
        window
            .unminimize()
            .map_err(|e| AppError::Other(format!("取消最小化失败：{e}")))?;
        window
            .set_focus()
            .map_err(|e| AppError::Other(format!("聚焦失败：{e}")))?;
        return Ok(());
    }

    let url = WebviewUrl::App(
        format!(
            "index.html?window=usage-card&id={}",
            urlencoding_minimal(&subscription_id),
        )
        .into(),
    );

    let mut builder = WebviewWindowBuilder::new(&app, &label, url)
        .title("SkillStar 用量卡片")
        .inner_size(CARD_WIDTH, CARD_HEIGHT)
        .min_inner_size(280.0, 360.0)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false);

    // Position: cascade from top-right based on how many cards are already open.
    // `WebviewWindowBuilder::position` takes **logical** pixels; monitor work
    // area is physical — convert via scale_factor or Retina/4K parks the card
    // off-screen (e.g. x≈3460 on a 1920-logical main display).
    if let Ok(Some((x, y))) = next_cascade_position_logical(&app) {
        builder = builder.position(x, y);
    }

    let window = builder
        .build()
        .map_err(|e| AppError::Other(format!("创建用量卡片窗口失败：{e}")))?;

    // macOS needs the window shown after build when created hidden.
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

/// Close a single card window by subscription id. No-op if it isn't open.
#[tauri::command]
pub fn close_usage_card_window(app: AppHandle, subscription_id: String) -> Result<(), AppError> {
    let label = card_label(&subscription_id);
    if let Some(window) = app.get_webview_window(&label) {
        window
            .close()
            .map_err(|e| AppError::Other(format!("关闭用量卡片失败：{e}")))?;
    }
    Ok(())
}

/// Close every open usage card window (e.g. on app quit).
#[tauri::command]
pub fn close_all_usage_card_windows(app: AppHandle) -> Result<(), AppError> {
    for (label, window) in app.webview_windows() {
        if label.starts_with(USAGE_CARD_LABEL_PREFIX) {
            let _ = window.close();
        }
    }
    Ok(())
}

/// Broadcast that the active account for `catalog_id` changed (called by
/// `set_active_subscription` after a successful pin). Open card windows
/// subscribe to refresh their own `is_active` badge.
pub fn emit_active_changed<R: Runtime>(
    app: &AppHandle<R>,
    catalog_id: &str,
    subscription_id: &str,
) {
    let payload = serde_json::json!({
        "catalogId": catalog_id,
        "subscriptionId": subscription_id,
    });
    let _ = app.emit(USAGE_ACTIVE_CHANGED_EVENT, payload);
}

/// Close the card window bound to `subscription_id` if one is open. Used by
/// `delete_subscription` so deleting an account also dismisses its card.
pub fn close_card_for_subscription<R: Runtime>(app: &AppHandle<R>, subscription_id: &str) {
    let label = card_label(subscription_id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
}

/// Pure cascade math in **logical** pixels (unit-tested).
///
/// Places the card at the top-right of the work area, stepping down-left by
/// `CARD_OFFSET_STEP` for each already-open card.
pub(crate) fn cascade_logical_xy(
    origin_x: f64,
    origin_y: f64,
    logical_width: f64,
    stack_index: i32,
) -> (f64, f64) {
    let offset = f64::from(stack_index.max(0)) * f64::from(CARD_OFFSET_STEP);
    let x = origin_x + logical_width - CARD_WIDTH - f64::from(CARD_DEFAULT_MARGIN) - offset;
    let y = origin_y + f64::from(CARD_DEFAULT_MARGIN) + offset;
    (x.max(origin_x), y)
}

/// Compute the top-right cascade position for the next card window in **logical**
/// pixels (what `WebviewWindowBuilder::position` expects).
fn next_cascade_position_logical<R: Runtime>(app: &AppHandle<R>) -> Result<Option<(f64, f64)>, ()> {
    let monitor = app.primary_monitor().map_err(|_| ())?.ok_or(())?;
    let scale = monitor.scale_factor();
    if !(scale.is_finite() && scale > 0.0) {
        return Err(());
    }
    let work_area = monitor.work_area(); // physical
    let origin_x = f64::from(work_area.position.x) / scale;
    let origin_y = f64::from(work_area.position.y) / scale;
    let logical_width = f64::from(work_area.size.width) / scale;
    let stack_index = count_visible_card_windows(app);
    Ok(Some(cascade_logical_xy(
        origin_x,
        origin_y,
        logical_width,
        stack_index,
    )))
}

/// If `window` is outside the primary work area (common after the pre-fix
/// physical/logical mix-up), snap it back to the top-right cascade slot.
fn ensure_card_window_on_screen<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let Ok(Some(monitor)) = app.primary_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    if !(scale.is_finite() && scale > 0.0) {
        return;
    }
    let work = monitor.work_area();
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };

    // Allow a small margin; treat "mostly outside" as off-screen.
    let slack = 40i32;
    let left = work.position.x - slack;
    let top = work.position.y - slack;
    let right = work.position.x + i32::try_from(work.size.width).unwrap_or(0) + slack;
    let bottom = work.position.y + i32::try_from(work.size.height).unwrap_or(0) + slack;
    let win_right = pos.x.saturating_add(i32::try_from(size.width).unwrap_or(0));
    let win_bottom = pos
        .y
        .saturating_add(i32::try_from(size.height).unwrap_or(0));

    let fully_off = win_right < left || win_bottom < top || pos.x > right || pos.y > bottom;
    if !fully_off {
        return;
    }

    // Place as the first cascade slot (ignore stack — this is a recovery path).
    let origin_x = f64::from(work.position.x) / scale;
    let origin_y = f64::from(work.position.y) / scale;
    let logical_width = f64::from(work.size.width) / scale;
    let (x, y) = cascade_logical_xy(origin_x, origin_y, logical_width, 0);
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
}

fn count_visible_card_windows<R: Runtime>(app: &AppHandle<R>) -> i32 {
    app.webview_windows()
        .values()
        .filter(|w| {
            w.label().starts_with(USAGE_CARD_LABEL_PREFIX) && w.is_visible().unwrap_or(false)
        })
        .count() as i32
}

/// Minimal percent-encoding for a subscription id in a query string (ids are
/// uuids so this is mostly a safety net, but keeps the URL well-formed).
fn urlencoding_minimal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &b in value.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// All usage-card window labels currently open (used by tests / diagnostics).
#[allow(dead_code)]
fn card_window_labels<R: Runtime>(app: &AppHandle<R>) -> HashSet<String> {
    app.webview_windows()
        .keys()
        .filter(|l| l.starts_with(USAGE_CARD_LABEL_PREFIX))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_alnum_and_dashes() {
        assert_eq!(sanitize_label_segment("abc-123_xyz"), "abc-123_xyz");
    }

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(sanitize_label_segment("a/b c"), "a-b-c");
    }

    #[test]
    fn sanitize_empty_falls_back_to_card() {
        assert_eq!(sanitize_label_segment("///"), "card");
    }

    #[test]
    fn card_label_has_prefix() {
        assert_eq!(card_label("550e8400-e29b"), "usage-card-550e8400-e29b");
    }

    #[test]
    fn urlencoding_passes_safe_chars_through() {
        assert_eq!(urlencoding_minimal("abc-1_2.3~"), "abc-1_2.3~");
    }

    #[test]
    fn urlencoding_encodes_unsafe() {
        assert_eq!(urlencoding_minimal("a b"), "a%20b");
        assert_eq!(urlencoding_minimal("a/b"), "a%2Fb");
    }

    #[test]
    fn cascade_logical_places_top_right_of_1920_display() {
        // Bug repro: physical 3840×2160 @ scale 2 → logical 1920×1080.
        // Pre-fix code fed physical width into logical position → x≈3460 (off-screen).
        let (x, y) = cascade_logical_xy(0.0, 0.0, 1920.0, 0);
        assert!((x - (1920.0 - CARD_WIDTH - f64::from(CARD_DEFAULT_MARGIN))).abs() < 0.01);
        assert!((y - f64::from(CARD_DEFAULT_MARGIN)).abs() < 0.01);
        assert!(
            x < 1920.0,
            "card origin must stay inside logical width, got {x}"
        );
        assert!(x + CARD_WIDTH <= 1920.0 + 0.01);
    }

    #[test]
    fn cascade_logical_steps_for_stack() {
        let (x0, y0) = cascade_logical_xy(0.0, 0.0, 1920.0, 0);
        let (x1, y1) = cascade_logical_xy(0.0, 0.0, 1920.0, 1);
        assert!((x0 - x1 - f64::from(CARD_OFFSET_STEP)).abs() < 0.01);
        assert!((y1 - y0 - f64::from(CARD_OFFSET_STEP)).abs() < 0.01);
    }
}
