use super::subscription_types::ChannelReleaseTarget;
use super::{SharedChannelError, SharedChannelErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path};

pub const CHANNEL_CONTENT_HASH_VERSION: u32 = crate::content::SNAPSHOT_HASH_VERSION;
pub const MAX_CHANNEL_SKILLS: usize = 4_096;
pub const MAX_RELEASE_TITLE_BYTES: usize = 120;
pub const MAX_RELEASE_NOTES_BYTES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelPublisherIdentity {
    pub id: u64,
    pub login: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateStatus {
    UpToDate,
    UpdateAvailable,
    PartiallyUpgraded,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateChange {
    Added,
    Updated,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateItemState {
    Current,
    Available,
    Applied,
    Blocked,
    Failed,
    Notification,
    RemovedFromChannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateBlockReason {
    LocalContentChanged,
    BaselineMissing,
    SnapshotFailed,
    RemovedUpstream,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelUpdateItem {
    pub id: String,
    pub change: ChannelUpdateChange,
    pub state: ChannelUpdateItemState,
    pub selected: bool,
    pub from_content_hash: Option<String>,
    pub to_content_hash: Option<String>,
    pub block_reason: Option<ChannelUpdateBlockReason>,
    pub suggested_local_name: Option<String>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_target: Option<ChannelReleaseTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<SharedChannelErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelUpdateSnapshot {
    pub target: ChannelReleaseTarget,
    pub title: String,
    pub notes: String,
    pub publisher: ChannelPublisherIdentity,
    pub published_at: String,
    pub checked_at: String,
    pub status: ChannelUpdateStatus,
    pub acknowledgement_required: bool,
    pub items: Vec<ChannelUpdateItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_error_code: Option<SharedChannelErrorCode>,
}

pub fn revision_tag(revision: u64) -> String {
    format!("channel-v{revision:06}")
}

pub fn parse_revision_tag(tag: &str) -> Option<u64> {
    let revision = tag.strip_prefix("channel-v")?;
    if revision.len() < 6 || !revision.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let revision = revision.parse::<u64>().ok()?;
    (revision > 0 && revision_tag(revision) == tag).then_some(revision)
}

pub fn validate_publisher(identity: &ChannelPublisherIdentity) -> Result<(), SharedChannelError> {
    let valid = identity.id > 0
        && !identity.login.is_empty()
        && identity.login.len() <= 39
        && identity
            .login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::Integrity,
            "The channel publisher identity is invalid",
        ))
    }
}

pub fn validate_release_text(title: &str, notes: &str) -> Result<(), SharedChannelError> {
    if title.trim().is_empty()
        || title.len() > MAX_RELEASE_TITLE_BYTES
        || notes.len() > MAX_RELEASE_NOTES_BYTES
        || title.chars().any(char::is_control)
        || notes.contains('\0')
    {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "Release title is required and release text exceeds the supported limits",
        ))
    } else {
        Ok(())
    }
}

pub fn valid_skill_id(value: &str) -> bool {
    value.len() <= 128 && crate::content::validate_skill_name(value).is_ok()
}

pub fn valid_content_root(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let path = Path::new(value);
    !path.is_absolute()
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

pub fn valid_content_hash(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) && CHANNEL_CONTENT_HASH_VERSION == crate::content::SNAPSHOT_HASH_VERSION
}

pub fn validate_update_snapshot(
    snapshot: &ChannelUpdateSnapshot,
) -> Result<(), SharedChannelError> {
    if snapshot.target.revision == 0
        || snapshot.target.tag_name != revision_tag(snapshot.target.revision)
        || snapshot.target.commit_sha.len() != 40
        || snapshot
            .target
            .commit_sha
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit())
        || chrono::DateTime::parse_from_rfc3339(&snapshot.published_at).is_err()
        || chrono::DateTime::parse_from_rfc3339(&snapshot.checked_at).is_err()
        || validate_publisher(&snapshot.publisher).is_err()
        || validate_release_text(&snapshot.title, &snapshot.notes).is_err()
        || snapshot.items.len() > MAX_CHANNEL_SKILLS
        || snapshot
            .check_error
            .as_ref()
            .is_some_and(|error| error.is_empty() || error.contains('\0'))
        || snapshot.check_error.is_some() != snapshot.check_error_code.is_some()
    {
        return Err(invalid_snapshot());
    }
    let mut skills = BTreeSet::new();
    for item in &snapshot.items {
        if crate::content::validate_skill_name(&item.id).is_err()
            || !skills.insert(item.id.to_ascii_lowercase())
            || item
                .from_content_hash
                .as_deref()
                .is_some_and(|hash| !valid_content_hash(hash))
            || item
                .to_content_hash
                .as_deref()
                .is_some_and(|hash| !valid_content_hash(hash))
            || !valid_update_item(item)
        {
            return Err(invalid_snapshot());
        }
    }
    let has_pending = snapshot.items.iter().any(|item| {
        matches!(
            item.state,
            ChannelUpdateItemState::Available
                | ChannelUpdateItemState::Blocked
                | ChannelUpdateItemState::Failed
                | ChannelUpdateItemState::RemovedFromChannel
        )
    });
    let has_available = snapshot
        .items
        .iter()
        .any(|item| item.state == ChannelUpdateItemState::Available);
    let has_notification = snapshot
        .items
        .iter()
        .any(|item| item.state == ChannelUpdateItemState::Notification);
    let has_advanced = snapshot.items.iter().any(|item| {
        matches!(
            item.state,
            ChannelUpdateItemState::Current | ChannelUpdateItemState::Applied
        )
    });
    let valid_status = match snapshot.status {
        ChannelUpdateStatus::UpToDate => {
            !has_pending && !has_notification && !snapshot.acknowledgement_required
        }
        ChannelUpdateStatus::UpdateAvailable => {
            has_available || has_notification || snapshot.acknowledgement_required
        }
        ChannelUpdateStatus::PartiallyUpgraded => has_pending && has_advanced,
        ChannelUpdateStatus::Blocked => has_pending && !has_available,
    };
    if !valid_status {
        return Err(invalid_snapshot());
    }
    Ok(())
}

