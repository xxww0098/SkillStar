use super::{auto_update_locked_skills, project_auto_update_report};
use crate::git::transport::{
    GitAuthMaterial, GitOperationPhase, GitOperationProgress, GitOperationSession, GitProgressSink,
};
use crate::git_skill::GitSkillFacade;
use crate::installer::{self, InstallUnit};
use crate::projects::{self, ProjectDeployMode, ProjectEntry};
use crate::skill_update::{SkillUpdateFailure, SkillUpdateReport, UpdateResult};
use crate::test_sandbox::Sandbox;
use ss_core::infra::paths;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn update_result(name: &str, agent_failures: &[&str]) -> UpdateResult {
    UpdateResult {
        skill: ss_core::types::Skill::from_skills_sh(
            name.into(),
            String::new(),
            0,
            "test".into(),
            String::new(),
        ),
        siblings_cleared: Vec::new(),
        agent_link_failures: agent_failures
            .iter()
            .map(|failure| (*failure).into())
            .collect(),
        project_failures: Vec::new(),
        projects_updated: Vec::new(),
    }
}

#[test]
fn auto_projection_preserves_successes_and_all_deployment_failures() {
    let outcome = SkillUpdateReport {
        updated: vec![
            update_result("foo", &["codex: unavailable"]),
            update_result("bar", &["cursor: unavailable"]),
        ],
        skipped: vec!["removed".into()],
        kept_local: vec!["edited".into()],
        failed: vec![SkillUpdateFailure {
            name: "broken".into(),
            error: "fetch failed".into(),
        }],
        project_failures: vec!["missing-project: unavailable".into()],
        projects_updated: vec!["good-project".into()],
        ..Default::default()
    };

    let report = project_auto_update_report(7, outcome);

    assert_eq!(report.checked, 7);
    assert_eq!(report.updated, ["foo", "bar"]);
    assert_eq!(report.skipped, ["removed"]);
    assert_eq!(report.kept_local, ["edited"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].name, "broken");
    assert_eq!(report.failed[0].error, "fetch failed");
    assert_eq!(report.project_failures, ["missing-project: unavailable"]);
    assert_eq!(report.projects_updated, ["good-project"]);
    assert_eq!(
        report.agent_link_failures,
        ["codex: unavailable", "cursor: unavailable"]
    );
    assert!(report.error.is_none());
}

#[test]
fn deployment_only_outcomes_survive_auto_projection() {
    let report = project_auto_update_report(
        2,
        SkillUpdateReport {
            project_failures: vec!["missing-project: unavailable".into()],
            projects_updated: vec!["good-project".into()],
            ..Default::default()
        },
    );
    assert!(report.updated.is_empty());
    assert!(report.failed.is_empty());
    assert_eq!(report.project_failures, ["missing-project: unavailable"]);
    assert_eq!(report.projects_updated, ["good-project"]);
}

/// Pinned file repositories exercise the real refetch and scoped apply workers
/// offline; unpinned file sources would be borrowed without any remote Git.
struct PinnedUpstream {
    root: PathBuf,
    name: String,
}

impl PinnedUpstream {
    fn install(env: &Sandbox, name: &str) -> Self {
        let fixture = Self {
            root: env.root().join(format!("upstream-{name}")),
            name: name.into(),
        };
        fs::create_dir_all(&fixture.root).unwrap();
        fixture.git(&["init", "--quiet", "--initial-branch=main"]);
        fixture.commit("v1");
        let mut spec =
            crate::source_resolver::Source::parse(&crate::git::ops::local_file_url(&fixture.root))
                .unwrap();
        spec.git_ref = Some("main".into());
        let session = GitOperationSession::public();
        let checkout = crate::fetch::fetch_source(&spec, &session).unwrap();
        installer::install_units(
            checkout.dir(),
            &spec,
            &[InstallUnit {
                id: name.into(),
                folder_path: format!("skills/{name}"),
            }],
        )
        .unwrap();
        crate::skill_lock::mutate(|lock| {
            lock.skills.get_mut(name).unwrap().source_type = crate::skill_lock::SourceType::Github;
        })
        .unwrap();
        fixture
    }

