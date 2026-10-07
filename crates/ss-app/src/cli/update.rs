//! `skillstar update`: check upstream, then overwrite-update what changed.

use ss_core::types::UpstreamChange;
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::skill_lock;
use ss_skills::skill_update::{SkillUpdateReport, UpdateResult};

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
}

/// Print an applied report; only real failures change the exit code.
fn finish(report: &SkillUpdateReport) {
    for result in &report.updated {
        print_update_result(result);
    }
    for failure in &report.failed {
        eprintln!("✗ Failed to update '{}': {}", failure.name, failure.error);
    }
    for name in &report.skipped {
        println!(
            "- '{name}' is no longer available upstream; remove it or convert it to a local copy."
        );
    }
    for change in &report.identity_changed {
        println!(
            "- {}",
            ss_skills::git_skill::identity_changed_message(&change.name, &change.upstream_name)
        );
    }
    for name in &report.not_updatable {
        println!("- Skipped '{name}': it has no upstream source to update from.");
    }
    for managed in &report.channel_managed {
        println!(
            "- Skipped '{}': a shared channel manages it (repository {}); use `skillstar channel apply {}`.",
            managed.name, managed.repository_id, managed.repository_id
        );
    }
    for failure in &report.project_failures {
        eprintln!("  ! project copy not refreshed: {failure}");
    }
    if !report.failed.is_empty() {
        std::process::exit(1);
    }
}

fn print_update_result(result: &UpdateResult) {
    println!("✓ Updated '{}'", result.skill.name);
    for failure in &result.agent_link_failures {
        eprintln!("  ! agent relink failed: {}", failure);
    }
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
