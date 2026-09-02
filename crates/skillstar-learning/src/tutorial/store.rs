use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_core::infra::error::AppError;
use uuid::Uuid;

use super::{
    GeneratorFingerprint, PrivateTutorial, PrivateTutorialMetadata, TutorialStaleReason,
    TutorialState, ValidatedTutorialHtml, validate_html,
};
use crate::identity::{ResolvedSkill, SkillIdentity, SkillRevision};

const LEGACY_ARTIFACT_KEY_DOMAIN: &[u8] = b"skillstar.skill-tutorial-key.v1\0";
const STORE_SCHEMA: u32 = 2;

static ARTIFACT_IO_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredMetadata {
    #[serde(default = "default_store_schema")]
    store_schema: u32,
    #[serde(default)]
    bound: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<SkillIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generated_revision: Option<SkillRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skill_name: Option<String>,
    content_hash: String,
    prompt_version: String,
    #[serde(alias = "schemaVersion")]
    schema_version: String,
    tutorial_style: String,
    agent_label: String,
    generated_at: String,
    file_count: usize,
    total_bytes: u64,
    #[serde(default)]
    source_files: Vec<String>,
}

fn default_store_schema() -> u32 {
    1
}

pub fn load(
    resolved: &ResolvedSkill,
    inventory: &[String],
    total_bytes: u64,
    generator: &GeneratorFingerprint,
) -> Result<PrivateTutorial, AppError> {
    let identity_dir = identity_directory(&resolved.identity);
    let identity_root = identity_dir.parent().ok_or_else(|| {
        AppError::Other(format!(
            "Tutorial artifact has no parent: {}",
            identity_dir.display()
        ))
    })?;
    let _guard = lock_artifact_io(identity_root)?;
    recover_missing_artifact_directory(&identity_dir)?;
    if identity_dir.join("tutorial.html").is_file() {
        return load_from_directory(
            &identity_dir,
            resolved,
            inventory,
            total_bytes,
            generator,
            true,
        );
    }

    if let Some(name) = resolved.installed_name.as_deref() {
        let legacy_dir = legacy_directory(name);
        recover_missing_artifact_directory(&legacy_dir)?;
        if legacy_dir.join("tutorial.html").is_file() {
            return load_from_directory(
                &legacy_dir,
                resolved,
                inventory,
                total_bytes,
                generator,
                false,
            );
        }
    }
    Ok(missing())
}

pub fn commit(
    resolved: &ResolvedSkill,
    inventory: &[String],
    total_bytes: u64,
    generator: &GeneratorFingerprint,
    tutorial_style: &str,
    agent_label: &str,
    validated_html: ValidatedTutorialHtml,
) -> Result<PrivateTutorial, AppError> {
    let identity = resolved.identity.clone().verified()?;
    let revision = resolved.revision.clone().verified(&identity)?;
    let directory = identity_directory(&identity);
    let root = directory.parent().ok_or_else(|| {
        AppError::Other(format!(
            "Tutorial artifact has no parent: {}",
            directory.display()
        ))
    })?;
    let _guard = lock_artifact_io(root)?;
    recover_missing_artifact_directory(&directory)?;

    let unique = inventory.iter().collect::<BTreeSet<_>>();
    if unique.len() != inventory.len() {
        return Err(AppError::Other(
            "Private tutorial inventory contains duplicate paths".to_string(),
        ));
    }

    let stored = StoredMetadata {
        store_schema: STORE_SCHEMA,
        bound: true,
        identity: Some(identity),
        generated_revision: Some(revision.clone()),
        skill_name: None,
        content_hash: revision.content.content_hash.clone(),
        prompt_version: generator.prompt_version.clone(),
        schema_version: generator.schema_version.clone(),
        tutorial_style: tutorial_style.to_string(),
        agent_label: agent_label.to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        file_count: inventory.len(),
        total_bytes,
        source_files: inventory.to_vec(),
    };
    let metadata_json = serde_json::to_string_pretty(&stored)?;
    let html = validated_html.into_inner();
    replace_artifact_directory(&directory, &html, &metadata_json)?;
    Ok(PrivateTutorial {
        state: TutorialState::Fresh,
        bound: true,
        html: Some(html),
        metadata: Some(to_public_metadata(stored)),
        stale_reason: None,
        stale_reasons: Vec::new(),
    })
}

