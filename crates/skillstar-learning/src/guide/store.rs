//! Atomic Guide progress and Draft persistence under `~/.skillstar/learning/`.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use uuid::Uuid;

use super::{
    ConversionPreview, GuideDraft, GuideId, GuideRevisionKey, LearningProgress, ProgressSnapshot,
    draft_revision_key,
};

static IO_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProgress {
    guide_id: GuideId,
    guide_revision_key: GuideRevisionKey,
    current_step_id: String,
    completed_step_ids: Vec<String>,
    updated_at: String,
}

pub fn load_progress(
    guide_id: &GuideId,
    guide_revision: &GuideRevisionKey,
) -> Result<ProgressSnapshot, AppError> {
    let root = progress_dir(guide_id);
    let _guard = lock_io(&root)?;
    recover_json(&current_progress_path(guide_id, guide_revision))?;
    let current = read_progress(&current_progress_path(guide_id, guide_revision))?;
    let stale = latest_other_progress(&root, guide_id, guide_revision)?;
    Ok(ProgressSnapshot { current, stale })
}

pub fn save_progress(progress: &LearningProgress) -> Result<LearningProgress, AppError> {
    let mut stored = StoredProgress {
        guide_id: progress.guide_id.clone(),
        guide_revision_key: progress.guide_revision_key.clone(),
        current_step_id: progress.current_step_id.clone(),
        completed_step_ids: unique_keep_order(&progress.completed_step_ids),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let root = progress_dir(&stored.guide_id);
    let path = current_progress_path(&stored.guide_id, &stored.guide_revision_key);
    let _guard = lock_io(&root)?;
    recover_json(&path)?;
    let json = serde_json::to_string_pretty(&stored)?;
    replace_json(&path, json.as_bytes())?;
    Ok(LearningProgress {
        guide_id: stored.guide_id,
        guide_revision_key: stored.guide_revision_key,
        current_step_id: std::mem::take(&mut stored.current_step_id),
        completed_step_ids: std::mem::take(&mut stored.completed_step_ids),
        updated_at: stored.updated_at,
    })
}

pub fn commit_draft(preview: ConversionPreview) -> Result<GuideDraft, AppError> {
    let converted_at = chrono::Utc::now().to_rfc3339();
    let mut draft = preview.into_draft(converted_at, String::new());
    draft.revision_key = draft_revision_key(&draft);
    let root = skillstar_core::infra::paths::learning_drafts_dir()
        .join(&draft.source_tutorial_key);
    let path = root.join(format!("{}.json", draft.revision_key.replace(':', "-")));
    let _guard = lock_io(&root)?;
    if path.exists() {
        return Err(AppError::Other(
            "A Guide Draft with this revision already exists; conversion did not overwrite it"
                .to_string(),
        ));
    }
    let json = serde_json::to_string_pretty(&draft)?;
    replace_json(&path, json.as_bytes())?;
    Ok(draft)
}

pub fn list_drafts() -> Result<Vec<GuideDraft>, AppError> {
    let root = skillstar_core::infra::paths::learning_drafts_dir();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let _guard = lock_io(&root)?;
    let mut drafts = Vec::new();
    for identity in read_dirs(&root)? {
        for path in read_json_files(&identity)? {
            recover_json(&path)?;
            if let Some(draft) = read_draft(&path)? {
                drafts.push(draft);
            }
        }
    }
    drafts.sort_by(|a, b| b.converted_at.cmp(&a.converted_at));
    Ok(drafts)
}

fn progress_dir(guide_id: &GuideId) -> PathBuf {
    skillstar_core::infra::paths::learning_progress_dir().join(guide_id.storage_segment())
}

fn current_progress_path(guide_id: &GuideId, revision: &GuideRevisionKey) -> PathBuf {
    progress_dir(guide_id).join(format!("{}.json", revision.storage_segment()))
}

fn read_progress(path: &Path) -> Result<Option<LearningProgress>, AppError> {
    if !path.is_file() {
        return Ok(None);
    }
    let stored: StoredProgress = serde_json::from_str(&std::fs::read_to_string(path)?).map_err(
        |error| {
            AppError::Other(format!(
                "Failed to read learning progress {}: {error}",
                path.display()
            ))
        },
    )?;
    Ok(Some(LearningProgress {
        guide_id: stored.guide_id,
        guide_revision_key: stored.guide_revision_key,
        current_step_id: stored.current_step_id,
        completed_step_ids: stored.completed_step_ids,
        updated_at: stored.updated_at,
    }))
}

fn latest_other_progress(
    root: &Path,
    guide_id: &GuideId,
    current: &GuideRevisionKey,
) -> Result<Option<LearningProgress>, AppError> {
    if !root.is_dir() {
        return Ok(None);
    }
    let skip = format!("{}.json", current.storage_segment());
    let mut newest: Option<(String, LearningProgress)> = None;
    for path in read_json_files(root)? {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name == skip || name.starts_with('.') {
            continue;
        }
        if let Some(progress) = read_progress(&path)?
            && progress.guide_id == *guide_id
        {
            let stamp = progress.updated_at.clone();
            if newest
                .as_ref()
                .is_none_or(|(existing, _)| stamp > *existing)
            {
                newest = Some((stamp, progress));
            }
        }
    }
    Ok(newest.map(|(_, progress)| progress))
}

fn read_draft(path: &Path) -> Result<Option<GuideDraft>, AppError> {
    if !path.is_file() {
        return Ok(None);
    }
    let draft: GuideDraft = serde_json::from_str(&std::fs::read_to_string(path)?).map_err(
        |error| {
            AppError::Other(format!(
                "Failed to read Guide Draft {}: {error}",
                path.display()
            ))
        },
    )?;
    Ok(Some(draft))
}

fn unique_keep_order(ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            out.push(id.clone());
        }
    }
    out
}

