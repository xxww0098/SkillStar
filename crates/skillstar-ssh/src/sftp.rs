//! Remote skill operations over SFTP: push / list / delete.
//!
//! Pushing mirrors the local skill directory onto a remote agent folder:
//!
//! 1. Resolve the local skill's **real** content dir (following the hub
//!    symlink, same idea as `commands/skill_content.rs::resolve_skill_dir`).
//! 2. Walk it recursively (skipping `.git`), collecting relative paths — the
//!    same rule `list_skill_files` uses.
//! 3. For each file: `create_dir -p` its remote parent, write bytes to a
//!    `.skillstar.tmp` sibling, then `rename` over the target — atomic, so a
//!    crash mid-skill never leaves a half-written file.
//!
//! `list_remote_skills` walks the remote dir and reports each subdirectory
//! that contains a `SKILL.md` (so only genuine skills appear, not stray dirs).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use russh::client::Handle;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::client::{RemoteExec, SshHandler};
use crate::hub::{REMOTE_HUB_CONTENT, shell_quote};
use crate::remote_fs::{RemoteDiscoveryFs, is_skill_entry};
use crate::types::{RemoteSkill, RemoteSkillLayout};

/// A known agent's skills directory on a remote host — the push targets the UI
/// offers. Mirrors the agent `project_skills_rel` paths from the builtin agent
/// table (kept here as a plain constant so the ssh crate doesn't depend on
/// `skillstar-projects`).
///
/// `~` is expanded by the SFTP server; paths are relative to the login $HOME.
pub const KNOWN_AGENT_SKILL_DIRS: &[(&str, &str)] = &[
    ("claude", "~/.claude/skills"),
    ("codex", "~/.codex/skills"),
    ("gemini", "~/.gemini/skills"),
    ("opencode", "~/.opencode/skills"),
    ("cursor", "~/.cursor/skills"),
    ("qoder", "~/.qoder/skills"),
    ("trae", "~/.trae/skills"),
    ("zcode", "~/.zcode/skills"),
    // Generic fallbacks some agents share.
    ("agent", "~/.agent/skills"),
];

/// One detected agent skills directory on the remote host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAgentDir {
    /// Agent id (`claude`, `codex`, …) from [`KNOWN_AGENT_SKILL_DIRS`].
    pub agent: String,
    /// Absolute or `~`-prefixed path that exists on the remote.
    pub path: String,
}

/// An agent discovered by scanning the remote `$HOME`, with the skills found
/// under its `skills/` directory aggregated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAgentSkills {
    /// Agent id derived from the parent dir name (`~/.grok/skills` → `grok`).
    pub agent: String,
    /// Absolute path of the agent's skills directory (`/root/.grok/skills`).
    pub path: String,
    /// Number of skills (dirs containing SKILL.md) under this agent.
    pub count: u32,
}

/// Result of a remote skill discovery scan: the agents found plus every skill
/// (carrying its `agent` so the UI can group/filter).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscoveryResult {
    /// Agents with at least one skill, sorted by agent name.
    pub agents: Vec<RemoteAgentSkills>,
    /// All skills across every agent, sorted by agent then name.
    pub skills: Vec<RemoteSkill>,
    /// Count of skills with `layout = standalone` (candidates for hub migration).
    #[serde(default)]
    pub needs_migration_count: u32,
}

/// Top-level hidden directories under `$HOME` that never hold agent skills and
/// can be slow/large to scan (cache stores, toolchains, secrets).
const SKIP_HOME_DIRS: &[&str] = &[
    ".cache",
    ".npm",
    ".config",
    ".ssh",
    ".local",
    ".docker",
    ".vscode-server",
    ".dotnet",
    ".bun",
    ".cargo",
    ".rustup",
    ".gnupg",
    ".pki",
    ".mozilla",
    ".nvm",
    ".pyenv",
    ".gradle",
    ".m2",
    ".electron-gyp",
    ".node-gyp",
    ".pm2",
    ".pip",
    ".kube",
    ".terraform",
];

/// Whether a top-level `$HOME` entry should be skipped during discovery.
pub(crate) fn should_skip_home_dir(name: &str) -> bool {
    name == "."
        || name == ".."
        || !name.starts_with('.')
        || SKIP_HOME_DIRS.contains(&name)
}

