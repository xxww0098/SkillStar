//! Wave-4 async-bridge foundation: the cancel handle and the progress
//! stream every deep-async slice (import modal, GitHub sign-in) rides.
//! This is bridge plumbing, not a domain seam — the two slices own
//! `my_skills_import.rs` / `my_skills_github.rs` and only *use* what
//! lives here.
//!
//! The three primitives, each mirroring its GPUI original:
//!
//! * **Cancel handle** — `DomainJob`, a `#[class]` the TSX side holds. Its
//!   shared [`JobFlag`] is what blocking workers read between steps; `cancel()`
//!   is idempotent, and `Drop` cancels too (JS `dispose()` or GC of the object
//!   drops the Rust value), the counterpart of `ImportDialog`'s
//!   `Drop::cancel_active` (import_modal.rs:678-682 — every close path that
//!   skips `on_close` still aborts the git session).
//! * **Progress stream** — a `std::sync::mpsc` channel whose sender side is a
//!   `Send + Sync` sink on the blocking pool (the `DialogProgressSink` shape,
//!   progress.rs:38-46) and whose receiver is drained by
//!   [`pump_progress`] on the engine thread: every [`PROGRESS_TICK_MS`] keep
//!   only the newest event (`rx.try_recv` loop, progress.rs:63-73) and
//!   `Callback::post` it for the next JS turn. The tick *is* the 120ms
//!   throttle — faster domain events collapse to the last one per window,
//!   so the throttle lives in this crate (see the slot contracts for why
//!   not in TSX).
//! * **Stale gating** — a cancelled job's pump stops consuming and stops
//!   posting (`JobFlag::cancelled` check each tick, the `still`/phase gate
//!   of progress.rs:81-93); events already queued when cancel lands are
//!   dropped by the TS-side epoch discipline (`services/domainJob.ts`).
//!
//! `Callback::post` is safe here because `#[module]` async fns run as tasks
//! of the core on the engine thread (single-threaded executor; nsh-core
//! `element/task.rs` polls `LocalFuture`s there), which is exactly where
//! `post` queues for the next JS turn (`element/callback.rs`: "from
//! anywhere on the engine thread"). Workers on the blocking pool never
//! touch the callback — they only send on the channel.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use nsh_core::JsonValue;
use nsh_core::prelude::*;

/// The import dialog's watcher tick (progress.rs:61) — the one throttle
/// window every wave-4 progress stream uses.
pub(crate) const PROGRESS_TICK_MS: u64 = 120;

// ---- The cancel handle --------------------------------------------------------

