//! Behavior tests for the omp parser (pi's format plus omp's own additions:
//! the title line, model_usage entries, and the artifacts sub-sessions that
//! count in the session they ran in).

use std::path::{Path, PathBuf};

use super::omp::OmpParser;
use super::{SessionCall, SessionParser, SessionFile, SessionTokens};
use crate::test_support::EnvGuard;

const MAIN: &str = include_str!("fixtures/omp_session.jsonl");
const SUB_EXPLORE: &str = include_str!("fixtures/omp_sub_explore.jsonl");
const SUB_FIND: &str = include_str!("fixtures/omp_sub_find.jsonl");
const SUB_ADVISOR: &str = include_str!("fixtures/omp_sub_advisor.jsonl");
const MAIN_ID: &str = "019a0000-0000-7000-8000-00000000000a";

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

/// The sandbox's omp project folder: `<home>/.omp/agent/sessions/--work-omp--`.
fn omp_project(home: &Path) -> PathBuf {
    let dir = home.join(".omp").join("agent").join("sessions").join("--work-omp--");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn main_session(home: &Path) -> PathBuf {
    let path = omp_project(home).join(format!("2026-09-28T08-00-00-000Z_{MAIN_ID}.jsonl"));
    std::fs::write(&path, MAIN).unwrap();
    path
}

fn session_file(path: &Path) -> SessionFile {
    let meta = std::fs::metadata(path).unwrap();
    SessionFile {
        agent: OmpParser::AGENT,
        path: path.to_path_buf(),
        size: meta.len(),
        modified_ms: 0,
    }
}

fn tokens_of(calls: &[SessionCall]) -> Vec<(String, SessionTokens)> {
    calls
        .iter()
        .map(|c| (c.model_answered.clone(), c.tokens))
        .collect()
}

#[test]
fn title_lines_and_model_usage_entries() {
    let (home, _data, _guard) = sandbox();
    let path = main_session(home.path());

    let (delta, checkpoint) = OmpParser.parse(&session_file(&path), None);
    // a0000004 (assistant, opus), a0000005 (a model_usage entry, haiku) and
    // a0000007 (assistant, opus). The leading fixed-width "title" line, the
    // header's own title and the title_change entry carry no usage; the task
    // tool's summed usage sits inside `details` and is not counted (the
    // subagents' own sessions already recorded it); the compaction entry has
    // no usage of its own.
    assert_eq!(delta.len(), 3);
    assert_eq!(checkpoint.calls_seen, 3);
    assert_eq!(
        tokens_of(&delta),
        vec![
            ("claude-opus-5-5".to_string(), SessionTokens { input: 100, output: 50, cache_read: 1000, cache_write: 200 }),
            ("claude-haiku-5".to_string(), SessionTokens { input: 20, output: 5, cache_read: 0, cache_write: 0 }),
            ("claude-opus-5-5".to_string(), SessionTokens { input: 10, output: 5, cache_read: 500, cache_write: 0 }),
        ]
    );
    assert!(delta.iter().all(|c| c.agent == "omp"));
    assert!(delta.iter().all(|c| c.session == MAIN_ID));
    // input as it stands: omp shares pi's semantics, input already without
    // the cache read (the codex.rs matrix).
    assert_eq!(delta[0].tokens.input, 100);
}

#[test]
fn artifacts_sub_sessions_count_in_their_parent() {
    let (home, _data, _guard) = sandbox();
    let path = main_session(home.path());
    // The artifacts folder beside the main file: a subagent, its own nested
    // subagent one folder deeper, the advisor, and side questions kept apart
    // as .json under btw-history.
    let artifacts = path.with_extension("");
    let explore_dir = artifacts.join("0-Explore");
    std::fs::create_dir_all(&explore_dir).unwrap();
    std::fs::create_dir_all(artifacts.join("btw-history")).unwrap();
    std::fs::write(artifacts.join("0-Explore.jsonl"), SUB_EXPLORE).unwrap();
    std::fs::write(explore_dir.join("0-Find.jsonl"), SUB_FIND).unwrap();
    std::fs::write(artifacts.join("__advisor.jsonl"), SUB_ADVISOR).unwrap();
    std::fs::write(artifacts.join("btw-history").join("1790582450000.json"), b"{}").unwrap();
    std::fs::write(artifacts.join("notes.txt"), b"not a session").unwrap();

    let files = OmpParser.discover(home.path());
    assert_eq!(files.len(), 4, "the main file and its three artifact sessions, nothing else");
    assert!(files.iter().all(|f| f.agent == "omp"));

    // Every artifact call is attributed to the session they ran in — also
    // the one nested a folder deeper.
    let mut all: Vec<(String, SessionTokens)> = Vec::new();
    for file in &files {
        let (delta, _) = OmpParser.parse(file, None);
        assert!(delta.iter().all(|c| c.session == MAIN_ID), "artifact calls carry the parent session id");
        all.extend(tokens_of(&delta));
    }
    // Main 3 calls + sonnet {300,30,100,0} + sonnet {40,4,0,0} + gpt-6-astra
    // {50,10,0,0} (magpie's pinned omp totals: the task tool's summed usage
    // counted once, by the subagents' own sessions).
    assert_eq!(all.len(), 6);
    let sonnet: Vec<_> = all.iter().filter(|(m, _)| m == "claude-sonnet-5").collect();
    assert_eq!(sonnet.len(), 2);
    assert_eq!(sonnet[0].1.input + sonnet[1].1.input, 340);
    assert_eq!(sonnet[0].1.output + sonnet[1].1.output, 34);
    assert!(all.iter().any(|(m, t)| m == "gpt-6-astra" && *t == SessionTokens { input: 50, output: 10, cache_read: 0, cache_write: 0 }));
}

#[test]
fn profiles_and_xdg_roots() {
    let xdg = tempfile::tempdir().unwrap();
    let xdg_project = xdg.path().join("omp").join("sessions").join("--x--");
    std::fs::create_dir_all(&xdg_project).unwrap();
    std::fs::write(xdg_project.join("2026-09-29T11-00-00-000Z_xdg-0001.jsonl"), MAIN).unwrap();

    // A profile's sessions folder is a root of its own; the sandbox pins
    // every root under home and ignores $XDG_DATA_HOME.
    {
        let (home, _data, _guard) = sandbox();
        let profile_project = home
            .path()
            .join(".omp")
            .join("profiles")
            .join("work")
            .join("agent")
            .join("sessions")
            .join("--p--");
        std::fs::create_dir_all(&profile_project).unwrap();
        std::fs::write(profile_project.join("2026-09-29T10-00-00-000Z_prof-0001.jsonl"), MAIN).unwrap();
        main_session(home.path());

        let files = OmpParser.discover(home.path());
        assert_eq!(files.len(), 2, "the agent folder's and the profile's");
        assert!(files.iter().any(|f| f.path.ends_with("2026-09-29T10-00-00-000Z_prof-0001.jsonl")));
        assert!(
            !files.iter().any(|f| f.path.to_str().is_some_and(|p| p.contains("xdg-0001"))),
            "the sandbox ignores $XDG_DATA_HOME"
        );
    }

    // Not sandboxed, $XDG_DATA_HOME/omp/sessions is another root.
    {
        let plain_home = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set(&[("XDG_DATA_HOME", xdg.path())]);
        let files = OmpParser.discover(plain_home.path());
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("2026-09-29T11-00-00-000Z_xdg-0001.jsonl"));
    }
}
