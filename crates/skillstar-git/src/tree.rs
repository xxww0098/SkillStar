//! Bounded reads of tracked Git tree metadata.

use anyhow::{Context, Result, anyhow};
use skillstar_core::infra::path_env::command_with_path;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

const MAX_TREE_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_TREE_ENTRIES: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTreeEntry {
    pub mode: String,
    pub kind: String,
    pub path: String,
}

pub fn list_tree_paths(repo_path: &Path) -> Result<Vec<String>> {
    list_tree_paths_at(repo_path, "HEAD")
}

pub fn list_tree_paths_at(repo_path: &Path, revision: &str) -> Result<Vec<String>> {
    Ok(list_tree_entries_at(repo_path, revision)?
        .into_iter()
        .map(|entry| entry.path)
        .collect())
}

pub fn list_tree_entries_at(repo_path: &Path, revision: &str) -> Result<Vec<GitTreeEntry>> {
    let mut command = command_with_path("git");
    command
        .current_dir(repo_path)
        .args(["ls-tree", "-r", "-z", revision])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("Failed to execute git ls-tree")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("git ls-tree stdout is unavailable"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("git ls-tree stderr is unavailable"))?;
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let mut stdout = stdout.take((MAX_TREE_OUTPUT_BYTES + 1) as u64);
    let mut stdout_bytes = Vec::new();
    stdout
        .read_to_end(&mut stdout_bytes)
        .context("Failed to read git ls-tree output")?;
    if stdout_bytes.len() > MAX_TREE_OUTPUT_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stderr_reader.join();
        return Err(anyhow!("git ls-tree output exceeds the supported limit"));
    }
    let status = child.wait().context("Failed to wait for git ls-tree")?;
    let stderr = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        let err = String::from_utf8_lossy(&stderr);
        return Err(anyhow!("git ls-tree failed: {}", err.trim()));
    }

    let entries = String::from_utf8(stdout_bytes)
        .context("git ls-tree returned a non-UTF-8 repository path")?
        .split_terminator('\0')
        .take(MAX_TREE_ENTRIES + 1)
        .map(parse_tree_entry)
        .collect::<Result<Vec<_>>>()?;
    if entries.len() > MAX_TREE_ENTRIES {
        return Err(anyhow!(
            "git ls-tree entry count exceeds the supported limit"
        ));
    }
    Ok(entries)
}

/// Does `revision` still track this repository-relative path?
///
/// A managed Skill link can dangle for two very different reasons: a sparse
/// checkout simply has not materialized the path, or upstream deleted the
/// Skill outright. Only the tree can tell them apart, so this answers for one
/// exact revision. `None` means the revision itself does not resolve — an
/// unfetched clone must never be read as "upstream removed it".
pub fn revision_contains_path(repo_path: &Path, revision: &str, pathspec: &str) -> Option<bool> {
    if pathspec.is_empty() {
        return None;
    }
    run_tree_git(repo_path, &["rev-parse", "--verify", "--quiet", revision])?;
    let listed = run_tree_git(
        repo_path,
        &["ls-tree", "--name-only", revision, "--", pathspec],
    )?;
    Some(!listed.trim().is_empty())
}

/// Run a read-only git command, returning `None` for any non-zero exit.
fn run_tree_git(repo_path: &Path, args: &[&str]) -> Option<String> {
    let output = command_with_path("git")
        .current_dir(repo_path)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_tree_entry(record: &str) -> Result<GitTreeEntry> {
    let (header, path) = record
        .split_once('\t')
        .ok_or_else(|| anyhow!("git ls-tree returned an invalid entry"))?;
    let mut header = header.split_whitespace();
    let mode = header
        .next()
        .ok_or_else(|| anyhow!("git ls-tree entry has no mode"))?;
    let kind = header
        .next()
        .ok_or_else(|| anyhow!("git ls-tree entry has no type"))?;
    let _object_id = header
        .next()
        .ok_or_else(|| anyhow!("git ls-tree entry has no object id"))?;
    if header.next().is_some() {
        return Err(anyhow!("git ls-tree entry has unexpected metadata"));
    }
    Ok(GitTreeEntry {
        mode: mode.to_string(),
        kind: kind.to_string(),
        path: path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blob_and_gitlink_entries() {
        assert_eq!(
            parse_tree_entry("100644 blob aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tSKILL.md")
                .unwrap(),
            GitTreeEntry {
                mode: "100644".into(),
                kind: "blob".into(),
                path: "SKILL.md".into(),
            }
        );
        assert_eq!(
            parse_tree_entry("160000 commit bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\tvendor/sub")
                .unwrap()
                .kind,
            "commit"
        );
    }
}