fn load_from_directory(
    directory: &Path,
    resolved: &ResolvedSkill,
    inventory: &[String],
    total_bytes: u64,
    generator: &GeneratorFingerprint,
    expect_bound: bool,
) -> Result<PrivateTutorial, AppError> {
    let html_path = directory.join("tutorial.html");
    let metadata_path = directory.join("metadata.json");
    let html_exists = html_path.is_file();
    let metadata_exists = metadata_path.is_file();
    if html_exists != metadata_exists {
        return Err(AppError::Other(format!(
            "Skill tutorial artifact is incomplete: {}",
            directory.display()
        )));
    }

    let stored: StoredMetadata = serde_json::from_str(&std::fs::read_to_string(&metadata_path)?)
        .map_err(|error| {
            AppError::Other(format!(
                "Failed to read Skill tutorial metadata {}: {error}",
                metadata_path.display()
            ))
        })?;
    let bound = expect_bound && stored.bound && stored.identity.is_some();
    if bound {
        let identity = stored
            .identity
            .clone()
            .ok_or_else(|| AppError::Other("Bound tutorial is missing identity".to_string()))?
            .verified()?;
        if identity.key != resolved.identity.key {
            return Err(AppError::Other(
                "Stored private tutorial identity does not match the resolved Skill".to_string(),
            ));
        }
        if let Some(revision) = stored.generated_revision.clone() {
            revision.verified(&identity)?;
        }
    } else if stored.skill_name.as_deref().is_some_and(|name| {
        resolved
            .installed_name
            .as_deref()
            .is_some_and(|installed| installed != name)
    }) {
        return Err(AppError::Other(format!(
            "Skill tutorial metadata name mismatch: expected {:?}, found {:?}",
            resolved.installed_name, stored.skill_name
        )));
    }

    if stored.file_count != stored.source_files.len() {
        return Err(AppError::Other(format!(
            "Skill tutorial metadata file count mismatch: expected {}, found {} paths",
            stored.file_count,
            stored.source_files.len()
        )));
    }
    let unique_source_files = stored.source_files.iter().collect::<BTreeSet<_>>();
    if unique_source_files.len() != stored.source_files.len() {
        return Err(AppError::Other(
            "Skill tutorial metadata contains duplicate source paths".to_string(),
        ));
    }

    let current_files = inventory.to_vec();
    let content_matches = if bound {
        stored
            .generated_revision
            .as_ref()
            .is_some_and(|revision| revision.key == resolved.revision.key)
    } else {
        stored.content_hash == resolved.revision.content.content_hash
    };
    let validation_paths = if content_matches {
        if stored.source_files != current_files
            || stored.file_count != current_files.len()
            || stored.total_bytes != total_bytes
        {
            return Err(AppError::Other(
                "Skill tutorial metadata does not match the current Skill snapshot".to_string(),
            ));
        }
        &current_files
    } else {
        &stored.source_files
    };

    let html = std::fs::read_to_string(&html_path)?;
    validate_html(&html, validation_paths).map_err(|error| {
        AppError::Other(format!(
            "Stored Skill tutorial failed validation ({}): {error}",
            html_path.display()
        ))
    })?;

    let mut reasons = Vec::new();
    if !content_matches {
        reasons.push(TutorialStaleReason::ContentChanged);
    }
    if !generator.matches(&stored.prompt_version, &stored.schema_version) {
        reasons.push(TutorialStaleReason::GeneratorChanged);
    }
    let (state, stale_reason) = if reasons.is_empty() {
        (TutorialState::Fresh, None)
    } else {
        (
            TutorialState::Stale,
            if reasons.contains(&TutorialStaleReason::ContentChanged) {
                Some(TutorialStaleReason::ContentChanged)
            } else {
                reasons.first().copied()
            },
        )
    };

    Ok(PrivateTutorial {
        state,
        bound,
        html: Some(html),
        metadata: Some(to_public_metadata(stored)),
        stale_reason,
        stale_reasons: reasons,
    })
}

fn missing() -> PrivateTutorial {
    PrivateTutorial {
        state: TutorialState::Missing,
        bound: false,
        html: None,
        metadata: None,
        stale_reason: None,
        stale_reasons: Vec::new(),
    }
}

fn to_public_metadata(stored: StoredMetadata) -> PrivateTutorialMetadata {
    PrivateTutorialMetadata {
        bound: stored.bound && stored.identity.is_some(),
        identity: stored.identity,
        generated_revision: stored.generated_revision,
        skill_name: stored.skill_name,
        content_hash: stored.content_hash,
        prompt_version: stored.prompt_version,
        schema_version: stored.schema_version,
        tutorial_style: stored.tutorial_style,
        agent_label: stored.agent_label,
        generated_at: stored.generated_at,
        file_count: stored.file_count,
        total_bytes: stored.total_bytes,
        source_files: stored.source_files,
    }
}

