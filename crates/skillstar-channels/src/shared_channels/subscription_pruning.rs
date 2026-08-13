//! Offline reconciliation of the subscription store with the Skill hub.
//!
//! Global storage maintenance (force-delete, cache reset) removes Skills by
//! wiping directories, never through the per-Skill mutation gate. Those paths
//! report the removed names here so the subscription store stops claiming
//! Skills that no longer exist: a stale claim keeps
//! [`generic_installed_skill_is_mutable`](super::generic_installed_skill_is_mutable)
//! answering "immutable" for a name with nothing behind it, which the user can
//! neither reinstall nor delete.
//!
//! Only the *selection* is pruned. The subscription itself survives with its
//! `known_skill_ids`, so the channel controls can reinstall the Skills it
//! offers instead of forcing the user to re-subscribe.

use super::{
    ChannelSubscriptionRegistry, DiskChannelSubscriptionRegistry, DiskSharedChannelRegistry,
    SharedChannelError,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Local files that record which installed Skills a shared channel owns.
///
/// These are provenance for content that lives elsewhere (the hub), not user
/// preferences, so a config reset must keep them for as long as that content
/// can still be installed. See
/// `skillstar_skills::skill_mutation::SkillMutationPolicy::provenance_paths`.
pub(crate) fn provenance_paths() -> Vec<PathBuf> {
    vec![
        DiskSharedChannelRegistry::path(),
        DiskChannelSubscriptionRegistry::path(),
    ]
}

/// Drop `skill_ids` from every subscription's selection.
///
/// Returns how many tracked Skills were dropped. Errors are propagated so the
/// caller can abort before deleting anything: a subscription store on an
/// unsupported schema cannot be rewritten, and silently continuing would leave
/// exactly the stale claim this function exists to prevent.
pub(crate) fn prune_removed_skills(skill_ids: &[String]) -> Result<usize, SharedChannelError> {
    if skill_ids.is_empty() || !DiskChannelSubscriptionRegistry::path().exists() {
        return Ok(0);
    }
    let removed = skill_ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();

    let registry = DiskChannelSubscriptionRegistry;
    let mut store = registry.load_mutable()?;
    let mut pruned = 0usize;
    let now = chrono::Utc::now().to_rfc3339();

    for subscription in &mut store.subscriptions {
        let before = subscription.skills.len();
        subscription
            .skills
            .retain(|skill| !removed.contains(&skill.id.to_ascii_lowercase()));
        if subscription.skills.len() == before {
            continue;
        }
        pruned += before - subscription.skills.len();

        let remaining = subscription
            .skills
            .iter()
            .map(|skill| skill.id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        subscription
            .pins
            .retain(|pin| remaining.contains(&pin.skill_id.to_ascii_lowercase()));
        // The stored review describes Skills that are no longer installed, and
        // it cannot be recomputed offline (that needs the remote release). The
        // next channel check rebuilds it from scratch.
        subscription.last_update = None;
        subscription.updated_at = now.clone();
    }

    if pruned == 0 {
        return Ok(0);
    }
    registry.save(&store)?;
    Ok(pruned)
}

#[cfg(test)]
#[path = "subscription_pruning_tests.rs"]
mod tests;
