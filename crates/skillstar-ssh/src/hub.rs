//! Push skills to the remote host using the same layout as local SkillStar:
//! `$HOME/.skillstar/hub/content/<name>` holds the files; the agent dir gets a
//! symlink `~/.<agent>/skills/<name>` → hub content (POSIX `ln -sfn`).
//!
//! Path discipline (the stability contract of this module):
//! - Shell scripts reference the remote home as `"$HOME"` (expanded remotely) —
//!   never a quoted `~`, which the shell would take literally.
//! - SFTP paths are made absolute with the home dir resolved once per session
//!   via `canonicalize(".")` — SFTP servers don't expand `~` either.
//! - Every mutating script runs under `set -e` and its **exit status is
//!   checked** ([`ensure_exec_ok`]); a failed remote `mv`/`ln`/`git` surfaces
//!   as an error instead of a silent no-op.

use anyhow::Result;
use russh::client::Handle;
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};

use crate::client::{SshHandler, ensure_exec_ok, exec_capture, exec_capture_status};
use crate::hub_scripts::{
    self, expand_remote_home, hub_skill_abs, validate_skill_name,
};
use crate::sftp::{PushResult, read_remote_file, upload_local_skill_tree, write_remote_file};
use crate::types::{RemoteSkillContent, RemoteSkillUpdateState};

/// Legacy `~`-literal hub root (kept for discovery back-compat probing).
pub const REMOTE_HUB_CONTENT: &str = hub_scripts::LEGACY_HUB_PREFIX;

/// Re-export so existing callers (`sftp::list`) keep one quoting helper.
pub use crate::hub_scripts::shell_quote;

/// Resolve the remote `$HOME` for SFTP path building. Fails loudly when the
/// server can't canonicalize (an unusable relative home would silently create
/// wrong layouts — exactly the bug class this module guards against).
pub(crate) async fn resolve_sftp_home(sftp: &SftpSession) -> Result<String> {
    let home = sftp
        .canonicalize(".")
        .await
        .map_err(|e| anyhow::anyhow!("could not resolve remote home dir: {e}"))?;
    if !home.starts_with('/') {
        anyhow::bail!("remote home dir is not absolute: {home:?}");
    }
    Ok(home)
}

/// Push one hub skill: mirror content under `$HOME/.skillstar/hub/content/<name>`
/// (SFTP, atomic tmp→rename per file), then symlink into `agent_skills_dir`.
///
/// The caller opens the SFTP channel and passes it in — batch pushes reuse one
/// channel for every skill instead of opening one per skill (OpenSSH caps
/// concurrent channels at `MaxSessions`, default 10, so per-skill channels
/// broke batches of ten or more).
pub async fn push_skill_via_hub(
    handle: &mut Handle<SshHandler>,
    sftp: &SftpSession,
    skill_name: &str,
    agent_skills_dir: &str,
) -> Result<PushResult> {
    validate_skill_name(skill_name)?;
    let home = resolve_sftp_home(sftp).await?;
    let remote_content = hub_skill_abs(&home, skill_name);
    let (files_uploaded, bytes) =
        upload_local_skill_tree(sftp, skill_name, &remote_content).await?;

    let script = hub_scripts::link_skill_script(agent_skills_dir, skill_name);
    let (out, code) = exec_capture_status(handle, &script).await?;
    ensure_exec_ok("create agent symlink", &out, code)?;

    let agent_base = agent_skills_dir.trim_end_matches('/');
    let remote_link = expand_remote_home(&home, &format!("{agent_base}/{skill_name}"));

    tracing::info!(
        target: "ssh",
        skill = skill_name,
        remote = %remote_link,
        hub = %remote_content,
        files = files_uploaded,
        bytes,
        "skill pushed via remote hub layout"
    );

    Ok(PushResult {
        files_uploaded,
        bytes,
        remote_path: remote_link,
    })
}

/// Result of migrating one standalone remote skill into the hub layout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrateResult {
    /// Symlink path under the agent skills directory.
    pub remote_path: String,
    /// Canonical hub content directory for the skill files.
    pub hub_content_path: String,
}

/// Move a standalone agent-dir skill into `$HOME/.skillstar/hub/content/<name>`
/// and replace the agent entry with a symlink (same layout as local SkillStar).
pub async fn migrate_remote_skill_to_hub(
    handle: &mut Handle<SshHandler>,
    skill_name: &str,
    agent_skills_dir: &str,
    standalone_path: &str,
) -> Result<MigrateResult> {
    validate_skill_name(skill_name)?;
    let script = hub_scripts::migrate_script(skill_name, agent_skills_dir, standalone_path);
    let (out, code) = exec_capture_status(handle, &script).await?;
    if out.contains("HUB_EXISTS") {
        anyhow::bail!("hub content already exists for skill '{skill_name}'");
    }
    if out.contains("MISSING_STANDALONE") {
        anyhow::bail!("standalone path missing: {standalone_path}");
    }
    ensure_exec_ok("migrate skill to hub", &out, code)?;

    let agent_base = agent_skills_dir.trim_end_matches('/');
    let remote_link = format!("{agent_base}/{skill_name}");
    let remote_content = format!("{}/{skill_name}", hub_scripts::REMOTE_HUB_REL);

    tracing::info!(
        target: "ssh",
        skill = skill_name,
        remote = %remote_link,
        hub = %remote_content,
        "skill migrated to remote hub layout"
    );

    Ok(MigrateResult {
        remote_path: remote_link,
        hub_content_path: remote_content,
    })
}

