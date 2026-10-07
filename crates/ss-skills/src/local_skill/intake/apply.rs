//! Copy, lock, and link steps for one intake action. Each function restores
//! the step it started when a later write fails.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use ss_core::infra::{fs_ops, paths};

use super::{INTAKE_EXCLUDES, blocking, channel_block, content_matches, entry_set, failpoint};
use crate::materialize::{self, StagedReplace};
use crate::skill_lock::{self, SkillLockEntry, SourceType};

pub(super) fn adopt(agent_dir: &Path, name: &str, agent_id: &str) -> Result<()> {
    if let Some(reason) = channel_block(name) {
        bail!(reason);
    }
    let local = paths::local_skills_dir().join(name);
    let hub = paths::hub_skills_dir().join(name);
    if local.symlink_metadata().is_ok() || hub.symlink_metadata().is_ok() {
        bail!("canonical name is occupied; left unchanged");
    }
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = hub.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut staged_local = StagedReplace::stage(&local, |staging| {
        materialize::copy_confined(agent_dir, staging, agent_dir, INTAKE_EXCLUDES)
    })?;
    staged_local.swap()?;
    let finish = finish_adopt(agent_dir, name, agent_id, &local, &hub);
    if finish.is_err() {
        drop(staged_local);
        return finish;
    }
    staged_local.commit();
    Ok(())
}

fn finish_adopt(
    agent_dir: &Path,
    name: &str,
    agent_id: &str,
    local: &Path,
    hub: &Path,
) -> Result<()> {
    failpoint("copy")?;
    verify_covers(agent_dir, local)?;
    crate::local_identity::replace_untrusted_sidecar(local)
        .map_err(anyhow::Error::from)
        .context("could not mint a local skill identity")?;
    let mut hub_link = stage_absolute_link(hub, local)?;
    hub_link.swap()?;
    let linked = link_agent_after_copy(agent_dir, name, agent_id, local, hub);
    if linked.is_ok() {
        hub_link.commit();
    }
    linked
}

fn link_agent_after_copy(
    agent_dir: &Path,
    name: &str,
    agent_id: &str,
    local: &Path,
    hub: &Path,
) -> Result<()> {
    record_adoption(name, agent_id)?;
    let recorded = replace_agent_with_link(agent_dir, local, hub);
    if recorded.is_err() {
        let _ = forget_adoption(name);
    }
    recorded
}

fn replace_agent_with_link(agent_dir: &Path, local: &Path, hub: &Path) -> Result<()> {
    if let Some(reason) = blocking(agent_dir)? {
        bail!(reason);
    }
    verify_covers(agent_dir, local)?;
    let mut agent_link = stage_relative_link(agent_dir, hub)?;
    agent_link.swap()?;
    let verified = verify_relative_link(agent_dir, hub).and_then(|_| {
        if agent_dir.join("SKILL.md").is_file() {
            Ok(())
        } else {
            bail!("SKILL.md missing after linking")
        }
    });
    if verified.is_ok() {
        commit_unless_backup_drifted(agent_link, local)
    } else {
        verified
    }
}

/// `Ok(Some(reason))` when the directory should stay. `Ok(None)` when the
/// relative link is in place.
pub(super) fn relink(agent_dir: &Path, target: &Path) -> Result<Option<String>> {
    let resolved = std::fs::canonicalize(target)
        .with_context(|| format!("canonical skill {} is not readable", target.display()))?;
    if !content_matches(agent_dir, &resolved)? {
        return Ok(Some("Content conflict; both versions kept".into()));
    }
    if let Some(reason) = blocking(agent_dir)? {
        return Ok(Some(reason));
    }
    let mut link = stage_relative_link(agent_dir, target)?;
    link.swap()?;
    let verified = verify_relative_link(agent_dir, target);
    if verified.is_ok() {
        commit_unless_backup_drifted(link, target).map(|()| None)
    } else {
        verified.map(|()| None)
    }
}

fn record_adoption(name: &str, agent_id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let source = format!("local/{agent_id}");
    let skill = name.to_string();
    let inserted = skill_lock::mutate(|lock| {
        if lock.entry_for_folder(&skill).is_some() {
            return false;
        }
        lock.upsert(
            &skill,
            SkillLockEntry {
                source: source.clone(),
                source_type: SourceType::Local,
                source_url: String::new(),
                git_ref: None,
                skill_path: None,
                skill_folder_hash: None,
                installed_at: now.clone(),
                updated_at: now.clone(),
                extra: Default::default(),
            },
        );
        true
    })?;
    if !inserted {
        bail!("install lock already records {name}");
    }
    Ok(())
}

