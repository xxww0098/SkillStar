//! Explicit intake of skills an Agent already installed.
//!
//! This is not the storage doctor ([`crate::health`]). `skillstar doctor --fix`
//! and the Settings repair action never call [`repair_installations`]. The
//! function runs the same plan as `skillstar doctor --adopt --apply`: a new
//! skill is copied into local storage and the Agent directory becomes a
//! relative link to the canonical entry; a byte-identical directory is only
//! relinked. Conflicts, directories that contain `.git` or another excluded
//! name, and agents that read the canonical root are reported and left in place.

use anyhow::Result;

use super::intake;

/// Per-run outcomes; failures preserve their source and do not stop other skills.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct SkillRepairReport {
    pub repaired: usize,
    pub issues: Vec<SkillRepairIssue>,
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SkillRepairIssue {
    pub name: String,
    pub reason: String,
}

impl SkillRepairIssue {
    pub fn maintenance(reason: String) -> Self {
        Self {
            name: String::new(),
            reason,
        }
    }
}

/// Take Agent-installed skills under management. Conflicting versions and
/// excluded directories are left untouched and returned to the caller.
pub fn repair_installations() -> Result<SkillRepairReport> {
    let scanned = intake::scan();
    let planned = intake::plan(&scanned);
    let outcome = intake::apply(&planned)?;
    let mut report = SkillRepairReport {
        repaired: outcome.applied_count(),
        ..SkillRepairReport::default()
    };
    for finding in &planned.reported {
        report.issues.push(SkillRepairIssue {
            name: finding.skill.clone(),
            reason: finding.reason.clone(),
        });
    }
    for step in &outcome.steps {
        let reason = match &step.status {
            intake::IntakeStatus::Failed { error } => error.clone(),
            intake::IntakeStatus::Skipped { reason } => reason.clone(),
            _ => continue,
        };
        report.issues.push(SkillRepairIssue {
            name: step.step.skill.clone(),
            reason,
        });
    }
    Ok(report)
}
