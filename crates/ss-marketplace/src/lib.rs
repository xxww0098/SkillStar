mod db;
mod models;
pub mod remote;
pub mod snapshot;

pub use models::{
    CuratedRegistryEntry, CuratedRegistryKind, CuratedRegistryUpsert, MarketplaceCategory,
    MarketplaceCategoryUpsert, MarketplaceRatingSummary, MarketplaceRatingSummaryUpsert,
    MarketplaceReview, MarketplaceReviewUpsert, MarketplaceSkillCategoryAssignment,
    MarketplaceSkillCategoryAssignmentInput, MarketplaceSkillTagAssignment,
    MarketplaceSkillTagAssignmentInput, MarketplaceSourceObservation,
    MarketplaceSourceObservationUpsert, MarketplaceSourceSummary, MarketplaceTag,
    MarketplaceTagUpsert, MarketplaceUpdateNotification, MarketplaceUpdateNotificationUpsert,
};
pub use remote::{
    MarketplaceResult, MarketplaceSkillDetails, PublisherRepo, PublisherRepoSkill, SecurityAudit,
};
pub use snapshot::{LocalFirstResult, SnapshotRuntimeConfig, SnapshotStatus, SyncStateEntry};
pub use ss_core::types::skill::{
    OfficialPublisher, Skill, SkillCategory, SkillType, extract_github_source_from_url,
};

#[cfg(test)]
mod contract_tests {
    use super::Skill;

    #[test]
    fn marketplace_skill_is_the_core_contract() {
        fn accepts_marketplace_skill(_: Skill) {}
        let _: fn(ss_core::types::Skill) = accepts_marketplace_skill;
    }
}
