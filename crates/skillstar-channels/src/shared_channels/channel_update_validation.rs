use super::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelUpdateBlockReason, ChannelUpdateChange, ChannelUpdateItem,
    ChannelUpdateItemState, ChannelUpdateSnapshot, ChannelUpdateStatus, SharedChannelError,
    SharedChannelErrorCode,
};
use std::collections::BTreeSet;

pub(super) fn validate_update_snapshot(
    snapshot: &ChannelUpdateSnapshot,
) -> Result<(), SharedChannelError> {
    if snapshot.target.revision == 0
        || snapshot.target.tag_name != super::revision_tag(snapshot.target.revision)
        || snapshot.target.commit_sha.len() != 40
        || snapshot
            .target
            .commit_sha
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit())
        || chrono::DateTime::parse_from_rfc3339(&snapshot.published_at).is_err()
        || chrono::DateTime::parse_from_rfc3339(&snapshot.checked_at).is_err()
        || super::release::validate_publisher(&snapshot.publisher).is_err()
        || super::release::validate_release_text(&snapshot.title, &snapshot.notes).is_err()
        || snapshot.items.len() > 4_096
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
        if skillstar_skills::content::validate_skill_name(&item.id).is_err()
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

fn valid_content_hash(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) && CHANNEL_CONTENT_HASH_VERSION == skillstar_skills::content::SNAPSHOT_HASH_VERSION
}

fn invalid_snapshot() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Storage,
        "Unable to validate the shared channel update snapshot",
    )
}
