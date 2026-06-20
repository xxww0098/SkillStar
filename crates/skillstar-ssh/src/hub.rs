//! Push skills to the remote host using the same layout as local SkillStar:
//! `~/.skillstar/hub/content/<name>` holds the files; the agent dir gets a
//! symlink `~/.<agent>/skills/<name>` → hub content (POSIX `ln -sfn`).

use anyhow::Result;
use russh::client::Handle;
use serde::{Deserialize, Serialize};

use crate::client::{SshHandler, exec_capture};
use crate::sftp::{PushResult, open_sftp, upload_local_skill_tree};

pub const REMOTE_HUB_CONTENT: &str = "~/.skillstar/hub/content";

/// Push one hub skill: mirror content under `~/.skillstar/hub/content/<name>`,
/// then symlink into `agent_skills_dir/<name>`.
pub async fn push_skill_via_hub(
    handle: &mut Handle<SshHandler>,
    session_id: &str,
    sink: &impl crate::progress::ProgressSink,
    skill_name: &str,
    agent_skills_dir: &str,
) -> Result<PushResult> {
    let sftp = open_sftp(handle, session_id, sink).await?;
    let remote_content = format!("{REMOTE_HUB_CONTENT}/{skill_name}");
    let (files_uploaded, bytes) =
        upload_local_skill_tree(&sftp, skill_name, &remote_content).await?;

    let agent_base = agent_skills_dir.trim_end_matches('/');
    let remote_link = format!("{agent_base}/{skill_name}");
    let target = format!("{REMOTE_HUB_CONTENT}/{skill_name}");

    // Ensure agent skills parent exists (SFTP mkdir -p).
    crate::sftp::ensure_remote_dir_pub(&sftp, agent_base).await?;

    let link_q = shell_quote(&remote_link);
    let target_q = shell_quote(&target);
    let script = format!("ln -sfn {target_q} {link_q}");
    let out = exec_capture(handle, &script).await?;
    if out.to_lowercase().contains("error") && !out.trim().is_empty() {
        tracing::warn!(target: "ssh", %out, "ln -sfn stderr/stdout");
    }

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

/// Move a standalone agent-dir skill into `~/.skillstar/hub/content/<name>` and
/// replace the agent entry with a symlink (same layout as local SkillStar).
pub async fn migrate_remote_skill_to_hub(
    handle: &mut Handle<SshHandler>,
    skill_name: &str,
    agent_skills_dir: &str,
    standalone_path: &str,
) -> Result<MigrateResult> {
    let agent_base = agent_skills_dir.trim_end_matches('/');
    let remote_link = format!("{agent_base}/{skill_name}");
    let remote_content = format!("{REMOTE_HUB_CONTENT}/{skill_name}");

    let link_q = shell_quote(&remote_link);
    let content_q = shell_quote(&remote_content);
    let standalone_q = shell_quote(standalone_path);
    let hub_parent_q = shell_quote(REMOTE_HUB_CONTENT);

    let script = format!(
        r#"set -e
mkdir -p {hub_parent_q}
if [ -e {content_q} ]; then
  echo "HUB_EXISTS"
  exit 1
fi
if [ -L {standalone_q} ]; then
  rm -f {standalone_q}
elif [ -d {standalone_q} ]; then
  mv {standalone_q} {content_q}
else
  echo "MISSING_STANDALONE"
  exit 1
fi
ln -sfn {content_q} {link_q}
echo OK
"#
    );
    let out = exec_capture(handle, &script).await?;
    if out.contains("HUB_EXISTS") {
        anyhow::bail!("hub content already exists for skill '{skill_name}'");
    }
    if out.contains("MISSING_STANDALONE") {
        anyhow::bail!("standalone path missing: {standalone_path}");
    }

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

/// Shell-safe single-quoted string (exported for SFTP discovery helpers).
pub fn shell_quote(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("a"), "'a'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}