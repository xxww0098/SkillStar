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
use std::io::{self, Write};
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
    let mut file = OpenOptions::new().append(true).create(true).open(&path)?;
    file.write_all(line)
}

/// Move a live file that reached the cap out of the append path. A rename
/// failure is the caller's warning and nothing worse: the line then still
/// lands in the oversized live file.
fn rotate_if_full(dir: &Path, path: &Path) -> io::Result<()> {
    let len = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if len < ROTATE_AT_BYTES {
        return Ok(());
    }
    for number in 1..MAX_ARCHIVES {
        let archived = archive_path(dir, number);
        if !archived.exists() {
            fs::rename(path, archived)?;
            return Ok(());
        }
    }
    Ok(())
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
}