fn forget_adoption(name: &str) -> Result<()> {
    let skill = name.to_string();
    skill_lock::mutate(|lock| {
        let ours = lock.skills.get(&skill).is_some_and(|entry| {
            entry.source_type == SourceType::Local && entry.source.starts_with("local/")
        });
        if ours {
            lock.remove(&skill);
        }
    })?;
    Ok(())
}

fn stage_relative_link(link_path: &Path, target: &Path) -> Result<StagedReplace> {
    let parent = link_path
        .parent()
        .context("skill path has no parent directory")?;
    let relative = relative_between(parent, target)?;
    StagedReplace::stage(link_path, move |staging| {
        fs_ops::create_symlink(&relative, staging)
    })
}

fn stage_absolute_link(link_path: &Path, target: &Path) -> Result<StagedReplace> {
    let target = std::fs::canonicalize(target)?;
    StagedReplace::stage(link_path, move |staging| {
        fs_ops::create_symlink(&target, staging)
    })
}

fn verify_relative_link(link: &Path, target: &Path) -> Result<()> {
    let raw = std::fs::read_link(link)
        .with_context(|| format!("could not read link {}", link.display()))?;
    if raw.is_absolute() {
        bail!("refusing an absolute link at {}", link.display());
    }
    let resolved = std::fs::canonicalize(link)
        .with_context(|| format!("link {} does not resolve", link.display()))?;
    let expected = std::fs::canonicalize(target)?;
    if resolved != expected {
        bail!(
            "link {} resolved to {}, expected {}",
            link.display(),
            resolved.display(),
            expected.display()
        );
    }
    Ok(())
}

/// Every file in `source` is present in `dest`. An excluded name or a link
/// the copy dropped is an error: the Agent directory must not be replaced.
/// `Ok(true)` when `backup` has a file the verified tree lacks or a file
/// whose bytes differ. Commit must restore in that case instead of deleting
/// the backup.
pub(super) fn backup_drifted(backup: &Path, verified: &Path) -> Result<bool> {
    let backup_entries = entry_set(backup)?;
    let verified_entries = entry_set(verified)?;
    for rel in &backup_entries {
        if !verified_entries.contains(rel) {
            return Ok(true);
        }
        let from = backup.join(rel);
        let to = verified.join(rel);
        if from.is_file() && std::fs::read(&from)? != std::fs::read(&to)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn commit_unless_backup_drifted(staged: StagedReplace, verified: &Path) -> Result<()> {
    let backup = staged.backup_path().map(Path::to_path_buf);
    if let Some(backup) = backup
        && backup_drifted(&backup, verified)?
    {
        drop(staged);
        bail!("Content conflict; the Agent directory was restored");
    }
    staged.commit();
    Ok(())
}

fn verify_covers(source: &Path, dest: &Path) -> Result<()> {
    if let Some(reason) = blocking(source)? {
        bail!(reason);
    }
    let source_entries = entry_set(source)?;
    let dest_entries = entry_set(dest)?;
    for rel in &source_entries {
        if !dest_entries.contains(rel) {
            bail!(
                "copy dropped {}; the Agent directory was left unchanged",
                rel.display()
            );
        }
        let from = source.join(rel);
        let to = dest.join(rel);
        if from.is_file() && std::fs::read(&from)? != std::fs::read(&to)? {
            bail!(
                "copy of {} does not match the Agent directory",
                rel.display()
            );
        }
    }
    Ok(())
}

fn relative_between(from_dir: &Path, target: &Path) -> Result<PathBuf> {
    let from = std::fs::canonicalize(from_dir)?;
    let to = logical_absolute(target)?;
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    if let (Some(Component::Prefix(left)), Some(Component::Prefix(right))) =
        (from_parts.first(), to_parts.first())
        && left != right
    {
        bail!(
            "cannot form a relative link from {} to {}",
            from.display(),
            to.display()
        );
    }
    let mut shared = 0;
    while shared < from_parts.len()
        && shared < to_parts.len()
        && from_parts[shared] == to_parts[shared]
    {
        shared += 1;
    }
    let mut relative = PathBuf::new();
    for _ in shared..from_parts.len() {
        relative.push("..");
    }
    for part in &to_parts[shared..] {
        relative.push(part);
    }
    if relative.as_os_str().is_empty() {
        bail!("refusing to link {} onto itself", target.display());
    }
    Ok(relative)
}

/// Absolute path that keeps the final component even when it is a symlink,
/// so the Agent link names the canonical entry rather than the local directory
/// that entry points at.
fn logical_absolute(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .context("link target has no parent directory")?;
    let name = path.file_name().context("link target has no file name")?;
    Ok(std::fs::canonicalize(parent)?.join(name))
}