fn identity_directory(identity: &SkillIdentity) -> PathBuf {
    skillstar_core::infra::paths::learning_tutorials_dir().join(identity.key.storage_segment())
}

fn legacy_directory(skill_name: &str) -> PathBuf {
    skillstar_core::infra::paths::tutorials_dir().join(legacy_artifact_key(skill_name))
}

pub(crate) fn legacy_artifact_key(skill_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(LEGACY_ARTIFACT_KEY_DOMAIN);
    hasher.update((skill_name.len() as u64).to_le_bytes());
    hasher.update(skill_name.as_bytes());
    hex_digest(&hasher.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

struct ArtifactIoGuard {
    _process: MutexGuard<'static, ()>,
    _file: File,
}

fn lock_artifact_io(root: &Path) -> Result<ArtifactIoGuard, AppError> {
    let process = ARTIFACT_IO_LOCK
        .lock()
        .map_err(|_| AppError::Other("Skill tutorial artifact lock is poisoned".to_string()))?;
    std::fs::create_dir_all(root)?;
    let lock_path = root.join(".artifacts.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options.open(&lock_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600))?;
    }
    file.lock()?;
    Ok(ArtifactIoGuard {
        _process: process,
        _file: file,
    })
}

fn recover_missing_artifact_directory(final_directory: &Path) -> Result<(), AppError> {
    if final_directory.exists() {
        return Ok(());
    }
    let Some(root) = final_directory.parent() else {
        return Ok(());
    };
    if !root.is_dir() {
        return Ok(());
    }
    let key = final_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Other("Tutorial artifact key is not valid UTF-8".to_string()))?;
    let prefix = format!(".{key}.");
    let mut backups = Vec::<(SystemTime, PathBuf)>::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with(&prefix) && file_name.ends_with(".bak") {
            let modified = entry
                .metadata()?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            backups.push((modified, entry.path()));
        }
    }
    backups.sort_by_key(|(modified, _)| *modified);
    if let Some((_, backup)) = backups.pop() {
        std::fs::rename(&backup, final_directory).map_err(|error| {
            AppError::Other(format!(
                "Failed to recover Skill tutorial artifact {} from {}: {error}",
                final_directory.display(),
                backup.display()
            ))
        })?;
        sync_directory(root)?;
    }
    Ok(())
}

fn replace_artifact_directory(
    final_directory: &Path,
    html: &str,
    metadata_json: &str,
) -> Result<(), AppError> {
    let root = final_directory.parent().ok_or_else(|| {
        AppError::Other(format!(
            "Tutorial artifact has no parent: {}",
            final_directory.display()
        ))
    })?;
    std::fs::create_dir_all(root)?;
    let key = final_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Other("Tutorial artifact key is not valid UTF-8".to_string()))?;
    let nonce = Uuid::new_v4();
    let staging = root.join(format!(".{key}.{nonce}.tmp"));
    let backup = root.join(format!(".{key}.{nonce}.bak"));

    std::fs::create_dir(&staging)?;
    let staged = (|| -> Result<(), AppError> {
        write_synced(&staging.join("tutorial.html"), html.as_bytes())?;
        write_synced(&staging.join("metadata.json"), metadata_json.as_bytes())?;
        sync_directory(&staging)?;
        Ok(())
    })();
    if let Err(error) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    let had_previous = final_directory.exists();
    if had_previous {
        if let Err(error) = std::fs::rename(final_directory, &backup) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(AppError::Other(format!(
                "Failed to stage the previous Skill tutorial artifact: {error}"
            )));
        }
        sync_directory(root)?;
    }
    match std::fs::rename(&staging, final_directory) {
        Ok(()) => {
            sync_directory(root)?;
            if had_previous {
                let _ = std::fs::remove_dir_all(&backup);
                sync_directory(root)?;
            }
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            let restore_error = had_previous
                .then(|| std::fs::rename(&backup, final_directory).err())
                .flatten();
            let sync_error = sync_directory(root).err();
            match restore_error {
                Some(restore_error) => Err(AppError::Other(format!(
                    "Failed to replace Skill tutorial artifact: {error}; restoring the previous artifact also failed: {restore_error}"
                ))),
                None if sync_error.is_some() => Err(AppError::Other(format!(
                    "Failed to replace Skill tutorial artifact: {error}; rollback directory sync also failed: {}",
                    sync_error.expect("checked above")
                ))),
                None => Err(AppError::Other(format!(
                    "Failed to replace Skill tutorial artifact: {error}"
                ))),
            }
        }
    }
}

fn write_synced(path: &Path, content: &[u8]) -> Result<(), AppError> {
    let mut file = File::create(path)?;
    file.write_all(content)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), AppError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), AppError> {
    Ok(())
}
