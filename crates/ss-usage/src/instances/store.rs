//! Persist the desktop-instance registry next to other user config.

use super::apps::DesktopAppId;
use super::error::InstanceError;
use serde::{Deserialize, Serialize};
use ss_core::infra::{fs_ops, paths};
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
    instances: Vec<serde_json::Value>,
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
    let mut parsed: StoreFile = serde_json::from_slice(&bytes)
        .map_err(|e| InstanceError::Other(format!("无法读取实例清单: {e}")))?;
    for value in &mut parsed.instances {
        if let Some(mut row) = supported_row(value)? {
            migrate_devin_desktop_profiles(std::slice::from_mut(&mut row));
            *value = serde_json::to_value(row)
                .map_err(|e| InstanceError::Other(format!("无法读取实例清单: {e}")))?;
        }
    }
    Ok(parsed)
}

// Keep retired records opaque so editing a supported instance cannot erase them.
fn supported_row(value: &serde_json::Value) -> Result<Option<StoredInstance>, InstanceError> {
    let app = value
        .get("app")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| InstanceError::Other("实例记录缺少应用 ID".to_string()))?;
    if serde_json::from_value::<DesktopAppId>(serde_json::json!(app)).is_err() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|e| InstanceError::Other(format!("无法读取实例清单: {e}")))
}

/// Move `instances/windsurf/<id>` profile dirs onto the renamed
/// `instances/devin-desktop/<id>` path. The serde alias on
/// [`DesktopAppId::DevinDesktop`] already remaps the stored rows; this keeps
/// the on-disk profile data attached to them. No-op once migrated.
fn migrate_devin_desktop_profiles(instances: &mut [StoredInstance]) {
    for row in instances.iter_mut() {
        if row.app != DesktopAppId::DevinDesktop {
            continue;
        }
        let legacy = paths::instance_profile_dir("windsurf", &row.id);
        let current = paths::instance_profile_dir("devin-desktop", &row.id);
        if legacy.is_dir()
            && !current.exists()
            && let Err(error) = std::fs::rename(&legacy, &current)
        {
            tracing::warn!(
                ?legacy,
                ?current,
                %error,
                "windsurf → devin-desktop instance profile migration skipped"
            );
        }
    }
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
    let rows = file
        .instances
        .iter()
        .map(supported_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .flatten()
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
    file.instances.push(
        serde_json::to_value(&row)
            .map_err(|e| InstanceError::Other(format!("无法写入实例清单: {e}")))?,
    );
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
        .position(|row| row.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .ok_or_else(|| InstanceError::NotFound(id.to_string()))?;
    let row = supported_row(&file.instances[index])?
        .ok_or_else(|| InstanceError::NotFound(id.to_string()))?;
    file.instances.remove(index);
    save_unlocked(&file)?;
    Ok(row)
}
