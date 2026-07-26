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

#[derive(Debug, Clone)]
pub(super) struct LoadedAuth {
    pub(super) root: Value,
    pub(super) revision: [u8; 32],
}

#[derive(Debug)]
pub(super) struct AuthCommitError {
    pub(super) message: String,
    /// The replacement completed and the expected target entry was still on
    /// disk when the later failure occurred (for example chmod verification).
    pub(super) target_installed: bool,
}

pub(super) struct AuthFileLease {
    _file: File,
}

impl AuthCommitError {
    fn before_replace(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            target_installed: false,
        }
    }

    fn after_replace(message: impl Into<String>, target_installed: bool) -> Self {
        Self {
            message: message.into(),
            target_installed,
        }
    }
}

pub(super) trait GrokAuthFile {
    fn path(&self) -> &Path;
    fn load(&self) -> Result<LoadedAuth, String>;
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
    pub(super) fn resolved() -> Result<Self, String> {
        resolve_grok_auth_path()
            .map(|path| Self { path })
            .map_err(|error| error.to_string())
    }

    pub(super) fn lock_transaction(&self) -> Result<AuthFileLease, String> {
        // Grok itself uses this exact adjacent flock while refreshing tokens.
        // Sharing it prevents refresh-token double-spend and disk overwrite.
        lock_file(&self.path.with_extension("json.lock")).map(|file| AuthFileLease { _file: file })
    }
}

impl GrokAuthFile for DiskGrokAuthFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<LoadedAuth, String> {
        let raw = read_auth_bytes(&self.path)?;
        let root = if raw.is_empty() {
            Value::Object(Map::new())
        } else {
            let value: Value = serde_json::from_slice(&raw)
                .map_err(|error| format!("{} 不是有效 JSON：{error}", self.path.display()))?;
            if !value.is_object() {
                return Err(format!("{} 的根必须是 JSON 对象", self.path.display()));
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
        let content = serde_json::to_vec_pretty(&loaded.root).map_err(|error| {
            AuthCommitError::before_replace(format!("序列化 Grok auth.json 失败：{error}"))
        })?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| AuthCommitError::before_replace("无法定位 Grok auth.json 目录"))?;
        fs::create_dir_all(parent)
            .map_err(|error| AuthCommitError::before_replace(format!("创建目录失败：{error}")))?;
        let tmp = self
            .path
            .with_extension(format!("json.skillstar-{}.tmp", std::process::id()));
        write_private_tmp(&tmp, &content).map_err(AuthCommitError::before_replace)?;
        // Reject an already-stale read before spending time on the backup.
        let current = read_auth_bytes(&self.path).map_err(AuthCommitError::before_replace)?;
        if revision(&current) != loaded.revision {
            let _ = fs::remove_file(&tmp);
            return Err(AuthCommitError::before_replace(
                "Grok auth.json 在切换期间被其他进程修改，请关闭正在运行的 Grok 后重试",
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
                "Grok auth.json 在切换期间被其他进程修改，请关闭正在运行的 Grok 后重试",
            ));
        }

        fs::rename(&tmp, &self.path).map_err(|error| {
            let _ = fs::remove_file(&tmp);
            AuthCommitError::before_replace(format!("原子替换 Grok auth.json 失败：{error}"))
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
                "Grok auth.json 写后核验缺少目标 OIDC entry",
                false,
            ));
        };
        if actual_entry != expected_entry {
            return Err(AuthCommitError::after_replace(
                "Grok auth.json 写入后被覆盖，请关闭正在运行的 Grok 后重试",
                false,
            ));
        }
        verify_private_mode(&self.path)
            .map_err(|error| AuthCommitError::after_replace(error, true))?;
        Ok(backup)
    }
}

fn read_auth_bytes(path: &Path) -> Result<Vec<u8>, String> {
    match fs::read(path) {
        Ok(raw) => Ok(raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("读取 {} 失败：{error}", path.display())),
    }
}

fn target_entry_equals(path: &Path, scope: &str, expected_entry: &Value) -> bool {
    fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        .and_then(|root| root.get(scope).cloned())
        .is_some_and(|actual| actual == *expected_entry)
}

fn lock_file(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("打开凭据文件锁失败：{error}"))?;
    set_private_mode(path)?;
    file.lock()
        .map_err(|error| format!("锁定凭据文件失败：{error}"))?;
    // Grok's stale-lock recovery reads `PID:unix_seconds`. Updating the holder
    // after flock acquisition prevents it from mistaking an old dead owner for
    // this live SkillStar lease and recreating the lock inode underneath us.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    file.set_len(0)
        .map_err(|error| format!("清空凭据锁 holder 失败：{error}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("定位凭据锁 holder 失败：{error}"))?;
    write!(file, "{}:{timestamp}", std::process::id())
        .map_err(|error| format!("写入凭据锁 holder 失败：{error}"))?;
    file.sync_data()
        .map_err(|error| format!("同步凭据锁 holder 失败：{error}"))?;
    Ok(file)
}

pub(super) fn revision(raw: &[u8]) -> [u8; 32] {
    Sha256::digest(raw).into()
}

fn write_private_tmp(path: &Path, content: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("创建临时凭据文件失败：{error}"))?;
    file.write_all(content)
        .map_err(|error| format!("写入临时凭据文件失败：{error}"))?;
    file.sync_all()
        .map_err(|error| format!("同步临时凭据文件失败：{error}"))?;
    set_private_mode(path)
}

