use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use skillstar_models::tool_sync::{create_rolling_backup, resolve_grok_auth_path};
use skillstar_usage::crypto;

use super::error::{AuthCommitError, AuthCommitReason, GrokFile, GrokIoError};

#[derive(Debug, Clone)]
pub(super) struct LoadedAuth {
    pub(super) root: Value,
    pub(super) revision: [u8; 32],
}

pub(super) struct AuthFileLease {
    _file: File,
}

pub(super) trait GrokAuthFile {
    fn path(&self) -> &Path;
    fn load(&self) -> Result<LoadedAuth, GrokIoError>;
    fn commit_verified(
        &self,
        loaded: &LoadedAuth,
        scope: &str,
        expected_entry: &Value,
    ) -> Result<Option<PathBuf>, AuthCommitError>;
}

#[derive(Debug)]
pub(super) struct DiskGrokAuthFile {
    pub(super) path: PathBuf,
}

impl DiskGrokAuthFile {
    pub(super) fn resolved() -> Result<Self, GrokIoError> {
        resolve_grok_auth_path()
            .map(|path| Self { path })
            .map_err(|error| GrokIoError::ResolveAuthPath(error.to_string()))
    }

    pub(super) fn lock_transaction(&self) -> Result<AuthFileLease, GrokIoError> {
        // Grok itself uses this exact adjacent flock while refreshing tokens.
        // Sharing it prevents refresh-token double-spend and disk overwrite.
        lock_file(&self.path.with_extension("json.lock"), GrokFile::Auth)
            .map(|file| AuthFileLease { _file: file })
    }
}

