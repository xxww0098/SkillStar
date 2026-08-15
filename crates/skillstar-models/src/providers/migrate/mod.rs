//! Store migrations.
//!
//! The chain is `v1 → v2 → v3 → v4`, and every link stays: a user who has not
//! opened SkillStar since the v1 store must still land on v4 with their
//! providers intact. `store.rs` owns the v1→v2→v3 links; this module owns
//! v3→v4 and the safety envelope around it.

mod report;
mod v3_to_v4;

#[cfg(test)]
mod tests;

pub use report::{
    BackfilledEndpoint, DropReason, DroppedBinding, ExternalizedCatalog, MigrationReport,
};
pub use v3_to_v4::{
    ExtractedCatalog, MigrationOutcome, PLANNED_AGENT_CLAUDE_DESKTOP, ROLE_DEFAULT, ROLE_FAST,
    ROLE_PLAN, ROLE_SUBAGENT, ROLE_VISION, canonical_role_key, migrate_v3_to_v4,
};
