//! Behavior tests for the claude family parsers (all pinning the contract
//! points of slice 05).
//!
//! Sandbox discipline: all tests are doubly sandboxed via
//! `SKILLSTAR_TOOL_SYNC_HOME` (agent directory tree) + `SKILLSTAR_DATA_DIR`
//! (checkpoint index) and never touch the real `$HOME` (the spec's global
//! firewall rule 4). Fixtures were rewritten from magpie testdata plus
//! synthetic boundary samples, embedded at compile time with include_str!
//! and expanded into temp directories at runtime.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::claude::{ClaudeCodeParser, ClaudeDesktopParser};
use super::{SessionCall, SessionFile, SessionParser, read_calls};
use crate::test_support::EnvGuard;
use sha2::Digest;

const NORMAL: &str = include_str!("fixtures/normal.jsonl");
const SUBAGENT: &str = include_str!("fixtures/subagent.jsonl");
const RESUMED_SOURCE: &str = include_str!("fixtures/resumed_source.jsonl");
const RESUMED_COPY: &str = include_str!("fixtures/resumed_copy.jsonl");
const DESKTOP_ENTRYPOINT: &str = include_str!("fixtures/desktop_entrypoint.jsonl");

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

/// Set up a claude projects tree under the sandbox home.
fn claude_projects(home: &Path) -> PathBuf {
    let projects = home.join(".claude").join("projects").join("-work-app");
    std::fs::create_dir_all(&projects).unwrap();
    projects
}

fn session_file(path: &Path) -> SessionFile {
    let meta = std::fs::metadata(path).unwrap();
    SessionFile {
        agent: ClaudeCodeParser::AGENT,
        path: path.to_path_buf(),
        size: meta.len(),
        modified_ms: 0,
    }
}

/// A read-only snapshot of the agent directory tree: path → (size, mtime
/// nanoseconds, content sha256).
fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, (u64, u128, String)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let meta = std::fs::metadata(&path).unwrap();
            let content = std::fs::read(&path).unwrap();
            let mtime = meta
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            out.insert(
                path,
                (
                    meta.len(),
                    mtime,
                    super::checkpoint::hex(&sha2::Sha256::digest(&content)),
                ),
            );
        }
    }
    out
}

fn find_call<'a>(calls: &'a [SessionCall], needle: &str) -> &'a SessionCall {
    calls
        .iter()
        .find(|c| c.request_id.as_deref() == Some(needle))
        .unwrap_or_else(|| panic!("request_id {needle} not found"))
}

// ---------------------------------------------------------------------------
// Full parsing (pinning magpie's empirical lessons)
// ---------------------------------------------------------------------------

#[test]
fn normal_session_counts_calls_with_multi_block_override() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("11111111-2222-3333-4444-555555555555.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let (delta, checkpoint) = ClaudeCodeParser.parse(&session_file(&session), None);
    // msg_1 (two blocks merged into one), msg_2, msg_err, msg_3 = 4 calls;
    // the count after msg id dedup.
    assert_eq!(
        delta.len(),
        4,
        "permission/ai-title/file-history/identity/user lines produce no call"
    );
    assert_eq!(checkpoint.calls_seen, 4);
    assert_eq!(
        checkpoint.offset,
        std::fs::metadata(&session).unwrap().len()
    );

    // Same message id, multiple blocks: later block usage overrides the
    // earlier one (110/60/5500/1100); From takes the first block's line
    // position (file start; no assistant line before it → 0).
    let first = find_call(&delta, "req_1");
    assert_eq!(first.model_answered, "claude-opus-5-5");
    assert_eq!(
        first.tokens,
        super::SessionTokens {
            input: 110,
            output: 60,
            cache_read: 5500,
            cache_write: 1100,
        }
    );
    assert_eq!(first.from, 0);
    // The final version is the later block (at 10:00:06); latency measured
    // from the ask start (10:00:01): 5000ms; From unchanged, to is the later
    // block's line end (the byte sum of the first 6 lines).
    assert_eq!(first.latency_ms, Some(5000));
    let six_lines: u64 = NORMAL.lines().take(6).map(|l| l.len() as u64 + 1).sum();
    assert_eq!(first.to, six_lines);
    assert_eq!(first.session, "11111111-2222-3333-4444-555555555555");
    assert_eq!(first.agent, "claude-code");
}

