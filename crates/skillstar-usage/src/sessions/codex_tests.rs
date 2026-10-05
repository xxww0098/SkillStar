//! Behavior tests for the codex parser (pinning the contract points of
//! slice 06: rollout dual format, token_count cumulative deltas, and the
//! input-cache semantics).
//!
//! The packed-rollout golden fixture (`codex_rollout.jsonl.zst`) is a real
//! zstd frame produced by the zstd CLI, pinning that ruzstd decodes what the
//! Codex app's "compress local chat history" actually writes.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::codex::CodexParser;
use super::{SessionCall, SessionFile, SessionParser, SessionTokens};
use crate::test_support::EnvGuard;

const ROLLOUT: &str = include_str!("fixtures/codex_rollout.jsonl");
const ROLLOUT_ZST: &[u8] = include_bytes!("fixtures/codex_rollout.jsonl.zst");
const THREAD: &str = "01a0bdc9-b5fd-7e63-9658-68bc8dce5ecd";

/// Sandbox home + data, returns (home_dir, data_dir, guard).
fn sandbox() -> (tempfile::TempDir, tempfile::TempDir, EnvGuard) {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let guard = EnvGuard::set(&[
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
        ("SKILLSTAR_DATA_DIR", data.path()),
    ]);
    (home, data, guard)
}