/// Derive agent id from a hidden home dir (`.codex` → `codex`).
pub(crate) fn agent_id_from_home_dir(name: &str) -> Option<String> {
    if !name.starts_with('.') || name.len() <= 1 {
        return None;
    }
    let id = name.trim_start_matches('.').to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Shell script used by [`resolve_skill_layout`] — extracted for unit tests.
pub(crate) fn layout_classify_shell_script(skill_path: &str, skill_name: &str) -> String {
    let hub_content = format!("{REMOTE_HUB_CONTENT}/{skill_name}");
    let path_q = shell_quote(skill_path);
    let hub_q = shell_quote(&hub_content);
    format!(
        r#"if [ -L {path_q} ]; then
  tgt=$(readlink {path_q} 2>/dev/null || true)
  case "$tgt" in
    *"/.skillstar/hub/content/{skill_name}"*|*".skillstar/hub/content/{skill_name}"*)
      if [ -f {hub_q}/SKILL.md ]; then
        echo hub_managed
        exit 0
      fi
      ;;
  esac
fi
echo standalone
"#
    )
}

/// One skill entry (directory or hub symlink) under an agent's `skills/` dir.
#[derive(Debug, Clone)]
pub(crate) struct ScannedSkillEntry {
    pub name: String,
    pub is_skill_entry: bool,
    pub has_skill_md: bool,
    pub size: u64,
    pub modified: Option<String>,
    pub layout: RemoteSkillLayout,
}

/// One `~/.<agent>` directory from a home scan.
#[derive(Debug, Clone)]
pub(crate) struct ScannedAgentEntry {
    pub dir_name: String,
    pub is_dir: bool,
    pub skills: Vec<ScannedSkillEntry>,
}

/// Build a [`DiscoveryResult`] from pre-collected scan data (pure, testable).
pub(crate) fn build_discovery_result(
    home: &str,
    entries: &[ScannedAgentEntry],
    known_fallback: &[RemoteAgentSkills],
) -> DiscoveryResult {
    let mut agents = Vec::new();
    let mut skills = Vec::new();
    let mut needs_migration_count = 0u32;

    for entry in entries {
        if !entry.is_dir || should_skip_home_dir(&entry.dir_name) {
            continue;
        }
        let Some(agent_id) = agent_id_from_home_dir(&entry.dir_name) else {
            continue;
        };
        let skills_dir = format!("{home}/{}/skills", entry.dir_name);
        let mut count = 0u32;
        for skill in &entry.skills {
            if !skill.is_skill_entry || !skill.has_skill_md {
                continue;
            }
            count += 1;
            if skill.layout == RemoteSkillLayout::Standalone {
                needs_migration_count += 1;
            }
            skills.push(RemoteSkill {
                name: skill.name.clone(),
                path: format!("{skills_dir}/{}", skill.name),
                agent: agent_id.clone(),
                size: skill.size,
                modified: skill.modified.clone(),
                layout: skill.layout,
            });
        }
        if count > 0 {
            agents.push(RemoteAgentSkills {
                agent: agent_id,
                path: skills_dir,
                count,
            });
        }
    }

    if agents.is_empty() {
        agents.extend(known_fallback.iter().cloned());
    }

    agents.sort_by(|a, b| a.agent.cmp(&b.agent));
    skills.sort_by(|a, b| a.agent.cmp(&b.agent).then(a.name.cmp(&b.name)));
    DiscoveryResult {
        agents,
        skills,
        needs_migration_count,
    }
}

/// Entry shape for [`filter_remote_skill_list`].
#[derive(Debug, Clone)]
pub(crate) struct ListDirEntry {
    pub name: String,
    pub is_skill_entry: bool,
    pub has_skill_md: bool,
    pub size: u64,
    pub modified: Option<String>,
}

