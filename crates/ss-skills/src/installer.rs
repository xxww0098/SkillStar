//! Skill install semantics (D-081).
//!
//! Install = copy a skill folder into the canonical `~/.skillstar/data/skills/installed/<name>`
//! as real files and record provenance in the install lock. Same-name installs
//! from any source overwrite. Agent symlinking stays in `deployment`. A new
//! install is not linked into every enabled Agent. This module owns disk + lock only.
//!
//! Every write runs inside the Skill mutation transaction and goes through
//! [`crate::materialize`]: names are mapped to one canonical folder (or
//! refused), links escaping the fetched checkout are never followed, and the
//! new folders replace the old ones only after all of them are staged; the
//! lock is written last and a failed lock write restores the old folders.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use ss_core::infra::paths;

use crate::channels::shared_channels::ChannelInstallAuthority;
use crate::fetch;
use crate::materialize::{self, StagedReplace};
use crate::skill_lock::{self, SkillLockEntry};
use crate::source_resolver::Source;

pub use crate::materialize::canonical_skill_name;

/// Directory/file names never copied out of a source folder (skills CLI parity).
pub const COPY_EXCLUDES: &[&str] = &[".git", "__pycache__", "__pypackages__", "metadata.json"];

/// One skill to install out of a fetched checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallUnit {
    /// Skill identity (frontmatter `name`).
    pub id: String,
    /// Repo-relative folder ("" = the checkout root itself).
    pub folder_path: String,
}

struct PreparedUnit<'a> {
    unit: &'a InstallUnit,
    name: String,
    source_dir: std::path::PathBuf,
}

/// Copy `units` from `checkout` into canonical and write lock entries.
///
/// Overwrite semantics (skills CLI parity): an existing canonical folder with the
/// same name is replaced; provenance in the lock is rewritten. The batch fails
/// closed before any disk mutation when a unit's frontmatter does not pass the
/// install gate, when its identity has no safe canonical folder name, or when
/// two units map to the same folder.
///
/// This is the generic entry: channel-managed Skills and repositories are
/// refused. Channel flows use [`install_units_for_channel`].
pub fn install_units(checkout: &Path, spec: &Source, units: &[InstallUnit]) -> Result<Vec<String>> {
    install_units_as(checkout, spec, units, None)
}

/// [`install_units`] for a shared-channel install, upgrade or rollback: the
/// generic gate would refuse the channel's own Skills, so it is replaced by
/// "no Skill another channel manages".
pub(crate) fn install_units_for_channel(
    checkout: &Path,
    spec: &Source,
    units: &[InstallUnit],
    authority: &ChannelInstallAuthority,
) -> Result<Vec<String>> {
    install_units_as(checkout, spec, units, Some(authority))
}

fn ensure_writable(name: &str, authority: Option<&ChannelInstallAuthority>) -> Result<()> {
    let policy = crate::skill_mutation::policy();
    let Some(authority) = authority else {
        return policy.ensure_skill_mutation_allowed(name);
    };
    match policy.managed_repository_for_skill(name)? {
        Some(owner) if owner != authority.repository_id() => bail!(
            "'{name}' is managed by shared channel {owner}, not channel {}",
            authority.repository_id()
        ),
        _ => Ok(()),
    }
}

