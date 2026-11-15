//! Skill storage health: a read-only [`scan`], a [`plan`] that only proposes
//! changes SkillStar can prove it owns, and an idempotent [`apply`].
//!
//! Content without ownership evidence (unmarked folders, foreign links, edited
//! copies, `SKILL.md`-less folders) is reported and never touched.

mod apply;
mod plan;
mod scan;

pub use apply::{ApplyOptions, RepairOutcome, StepOutcome, StepStatus, apply, apply_with};
pub use plan::{
    PlanOptions, RepairAction, RepairPlan, RepairStep, UntouchedIssue, plan, plan_with,
};
pub use scan::{HealthIssue, IssueKind, SkillHealthReport, scan};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod production_tests;
