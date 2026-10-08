//! Cross-process update serialization and channel-facing update report types.
//!
//! D-081 removed the generic update transaction (divergence inspection,
//! checkout rollback, resolve flows): generic updates are overwrite-reinstalls
//! in [`crate::update`]. What survives here:
//! - [`transaction`]: the update-transaction mutex/file lock shared channels
//!   still use to serialize installs, upgrades and rollbacks;
//! - the report DTOs commands return for `update_skill(s)`;
//! - the divergence vocabulary shared-channel dialogs keep using (reasons,
//!   resolutions, suggested `<name>.local` copies).

pub(crate) mod transaction;

use serde::Serialize;
use ss_core::types::Skill;

pub use transaction::{
    OperationInProgress, UpdateTransactionGuard, acquire_update_transaction_lock,
    sweep_stale_transients, try_acquire_update_transaction_lock,
};

/// Vocabulary kept for shared-channel dialogs; generic updates no longer
/// produce these (they overwrite), but channel-managed skills still report
/// divergence against their subscription baseline.
pub mod divergence {
    use serde::{Deserialize, Serialize};

    /// Why a channel-managed Skill stopped before an upgrade.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum LocalDivergenceReason {
        ContentChanged,
        BaselineMissing,
        ContentReadFailed,
        SnapshotFailed,
        SourceMissing,
        SourceRemoved,
        SourceRemovedContentLost,
    }

    /// A user's explicit choice for divergent content before a channel action.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum LocalDivergenceResolution {
        Preserve { local_name: String },
        Discard,
    }

    /// A Skill a channel upgrade refused to touch.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SkillUpdateBlocked {
        pub name: String,
        pub reason: LocalDivergenceReason,
        pub suggested_local_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub error: Option<String>,
    }

    fn name_taken(name: &str) -> bool {
        ss_core::infra::paths::agents_skill_dir(name)
            .symlink_metadata()
            .is_ok()
    }

    /// Non-colliding `<name>.local` candidate for preserving a divergent copy.
    pub fn suggested_local_name(name: &str) -> String {
        let base = format!("{name}.local");
        if !name_taken(&base) {
            return base;
        }
        for index in 2.. {
            let candidate = format!("{base}.{index}");
            if !name_taken(&candidate) {
                return candidate;
            }
        }
        base
    }
}

pub use divergence::{LocalDivergenceReason, LocalDivergenceResolution, SkillUpdateBlocked};

/// One overwrite-update applied by [`crate::git_skill`] (`update_skill`).
#[derive(Debug, Clone, Serialize)]
pub struct UpdateResult {
    pub skill: Skill,
    /// Kept for IPC compatibility; D-081 has no same-checkout sibling fan-out.
    pub siblings_cleared: Vec<String>,
    pub agent_link_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateFailure {
    pub name: String,
    pub error: String,
}

/// A Skill a shared channel owns, which the generic update path declines by
/// design rather than fails on.
#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateChannelManaged {
    pub name: String,
    pub repository_id: u64,
}

/// Upstream renamed the Skill's frontmatter `name`; nothing was installed.
#[derive(Debug, Clone, Serialize)]
pub struct SkillIdentityChange {
    pub name: String,
    pub upstream_name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SkillUpdateReport {
    pub updated: Vec<UpdateResult>,
    pub blocked: Vec<SkillUpdateBlocked>,
    pub failed: Vec<SkillUpdateFailure>,
    /// Names whose upstream no longer ships them (manual remove/convert exits).
    pub skipped: Vec<String>,
    pub channel_managed: Vec<SkillUpdateChannelManaged>,
    pub identity_changed: Vec<SkillIdentityChange>,
    /// Local creations and bundle installs: no upstream to update from.
    pub not_updatable: Vec<String>,
    /// Automatic admission kept these canonical copies. Manual updates leave
    /// this empty; the update module already recorded the local-change marker.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept_local: Vec<String>,
    /// Project copies of updated Skills that could not be refreshed, as
    /// `<project>: <reason>`.
    pub project_failures: Vec<String>,
}