#[test]
fn synthetic_api_error_call_has_error_kind_and_no_tokens() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("11111111-2222-3333-4444-555555555555.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let (delta, _) = ClaudeCodeParser.parse(&session_file(&session), None);
    let error = find_call(&delta, "req_err");
    assert_eq!(error.error_kind.as_deref(), Some("rate_limit"));
    assert!(error.tokens.is_zero());
    // magpie precedent: an error line's Model stays empty.
    assert_eq!(error.model_answered, "");
}

#[test]
fn identity_attachment_sets_model_asked() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("11111111-2222-3333-4444-555555555555.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let (delta, _) = ClaudeCodeParser.parse(&session_file(&session), None);
    // msg_2 / msg_3 come after the identity attachment: asked is the
    // identity's modelId. msg_1 comes before it: without identity
    // information, asked equals answered.
    let early = find_call(&delta, "req_1");
    assert_eq!(early.model_asked, "claude-opus-5-5");
    let after = find_call(&delta, "req_2");
    assert_eq!(after.model_asked, "claude-sonnet-5");
    assert_eq!(after.model_answered, "claude-haiku-4-5-20251001");
    // effort: perTurnEffort wins over effort (msg_3's effort=high/perTurn=max).
    assert_eq!(find_call(&delta, "req_3").effort.as_deref(), Some("max"));
}

#[test]
fn user_without_session_id_falls_back_to_file_name() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("22222222-2222-3333-4444-555555555555.jsonl");
    // An assistant line with the sessionId field removed: the session id is
    // derived from the file name.
    let line = r#"{"parentUuid":null,"isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_n1","type":"message","role":"assistant","content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":2}},"requestId":"req_n1","type":"assistant","uuid":"x1","timestamp":"2026-09-20T12:00:00.000Z","cwd":"/work/app"}"#;
    std::fs::write(&session, format!("{line}\n")).unwrap();

    let (delta, _) = ClaudeCodeParser.parse(&session_file(&session), None);
    assert_eq!(delta[0].session, "22222222-2222-3333-4444-555555555555");
}

#[test]
fn desktop_entrypoint_attribute_moves_call_to_claude_desktop() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("dddddddd-0000-0000-0000-000000000004.jsonl");
    std::fs::write(&session, DESKTOP_ENTRYPOINT).unwrap();

    let (delta, _) = ClaudeCodeParser.parse(&session_file(&session), None);
    // The value observed on this machine, `claude-desktop-3p`: the prefix
    // attributes to claude-desktop.
    assert_eq!(delta[0].agent, "claude-desktop");
    assert_eq!(delta[0].session, "dddddddd-0000-0000-0000-000000000004");
}

#[test]
fn assistant_line_without_usage_is_not_a_call() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("no-usage.jsonl");
    let line = r#"{"parentUuid":null,"isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_x","type":"message","role":"assistant","content":[{"type":"text","text":"partial"}]},"requestId":"req_x","type":"assistant","uuid":"x","timestamp":"2026-09-20T12:00:00.000Z","cwd":"/work/app"}"#;
    std::fs::write(&session, format!("{line}\n")).unwrap();

    let (delta, checkpoint) = ClaudeCodeParser.parse(&session_file(&session), None);
    assert!(delta.is_empty());
    assert_eq!(checkpoint.calls_seen, 0);
    // offset still advanced past the complete line (the next append resumes
    // from here).
    assert_eq!(checkpoint.offset, line.len() as u64 + 1);
}

#[test]
fn partial_trailing_line_is_left_for_next_read() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("partial.jsonl");
    let full = r#"{"parentUuid":null,"isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_p","type":"message","role":"assistant","content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":2}},"requestId":"req_p","type":"assistant","uuid":"p1","timestamp":"2026-09-20T12:00:00.000Z","cwd":"/work/app"}
"#;
    // First write the half line without its newline, then complete it.
    let (whole, _tail) = full.split_at(full.len() - 10);
    std::fs::write(&session, whole).unwrap();
    let (delta, checkpoint) = ClaudeCodeParser.parse(&session_file(&session), None);
    assert!(delta.is_empty(), "a half-written line is not consumed");
    assert_eq!(checkpoint.offset, 0);

    std::fs::write(&session, full).unwrap();
    let (delta, _) = ClaudeCodeParser.parse(&session_file(&session), Some(checkpoint));
    assert_eq!(delta.len(), 1);
}