impl GrokAuthFile for DiskGrokAuthFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<LoadedAuth, GrokIoError> {
        let raw = read_auth_bytes(&self.path)?;
        let root = if raw.is_empty() {
            Value::Object(Map::new())
        } else {
            let value: Value =
                serde_json::from_slice(&raw).map_err(|source| GrokIoError::InvalidJson {
                    path: self.path.clone(),
                    source,
                })?;
            if !value.is_object() {
                return Err(GrokIoError::NotAnObject {
                    path: self.path.clone(),
                });
            }
            value
        };
        Ok(LoadedAuth {
            root,
            revision: revision(&raw),
        })
    }

    fn commit_verified(
        &self,
        loaded: &LoadedAuth,
        scope: &str,
        expected_entry: &Value,
    ) -> Result<Option<PathBuf>, AuthCommitError> {
        let content = serde_json::to_vec_pretty(&loaded.root).map_err(|source| {
            AuthCommitError::before_replace(GrokIoError::Serialize {
                file: GrokFile::Auth,
                source,
            })
        })?;
        let parent = self.path.parent().ok_or_else(|| {
            AuthCommitError::before_replace(GrokIoError::MissingParentDir {
                file: GrokFile::Auth,
            })
        })?;
        fs::create_dir_all(parent)
            .map_err(|source| AuthCommitError::before_replace(GrokIoError::CreateDir { source }))?;
        let tmp = self
            .path
            .with_extension(format!("json.skillstar-{}.tmp", std::process::id()));
        write_private_tmp(&tmp, &content).map_err(AuthCommitError::before_replace)?;
        // Reject an already-stale read before spending time on the backup.
        let current = read_auth_bytes(&self.path).map_err(AuthCommitError::before_replace)?;
        if revision(&current) != loaded.revision {
            let _ = fs::remove_file(&tmp);
            return Err(AuthCommitError::before_replace(
                AuthCommitReason::ConcurrentModification,
            ));
        }

        let backup = if self.path.exists() {
            match create_rolling_backup(&self.path) {
                Ok(path) => {
                    if let Err(error) = set_private_mode(&path) {
                        let _ = fs::remove_file(&path);
                        let _ = fs::remove_file(&tmp);
                        return Err(AuthCommitError::before_replace(error));
                    }
                    Some(path)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        // Backup creation is the slow portion. Re-read immediately before the
        // atomic replace so the remaining optimistic-check window is minimal.
        let current = read_auth_bytes(&self.path).map_err(|error| {
            let _ = fs::remove_file(&tmp);
            AuthCommitError::before_replace(error)
        })?;
        if revision(&current) != loaded.revision {
            let _ = fs::remove_file(&tmp);
            return Err(AuthCommitError::before_replace(
                AuthCommitReason::ConcurrentModification,
            ));
        }

        fs::rename(&tmp, &self.path).map_err(|source| {
            let _ = fs::remove_file(&tmp);
            AuthCommitError::before_replace(GrokIoError::Replace {
                file: GrokFile::Auth,
                source,
            })
        })?;
        set_private_mode(&self.path).map_err(|error| {
            AuthCommitError::after_replace(
                error,
                target_entry_equals(&self.path, scope, expected_entry),
            )
        })?;

        let verified = self.load().map_err(|error| {
            AuthCommitError::after_replace(
                error,
                target_entry_equals(&self.path, scope, expected_entry),
            )
        })?;
        let Some(actual_entry) = verified.root.get(scope) else {
            return Err(AuthCommitError::after_replace(
                AuthCommitReason::MissingVerifiedEntry,
                false,
            ));
        };
        if actual_entry != expected_entry {
            return Err(AuthCommitError::after_replace(
                AuthCommitReason::OverwrittenAfterCommit,
                false,
            ));
        }
        verify_private_mode(&self.path)
            .map_err(|error| AuthCommitError::after_replace(error, true))?;
        Ok(backup)
    }
}

fn read_auth_bytes(path: &Path) -> Result<Vec<u8>, GrokIoError> {
    match fs::read(path) {
        Ok(raw) => Ok(raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(GrokIoError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn target_entry_equals(path: &Path, scope: &str, expected_entry: &Value) -> bool {
    fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|root| root.get(scope).cloned())
        .is_some_and(|actual| actual == *expected_entry)
}

fn lock_file(path: &Path, file_kind: GrokFile) -> Result<File, GrokIoError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| GrokIoError::OpenLock {
        file: file_kind,
        source,
    })?;
    set_private_mode(path)?;
    file.lock().map_err(|source| GrokIoError::AcquireLock {
        file: file_kind,
        source,
    })?;
    // Grok's stale-lock recovery reads `PID:unix_seconds`. Updating the holder
    // after flock acquisition prevents it from mistaking an old dead owner for
    // this live SkillStar lease and recreating the lock inode underneath us.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    file.set_len(0).map_err(|source| GrokIoError::LockHolder {
        action: "清空",
        source,
    })?;
    file.seek(SeekFrom::Start(0))
        .map_err(|source| GrokIoError::LockHolder {
            action: "定位",
            source,
        })?;
    write!(file, "{}:{timestamp}", std::process::id()).map_err(|source| {
        GrokIoError::LockHolder {
            action: "写入",
            source,
        }
    })?;
    file.sync_data().map_err(|source| GrokIoError::LockHolder {
        action: "同步",
        source,
    })?;
    Ok(file)
}

pub(super) fn revision(raw: &[u8]) -> [u8; 32] {
    Sha256::digest(raw).into()
}

fn write_private_tmp(path: &Path, content: &[u8]) -> Result<(), GrokIoError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| GrokIoError::TempFile {
        action: "创建",
        source,
    })?;
    file.write_all(content)
        .map_err(|source| GrokIoError::TempFile {
            action: "写入",
            source,
        })?;
    file.sync_all().map_err(|source| GrokIoError::TempFile {
        action: "同步",
        source,
    })?;
    set_private_mode(path)
}

fn set_private_mode(path: &Path) -> Result<(), GrokIoError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
            GrokIoError::SetMode {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    let _ = path;
    Ok(())
}

fn verify_private_mode(path: &Path) -> Result<(), GrokIoError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|source| GrokIoError::ReadMode {
                path: path.to_path_buf(),
                source,
            })?
            .permissions()
            .mode()
            & 0o777;
        if mode != 0o600 {
            return Err(GrokIoError::UnexpectedMode {
                path: path.to_path_buf(),
                mode,
            });
        }
    }
    let _ = path;
    Ok(())
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct StoredSessions {
    #[serde(default)]
    pub(super) entries: HashMap<String, String>,
}

pub(super) trait GrokSessionStore {
    fn load(&self, subscription_id: &str) -> Result<Option<Value>, GrokIoError>;
    fn save(&mut self, subscription_id: &str, entry: &Value) -> Result<(), GrokIoError>;
    fn remove(&mut self, subscription_id: &str) -> Result<(), GrokIoError>;
}

