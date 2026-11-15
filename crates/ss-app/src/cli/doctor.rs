//! `skillstar doctor`: report skill-storage health, and repair only what
//! SkillStar can prove it owns.
//!
//! Agent skills that can be taken under management are listed too. `--fix`
//! does not adopt them. `--adopt` previews that intake; `--adopt --apply`
//! carries it out.

use serde::Serialize;
use ss_skills::health::{self, RepairOutcome, RepairPlan, SkillHealthReport, StepStatus};
use ss_skills::local_skill::{
    IntakeApplyOptions, IntakeOutcome, IntakePlan, IntakeReport, apply_agent_intake_with,
    plan_agent_intake, scan_agent_intake,
};

#[derive(Serialize)]
struct DoctorOutput<'a> {
    report: &'a SkillHealthReport,
    plan: &'a RepairPlan,
    intake: &'a IntakeReport,
    intake_plan: &'a IntakePlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<&'a RepairOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intake_outcome: Option<&'a IntakeOutcome>,
    dry_run: bool,
    adopt_dry_run: bool,
}

pub fn cmd_doctor(json: bool, fix: bool, dry_run: bool, adopt: bool, apply_intake: bool) {
    let report = health::scan();
    let plan = health::plan(&report);
    let intake = scan_agent_intake();
    let intake_plan = plan_agent_intake(&intake);
    let outcome = if fix {
        match health::apply_with(&plan, health::ApplyOptions { dry_run }) {
            Ok(outcome) => Some(outcome),
            Err(error) => {
                eprintln!("doctor: {error:#}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    let intake_outcome = if adopt {
        match apply_agent_intake_with(
            &intake_plan,
            IntakeApplyOptions {
                dry_run: !apply_intake,
            },
        ) {
            Ok(outcome) => Some(outcome),
            Err(error) => {
                eprintln!("doctor: {error:#}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    if json {
        let output = DoctorOutput {
            report: &report,
            plan: &plan,
            intake: &intake,
            intake_plan: &intake_plan,
            outcome: outcome.as_ref(),
            intake_outcome: intake_outcome.as_ref(),
            dry_run: fix && dry_run,
            adopt_dry_run: adopt && !apply_intake,
        };
        match serde_json::to_string_pretty(&output) {
            Ok(text) => println!("{text}"),
            Err(error) => {
                eprintln!("doctor: {error}");
                std::process::exit(1);
            }
        }
    } else {
        print_human(
            &report,
            &plan,
            &intake_plan,
            outcome.as_ref(),
            intake_outcome.as_ref(),
            fix && dry_run,
            adopt && !apply_intake,
        );
    }

    if outcome
        .as_ref()
        .is_some_and(|outcome| outcome.failed().next().is_some())
        || intake_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.failed().next().is_some())
    {
        std::process::exit(1);
    }
}

fn print_human(
    report: &SkillHealthReport,
    plan: &RepairPlan,
    intake_plan: &IntakePlan,
    outcome: Option<&RepairOutcome>,
    intake_outcome: Option<&IntakeOutcome>,
    dry_run: bool,
    adopt_dry_run: bool,
) {
    println!(
        "Skill storage: {} healthy, {} issue(s).",
        report.healthy_count,
        report.issues.len()
    );
    if !report.lock_writable {
        println!("The install lock cannot be written safely; repair will not change it.");
    }
    for issue in &report.issues {
        let skill = issue.skill.as_deref().unwrap_or("-");
        println!(
            "  - {} {} {}",
            tag(&issue.kind),
            skill,
            issue.path.display()
        );
    }
    println!(
        "Repair plan: {} step(s), {} left untouched.",
        plan.steps.len(),
        plan.untouched.len()
    );
    for step in &plan.steps {
        let skill = step.skill.as_deref().unwrap_or("-");
        println!("  - {} {} — {}", tag(&step.action), skill, step.evidence);
    }
    for item in &plan.untouched {
        let skill = item.issue.skill.as_deref().unwrap_or("-");
        println!("  · {skill}: {}", item.reason);
    }
    println!(
        "Agent skills that can be managed: {} step(s), {} left in place.",
        intake_plan.steps.len(),
        intake_plan.reported.len()
    );
    for step in &intake_plan.steps {
        println!(
            "  - {} {} ({}) {} — {}",
            tag(&step.action),
            step.skill,
            step.agent_id,
            step.path.display(),
            step.reason
        );
    }
    for item in &intake_plan.reported {
        println!(
            "  · {} ({}) {}: {}",
            item.skill,
            item.agent_id,
            item.path.display(),
            item.reason
        );
    }
    if intake_outcome.is_none() && !intake_plan.steps.is_empty() {
        println!(
            "Preview with `skillstar doctor --adopt`. Apply with `skillstar doctor --adopt --apply`."
        );
    }
    print_repair_outcome(outcome, dry_run);
    print_intake_outcome(intake_outcome, adopt_dry_run);
}

fn print_repair_outcome(outcome: Option<&RepairOutcome>, dry_run: bool) {
    let Some(outcome) = outcome else {
        return;
    };
    if dry_run {
        println!(
            "Dry run: {} step(s) would be applied. Nothing was written.",
            outcome.would_apply_count()
        );
    } else {
        println!("Applied {} step(s).", outcome.applied_count());
    }
    for step in &outcome.steps {
        let skill = step.step.skill.as_deref().unwrap_or("-");
        println!("  - {skill}: {}", status_line(&step.status));
    }
}

fn print_intake_outcome(outcome: Option<&IntakeOutcome>, dry_run: bool) {
    let Some(outcome) = outcome else {
        return;
    };
    if dry_run {
        println!(
            "Adopt dry run: {} step(s) would be applied. Nothing was written.",
            outcome.would_apply_count()
        );
    } else {
        println!("Adopted {} step(s).", outcome.applied_count());
    }
    for step in &outcome.steps {
        println!(
            "  - {} ({}): {}",
            step.step.skill,
            step.step.agent_id,
            intake_status(&step.status)
        );
    }
}

fn status_line(status: &StepStatus) -> String {
    match status {
        StepStatus::Applied => "applied".to_string(),
        StepStatus::WouldApply => "would apply".to_string(),
        StepStatus::AlreadyDone => "already done".to_string(),
        StepStatus::Skipped { reason } => format!("skipped: {reason}"),
        StepStatus::Failed { error } => format!("failed: {error}"),
    }
}

fn intake_status(status: &ss_skills::local_skill::IntakeStatus) -> String {
    match status {
        ss_skills::local_skill::IntakeStatus::Applied => "applied".to_string(),
        ss_skills::local_skill::IntakeStatus::WouldApply => "would apply".to_string(),
        ss_skills::local_skill::IntakeStatus::AlreadyDone => "already done".to_string(),
        ss_skills::local_skill::IntakeStatus::Skipped { reason } => format!("skipped: {reason}"),
        ss_skills::local_skill::IntakeStatus::Failed { error } => format!("failed: {error}"),
    }
}

fn tag(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| {
            value
                .get("kind")
                .or_else(|| value.get("action"))
                .and_then(|tag| tag.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "item".to_string())
}
