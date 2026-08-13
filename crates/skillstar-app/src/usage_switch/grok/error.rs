//! Typed failures for the two credential files the `grok` module owns.
//!
//! Everything here stays private to `grok`: the parent `usage_switch` module
//! only ever sees the rendered message, so the variants can describe disk
//! reality precisely without widening the module's facade.

use std::path::PathBuf;

/// Which owned artifact a failure refers to. Kept as a single enum so the
/// shared failure shapes (serialize / replace / lock) need one variant each
/// instead of one per file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GrokFile {
    /// Grok CLI's own `auth.json`.
    Auth,
    /// SkillStar's per-subscription session snapshot store.
    SessionStore,
    /// A single session snapshot inside that store.
    SessionSnapshot,
}

impl std::fmt::Display for GrokFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Auth => "Grok auth.json",
            Self::SessionStore => "Grok session store",
            Self::SessionSnapshot => "Grok session snapshot",
        };
        formatter.write_str(label)
    }
}

/// Reading, locking, or rewriting one of the credential files failed.
#[derive(Debug, thiserror::Error)]
pub(super) enum GrokIoError {
    #[error("{0}")]
    ResolveAuthPath(String),

    #[error("读取 {} 失败：{source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{} 不是有效 JSON：{source}", path.display())]
    InvalidJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("{} 的根必须是 JSON 对象", path.display())]
    NotAnObject { path: PathBuf },

    #[error("序列化 {file} 失败：{source}")]
    Serialize {
        file: GrokFile,
        #[source]
        source: serde_json::Error,
    },

    #[error("无法定位 {file} 目录")]
    MissingParentDir { file: GrokFile },

    #[error("创建目录失败：{source}")]
    CreateDir {
        #[source]
        source: std::io::Error,
    },

    #[error("原子替换 {file} 失败：{source}")]
    Replace {
        file: GrokFile,
        #[source]
        source: std::io::Error,
    },

    #[error("打开 {file} 锁失败：{source}")]
    OpenLock {
        file: GrokFile,
        #[source]
        source: std::io::Error,
    },

    #[error("锁定 {file} 失败：{source}")]
    AcquireLock {
        file: GrokFile,
        #[source]
        source: std::io::Error,
    },

    /// Grok's stale-lock recovery reads a `PID:unix_seconds` holder line; these
    /// are the steps that rewrite it after the flock is acquired.
    #[error("{action}凭据锁 holder 失败：{source}")]
    LockHolder {
        action: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("{action}临时凭据文件失败：{source}")]
    TempFile {
        action: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("设置 {} 权限为 0600 失败：{source}", path.display())]
    SetMode {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("读取 {} 权限失败：{source}", path.display())]
    ReadMode {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{} 权限应为 0600，实际为 {mode:o}", path.display())]
    UnexpectedMode { path: PathBuf, mode: u32 },

    #[error("Grok 账号 {subscription_id} 的 session snapshot 无法解密")]
    SnapshotUndecryptable { subscription_id: String },

    #[error("Grok 账号 {subscription_id} 的 session snapshot 损坏：{source}")]
    SnapshotCorrupt {
        subscription_id: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("Grok 账号 {subscription_id} 的 session snapshot 不是 JSON 对象")]
    SnapshotNotAnObject { subscription_id: String },
}

/// Why a verified commit of `auth.json` did not complete.
#[derive(Debug, thiserror::Error)]
pub(super) enum AuthCommitReason {
    #[error(transparent)]
    Io(#[from] GrokIoError),

    #[error("Grok auth.json 在切换期间被其他进程修改，请关闭正在运行的 Grok 后重试")]
    ConcurrentModification,

    #[error("Grok auth.json 写后核验缺少目标 OIDC entry")]
    MissingVerifiedEntry,

    #[error("Grok auth.json 写入后被覆盖，请关闭正在运行的 Grok 后重试")]
    OverwrittenAfterCommit,
}

/// A failed `commit_verified`, paired with whether the target entry is
/// nonetheless live on disk — the caller needs that to decide between rolling
/// the active pin back and leaving it where it is.
#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub(super) struct AuthCommitError {
    #[source]
    pub(super) reason: AuthCommitReason,
    /// The replacement completed and the expected target entry was still on
    /// disk when the later failure occurred (for example chmod verification).
    pub(super) target_installed: bool,
}

impl AuthCommitError {
    pub(super) fn before_replace(reason: impl Into<AuthCommitReason>) -> Self {
        Self {
            reason: reason.into(),
            target_installed: false,
        }
    }

    pub(super) fn after_replace(
        reason: impl Into<AuthCommitReason>,
        target_installed: bool,
    ) -> Self {
        Self {
            reason: reason.into(),
            target_installed,
        }
    }
}
