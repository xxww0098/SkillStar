//! Push skills to the remote host using the same layout as local SkillStar:
//! `~/.skillstar/hub/content/<name>` holds the files; the agent dir gets a
//! symlink `~/.<agent>/skills/<name>` → hub content (POSIX `ln -sfn`).

use anyhow::Result;
use russh::client::Handle;
use serde::{Deserialize, Serialize};

use crate::client::{SshHandler, exec_capture};
use crate::sftp::{
    PushResult, open_sftp, read_remote_file, upload_local_skill_tree, write_remote_file,
};
use crate::types::{RemoteSkillContent, RemoteSkillUpdateState};

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

/// Read the raw SKILL.md content for a hub-managed remote skill.
///
/// Resolves `~/.skillstar/hub/content/<name>/SKILL.md` and returns the text
/// along with a best-effort mtime. Returns an error if the hub content or
/// SKILL.md is missing.
pub async fn read_remote_skill_content(
    handle: &mut Handle<SshHandler>,
    session_id: &str,
    sink: &impl crate::progress::ProgressSink,
    skill_name: &str,
) -> Result<RemoteSkillContent> {
    let sftp = open_sftp(handle, session_id, sink).await?;
    let remote_path = format!("{REMOTE_HUB_CONTENT}/{skill_name}/SKILL.md");
    let bytes = read_remote_file(&sftp, &remote_path).await?;
    let content = String::from_utf8_lossy(&bytes).into_owned();

    // Best-effort mtime via stat (SFTP attrs). We don't have direct stat in the
    // current sftp helpers; fall back to exec `stat -c %Y` for RFC3339-ish.
    let path_q = shell_quote(&remote_path);
    let mtime = exec_capture(handle, &format!("stat -c %Y {path_q} 2>/dev/null || true"))
        .await
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .and_then(|secs| chrono_like_rfc3339(secs));

    Ok(RemoteSkillContent {
        name: skill_name.to_string(),
        content,
        modified: mtime,
    })
}

/// Write raw text to `~/.skillstar/hub/content/<name>/SKILL.md` atomically.
pub async fn write_remote_skill_content(
    handle: &mut Handle<SshHandler>,
    session_id: &str,
    sink: &impl crate::progress::ProgressSink,
    skill_name: &str,
    content: &str,
) -> Result<()> {
    let sftp = open_sftp(handle, session_id, sink).await?;
    let remote_path = format!("{REMOTE_HUB_CONTENT}/{skill_name}/SKILL.md");
    write_remote_file(&sftp, &remote_path, content.as_bytes()).await?;
    Ok(())
}

/// Pull updates for a hub-managed remote skill via git.
///
/// Runs `git -C ~/.skillstar/hub/content/<name> pull --ff-only`.
/// Only meaningful for hub_managed skills that are git clones.
pub async fn pull_remote_skill(
    handle: &mut Handle<SshHandler>,
    _session_id: &str,
    _sink: &impl crate::progress::ProgressSink,
    skill_name: &str,
) -> Result<()> {
    let content_q = shell_quote(&format!("{REMOTE_HUB_CONTENT}/{skill_name}"));
    let script = format!(
        r#"set -e
if [ ! -d {content_q}/.git ]; then
  echo "NOT_A_GIT_REPO"
  exit 1
fi
git -C {content_q} pull --ff-only
echo OK
"#
    );
    let out = exec_capture(handle, &script).await?;
    if out.contains("NOT_A_GIT_REPO") {
        anyhow::bail!("remote skill '{}' is not a git repo under hub", skill_name);
    }
    Ok(())
}

/// Toggle (create/remove) the agent symlink for a hub-managed skill.
///
/// enable=true  → `ln -sfn <hub>/<name> <agent>/<name>`
/// enable=false → `rm -f <agent>/<name>` (idempotent)
pub async fn toggle_remote_agent_link(
    handle: &mut Handle<SshHandler>,
    skill_name: &str,
    agent_skills_dir: &str,
    enable: bool,
) -> Result<()> {
    let agent_base = agent_skills_dir.trim_end_matches('/');
    let link_q = shell_quote(&format!("{}/{}", agent_base, skill_name));
    let target_q = shell_quote(&format!("{REMOTE_HUB_CONTENT}/{}", skill_name));

    let script = if enable {
        format!("ln -sfn {target_q} {link_q} && echo OK")
    } else {
        format!("rm -f {link_q} && echo OK")
    };
    let out = exec_capture(handle, &script).await?;
    if !out.to_uppercase().contains("OK") && !out.trim().is_empty() {
        tracing::warn!(target: "ssh", %out, "toggle_remote_agent_link output");
    }
    Ok(())
}