/// Write a rollout under the sandbox's sessions tree, returning its path.
fn rollout(home: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let dir = home
        .join(".codex")
        .join("sessions")
        .join("2026")
        .join("09")
        .join("20");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn plain_rollout(home: &Path) -> PathBuf {
    rollout(
        home,
        &format!("rollout-2026-09-20T15-48-28-{THREAD}.jsonl"),
        ROLLOUT.as_bytes(),
    )
}

fn session_file(path: &Path) -> SessionFile {
    let meta = std::fs::metadata(path).unwrap();
    SessionFile {
        agent: CodexParser::AGENT,
        path: path.to_path_buf(),
        size: meta.len(),
        modified_ms: 0,
    }
}

fn at(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}

// ---------------------------------------------------------------------------
// token_count deltas and the input-cache semantics
// ---------------------------------------------------------------------------

#[test]
fn token_count_deltas_and_cache_semantics() {
    let (home, _data, _guard) = sandbox();
    let path = plain_rollout(home.path());

    let (delta, checkpoint) = CodexParser.parse(&session_file(&path), None);
    // Lines 7 and 12 repeat the same totals and add nothing; info:null counts
    // nothing: three calls in total.
    assert_eq!(delta.len(), 3, "only grown totals count as calls");
    assert_eq!(checkpoint.calls_seen, 3);
    assert_eq!(checkpoint.offset, std::fs::metadata(&path).unwrap().len());
    assert_eq!(checkpoint.version, super::codex::CODEX_PARSER_VERSION);

    // First count in the file: no previous total, so last_token_usage counts
    // ({10000,300,4000,0}); codex's input INCLUDES the cached tokens, so the
    // reported input strips cache_read: 10000-4000 = 6000 (the opposite of
    // the pi family; see the codex.rs matrix).
    let first = &delta[0];
    assert_eq!(first.at, at("2026-09-20T15:48:36.000Z"));
    assert_eq!(first.session, THREAD, "session_meta's id names the session");
    assert_eq!(first.model_answered, "gpt-6-astra");
    assert_eq!(first.model_asked, "gpt-6-astra");
    assert_eq!(first.effort.as_deref(), Some("low"));
    assert_eq!(
        first.tokens,
        SessionTokens {
            input: 6000,
            output: 300,
            cache_read: 4000,
            cache_write: 0
        }
    );
    // Latency measured from the user_message line (15:48:29.2 → 15:48:36).
    assert_eq!(first.latency_ms, Some(6800));
    // The first call's interval starts at the file head.
    assert_eq!(first.from, 0);

    // Grown total: the difference against the previous one
    // ({22000,500,14000,10} - {10000,300,4000,0}), input again without the
    // cache: 12000-10000 = 2000.
    let second = &delta[1];
    assert_eq!(second.at, at("2026-09-20T15:50:10.000Z"));
    assert_eq!(second.model_answered, "gpt-6-luna");
    // The second turn_context carries no effort: the turn's ask for none
    // clears the one before it.
    assert_eq!(second.effort, None);
    assert_eq!(
        second.tokens,
        SessionTokens {
            input: 2000,
            output: 200,
            cache_read: 10000,
            cache_write: 0
        }
    );

    // A total that started over (input fell below the previous one): the
    // last turn's own usage counts, not the difference.
    let third = &delta[2];
    assert_eq!(third.at, at("2026-09-20T15:51:00.000Z"));
    assert_eq!(
        third.tokens,
        SessionTokens {
            input: 400,
            output: 50,
            cache_read: 100,
            cache_write: 0
        }
    );
}

#[test]
fn session_id_prefers_meta_session_id_over_thread_id() {
    let (home, _data, _guard) = sandbox();
    let lines = r#"{"timestamp":"2026-09-20T15:48:28.100Z","type":"session_meta","payload":{"id":"ignored","session_id":"sess-cx-1","cwd":"/work"}}
{"timestamp":"2026-09-20T15:48:30.000Z","type":"turn_context","payload":{"model":"gpt-6-astra"}}
{"timestamp":"2026-09-20T15:48:31.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":2,"cache_write_input_tokens":0},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":2,"cache_write_input_tokens":0}}}}
"#;
    let path = rollout(
        home.path(),
        &format!("rollout-2026-09-20T15-48-28-{THREAD}.jsonl"),
        lines.as_bytes(),
    );

    let (delta, _) = CodexParser.parse(&session_file(&path), None);
    assert_eq!(delta.len(), 1);
    assert_eq!(
        delta[0].session, "sess-cx-1",
        "session_id wins over payload id"
    );
}

// ---------------------------------------------------------------------------
// The packed (.jsonl.zst) form
// ---------------------------------------------------------------------------

#[test]
fn packed_rollout_reads_the_same_calls() {
    let (home, _data, _guard) = sandbox();
    // Only the packed form on disk (the Codex app removed the plain one):
    // ruzstd must decode the real zstd frame and the calls must equal the
    // plain rollout's.
    let plain = plain_rollout(home.path());
    let (plain_delta, plain_checkpoint) = CodexParser.parse(&session_file(&plain), None);
    let zst_path = rollout(
        home.path(),
        &format!("rollout-2026-09-20T15-48-28-{THREAD}.jsonl.zst"),
        ROLLOUT_ZST,
    );
    std::fs::remove_file(&plain).unwrap();

    let (packed_delta, packed_checkpoint) = CodexParser.parse(&session_file(&zst_path), None);
    assert_eq!(packed_delta.len(), 3, "ruzstd decodes a real zstd frame");
    // from/to point into the decompressed text, so only the file path and
    // the intervals' file differ; strip those and the calls are the same.
    let strip = |calls: &[SessionCall]| {
        calls
            .iter()
            .map(|c| {
                (
                    c.at,
                    c.session.clone(),
                    c.model_answered.clone(),
                    c.tokens,
                    c.effort.clone(),
                    c.latency_ms,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(strip(&packed_delta), strip(&plain_delta));
    assert_eq!(packed_checkpoint.calls_seen, plain_checkpoint.calls_seen);
    // A packed rollout is read whole every run: even an unchanged file
    // reparses (no resume), still yielding the same full view.
    let (again_delta, again) =
        CodexParser.parse(&session_file(&zst_path), Some(packed_checkpoint.clone()));
    assert_eq!(again_delta.len(), 3);
    assert_eq!(again.calls_seen, 3);
}

#[test]
fn both_forms_side_by_side_count_once() {
    let (home, _data, _guard) = sandbox();
    let plain = plain_rollout(home.path());
    rollout(
        home.path(),
        &format!("rollout-2026-09-20T15-48-28-{THREAD}.jsonl.zst"),
        ROLLOUT_ZST,
    );
    // While the Codex app packs it, both forms sit side by side: only the
    // plain one is read (the compressed one may be half written).
    let files = CodexParser.discover(home.path());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, plain);
}

#[test]
fn corrupt_packed_rollout_degrades_to_empty() {
    let (home, _data, _guard) = sandbox();
    let path = rollout(
        home.path(),
        "rollout-2026-09-20T15-48-28-deadbeef.jsonl.zst",
        b"not a zstd frame",
    );
    let (delta, checkpoint) = CodexParser.parse(&session_file(&path), None);
    assert!(delta.is_empty());
    assert_eq!(checkpoint.calls_seen, 0);
}

// ---------------------------------------------------------------------------
// Incremental semantics
// ---------------------------------------------------------------------------

#[test]
fn append_produces_only_delta_and_calls_seen_converges() {
    let (home, _data, _guard) = sandbox();
    let path = plain_rollout(home.path());
    let (first_delta, first) = CodexParser.parse(&session_file(&path), None);
    assert_eq!(first_delta.len(), 3);

    // Append one more token_count: only its difference counts.
    let appended = r#"{"timestamp":"2026-09-20T15:52:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":600,"cached_input_tokens":100,"output_tokens":60,"cache_write_input_tokens":0},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"cache_write_input_tokens":0}}}}
"#;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(appended.as_bytes())
        .unwrap();

    let (second_delta, second) = CodexParser.parse(&session_file(&path), Some(first.clone()));
    assert_eq!(second_delta.len(), 1, "append produces only the delta call");
    // 600-500 total difference with the previous total {500,100,50,0}:
    // input 100, cached 0, output 10.
    assert_eq!(
        second_delta[0].tokens,
        SessionTokens {
            input: 100,
            output: 10,
            cache_read: 0,
            cache_write: 0
        }
    );
    assert_eq!(second.calls_seen, 4);
    assert_eq!(second.offset, std::fs::metadata(&path).unwrap().len());

    // Full reread from zero converges; parsing unchanged hands it back as-is.
    let (full_delta, full) = CodexParser.parse(&session_file(&path), None);
    assert_eq!(full.calls_seen, second.calls_seen);
    assert_eq!(full_delta.len(), 4);
    let (noop, returned) = CodexParser.parse(&session_file(&path), Some(full.clone()));
    assert!(noop.is_empty());
    assert_eq!(returned, full);
}

#[test]
fn truncated_file_rereads_from_zero() {
    let (home, _data, _guard) = sandbox();
    let path = plain_rollout(home.path());
    let (_, prior) = CodexParser.parse(&session_file(&path), None);
    assert_eq!(prior.calls_seen, 3);

    // Cut back to the first six lines: only call 1 remains.
    let cut: String = ROLLOUT.lines().take(6).map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, &cut).unwrap();
    let (delta, after) = CodexParser.parse(&session_file(&path), Some(prior));
    assert_eq!(delta.len(), 1);
    assert_eq!(after.calls_seen, 1);
}

#[test]
fn partial_trailing_line_is_left_for_next_read() {
    let (home, _data, _guard) = sandbox();
    // Everything but the last 12 bytes: line 13 is half written, lines 1-12
    // (two calls: the third count lives in line 13) are consumed.
    let (whole, _tail) = ROLLOUT.split_at(ROLLOUT.len() - 12);
    let path = rollout(
        home.path(),
        &format!("rollout-2026-09-20T15-48-28-{THREAD}.jsonl"),
        whole.as_bytes(),
    );
    let (delta, checkpoint) = CodexParser.parse(&session_file(&path), None);
    assert_eq!(delta.len(), 2);
    assert_eq!(checkpoint.calls_seen, 2);
    let twelve_lines: u64 = ROLLOUT.lines().take(12).map(|l| l.len() as u64 + 1).sum();
    assert_eq!(
        checkpoint.offset, twelve_lines,
        "the half-written line is left for the next read"
    );

    // Completing the line yields the third call on the next read.
    std::fs::write(&path, ROLLOUT).unwrap();
    let (delta, _) = CodexParser.parse(&session_file(&path), Some(checkpoint));
    assert_eq!(delta.len(), 1);
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

#[test]
fn discovery_walks_and_filters_rollout_names() {
    let (home, _data, _guard) = sandbox();
    plain_rollout(home.path());
    // A resumed session's second segment (same thread id, _segment suffix).
    rollout(
        home.path(),
        &format!("rollout-2026-09-21T10-00-00-{THREAD}_01a0c9f1-3d6f-7f33-b3bd-25c1dd14251b.jsonl"),
        ROLLOUT.as_bytes(),
    );
    // Not rollouts: wrong prefix, no thread id, a second underscore.
    rollout(home.path(), "other.jsonl", ROLLOUT.as_bytes());
    rollout(home.path(), "rollout-x.jsonl", ROLLOUT.as_bytes());
    rollout(
        home.path(),
        "rollout-2026-09-20T15-48-28-a_b_c.jsonl",
        ROLLOUT.as_bytes(),
    );

    let files = CodexParser.discover(home.path());
    assert_eq!(files.len(), 2, "both segments of the thread, nothing else");
    assert!(files.iter().all(|f| f.agent == "codex"));
}

#[test]
fn sandbox_wins_over_codex_home() {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_sessions = outside.path().join(".codex").join("sessions");
    std::fs::create_dir_all(&outside_sessions).unwrap();
    std::fs::write(
        outside_sessions.join("rollout-2026-09-20T15-48-28-outside.jsonl"),
        ROLLOUT,
    )
    .unwrap();
    plain_rollout(home.path());

    let _guard = EnvGuard::set(&[
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
        ("SKILLSTAR_DATA_DIR", data.path()),
        ("CODEX_HOME", outside.path()),
    ]);
    let files = CodexParser.discover(home.path());
    assert_eq!(
        files.len(),
        1,
        "SKILLSTAR_TOOL_SYNC_HOME wins over $CODEX_HOME"
    );
    assert!(
        files[0]
            .path
            .ends_with(format!("rollout-2026-09-20T15-48-28-{THREAD}.jsonl"))
    );
}

#[test]
fn codex_home_honored_when_not_sandboxed() {
    let codex = tempfile::tempdir().unwrap();
    let sessions = codex.path().join("sessions").join("2026").join("09");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join("rollout-2026-09-20T15-48-28-env.jsonl"),
        ROLLOUT,
    )
    .unwrap();

    let home = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::set(&[("CODEX_HOME", codex.path())]);
    let files = CodexParser.discover(home.path());
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].path,
        sessions.join("rollout-2026-09-20T15-48-28-env.jsonl")
    );
}
