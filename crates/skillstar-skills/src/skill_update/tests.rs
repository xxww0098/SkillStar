use super::*;
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::git::transport::{
    GitAuthMaterial, GitOperationProgress, GitOperationSession, GitProgressSink,
};
use crate::git_skill::GitSkillFacade;

struct TestHub {
    previous_env: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _temp: tempfile::TempDir,
}

impl TestHub {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let overrides = [
            ("SKILLSTAR_HUB_DIR", temp.path().join("hub")),
            ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
            ("SKILLSTAR_TOOL_SYNC_HOME", temp.path().join("tool-home")),
            ("HOME", temp.path().join("home")),
            ("USERPROFILE", temp.path().join("home")),
            ("GIT_CONFIG_GLOBAL", temp.path().join("gitconfig")),
        ];
        let previous_env = overrides
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        unsafe {
            for (key, value) in overrides {
                std::env::set_var(key, value);
            }
        }
        Self {
            previous_env,
            _temp: temp,
        }
    }
}

#[derive(Default)]
struct RecordingGitProgress(Mutex<Vec<GitOperationProgress>>);

impl GitProgressSink for RecordingGitProgress {
    fn emit(&self, progress: GitOperationProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        unsafe {
            for (key, previous) in self.previous_env.drain(..).rev() {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn committed_remote() -> tempfile::TempDir {
    let remote = tempfile::tempdir().unwrap();
    run_git(remote.path(), &["init", "--initial-branch=main"]);
    run_git(remote.path(), &["config", "user.email", "test@example.com"]);
    run_git(remote.path(), &["config", "user.name", "SkillStar Tests"]);
    std::fs::create_dir_all(remote.path().join("scripts")).unwrap();
    std::fs::write(
        remote.path().join("SKILL.md"),
        "---\ndescription: v1\n---\n",
    )
    .unwrap();
    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v1\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v1"]);
    remote
}

fn committed_multi_skill_remote() -> tempfile::TempDir {
    let remote = tempfile::tempdir().unwrap();
    run_git(remote.path(), &["init", "--initial-branch=main"]);
    run_git(remote.path(), &["config", "user.email", "test@example.com"]);
    run_git(remote.path(), &["config", "user.name", "SkillStar Tests"]);
    for name in ["alpha", "beta"] {
        let directory = remote.path().join("skills").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} v1\n---\n"),
        )
        .unwrap();
    }
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v1"]);
    remote
}

fn committed_root_and_nested_remote() -> tempfile::TempDir {
    let remote = committed_multi_skill_remote();
    std::fs::write(
        remote.path().join("SKILL.md"),
        "---\nname: root\ndescription: root v1\n---\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "root skill"]);
    remote
}

#[test]
fn high_level_facade_scans_installs_and_updates_private_github_without_persisting_token() {
    const TOKEN: &str = "github_pat_high_level_private_canary";
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();
    let global_config = std::env::var_os("GIT_CONFIG_GLOBAL").unwrap();
    let remote_url = "https://github.com/acme/private-skills.git";
    std::fs::write(
        &global_config,
        format!(
            "[url \"file://{}\"]\n\tinsteadOf = {remote_url}\n",
            remote.path().display()
        ),
    )
    .unwrap();

    let progress = Arc::new(RecordingGitProgress::default());
    let session = GitOperationSession::new(
        "private-facade",
        GitAuthMaterial::available(TOKEN),
        progress.clone(),
    );
    let facade = GitSkillFacade::new(session.clone());

    let scan = facade.scan_repo(remote_url, true).unwrap();
    assert_eq!(scan.source_url, remote_url);
    assert_eq!(scan.skills.len(), 1);
    let target = crate::repo_scanner::SkillInstallTarget {
        id: scan.skills[0].id.clone(),
        folder_path: scan.skills[0].folder_path.clone(),
    };
    let installed = facade
        .install_from_scan(
            &scan.source,
            &scan.source_url,
            std::slice::from_ref(&target),
        )
        .unwrap();
    assert_eq!(installed.as_slice(), std::slice::from_ref(&target.id));

    std::fs::write(remote.path().join("scripts/run.sh"), "echo private-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "private-v2"]);
    facade.update_skill(&target.id).unwrap();

    let installed_path = skillstar_core::infra::paths::hub_skills_dir().join(&target.id);
    assert_eq!(
        std::fs::read_to_string(installed_path.join("scripts/run.sh")).unwrap(),
        "echo private-v2\n"
    );

    std::fs::write(
        remote.path().join("scripts/run.sh"),
        "echo unreachable-v3\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "unreachable-v3"]);
    std::fs::write(
        &global_config,
        format!(
            "[url \"file://{}/missing.git\"]\n\tinsteadOf = {remote_url}\n",
            remote.path().display()
        ),
    )
    .unwrap();
    let failed_update = facade
        .update_skill(&target.id)
        .expect_err("unreachable private remote must fail");
    assert!(!format!("{failed_update:#}").contains(TOKEN));
    assert_eq!(
        std::fs::read_to_string(installed_path.join("scripts/run.sh")).unwrap(),
        "echo private-v2\n",
        "a failed private update must preserve the last usable version"
    );

    let repo_root = crate::repo_link::repo_root_of(&installed_path).unwrap();
    let repository_config = std::fs::read_to_string(repo_root.join(".git/config")).unwrap();
    let lockfile = std::fs::read_to_string(crate::lockfile::lockfile_path()).unwrap();
    let progress_debug = format!("{:?}", progress.0.lock().unwrap());
    assert!(!repository_config.contains(TOKEN));
    assert!(
        !std::fs::read_to_string(global_config)
            .unwrap()
            .contains(TOKEN)
    );
    assert!(!lockfile.contains(TOKEN));
    assert!(!progress_debug.contains(TOKEN));
    assert!(!format!("{session:?}").contains(TOKEN));
}

#[test]
fn local_divergence_blocks_update_before_any_file_changes() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();

    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();

    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let report = update_skills(&["demo".to_string()]);

    assert!(report.updated.is_empty());
    assert!(report.failed.is_empty());
    assert_eq!(report.blocked.len(), 1);
    assert_eq!(report.blocked[0].name, "demo");
    assert_eq!(report.blocked[0].suggested_local_name, "demo.local");
    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo local-change\n",
        "detecting divergence must not fetch/reset or rewrite the Skill"
    );
}

#[test]
fn skillstar_state_and_temporary_files_do_not_block_a_clean_update() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();

    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::create_dir_all(installed.join(".skillstar")).unwrap();
    std::fs::write(installed.join(".skillstar/update.json"), "transient").unwrap();
    std::fs::write(installed.join(".DS_Store"), "finder").unwrap();
    std::fs::write(installed.join("notes.md~"), "editor backup").unwrap();

    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let report = update_skills(&["demo".to_string()]);

    assert!(report.blocked.is_empty());
    assert!(report.failed.is_empty());
    assert_eq!(report.updated.len(), 1);
    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo remote-v2\n"
    );
}

#[test]
fn unversioned_content_hash_fails_closed_until_explicit_resolution() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();
    let snapshot = content::snapshot("demo").unwrap();
    let lock_path = lockfile::lockfile_path();
    let mut installed = lockfile::Lockfile::load(&lock_path).unwrap();
    installed.skills[0].content_hash = Some(snapshot.content_hash);
    installed.skills[0].content_hash_version = None;
    installed.save(&lock_path).unwrap();

    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let report = update_skills(&["demo".to_string()]);