    fn commit(&self, version: &str) {
        let source = self.root.join("skills").join(&self.name);
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: {version}\n---\n{version}\n",
                self.name
            ),
        )
        .unwrap();
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", version, "--no-gpg-sign"]);
    }

    fn git(&self, args: &[&str]) {
        let output = ss_core::infra::path_env::command_with_path("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn deploy_project(env: &Sandbox, name: &str, skills: &[&str]) -> ProjectEntry {
    let root = env.root().join(name);
    fs::create_dir_all(&root).unwrap();
    projects::save_and_sync(
        root.to_str().unwrap(),
        HashMap::from([(
            "codex".into(),
            skills.iter().map(|skill| (*skill).to_string()).collect(),
        )]),
        HashMap::from([(".agents/skills".into(), ProjectDeployMode::Copy)]),
    )
    .unwrap();
    projects::register_project(root.to_str().unwrap()).unwrap()
}

fn assert_updated_copy(project: &ProjectEntry, skill: &str) {
    let target = Path::new(&project.path).join(".agents/skills").join(skill);
    assert!(!ss_core::infra::fs_ops::is_link(&target));
    assert!(
        fs::read_to_string(target.join("SKILL.md"))
            .unwrap()
            .contains("v2")
    );
}

#[derive(Default)]
struct UnlockedFetchProbe {
    calls: AtomicUsize,
}

impl GitProgressSink for UnlockedFetchProbe {
    fn emit(&self, progress: GitOperationProgress) {
        if progress.phase != GitOperationPhase::Preparing {
            return;
        }
        assert_eq!(
            crate::skill_update::transaction::transaction_depth_for_test(),
            0
        );
        // Detect a parent-thread guard as well as a worker-thread guard. A
        // caller holding the lock while joining workers would self-deadlock.
        let _guard = crate::skill_update::try_acquire_update_transaction_lock(Duration::ZERO)
            .expect("refetch must not run beneath a parent update guard");
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn single_update_returns_project_failures_and_successes_after_unlocked_refetch() {
    let env = Sandbox::new();
    let upstream = PinnedUpstream::install(&env, "foo");
    let missing = deploy_project(&env, "missing", &["foo"]);
    let edited = deploy_project(&env, "edited", &["foo"]);
    let good = deploy_project(&env, "good", &["foo"]);
    fs::remove_dir_all(&missing.path).unwrap();
    let edited_body = Path::new(&edited.path).join(".agents/skills/foo/SKILL.md");
    fs::write(&edited_body, "local project edit").unwrap();
    upstream.commit("v2");
    let probe = Arc::new(UnlockedFetchProbe::default());
    let session =
        GitOperationSession::new("report-test", GitAuthMaterial::missing(), probe.clone());

    let result = GitSkillFacade::new(session).update_skill("foo").unwrap();

    assert!(
        probe.calls.load(Ordering::SeqCst) > 0,
        "no refetch was exercised"
    );
    assert_eq!(result.skill.name, "foo");
    assert_eq!(result.projects_updated, [good.name.clone()]);
    assert_eq!(result.project_failures.len(), 2, "{result:?}");
    assert!(
        result
            .project_failures
            .iter()
            .any(|failure| failure.contains(&missing.name))
    );
    assert!(
        result
            .project_failures
            .iter()
            .any(|failure| failure.contains(&edited.name))
    );
    assert_updated_copy(&good, "foo");
    assert_eq!(
        fs::read_to_string(edited_body).unwrap(),
        "local project edit"
    );
    assert!(!Path::new(&missing.path).exists());
    assert!(
        fs::read_to_string(paths::agents_skill_dir("foo").join("SKILL.md"))
            .unwrap()
            .contains("v2")
    );
}

#[test]
fn batch_project_failure_does_not_block_other_projects_or_source_workers() {
    let env = Sandbox::new();
    let foo = PinnedUpstream::install(&env, "foo");
    let bar = PinnedUpstream::install(&env, "bar");
    let missing = deploy_project(&env, "missing", &["foo", "bar"]);
    let good = deploy_project(&env, "good", &["foo", "bar"]);
    fs::remove_dir_all(&missing.path).unwrap();
    foo.commit("v2");
    bar.commit("v2");

    let report = GitSkillFacade::new(GitOperationSession::public())
        .update_skills(&["foo".into(), "bar".into()]);

    assert_eq!(
        report
            .updated
            .iter()
            .map(|result| result.skill.name.as_str())
            .collect::<Vec<_>>(),
        ["foo", "bar"]
    );
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.projects_updated, [good.name.clone()]);
    assert_eq!(report.project_failures.len(), 1, "{report:?}");
    assert!(report.project_failures[0].contains(&missing.name));
    for result in &report.updated {
        assert!(result.project_failures.is_empty());
        assert!(result.projects_updated.is_empty());
        assert_updated_copy(&good, &result.skill.name);
        assert_eq!(crate::update_state::get(&result.skill.name), Some(false));
    }
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["projects_updated"], serde_json::json!([good.name]));
}

#[test]
fn auto_update_preserves_project_outcomes_without_failing_successful_skills() {
    let env = Sandbox::new();
    let upstream = PinnedUpstream::install(&env, "foo");
    let missing = deploy_project(&env, "missing", &["foo"]);
    let good = deploy_project(&env, "good", &["foo"]);
    fs::remove_dir_all(&missing.path).unwrap();
    upstream.commit("v2");
    let probe = Arc::new(UnlockedFetchProbe::default());
    let session =
        GitOperationSession::new("report-test", GitAuthMaterial::missing(), probe.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let report = runtime.block_on(auto_update_locked_skills(&session));

    assert!(
        probe.calls.load(Ordering::SeqCst) > 0,
        "no remote Git was exercised"
    );
    assert_eq!(report.checked, 1);
    assert_eq!(report.updated, ["foo"]);
    assert_eq!(report.projects_updated, [good.name.clone()]);
    assert_eq!(report.project_failures.len(), 1, "{report:?}");
    assert!(report.project_failures[0].contains(&missing.name));
    assert!(report.failed.is_empty(), "{report:?}");
    assert!(report.agent_link_failures.is_empty(), "{report:?}");
    assert!(report.error.is_none(), "{report:?}");
    assert_updated_copy(&good, "foo");
    assert_eq!(crate::update_state::get("foo"), Some(false));
}
