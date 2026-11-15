//! `skillstar update`: check upstream, then overwrite-update what changed.

use std::io::{self, Write};

use ss_core::types::UpstreamChange;
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::skill_lock;
use ss_skills::skill_update::SkillUpdateReport;

/// Options passed to the update handler.
pub struct UpdateOpts<'a> {
    pub name: Option<&'a str>,
    /// Report what upstream has, change nothing.
    pub check: bool,
    /// Print what an update would do, change nothing.
    pub dry_run: bool,
}

/// What a no-argument update works on, from the lock and a fresh check.
#[derive(Debug, Default, PartialEq, Eq)]
struct UpdatePlan {
    apply: Vec<String>,
    /// Applying them overwrites edits made after install.
    overwrites_local_changes: Vec<String>,
    removed_upstream: Vec<String>,
    renamed_upstream: Vec<(String, String)>,
    up_to_date: usize,
    /// Local creations and bundle installs: nothing upstream to compare.
    not_updatable: Vec<String>,
    channel_managed: Vec<String>,
    /// Present upstream but not in the lock. Never installed by update.
    newly_added: Vec<ss_skills::update_added::NewlyAddedSkill>,
}

fn plan(
    lock: &skill_lock::SkillLock,
    states: &[ss_skills::installed_skill::SkillUpdateState],
    is_channel_managed: impl Fn(&str) -> bool,
) -> UpdatePlan {
    let mut plan = UpdatePlan::default();
    for (name, entry) in &lock.skills {
        if !entry.source_type.is_updatable() {
            plan.not_updatable.push(name.clone());
        } else if is_channel_managed(name) {
            plan.channel_managed.push(name.clone());
        }
    }
    for state in states {
        match &state.upstream_change {
            Some(UpstreamChange::Removed { .. }) => {
                plan.removed_upstream.push(state.name.clone());
                continue;
            }
            Some(UpstreamChange::IdentityChanged { upstream_name }) => {
                plan.renamed_upstream
                    .push((state.name.clone(), upstream_name.clone()));
                continue;
            }
            Some(UpstreamChange::LocalChanges { .. }) if state.update_available => {
                plan.overwrites_local_changes.push(state.name.clone());
            }
            _ => {}
        }
        if state.update_available {
            plan.apply.push(state.name.clone());
        } else {
            plan.up_to_date += 1;
        }
    }
    plan
}

fn runtime() -> tokio::runtime::Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("✗ Failed to start async runtime: {error}");
            std::process::exit(1);
        }
    }
}

pub fn cmd_update(opts: UpdateOpts<'_>) {
    let git = GitSkillFacade::from_file_store();
    if let Some(name) = opts.name
        && !opts.check
        && !opts.dry_run
    {
        // A named update is an explicit request: overwrite from upstream
        // whether or not a check would have reported a change.
        let report = git.update_skills(&[name.to_string()]);
        finish(&report);
        return;
    }

    let states = match runtime().block_on(git.refresh_skill_updates()) {
        Ok(states) => states,
        Err(error) => {
            eprintln!("✗ Failed to check for updates: {error:#}");
            std::process::exit(1);
        }
    };
    let lock = skill_lock::load();
    let mut plan = plan(&lock, &states, |name| {
        // Unknown ownership counts as managed: never update it generically.
        ss_skills::skill_mutation::skill_is_channel_managed(name).unwrap_or(true)
    });
    if let Some(name) = opts.name {
        plan.apply.retain(|candidate| candidate == name);
        plan.overwrites_local_changes
            .retain(|candidate| candidate == name);
        plan.removed_upstream.retain(|candidate| candidate == name);
        plan.renamed_upstream
            .retain(|(candidate, _)| candidate == name);
        plan.not_updatable.retain(|candidate| candidate == name);
        plan.channel_managed.retain(|candidate| candidate == name);
        plan.up_to_date = usize::from(
            plan.apply.is_empty()
                && plan.removed_upstream.is_empty()
                && plan.renamed_upstream.is_empty()
                && states.iter().any(|state| state.name == name),
        );
    }
    if opts.check || opts.dry_run {
        plan.newly_added = list_newly_added(&lock);
    }
    print_plan(&plan, opts.check || opts.dry_run);
    if opts.check || opts.dry_run || plan.apply.is_empty() {
        return;
    }
    let report = git.update_skills(&plan.apply);
    finish(&report);
}

