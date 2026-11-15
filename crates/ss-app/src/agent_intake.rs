//! Preview and apply Agent-skill intake. The storage doctor does not call this.

use ss_core::infra::error::AppError;
use ss_skills::local_skill::{
    IntakeAction, IntakeApplyOptions, IntakeKind, apply_agent_intake as apply_intake,
    apply_agent_intake_with, plan_agent_intake, scan_agent_intake,
};

use super::SkillRepairApplyReport;

/// One Agent skill the storage page can list before anyone adopts it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntakeListItem {
    pub agent_id: String,
    pub skill: String,
    /// `adopt`, `relink`, `conflict`, `excluded`, `foreign`, or `occupied`.
    pub kind: String,
}

pub(super) fn intake_rows() -> Vec<IntakeListItem> {
    let report = scan_agent_intake();
    let planned = plan_agent_intake(&report);
    let mut rows = Vec::new();
    for step in &planned.steps {
        rows.push(IntakeListItem {
            agent_id: step.agent_id.clone(),
            skill: step.skill.clone(),
            kind: action_kind(step.action).to_string(),
        });
    }
    for finding in &planned.reported {
        rows.push(IntakeListItem {
            agent_id: finding.agent_id.clone(),
            skill: finding.skill.clone(),
            kind: finding_kind(finding.kind).to_string(),
        });
    }
    rows
}

fn action_kind(action: IntakeAction) -> &'static str {
    match action {
        IntakeAction::Adopt => "adopt",
        IntakeAction::Relink => "relink",
    }
}

fn finding_kind(kind: IntakeKind) -> &'static str {
    match kind {
        IntakeKind::Adopt => "adopt",
        IntakeKind::Relink => "relink",
        IntakeKind::Conflict => "conflict",
        IntakeKind::Excluded => "excluded",
        IntakeKind::Foreign => "foreign",
        IntakeKind::Occupied => "occupied",
    }
}

/// What the storage page shows before adopting Agent skills. Nothing is written.
#[derive(Debug)]
pub struct AgentIntakePreview {
    pub steps: Vec<String>,
    pub reported: Vec<String>,
    pub would_apply: usize,
}

pub async fn preview_agent_intake() -> Result<AgentIntakePreview, AppError> {
    tokio::task::spawn_blocking(|| -> Result<AgentIntakePreview, AppError> {
        let report = scan_agent_intake();
        let planned = plan_agent_intake(&report);
        let outcome = apply_agent_intake_with(&planned, IntakeApplyOptions { dry_run: true })?;
        Ok(AgentIntakePreview {
            would_apply: outcome.would_apply_count(),
            steps: outcome
                .steps
                .iter()
                .map(|step| {
                    format!(
                        "{} {} ({}) — {}",
                        action_kind(step.step.action),
                        step.step.skill,
                        step.step.agent_id,
                        step.step.reason
                    )
                })
                .collect(),
            reported: planned
                .reported
                .iter()
                .map(|item| format!("{} ({}): {}", item.skill, item.agent_id, item.reason))
                .collect(),
        })
    })
    .await?
}

pub async fn apply_agent_intake() -> Result<SkillRepairApplyReport, AppError> {
    tokio::task::spawn_blocking(|| -> Result<SkillRepairApplyReport, AppError> {
        let report = scan_agent_intake();
        let planned = plan_agent_intake(&report);
        let outcome = apply_intake(&planned)?;
        let mut failed = Vec::new();
        for step in &outcome.steps {
            match &step.status {
                ss_skills::local_skill::IntakeStatus::Failed { error } => {
                    failed.push(format!("{}: {error}", step.step.skill));
                }
                ss_skills::local_skill::IntakeStatus::Skipped { reason } => {
                    failed.push(format!("{}: {reason}", step.step.skill));
                }
                _ => {}
            }
        }
        Ok(SkillRepairApplyReport {
            applied: outcome.applied_count(),
            failed,
        })
    })
    .await?
}
