//! Mutation-gate query seam for the skill lifecycle.
//!
//! The default policy consults the channel registry within this domain.
//! GUI, CLI and direct library callers receive identical ownership protection
//! without composition-root registration or a process-global policy override.

use std::path::PathBuf;

/// Decides whether a generic mutation path may touch a skill or repository.
pub(crate) trait SkillMutationPolicy: Send + Sync + 'static {
    /// Reject generic mutation of a skill that a shared channel manages.
    fn ensure_skill_mutation_allowed(&self, skill_id: &str) -> anyhow::Result<()>;

    /// Reject generic mutation of a repository that a shared channel owns.
    fn ensure_repository_mutation_allowed(&self, repository_url: &str) -> anyhow::Result<()>;

    /// Repository id of the channel managing `skill_id`, if any.
    fn managed_repository_for_skill(&self, skill_id: &str) -> anyhow::Result<Option<u64>>;

    /// Reconcile ownership bookkeeping after a bulk removal of `skill_ids`.
    ///
    /// Global maintenance (storage resets) wipes whole directories instead of
    /// walking the per-Skill gate, so without this the registry would keep
    /// claiming names that no longer exist on disk — and those names stay
    /// permanently immutable, impossible to reinstall or delete.
    fn on_bulk_skill_removal(&self, skill_ids: &[String]) -> anyhow::Result<()>;

    /// Config files that record *ownership of installed content*, not user
    /// preferences.
    ///
    /// A generic config reset must preserve them: deleting the record while
    /// the content it describes is still installed strands that content —
    /// it stops being recognised as channel-owned, and the ordinary update
    /// path then tries to fetch a private channel repository anonymously.
    fn provenance_paths(&self) -> Vec<PathBuf>;
}

/// Whether a shared channel currently manages `skill_id`.
///
/// The narrow read entry point for global maintenance paths outside this crate
/// (they cannot reach [`policy`], which stays crate-private so the gate is not
/// a general-purpose registry lookup). Callers that must not touch managed
/// Skills treat an `Err` as "owned": an unreadable registry means ownership is
/// unknown, never that the Skill is free.
pub fn skill_is_channel_managed(skill_id: &str) -> anyhow::Result<bool> {
    Ok(policy().managed_repository_for_skill(skill_id)?.is_some())
}

/// Report a bulk removal of `skill_ids` performed outside the per-Skill gate.
///
/// Storage resets delete whole directories at once; this is how they hand the
/// resulting name list back to the gate owner so its bookkeeping stays aligned
/// with the filesystem. Call it *before* the destructive work: an error means
/// the bookkeeping cannot be updated, and the reset must abort rather than
/// leave records pointing at content it is about to delete.
pub fn notify_bulk_skill_removal(skill_ids: &[String]) -> anyhow::Result<()> {
    policy().on_bulk_skill_removal(skill_ids)
}

/// Config files a generic reset must preserve — see
/// [`SkillMutationPolicy::provenance_paths`].
pub fn provenance_paths() -> Vec<PathBuf> {
    policy().provenance_paths()
}

/// The domain-default policy. No mutable global registration or permissive fallback.
pub(crate) fn policy() -> &'static dyn SkillMutationPolicy {
    &crate::channels::policy::ChannelAwarePolicy
}