// ---------------------------------------------------------------------------
// Incremental semantics
// ---------------------------------------------------------------------------

#[test]
fn append_produces_only_delta_and_calls_seen_converges() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("grow.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let (first_delta, first) = ClaudeCodeParser.parse(&session_file(&session), None);
    assert_eq!(first_delta.len(), 4);
    assert_eq!(first.calls_seen, 4);

    // Append one new user + assistant: only the new assistant produces a
    // delta.
    let appended = r#"{"parentUuid":"a5","isSidechain":false,"type":"user","message":{"role":"user","content":"more"},"uuid":"u4","timestamp":"2026-09-20T10:07:00.000Z","cwd":"/work/app","sessionId":"11111111-2222-3333-4444-555555555555"}
{"parentUuid":"u4","isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_4","type":"message","role":"assistant","content":[{"type":"text","text":"more!"}],"usage":{"input_tokens":9,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":3}},"requestId":"req_4","type":"assistant","uuid":"a6","timestamp":"2026-09-20T10:07:05.000Z","cwd":"/work/app","sessionId":"11111111-2222-3333-4444-555555555555"}
"#;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&session)
        .unwrap()
        .write_all(appended.as_bytes())
        .unwrap();

    let (second_delta, second) =
        ClaudeCodeParser.parse(&session_file(&session), Some(first.clone()));
    assert_eq!(second_delta.len(), 1, "append produces only delta calls");
    assert_eq!(second_delta[0].request_id.as_deref(), Some("req_4"));
    assert_eq!(
        second_delta[0].latency_ms,
        Some(5000),
        "incremental parsing also gets latency (the user line is inside the append)"
    );
    assert_eq!(second.calls_seen, 5);
    assert_eq!(second.offset, std::fs::metadata(&session).unwrap().len());

    // Full reread from zero: calls_seen converges to the same value.
    let (full_reread_delta, full_reread) = ClaudeCodeParser.parse(&session_file(&session), None);
    assert_eq!(full_reread.calls_seen, second.calls_seen);
    assert_eq!(full_reread_delta.len(), 5);
    // Parsing again when unchanged: handed back as-is, zero delta.
    let (noop, returned) =
        ClaudeCodeParser.parse(&session_file(&session), Some(full_reread.clone()));
    assert!(noop.is_empty());
    assert_eq!(returned, full_reread);
}

#[test]
fn later_block_after_checkpoint_overrides_and_keeps_first_from() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("block.jsonl");
    let head = r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"go"},"uuid":"u1","timestamp":"2026-09-20T10:00:01.000Z","cwd":"/work/app","sessionId":"sess-block"}
{"parentUuid":"u1","isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_z","type":"message","role":"assistant","content":[{"type":"thinking","thinking":".."}],"usage":{"input_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":1}},"requestId":"req_z1","type":"assistant","uuid":"z1","timestamp":"2026-09-20T10:00:05.000Z","cwd":"/work/app","sessionId":"sess-block"}
"#;
    std::fs::write(&session, head).unwrap();
    let (delta, first) = ClaudeCodeParser.parse(&session_file(&session), None);
    assert_eq!(delta.len(), 1);

    // Append a later block with the same msg id: usage overrides, From
    // keeps the first block's position.
    let tail = r#"{"parentUuid":"z1","isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_z","type":"message","role":"assistant","content":[{"type":"tool_use","id":"t9","name":"Edit","input":{}}],"usage":{"input_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":2}},"requestId":"req_z1","type":"assistant","uuid":"z2","timestamp":"2026-09-20T10:00:08.000Z","cwd":"/work/app","sessionId":"sess-block"}
