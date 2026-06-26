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
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindowBuilder};

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
    format!("{USAGE_CARD_LABEL_PREFIX}{}", sanitize_label_segment(subscription_id))
}

/// Open (or focus) a floating card window for `subscription_id`.
///
/// If a window with that label already exists it is just shown + focused;
/// otherwise a new always-on-top, frameless, transparent window is created
/// loading `index.html?window=usage-card&id=<subscription_id>`.
#[tauri::command]
pub fn open_usage_card_window(
    app: AppHandle,
    subscription_id: String,
) -> Result<(), AppError> {
    let label = card_label(&subscription_id);
    if let Some(window) = app.get_webview_window(&label) {
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

    let url = WebviewUrl::App(format!(
        "index.html?window=usage-card&id={}",
        urlencoding_minimal(&subscription_id),
    )
    .into());

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
    if let Ok(Some(position)) = next_cascade_position(&app) {
        builder = builder.position(position.x as f64, position.y as f64);
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
pub fn close_usage_card_window(
    app: AppHandle,
    subscription_id: String,
) -> Result<(), AppError> {
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
pub fn emit_active_changed<R: Runtime>(app: &AppHandle<R>, catalog_id: &str, subscription_id: &str) {
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

/// Compute the top-right cascade position for the next card window, offset by
/// the number of currently-visible cards so stacks fan out instead of piling
/// exactly on top of each other.
fn next_cascade_position<R: Runtime>(app: &AppHandle<R>) -> Result<Option<PhysicalPosition<i32>>, ()> {
    let monitor = app.primary_monitor().map_err(|_| ())?.ok_or(())?;
    let work_area = monitor.work_area();
    let stack_index = count_visible_card_windows(app);
    let offset = stack_index * CARD_OFFSET_STEP;
    let x = work_area.position.x
        + i32::try_from(work_area.size.width).unwrap_or(0)
        - CARD_WIDTH as i32
        - CARD_DEFAULT_MARGIN
        - offset;
    let y = work_area.position.y + CARD_DEFAULT_MARGIN + offset;
    Ok(Some(PhysicalPosition::new(x.max(work_area.position.x), y)))
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
}
