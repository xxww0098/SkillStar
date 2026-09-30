//! The recent-call ring stays in memory and keeps sixty.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Mutex;

use skillstar_gateway::{
    TRACE_KEEP, RecentCall, clear_recent_calls, note_recent_call, recent_calls,
};

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct DataDir {
    previous: Option<OsString>,
    dir: PathBuf,
}

impl DataDir {
    fn new() -> Self {
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        let dir = std::env::temp_dir().join(format!(
            "skillstar-trace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &dir) };
        Self { previous, dir }
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn trace_keeps_60() {
    let _guard = lock();
    assert_eq!(TRACE_KEEP, 60);
    clear_recent_calls();
    for n in 0..61 {
        note_recent_call(RecentCall {
            at: "2026-09-30 00:00:00".to_string(),
            agent: "codex".to_string(),
            model: format!("m{n}"),
            status: 200,
            completion_tokens: Some(n),
        });
    }
    let calls = recent_calls();
    assert_eq!(calls.len(), 60);
    assert_eq!(calls[0].model, "m1");
    assert_eq!(calls.last().map(|call| call.model.as_str()), Some("m60"));
    let text = format!("{calls:?}");
    assert!(!text.contains("https://"), "{text}");
    assert!(!text.contains("sk-"), "{text}");
    clear_recent_calls();
}

#[test]
fn trace_is_memory_only() {
    let _guard = lock();
    clear_recent_calls();
    let data = DataDir::new();
    note_recent_call(RecentCall {
        at: "2026-09-30 00:00:01".to_string(),
        agent: "codex".to_string(),
        model: "m1".to_string(),
        status: 200,
        completion_tokens: None,
    });
    let calls = recent_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].completion_tokens, None);
    assert!(
        std::fs::read_dir(&data.dir).unwrap().next().is_none(),
        "a recorded call must not create a file"
    );
    clear_recent_calls();
    assert!(recent_calls().is_empty());
    assert!(std::fs::read_dir(&data.dir).unwrap().next().is_none());
}
