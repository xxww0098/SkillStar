//! `.ags` / `.agd` import.
//!
//! The archive is read into memory under size caps and its checksum verified
//! before anything touches the canonical root. Every Skill is then staged
//! beside its destination, all of them are swapped in, the lock records them
//! as `bundle` installs, and only then are the replaced entries dropped. A
//! failure at any step rolls every Skill of the bundle back.

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use tar::Archive;

use super::{
    BundleManifest, FORMAT_VERSION, ImportBundleResult, ImportMultiBundleResult, MANIFEST_NAME,
    MULTI_MANIFEST_NAME, MultiManifest, is_unsafe_archive_path,
};
use crate::materialize::{self, StagedReplace};
use crate::skill_lock::{self, SkillLockEntry, SourceType};

/// One archive member. Real bundles hold a handful of small text files; the
/// caps only exist so a crafted archive cannot exhaust memory or disk.
pub(super) const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

type Files = Vec<(String, Vec<u8>)>;

/// Regular-file members with safe, non-hidden relative paths, in archive order.
fn read_archive(file_path: &str) -> Result<Files> {
    let file = std::fs::File::open(file_path)
        .with_context(|| format!("Cannot open bundle: {file_path}"))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    let mut total = 0u64;
    let mut files = Vec::new();
    for entry in archive.entries()? {
        let entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        if is_unsafe_archive_path(&path) || path.split('/').any(|part| part.starts_with('.')) {
            continue;
        }
        if files.len() >= MAX_ENTRIES {
            bail!("Bundle holds more than {MAX_ENTRIES} files");
        }
        let mut content = Vec::new();
        entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut content)?;
        if content.len() as u64 > MAX_ENTRY_BYTES {
            bail!(
                "Bundle file {path} exceeds {} MB",
                MAX_ENTRY_BYTES / (1024 * 1024)
            );
        }
        total += content.len() as u64;
        if total > MAX_TOTAL_BYTES {
            bail!(
                "Bundle content exceeds {} MB",
                MAX_TOTAL_BYTES / (1024 * 1024)
            );
        }
        files.push((path, content));
    }
    Ok(files)
}

fn take_manifest<T: serde::de::DeserializeOwned>(
    files: &mut Files,
    name: &str,
) -> Result<Option<T>> {
    let Some(index) = files.iter().position(|(path, _)| path == name) else {
        return Ok(None);
    };
    let (_, content) = files.remove(index);
    serde_json::from_slice(&content)
        .with_context(|| format!("Invalid {name} in bundle"))
        .map(Some)
}

fn sorted(mut files: Files) -> Files {
    files.sort_by(|(left, _), (right, _)| left.cmp(right));
    files
}

fn checksum<'a>(contents: impl Iterator<Item = &'a [u8]>) -> String {
    let mut hasher = Sha256::new();
    for content in contents {
        hasher.update(content);
    }
    let hex: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}")
}

#[cfg(test)]
pub(super) fn checksum_for_test(contents: &[&[u8]]) -> String {
    checksum(contents.iter().copied())
}

fn ensure_checksum(expected: &str, actual: &str) -> Result<()> {
    if expected != actual {
        bail!(
            "Checksum mismatch: bundle may be corrupted (expected {}, got {})",
            expected.get(..19).unwrap_or(expected),
            actual.get(..19).unwrap_or(actual)
        );
    }
    Ok(())
}

fn ensure_format(version: u32) -> Result<()> {
    if version > FORMAT_VERSION {
        bail!("Bundle format version {version} is not supported (max: {FORMAT_VERSION})");
    }
    Ok(())
}

struct BundleSkill {
    /// Canonical folder name under the canonical root.
    name: String,
    files: Files,
}

struct Installed {
    name: String,
    file_count: usize,
    replaced: bool,
}

/// Import a `.ags` file into the canonical root.
///
/// If `force` is true, replaces an existing skill with the same name.
pub fn import_bundle(file_path: &str, force: bool) -> Result<ImportBundleResult> {
    let mut files = read_archive(file_path)?;
    let manifest: BundleManifest = take_manifest(&mut files, MANIFEST_NAME)?
        .ok_or_else(|| anyhow::anyhow!("Bundle does not contain manifest.json"))?;
    import_single(file_path, manifest, files, force)
}

fn import_single(
    file_path: &str,
    manifest: BundleManifest,
    files: Files,
    force: bool,
) -> Result<ImportBundleResult> {
    ensure_format(manifest.format_version)?;
    let name = folder_name(&manifest.name)?;
    let files = sorted(files);
    ensure_checksum(
        &manifest.checksum,
        &checksum(files.iter().map(|(_, content)| content.as_slice())),
    )?;
    let installed = install(file_path, vec![BundleSkill { name, files }], force)?;
    let installed = installed
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Bundle installed nothing"))?;
    Ok(ImportBundleResult {
        name: installed.name,
        description: manifest.description,
        file_count: installed.file_count,
        replaced: installed.replaced,
    })
}