/// Per-process identity of a job (TS-side epoch key; strictly increasing).
fn next_token() -> u32 {
    static TOKENS: AtomicU32 = AtomicU32::new(0);
    TOKENS.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

/// The shared cancel state of one job. Cloned into blocking workers and
/// facade task loops; the TSX side holds the one [`DomainJob`] whose Drop
/// flips it.
#[doc(hidden)]
pub(crate) struct JobFlag {
    token: u32,
    cancelled: Arc<AtomicBool>,
}

impl Clone for JobFlag {
    fn clone(&self) -> Self {
        Self {
            token: self.token,
            cancelled: self.cancelled.clone(),
        }
    }
}

impl JobFlag {
    /// Whether the job was cancelled (`DomainJob::cancel`, or its Drop).
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// The job's identity (the TS-side epoch key, `DomainJob.token`).
    pub(crate) fn token(&self) -> u32 {
        self.token
    }
}

/// One running domain job's cancel handle (`DomainJob` in TSX): `new` it,
/// pass it to a facade task, `cancel()` to stop, `dispose()` (or let it go)
/// to stop as well. The label is for diagnostics only.
///
/// - `job.cancel()` — idempotent; flips the flag every blocking worker and
///   task loop of this job reads.
/// - `job.cancelled` — whether it was cancelled.
/// - `job.token` — the job's identity for this process; a TS binding keeps
///   it as its epoch so a late-queued event after cancel cannot land.
///
/// Dropping the value (JS `dispose()`, or the object's collection) cancels
/// the job — the counterpart of `ImportDialog`'s own `Drop` calling
/// `cancel_active` (import_modal.rs:678-682).
#[class]
pub struct DomainJob {
    flag: JobFlag,
}

#[class]
impl DomainJob {
    /// A fresh job (not yet cancelled).
    pub fn new(label: String) -> Self {
        let _ = label; // diagnostics only; nothing recorded on this side.
        Self {
            flag: JobFlag {
                token: next_token(),
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        }
    }

    /// Cancel the job. Idempotent; every worker of this job stops reading
    /// further work at its next flag check.
    pub fn cancel(&self) {
        self.flag.cancelled.store(true, Ordering::SeqCst);
    }

    /// Whether the job was cancelled.
    #[getter]
    pub fn cancelled(&self) -> bool {
        self.flag.cancelled()
    }

    /// The job's identity for this process (TS epoch key).
    #[getter]
    pub fn token(&self) -> u32 {
        self.flag.token()
    }

    /// The shared flag facade tasks clone into their workers. Rust-only
    /// helper (no visibility: not a class member) — other modules go
    /// through [`job_flag`].
    fn flag(&self) -> JobFlag {
        self.flag.clone()
    }
}

/// The [`JobFlag`] of a TS-held job, for facade task loops in the slice
/// modules (they cannot call the class's private helper). Panics only on a
/// disposed instance, which is a caller bug (the job was cancelled anyway).
pub(crate) fn job_flag(job: &Instance<DomainJob>) -> JobFlag {
    job.with(|job| job.flag())
}

impl Drop for DomainJob {
    fn drop(&mut self) {
        // drop 即取消：dispose()/GC 与显式 cancel 同效。
        self.flag.cancelled.store(true, Ordering::SeqCst);
    }
}

// ---- The progress stream ------------------------------------------------------

/// What one drain of a progress channel holds (progress.rs:63-73's loop as
/// a value): the newest event, and whether the sender side is gone.
pub(crate) struct Drained<E> {
    pub latest: Option<E>,
    pub disconnected: bool,
}

/// Keeps only the newest of everything the channel currently holds. The
/// throttle's other half: called once per tick, faster domain events
/// collapse to their last one before anything posts.
pub(crate) fn drain_latest<E>(rx: &Receiver<E>) -> Drained<E> {
    let mut latest = None;
    let mut disconnected = false;
    loop {
        match rx.try_recv() {
            Ok(event) => latest = Some(event),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                disconnected = true;
                break;
            }
        }
    }
    Drained { latest, disconnected }
}

/// The progress pump every wave-4 facade task runs on the engine thread:
/// every `tick`, keep the newest event the channel holds and `post` it to
/// TSX for the next JS turn; return when the channel disconnects (the
/// worker finished and dropped its sender) or the job was cancelled
/// (progress.rs:57-96's watcher, with `JobFlag` as the phase gate).
///
/// A cancelled job consumes nothing further and posts nothing more — the
/// events already queued in JS are the TS epoch's to drop.
#[allow(dead_code)] // Kept for future streaming tasks after BridgeDemo removal.
pub(crate) async fn pump_progress(
    rx: Receiver<JsonValue>,
    on_event: Callback<JsonValue>,
    flag: JobFlag,
    tick: Duration,
) {
    loop {
        timer(tick).await;
        if flag.cancelled() {
            return;
        }
        let drained = drain_latest(&rx);
        if let Some(event) = drained.latest {
            on_event.post(event);
        }
        if drained.disconnected {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `drain_latest` keeps only the newest event and reports a dropped
    /// sender as disconnected — the 120ms window's collapse, as a value.
    #[test]
    fn drain_keeps_the_newest_and_sees_disconnect() {
        let (tx, rx) = std::sync::mpsc::channel::<u32>();
        for event in [1, 2, 3] {
            tx.send(event).unwrap();
        }
        let drained = drain_latest(&rx);
        assert_eq!(drained.latest, Some(3), "three queued events collapse to the last");
        assert!(!drained.disconnected);

        drop(tx);
        let drained = drain_latest(&rx);
        assert_eq!(drained.latest, None, "nothing was queued after the first drain");
        assert!(drained.disconnected, "a dropped sender ends the pump");
    }

    /// `cancel()` and Drop both flip the flag; the flag clones share it.
    #[test]
    fn cancel_and_drop_flip_the_shared_flag() {
        let job = DomainJob::new("demo".to_owned());
        let flag = job.flag();
        assert!(!flag.cancelled());
        assert!(flag.token() > 0, "tokens are non-zero identities");

        job.cancel();
        assert!(flag.cancelled(), "the cloned flag sees the cancel");
        job.cancel();
        assert!(flag.cancelled(), "cancel is idempotent");
    }

    /// Drop cancels — the dispose()/GC path (import_modal.rs's `Drop` arm).
    #[test]
    fn dropping_the_job_cancels_it() {
        let (flag, tx) = {
            let job = DomainJob::new("demo".to_owned());
            let flag = job.flag();
            let (tx, _rx) = std::sync::mpsc::channel::<u32>();
            (flag, tx)
        };
        let _ = tx;
        assert!(flag.cancelled(), "the job's Drop flipped the shared flag");
    }

    /// Distinct jobs carry distinct tokens (the TS epoch key must not
    /// collide within a process).
    #[test]
    fn jobs_have_distinct_tokens() {
        let first = DomainJob::new("a".to_owned());
        let second = DomainJob::new("b".to_owned());
        assert_ne!(first.flag().token(), second.flag().token());
    }

    /// A min-gap gate for the slices that poll rather than push (the GitHub
    /// device flow's own retry cadence is the domain's; this only proofs
    /// the tick constant used by the pump).
    #[test]
    fn progress_tick_is_the_gpui_watcher_window() {
        assert_eq!(PROGRESS_TICK_MS, 120, "progress.rs:61 watcher window");
    }
}
