//! Mutation-gate policy implementation for the shared-channel domain.
//!
//! [`ChannelAwarePolicy`] implements
//! `skillstar_skills::skill_mutation::SkillMutationPolicy` by consulting the
//! channel subscription registry: generic (non-channel) mutation paths in
//! `skillstar-skills` reject skills/repositories that a shared channel
//! manages. Application composition roots (Tauri setup, CLI entry) call
//! [`install_global_policy`] once; until then the allow-all default applies.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skillstar_skills::skill_mutation::SkillMutationPolicy;

/// Policy that consults the on-disk channel subscription registry.
pub struct ChannelAwarePolicy;

impl SkillMutationPolicy for ChannelAwarePolicy {
    fn ensure_skill_mutation_allowed(&self, skill_id: &str) -> anyhow::Result<()> {
        crate::shared_channels::ensure_generic_skill_mutation_allowed(skill_id)
    }

    fn ensure_repository_mutation_allowed(&self, repository_url: &str) -> anyhow::Result<()> {
        crate::shared_channels::ensure_generic_repository_mutation_allowed(repository_url)
    }

    fn installed_skill_is_mutable(
        &self,
        skill_id: &str,
        skill_path: &Path,
    ) -> anyhow::Result<bool> {
        crate::shared_channels::generic_installed_skill_is_mutable(skill_id, skill_path)
    }

    fn managed_repository_for_skill(&self, skill_id: &str) -> anyhow::Result<Option<u64>> {
        crate::shared_channels::managed_repository_for_skill(skill_id).map_err(anyhow::Error::from)
    }

    fn managed_repository_for_url(&self, repository_url: &str) -> anyhow::Result<Option<u64>> {
        crate::shared_channels::managed_repository_for_url(repository_url)
            .map_err(anyhow::Error::from)
    }

    fn on_bulk_skill_removal(&self, skill_ids: &[String]) -> anyhow::Result<()> {
        crate::shared_channels::prune_removed_skills(skill_ids)
            .map(|_| ())
            .map_err(anyhow::Error::from)
    }

    fn provenance_paths(&self) -> Vec<PathBuf> {
        crate::shared_channels::subscription_provenance_paths()
    }
}

/// Install [`ChannelAwarePolicy`] as the process-wide mutation gate.
///
/// Safe to call multiple times; the first registration wins, so a later caller
/// can never downgrade the gate. Must be called before any generic skill
/// mutation path runs (GUI setup, CLI entry).
pub fn install_global_policy() {
    skillstar_skills::skill_mutation::set_skill_mutation_policy(Arc::new(ChannelAwarePolicy));
}