    assert!(report.updated.is_empty());
    assert_eq!(report.blocked.len(), 1);
    assert_eq!(
        report.blocked[0].reason,
        LocalDivergenceReason::BaselineMissing
    );

    let resolved = resolve_skill_update("demo", LocalDivergenceResolution::Discard).unwrap();
    assert!(resolved.update.is_some());
    let migrated = lockfile::Lockfile::load(&lock_path).unwrap();
    assert_eq!(
        migrated.skills[0].content_hash_version,
        Some(content::SNAPSHOT_HASH_VERSION)
    );
    assert_eq!(
        migrated.skills[0].content_hash.as_deref(),
        Some(content::snapshot("demo").unwrap().content_hash.as_str())
    );
}

#[cfg(unix)]
#[test]
fn executable_bit_change_is_local_divergence() {
    use std::os::unix::fs::PermissionsExt as _;

    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();
    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();

    let script = skillstar_core::infra::paths::hub_skills_dir().join("demo/scripts/run.sh");
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    std::fs::set_permissions(&script, permissions).unwrap();

    let report = update_skills(&["demo".to_string()]);

    assert_eq!(report.blocked.len(), 1);
    assert_eq!(
        report.blocked[0].reason,
        LocalDivergenceReason::ContentChanged
    );
}

#[test]
fn preserving_divergence_copies_the_full_tree_then_updates_the_subscription() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();

    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::write(installed.join("SKILL.md"), "---\ndescription: local\n---\n").unwrap();
    std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();
    std::fs::create_dir_all(installed.join("assets")).unwrap();
    std::fs::write(installed.join("assets/prompt.txt"), "local asset\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("assets/prompt.txt", installed.join("prompt-link")).unwrap();

    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let result = resolve_skill_update(
        "demo",
        LocalDivergenceResolution::Preserve {
            local_name: "custom-demo".to_string(),
        },
    )
    .unwrap();

    assert_eq!(result.update.as_ref().unwrap().skill.name, "demo");
    assert_eq!(
        result.local_copy.as_ref().map(|skill| skill.name.as_str()),
        Some("custom-demo")
    );
    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo remote-v2\n"
    );
    assert!(
        !installed.join("assets/prompt.txt").exists(),
        "preserve-and-update must leave the subscribed Skill at the remote tree"
    );
    #[cfg(unix)]
    assert!(!installed.join("prompt-link").exists());

    let local = skillstar_core::infra::paths::hub_skills_dir().join("custom-demo");
    assert!(crate::local_skill::is_local_skill("custom-demo"));
    assert_eq!(
        std::fs::read_to_string(local.join("scripts/run.sh")).unwrap(),
        "echo local-change\n"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("assets/prompt.txt")).unwrap(),
        "local asset\n"
    );
    #[cfg(unix)]
    assert_eq!(
        std::fs::read_link(local.join("prompt-link")).unwrap(),
        Path::new("assets/prompt.txt")
    );
}