fn set_private_mode(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("设置 {} 权限为 0600 失败：{error}", path.display()))?;
    }
    let _ = path;
    Ok(())
}

fn verify_private_mode(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|error| format!("读取 {} 权限失败：{error}", path.display()))?
            .permissions()
            .mode()
            & 0o777;
        if mode != 0o600 {
            return Err(format!("{} 权限应为 0600，实际为 {mode:o}", path.display()));
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
    fn load(&self, subscription_id: &str) -> Result<Option<Value>, String>;
    fn save(&mut self, subscription_id: &str, entry: &Value) -> Result<(), String>;
    fn remove(&mut self, subscription_id: &str) -> Result<(), String>;
}

pub(super) struct DiskGrokSessionStore {
    path: PathBuf,
}

impl DiskGrokSessionStore {
    pub(super) fn open_default() -> Result<Self, String> {
        let store = Self {
            path: skillstar_core::infra::paths::config_dir()
                .join("usage")
                .join("grok_cli_sessions.json"),
        };
        let _guard = store.lock()?;
        store.read_unlocked()?;
        Ok(store)
    }

    fn lock(&self) -> Result<File, String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "无法定位 Grok session store 目录".to_string())?;
        fs::create_dir_all(parent).map_err(|error| format!("创建目录失败：{error}"))?;
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
            .map_err(|error| format!("打开 Grok session store 锁失败：{error}"))?;
        set_private_mode(&lock_path)?;
        file.lock()
            .map_err(|error| format!("锁定 Grok session store 失败：{error}"))?;
        Ok(file)
    }

    fn read_unlocked(&self) -> Result<StoredSessions, String> {
        match fs::read(&self.path) {
            Ok(raw) if raw.is_empty() => Ok(StoredSessions::default()),
            Ok(raw) => serde_json::from_slice(&raw)
                .map_err(|error| format!("{} 不是有效 JSON：{error}", self.path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(StoredSessions::default())
            }
            Err(error) => Err(format!("读取 {} 失败：{error}", self.path.display())),
        }
    }

    fn flush_unlocked(&self, sessions: &StoredSessions) -> Result<(), String> {
        let content = serde_json::to_vec_pretty(sessions)
            .map_err(|error| format!("序列化 Grok session store 失败：{error}"))?;
        let tmp = self
            .path
            .with_extension(format!("json.skillstar-{}.tmp", std::process::id()));
        write_private_tmp(&tmp, &content)?;
        fs::rename(&tmp, &self.path).map_err(|error| {
            let _ = fs::remove_file(&tmp);
            format!("原子替换 Grok session store 失败：{error}")
        })?;
        set_private_mode(&self.path)
    }
}

impl GrokSessionStore for DiskGrokSessionStore {
    fn load(&self, subscription_id: &str) -> Result<Option<Value>, String> {
        let _guard = self.lock()?;
        let sessions = self.read_unlocked()?;
        let Some(encrypted) = sessions.entries.get(subscription_id) else {
            return Ok(None);
        };
        let raw = crypto::decrypt(encrypted);
        if raw.is_empty() {
            return Err(format!(
                "Grok 账号 {subscription_id} 的 session snapshot 无法解密"
            ));
        }
        let entry: Value = serde_json::from_str(&raw).map_err(|error| {
            format!("Grok 账号 {subscription_id} 的 session snapshot 损坏：{error}")
        })?;
        if !entry.is_object() {
            return Err(format!(
                "Grok 账号 {subscription_id} 的 session snapshot 不是 JSON 对象"
            ));
        }
        Ok(Some(entry))
    }

    fn save(&mut self, subscription_id: &str, entry: &Value) -> Result<(), String> {
        let _guard = self.lock()?;
        let mut sessions = self.read_unlocked()?;
        let raw = serde_json::to_string(entry)
            .map_err(|error| format!("序列化 Grok session snapshot 失败：{error}"))?;
        sessions
            .entries
            .insert(subscription_id.to_string(), crypto::encrypt(&raw));
        self.flush_unlocked(&sessions)
    }

    fn remove(&mut self, subscription_id: &str) -> Result<(), String> {
        let _guard = self.lock()?;
        let mut sessions = self.read_unlocked()?;
        if sessions.entries.remove(subscription_id).is_some() {
            self.flush_unlocked(&sessions)?;
        }
        Ok(())
    }
}