fn install_units_as(
    checkout: &Path,
    spec: &Source,
    units: &[InstallUnit],
    authority: Option<&ChannelInstallAuthority>,
) -> Result<Vec<String>> {
    if units.is_empty() {
        return Err(anyhow!("No skills selected for installation"));
    }
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    if authority.is_none() {
        crate::skill_mutation::policy().ensure_repository_mutation_allowed(&spec.repo_url)?;
    }
    let checkout_root = std::fs::canonicalize(checkout)
        .with_context(|| format!("Failed to resolve checkout {}", checkout.display()))?;

    let mut prepared: Vec<PreparedUnit> = Vec::new();
    let mut claimed: BTreeMap<String, &InstallUnit> = BTreeMap::new();
    let mut gate_errors = Vec::new();
    for unit in units {
        let name = match canonical_skill_name(&unit.id) {
            Ok(name) => name,
            Err(error) => {
                gate_errors.push(format!("'{}': {error}", unit.id));
                continue;
            }
        };
        if let Some(previous) = claimed.get(&name) {
            if *previous == unit {
                continue;
            }
            bail!(
                "Skills '{}' ({}) and '{}' ({}) both install as '{name}'; install them separately",
                previous.id,
                display_folder(&previous.folder_path),
                unit.id,
                display_folder(&unit.folder_path)
            );
        }
        claimed.insert(name.clone(), unit);
        ensure_writable(&name, authority)?;
        let source_dir = if unit.folder_path.is_empty() {
            checkout_root.clone()
        } else {
            checkout_root.join(&unit.folder_path)
        };
        if !source_dir.is_dir() {
            return Err(anyhow!(
                "Skill folder '{}' not found in the fetched source",
                unit.folder_path
            ));
        }
        if !std::fs::canonicalize(&source_dir).is_ok_and(|real| real.starts_with(&checkout_root)) {
            bail!(
                "Skill folder '{}' resolves outside the fetched source",
                unit.folder_path
            );
        }
        if let Err(reason) = crate::validation::ensure_installable(&source_dir) {
            gate_errors.push(format!("'{}': {reason}", unit.id));
            continue;
        }
        prepared.push(PreparedUnit {
            unit,
            name,
            source_dir,
        });
    }
    if !gate_errors.is_empty() {
        return Err(anyhow!(
            "Refusing to install invalid skill(s):\n{}",
            gate_errors.join("\n")
        ));
    }

    let root = paths::agents_skills_root();
    std::fs::create_dir_all(&root).context("Failed to create canonical skills directory")?;
    let has_git = checkout.join(".git").exists();
    let folder_hash = |folder: &str| -> Option<String> {
        has_git
            .then(|| fetch::folder_tree_hash(checkout, (!folder.is_empty()).then_some(folder)))
            .flatten()
    };
    let source_type = skill_lock::classify_source(&spec.repo_url);
    let now = chrono::Utc::now().to_rfc3339();

    let mut staged = Vec::with_capacity(prepared.len());
    for item in &prepared {
        let dest = root.join(&item.name);
        let replace = StagedReplace::stage(&dest, |staging| {
            materialize::copy_confined(&item.source_dir, staging, &checkout_root, COPY_EXCLUDES)
        })
        .with_context(|| format!("Failed to copy skill '{}' into canonical", item.name))?;
        staged.push(replace);
    }
    for replace in &mut staged {
        replace.swap()?;
    }

    let entries: Vec<(String, SkillLockEntry)> = prepared
        .iter()
        .map(|item| {
            let folder = &item.unit.folder_path;
            (
                item.name.clone(),
                SkillLockEntry {
                    source: spec.short.clone(),
                    source_type,
                    source_url: spec.repo_url.clone(),
                    git_ref: spec.git_ref.clone(),
                    skill_path: (!folder.is_empty()).then(|| folder.clone()),
                    skill_folder_hash: folder_hash(folder),
                    installed_at: now.clone(),
                    updated_at: now.clone(),
                    extra: Default::default(),
                },
            )
        })
        .collect();
    skill_lock::mutate(|lock| {
        for (name, entry) in entries {
            lock.upsert(&name, entry);
        }
    })
    .context("Failed to record installed skills in the lock")?;
    for replace in staged {
        replace.commit();
    }
    let names: Vec<String> = prepared.into_iter().map(|item| item.name).collect();
    crate::install_baseline::record(&names);
    Ok(names)
}

fn display_folder(folder: &str) -> &str {
    if folder.is_empty() {
        "repository root"
    } else {
        folder
    }
}

/// Remove the canonical folder and lock entry for one skill.
///
/// Agent and project deployments are cleared by the uninstall module before
/// this runs. The folder is moved aside first and put back when the lock
/// cannot be rewritten.
pub fn uninstall_canonical(name: &str) -> Result<()> {
    crate::content::validate_skill_name(name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;
    // Prove the lock can be rewritten before anything is moved. A refused
    // lock must not leave the Skill half removed.
    crate::skill_lock::ensure_writable()?;
    let dest = paths::agents_skill_dir(name);
    let aside = dest.with_file_name(format!(
        "{}remove-{name}-{}",
        materialize::TRANSIENT_PREFIX,
        uuid::Uuid::new_v4().simple()
    ));
    let moved = dest.symlink_metadata().is_ok();
    if moved {
        std::fs::rename(&dest, &aside)
            .with_context(|| format!("Failed to remove canonical skill '{name}'"))?;
        materialize::note_transient_born(&aside);
    }
    if let Err(error) = skill_lock::mutate(|lock| lock.remove(name)) {
        if moved && std::fs::rename(&aside, &dest).is_ok() {
            materialize::forget_transient_born(&aside);
        }
        return Err(error.context("Failed to remove skill from the lock"));
    }
    crate::install_baseline::forget(name);
    if moved {
        materialize::remove_entry(&aside)
            .with_context(|| format!("Failed to remove canonical skill '{name}'"))?;
    }
    Ok(())
}

/// Enumerate installed skill names from the canonical directory. Hidden
/// entries (staging/backup folders of an in-flight install) are not Skills.
pub fn installed_names() -> Vec<String> {
    let root = paths::agents_skills_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .into_string()
                .ok()
                .map(|name| (entry, name))
        })
        .filter(|(entry, name)| !name.starts_with('.') && entry.path().join("SKILL.md").is_file())
        .map(|(_, name)| name)
        .collect()
}

#[cfg(test)]
#[path = "installer_tests.rs"]
mod tests;