fn valid_update_item(item: &ChannelUpdateItem) -> bool {
    let no_resolution_metadata = item.block_reason.is_none()
        && item.suggested_local_name.is_none()
        && item.error.is_none()
        && item.error_code.is_none();
    match (item.change, item.state) {
        (ChannelUpdateChange::Added, ChannelUpdateItemState::Notification) => {
            !item.selected
                && item.from_content_hash.is_none()
                && item.to_content_hash.is_some()
                && no_resolution_metadata
        }
        (ChannelUpdateChange::Removed, ChannelUpdateItemState::RemovedFromChannel) => {
            item.selected
                && item.from_content_hash.is_some()
                && item.to_content_hash.is_none()
                && item.block_reason == Some(ChannelUpdateBlockReason::RemovedUpstream)
                && item.suggested_local_name.is_some()
                && item.error.is_none()
                && item.error_code.is_none()
        }
        (ChannelUpdateChange::Removed, ChannelUpdateItemState::Blocked) => {
            item.selected
                && item.from_content_hash.is_some()
                && item.to_content_hash.is_none()
                && item.block_reason == Some(ChannelUpdateBlockReason::RemovedUpstream)
                && item.suggested_local_name.is_none()
                && item.error.is_none()
                && item.error_code.is_none()
        }
        (ChannelUpdateChange::Unchanged, ChannelUpdateItemState::Current) => {
            item.selected
                && item.from_content_hash.is_some()
                && item.from_content_hash == item.to_content_hash
                && no_resolution_metadata
        }
        (ChannelUpdateChange::Updated, ChannelUpdateItemState::Available)
        | (ChannelUpdateChange::Updated, ChannelUpdateItemState::Applied) => {
            item.selected
                && item.from_content_hash.is_some()
                && item.to_content_hash.is_some()
                && no_resolution_metadata
        }
        (ChannelUpdateChange::Updated, ChannelUpdateItemState::Blocked) => {
            item.selected
                && item.from_content_hash.is_some()
                && item.to_content_hash.is_some()
                && item
                    .block_reason
                    .is_some_and(|reason| reason != ChannelUpdateBlockReason::RemovedUpstream)
                && item.suggested_local_name.is_some()
                && item
                    .error
                    .as_ref()
                    .is_none_or(|error| !error.contains('\0'))
                && item.error_code.is_none()
        }
        (ChannelUpdateChange::Updated, ChannelUpdateItemState::Failed) => {
            item.selected
                && item.from_content_hash.is_some()
                && item.to_content_hash.is_some()
                && item
                    .error
                    .as_ref()
                    .is_some_and(|error| !error.is_empty() && !error.contains('\0'))
                && item.error_code.is_some()
                && item
                    .block_reason
                    .is_none_or(|reason| reason != ChannelUpdateBlockReason::RemovedUpstream)
        }
        _ => false,
    }
}

fn invalid_snapshot() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Storage,
        "Unable to validate the shared channel update snapshot",
    )
}
