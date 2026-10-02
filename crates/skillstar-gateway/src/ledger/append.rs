//! Appending to and reading back the usage ledger at
//! `data_root()/gateway/usage.jsonl`.
//!
//! One turn is one line, written with a single append-mode write while a
//! process-wide lock also holds rotation back, so turns from concurrent
//! connections cannot tear each other's line. A write failure — a read-only
//! directory included — is a warning and nothing else: the turn already sits
//! on the in-memory ring, and the agent's reply is already on its way (spec
//! firewall 1). The live file rolls over to `usage.<n>.jsonl` once it reaches
//! [`ROTATE_AT_BYTES`]; `load` walks the archives and the live file and keeps
//! complete lines only — a tail without its newline is a crash mid-write and
//! is dropped.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use skillstar_core::infra::paths::data_root;

use super::record::Record;

/// Directory under the data root that owns the ledger.
const LEDGER_DIR: &str = "gateway";
/// The live ledger file every turn appends to.
const LEDGER_FILE: &str = "usage.jsonl";
/// A live file at or past this size is rolled over before the next append.
const ROTATE_AT_BYTES: u64 = 5 * 1024 * 1024;
/// Archives kept beyond the live file. A rotation that would exceed this
/// drops the oldest archive first: the ledger is a metering face, not an
/// archive, and old lines may go (spec D10).
const KEEP_ARCHIVES: u32 = 6;
/// Ceiling for the archive-number search, so a full disk of archives cannot
/// spin the loop.
const MAX_ARCHIVES: u32 = 10_000;

static APPEND_LOCK: Mutex<()> = Mutex::new(());

/// Append one record as one line. Never fails the caller: any error along
/// the way is logged as a warning and the turn moves on.
pub fn append(record: &Record) {
    let _guard = APPEND_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut line = match serde_json::to_vec(record) {
        Ok(line) => line,
        Err(error) => {
            tracing::warn!(error = %error, "usage ledger line would not serialize");
            return;
        }
    };
    line.push(b'\n');
    if let Err(error) = write_line(&line) {
        tracing::warn!(
            error = %error,
            "usage ledger append failed; the turn stays on the in-memory ring"
        );
    }
}

/// Records at or after `since` (Unix milliseconds), oldest first, from the
/// rotated archives and then the live file. Files that are missing or
/// unreadable read as empty; lines that do not parse are skipped — the
/// ledger is a metering face, not an audit that stops on a bad byte.
pub fn load(since: i64) -> Vec<Record> {
    let dir = ledger_dir();
    let mut files = archive_paths(&dir);
    files.push(dir.join(LEDGER_FILE));
    let mut records = Vec::new();
    for path in files {
        if let Ok(text) = fs::read_to_string(&path) {
            records.extend(records_from_text(&text, since));
        }
    }
    records
}

fn ledger_dir() -> PathBuf {
    data_root().join(LEDGER_DIR)
}

fn archive_path(dir: &Path, number: u32) -> PathBuf {
    dir.join(format!("usage.{number}.jsonl"))
}

/// `usage.1.jsonl`, `usage.2.jsonl`, ... in number order, stopping at the
/// first gap. Rotation always fills the lowest free number, so the numbering
/// has no holes while it grows.
fn archive_paths(dir: &Path) -> Vec<PathBuf> {
    let mut archives = Vec::new();
    for number in 1..MAX_ARCHIVES {
        let path = archive_path(dir, number);
        if !path.exists() {
            break;
        }
        archives.push(path);
    }
    archives
}

fn write_line(line: &[u8]) -> io::Result<()> {
    let dir = ledger_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(LEDGER_FILE);
    rotate_if_full(&dir, &path)?;
    let mut file = OpenOptions::new()
        .append(true)
        .read(true)
        .create(true)
        .open(&path)?;
    // A crash mid-write leaves a tail without its newline, which reads
    // drop. Start this line on a fresh boundary so the append does not
    // glue itself onto that tail and lose both lines.
    let len = file.metadata()?.len();
    if len > 0 {
        let mut last = [0u8; 1];
        file.seek(SeekFrom::Start(len - 1))?;
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            file.write_all(b"\n")?;
        }
    }
    file.write_all(line)
}

/// Move a live file that reached the cap out of the append path. When the
/// retained set is full, the oldest archive dies and the rest shift down a
/// number, so the no-gap numbering [`archive_paths`] walks stays true and
/// the ledger stays bounded. A rename failure is the caller's warning and
/// nothing worse: the line then still lands in the oversized live file.
fn rotate_if_full(dir: &Path, path: &Path) -> io::Result<()> {
    let len = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if len < ROTATE_AT_BYTES {
        return Ok(());
    }
    let existing = archive_paths(dir).len() as u32;
    let target = if existing >= KEEP_ARCHIVES {
        let _ = fs::remove_file(archive_path(dir, 1));
        for number in 2..=existing {
            let from = archive_path(dir, number);
            if !from.exists() {
                break;
            }
            fs::rename(from, archive_path(dir, number - 1))?;
        }
        existing
    } else {
        existing + 1
    };
    fs::rename(path, archive_path(dir, target.min(MAX_ARCHIVES)))
}

/// The parseable records of one file's text, at or after `since`.
fn records_from_text(text: &str, since: i64) -> Vec<Record> {
    complete_lines(text)
        .filter_map(|line| serde_json::from_str::<Record>(line).ok())
        .filter(|record| record.at >= since)
        .collect()
}

