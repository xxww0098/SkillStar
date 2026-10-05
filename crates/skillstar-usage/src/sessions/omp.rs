//! omp (oh-my-pi, a fork of pi) session parsing: pi's format plus its own
//! discovery.
//!
//! omp keeps its sessions as pi does (`<time>_<session id>.jsonl` in a folder
//! per working directory) and they are read by the shared pi parse (see
//! pi.rs; the token matrix holds: input already excludes the cache read).
//! What omp adds on top:
//!
//! - a fixed-width `"title"` line before the header (rewritten in place) and
//!   `title_change` entries — neither carries usage, so both are simply
//!   stepped over like any other token-less entry;
//! - `"model_usage"` entries for model calls outside the conversation (the
//!   shared parse reads them as pi's `"usage"` ones);
//! - its subagents' and advisor's sessions in an artifacts folder beside the
//!   main file (`<time>_<id>/<agent>.jsonl`, theirs in turn one folder
//!   deeper; side questions under `btw-history` are `.json` and skipped):
//!   their calls are attributed to the session they ran in (the shared
//!   `family_session_of_path` in pi.rs resolves the parent from the path).
//!
//! Roots (magpie ompSessionRoots precedent): the agent folder's `sessions/`,
//! each profile's (`~/.omp/profiles/*/agent/sessions`), and
//! `$XDG_DATA_HOME/omp/sessions` on non-Windows once omp has moved its data
//! there. The sandbox wins over `XDG_DATA_HOME`.

use std::path::{Path, PathBuf};

use super::pi::{pi_parse, pi_replay, session_file_of};
use super::{SessionCall, SessionFile, SessionParser};

/// Parser-private version: omp shares the pi family's semantics, but keeps
/// its own number so an omp checkpoint never resumes under changed pi rules
/// by accident.
pub(crate) const OMP_PARSER_VERSION: u32 = 1;

/// Session parser for omp.
pub(crate) struct OmpParser;

impl SessionParser for OmpParser {
    const AGENT: &'static str = "omp";
    const PARSER_VERSION: u32 = OMP_PARSER_VERSION;

    fn discover(&self, home: &Path) -> Vec<SessionFile> {
        omp_files(Self::AGENT, home)
    }

    fn parse(
        &self,
        file: &SessionFile,
        prior: Option<super::FileCheckpoint>,
    ) -> (Vec<SessionCall>, super::FileCheckpoint) {
        pi_parse(Self::AGENT, Self::PARSER_VERSION, file, prior)
    }

    fn unchanged(&self, file: &SessionFile, prior: &super::FileCheckpoint) -> bool {
        super::checkpoint::is_unchanged(prior, file, Self::PARSER_VERSION)
    }

    fn replay(&self, checkpoint: &super::FileCheckpoint) -> Vec<(String, SessionCall)> {
        pi_replay(checkpoint)
    }
}

/// omp's agent folder: `~/.omp/agent` (no override of its own; the sandbox
/// pins it under `home`).
fn omp_agent_dir(home: &Path) -> PathBuf {
    home.join(".omp").join("agent")
}

/// The folders omp keeps sessions in: the agent folder's `sessions/`, each
/// profile's, and `$XDG_DATA_HOME/omp/sessions` where omp has moved its data
/// (Linux and macOS; the sandbox wins over the env).
fn omp_session_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![omp_agent_dir(home).join("sessions")];
    let profiles = home.join(".omp").join("profiles");
    if let Ok(entries) = std::fs::read_dir(&profiles) {
        for profile in entries.flatten() {
            roots.push(profile.path().join("agent").join("sessions"));
        }
    }
    if !cfg!(target_os = "windows")
        && !crate::tool_paths::is_tool_sync_sandboxed()
        && let Some(xdg) = std::env::var_os("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        roots.push(PathBuf::from(xdg).join("omp").join("sessions"));
    }
    roots
}

/// Find omp's session files: each main `<time>_<id>.jsonl` one folder under
/// a root, plus every `*.jsonl` in its artifacts folder (any depth,
/// `btw-history` skipped), which belongs to the same session.
fn omp_files(agent: &'static str, home: &Path) -> Vec<SessionFile> {
    let mut out = Vec::new();
    for root in omp_session_roots(home) {
        let Ok(folders) = std::fs::read_dir(&root) else {
            continue;
        };
        for folder in folders.flatten() {
            if !folder.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(folder.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.extension().is_some_and(|ext| ext == "jsonl") {
                    continue;
                }
                let Some(file) = session_file_of(agent, &path) else {
                    continue;
                };
                out.push(file);
                // Its subagents' and advisor's sessions, at any depth.
                collect_artifacts(agent, path.with_extension(""), &mut out);
            }
        }
    }
    out
}

/// Collect the `*.jsonl` files under an artifacts folder (`<time>_<id>/`),
/// skipping the `btw-history` side-question folders. Unlike main session
/// files, an artifact's own name carries no `<time>_<id>` shape — the parent
/// session is resolved from the path at parse time.
fn collect_artifacts(agent: &'static str, dir: PathBuf, out: &mut Vec<SessionFile>) {
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if entry.file_name() == "btw-history" {
                continue;
            }
            collect_artifacts(agent, path, out);
        } else if path.extension().is_some_and(|ext| ext == "jsonl")
            && let Ok(meta) = entry.metadata()
            && meta.is_file()
        {
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            out.push(SessionFile {
                agent,
                path,
                size: meta.len(),
                modified_ms,
            });
        }
    }
}
