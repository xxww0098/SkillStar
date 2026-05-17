//! Generic poll-based OAuth flow (Cursor / Qoder style).
//!
//! Caller provides:
//! - the URL to poll
//! - per-attempt request builder
//! - a parser that turns the HTTP response into `Poll<T>` (Ready/Pending/Err)
//!
//! Returns the parsed token payload or a timeout error.

use std::time::Duration;

use tokio::time::{Instant, sleep};

use crate::{UsageError, UsageResult};

/// Per-attempt parser outcome.
pub enum Poll<T> {
    /// Token retrieved.
    Ready(T),
    /// Still waiting; keep polling.
    Pending,
    /// Stop with an error.
    Failed(String),
}

pub struct PollConfig {
    pub interval: Duration,
    pub max_attempts: usize,
}

impl PollConfig {
    pub const fn new(interval_ms: u64, max_attempts: usize) -> Self {
        Self {
            interval: Duration::from_millis(interval_ms),
            max_attempts,
        }
    }
}

/// Loop `attempt` until `Ready` / `Failed` / max attempts.
pub async fn run<T, F, Fut>(config: PollConfig, mut attempt: F) -> UsageResult<T>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = UsageResult<Poll<T>>>,
{
    let start = Instant::now();
    for n in 0..config.max_attempts {
        match attempt(n).await? {
            Poll::Ready(value) => {
                tracing::debug!(
                    "[poll_flow] ready after {} attempts ({:?})",
                    n + 1,
                    start.elapsed()
                );
                return Ok(value);
            }
            Poll::Pending => {
                sleep(config.interval).await;
            }
            Poll::Failed(msg) => return Err(UsageError::Other(msg)),
        }
    }
    Err(UsageError::Other(format!(
        "轮询超时（{} 次尝试，{:?}）",
        config.max_attempts,
        start.elapsed()
    )))
}
