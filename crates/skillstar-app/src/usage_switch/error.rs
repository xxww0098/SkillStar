//! Typed failures for symlink custody.
//!
//! Custody is a *transaction over two stores* — a file the CLI opens and, on
//! macOS Codex, a keychain entry that outranks it — so "it failed" is never
//! enough information. Two things have to survive all the way to the caller:
//!
//! 1. **What broke**, precisely enough that the Chinese message the user reads
//!    is generated from the failure rather than concatenated at the throw site.
//! 2. **When it broke**, relative to the one irreversible step: replacing the
//!    live path. [`Stage`] carries that, and it is the entire basis on which
//!    [`super::custody::Custody::activate`] decides whether to roll back. A
//!    failure before the swap leaves the previous credential intact and must
//!    *not* be rolled back over; a failure after it leaves a half-applied
//!    switch that must be.
//!
//! Everything here is private to `usage_switch`: the command layer only ever
//! sees the rendered message on [`super::SwitchOutcome::error`], so the
//! variants can describe disk reality exactly without widening the facade.

use std::io;
use std::path::PathBuf;

use skillstar_usage::UsageError;

use super::custody::LinkState;

pub(super) type CustodyResult<T> = Result<T, CustodyError>;

/// Everything that can go wrong while holding, reading or rewriting custody
/// of a CLI credential.
#[derive(Debug, thiserror::Error)]
pub(super) enum CustodyError {
    // ── path resolution ──────────────────────────────────────────────────
    /// The CLI's own home resolution failed (`CODEX_HOME` / `GROK_HOME` /
    /// `XDG_DATA_HOME` overrides included), so there is no live path to hold.
    #[error("无法定位 {tool} 凭证路径：{detail}")]
    ResolveLivePath { tool: &'static str, detail: String },

    // ── the lock ─────────────────────────────────────────────────────────
    #[error("打开 {} 失败：{source}", path.display())]
    OpenLock {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("获取 {} 锁失败：{source}", path.display())]
    AcquireLock {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Grok's stale-lock recovery reads a `PID:unix_seconds` holder line;
    /// these are the steps that rewrite it once the flock is held.
    #[error("{action}失败：{source}")]
    LockHolder {
        action: &'static str,
        #[source]
        source: io::Error,
    },

    // ── reading ──────────────────────────────────────────────────────────
    /// The snapshot the live path is supposed to serve is gone or is not a
    /// JSON object — the "snapshot missing or corrupt" case, which is fatal
    /// precisely because the snapshot *is* the credential under custody.
    #[error("快照 {} 不可读", path.display())]
    SnapshotUnreadable { path: PathBuf },

    /// The rolling backup taken before the swap cannot be read back, so a
    /// rollback has nothing to restore from.
    #[error("备份 {} 不可读", path.display())]
    BackupUnreadable { path: PathBuf },

    // ── writing / atomic replace ─────────────────────────────────────────
    #[error("创建 {} 失败：{source}", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Pre-creating the credential file at 0600 before it holds bytes.
    #[error("创建 {} 失败：{source}", path.display())]
    CreateFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("序列化失败：{source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },

    #[error("写入 {} 失败：{source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("设置 {} 权限失败：{source}", path.display())]
    SetMode {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("删除 {} 失败：{source}", path.display())]
    Remove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    // ── binding the live path ────────────────────────────────────────────
    /// `symlink()` / `symlink_file()` refused. On Windows without developer
    /// mode this is the *expected* failure and custody degrades to
    /// [`super::LinkMode::Copy`] rather than failing the switch — see
    /// [`CustodyError::is_symlink_denied`].
    #[error("创建软链失败：{source}")]
    Symlink {
        #[source]
        source: io::Error,
    },

    /// The `rename()` that swaps the directory entry — the one step after
    /// which the live path is no longer what it was.
    #[error("替换 {} 失败：{source}", path.display())]
    ReplaceLive {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    // ── verification ─────────────────────────────────────────────────────
    /// Written, then read back — and the CLI is not serving the account we
    /// just installed. On macOS this also catches a keychain that outranked
    /// the file we replaced.
    #[error("{tool} 凭证写入后回读校验失败（{observed:?}）")]
    ReadBackMismatch {
        tool: &'static str,
        observed: LinkState,
    },

    // ── identity attribution ─────────────────────────────────────────────
    /// The credential on disk matches more than one subscription row. Fails
    /// closed: guessing an owner is how one account's session gets written
    /// into another account's snapshot.
    #[error("{tool} 磁盘凭证同时匹配多个订阅，已拒绝归属；请重新授权目标账号")]
    AmbiguousOwner { tool: &'static str },

    /// A refresh SkillStar performed cannot be projected because the CLI is
    /// currently serving somebody else. The pin is a cache; the file wins.
    #[error("{tool} 当前凭证属于其它账号，已跳过刷新回投")]
    ServingAnotherAccount { tool: &'static str },

    // ── the material a snapshot is built from ────────────────────────────
    #[error(transparent)]
    Materialize(#[from] MaterializeError),

    // ── the second store ─────────────────────────────────────────────────
    #[error(transparent)]
    ExternalStore(#[from] ExternalStoreError),

    // ── the subscription store ───────────────────────────────────────────
    #[error(transparent)]
    Storage(#[from] UsageError),
}

impl CustodyError {
    /// Whether this is the symlink refusal custody is allowed to *degrade* on
    /// instead of failing: Windows without developer mode returns
    /// `PermissionDenied`, and a filesystem that has no symlinks at all
    /// returns `Unsupported`. Any other symlink error is a real fault, and the
    /// distinction is what keeps a degraded [`super::LinkMode::Copy`] from
    /// hiding a genuine one.
    pub(super) fn is_symlink_denied(&self) -> bool {
        matches!(
            self,
            Self::Symlink { source }
                if matches!(
                    source.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::Unsupported
                )
        )
    }
}

/// Custody failures reach the Usage layer as `UsageError`. A failure that
/// *originated* in the subscription store is handed back unchanged so its
/// classification (`NotFound`, `AuthRequired`, …) is not flattened into
/// `Other` on the way out.
impl From<CustodyError> for UsageError {
    fn from(error: CustodyError) -> Self {
        match error {
            CustodyError::Storage(inner) => inner,
            other => UsageError::Other(other.to_string()),
        }
    }
}

// ── the material a snapshot is built from ────────────────────────────────

/// The subscription row cannot produce a credential the CLI would accept.
///
/// Separate from [`CustodyError`] because nothing on disk is wrong: these are
/// all "re-authorize this account", and every one of them is raised *before*
/// the live path is touched.
#[derive(Debug, thiserror::Error)]
pub(super) enum MaterializeError {
    /// A credential field the CLI cannot run without is absent from the row.
    /// `remedy` differs per CLI because the way back differs (Grok re-authorizes,
    /// Codex re-logs-in), and telling a user the wrong one wastes their time.
    #[error("{tool} 切号缺少 {field}，{remedy}")]
    MissingSecret {
        tool: &'static str,
        field: &'static str,
        remedy: &'static str,
    },

    /// A synthesised entry that the CLI would read as logged-in but behave
    /// logged-out on: a half-formed session is worse than a refused switch.
    #[error("{tool} 凭证缺少必填字段 {field}，请重新授权一次")]
    IncompleteEntry {
        tool: &'static str,
        field: &'static str,
    },

    #[error("{tool} 凭证缺少 refresh token，请重新授权一次")]
    MissingRefreshToken { tool: &'static str },

    /// The token authenticates for billing but is rejected by every CLI call.
    #[error("{tool} 账号授权缺少 CLI scope（{}），请重新授权一次", missing.join(", "))]
    MissingCliScopes {
        tool: &'static str,
        missing: Vec<String>,
    },

    /// Nothing was ever captured for this account and the row holds nothing to
    /// build from — the user has to log in through the CLI once so custody has
    /// a session to take.
    #[error(
        "{tool} 账号还没有可用的本地凭证：请先在 {tool} CLI 里登录该账号，\
         SkillStar 会在下次切换时把它捕获为快照"
    )]
    NoCapturedSession { tool: &'static str },
}

// ── the second store ─────────────────────────────────────────────────────

/// The store a symlink cannot reach: the macOS keychain entry the Codex CLI
/// reads *before* `auth.json`.
///
/// Off macOS no target has a second store, so nothing constructs these — the
/// enum stays in the signature so the trait has one shape on every platform.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, thiserror::Error)]
pub(super) enum ExternalStoreError {
    #[error("无法定位 Codex home 目录")]
    MissingCodexHome,

    /// Writing is read-modify-write, so a blob that is not an object cannot be
    /// merged into without dropping whatever the CLI keeps alongside ours.
    #[error("Codex keychain 内容不是 JSON 对象")]
    NotAnObject,

    #[error("序列化 keychain 失败：{source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },

    #[error("执行 security 命令失败：{source}")]
    Spawn {
        #[source]
        source: io::Error,
    },

    #[error("macOS keychain 写入失败：{detail}")]
    Write { detail: String },
}

// ── before / after the replace ───────────────────────────────────────────

/// Where an activation failure fell relative to the single irreversible step:
/// swapping the live path.
///
/// This is the rollback contract. Before the swap the previous credential is
/// still exactly where it was, and "restoring" it would be the only thing that
/// could damage it; after the swap the CLI is holding a half-applied switch
/// and leaving it there is the damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    /// The live path was never touched: the old state is intact, nothing to
    /// undo.
    BeforeReplace,
    /// The live path may already be bound to the new snapshot.
    AfterReplace {
        /// The replacement itself completed and the target account is on disk
        /// — the failure came from a later step (the second store, or the
        /// read-back check). When it is `false` the bind is what failed, so
        /// the second store was never reached and must not be rewritten.
        target_installed: bool,
    },
}

impl Stage {
    pub(super) fn needs_rollback(self) -> bool {
        matches!(self, Self::AfterReplace { .. })
    }

    pub(super) fn target_installed(self) -> bool {
        matches!(
            self,
            Self::AfterReplace {
                target_installed: true
            }
        )
    }
}

/// A failed activation, paired with where it fell.
#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub(super) struct ActivationError {
    #[source]
    pub(super) reason: CustodyError,
    pub(super) stage: Stage,
}

impl ActivationError {
    pub(super) fn before_replace(reason: impl Into<CustodyError>) -> Self {
        Self {
            reason: reason.into(),
            stage: Stage::BeforeReplace,
        }
    }

    pub(super) fn after_replace(reason: impl Into<CustodyError>, target_installed: bool) -> Self {
        Self {
            reason: reason.into(),
            stage: Stage::AfterReplace { target_installed },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of [`Stage`]: a caller can tell "the old credential is
    /// untouched" from "the CLI is holding a half-applied switch".
    #[test]
    fn stage_decides_rollback_not_the_error_kind() {
        let before =
            ActivationError::before_replace(MaterializeError::MissingRefreshToken { tool: "Grok" });
        assert!(!before.stage.needs_rollback());
        assert!(!before.stage.target_installed());

        let bind_failed = ActivationError::after_replace(
            CustodyError::Symlink {
                source: io::Error::from(io::ErrorKind::PermissionDenied),
            },
            false,
        );
        assert!(bind_failed.stage.needs_rollback());
        assert!(
            !bind_failed.stage.target_installed(),
            "a failed bind never reached the second store"
        );

        let verify_failed = ActivationError::after_replace(
            CustodyError::ReadBackMismatch {
                tool: "grok",
                observed: LinkState::Missing,
            },
            true,
        );
        assert!(verify_failed.stage.needs_rollback());
        assert!(verify_failed.stage.target_installed());
    }

    /// Windows without developer mode degrades; a disk that is out of inodes
    /// does not.
    #[test]
    fn only_a_refused_symlink_counts_as_degradable() {
        for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::Unsupported] {
            assert!(
                CustodyError::Symlink {
                    source: io::Error::from(kind)
                }
                .is_symlink_denied(),
                "{kind:?}"
            );
        }
        assert!(
            !CustodyError::Symlink {
                source: io::Error::from(io::ErrorKind::OutOfMemory)
            }
            .is_symlink_denied()
        );
        assert!(
            !CustodyError::ReplaceLive {
                path: PathBuf::from("/tmp/auth.json"),
                source: io::Error::from(io::ErrorKind::PermissionDenied),
            }
            .is_symlink_denied(),
            "a failed rename is not a missing symlink capability"
        );
    }

    /// The user-facing text is generated by the variant, so it cannot drift
    /// back to English or lose the remedy half of the sentence.
    #[test]
    fn messages_stay_chinese_and_actionable() {
        assert_eq!(
            MaterializeError::MissingSecret {
                tool: "Codex",
                field: "id_token",
                remedy: "请重新登录该账号补充凭证",
            }
            .to_string(),
            "Codex 切号缺少 id_token，请重新登录该账号补充凭证"
        );
        assert_eq!(
            MaterializeError::MissingCliScopes {
                tool: "Grok",
                missing: vec!["grok-cli:access".into(), "conversations:read".into()],
            }
            .to_string(),
            "Grok 账号授权缺少 CLI scope（grok-cli:access, conversations:read），请重新授权一次"
        );
        assert!(
            MaterializeError::NoCapturedSession { tool: "OpenCode" }
                .to_string()
                .starts_with("OpenCode 账号还没有可用的本地凭证：")
        );
    }

    /// A failure that came from the subscription store keeps its
    /// classification instead of collapsing into `Other`.
    #[test]
    fn storage_failures_are_not_flattened_on_the_way_out() {
        let error: UsageError = CustodyError::Storage(UsageError::AuthRequired).into();
        assert!(matches!(error, UsageError::AuthRequired));

        let error: UsageError = CustodyError::AmbiguousOwner { tool: "grok" }.into();
        assert!(matches!(error, UsageError::Other(message) if message.contains("拒绝归属")));
    }
}
