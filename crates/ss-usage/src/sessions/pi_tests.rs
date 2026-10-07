//! Behavior tests for the pi parser (fork-copy skipping, the usage-bearing
//! entry kinds, jsonc tolerance and the input-cache matrix: pi's input
//! already excludes the cache read — the opposite of codex).

use std::io::Write;
use std::path::{Path, PathBuf};

use super::pi::PiParser;
use super::{SessionCall, SessionFile, SessionParser, SessionTokens, read_calls};
use crate::test_support::EnvGuard;

const SESSION: &str = include_str!("fixtures/pi_session.jsonl");
const FORK: &str = include_str!("fixtures/pi_fork.jsonl");
const ID: &str = "0199aaaa-1111-7222-8333-444455556666";
const FORK_ID: &str = "0199bbbb-1111-7222-8333-444455556666";

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

/// A pi session file under `<home>/.pi/agent/sessions/<folder>/`.
fn pi_session(home: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let dir = home
        .join(".pi")
        .join("agent")
        .join("sessions")
        .join("--work-pi--");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn session_file(path: &Path) -> SessionFile {
    let meta = std::fs::metadata(path).unwrap();
    SessionFile {
        agent: PiParser::AGENT,
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
// The usage-bearing entry kinds
// ---------------------------------------------------------------------------

#[test]
fn usage_entries_model_work_and_compaction_count() {
    let (home, _data, _guard) = sandbox();
    let path = pi_session(
        home.path(),
        &format!("2026-09-24T08-00-00-000Z_{ID}.jsonl"),
        SESSION.as_bytes(),
    );

    let (delta, checkpoint) = PiParser.parse(&session_file(&path), None);
    // e0000005 (assistant), e0000006 (toolResult on the model in use),
    // e0000007 (assistant, its own model string), e0000008 (a usage entry)
    // and e0000009 (compaction, on the model last replied with).
    assert_eq!(delta.len(), 5);
    assert_eq!(checkpoint.calls_seen, 5);

    let first = &delta[0];
    assert_eq!(first.at, at("2026-09-24T08:00:05.000Z"));
    assert_eq!(first.session, ID, "the header's id names the session");
    assert_eq!(first.agent, "pi");
    // pi's input already excludes the cache read (the codex.rs matrix):
    // mapped as they stand, never stripped.
    assert_eq!(
        first.tokens,
        SessionTokens {
            input: 100,
            output: 50,
            cache_read: 1000,
            cache_write: 200
        }
    );
    // The ask was the user entry at 08:00:01.
    assert_eq!(first.latency_ms, Some(4000));

    // The toolResult's model work counts on the model in use.
    assert_eq!(delta[1].model_answered, "claude-opus-5-5");
    assert_eq!(delta[1].tokens.input, 10);

    // An assistant message keeps its own model string verbatim.
    assert_eq!(delta[2].model_answered, "claude/claude-opus-5-5");

    // A usage entry with only cache_read is still a call.
    assert_eq!(
        delta[3].tokens,
        SessionTokens {
            input: 0,
            output: 0,
            cache_read: 300,
            cache_write: 0
        }
    );

    // The compaction counts on the model last replied with.
    assert_eq!(delta[4].model_answered, "claude/claude-opus-5-5");
    assert_eq!(delta[4].tokens.input, 40);
}

#[test]
fn fork_copy_is_skipped_by_time() {
    let (home, _data, _guard) = sandbox();
    let source = pi_session(
        home.path(),
        &format!("2026-09-24T08-00-00-000Z_{ID}.jsonl"),
        SESSION.as_bytes(),
    );
    // Forked an hour later: its file copies the source's entries (times and
    // all) then goes on. Set the copy's mtime later so the source file is
    // read first.
    let fork = pi_session(
        home.path(),
        &format!("2026-09-25T09-00-00-000Z_{FORK_ID}.jsonl"),
        FORK.as_bytes(),
    );
    let later = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let older = later - 7200;
    set_mtime(&fork, later);
    set_mtime(&source, older);

    let calls = read_calls(home.path(), None);
    let pi: Vec<&SessionCall> = calls.iter().filter(|c| c.agent == "pi").collect();
    // The source's 5 calls plus the fork's own single reply (f0000002);
    // the fork's copied entries counted where they were first written.
    assert_eq!(
        pi.len(),
        6,
        "the fork's copy of the source's entries is skipped"
    );
    let fork_calls: Vec<&SessionCall> = pi
        .iter()
        .filter(|c| c.session == FORK_ID)
        .copied()
        .collect();
    assert_eq!(fork_calls.len(), 1);
    assert_eq!(fork_calls[0].tokens.input, 7);
    let source_calls: Vec<&SessionCall> = pi.iter().filter(|c| c.session == ID).copied().collect();
    assert_eq!(source_calls.len(), 5);
}

fn set_mtime(path: &Path, secs: u64) {
    let time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    let f = std::fs::File::options().append(true).open(path).unwrap();
    f.set_times(std::fs::FileTimes::new().set_modified(time))
        .unwrap();
}

// ---------------------------------------------------------------------------
// jsonc tolerance and shape drift
// ---------------------------------------------------------------------------

#[test]
fn jsonc_comments_and_trailing_commas_are_tolerated() {
    let (home, _data, _guard) = sandbox();
    let lines = r#"{"type":"session","version":3,"id":"sess-jsonc","timestamp":"2026-09-24T08:00:00.000Z","cwd":"/work/pi", // the header
  "parentSession":null,}
{"type":"model_change","id":"e1","timestamp":"2026-09-24T08:00:00.100Z","model":"anthropic/claude-opus-5-5", /* provider/model */}
{"type":"message","id":"e2","timestamp":"2026-09-24T08:00:01.000Z","message":{"role":"user","content":"go",}}
{"type":"message","id":"e3","timestamp":"2026-09-24T08:00:05.000Z","message":{"role":"assistant","model":"claude-opus-5-5","usage":{"input":9,"output":3,"cacheRead":10,"cacheWrite":0,"totalTokens":22,},},}
{"not json at all"
"#;
    let path = pi_session(
        home.path(),
        "2026-09-24T08-00-00-000Z_sess-jsonc.jsonl",
        lines.as_bytes(),
    );

    let (delta, checkpoint) = PiParser.parse(&session_file(&path), None);
    assert_eq!(
        delta.len(),
        1,
        "comments, trailing commas and junk lines tolerated"
    );
    assert_eq!(delta[0].session, "sess-jsonc");
    assert_eq!(
        delta[0].model_answered, "claude-opus-5-5",
        "model_change's provider/model takes the id half"
    );
    assert_eq!(delta[0].tokens.input, 9);
    assert_eq!(checkpoint.calls_seen, 1);
}

#[test]
fn model_change_without_model_id_does_not_clear_the_model() {
    let (home, _data, _guard) = sandbox();
    let lines = r#"{"type":"session","id":"sess-mc","timestamp":"2026-09-24T08:00:00.000Z"}
{"type":"model_change","id":"e1","timestamp":"2026-09-24T08:00:00.100Z","modelId":"claude-opus-5-5"}
{"type":"model_change","id":"e2","timestamp":"2026-09-24T08:00:00.200Z","model":"bare-without-slash"}
{"type":"compaction","id":"e3","timestamp":"2026-09-24T08:03:00.000Z","usage":{"input":5,"output":1,"cacheRead":0,"cacheWrite":0}}
"#;
    let path = pi_session(
        home.path(),
        "2026-09-24T08-00-00-000Z_sess-mc.jsonl",
        lines.as_bytes(),
    );
    let (delta, _) = PiParser.parse(&session_file(&path), None);
    // A bare model without '/' does not override; the compaction counts on
    // the model in force.
    assert_eq!(delta.len(), 1);
    assert_eq!(delta[0].model_answered, "claude-opus-5-5");
}

// ---------------------------------------------------------------------------
// Incremental semantics
// ---------------------------------------------------------------------------

#[test]
fn append_produces_only_delta_and_calls_seen_converges() {
    let (home, _data, _guard) = sandbox();
    let path = pi_session(
        home.path(),
        &format!("2026-09-24T08-00-00-000Z_{ID}.jsonl"),
        SESSION.as_bytes(),
    );
    let (first_delta, first) = PiParser.parse(&session_file(&path), None);
    assert_eq!(first_delta.len(), 5);

    let appended = r#"{"type":"message","id":"e0000011","parentId":"e0000009","timestamp":"2026-09-24T08:05:00.000Z","message":{"role":"user","content":"again","timestamp":1790237100000}}
{"type":"message","id":"e0000012","parentId":"e0000011","timestamp":"2026-09-24T08:05:06.000Z","message":{"role":"assistant","content":[],"provider":"anthropic","model":"claude-opus-5-5","usage":{"input":8,"output":2,"cacheRead":0,"cacheWrite":0,"totalTokens":10},"stopReason":"stop","timestamp":1790237106000}}
"#;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(appended.as_bytes())
        .unwrap();

    let (second_delta, second) = PiParser.parse(&session_file(&path), Some(first.clone()));
    assert_eq!(second_delta.len(), 1, "append produces only the delta call");
    assert_eq!(second_delta[0].latency_ms, Some(6000));
    assert_eq!(second.calls_seen, 6);
    assert_eq!(second.offset, std::fs::metadata(&path).unwrap().len());

    let (full_delta, full) = PiParser.parse(&session_file(&path), None);
    assert_eq!(full.calls_seen, second.calls_seen);
    assert_eq!(full_delta.len(), 6);
    let (noop, returned) = PiParser.parse(&session_file(&path), Some(full.clone()));
    assert!(noop.is_empty());
    assert_eq!(returned, full);
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

#[test]
fn discovery_reads_session_dir_setting_and_env() {
    // The sessionDir setting (jsonc-tolerated settings.json) adds a flat
    // folder of the user's choosing.
    let (home, _data, _guard) = sandbox();
    let flat = home.path().join("flat");
    std::fs::create_dir_all(&flat).unwrap();
    std::fs::write(
        flat.join(format!("2026-09-24T08-00-00-000Z_{ID}.jsonl")),
        SESSION,
    )
    .unwrap();
    let agent = home.path().join(".pi").join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(
        agent.join("settings.json"),
        format!(
            "{{\n  // where the sessions go\n  \"sessionDir\": {},\n}}",
            json_string(&flat)
        ),
    )
    .unwrap();

    let files = PiParser.discover(home.path());
    assert_eq!(files.len(), 1, "the flat folder named by the setting");
    assert_eq!(
        files[0].path,
        flat.join(format!("2026-09-24T08-00-00-000Z_{ID}.jsonl"))
    );
}

fn json_string(path: &Path) -> String {
    serde_json::to_string(&path.to_string_lossy()).unwrap()
}

#[test]
fn agent_dir_env_and_session_dir_env_honored_when_not_sandboxed() {
    let agent = tempfile::tempdir().unwrap();
    let dir = agent.path().join("sessions").join("--proj--");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("2026-09-24T08-00-00-000Z_{ID}.jsonl")),
        SESSION,
    )
    .unwrap();

    let home = tempfile::tempdir().unwrap();
    let files = {
        let _guard = EnvGuard::set(&[("PI_CODING_AGENT_DIR", agent.path())]);
        PiParser.discover(home.path())
    };
    assert_eq!(
        files.len(),
        1,
        "$PI_CODING_AGENT_DIR names the agent folder"
    );

    // $PI_CODING_AGENT_SESSION_DIR names a flat folder outright.
    let flat = tempfile::tempdir().unwrap();
    std::fs::write(
        flat.path().join("2026-09-24T08-00-00-000Z_flat-id.jsonl"),
        SESSION,
    )
    .unwrap();
    let files = {
        let _guard = EnvGuard::set(&[("PI_CODING_AGENT_SESSION_DIR", flat.path())]);
        PiParser.discover(home.path())
    };
    assert_eq!(files.len(), 1);
    assert!(
        files[0]
            .path
            .ends_with("2026-09-24T08-00-00-000Z_flat-id.jsonl")
    );
}

#[test]
fn non_session_names_are_skipped() {
    let (home, _data, _guard) = sandbox();
    let dir = home
        .path()
        .join(".pi")
        .join("agent")
        .join("sessions")
        .join("--work-pi--");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("no-underscore.jsonl"), SESSION).unwrap();
    std::fs::write(dir.join("trailing_.jsonl"), SESSION).unwrap();
    std::fs::write(
        dir.join(format!("2026-09-24T08-00-00-000Z_{ID}.jsonl")),
        SESSION,
    )
    .unwrap();

    let files = PiParser.discover(home.path());
    assert_eq!(files.len(), 1, "only <time>_<id>.jsonl names are sessions");
}
