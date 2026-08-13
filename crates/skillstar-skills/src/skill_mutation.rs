//! Injected mutation-gate policy seam.
//!
//! Generic (non-channel) skill mutation paths must reject skills/repositories
//! that a shared channel manages. The channel registry lives in
//! `skillstar-channels`, so this crate cannot query it directly; instead the
//! check is delegated to a process-wide [`SkillMutationPolicy`] registered by
//! an application composition root (`skillstar-channels::policy::install_global_policy`).
//! Until registered, the allow-all default applies.

use std::path::Path;
use std::sync::{Arc, RwLock};

/// Decides whether a generic mutation path may touch a skill or repository.
pub trait SkillMutationPolicy: Send + Sync + 'static {
    /// Reject generic mutation of a skill that a shared channel manages.
    fn ensure_skill_mutation_allowed(&self, skill_id: &str) -> anyhow::Result<()>;

    /// Reject generic mutation of a repository that a shared channel owns.
    fn ensure_repository_mutation_allowed(&self, repository_url: &str) -> anyhow::Result<()>;

    /// Whether the installed skill at `skill_path` may be touched generically.
    fn installed_skill_is_mutable(
        &self,
        skill_id: &str,
        skill_path: &Path,
    ) -> anyhow::Result<bool>;

    /// Repository id of the channel managing `skill_id`, if any.
    fn managed_repository_for_skill(&self, skill_id: &str) -> anyhow::Result<Option<u64>>;

    /// Repository id of the channel owning `repository_url`, if any.
    fn managed_repository_for_url(&self, repository_url: &str) -> anyhow::Result<Option<u64>>;
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

/// Current policy snapshot (allow-all until registered).
pub(crate) fn policy() -> Arc<dyn SkillMutationPolicy> {
    POLICY
        .read()
        .expect("skill mutation policy lock poisoned")
        .clone()
        .unwrap_or_else(|| Arc::new(AllowAllPolicy))
}