pub(super) struct DiskGrokSessionStore {
    path: PathBuf,
}

impl DiskGrokSessionStore {
    pub(super) fn open_default() -> Result<Self, GrokIoError> {
        let store = Self {
            path: skillstar_core::infra::paths::config_dir()
                .join("usage")
                .join("grok_cli_sessions.json"),
        };
        let _guard = store.lock()?;
        store.read_unlocked()?;
        Ok(store)
    }

    fn lock(&self) -> Result<File, GrokIoError> {
        let parent = self.path.parent().ok_or(GrokIoError::MissingParentDir {
            file: GrokFile::SessionStore,
        })?;
        fs::create_dir_all(parent).map_err(|source| GrokIoError::CreateDir { source })?;
        let lock_path = self.path.with_extension("json.lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&lock_path)
            .map_err(|source| GrokIoError::OpenLock {
                file: GrokFile::SessionStore,
                source,
            })?;
        set_private_mode(&lock_path)?;
        file.lock().map_err(|source| GrokIoError::AcquireLock {
            file: GrokFile::SessionStore,
            source,
        })?;
        Ok(file)
    }

    fn read_unlocked(&self) -> Result<StoredSessions, GrokIoError> {
        match fs::read(&self.path) {
            Ok(raw) if raw.is_empty() => Ok(StoredSessions::default()),
            Ok(raw) => serde_json::from_slice(&raw).map_err(|source| GrokIoError::InvalidJson {
                path: self.path.clone(),
                source,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(StoredSessions::default())
            }
            Err(source) => Err(GrokIoError::Read {
                path: self.path.clone(),
                source,
            }),
        }
    }

    fn flush_unlocked(&self, sessions: &StoredSessions) -> Result<(), GrokIoError> {
        let content =
            serde_json::to_vec_pretty(sessions).map_err(|source| GrokIoError::Serialize {
                file: GrokFile::SessionStore,
                source,
            })?;
        let tmp = self
            .path
            .with_extension(format!("json.skillstar-{}.tmp", std::process::id()));
        write_private_tmp(&tmp, &content)?;
        fs::rename(&tmp, &self.path).map_err(|source| {
            let _ = fs::remove_file(&tmp);
            GrokIoError::Replace {
                file: GrokFile::SessionStore,
                source,
            }
        })?;
        set_private_mode(&self.path)
    }
}

impl GrokSessionStore for DiskGrokSessionStore {
    fn load(&self, subscription_id: &str) -> Result<Option<Value>, GrokIoError> {
        let _guard = self.lock()?;
        let sessions = self.read_unlocked()?;
        let Some(encrypted) = sessions.entries.get(subscription_id) else {
            return Ok(None);
        };
        let raw = crypto::decrypt(encrypted);
        if raw.is_empty() {
            return Err(GrokIoError::SnapshotUndecryptable {
                subscription_id: subscription_id.to_string(),
            });
        }
        let entry: Value =
            serde_json::from_str(&raw).map_err(|source| GrokIoError::SnapshotCorrupt {
                subscription_id: subscription_id.to_string(),
                source,
            })?;
        if !entry.is_object() {
            return Err(GrokIoError::SnapshotNotAnObject {
                subscription_id: subscription_id.to_string(),
            });
        }
        Ok(Some(entry))
    }

    fn save(&mut self, subscription_id: &str, entry: &Value) -> Result<(), GrokIoError> {
        let _guard = self.lock()?;
        let mut sessions = self.read_unlocked()?;
        let raw = serde_json::to_string(entry).map_err(|source| GrokIoError::Serialize {
            file: GrokFile::SessionSnapshot,
            source,
        })?;
        sessions
            .entries
            .insert(subscription_id.to_string(), crypto::encrypt(&raw));
        self.flush_unlocked(&sessions)
    }

    fn remove(&mut self, subscription_id: &str) -> Result<(), GrokIoError> {
        let _guard = self.lock()?;
        let mut sessions = self.read_unlocked()?;
        if sessions.entries.remove(subscription_id).is_some() {
            self.flush_unlocked(&sessions)?;
        }
        Ok(())
    }
}
