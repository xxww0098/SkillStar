//! User-facing copy for a managed-skills toggle: what succeeded, what was
//! skipped, and what failed.
//!
//! Outcomes leave the page through `notify::toast` into the shell's alert
//! band. Nothing is parked in the Settings row: the page entity is KeepAlive
//! for the whole session, so inline text there has no lifetime of its own.

use gpui_kit::*;
use ss_skills::workflows::agent_managed_skills::{
    AgentManagedSkillsAction, AgentManagedSkillsSkip, AgentManagedSkillsToggleReport,
};

use super::state::{AgentNotice, AgentTone, SKIP_UNMANAGED_REAL_DIRECTORY};
use crate::i18n::{t, tf};
use crate::notify::Notice;

/// Which alert family an outcome belongs to. Alerts in one family replace
/// each other; families stack, so a failed re-read cannot erase the result
/// that triggered it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NoticeKind {
    /// A command the user just ran: clear, pause/restore, link or unlink.
    /// Autohides like any ordinary status.
    Result,
    /// A background read that failed: preload or refresh. Pinned open — the row
    /// keeps its loading state, so the reason must not fade on its own.
    Read,
}

/// Show one outcome as an alert in the shell's band.
///
/// A result alert autohides. A failed read stays until the user closes it.
/// A skipped item keeps an "open the occupied folder" action, and an action
/// pins the alert open — closing it is how the user accepts the skip.
pub(super) fn show_notice(notice: AgentNotice, kind: NoticeKind, key: &str, cx: &mut App) {
    let mut alert = match notice.tone {
        AgentTone::Ok => Notice::success(notice.text),
        AgentTone::Warn => Notice::warning(notice.text),
        AgentTone::Error => Notice::error(notice.text),
    };
    alert = match kind {
        NoticeKind::Result => alert.replace_key(format!("managed-result-{key}")),
        NoticeKind::Read => alert.replace_key(format!("managed-read-{key}")).pinned(),
    };
    if let Some(path) = notice.open_path {
        alert = alert
            .pinned()
            .action(t("skillToggle.openOccupiedFolder"), move |_, _| {
                crate::os_open::open_folder(&path);
            });
    }
    crate::notify::toast(alert, cx);
}

/// Success copy for the one-click clear: the domain layer counts what it
/// actually removed, so zero removals (nothing managed, or only unmanaged
/// real directories) reads as "nothing to clear" rather than a silent success.
pub(super) fn cleared_links_notice(removed: u32, name: &str) -> AgentNotice {
    if removed == 0 {
        return AgentNotice::ok(t("settings.unlinkAllFromAgentNone"));
    }
    AgentNotice::ok(tf(
        "settings.unlinkAllFromAgentDone",
        &[("count", &removed.to_string()), ("name", name)],
    ))
}

pub(super) fn notice_from_report(
    name: &str,
    report: &AgentManagedSkillsToggleReport,
) -> AgentNotice {
    let failed = report.failed.len();
    let skipped = report.skipped.len();
    let total = report.succeeded.len() + skipped + failed;
    let paused = report.action == AgentManagedSkillsAction::Paused;
    let mut details = report
        .failed
        .iter()
        .take(3)
        .map(|failure| format!("{}: {}", failure.skill_name, failure.error))
        .collect::<Vec<_>>();
    let hidden_failures = failed.saturating_sub(details.len());
    if hidden_failures > 0 {
        details.push(
            tf(
                "settings.managedSkillsMoreFailures",
                &[("count", &hidden_failures.to_string())],
            )
            .to_string(),
        );
    }
    let visible_skips = report
        .skipped
        .iter()
        .take(3)
        .map(format_skip)
        .collect::<Vec<_>>();
    let hidden_skips = skipped.saturating_sub(visible_skips.len());
    details.extend(visible_skips);
    if hidden_skips > 0 {
        details.push(
            tf(
                "settings.managedSkillsMoreSkipped",
                &[("count", &hidden_skips.to_string())],
            )
            .to_string(),
        );
    }
    let open_path = report
        .skipped
        .iter()
        .find(|skip| !skip.path.trim().is_empty())
        .map(|skip| skip.path.clone());
    let (tone, head) = if failed > 0 {
        let key = match (paused, skipped > 0) {
            (true, true) => "settings.managedSkillsPausePartialMixed",
            (true, false) => "settings.managedSkillsPausePartialFailed",
            (false, true) => "settings.managedSkillsRestorePartialMixed",
            (false, false) => "settings.managedSkillsRestorePartialFailed",
        };
        (
            AgentTone::Error,
            tf(
                key,
                &[
                    ("failed", &failed.to_string()),
                    ("skipped", &skipped.to_string()),
                    ("total", &total.to_string()),
                ],
            )
            .to_string(),
        )
    } else if skipped > 0 {
        let key = if paused {
            "settings.managedSkillsPausePartialSkipped"
        } else {
            "settings.managedSkillsRestorePartialSkipped"
        };
        (
            AgentTone::Warn,
            tf(
                key,
                &[
                    ("skipped", &skipped.to_string()),
                    ("total", &total.to_string()),
                ],
            )
            .to_string(),
        )
    } else {
        let key = if paused {
            "settings.managedSkillsPaused"
        } else {
            "settings.managedSkillsRestored"
        };
        (
            AgentTone::Ok,
            tf(
                key,
                &[
                    ("count", &report.succeeded.len().to_string()),
                    ("name", name),
                ],
            )
            .to_string(),
        )
    };
    let text = if details.is_empty() {
        head
    } else {
        format!("{head} · {}", details.join(" · "))
    };
    AgentNotice {
        tone,
        text,
        open_path,
    }
}

fn format_skip(skip: &AgentManagedSkillsSkip) -> String {
    if skip.code == SKIP_UNMANAGED_REAL_DIRECTORY {
        return tf(
            "skillToggle.skipUnmanagedDirItem",
            &[("name", &skip.skill_name), ("path", &skip.path)],
        )
        .to_string();
    }
    if skip.reason.is_empty() {
        skip.skill_name.clone()
    } else {
        format!("{}: {}", skip.skill_name, skip.reason)
    }
}
