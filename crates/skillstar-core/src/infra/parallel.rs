//! Bounded OS-thread fan-out for blocking work (git subprocesses, FS scans).

use std::collections::VecDeque;
use std::sync::Mutex;

/// Local CPU / `git rev-parse` style work: one worker per core, capped.
pub fn blocking_concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8))
        .unwrap_or(4)
}

/// Network-bound `git fetch`: overlap more child processes than cores.
pub fn git_fetch_concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() * 2).clamp(4, 16))
        .unwrap_or(8)
}

/// Run `work` over `items` with at most `limit` OS threads.
/// Result order matches input order.
pub fn map_bounded<T, R, F>(items: Vec<T>, limit: usize, work: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
{
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 || limit <= 1 {
        return items.into_iter().map(work).collect();
    }

    let queue = Mutex::new(items.into_iter().enumerate().collect::<VecDeque<_>>());
    let done = Mutex::new(Vec::with_capacity(n));
    let workers = limit.min(n);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some((index, item)) = pop(&queue) else {
                        return;
                    };
                    let result = work(item);
                    lock(&done).push((index, result));
                }
            });
        }
    });

    let mut results = done.into_inner().unwrap_or_else(|err| err.into_inner());
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

fn pop<T>(queue: &Mutex<VecDeque<T>>) -> Option<T> {
    lock(queue).pop_front()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn map_bounded_preserves_input_order() {
        let out = map_bounded(vec![3, 1, 2], 2, |n| n * 10);
        assert_eq!(out, vec![30, 10, 20]);
    }

    #[test]
    fn map_bounded_overlaps_workers() {
        let inflight = AtomicUsize::new(0);
        let max_inflight = AtomicUsize::new(0);
        map_bounded(vec![(); 4], 4, |_| {
            let n = inflight.fetch_add(1, Ordering::SeqCst) + 1;
            max_inflight.fetch_max(n, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            inflight.fetch_sub(1, Ordering::SeqCst);
        });
        assert!(
            max_inflight.load(Ordering::SeqCst) >= 2,
            "workers must overlap on a 4-item / 4-worker batch"
        );
    }
}
