use super::{
    ChannelSubscription, ChannelSubscriptionRegistry, ChannelSubscriptionRemoteState,
    ChannelSubscriptionRemoteStatus, ChannelSubscriptionStore, SharedChannelError,
    SharedChannelErrorCode,
};
use chrono::Utc;

pub(super) fn ensure_remote_access(
    subscription: &ChannelSubscription,
) -> Result<(), SharedChannelError> {
    if subscription.remote_state.status == ChannelSubscriptionRemoteStatus::Revoked {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionAccessRevoked,
            subscription.remote_state.message.clone().unwrap_or_else(|| {
                "GitHub repository access was revoked; installed Skills are preserved, but remote changes are frozen"
                    .into()
            }),
        ));
    }
    Ok(())
}

pub(super) fn mark_remote_active(subscription: &mut ChannelSubscription) {
    subscription.remote_state = ChannelSubscriptionRemoteState::active();
    subscription.updated_at = Utc::now().to_rfc3339();
}

pub(super) fn mark_definitive_remote_failure(
    subscription: &mut ChannelSubscription,
    error: &SharedChannelError,
) -> bool {
    if !definitive_access_loss(error.code) {
        return false;
    }
    subscription.remote_state = ChannelSubscriptionRemoteState::revoked(error.message.clone());
    subscription.updated_at = Utc::now().to_rfc3339();
    true
}

pub(super) fn persist_definitive_remote_failure<S: ChannelSubscriptionRegistry>(
    subscriptions: &S,
    store: &mut ChannelSubscriptionStore,
    index: usize,
    error: &SharedChannelError,
) -> Result<bool, SharedChannelError> {
    let changed = mark_definitive_remote_failure(&mut store.subscriptions[index], error);
    if changed {
        subscriptions.save(store)?;
    }
    Ok(changed)
}

fn definitive_access_loss(code: SharedChannelErrorCode) -> bool {
    matches!(
        code,
        SharedChannelErrorCode::RepositoryNotFound
            | SharedChannelErrorCode::AppRepositoryAccessRequired
    )
}
