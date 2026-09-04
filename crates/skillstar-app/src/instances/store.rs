//! Persist the desktop-instance registry next to other user config.

use super::apps::DesktopAppId;
use super::error::InstanceError;
use serde::{Deserialize, Serialize};
use skillstar_core::infra::{fs_ops, paths};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredInstance {
    pub id: String,
    pub app: DesktopAppId,
    pub name: String,
    #[serde(default)]
    pub extra_args: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoreFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    instances: Vec<StoredInstance>,
}

fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn load_unlocked() -> Result<StoreFile, InstanceError> {
    let path = paths::app_instances_config_path();
    if !path.exists() {
        return Ok(StoreFile {
            version: 1,
            instances: Vec::new(),
        });
    }
    let bytes = std::fs::read(&path)?;
    let parsed: StoreFile = serde_json::from_slice(&bytes)
        .map_err(|e| InstanceError::Other(format!("无法读取实例清单: {e}")))?;
    Ok(parsed)
}

fn save_unlocked(file: &StoreFile) -> Result<(), InstanceError> {
    let path = paths::app_instances_config_path();
    let mut out = file.clone();
    out.version = 1;
    let bytes = serde_json::to_vec_pretty(&out)
        .map_err(|e| InstanceError::Other(format!("无法写入实例清单: {e}")))?;
    fs_ops::atomic_write(&path, &bytes)?;
    Ok(())
}

pub fn list_stored(app: Option<DesktopAppId>) -> Result<Vec<StoredInstance>, InstanceError> {
    let _guard = store_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let file = load_unlocked()?;
    Ok(file
        .instances
        .into_iter()
        .filter(|row| app.is_none_or(|wanted| row.app == wanted))
        .collect())
}

pub fn get_stored(id: &str) -> Result<StoredInstance, InstanceError> {
    list_stored(None)?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| InstanceError::NotFound(id.to_string()))
}

pub fn profile_dir(app: DesktopAppId, id: &str) -> Result<PathBuf, InstanceError> {
    if !is_safe_segment(app.as_str()) || !is_safe_segment(id) {
        return Err(InstanceError::Other("非法的实例路径".to_string()));
    }
    Ok(paths::instance_profile_dir(app.as_str(), id))
}

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn create_stored(app: DesktopAppId, name: String) -> Result<StoredInstance, InstanceError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(InstanceError::EmptyName);
    }
    if name.chars().count() > 64 {
        return Err(InstanceError::Other("实例名称过长".to_string()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let dir = profile_dir(app, &id)?;
    std::fs::create_dir_all(&dir)?;
    let row = StoredInstance {
        id,
        app,
        name,
        extra_args: Vec::new(),
        created_at: chrono::Utc::now().timestamp(),
    };
    let _guard = store_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = load_unlocked()?;
    file.instances.push(row.clone());
    save_unlocked(&file)?;
    Ok(row)
}

pub fn delete_stored(id: &str) -> Result<StoredInstance, InstanceError> {
    let _guard = store_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = load_unlocked()?;
    let index = file
        .instances
        .iter()
        .position(|row| row.id == id)
        .ok_or_else(|| InstanceError::NotFound(id.to_string()))?;
    let row = file.instances.remove(index);
    save_unlocked(&file)?;
    Ok(row)
}
