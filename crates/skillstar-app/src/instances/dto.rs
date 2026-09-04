use super::apps::DesktopAppId;
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "DesktopApp.ts", rename = "DesktopApp")]
pub struct DesktopAppDto {
    pub id: DesktopAppId,
    pub display_name: String,
    pub catalog_id: Option<String>,
    pub macos_app_name: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "AppInstance.ts", rename = "AppInstance")]
pub struct AppInstanceDto {
    pub id: String,
    pub app: DesktopAppId,
    pub name: String,
    pub user_data_dir: String,
    pub extra_args: Vec<String>,
    pub running: bool,
    pub pid: Option<u32>,
    #[ts(type = "number")]
    pub created_at: i64,
}