/// Read the raw SKILL.md content for a hub-managed remote skill.
///
/// Reads `$HOME/.skillstar/hub/content/<name>/SKILL.md`; falls back to the
/// legacy literal-`~` location for hosts that haven't been healed yet.
pub async fn read_remote_skill_content(
    handle: &mut Handle<SshHandler>,
    sftp: &SftpSession,
    skill_name: &str,
) -> Result<RemoteSkillContent> {
    validate_skill_name(skill_name)?;
    let home = resolve_sftp_home(sftp).await?;
    let remote_path = format!("{}/SKILL.md", hub_skill_abs(&home, skill_name));
    let legacy_path = format!("{REMOTE_HUB_CONTENT}/{skill_name}/SKILL.md");
    let bytes = match read_remote_file(sftp, &remote_path).await {
        Ok(b) => b,
        Err(primary_err) => read_remote_file(sftp, &legacy_path)
            .await
            .map_err(|_| primary_err)?,
    };
    let content = String::from_utf8_lossy(&bytes).into_owned();

    // Best-effort mtime via `stat -c %Y` (GNU; BSD servers just yield None).
    let mtime = exec_capture(handle, &hub_scripts::stat_skill_md_mtime_script(skill_name))
        .await
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .and_then(chrono_like_rfc3339);

    Ok(RemoteSkillContent {
        name: skill_name.to_string(),
        content,
        modified: mtime,
    })
}

/// Write raw text to `$HOME/.skillstar/hub/content/<name>/SKILL.md` atomically.
pub async fn write_remote_skill_content(
    sftp: &SftpSession,
    skill_name: &str,
    content: &str,
) -> Result<()> {
    validate_skill_name(skill_name)?;
    let home = resolve_sftp_home(sftp).await?;
    let remote_path = format!("{}/SKILL.md", hub_skill_abs(&home, skill_name));
    write_remote_file(sftp, &remote_path, content.as_bytes()).await?;
    Ok(())
}

/// Pull updates for a hub-managed remote skill via git (`pull --ff-only`).
///
/// A failed pull (diverged history, network, auth) now errors instead of being
/// silently swallowed — the script's exit status is checked.
pub async fn pull_remote_skill(
    handle: &mut Handle<SshHandler>,
    skill_name: &str,
) -> Result<()> {
    validate_skill_name(skill_name)?;
    let script = hub_scripts::pull_script(skill_name);
    let (out, code) = exec_capture_status(handle, &script).await?;
    if out.contains("NOT_A_GIT_REPO") {
        anyhow::bail!("remote skill '{skill_name}' is not a git repo under hub");
    }
    ensure_exec_ok("git pull", &out, code)?;
    Ok(())
}

/// Toggle (create/remove) the agent symlink for a hub-managed skill.
pub async fn toggle_remote_agent_link(
    handle: &mut Handle<SshHandler>,
    skill_name: &str,
    agent_skills_dir: &str,
    enable: bool,
) -> Result<()> {
    validate_skill_name(skill_name)?;
    let script = if enable {
        hub_scripts::link_skill_script(agent_skills_dir, skill_name)
    } else {
        hub_scripts::unlink_skill_script(agent_skills_dir, skill_name)
    };
    let (out, code) = exec_capture_status(handle, &script).await?;
    ensure_exec_ok(
        if enable { "create agent symlink" } else { "remove agent symlink" },
        &out,
        code,
    )?;
    Ok(())
}

/// Install a skill from a git URL directly onto the remote host.
///
/// Clones into `$HOME/.skillstar/hub/content/<name>` (if not present) and
/// creates the agent symlink. `set -e` in the script guarantees a failed clone
/// aborts before the symlink exists — no more dangling links on clone errors.
pub async fn install_remote_skill(
    handle: &mut Handle<SshHandler>,
    url: &str,
    skill_name: &str,
    agent_skills_dir: &str,
) -> Result<()> {
    validate_skill_name(skill_name)?;
    let script = hub_scripts::install_script(url, skill_name, agent_skills_dir);
    let (out, code) = exec_capture_status(handle, &script).await?;
    ensure_exec_ok("install remote skill (git clone + link)", &out, code)?;
    Ok(())
}

/// Check update availability for all hub-managed skills on this host.
///
/// Lists hub content dirs bearing a SKILL.md, then per skill runs a
/// prompt-free `git fetch` + `rev-list --count HEAD..@{u}`.
pub async fn check_remote_skill_updates(
    handle: &mut Handle<SshHandler>,
) -> Result<Vec<RemoteSkillUpdateState>> {
    let names_out = exec_capture(handle, &hub_scripts::list_hub_skills_script()).await?;
    let names: Vec<String> = names_out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && validate_skill_name(l).is_ok())
        .collect();

    let mut out = Vec::new();
    for name in names {
        let script = hub_scripts::update_check_script(&name);
        let cnt_str = exec_capture(handle, &script)
            .await
            .unwrap_or_else(|_| "0".to_string());
        let cnt: u32 = cnt_str.trim().parse().unwrap_or(0);
        out.push(RemoteSkillUpdateState {
            name,
            update_available: cnt > 0,
        });
    }
    Ok(out)
}

/// Best-effort RFC3339 date from epoch seconds (duplicate of sftp helper; local copy to keep hub self-contained).
fn chrono_like_rfc3339(secs: i64) -> Option<String> {
    let days = secs.div_euclid(86_400);
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
