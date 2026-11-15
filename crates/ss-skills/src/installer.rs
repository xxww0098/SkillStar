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

use crate::channel_registry::ChannelInstallAuthority;
use crate::fetch;
use crate::materialize::{self, StagedReplace};
use crate::skill_lock::{self, SkillLockEntry};
use crate::source_resolver::Source;

pub use crate::materialize::{
    canonical_skill_name, resolve_canonical_skill_name, rewrite_frontmatter_skill_name,
};

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
    called: Option<String>,
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

    let existing_lock = skill_lock::load();
    let existing_lib = crate::library::load();
    let root = paths::agents_skills_root();
    let is_same_source = |slot: &str| -> bool {
        if let Some((_, lock_entry)) = existing_lock.entry_for_folder(slot) {
            if !lock_entry.source_url.is_empty() && !spec.repo_url.is_empty() {
                return crate::source_resolver::same_remote_url(
                    &lock_entry.source_url,
                    &spec.repo_url,
                );
            }
            return lock_entry.source == spec.short;
        }
        if let Some(lib_entry) = existing_lib.skills.get(slot) {
            if let Some(url) = lib_entry.source_url.as_deref()
                && !spec.repo_url.is_empty()
            {
                return crate::source_resolver::same_remote_url(url, &spec.repo_url);
            }
            if let Some(src) = lib_entry.source.as_deref() {
                return src == spec.short;
            }
        }
        false
    };
    let existing_called = |slot: &str| -> Option<String> {
        existing_lib
            .skills
            .get(slot)
            .and_then(|e| e.called.clone())
            .or_else(|| {
                existing_lock.entry_for_folder(slot).and_then(|(_, e)| {
                    e.extra
                        .get("called")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
            })
    };
    let is_occupied = |slot: &str| -> bool {
        root.join(slot).symlink_metadata().is_ok()
            || existing_lock.entry_for_folder(slot).is_some()
    };
    let owner_slug = crate::update_api::owner_repo_from_git_url(&spec.repo_url)
        .map(|(owner, _)| owner)
        .or_else(|| {
            spec.short
                .split('/')
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .and_then(|raw| canonical_skill_name(&raw).ok())
        .unwrap_or_else(|| "alt".to_string());

    let mut prepared: Vec<PreparedUnit> = Vec::new();
    let mut claimed: BTreeMap<String, &InstallUnit> = BTreeMap::new();
    let mut gate_errors = Vec::new();
    for unit in units {
        let (base_name, fallback_called) =
            match resolve_canonical_skill_name(&unit.id, &unit.folder_path) {
                Ok(resolved) => resolved,
                Err(error) => {
                    gate_errors.push(format!("'{}': {error}", unit.id));
                    continue;
                }
            };
        let (name, called) = if authority.is_some() {
            (base_name, fallback_called)
        } else {
            let raw_id = unit.id.trim();
            let existing_disambiguated = (!raw_id.is_empty())
                .then(|| {
                    existing_lib
                        .skills
                        .iter()
                        .find(|(slot, entry)| {
                            entry.called.as_deref() == Some(raw_id)
                                && is_same_source(slot)
                        })
                        .map(|(slot, _)| slot.clone())
                        .or_else(|| {
                            existing_lock
                                .skills
                                .iter()
                                .find(|(slot, entry)| {
                                    entry
                                        .extra
                                        .get("called")
                                        .and_then(|v| v.as_str())
                                        == Some(raw_id)
                                        && is_same_source(slot)
                                })
                                .map(|(slot, _)| slot.clone())
                        })
                })
                .flatten();

            if let Some(slot) = existing_disambiguated {
                (slot, Some(raw_id.to_string()))
            } else if !is_occupied(&base_name) || is_same_source(&base_name) {
                let preserved_called = fallback_called.or_else(|| {
                    (raw_id == base_name)
                        .then(|| existing_called(&base_name))
                        .flatten()
                });
                (base_name, preserved_called)
            } else {
                let mut chosen = None;
                let base_candidate = format!("{base_name}-{owner_slug}");
                if let Ok(valid) = canonical_skill_name(&base_candidate)
                    && (!is_occupied(&valid) || is_same_source(&valid))
                {
                    chosen = Some(valid);
                } else {
                    for n in 2..=25u32 {
                        let candidate = format!("{base_name}-{owner_slug}-{n}");
                        if let Ok(valid) = canonical_skill_name(&candidate)
                            && (!is_occupied(&valid) || is_same_source(&valid))
                        {
                            chosen = Some(valid);
                            break;
                        }
                    }
                }
                let final_name = chosen.unwrap_or(base_name);
                let orig_called = if raw_id.is_empty() {
                    fallback_called
                } else {
                    Some(raw_id.to_string())
                };
                (final_name, orig_called)
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
            called,
            source_dir,
        });
    }
    if !gate_errors.is_empty() {
        return Err(anyhow!(
            "Refusing to install invalid skill(s):\n{}",
            gate_errors.join("\n")
        ));
    }

    std::fs::create_dir_all(&root).context("Failed to create canonical skills directory")?;
    let has_git = checkout.join(".git").exists();
    let folder_hash = |folder: &str| -> Option<String> {
        has_git
            .then(|| fetch::folder_tree_hash(checkout, (!folder.is_empty()).then_some(folder)))
            .flatten()
    };
    let source_type = skill_lock::source_type_for(spec);
    let now = chrono::Utc::now().to_rfc3339();

    let mut staged = Vec::with_capacity(prepared.len());
    let mut changed_descriptions = Vec::new();
    for item in &prepared {
        let dest = root.join(&item.name);
        if authority.is_none()
            && dest.is_dir()
            && crate::install_baseline::local_content(&item.name)
                == crate::install_baseline::LocalContent::Modified
        {
            crate::backup::archive_skill(
                &item.name,
                &dest,
                crate::backup::BackupReason::UpdateOverwrite,
            )?;
        }
        let old_description = dest
            .is_dir()
            .then(|| ss_core::types::extract_skill_description(&dest));
        let replace = StagedReplace::stage(&dest, |staging| {
            materialize::copy_confined(&item.source_dir, staging, &checkout_root, COPY_EXCLUDES)?;
            let fm_needs_rewrite = item.called.is_some()
                || crate::validation::inspect_skill_frontmatter(staging)
                    .name
                    .as_deref()
                    .and_then(|n| canonical_skill_name(n).ok())
                    .as_deref()
                    != Some(item.name.as_str());
            if fm_needs_rewrite {
                rewrite_frontmatter_skill_name(staging, &item.name)?;
            }
            let new_description = ss_core::types::extract_skill_description(staging);
            if old_description
                .as_deref()
                .is_some_and(|old| old.trim() != new_description.trim())
            {
                changed_descriptions.push(item.name.as_str());
            }
            Ok(())
        })
        .with_context(|| format!("Failed to copy skill '{}' into canonical", item.name))?;
        staged.push(replace);
    }
    for replace in &mut staged {
        replace.swap()?;
    }

    let entries: Vec<(String, SkillLockEntry, Option<String>)> = prepared
        .iter()
        .map(|item| {
            let folder = &item.unit.folder_path;
            let mut extra = existing_lock
                .entry_for_folder(&item.name)
                .map(|(_, e)| e.extra.clone())
                .unwrap_or_default();
            if let Some(called) = &item.called {
                extra.insert("called".to_string(), serde_json::json!(called));
            }
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
                    extra,
                },
                item.called.clone(),
            )
        })
        .collect();
    skill_lock::mutate(|lock| {
        for (name, entry, _) in &entries {
            lock.upsert(name, entry.clone());
        }
    })
    .context("Failed to record installed skills in the lock")?;
    for (item, replace) in prepared.iter().zip(&staged) {
        if let Some(old_dir) = replace.backup_path() {
            let _ = crate::runtime_ignore::carry_runtime_artifacts(old_dir, &root.join(&item.name));
        }
    }
    for replace in staged {
        replace.commit();
    }
    // Only a committed description change resets the language choice; a
    // body-only update or a rolled-back install keeps the cached translation.
    if !changed_descriptions.is_empty()
        && let Err(error) = ss_core::translation::set_description_choices(
            changed_descriptions.into_iter().map(|name| (name, false)),
        )
    {
        tracing::warn!("failed to reset updated description choices: {error}");
    }
    let names: Vec<String> = prepared.into_iter().map(|item| item.name).collect();
    crate::install_baseline::record(&names);
    for (name, entry, called) in &entries {
        if let Err(error) = crate::library::record_remote_install(
            name,
            entry,
            called.clone(),
            crate::install_baseline::recorded_hash(name),
            authority.is_some(),
        ) {
            tracing::warn!(target: "skills", skill = %name, "failed to record skill in library.json: {error:#}");
        }
    }
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
    if dest.exists() {
        crate::backup::archive_skill(name, &dest, crate::backup::BackupReason::Uninstall)?;
    }
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
    let _ = crate::library::forget_skill(name);
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