"#;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&session)
        .unwrap()
        .write_all(tail.as_bytes())
        .unwrap();

    let (delta, second) = ClaudeCodeParser.parse(&session_file(&session), Some(first));
    assert_eq!(
        delta.len(),
        1,
        "the overriding block yields the message's final version"
    );
    assert_eq!(
        delta[0].tokens.input, 20,
        "later block usage overrides the earlier one"
    );
    assert_eq!(
        delta[0].from, 0,
        "From keeps the first block's position (no assistant line before it, i.e. file start)"
    );
    assert_eq!(
        delta[0].to,
        std::fs::metadata(&session).unwrap().len(),
        "To is the later block's line end"
    );
    assert_eq!(second.calls_seen, 1, "same msg id not counted twice");
}

#[test]
fn truncated_file_rereads_from_zero() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("truncated.jsonl");
    std::fs::write(&session, NORMAL).unwrap();
    let (_, prior) = ClaudeCodeParser.parse(&session_file(&session), None);
    assert_eq!(prior.calls_seen, 4);

    // Truncated to half (the 256B head gets shorter too): must reread from
    // zero; calls_seen falls back.
    let content = std::fs::read_to_string(&session).unwrap();
    let cut: String = content.lines().take(5).map(|l| format!("{l}\n")).collect();
    std::fs::write(&session, &cut).unwrap();

    let (delta, after) = ClaudeCodeParser.parse(&session_file(&session), Some(prior));
    assert_eq!(
        delta.len(),
        1,
        "after truncation only msg_1 (first block) remains"
    );
    assert_eq!(after.calls_seen, 1);
    assert_eq!(after.offset, cut.len() as u64);
}

#[test]
fn replaced_file_with_changed_head_rereads_from_zero() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("replaced.jsonl");
    std::fs::write(&session, NORMAL).unwrap();
    let (_, prior) = ClaudeCodeParser.parse(&session_file(&session), None);

    // Same-length replacement: the first 256B of the head change → head
    // fingerprint mismatch → from zero (the fast path lets it through into
    // the full check, where the prefix check confirms it again).
    let padded = format!(
        "{DESKTOP_ENTRYPOINT}\n{}",
        " ".repeat(NORMAL.len() - DESKTOP_ENTRYPOINT.len() - 1)
    );
    assert_eq!(padded.len(), NORMAL.len(), "same-length replacement");
    assert_ne!(&padded.as_bytes()[..256], &NORMAL.as_bytes()[..256]);
    std::fs::write(&session, &padded).unwrap();

    let (delta, after) = ClaudeCodeParser.parse(&session_file(&session), Some(prior));
    assert_eq!(delta.len(), 1, "the msg_d1 inside the replaced content");
    assert_eq!(after.calls_seen, 1);
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

#[test]
fn discovery_defaults_to_sandboxed_claude_projects() {
    let (home, _data, _guard) = sandbox();
    // SKILLSTAR_TOOL_SYNC_HOME is set: claude-code reads
    // <home>/.claude/projects.
    let projects = claude_projects(home.path());
    std::fs::write(
        projects.join("11111111-2222-3333-4444-555555555555.jsonl"),
        NORMAL,
    )
    .unwrap();
    let subagents = projects
        .join("11111111-2222-3333-4444-555555555555")
        .join("subagents");
    std::fs::create_dir_all(&subagents).unwrap();
    std::fs::write(subagents.join("agent-a1.jsonl"), SUBAGENT).unwrap();

    let files = ClaudeCodeParser.discover(home.path());
    assert_eq!(
        files.len(),
        2,
        "main session + subagents discovered together"
    );
    let paths: Vec<&Path> = files.iter().map(|f| f.path.as_path()).collect();
    assert!(
        paths.contains(
            &projects
                .join("11111111-2222-3333-4444-555555555555.jsonl")
                .as_path()
        )
    );
    assert!(paths.contains(&subagents.join("agent-a1.jsonl").as_path()));

    // A subagent file's session id is the parent directory name
    // (sessionOfPath precedent).
    let sub = files
        .iter()
        .find(|f| f.path.ends_with("agent-a1.jsonl"))
        .unwrap();
    let (delta, _) = ClaudeCodeParser.parse(sub, None);
    assert_eq!(delta.len(), 1);
    assert_eq!(delta[0].session, "11111111-2222-3333-4444-555555555555");
}

