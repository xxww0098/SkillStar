//! Transient notices — SkillStar's alert band.
//!
//! Pages call [`toast`] with a [`Notice`]; the shell's [`NoticeBoard`]
//! renders them as `gpui-kit` `Alert` banners docked to the bottom of the
//! main panel. Unpinned notices age out after [`NOTICE_TTL`]; pinned ones
//! stay until the user closes them. Notices sharing a replace key displace
//! each other, so a retried operation never stacks duplicates.

use std::rc::Rc;
use std::time::Duration;

use gpui::{App, Context, Entity, Global, SharedString, WeakEntity, Window};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::Button;
use gpui_kit::*;
use ss_skills::workflows::agent_links::AgentLinkReport;

/// How long an unpinned notice stays on the band. Carried over from the
/// kit notification layer the Alert migration replaced.
const NOTICE_TTL: Duration = Duration::from_secs(5);

/// Upper bound on simultaneous notices; the oldest unpinned one is dropped
/// first so a pinned reason can never be crowded out by a burst.
const NOTICE_CAP: usize = 4;

pub(crate) enum NoticeTone {
    Info,
    Success,
    Warning,
    Error,
}

/// An inline button rendered beside the alert (e.g. "open the occupied
/// folder" on a pinned skip reason).
pub(crate) struct NoticeAction {
    label: SharedString,
    run: Rc<dyn Fn(&mut Window, &mut App)>,
}

/// One user-facing outcome. Plain data; the board owns scheduling and layout.
pub(crate) struct Notice {
    tone: NoticeTone,
    message: SharedString,
    replace_key: Option<SharedString>,
    pinned: bool,
    action: Option<NoticeAction>,
}

impl Notice {
    pub(crate) fn info(message: impl Into<SharedString>) -> Self {
        Self::new(NoticeTone::Info, message)
    }

    pub(crate) fn success(message: impl Into<SharedString>) -> Self {
        Self::new(NoticeTone::Success, message)
    }

    pub(crate) fn warning(message: impl Into<SharedString>) -> Self {
        Self::new(NoticeTone::Warning, message)
    }

    pub(crate) fn error(message: impl Into<SharedString>) -> Self {
        Self::new(NoticeTone::Error, message)
    }

    fn new(tone: NoticeTone, message: impl Into<SharedString>) -> Self {
        Self {
            tone,
            message: message.into(),
            replace_key: None,
            pinned: false,
            action: None,
        }
    }

    /// Notices sharing a replace key displace each other; unkeyed notices
    /// stack freely.
    pub(crate) fn replace_key(mut self, key: impl Into<SharedString>) -> Self {
        self.replace_key = Some(key.into());
        self
    }

    /// Ignore the TTL; only the close button removes a pinned notice.
    pub(crate) fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }

    pub(crate) fn action(
        mut self,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some(NoticeAction {
            label: label.into(),
            run: Rc::new(run),
        });
        self
    }
}

impl From<String> for Notice {
    fn from(message: String) -> Self {
        Self::info(message)
    }
}

impl From<SharedString> for Notice {
    fn from(message: SharedString) -> Self {
        Self::info(message)
    }
}

/// Registry so [`toast`] can reach the shell's board from a bare `&mut App`.
struct NoticeHost(WeakEntity<NoticeBoard>);

impl Global for NoticeHost {}

/// Push `notice` onto the shell's alert band. The shell has one window; a
/// missing or already-closed board drops the notice, matching how the old
/// toast layer tolerated `active_window` misses.
pub(crate) fn toast(notice: impl Into<Notice>, cx: &mut App) {
    let Some(board) = board_of(cx) else {
        return;
    };
    let _ = board.update(cx, |board, cx| board.push(notice.into(), cx));
}

fn board_of(cx: &App) -> Option<Entity<NoticeBoard>> {
    cx.try_global::<NoticeHost>()?.0.upgrade()
}

/// Owns the live notices and renders them as Alert banners.
pub(crate) struct NoticeBoard {
    entries: Vec<NoticeEntry>,
    next_seq: u64,
}

impl Default for NoticeBoard {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_seq: 0,
        }
    }
}

struct NoticeEntry {
    seq: u64,
    notice: Notice,
}

impl NoticeBoard {
    /// Publish `board` as the app-wide toast target. Called once from
    /// `Shell::new`; a dropped board makes later `toast` calls no-ops.
    pub(crate) fn register(board: &Entity<Self>, cx: &mut App) {
        cx.set_global(NoticeHost(board.downgrade()));
    }

    fn push(&mut self, notice: Notice, cx: &mut Context<Self>) {
        if let Some(key) = &notice.replace_key {
            self.entries
                .retain(|entry| entry.notice.replace_key.as_ref() != Some(key));
        }
        while self.entries.len() >= NOTICE_CAP {
            let Some(oldest_unpinned) = self.entries.iter().position(|entry| !entry.notice.pinned)
            else {
                break;
            };
            self.entries.remove(oldest_unpinned);
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let pinned = notice.pinned;
        self.entries.push(NoticeEntry { seq, notice });
        cx.notify();
        if pinned {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_TTL).await;
            let _ = this.update(cx, |board, cx| board.remove(seq, cx));
        })
        .detach();
    }

    fn remove(&mut self, seq: u64, cx: &mut Context<Self>) {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.seq != seq);
        if self.entries.len() != before {
            cx.notify();
        }
    }

    /// The band docked to the bottom of the main panel; empty renders
    /// nothing so the layout does not reserve a slot.
    pub(crate) fn render(&self) -> Div {
        if self.entries.is_empty() {
            return div();
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .pt_1()
            .pb_2()
            .children(self.entries.iter().map(render_entry))
    }
}

fn render_entry(entry: &NoticeEntry) -> impl IntoElement {
    let seq = entry.seq;
    let alert = match entry.notice.tone {
        NoticeTone::Info => Alert::info(
            SharedString::from(format!("notice-{seq}")),
            entry.notice.message.clone(),
        ),
        NoticeTone::Success => Alert::success(
            SharedString::from(format!("notice-{seq}")),
            entry.notice.message.clone(),
        ),
        NoticeTone::Warning => Alert::warning(
            SharedString::from(format!("notice-{seq}")),
            entry.notice.message.clone(),
        ),
        NoticeTone::Error => Alert::error(
            SharedString::from(format!("notice-{seq}")),
            entry.notice.message.clone(),
        ),
    }
    .small()
    .on_close({
        move |_, _, cx| {
            if let Some(board) = board_of(cx) {
                let _ = board.update(cx, |board, cx| board.remove(seq, cx));
            }
        }
    });
    let row = div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().flex_1().child(alert));
    match entry.notice.action.as_ref() {
        Some(action) => row.child(
            Button::new(SharedString::from(format!("notice-{seq}-action")))
                .label(action.label.clone())
                .small()
                .on_click({
                    let run = action.run.clone();
                    move |_, window, cx| run(window, cx)
                }),
        ),
        None => row,
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