/// Import a `.agd` multi-bundle. Every Skill in it lands, or none does.
pub fn import_multi_bundle(file_path: &str, force: bool) -> Result<ImportMultiBundleResult> {
    let mut files = read_archive(file_path)?;
    if let Some(single) = take_manifest::<BundleManifest>(&mut files, MANIFEST_NAME)? {
        return import_single(file_path, single, files, force).map(|r| ImportMultiBundleResult {
            skill_names: vec![r.name],
            total_file_count: r.file_count,
            replaced_count: usize::from(r.replaced),
        });
    }
    let manifest: MultiManifest =
        take_manifest(&mut files, MULTI_MANIFEST_NAME)?.ok_or_else(|| {
            anyhow::anyhow!("Bundle does not contain multi_manifest.json or manifest.json")
        })?;
    ensure_format(manifest.format_version)?;

    let mut by_skill: HashMap<String, Files> = HashMap::new();
    for (path, content) in files {
        if let Some((skill, rel)) = path.split_once('/')
            && !rel.is_empty()
        {
            by_skill
                .entry(skill.to_string())
                .or_default()
                .push((rel.to_string(), content));
        }
    }

    let mut folders = HashSet::new();
    let mut skills = Vec::new();
    for entry in &manifest.skills {
        let name = folder_name(&entry.name)
            .with_context(|| "Invalid Skill name in multi-bundle manifest")?;
        if !folders.insert(name.clone()) {
            bail!(
                "Duplicate Skill name in multi-bundle manifest: {}",
                entry.name
            );
        }
        let Some(files) = by_skill.remove(&entry.name) else {
            continue;
        };
        skills.push(BundleSkill {
            name,
            files: sorted(files),
        });
    }
    ensure_checksum(
        &manifest.checksum,
        &checksum(
            skills
                .iter()
                .flat_map(|skill| skill.files.iter().map(|(_, content)| content.as_slice())),
        ),
    )?;

    let installed = install(file_path, skills, force)?;
    Ok(ImportMultiBundleResult {
        total_file_count: installed.iter().map(|skill| skill.file_count).sum(),
        replaced_count: installed.iter().filter(|skill| skill.replaced).count(),
        skill_names: installed.into_iter().map(|skill| skill.name).collect(),
    })
}

fn folder_name(manifest_name: &str) -> Result<String> {
    crate::content::validate_skill_name(manifest_name)
        .map_err(|error| anyhow::anyhow!("Invalid Skill name in bundle manifest: {error}"))?;
    materialize::canonical_skill_name(manifest_name)
}

fn install(file_path: &str, skills: Vec<BundleSkill>, force: bool) -> Result<Vec<Installed>> {
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let root = ss_core::infra::paths::hub_skills_dir();
    for skill in &skills {
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(&skill.name)?;
        if !force && root.join(&skill.name).symlink_metadata().is_ok() {
            bail!("CONFLICT:{}", skill.name);
        }
    }
    std::fs::create_dir_all(&root).context("Failed to create canonical skills directory")?;

    let mut staged = Vec::with_capacity(skills.len());
    let mut installed = Vec::with_capacity(skills.len());
    for skill in &skills {
        let dest = root.join(&skill.name);
        let replaced = dest.symlink_metadata().is_ok();
        let replace = StagedReplace::stage(&dest, |staging| {
            write_files(staging, &skill.files)?;
            crate::validation::ensure_installable(staging).map_err(|reason| {
                anyhow::anyhow!("Bundle skill '{}' is not installable: {reason}", skill.name)
            })
        })?;
        staged.push(replace);
        installed.push(Installed {
            name: skill.name.clone(),
            file_count: skill.files.len(),
            replaced,
        });
    }
    for replace in &mut staged {
        replace.swap()?;
    }

    let source_url = std::fs::canonicalize(file_path)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file_path.to_string());
    let source = Path::new(file_path)
        .file_name()
        .map(|name| format!("bundle/{}", name.to_string_lossy()))
        .unwrap_or_else(|| "bundle".to_string());
    let now = chrono::Utc::now().to_rfc3339();
    skill_lock::mutate(|lock| {
        for skill in &skills {
            lock.upsert(
                &skill.name,
                SkillLockEntry {
                    source: source.clone(),
                    source_type: SourceType::Bundle,
                    source_url: source_url.clone(),
                    git_ref: None,
                    skill_path: None,
                    skill_folder_hash: None,
                    installed_at: now.clone(),
                    updated_at: now.clone(),
                    extra: Default::default(),
                },
            );
        }
    })
    .context("Failed to record imported skills in the lock")?;
    for replace in staged {
        replace.commit();
    }

    let names: Vec<String> = installed.iter().map(|skill| skill.name.clone()).collect();
    crate::install_baseline::record(&names);
    crate::installed_skill::invalidate_cache();
    Ok(installed)
}

fn write_files(root: &Path, files: &Files) -> Result<()> {
    std::fs::create_dir_all(root)?;
    for (rel, content) in files {
        let dest = root.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, content)
            .with_context(|| format!("Failed to write {}", dest.display()))?;
    }
    Ok(())
}