#[test]
fn discovery_honors_claude_config_dir_when_not_sandboxed() {
    // Without SKILLSTAR_TOOL_SYNC_HOME: $CLAUDE_CONFIG_DIR takes effect
    // (the production path).
    let config = tempfile::tempdir().unwrap();
    let projects = config.path().join("projects").join("-proj");
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(projects.join("sess-env.jsonl"), NORMAL).unwrap();

    let home = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::set(&[("CLAUDE_CONFIG_DIR", config.path())]);
    let files = ClaudeCodeParser.discover(home.path());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, projects.join("sess-env.jsonl"));
}

#[test]
fn sandbox_wins_over_claude_config_dir() {
    // The sandbox wins: when SKILLSTAR_TOOL_SYNC_HOME is set,
    // CLAUDE_CONFIG_DIR is ignored (tool_paths.rs precedent: tests never
    // escape into a real ~/.claude).
    let config = tempfile::tempdir().unwrap();
    let env_projects = config.path().join("projects").join("-env");
    std::fs::create_dir_all(&env_projects).unwrap();
    std::fs::write(env_projects.join("sess-env.jsonl"), NORMAL).unwrap();

    let sandbox_home = tempfile::tempdir().unwrap();
    let other_projects = sandbox_home
        .path()
        .join(".claude")
        .join("projects")
        .join("-p");
    std::fs::create_dir_all(&other_projects).unwrap();
    std::fs::write(other_projects.join("sess-sandbox.jsonl"), NORMAL).unwrap();

    let _guard = EnvGuard::set(&[
        ("CLAUDE_CONFIG_DIR", config.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", sandbox_home.path()),
    ]);
    let files = ClaudeCodeParser.discover(sandbox_home.path());
    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("sess-sandbox.jsonl"));
}

#[test]
fn desktop_discovery_finds_cowork_claude_homes() {
    let (home, _data, _guard) = sandbox();
    // Cowork tree: local-agent-mode-sessions/<a>/<b>/local_*/.claude/projects/…
    let cowork_home = home
        .path()
        .join("Library")
        .join("Application Support")
        .join("Claude")
        .join("local-agent-mode-sessions")
        .join("ws-1")
        .join("profile-1")
        .join("local_6f88c8b9")
        .join(".claude");
    let projects = cowork_home.join("projects").join("-work");
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(projects.join("cowork-session.jsonl"), NORMAL).unwrap();

    let files = ClaudeDesktopParser.discover(home.path());
    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("cowork-session.jsonl"));
    assert_eq!(files[0].agent, "claude-desktop");

    let (delta, _) = ClaudeDesktopParser.parse(&files[0], None);
    assert_eq!(delta.len(), 4);
    assert_eq!(delta[0].agent, "claude-desktop");
}

// ---------------------------------------------------------------------------
// read_calls: cross-file dedup / since / checkpoint persistence / read-only red line
// ---------------------------------------------------------------------------

#[test]
fn read_calls_dedups_resumed_message_across_files() {
    let (home, _data, _guard) = sandbox();
    let projects = claude_projects(home.path());
    std::fs::write(projects.join("000-source.jsonl"), RESUMED_SOURCE).unwrap();
    std::fs::write(projects.join("999-copy.jsonl"), RESUMED_COPY).unwrap();

    let calls = read_calls(home.path(), None);
    // msg_A has one line in each of the two files; earliest file first,
    // counted once.
    assert_eq!(calls.len(), 2, "msg_A + msg_B");
    let tokens: Vec<_> = calls.iter().map(|c| c.tokens.input).collect();
    assert!(tokens.contains(&100), "msg_A (claimed by the source file)");
    assert!(tokens.contains(&7), "msg_B");
    // Reverse chronological order.
    assert!(calls[0].at >= calls[1].at);
}

#[test]
fn read_calls_filters_by_since() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("11111111-2222-3333-4444-555555555555.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let all = read_calls(home.path(), None);
    assert_eq!(all.len(), 4);

    // After 10:05:00 (msg_2): msg_2, msg_err, msg_3.
    let since = chrono::DateTime::parse_from_rfc3339("2026-09-20T10:05:00.000Z")
        .unwrap()
        .timestamp_millis();
    let later = read_calls(home.path(), Some(since));
    assert_eq!(later.len(), 3);
    assert!(later.iter().all(|c| c.at >= since));

    // A future timestamp: empty.
    let future = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00.000Z")
        .unwrap()
        .timestamp_millis();
    assert!(read_calls(home.path(), Some(future)).is_empty());
}