#[test]
fn discarding_divergence_updates_without_creating_a_local_copy() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();
    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::write(installed.join("scripts/run.sh"), "echo throw-away\n").unwrap();
    std::fs::write(installed.join("local-notes.txt"), "throw-away\n").unwrap();
    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    resolve_skill_update("demo", LocalDivergenceResolution::Discard).unwrap();

    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo remote-v2\n"
    );
    assert!(!installed.join("local-notes.txt").exists());
    assert!(!skillstar_core::infra::paths::local_skills_dir().exists());
}

#[test]
fn divergence_detection_does_not_materialize_an_empty_worktree() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();
    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::remove_file(installed.join("SKILL.md")).unwrap();
    std::fs::remove_dir_all(installed.join("scripts")).unwrap();

    let report = update_skills(&["demo".to_string()]);

    assert!(report.updated.is_empty());
    assert_eq!(
        report.blocked[0].reason,
        LocalDivergenceReason::SnapshotFailed
    );
    assert!(!installed.join("SKILL.md").exists());
    assert!(!installed.join("scripts").exists());
}

#[test]
fn missing_lockfile_fails_closed_until_the_user_explicitly_resolves_it() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();
    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::remove_file(lockfile::lockfile_path()).unwrap();
    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let report = update_skills(&["demo".to_string()]);

    assert!(report.updated.is_empty());
    assert_eq!(
        report.blocked[0].reason,
        LocalDivergenceReason::BaselineMissing
    );
    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo remote-v1\n"
    );

    let resolved = resolve_skill_update("demo", LocalDivergenceResolution::Discard).unwrap();
    assert!(resolved.update.is_some());
    assert!(resolved.remaining_blocked.is_empty());
    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo remote-v2\n"
    );
}

#[test]
fn missing_representative_lock_still_protects_locked_checkout_siblings() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    let repos = skillstar_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(&repos).unwrap();
    let cache = repos.join("missing-representative-lock");
    run_git(
        &repos,
        &[
            "clone",
            remote.path().to_str().unwrap(),
            cache.to_str().unwrap(),
        ],
    );
    let targets = [
        crate::repo_scanner::SkillInstallTarget {
            id: "alpha".to_string(),
            folder_path: "skills/alpha".to_string(),
        },
        crate::repo_scanner::SkillInstallTarget {
            id: "beta".to_string(),
            folder_path: "skills/beta".to_string(),
        },
    ];
    crate::repo_scanner::install_from_repo_at(
        &cache,
        &remote.path().to_string_lossy(),
        None,
        &targets,
    )
    .unwrap();

    let lock_path = lockfile::lockfile_path();
    let mut lockfile = lockfile::Lockfile::load(&lock_path).unwrap();
    lockfile.remove("alpha");
    lockfile.save(&lock_path).unwrap();
    let skills = skillstar_core::infra::paths::hub_skills_dir();
    std::fs::write(
        skills.join("beta/SKILL.md"),
        "---\nname: beta\ndescription: beta local\n---\n",
    )
    .unwrap();
    std::fs::write(
        remote.path().join("skills/alpha/SKILL.md"),
        "---\nname: alpha\ndescription: alpha v2\n---\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let report = update_skills(&["alpha".to_string()]);

    assert!(report.updated.is_empty());
    assert_eq!(
        report
            .blocked
            .iter()
            .map(|blocked| (blocked.name.clone(), blocked.reason))
            .collect::<Vec<_>>(),
        vec![
            ("alpha".to_string(), LocalDivergenceReason::BaselineMissing),
            ("beta".to_string(), LocalDivergenceReason::ContentChanged),
        ]
    );
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta local")
    );
    assert!(
        std::fs::read_to_string(skills.join("alpha/SKILL.md"))
            .unwrap()
            .contains("alpha v1")
    );
}

#[path = "tests/advanced.rs"]
mod advanced;

#[path = "tests/source_dropped.rs"]
mod source_dropped;