/// The lines of `text` that end in a newline. Bytes after the last newline —
/// a write a crash interrupted — are a torn tail and are dropped.
fn complete_lines(text: &str) -> impl Iterator<Item = &str> {
    let complete_upto = text.rfind('\n').unwrap_or_default();
    text[..complete_upto]
        .split('\n')
        .filter(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: i64, agent: &str) -> Record {
        Record {
            at,
            agent: agent.to_string(),
            session: String::new(),
            model_asked: "m1".to_string(),
            model_answered: String::new(),
            catalog: String::new(),
            account: String::new(),
            tokens: super::super::record::TokenCounts::default(),
            status: 200,
            latency_ms: 1,
            error_kind: None,
            endpoint: "/v1/chat/completions".to_string(),
        }
    }

    #[test]
    fn complete_lines_drop_the_torn_tail() {
        assert_eq!(complete_lines("").collect::<Vec<_>>(), Vec::<&str>::new());
        assert_eq!(complete_lines("one\n").collect::<Vec<_>>(), vec!["one"]);
        assert_eq!(
            complete_lines("one\ntwo\n").collect::<Vec<_>>(),
            vec!["one", "two"]
        );
        // No newline at all: the whole text is one torn line.
        assert_eq!(
            complete_lines("one").collect::<Vec<_>>(),
            Vec::<&str>::new()
        );
        // A trailing fragment without its newline is dropped.
        assert_eq!(complete_lines("one\ntw").collect::<Vec<_>>(), vec!["one"]);
    }

    #[test]
    fn records_from_text_filters_since_and_bad_lines() {
        let first = sample(1_000, "codex");
        let second = sample(2_000, "omp");
        let mut text = serde_json::to_string(&first).unwrap();
        text.push('\n');
        text.push_str("this line is not a record\n");
        text.push_str(&serde_json::to_string(&second).unwrap());
        text.push('\n');
        let all = records_from_text(&text, 0);
        assert_eq!(all, vec![first.clone(), second.clone()]);
        let later = records_from_text(&text, 1_500);
        assert_eq!(later, vec![second]);
        let none = records_from_text(&text, 3_000);
        assert!(none.is_empty());
        // A torn tail on the last record loses that record, not the file.
        let torn = format!("{}\n{{\"at\":3", serde_json::to_string(&first).unwrap());
        assert_eq!(records_from_text(&torn, 0), vec![first]);
    }

    /// Env-sandboxed ledger directory; `data_root()` follows the var.
    fn with_env_data(probe: impl FnOnce(&Path)) {
        let _lock = crate::TEST_PATH_ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let root = std::env::temp_dir().join(format!(
            "skillstar-ledger-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        // SAFETY: tests touching this var hold the lock above.
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
        std::fs::create_dir_all(ledger_dir()).unwrap();
        probe(ledger_dir().as_path());
        // SAFETY: see above.
        unsafe {
            match previous {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        };
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A full live file with every archive slot taken: the oldest archive
    /// dies, the rest shift down, and the live file takes the top slot —
    /// the retained set never grows past KEEP_ARCHIVES.
    #[test]
    fn rotation_drops_the_oldest_archive_past_the_keep() {
        with_env_data(|dir| {
            for number in 1..=KEEP_ARCHIVES {
                std::fs::write(
                    archive_path(dir, number),
                    format!("archive-{number}"),
                )
                .unwrap();
            }
            let live = dir.join(LEDGER_FILE);
            std::fs::File::create(&live)
                .unwrap()
                .set_len(ROTATE_AT_BYTES)
                .unwrap();
            rotate_if_full(dir, &live).unwrap();
            assert!(!live.exists());
            for number in 1..KEEP_ARCHIVES {
                let text = std::fs::read_to_string(archive_path(dir, number)).unwrap();
                assert_eq!(text, format!("archive-{}", number + 1));
            }
            let top = std::fs::metadata(archive_path(dir, KEEP_ARCHIVES)).unwrap();
            assert_eq!(top.len(), ROTATE_AT_BYTES);
        });
    }

    /// Below the cap the rotation only claims the next free slot.
    #[test]
    fn rotation_below_the_keep_claims_the_next_slot() {
        with_env_data(|dir| {
            std::fs::write(archive_path(dir, 1), "archive-1").unwrap();
            let live = dir.join(LEDGER_FILE);
            std::fs::write(&live, "old live").unwrap();
            std::fs::OpenOptions::new()
                .write(true)
                .open(&live)
                .unwrap()
                .set_len(ROTATE_AT_BYTES)
                .unwrap();
            rotate_if_full(dir, &live).unwrap();
            assert_eq!(
                std::fs::read_to_string(archive_path(dir, 1)).unwrap(),
                "archive-1"
            );
            assert!(archive_path(dir, 2).exists());
            assert!(!archive_path(dir, 3).exists());
        });
    }

    /// A crash mid-write leaves a torn tail; the next append starts a fresh
    /// line instead of gluing itself onto the fragment, so both the fragment
    /// (dropped) and the new record (kept) behave as complete lines.
    #[test]
    fn append_repairs_a_torn_tail() {
        with_env_data(|dir| {
            std::fs::create_dir_all(dir).unwrap();
            let live = dir.join(LEDGER_FILE);
            std::fs::write(&live, "{\"at\":1").unwrap();
            append(&sample(2_000, "codex"));
            let records = load(0);
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].at, 2_000);
        });
    }
}
