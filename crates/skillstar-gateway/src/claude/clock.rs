//! Monotonic clock for Claude run deadlines.
//!
//! A manual clock ignores wall time, so a test can expire a 20 minute idle
//! without waiting. A system clock tracks `Instant` and wakes early when a
//! run's deadline moves.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub(super) struct SharedClock {
    manual: bool,
    origin: Instant,
    extra_ms: Mutex<u128>,
    cv: Condvar,
}

impl SharedClock {
    pub(super) fn system() -> Self {
        Self {
            manual: false,
            origin: Instant::now(),
            extra_ms: Mutex::new(0),
            cv: Condvar::new(),
        }
    }

    pub(super) fn manual() -> Self {
        Self {
            manual: true,
            origin: Instant::now(),
            extra_ms: Mutex::new(0),
            cv: Condvar::new(),
        }
    }

    pub(super) fn now_ms(&self) -> u128 {
        let extra = *lock(&self.extra_ms);
        self.now_locked(extra)
    }

    /// Move a manual clock forward and wake waiters. A system clock ignores it.
    pub(super) fn advance(&self, by: Duration) -> bool {
        if !self.manual {
            return false;
        }
        let mut extra = lock(&self.extra_ms);
        *extra = extra.saturating_add(by.as_millis());
        self.cv.notify_all();
        true
    }

    pub(super) fn poke(&self) {
        let _extra = lock(&self.extra_ms);
        self.cv.notify_all();
    }

    /// Block until `deadline_ms`, or until `still` is false.
    pub(super) fn wait_until(&self, deadline_ms: u128, still: impl Fn() -> bool) {
        let mut extra = lock(&self.extra_ms);
        loop {
            if !still() {
                return;
            }
            let now = self.now_locked(*extra);
            if now >= deadline_ms {
                return;
            }
            if self.manual {
                extra = self
                    .cv
                    .wait(extra)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            } else {
                let left = deadline_ms.saturating_sub(now);
                let left_ms = u64::try_from(left).unwrap_or(u64::MAX);
                let (next, _) = self
                    .cv
                    .wait_timeout(extra, Duration::from_millis(left_ms))
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                extra = next;
            }
        }
    }

    fn now_locked(&self, extra: u128) -> u128 {
        if self.manual {
            extra
        } else {
            self.origin.elapsed().as_millis().saturating_add(extra)
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