fn print_plan(plan: &UpdatePlan, read_only: bool) {
    let verb = if read_only {
        "Would update"
    } else {
        "Updating"
    };
    for name in &plan.apply {
        if plan.overwrites_local_changes.contains(name) {
            println!("↑ {verb} '{name}' (overwrites local changes)");
        } else {
            println!("↑ {verb} '{name}'");
        }
    }
    for name in &plan.removed_upstream {
        println!(
            "- '{name}' is no longer available upstream; remove it or convert it to a local copy."
        );
    }
    for (name, upstream) in &plan.renamed_upstream {
        println!(
            "- {}",
            ss_skills::git_skill::identity_changed_message(name, upstream)
        );
    }
    for name in &plan.channel_managed {
        println!("- Skipped '{name}': a shared channel manages it; use `skillstar channel apply`.");
    }
    if !plan.not_updatable.is_empty() {
        println!(
            "- Skipped {} local or bundled Skill(s) with no upstream: {}",
            plan.not_updatable.len(),
            plan.not_updatable.join(", ")
        );
    }
    if plan.apply.is_empty() {
        println!("All checked Skills are up to date ({}).", plan.up_to_date);
    }
    for skill in &plan.newly_added {
        if skill.internal {
            println!(
                "+ '{}' is new in {} and marked internal. Update will not install it.",
                skill.name, skill.source
            );
        } else {
            println!(
                "+ '{}' is new in {} and is not installed. Update will not install it.",
                skill.name, skill.source
            );
        }
    }
}

