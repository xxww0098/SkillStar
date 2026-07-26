mod db;
pub mod mcp_models;
mod mcp_remote;
pub mod mcp_snapshot;
mod models;
pub mod remote;
pub mod snapshot;

pub use mcp_models::{
    McpMarketEntry, McpMarketServerDetail, McpPublisherSummary, McpRegistryPackageSummary,
    McpRegistryRemoteSummary, McpRegistryServer, McpServerKind,
};

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
    AiKeywordSearchResult, MarketplaceResult, MarketplaceSkillDetails, PublisherRepo,
    PublisherRepoSkill, SecurityAudit,
};
pub use skillstar_core::types::skill::{
    OfficialPublisher, Skill, SkillCategory, SkillType, extract_github_source_from_url,
};
pub use snapshot::{
    LocalFirstResult, MarketplacePack, SnapshotRuntimeConfig, SnapshotStatus, SyncStateEntry,
};

#[cfg(test)]
mod contract_tests {
    use super::Skill;

    #[test]
    fn marketplace_skill_is_the_core_contract() {
        fn accepts_marketplace_skill(_: Skill) {}
        let _: fn(skillstar_core::types::Skill) = accepts_marketplace_skill;
    }
}
