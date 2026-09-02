//! Durable UUID identity for locally authored Skills.
//!
//! The sidecar lives at `<local>/<name>/.skillstar/identity.json` and is
//! excluded from v2 content snapshots. External adopt/bundle copies must not
//! inherit a source sidecar; managed rename/move must keep the same UUID.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use uuid::Uuid;

pub const SIDECAR_DIR_NAME: &str = ".skillstar";
pub const SIDECAR_FILE_NAME: &str = "identity.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalSkillIdentityRecord {
    pub schema_version: u32,
    pub local_skill_id: Uuid,
}

pub fn sidecar_dir(skill_dir: &Path) -> PathBuf {
    skill_dir.join(SIDECAR_DIR_NAME)
}

pub fn sidecar_path(skill_dir: &Path) -> PathBuf {
    sidecar_dir(skill_dir).join(SIDECAR_FILE_NAME)
}

/// Drop a sidecar copied or moved from an untrusted tree, then mint a new id.
pub fn replace_untrusted_sidecar(skill_dir: &Path) -> Result<Uuid, AppError> {
    let sidecar = sidecar_dir(skill_dir);
    if sidecar.symlink_metadata().is_ok() {
        std::fs::remove_dir_all(&sidecar).map_err(|error| {
            AppError::Other(format!(
                "Failed to discard untrusted local Skill identity at {}: {error}",
                sidecar.display()
            ))
        })?;
    }
    ensure_local_identity(skill_dir)
}

/// Read the sidecar without minting. `Ok(None)` means the file is absent.
/// Corrupt or colliding sidecars fail closed and are left untouched.
pub fn read_local_identity(skill_dir: &Path) -> Result<Option<Uuid>, AppError> {
    let path = sidecar_path(skill_dir);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(load_existing(&path, skill_dir)?))
}

/// Return the existing UUID, or atomically mint one if the sidecar is missing.
pub fn ensure_local_identity(skill_dir: &Path) -> Result<Uuid, AppError> {
    if !skill_dir.is_dir() {
        return Err(AppError::Other(format!(
            "Local Skill directory is missing: {}",
            skill_dir.display()
        )));
    }
    let sidecar = sidecar_dir(skill_dir);
    std::fs::create_dir_all(&sidecar)?;
    let _lock = lock_sidecar(&sidecar)?;
    let path = sidecar_path(skill_dir);
    if path.is_file() {
        return load_existing(&path, skill_dir);
    }

    let local_skill_id = Uuid::new_v4();
    reject_duplicate(skill_dir, local_skill_id)?;
    let record = LocalSkillIdentityRecord {
        schema_version: SCHEMA_VERSION,
        local_skill_id,
    };
    let json = serde_json::to_string_pretty(&record)?;
    atomic_write_sidecar(&path, json.as_bytes())?;
    Ok(local_skill_id)
}

fn load_existing(path: &Path, skill_dir: &Path) -> Result<Uuid, AppError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        AppError::Other(format!(
            "Failed to read local Skill identity {}: {error}",
            path.display()
        ))
    })?;
    let record: LocalSkillIdentityRecord = serde_json::from_str(&raw).map_err(|error| {
        AppError::Other(format!(
            "Local Skill identity sidecar is corrupt {}: {error}",
            path.display()
        ))
    })?;
    if record.schema_version != SCHEMA_VERSION {
        return Err(AppError::Other(format!(
            "Unsupported local Skill identity schema {} in {}",
            record.schema_version,
            path.display()
        )));
    }
    if record.local_skill_id.is_nil() {
        return Err(AppError::Other(format!(
            "Local Skill identity sidecar has a nil UUID: {}",
            path.display()
        )));
    }
    reject_duplicate(skill_dir, record.local_skill_id)?;
    Ok(record.local_skill_id)
}

fn reject_duplicate(skill_dir: &Path, local_skill_id: Uuid) -> Result<(), AppError> {
    let Some(parent) = skill_dir.parent() else {
        return Ok(());
    };
    if !parent.is_dir() {
        return Ok(());
    }
    let self_canonical = std::fs::canonicalize(skill_dir).ok();
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if self_canonical
            .as_ref()
            .zip(std::fs::canonicalize(&path).ok())
            .is_some_and(|(left, right)| left == &right)
        {
            continue;
        }
        let other = sidecar_path(&path);
        if !other.is_file() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&other) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<LocalSkillIdentityRecord>(&raw) else {
            continue;
        };
        if record.local_skill_id == local_skill_id {
            return Err(AppError::Other(format!(
                "Local Skill identity {local_skill_id} is duplicated at {} and {}",
                skill_dir.display(),
                path.display()
            )));
        }
    }
    Ok(())
}

struct SidecarLock {
    _file: File,
}