/// Filter a remote `skills/` listing to genuine skills (dirs/symlinks with `SKILL.md`).
pub(crate) fn filter_remote_skill_list(
    remote_dir: &str,
    entries: &[ListDirEntry],
) -> Vec<RemoteSkill> {
    let base = remote_dir.trim_end_matches('/');
    let mut skills = Vec::new();
    for entry in entries {
        if entry.name.starts_with('.') || !entry.is_skill_entry || !entry.has_skill_md {
            continue;
        }
        skills.push(RemoteSkill {
            name: entry.name.clone(),
            path: format!("{base}/{}", entry.name),
            agent: String::new(),
            size: entry.size,
            modified: entry.modified.clone(),
            layout: RemoteSkillLayout::default(),
        });
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Probe whether a skill entry has a readable `SKILL.md` (direct path or hub content for symlinks).
async fn probe_has_skill_md<F: RemoteDiscoveryFs>(
    fs: &F,
    skill_path: &str,
    skill_name: &str,
    attrs: &russh_sftp::protocol::FileAttributes,
) -> bool {
    if fs
        .path_exists(&format!("{skill_path}/SKILL.md"))
        .await
    {
        return true;
    }
    if attrs.is_symlink() {
        return fs
            .path_exists(&format!("{REMOTE_HUB_CONTENT}/{skill_name}/SKILL.md"))
            .await;
    }
    false
}

/// Resolve layout: hub symlinks are inferred locally; standalone dirs use remote exec.
async fn resolve_skill_layout<E: RemoteExec, F: RemoteDiscoveryFs>(
    exec: &mut E,
    fs: &F,
    skill_path: &str,
    skill_name: &str,
    attrs: &russh_sftp::protocol::FileAttributes,
    has_skill_md: bool,
) -> RemoteSkillLayout {
    if !has_skill_md {
        return RemoteSkillLayout::Standalone;
    }
    if attrs.is_symlink()
        && fs
            .path_exists(&format!("{REMOTE_HUB_CONTENT}/{skill_name}/SKILL.md"))
            .await
    {
        return RemoteSkillLayout::HubManaged;
    }
    let script = layout_classify_shell_script(skill_path, skill_name);
    match exec.exec_script(&script).await {
        Ok(out) if out.trim() == "hub_managed" => RemoteSkillLayout::HubManaged,
        _ => RemoteSkillLayout::Standalone,
    }
}

/// Discover all agent skills on the remote host by scanning `$HOME/.*` for
/// `<dir>/skills/<name>/SKILL.md` layouts.
///
/// This is **discovery-based**, not a fixed-path lookup: any agent whose
/// `~/.<agent>/skills/` holds `SKILL.md`-bearing subdirs is reported (grok,
/// agents, claude, codex, … — known or not). [`KNOWN_AGENT_SKILL_DIRS`] is only
/// used as a fallback seed when the scan finds nothing (fresh server).
pub async fn discover_remote_skills<E, F>(exec: &mut E, fs: &F) -> Result<DiscoveryResult>
where
    E: RemoteExec,
    F: RemoteDiscoveryFs,
{
    let home = fs.canonicalize_home().await;
    let top = fs.read_dir(&home).await;

    let mut scan = Vec::new();
    for (name, attrs) in top {
        if should_skip_home_dir(&name) || !attrs.is_dir() {
            continue;
        }
        let skills_dir = format!("{home}/{name}/skills");
        let sub = fs.read_dir(&skills_dir).await;
        let mut skills = Vec::new();
        for (skill_name, skill_attrs) in sub {
            if skill_name.is_empty() || !is_skill_entry(&skill_attrs) {
                continue;
            }
            let skill_path = format!("{skills_dir}/{skill_name}");
            let has_skill_md =
                probe_has_skill_md(fs, &skill_path, &skill_name, &skill_attrs).await;
            let layout = resolve_skill_layout(
                exec,
                fs,
                &skill_path,
                &skill_name,
                &skill_attrs,
                has_skill_md,
            )
            .await;
            skills.push(ScannedSkillEntry {
                name: skill_name,
                is_skill_entry: true,
                has_skill_md,
                size: skill_attrs.size.unwrap_or(0),
                modified: skill_attrs
                    .mtime
                    .and_then(|t| chrono_like_rfc3339(t as i64)),
                layout,
            });
        }
        scan.push(ScannedAgentEntry {
            dir_name: name,
            is_dir: true,
            skills,
        });
    }

    let mut known_fallback = Vec::new();
    for (agent, path) in KNOWN_AGENT_SKILL_DIRS {
        if fs.path_exists(path).await {
            known_fallback.push(RemoteAgentSkills {
                agent: (*agent).to_string(),
                path: (*path).to_string(),
                count: 0,
            });
        }
    }

    Ok(build_discovery_result(&home, &scan, &known_fallback))
}

/// Aggregate result of a successful push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResult {
    /// Number of files written.
    pub files_uploaded: u32,
    /// Total bytes transferred.
    pub bytes: u64,
    /// Absolute remote path the skill now lives at.
    pub remote_path: String,
}

/// Open an SFTP subsystem session on an authenticated SSH handle.
pub async fn open_sftp(
    handle: &mut Handle<SshHandler>,
    session_id: &str,
    sink: &impl crate::progress::ProgressSink,
) -> Result<SftpSession> {
    sink.emit(crate::progress::event(
        session_id,
        crate::progress::Phase::Sftp,
        crate::progress::Status::Start,
        "opening SFTP subsystem…",
    ));
    let channel = handle
        .channel_open_session()
        .await
        .context("open SFTP session channel")?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .context("request sftp subsystem")?;
    let session = SftpSession::new(channel.into_stream())
        .await
        .context("initialise SFTP session")?;
    sink.emit(crate::progress::event(
        session_id,
        crate::progress::Phase::Sftp,
        crate::progress::Status::Ok,
        "SFTP ready",
    ));
    Ok(session)
}

// ── local skill resolution (mirrors commands/skill_content resolve logic) ──

/// Resolve a hub skill name to its real on-disk content directory.
///
/// Hub skills are symlinks (`skills/<name>` → `local/<name>` or a repo
/// checkout). We follow the link, then accept the first of: the dir itself if
/// it has `SKILL.md`, a nested `<name>/SKILL.md`, otherwise the resolved dir.
fn resolve_local_skill_dir(skill_name: &str) -> Result<PathBuf> {
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(skill_name);
    if !skill_dir.exists() {
        anyhow::bail!("skill '{}' is not installed in the hub", skill_name);
    }
    let effective = if skillstar_core::infra::fs_ops::is_link(&skill_dir) {
        skillstar_core::infra::fs_ops::read_link_resolved(&skill_dir)
            .unwrap_or_else(|_| skill_dir.clone())
    } else {
        skill_dir.clone()
    };

    if effective.join("SKILL.md").exists() {
        return Ok(effective);
    }
    // Nested layout (repo with subfolders): look one level deep.
    let nested = effective.join(skill_name);
    if nested.join("SKILL.md").exists() {
        return Ok(nested);
    }
    Ok(effective)
}

/// Recursively collect `(relative_path, absolute_path)` file pairs, skipping
/// `.git` (same rule as `list_skill_files`).
fn collect_local_files(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            out.push((rel_str, path));
        }
    }
    Ok(())
}

