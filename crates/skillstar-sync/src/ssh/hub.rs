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

use crate::ssh::client::{SshHandler, ensure_exec_ok, exec_capture_status};
use crate::ssh::hub_scripts::{self, expand_remote_home, hub_skill_abs, validate_skill_name};
use crate::ssh::sftp::{PushResult, upload_local_skill_tree};

/// Legacy `~`-literal hub root (kept for discovery back-compat probing).
pub const REMOTE_HUB_CONTENT: &str = hub_scripts::LEGACY_HUB_PREFIX;

/// Re-export so existing callers (`sftp::list`) keep one quoting helper.
pub use crate::ssh::hub_scripts::shell_quote;

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
