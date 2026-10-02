//! The persistent usage ledger: line-per-turn round trip, torn tails, size
//! rotation, and the never-break-the-turn rule under an unwritable data
//! directory. The sandbox pattern copies tests/access.rs; the real `$HOME` is
//! never written.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::thread;

use skillstar_gateway::{Record, TokenCounts, append, load};

fn lock_ledger_env() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn scratch(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

struct EnvRestore {
    saved: Vec<(&'static str, Option<OsString>)>,
    root: PathBuf,
}

impl EnvRestore {
    fn sandbox(root: &Path) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let pairs = [
            ("HOME", home.to_string_lossy().into_owned()),
            ("USERPROFILE", home.to_string_lossy().into_owned()),
            (
                "SKILLSTAR_TOOL_SYNC_HOME",
                home.to_string_lossy().into_owned(),
            ),
            ("SKILLSTAR_DATA_DIR", data.to_string_lossy().into_owned()),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
                (key, previous)
            })
            .collect();
        Self {
            saved,
            root: root.to_path_buf(),
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn ledger_file(env: &EnvRestore) -> PathBuf {
    env.data_dir().join("gateway").join("usage.jsonl")
}

fn record(at: i64, agent: &str) -> Record {
    Record {
        at,
        agent: agent.to_string(),
        session: "s1".to_string(),
        model_asked: "m1".to_string(),
        model_answered: "m2".to_string(),
        catalog: String::new(),
        account: "key:0123abcd".to_string(),
        tokens: TokenCounts {
            input: 10,
            output: 5,
            cache_read: 4,
            cache_write: 2,
            reasoning: 1,
        },
        status: 200,
        latency_ms: 42,
        error_kind: None,
        endpoint: "/v1/chat/completions".to_string(),
    }
}

#[test]
fn append_then_load_round_trips_one_line_per_record() {
    let _lock = lock_ledger_env();
    let root = scratch("ledger-roundtrip");
    let env = EnvRestore::sandbox(&root);

    let first = record(1_000, "codex");
    let second = record(2_000, "omp");
    append(&first);
    append(&second);

    let text = fs::read_to_string(ledger_file(&env)).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "one line per record: {text}");
    assert!(text.ends_with('\n'), "every line is terminated: {text:?}");
    // Snake-case keys, the fixed set, once and for all.
    let first_line = lines[0];
    let parsed: serde_json::Value = serde_json::from_str(first_line).unwrap();
    let mut keys: Vec<&str> = parsed
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "account",
            "agent",
            "at",
            "catalog",
            "endpoint",
            "error_kind",
            "latency_ms",
            "model_answered",
            "model_asked",
            "session",
            "status",
            "tokens",
        ],
        "the record schema is fixed: {first_line}"
    );
    let mut tokens: Vec<&str> = parsed
        .get("tokens")
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    tokens.sort_unstable();
    assert_eq!(
        tokens,
        vec!["cache_read", "cache_write", "input", "output", "reasoning"]
    );

    assert_eq!(load(0), vec![first, second.clone()], "oldest first");
    assert_eq!(load(1_500), vec![second], "since is a millisecond cutoff");
    assert!(load(3_000).is_empty());
}

#[test]
fn load_reads_nothing_until_a_turn_lands() {
    let _lock = lock_ledger_env();
    let root = scratch("ledger-empty");
    let env = EnvRestore::sandbox(&root);
    assert!(load(0).is_empty());
    assert!(
        !ledger_file(&env).exists(),
        "reading alone does not create the file"
    );
}

#[test]
fn load_drops_a_torn_tail() {
    let _lock = lock_ledger_env();
    let root = scratch("ledger-torn");
    let env = EnvRestore::sandbox(&root);

    let whole = record(1_000, "codex");
    append(&whole);
    // A crash mid-write leaves bytes without their newline.
    let mut torn = std::fs::OpenOptions::new()
        .append(true)
        .open(ledger_file(&env))
        .unwrap();
    std::io::Write::write_all(&mut torn, b"{\"at\":2").unwrap();

    let loaded = load(0);
    assert_eq!(
        loaded,
        vec![whole],
        "the torn tail is skipped, the whole line kept"
    );
}

#[test]
fn rotation_moves_the_live_file_and_load_reads_everything() {
    let _lock = lock_ledger_env();
    let root = scratch("ledger-rotate");
    let env = EnvRestore::sandbox(&root);

    // A live file past the cap (5 MB) with no complete lines at all.
    let dir = env.data_dir().join("gateway");
    fs::create_dir_all(&dir).unwrap();
    fs::write(ledger_file(&env), vec![b'x'; 5 * 1024 * 1024]).unwrap();

    let fresh = record(3_000, "codex");
    append(&fresh);

    let archive = dir.join("usage.1.jsonl");
    assert!(archive.exists(), "the oversized live file was rolled away");
    assert_eq!(
        fs::metadata(&archive).unwrap().len(),
        5 * 1024 * 1024,
        "the archive keeps the bytes it was given"
    );
    let live = fs::read_to_string(ledger_file(&env)).unwrap();
    assert_eq!(live.lines().count(), 1, "the live file starts over: {live}");
    assert_eq!(load(0), vec![fresh], "the cap filler reads as no records");
}

#[test]
fn append_into_an_unwritable_directory_is_only_a_warning() {
    let _lock = lock_ledger_env();
    let root = scratch("ledger-readonly");
    let env = EnvRestore::sandbox(&root);
    if !cfg!(unix) {
        return;
    }
    // The gateway directory exists but cannot be written to.
    let dir = env.data_dir().join("gateway");
    fs::create_dir_all(&dir).unwrap();
    make_readonly(&dir);

    // The turn is what matters: append returns, panics nothing, and the
    // in-memory ring (exercised through serve in tests/serve.rs) still holds
    // the call.
    append(&record(1_000, "codex"));
    assert!(!ledger_file(&env).exists(), "nothing could be written");
    assert!(load(0).is_empty());

    make_writable(&dir);
}

#[cfg(unix)]
fn make_readonly(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
}

#[cfg(unix)]
fn make_writable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn concurrent_appends_never_tear_a_line() {
    let _lock = lock_ledger_env();
    let root = scratch("ledger-threads");
    let env = EnvRestore::sandbox(&root);

    const THREADS: usize = 4;
    const PER_THREAD: usize = 250;
    thread::scope(|scope| {
        for thread in 0..THREADS {
            scope.spawn(move || {
                for n in 0..PER_THREAD {
                    append(&record(1_000 + n as i64, &format!("agent-{thread}")));
                }
            });
        }
    });

    let text = fs::read_to_string(ledger_file(&env)).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), THREADS * PER_THREAD, "no line went missing");
    assert!(text.ends_with('\n'), "the file ends on a line boundary");
    for line in &lines {
        let parsed = serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|error| panic!("a torn or corrupt line ({error}): {line}"));
        assert!(parsed.get("at").is_some(), "{line}");
    }
    assert_eq!(load(0).len(), THREADS * PER_THREAD);
}