// ── remote path helpers ─────────────────────────────────────────────

/// Join a remote base dir (possibly containing `~`) with a skill name.
/// SFTP servers expand `~` themselves, so we keep it as-is and just join.
fn remote_skill_dir(remote_base: &str, skill_name: &str) -> String {
    let trimmed = remote_base.trim_end_matches('/');
    if trimmed.is_empty() {
        skill_name.to_string()
    } else {
        format!("{trimmed}/{skill_name}")
    }
}

/// Split a posix remote path into its parent components, so we can mkdir -p.
///
/// Handles three SFTP path shapes:
/// - `~/.claude/skills`  → `["~", "~/.claude", "~/.claude/skills"]`
/// - `/home/u/skills`    → `["/home", "/home/u", "/home/u/skills"]`
/// - `relative/skills`   → `["relative", "relative/skills"]`
fn remote_parent_dirs(remote_path: &str) -> Vec<String> {
    let absolute = remote_path.starts_with('/');
    let mut dirs = Vec::new();
    let mut acc = String::new();
    for part in remote_path.split('/') {
        if part.is_empty() {
            continue;
        }
        if acc.is_empty() {
            acc = if absolute { format!("/{part}") } else { part.to_string() };
        } else {
            acc.push('/');
            acc.push_str(part);
        }
        dirs.push(acc.clone());
    }
    dirs
}

