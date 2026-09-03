//! Isolated desktop-app instances (Cursor / Grok Bot / Antigravity).
//!
//! Not a Usage catalog concern: quota cards stay quota cards. This module
//! owns the instance registry, profile paths, launch argv, and PID matching.

mod apps;
mod dto;
mod error;
mod launch;
mod process;
mod store;

#[cfg(test)]
mod tests;

use skillstar_core::infra::error::AppError;

pub use apps::DesktopAppId;
pub use dto::{AppInstanceDto, DesktopAppDto};
pub use error::{CLAUDE_DESKTOP_REASON, InstanceError};

use apps::all_apps;
use store::StoredInstance;

pub fn list_desktop_apps() -> Vec<DesktopAppDto> {
    all_apps()
        .into_iter()
        .map(|app| DesktopAppDto {
            id: app,
            display_name: app.display_name().to_string(),
            catalog_id: app.catalog_id().map(str::to_string),
            macos_app_name: app.launch_spec().macos_app_name.to_string(),
        })
        .collect()
}

pub fn list_instances(app: &str) -> Result<Vec<AppInstanceDto>, AppError> {
    let app = DesktopAppId::parse(app)?;
    let rows = store::list_stored(Some(app))?;
    Ok(rows.into_iter().map(to_dto).collect())
}

pub fn create_instance(app: &str, name: String) -> Result<AppInstanceDto, AppError> {
    let app = DesktopAppId::parse(app)?;
    Ok(to_dto(store::create_stored(app, name)?))
}

pub fn start_instance(id: &str) -> Result<AppInstanceDto, AppError> {
    let row = store::get_stored(id)?;
    let dir = store::profile_dir(row.app, &row.id)?;
    if !process::pids_for_user_data_dir(&dir).is_empty() {
        return Ok(to_dto(row));
    }
    launch::start_macos_app(row.app, &dir, &row.extra_args)?;
    Ok(to_dto(row))
}

pub fn stop_instance(id: &str) -> Result<AppInstanceDto, AppError> {
    if !cfg!(target_os = "macos") {
        return Err(InstanceError::Platform.into());
    }
    let row = store::get_stored(id)?;
    let dir = store::profile_dir(row.app, &row.id)?;
    let pids = process::pids_for_user_data_dir(&dir);
    process::stop_pids(&pids);
    Ok(to_dto(row))
}

pub fn delete_instance(id: &str) -> Result<(), AppError> {
    let row = store::get_stored(id)?;
    let dir = store::profile_dir(row.app, &row.id)?;
    if !process::pids_for_user_data_dir(&dir).is_empty() {
        return Err(InstanceError::Running.into());
    }
    store::delete_stored(id)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(InstanceError::from)?;
    }
    Ok(())
}

fn to_dto(row: StoredInstance) -> AppInstanceDto {
    let dir = store::profile_dir(row.app, &row.id).unwrap_or_default();
    let pids = process::pids_for_user_data_dir(&dir);
    AppInstanceDto {
        id: row.id,
        app: row.app,
        name: row.name,
        user_data_dir: dir.to_string_lossy().into_owned(),
        extra_args: row.extra_args,
        running: !pids.is_empty(),
        pid: pids.first().copied(),
        created_at: row.created_at,
    }
}
