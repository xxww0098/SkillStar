use skillstar_core::infra::error::AppError;

pub const CLAUDE_DESKTOP_REASON: &str = "Claude Desktop 不支持多开：应用会忽略 --user-data-dir 与 CLAUDE_USER_DATA_DIR，多个进程仍写入 ~/Library/Application Support/Claude，会造成账号冲突。";

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error("{0}")]
    UnsupportedApp(String),

    #[error("桌面多开目前仅支持 macOS")]
    Platform,

    #[error("实例不存在: {0}")]
    NotFound(String),

    #[error("实例正在运行，请先停止")]
    Running,

    #[error("实例名称不能为空")]
    EmptyName,

    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<InstanceError> for AppError {
    fn from(error: InstanceError) -> Self {
        AppError::Other(error.to_string())
    }
}
