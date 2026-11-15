mod db;
pub mod remote;
pub mod snapshot;

pub use remote::{
    MarketplaceResult, MarketplaceSkillDetails, PublisherRepo, PublisherRepoSkill, SecurityAudit,
    SkillSecurityAudit, fetch_public_github_skill_audits, may_query_install_audits,
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