#[test]
fn read_calls_persists_and_reuses_checkpoints() {
    let (home, data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("11111111-2222-3333-4444-555555555555.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let first = read_calls(home.path(), None);
    let index = data
        .path()
        .join("cache")
        .join("sessions")
        .join("index.json");
    assert!(
        index.exists(),
        "the checkpoint index lands in the rebuildable cache, not the agent directory"
    );

    // Second call (file unchanged): same result, checkpoint reused as-is.
    let second = read_calls(home.path(), None);
    assert_eq!(first, second);

    // The index content round-trips (save → load equal).
    let bytes = std::fs::read(&index).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["version"], 1);
    let files = value["files"].as_object().unwrap();
    assert_eq!(files.len(), 1);
    let entry = files.values().next().unwrap();
    assert_eq!(entry["calls_seen"], 4);
    assert_eq!(entry["version"], 1);

    // After the file vanishes: the index prunes the entry.
    std::fs::remove_file(&session).unwrap();
    read_calls(home.path(), None);
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();
    assert_eq!(value["files"].as_object().unwrap().len(), 0);
}

#[test]
fn read_calls_never_writes_agent_directory() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("11111111-2222-3333-4444-555555555555.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let before = snapshot_tree(home.path());
    let calls = read_calls(home.path(), None);
    assert!(!calls.is_empty());
    let after = snapshot_tree(home.path());
    assert_eq!(
        before, after,
        "read-only red line: no byte or mtime of the agent directory may change"
    );
}

#[test]
fn read_calls_incremental_matches_full_reread() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("11111111-2222-3333-4444-555555555555.jsonl");
    std::fs::write(&session, NORMAL).unwrap();

    let baseline = read_calls(home.path(), None);

    // After an append, the incremental read vs a from-zero reread (index
    // deleted) must give the same full view.
    let appended = r#"{"parentUuid":"a5","isSidechain":false,"type":"user","message":{"role":"user","content":"again"},"uuid":"u5","timestamp":"2026-09-20T10:08:00.000Z","cwd":"/work/app","sessionId":"11111111-2222-3333-4444-555555555555"}
{"parentUuid":"u5","isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_5","type":"message","role":"assistant","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":11,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":4}},"requestId":"req_5","type":"assistant","uuid":"a7","timestamp":"2026-09-20T10:08:05.000Z","cwd":"/work/app","sessionId":"11111111-2222-3333-4444-555555555555"}
"#;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&session)
        .unwrap()
        .write_all(appended.as_bytes())
        .unwrap();

    let incremental = read_calls(home.path(), None);
    assert_eq!(incremental.len(), baseline.len() + 1);

    // Clear the index → full reread.
    std::fs::remove_file(super::checkpoint::index_path()).ok();
    let full = read_calls(home.path(), None);
    assert_eq!(
        incremental, full,
        "the incremental view matches the full reread"
    );
}

// ---------------------------------------------------------------------------
// Lenient lines (shape drift in real files must not panic or miscount)
// ---------------------------------------------------------------------------

#[test]
fn lenient_fields_do_not_break_parsing() {
    let (home, _data, _guard) = sandbox();
    let session = claude_projects(home.path()).join("lenient.jsonl");
    let lines = r#"{"type":"attachment","attachment":{"type":"document","content":"not identity"}}
{"garbage line without json"
{"parentUuid":null,"isSidechain":false,"message":{"model":"claude-sonnet-5","id":"msg_l","type":"message","role":"assistant","content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":2}},"requestId":12345,"sessionId":{"nested":"object"},"type":"assistant","uuid":"l1","timestamp":"2026-09-20T12:00:00.000Z","cwd":"/work/app"}
"#;
    std::fs::write(&session, lines).unwrap();

    let (delta, checkpoint) = ClaudeCodeParser.parse(&session_file(&session), None);
    // Malformed lines don't crash; requestId/sessionId shape drift reads as
    // empty (session falls back to the file name).
    assert_eq!(delta.len(), 1);
    assert_eq!(delta[0].request_id, None);
    assert_eq!(delta[0].session, "lenient");
    assert_eq!(checkpoint.calls_seen, 1);
}