/// Scan each updatable source and list skills the lock does not record.
/// This refreshes the import cache. It does not install or write the lock.
fn list_newly_added(lock: &skill_lock::SkillLock) -> Vec<ss_skills::update_added::NewlyAddedSkill> {
    let git = GitSkillFacade::from_file_store();
    let entries = lock
        .skills
        .iter()
        .map(|(name, entry)| (name.clone(), entry.clone()))
        .collect::<Vec<_>>();
    let mut added = Vec::new();
    for ((source_url, git_ref), group) in skill_lock::SkillLock::by_source_group(&entries) {
        if source_url.is_empty()
            || group
                .iter()
                .all(|(_, entry)| !entry.source_type.is_updatable())
        {
            continue;
        }
        let input = match git_ref.as_deref() {
            Some(git_ref) if !git_ref.is_empty() && !source_url.contains('#') => {
                format!("{source_url}#{git_ref}")
            }
            _ => source_url.clone(),
        };
        let scanned = match git.scan_repo_with_refresh(&input, false, true) {
            Ok(scanned) => scanned,
            Err(error) => {
                eprintln!("- Could not list new skills in {source_url}: {error:#}");
                continue;
            }
        };
        let source = group
            .first()
            .map(|(_, entry)| entry.source.clone())
            .unwrap_or(source_url);
        let locked = group
            .iter()
            .map(|(name, entry)| ss_skills::update_added::LockedSkillRef {
                name: name.clone(),
                folder_path: entry.skill_path.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        let discovered = scanned
            .skills
            .iter()
            .map(|skill| ss_skills::update_added::DiscoveredSourceSkill {
                name: skill.id.clone(),
                folder_path: skill.folder_path.clone(),
                installable: skill.installable,
                internal: skill.internal,
            })
            .collect::<Vec<_>>();
        added.extend(ss_skills::update_added::newly_added_skills(
            &source,
            &locked,
            &discovered,
        ));
    }
    added
}

/// Print an applied report; only real failures change the exit code.
fn finish(report: &SkillUpdateReport) {
    if let Err(error) = write_report(report, &mut io::stdout().lock(), &mut io::stderr().lock()) {
        eprintln!("Failed to print update report: {error}");
        std::process::exit(1);
    }
    if !report.failed.is_empty() {
        std::process::exit(1);
    }
}

fn write_report(
    report: &SkillUpdateReport,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    for result in &report.updated {
        writeln!(stdout, "✓ Updated '{}'", result.skill.name)?;
        for failure in &result.agent_link_failures {
            writeln!(stderr, "  ! agent relink failed: {failure}")?;
        }
    }
    for failure in &report.failed {
        writeln!(
            stderr,
            "✗ Failed to update '{}': {}",
            failure.name, failure.error
        )?;
    }
    for name in &report.skipped {
        writeln!(
            stdout,
            "- '{name}' is no longer available upstream; remove it or convert it to a local copy."
        )?;
    }
    for change in &report.identity_changed {
        writeln!(
            stdout,
            "- {}",
            ss_skills::git_skill::identity_changed_message(&change.name, &change.upstream_name)
        )?;
    }
    for name in &report.not_updatable {
        writeln!(
            stdout,
            "- Skipped '{name}': it has no upstream source to update from."
        )?;
    }
    for managed in &report.channel_managed {
        writeln!(
            stdout,
            "- Skipped '{}': a shared channel manages it (repository {}); use `skillstar channel apply {}`.",
            managed.name, managed.repository_id, managed.repository_id
        )?;
    }
    // Project outcomes belong to the full batch, even when several Skills
    // refreshed the same project. Print the aggregate exactly once.
    for project in &report.projects_updated {
        writeln!(stdout, "  project copies refreshed: {project}")?;
    }
    for failure in &report.project_failures {
        writeln!(stderr, "  ! project copy not refreshed: {failure}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss_skills::installed_skill::SkillUpdateState;
    use ss_skills::skill_lock::{SkillLock, SkillLockEntry, SourceType};

    fn entry(source_type: SourceType) -> SkillLockEntry {
        SkillLockEntry {
            source: "o/r".into(),
            source_type,
            source_url: "https://github.com/o/r.git".into(),
            git_ref: None,
            skill_path: None,
            skill_folder_hash: Some("h".into()),
            installed_at: String::new(),
            updated_at: String::new(),
            extra: Default::default(),
        }
    }

    fn state(name: &str, available: bool, change: Option<UpstreamChange>) -> SkillUpdateState {
        SkillUpdateState {
            name: name.into(),
            update_available: available,
            upstream_change: change,
        }
    }

    #[test]
    fn full_report_prints_project_outcomes_once_for_multiple_skills() {
        let result = |name: &str| ss_skills::skill_update::UpdateResult {
            skill: ss_core::types::Skill::from_skills_sh(
                name.into(),
                String::new(),
                0,
                "test".into(),
                String::new(),
            ),
            siblings_cleared: Vec::new(),
            agent_link_failures: Vec::new(),
            project_failures: vec!["missing-project: unavailable".into()],
            projects_updated: vec!["shared-project".into()],
        };
        let report = SkillUpdateReport {
            updated: vec![result("foo"), result("bar")],
            project_failures: vec!["missing-project: unavailable".into()],
            projects_updated: vec!["shared-project".into()],
            ..Default::default()
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        write_report(&report, &mut stdout, &mut stderr).unwrap();

        let stdout = String::from_utf8(stdout).unwrap();
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stdout.contains("Updated 'foo'"), "{stdout}");
        assert!(stdout.contains("Updated 'bar'"), "{stdout}");
        assert_eq!(stdout.matches("shared-project").count(), 1, "{stdout}");
        assert_eq!(stderr.matches("missing-project").count(), 1, "{stderr}");
        assert!(!stderr.contains("Failed to update"), "{stderr}");
    }

    /// No-argument update applies only what the check found changed; local
    /// and channel Skills are reported as skipped, never as failures.
    #[test]
    fn plan_applies_only_available_updates() {
        let mut lock = SkillLock::default();
        for name in ["changed", "same", "gone", "edited", "team"] {
            lock.upsert(name, entry(SourceType::Github));
        }
        lock.upsert("mine", entry(SourceType::Local));
        let states = [
            state("changed", true, None),
            state("same", false, None),
            state(
                "gone",
                false,
                Some(UpstreamChange::Removed {
                    suggested_local_name: "gone.local".into(),
                    successor: None,
                }),
            ),
            state(
                "edited",
                true,
                Some(UpstreamChange::LocalChanges {
                    baseline_missing: false,
                }),
            ),
        ];

        let plan = plan(&lock, &states, |name| name == "team");

        assert_eq!(plan.apply, ["changed", "edited"]);
        assert_eq!(plan.overwrites_local_changes, ["edited"]);
        assert_eq!(plan.removed_upstream, ["gone"]);
        assert_eq!(plan.up_to_date, 1);
        assert_eq!(plan.not_updatable, ["mine"]);
        assert_eq!(plan.channel_managed, ["team"]);
    }
}
