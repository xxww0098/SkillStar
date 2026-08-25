//! Injected mutation-gate policy seam.
//!
//! Generic (non-channel) skill mutation paths must reject skills/repositories
//! that a shared channel manages. The channel registry lives in
//! `skillstar-channels`, so this crate cannot query it directly; instead the
//! check is delegated to a process-wide [`SkillMutationPolicy`] registered by
//! an application composition root (`skillstar-channels::policy::install_global_policy`).
//! Until registered, the allow-all default applies.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Decides whether a generic mutation path may touch a skill or repository.
pub trait SkillMutationPolicy: Send + Sync + 'static {
    /// Reject generic mutation of a skill that a shared channel manages.
    fn ensure_skill_mutation_allowed(&self, skill_id: &str) -> anyhow::Result<()>;

    /// Reject generic mutation of a repository that a shared channel owns.
    fn ensure_repository_mutation_allowed(&self, repository_url: &str) -> anyhow::Result<()>;

    /// Whether the installed skill at `skill_path` may be touched generically.
    fn installed_skill_is_mutable(&self, skill_id: &str, skill_path: &Path)
    -> anyhow::Result<bool>;

    /// Repository id of the channel managing `skill_id`, if any.
    fn managed_repository_for_skill(&self, skill_id: &str) -> anyhow::Result<Option<u64>>;

    /// Repository id of the channel owning `repository_url`, if any.
    fn managed_repository_for_url(&self, repository_url: &str) -> anyhow::Result<Option<u64>>;

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

struct AllowAllPolicy;

impl SkillMutationPolicy for AllowAllPolicy {
    fn ensure_skill_mutation_allowed(&self, _skill_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn ensure_repository_mutation_allowed(&self, _repository_url: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn installed_skill_is_mutable(
        &self,
        _skill_id: &str,
        _skill_path: &Path,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }

    fn managed_repository_for_skill(&self, _skill_id: &str) -> anyhow::Result<Option<u64>> {
        Ok(None)
    }

    fn managed_repository_for_url(&self, _repository_url: &str) -> anyhow::Result<Option<u64>> {
        Ok(None)
    }

    fn on_bulk_skill_removal(&self, _skill_ids: &[String]) -> anyhow::Result<()> {
        Ok(())
    }

    fn provenance_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }
}

static POLICY: RwLock<Option<Arc<dyn SkillMutationPolicy>>> = RwLock::new(None);

/// Register the process-wide policy. **First registration wins.**
///
/// The gate protects channel-owned Skills from generic mutation, so "last one
/// wins" made it possible for any later caller to silently swap the real policy
/// for a weaker one with no compile-time or runtime signal. Composition roots
/// call this once at startup; calling it again is tolerated (test binaries run
/// several entry points in one process) but never downgrades what is installed.
///
/// Tests that need a different policy must use
/// [`replace_skill_mutation_policy_for_test`], which restores the previous
/// policy when its guard drops.
pub fn set_skill_mutation_policy(policy: Arc<dyn SkillMutationPolicy>) {
    let mut slot = POLICY.write().expect("skill mutation policy lock poisoned");
    if slot.is_some() {
        tracing::debug!(
            target: "skills",
            "Skill mutation policy is already installed; keeping the existing one"
        );
        return;
    }
    *slot = Some(policy);
}

/// Scoped policy override for tests.
///
/// The returned guard restores the previous policy on drop, so a test can no
/// longer leave a policy installed for every test that runs after it in the
/// same binary.
#[must_use = "the policy is restored when the guard is dropped"]
pub fn replace_skill_mutation_policy_for_test(
    policy: Arc<dyn SkillMutationPolicy>,
) -> SkillMutationPolicyGuard {
    let mut slot = POLICY.write().expect("skill mutation policy lock poisoned");
    let previous = slot.replace(policy);
    SkillMutationPolicyGuard { previous }
}

/// Restores the policy that was installed before the override.
pub struct SkillMutationPolicyGuard {
    previous: Option<Arc<dyn SkillMutationPolicy>>,
}

impl Drop for SkillMutationPolicyGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = POLICY.write() {
            *slot = self.previous.take();
        }
    }
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

/// Current policy snapshot (allow-all until registered).
pub(crate) fn policy() -> Arc<dyn SkillMutationPolicy> {
    POLICY
        .read()
        .expect("skill mutation policy lock poisoned")
        .clone()
        .unwrap_or_else(|| Arc::new(AllowAllPolicy))
}
