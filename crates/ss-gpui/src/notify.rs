//! Transient toasts — the GPUI counterpart of React `sonner`.
//!
//! `gpui_component::init` registers `WindowState` on the window's `Root`
//! (mounted in `lib.rs`), which owns the `NotificationList` layer. Pages
//! push a `Notification` here instead of parking text in their toolbar;
//! the list animates in top-right and autohides.

use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use ss_skills::workflows::agent_links::AgentLinkReport;

/// Push `note` onto the active window's notification stack. The shell has
/// one window; a missing or already-closed one drops the toast, matching
/// how `Shell` tolerates `active_window` misses.
pub(crate) fn toast(note: impl Into<Notification>, cx: &mut App) {
    let Some(window) = cx.active_window() else {
        return;
    };
    let _ = window.update(cx, |_, window, cx| {
        window.push_notification(note, cx);
    });
}

/// Warn when an install-time deploy left some `(Skill, Agent)` pairs unlinked.
pub(crate) fn agent_link_problems(report: &AgentLinkReport, cx: &mut App) {
    if let Some(notice) = agent_link_notice(report) {
        toast(Notification::warning(notice), cx);
    }
}

/// Localized one-paragraph summary of failed and skipped pairs.
pub(crate) fn agent_link_notice(report: &AgentLinkReport) -> Option<String> {
    let total = (report.applied.len() + report.skipped.len() + report.failed.len()).to_string();
    let mut parts = Vec::new();
    if let Some(first) = report.failed.first() {
        parts.push(format!(
            "{}: {} → {}: {}",
            crate::i18n::tf(
                "skillCards.batchTogglePartialFailed",
                &[
                    ("failed", &report.failed.len().to_string()),
                    ("total", &total)
                ],
            ),
            first.skill,
            first.agent_id,
            first.reason
        ));
    }
    if !report.skipped.is_empty() {
        parts.push(
            crate::i18n::tf(
                "skillCards.batchTogglePartialSkipped",
                &[
                    ("skipped", &report.skipped.len().to_string()),
                    ("total", &total),
                ],
            )
            .to_string(),
        );
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}