/// `mkdir -p` over SFTP — ignore "already exists" failures.
pub async fn ensure_remote_dir_pub(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    ensure_remote_dir(sftp, remote_path).await
}

async fn ensure_remote_dir(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    for dir in remote_parent_dirs(remote_path) {
        match sftp.create_dir(&dir).await {
            Ok(()) => {}
            // SFTP returns Failure for existing dirs — treat as success.
            Err(_) => {}
        }
    }
    Ok(())
}

// ── public operations ───────────────────────────────────────────────

/// Read an entire remote file into memory using SFTP.
///
/// Opens the file for reading and drains it via `AsyncReadExt::read_to_end`.
/// Returns the raw bytes. The caller is responsible for interpreting the
/// content (e.g. UTF-8 text for SKILL.md).
pub async fn read_remote_file(sftp: &SftpSession, remote_path: &str) -> Result<Vec<u8>> {
    let mut file = sftp
        .open(remote_path)
        .await
        .with_context(|| format!("open remote {} for read", remote_path))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .await
        .with_context(|| format!("read remote {}", remote_path))?;
    Ok(buf)
}

/// Write bytes to a remote file using an atomic temp→rename pattern.
///
/// The file is first written to `{remote_path}.skillstar.tmp`, then renamed
/// over the target. This mirrors the pattern used by `upload_local_skill_tree`
/// so that interrupted writes never leave a partially-written final file.
pub async fn write_remote_file(sftp: &SftpSession, remote_path: &str, bytes: &[u8]) -> Result<()> {
    let parent = remote_path
        .rsplit_once('/')
        .map(|(p, _)| p)
        .unwrap_or(".");
    ensure_remote_dir(sftp, parent).await?;

    let tmp_path = format!("{}.skillstar.tmp", remote_path);
    let mut file = sftp
        .open_with_flags(&tmp_path, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE)
        .await
        .with_context(|| format!("open remote {} for write", tmp_path))?;
    file.write_all(bytes)
        .await
        .with_context(|| format!("write remote {}", tmp_path))?;
    file.flush().await.ok();
    drop(file);

    let _ = sftp.remove_file(remote_path).await;
    sftp.rename(&tmp_path, remote_path)
        .await
        .with_context(|| format!("rename {} -> {}", tmp_path, remote_path))?;

    Ok(())
}

/// Upload a local hub skill tree to `remote_content_dir` (no agent link).
pub async fn upload_local_skill_tree(
    sftp: &SftpSession,
    skill_name: &str,
    remote_content_dir: &str,
) -> Result<(u32, u64)> {
    let local_dir = resolve_local_skill_dir(skill_name)?;
    let files = collect_local_files(&local_dir)?;
    ensure_remote_dir(sftp, remote_content_dir).await?;

    let mut files_uploaded = 0u32;
    let mut bytes = 0u64;

    for (rel, abs) in &files {
        let remote_file = format!("{}/{rel}", remote_content_dir.trim_end_matches('/'));
        if let Some((file_parent, _)) = remote_file.rsplit_once('/') {
            ensure_remote_dir(sftp, file_parent).await?;
        }

        let local_bytes = std::fs::read(abs).with_context(|| format!("read {}", abs.display()))?;
        bytes += local_bytes.len() as u64;

        let tmp_path = format!("{remote_file}.skillstar.tmp");
        let mut file = sftp
            .open_with_flags(&tmp_path, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE)
            .await
            .with_context(|| format!("open remote {tmp_path}"))?;
        file.write_all(&local_bytes)
            .await
            .with_context(|| format!("write remote {tmp_path}"))?;
        file.flush().await.ok();
        drop(file);

        let _ = sftp.remove_file(&remote_file).await;
        sftp.rename(&tmp_path, &remote_file)
            .await
            .with_context(|| format!("rename {tmp_path} -> {remote_file}"))?;

        files_uploaded += 1;
    }

    Ok((files_uploaded, bytes))
}

