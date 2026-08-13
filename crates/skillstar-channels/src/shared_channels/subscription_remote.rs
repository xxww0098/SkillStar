use super::{
    ChannelSubscription, ChannelSubscriptionRegistry, ChannelSubscriptionRemoteState,
    ChannelSubscriptionRemoteStatus, ChannelSubscriptionStore, SharedChannelError,
    SharedChannelErrorCode,
};
use chrono::Utc;

pub(super) fn ensure_remote_access(
    subscription: &ChannelSubscription,
) -> Result<(), SharedChannelError> {
    let (code, fallback) = match subscription.remote_state.status {
        ChannelSubscriptionRemoteStatus::Active => return Ok(()),
        ChannelSubscriptionRemoteStatus::Revoked => (
            SharedChannelErrorCode::SubscriptionAccessRevoked,
            "GitHub repository access was revoked; installed Skills are preserved, but remote changes are frozen",
        ),
        ChannelSubscriptionRemoteStatus::Offline => (
            SharedChannelErrorCode::Network,
            "The shared channel is offline; installed Skills are preserved and remote changes remain frozen until a successful retry",
        ),
        ChannelSubscriptionRemoteStatus::RecoverableFailure => (
            SharedChannelErrorCode::Protocol,
            "The shared channel probe failed temporarily; installed Skills are preserved and remote changes remain frozen until a successful retry",
        ),
        ChannelSubscriptionRemoteStatus::IntegrityError => (
            SharedChannelErrorCode::Integrity,
            "Shared channel integrity validation failed; installed Skills are preserved and remote changes remain frozen until validation succeeds",
        ),
    };
    Err(SharedChannelError::new(
        code,
        subscription
            .remote_state
            .message
            .clone()
            .unwrap_or_else(|| fallback.into()),
    ))
}

fn remote_failure_status(code: SharedChannelErrorCode) -> Option<ChannelSubscriptionRemoteStatus> {
    match code {
        SharedChannelErrorCode::RepositoryNotFound
        | SharedChannelErrorCode::AppNotInstalled
        | SharedChannelErrorCode::AppRepositorySelectionRequired
        | SharedChannelErrorCode::AppRepositoryAccessRequired => {
            Some(ChannelSubscriptionRemoteStatus::Revoked)
        }
        SharedChannelErrorCode::Network => Some(ChannelSubscriptionRemoteStatus::Offline),
        SharedChannelErrorCode::NotAuthenticated
        | SharedChannelErrorCode::PermissionDenied
        | SharedChannelErrorCode::InvitationRateLimited
        | SharedChannelErrorCode::Protocol => {
            Some(ChannelSubscriptionRemoteStatus::RecoverableFailure)
        }
        SharedChannelErrorCode::Integrity
        | SharedChannelErrorCode::SubscriptionSchemaUnsupported
        | SharedChannelErrorCode::ReleaseConflict
        | SharedChannelErrorCode::ReleaseNotFound
        | SharedChannelErrorCode::OrganizationUnavailable
        | SharedChannelErrorCode::PersonalOwnerRejected
        | SharedChannelErrorCode::PublicRepositoryRejected
        | SharedChannelErrorCode::UnsupportedHost => {
            Some(ChannelSubscriptionRemoteStatus::IntegrityError)
        }
        _ => None,
    }
}

pub(super) fn mark_remote_active(subscription: &mut ChannelSubscription) {
    subscription.remote_state = ChannelSubscriptionRemoteState::active();
    subscription.updated_at = Utc::now().to_rfc3339();
}

pub(super) fn mark_remote_failure(
    subscription: &mut ChannelSubscription,
    error: &SharedChannelError,
) -> bool {
    let Some(status) = remote_failure_status(error.code) else {
        return false;
    };
    subscription.remote_state = ChannelSubscriptionRemoteState {
        status,
        checked_at: Some(Utc::now().to_rfc3339()),
        message: Some(error.message.clone()),
    };
    subscription.updated_at = Utc::now().to_rfc3339();
    true
}

pub(super) fn persist_remote_failure<S: ChannelSubscriptionRegistry>(
    subscriptions: &S,
    store: &mut ChannelSubscriptionStore,
    index: usize,
    error: &SharedChannelError,
) -> Result<bool, SharedChannelError> {
    let changed = mark_remote_failure(&mut store.subscriptions[index], error);
    if changed {
        subscriptions.save(store)?;
    }
    Ok(changed)
}