/// Install a skill from a git URL directly onto the remote host.
///
/// Clones into `~/.skillstar/hub/content/<name>` (if not present) and creates
/// the agent symlink. Uses `--depth 1` for speed.
pub async fn install_remote_skill(
    handle: &mut Handle<SshHandler>,
    session_id: &str,
    sink: &impl crate::progress::ProgressSink,
    url: &str,
    skill_name: &str,
    agent_skills_dir: &str,
) -> Result<()> {
    let content_q = shell_quote(&format!("{REMOTE_HUB_CONTENT}/{}", skill_name));
    let hub_parent_q = shell_quote(REMOTE_HUB_CONTENT);
    let url_q = shell_quote(url);

    let script = format!(
        r#"set -e
mkdir -p {hub_parent_q}
if [ -e {content_q} ]; then
  echo "EXISTS"
  exit 0
fi
git clone --depth 1 {url_q} {content_q}
echo CLONED
"#
    );
    let out = exec_capture(handle, &script).await?;
    if out.contains("EXISTS") {
        // Already present — treat as success; we still ensure symlink below.
    } else if !out.contains("CLONED") && !out.trim().is_empty() {
        // git clone failed or printed something unexpected; still proceed to link attempt
        tracing::warn!(target: "ssh", %out, "install_remote_skill clone output");
    }

    // Ensure symlink into agent dir (idempotent via ln -sfn)
    let agent_base = agent_skills_dir.trim_end_matches('/');
    crate::sftp::ensure_remote_dir_pub(
        &open_sftp(handle, session_id, sink).await?,
        agent_base,
    )
    .await?;
    let link_q = shell_quote(&format!("{}/{}", agent_base, skill_name));
    let target_q = shell_quote(&format!("{REMOTE_HUB_CONTENT}/{}", skill_name));
    let ln_script = format!("ln -sfn {target_q} {link_q} && echo LINKED");
    let _ = exec_capture(handle, &ln_script).await;

    Ok(())
}

/// Check update availability for all hub-managed skills on this host.
///
/// Returns a list of `{name, update_available}`. For each hub content dir that
/// looks like a git repo, runs `git fetch -q && git rev-list --count HEAD..@{u}`.
/// A count > 0 means updates available.
pub async fn check_remote_skill_updates(
    handle: &mut Handle<SshHandler>,
    _session_id: &str,
    _sink: &impl crate::progress::ProgressSink,
) -> Result<Vec<RemoteSkillUpdateState>> {
    // Discover hub-managed skill names by listing ~/.skillstar/hub/content/* that have SKILL.md
    let hub_q = shell_quote(REMOTE_HUB_CONTENT);
    let list_script = format!(
        r#"for d in {hub_q}/*/; do
  [ -d "$d" ] || continue
  name=$(basename "$d")
  if [ -f "$d/SKILL.md" ]; then
    echo "$name"
  fi
done
"#
    );
    let names_out = exec_capture(handle, &list_script).await?;
    let names: Vec<&str> = names_out
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    let mut out = Vec::new();
    for name in names {
        let dir_q = shell_quote(&format!("{REMOTE_HUB_CONTENT}/{}", name));
    let script = format!(
        r#"set -e
if [ ! -d {dir_q}/.git ]; then
  echo "0"
  exit 0
fi
git -C {dir_q} fetch -q || true
cnt=$(git -C {dir_q} rev-list --count HEAD..@{{u}} 2>/dev/null || echo 0)
echo "$cnt"
"#
    );
        let cnt_str = exec_capture(handle, &script).await.unwrap_or_else(|_| "0".to_string());
        let cnt: u32 = cnt_str.trim().parse().unwrap_or(0);
        out.push(RemoteSkillUpdateState {
            name: name.to_string(),
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