/// Push one locally-installed skill to `remote_base` on the connected host.
///
/// Files are written to a `.skillstar.tmp` sibling first and then renamed, so
/// an interrupted push never leaves partially-written files at the final path.
pub async fn push_skill(
    sftp: &SftpSession,
    skill_name: &str,
    remote_base: &str,
) -> Result<PushResult> {
    let remote_target = remote_skill_dir(remote_base, skill_name);
    let parent = remote_target
        .rsplit_once('/')
        .map(|(p, _)| p)
        .unwrap_or(".");
    ensure_remote_dir(sftp, parent).await?;
    ensure_remote_dir(sftp, &remote_target).await?;

    let (files_uploaded, bytes) = upload_local_skill_tree(sftp, skill_name, &remote_target).await?;

    tracing::info!(
        target: "ssh",
        skill = skill_name,
        remote = %remote_target,
        files = files_uploaded,
        bytes,
        "skill pushed to remote"
    );

    Ok(PushResult {
        files_uploaded,
        bytes,
        remote_path: remote_target,
    })
}

/// List skills under a remote directory. A subdirectory counts as a skill iff
/// it contains a `SKILL.md`.
pub async fn list_remote_skills<F: RemoteDiscoveryFs>(
    fs: &F,
    remote_dir: &str,
) -> Result<Vec<RemoteSkill>> {
    let entries = fs.read_dir(remote_dir).await;
    let base = remote_dir.trim_end_matches('/');

    let mut list_entries = Vec::new();
    for (name, attrs) in entries {
        let is_skill_entry = is_skill_entry(&attrs);
        let has_skill_md = if is_skill_entry && !name.starts_with('.') {
            let skill_path = format!("{base}/{name}");
            probe_has_skill_md(fs, &skill_path, &name, &attrs).await
        } else {
            false
        };
        list_entries.push(ListDirEntry {
            name,
            is_skill_entry,
            has_skill_md,
            size: attrs.size.unwrap_or(0),
            modified: attrs.mtime.and_then(|t| chrono_like_rfc3339(t as i64)),
        });
    }
    Ok(filter_remote_skill_list(remote_dir, &list_entries))
}

/// Best-effort RFC3339 formatting of a Unix timestamp.
fn chrono_like_rfc3339(secs: i64) -> Option<String> {
    // Avoid pulling chrono into this crate for one call; format manually.
    // Sufficient for display; not used for ordering.
    let days = secs.div_euclid(86_400);
    let _rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    Some(format!("{year:04}-{m:02}-{d:02}"))
}

/// Delete a remote skill directory (recursive). Removes files first, then
/// the (now empty) directory.
pub async fn delete_remote_skill(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    // Recursively remove children.
    remove_remote_tree(sftp, remote_path).await?;
    match sftp.remove_dir(remote_path).await {
        Ok(()) => Ok(()),
        Err(_) => Ok(()), // already gone — idempotent
    }
}