struct IoGuard {
    _process: MutexGuard<'static, ()>,
    _file: File,
}

fn lock_io(root: &Path) -> Result<IoGuard, AppError> {
    let process = IO_LOCK
        .lock()
        .map_err(|_| AppError::Other("Learning artifact lock is poisoned".to_string()))?;
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
    Ok(IoGuard {
        _process: process,
        _file: file,
    })
}

fn recover_json(final_path: &Path) -> Result<(), AppError> {
    if final_path.exists() {
        return Ok(());
    }
    let Some(root) = final_path.parent() else {
        return Ok(());
    };
    if !root.is_dir() {
        return Ok(());
    }
    let key = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Other("Learning artifact name is not valid UTF-8".to_string()))?;
    let prefix = format!(".{key}.");
    let mut backups = Vec::<(std::time::SystemTime, PathBuf)>::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
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
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            backups.push((modified, entry.path()));
        }
    }
    backups.sort_by_key(|(modified, _)| *modified);
    if let Some((_, backup)) = backups.pop() {
        std::fs::rename(&backup, final_path)?;
        sync_dir(root)?;
    }
    Ok(())
}

fn replace_json(final_path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let root = final_path.parent().ok_or_else(|| {
        AppError::Other(format!(
            "Learning artifact has no parent: {}",
            final_path.display()
        ))
    })?;
    std::fs::create_dir_all(root)?;
    let key = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::Other("Learning artifact name is not valid UTF-8".to_string()))?;
    let nonce = Uuid::new_v4();
    let staging = root.join(format!(".{key}.{nonce}.tmp"));
    let backup = root.join(format!(".{key}.{nonce}.bak"));
    write_synced(&staging, bytes)?;
    if final_path.exists() {
        std::fs::rename(final_path, &backup)?;
    }
    if let Err(error) = std::fs::rename(&staging, final_path) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, final_path);
        }
        let _ = std::fs::remove_file(&staging);
        return Err(AppError::Other(format!(
            "Failed to replace {}: {error}",
            final_path.display()
        )));
    }
    if backup.exists() {
        let _ = std::fs::remove_file(&backup);
    }
    sync_dir(root)?;
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn sync_dir(path: &Path) -> Result<(), AppError> {
    let file = File::open(path)?;
    file.sync_all()?;
    Ok(())
}

fn read_dirs(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    Ok(dirs)
}

fn read_json_files(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    if !root.is_dir() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                files.push(path);
            }
        }
    }
    Ok(files)
}