fn lock_sidecar(sidecar_dir: &Path) -> Result<SidecarLock, AppError> {
    let lock_path = sidecar_dir.join("identity.lock");
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
    file.lock().map_err(|error| {
        AppError::Other(format!(
            "Failed to lock local Skill identity {}: {error}",
            lock_path.display()
        ))
    })?;
    Ok(SidecarLock { _file: file })
}

fn atomic_write_sidecar(path: &Path, content: &[u8]) -> Result<(), AppError> {
    skillstar_core::infra::fs_ops::atomic_write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    struct Sandbox {
        previous: Vec<(&'static str, Option<OsString>)>,
        temp: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Sandbox {
        fn new() -> Self {
            let _guard = crate::lock_test_env();
            let temp = tempfile::tempdir().unwrap();
            let overrides = [
                ("SKILLSTAR_HUB_DIR", temp.path().join("hub")),
                ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
                ("HOME", temp.path().join("home")),
                ("USERPROFILE", temp.path().join("home")),
            ];
            let previous = overrides
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in &overrides {
                    std::env::set_var(key, value);
                }
            }
            Self {
                previous,
                temp,
                _guard,
            }
        }

        fn skill_dir(&self, name: &str) -> PathBuf {
            let dir = skillstar_core::infra::paths::local_skills_dir().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            unsafe {
                for (key, previous) in self.previous.drain(..).rev() {
                    match previous {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    #[test]
    fn ensure_mints_a_stable_uuid_and_ignores_it_in_snapshots() {
        let sandbox = Sandbox::new();
        let dir = sandbox.skill_dir("demo");
        std::fs::write(
            dir.join("SKILL.md"),
            "---\ndescription: demo\n---\n# demo\n",
        )
        .unwrap();
        let first = ensure_local_identity(&dir).unwrap();
        let second = ensure_local_identity(&dir).unwrap();
        assert_eq!(first, second);
        assert!(sidecar_path(&dir).is_file());
        assert_eq!(
            read_local_identity(&dir).unwrap(),
            Some(first),
            "ensure must be idempotent"
        );
    }

    #[test]
    fn rename_keeps_identity_copy_mints_a_new_one() {
        let sandbox = Sandbox::new();
        let original = sandbox.skill_dir("alpha");
        let id = ensure_local_identity(&original).unwrap();
        let renamed = skillstar_core::infra::paths::local_skills_dir().join("alpha-renamed");
        std::fs::rename(&original, &renamed).unwrap();
        assert_eq!(read_local_identity(&renamed).unwrap(), Some(id));

        let copy = sandbox.skill_dir("alpha-copy");
        std::fs::write(copy.join("SKILL.md"), "# copy\n").unwrap();
        let copied = replace_untrusted_sidecar(&copy).unwrap();
        assert_ne!(copied, id);
    }

    #[test]
    fn untrusted_source_sidecar_is_discarded() {
        let sandbox = Sandbox::new();
        let source = sandbox.temp.path().join("external");
        std::fs::create_dir_all(source.join(".skillstar")).unwrap();
        let injected = Uuid::new_v4();
        std::fs::write(
            source.join(".skillstar/identity.json"),
            serde_json::to_string(&LocalSkillIdentityRecord {
                schema_version: SCHEMA_VERSION,
                local_skill_id: injected,
            })
            .unwrap(),
        )
        .unwrap();
        let adopted = sandbox.skill_dir("adopted");
        std::fs::create_dir_all(adopted.join(".skillstar")).unwrap();
        std::fs::copy(
            source.join(".skillstar/identity.json"),
            adopted.join(".skillstar/identity.json"),
        )
        .unwrap();
        let minted = replace_untrusted_sidecar(&adopted).unwrap();
        assert_ne!(minted, injected);
    }

    #[test]
    fn corrupt_and_duplicate_sidecars_fail_closed() {
        let sandbox = Sandbox::new();
        let first = sandbox.skill_dir("one");
        let id = ensure_local_identity(&first).unwrap();
        let broken = sandbox.skill_dir("broken");
        std::fs::create_dir_all(sidecar_dir(&broken)).unwrap();
        let broken_path = sidecar_path(&broken);
        std::fs::write(&broken_path, "{not json").unwrap();
        assert!(read_local_identity(&broken).is_err());
        assert_eq!(std::fs::read_to_string(&broken_path).unwrap(), "{not json");

        let twin = sandbox.skill_dir("two");
        std::fs::create_dir_all(sidecar_dir(&twin)).unwrap();
        std::fs::write(
            sidecar_path(&twin),
            serde_json::to_string_pretty(&LocalSkillIdentityRecord {
                schema_version: SCHEMA_VERSION,
                local_skill_id: id,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(read_local_identity(&twin).is_err());
        assert!(sidecar_path(&twin).is_file());
    }
}