async fn remove_remote_tree(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    let entries = match sftp.read_dir(remote_path).await {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let name = entry.file_name();
        let attrs = entry.metadata();
        let child = format!("{}/{name}", remote_path.trim_end_matches('/'));
        if attrs.is_dir() {
            Box::pin(remove_remote_tree(sftp, &child)).await?;
            let _ = sftp.remove_dir(&child).await;
        } else {
            let _ = sftp.remove_file(&child).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_skill_dir_joins_without_double_slash() {
        assert_eq!(remote_skill_dir("~/.claude/skills", "foo"), "~/.claude/skills/foo");
        assert_eq!(remote_skill_dir("~/.claude/skills/", "foo"), "~/.claude/skills/foo");
        assert_eq!(remote_skill_dir("", "foo"), "foo");
    }

    #[test]
    fn remote_parent_dirs_splits_components() {
        // Tilde-prefixed: `~` is preserved so the SFTP server expands it.
        let dirs = remote_parent_dirs("~/.claude/skills");
        assert_eq!(dirs, vec!["~", "~/.claude", "~/.claude/skills"]);
    }

    #[test]
    fn remote_parent_dirs_absolute_path() {
        let dirs = remote_parent_dirs("/home/u/skills");
        assert_eq!(dirs, vec!["/home", "/home/u", "/home/u/skills"]);
    }

    #[test]
    fn collect_local_files_skips_git() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("SKILL.md"), "x").unwrap();
        std::fs::create_dir_all(root.join(".git/refs")).unwrap();
        std::fs::write(root.join(".git/refs/head"), "x").unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts/run.sh"), "x").unwrap();

        let files = collect_local_files(root).unwrap();
        let rels: Vec<_> = files.into_iter().map(|(r, _)| r).collect();
        assert!(rels.contains(&"SKILL.md".to_string()));
        assert!(rels.contains(&"scripts/run.sh".to_string()));
        assert!(!rels.iter().any(|r| r.contains(".git")));
    }

    #[test]
    fn resolve_local_skill_dir_errors_when_missing() {
        let _guard = test_data_dir();
        let err = resolve_local_skill_dir("definitely_not_installed").unwrap_err();
        assert!(err.to_string().contains("not installed"));
    }

    fn chrono_like_rfc3339_format_has_date_shape() {
        let s = chrono_like_rfc3339(1_700_000_000).unwrap();
        assert_eq!(s.len(), 10); // YYYY-MM-DD
        assert!(s.starts_with("20"));
    }

    struct DataDirGuard {
        _temp: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("SKILLSTAR_DATA_DIR");
            }
        }
    }
    fn test_data_dir() -> DataDirGuard {
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        // SAFETY: the env lock above serialises all such mutations.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        DataDirGuard {
            _temp: temp,
            _lock,
        }
    }

    #[test]
    fn rfc3339_helper_formats_date() {
        chrono_like_rfc3339_format_has_date_shape();
    }

    #[test]
    fn should_skip_home_dir_filters_blacklist_and_non_hidden() {
        assert!(should_skip_home_dir(".cache"));
        assert!(should_skip_home_dir(".ssh"));
        assert!(should_skip_home_dir(".pm2"));
        assert!(should_skip_home_dir("Documents"));
        assert!(should_skip_home_dir("."));
        assert!(!should_skip_home_dir(".codex"));
        assert!(!should_skip_home_dir(".grok"));
    }

    #[test]
    fn agent_id_from_home_dir_strips_leading_dot() {
        assert_eq!(agent_id_from_home_dir(".codex").as_deref(), Some("codex"));
        assert_eq!(agent_id_from_home_dir(".grok").as_deref(), Some("grok"));
        assert!(agent_id_from_home_dir("codex").is_none());
        assert!(agent_id_from_home_dir(".").is_none());
    }

    #[test]
    fn layout_classify_script_targets_hub_content() {
        let script = layout_classify_shell_script(
            "/root/.codex/skills/my-skill",
            "my-skill",
        );
        assert!(script.contains("hub/content/my-skill"));
        assert!(script.contains("hub_managed"));
        assert!(script.contains("readlink"));
    }

    /// vps-yy-style VPS layout: codex hub-managed + grok standalone + cache skipped.
    #[test]
    fn build_discovery_result_vps_yy_layout() {
        let home = "/root";
        let scan = vec![
            ScannedAgentEntry {
                dir_name: ".cache".into(),
                is_dir: true,
                skills: vec![ScannedSkillEntry {
                    name: "junk".into(),
                    is_skill_entry: true,
                    has_skill_md: true,
                    size: 0,
                    modified: None,
                    layout: RemoteSkillLayout::Standalone,
                }],
            },
            ScannedAgentEntry {
                dir_name: ".codex".into(),
                is_dir: true,
                skills: vec![
                    ScannedSkillEntry {
                        name: "real-skill".into(),
                        is_skill_entry: true,
                        has_skill_md: true,
                        size: 1024,
                        modified: None,
                        layout: RemoteSkillLayout::HubManaged,
                    },
                    ScannedSkillEntry {
                        name: "not-a-skill".into(),
                        is_skill_entry: true,
                        has_skill_md: false,
                        size: 0,
                        modified: None,
                        layout: RemoteSkillLayout::Standalone,
                    },
                ],
            },
            ScannedAgentEntry {
                dir_name: ".grok".into(),
                is_dir: true,
                skills: vec![ScannedSkillEntry {
                    name: "standalone-one".into(),
                    is_skill_entry: true,
                    has_skill_md: true,
                    size: 512,
                    modified: None,
                    layout: RemoteSkillLayout::Standalone,
                }],
            },
            ScannedAgentEntry {
                dir_name: ".npm".into(),
                is_dir: true,
                skills: vec![],
            },
        ];

        let result = build_discovery_result(home, &scan, &[]);
        assert_eq!(result.agents.len(), 2);
        assert_eq!(result.skills.len(), 2);
        assert_eq!(result.needs_migration_count, 1);

        let codex = result.agents.iter().find(|a| a.agent == "codex").unwrap();
        assert_eq!(codex.count, 1);
        assert_eq!(codex.path, "/root/.codex/skills");

        let grok = result.agents.iter().find(|a| a.agent == "grok").unwrap();
        assert_eq!(grok.count, 1);

        let hub_skill = result
            .skills
            .iter()
            .find(|s| s.name == "real-skill")
            .unwrap();
        assert_eq!(hub_skill.agent, "codex");
        assert_eq!(hub_skill.layout, RemoteSkillLayout::HubManaged);
        assert_eq!(hub_skill.path, "/root/.codex/skills/real-skill");

        let standalone = result
            .skills
            .iter()
            .find(|s| s.name == "standalone-one")
            .unwrap();
        assert_eq!(standalone.agent, "grok");
        assert_eq!(standalone.layout, RemoteSkillLayout::Standalone);
    }

    #[test]
    fn build_discovery_result_seeds_known_dirs_when_scan_empty() {
        let fallback = vec![RemoteAgentSkills {
            agent: "claude".into(),
            path: "~/.claude/skills".into(),
            count: 0,
        }];
        let result = build_discovery_result("/root", &[], &fallback);
        assert_eq!(result.agents.len(), 1);
        assert_eq!(result.agents[0].agent, "claude");
        assert!(result.skills.is_empty());
    }

    #[test]
    fn filter_remote_skill_list_keeps_only_skill_md_dirs() {
        let entries = vec![
            ListDirEntry {
                name: "good-skill".into(),
                is_skill_entry: true,
                has_skill_md: true,
                size: 100,
                modified: None,
            },
            ListDirEntry {
                name: "hub-link".into(),
                is_skill_entry: true,
                has_skill_md: true,
                size: 0,
                modified: None,
            },
            ListDirEntry {
                name: "empty-dir".into(),
                is_skill_entry: true,
                has_skill_md: false,
                size: 0,
                modified: None,
            },
            ListDirEntry {
                name: ".hidden".into(),
                is_skill_entry: true,
                has_skill_md: true,
                size: 0,
                modified: None,
            },
            ListDirEntry {
                name: "readme.md".into(),
                is_skill_entry: false,
                has_skill_md: false,
                size: 10,
                modified: None,
            },
        ];
        let skills = filter_remote_skill_list("~/.codex/skills", &entries);
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "good-skill");
        assert_eq!(skills[1].name, "hub-link");
    }

    use crate::remote_fs::{MockRemoteExec, MockRemoteFs};

    /// Drives the real `discover_remote_skills` entry point on a vps-yy mock tree.
    #[tokio::test]
    async fn discover_remote_skills_vps_yy_mock_fs() {
        let mut exec = MockRemoteExec::default();
        let fs = MockRemoteFs::vps_yy_layout();
        let result = discover_remote_skills(&mut exec, &fs).await.unwrap();

        assert_eq!(result.agents.len(), 2);
        assert_eq!(result.skills.len(), 2);
        assert_eq!(result.needs_migration_count, 1);

        let hub = result.skills.iter().find(|s| s.name == "hub-skill").unwrap();
        assert_eq!(hub.agent, "codex");
        assert_eq!(hub.layout, RemoteSkillLayout::HubManaged);

        let standalone = result
            .skills
            .iter()
            .find(|s| s.name == "standalone-one")
            .unwrap();
        assert_eq!(standalone.agent, "grok");
        assert_eq!(standalone.layout, RemoteSkillLayout::Standalone);
    }

    /// Drives the real `list_remote_skills` entry point including hub symlinks.
    #[tokio::test]
    async fn list_remote_skills_includes_hub_symlinks() {
        let fs = MockRemoteFs::vps_yy_layout();
        let skills = list_remote_skills(&fs, "/root/.codex/skills")
            .await
            .unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "hub-skill");
    }
}
