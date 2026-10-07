//! Mutation-gate policy implementation for the shared-channel domain.
//!
//! [`ChannelAwarePolicy`] implements
//! `crate::skill_mutation::SkillMutationPolicy` by consulting the
//! channel subscription registry: generic (non-channel) mutation paths in
//! `ss-skills` reject skills/repositories that a shared channel
//! manages. This is the default policy for every skill-domain caller.

use std::path::PathBuf;

use crate::skill_mutation::SkillMutationPolicy;

/// Policy that consults the on-disk channel subscription registry.
pub struct ChannelAwarePolicy;

impl SkillMutationPolicy for ChannelAwarePolicy {
    fn ensure_skill_mutation_allowed(&self, skill_id: &str) -> anyhow::Result<()> {
        crate::channels::shared_channels::ensure_generic_skill_mutation_allowed(skill_id)
    }

    fn ensure_repository_mutation_allowed(&self, repository_url: &str) -> anyhow::Result<()> {
        crate::channels::shared_channels::ensure_generic_repository_mutation_allowed(repository_url)
    }

    fn managed_repository_for_skill(&self, skill_id: &str) -> anyhow::Result<Option<u64>> {
        crate::channels::shared_channels::managed_repository_for_skill(skill_id)
            .map_err(anyhow::Error::from)
    }

    fn on_bulk_skill_removal(&self, skill_ids: &[String]) -> anyhow::Result<()> {
        crate::channels::shared_channels::prune_removed_skills(skill_ids)
            .map(|_| ())
            .map_err(anyhow::Error::from)
    }

    fn provenance_paths(&self) -> Vec<PathBuf> {
        crate::channels::shared_channels::subscription_provenance_paths()
    }
}
