use super::{install_skill_for_agent, install_skills_batch_in_session};
use crate::deployment::{self, batch_deploy_skills_to_agents};
use crate::git::transport::GitOperationSession;
use crate::lockfile;
use crate::projects::ProjectDeployMode;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Sandbox {
    previous: Vec<(&'static str, Option<OsString>)>,
    _temp: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    fn new() -> Self {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let overrides = [
            ("SKILLSTAR_HUB_DIR", Some(temp.path().join("hub"))),
            ("SKILLSTAR_DATA_DIR", Some(temp.path().join("data"))),
            (
                "SKILLSTAR_TOOL_SYNC_HOME",
                Some(temp.path().join("tool-home")),
            ),
            ("HOME", Some(temp.path().join("home"))),
            ("USERPROFILE", Some(temp.path().join("home"))),
            ("GIT_CONFIG_GLOBAL", Some(temp.path().join("gitconfig"))),
            ("GIT_CONFIG_NOSYSTEM", Some(PathBuf::from("1"))),
            ("DSH_HOME", None),
            ("CODEX_HOME", None),
        ];
        let previous = overrides
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        unsafe {
            for (key, value) in &overrides {
                match value {
                    Some(path) => std::env::set_var(key, path),
                    None => std::env::remove_var(key),
                }
            }
        }
        deployment::invalidate_profile_cache();
        Self {
            previous,
            _temp: temp,
            _guard,
        }
    }

    fn map_github_url(&self, github_url: &str, local_repo: &Path) {
        let config = std::env::var_os("GIT_CONFIG_GLOBAL").expect("GIT_CONFIG_GLOBAL");
        std::fs::write(
            config,
            format!(
                "[url \"file://{}\"]\n\tinsteadOf = {github_url}\n",
                local_repo.display()
            ),
        )
        .unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        deployment::invalidate_profile_cache();
        unsafe {
            for (key, previous) in self.previous.drain(..).rev() {
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

fn write_skill(dir: &Path, name: &str, marker: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {marker}\n---\n\n# {marker}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("payload.txt"), marker).unwrap();
}

fn init_pack(layout: &[(&str, &str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    run_git(dir.path(), &["init", "--initial-branch=main"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "SkillStar Tests"]);
    for (folder, name, marker) in layout {
        write_skill(&dir.path().join(folder), name, marker);
    }
    run_git(dir.path(), &["add", "."]);
    run_git(dir.path(), &["commit", "-m", "init"]);
    dir
}

fn lock_source_folder(name: &str) -> Option<String> {
    lockfile::Lockfile::load(&lockfile::lockfile_path())
        .unwrap()
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
        .and_then(|entry| entry.source_folder)
}

fn payload_at(path: &Path) -> String {
    std::fs::read_to_string(path.join("payload.txt")).unwrap_or_default()
}

fn home_skill(agent_dir: &str, name: &str) -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME");
    PathBuf::from(home)
        .join(agent_dir)
        .join("skills")
        .join(name)
}

fn deploy(name: &str, agent_id: &str) {
    deployment::invalidate_profile_cache();
    let deployed = batch_deploy_skills_to_agents(
        &[name.to_string()],
        &[agent_id.to_string()],
        ProjectDeployMode::Symlink,
    )
    .unwrap();
    assert_eq!(deployed, 1, "expected a new deploy for {agent_id}");
}

/// rust-skills-style pack: cursor then deepseek must deploy the dsh folder,
/// not reuse the cursor hub payload.
#[test]
fn second_harness_install_deploys_that_harness_folder() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[
        (".cursor/skills/rust", "rust", "cursor rust"),
        (".dsh/skills/rust", "rust", "dsh rust"),
    ]);
    std::fs::write(
        remote.path().join("SKILL.md"),
        "---\nname: rust\ndescription: root shim\n---\n\n# shim\n",
    )
    .unwrap();
    std::fs::create_dir_all(remote.path().join("tests")).unwrap();
    std::fs::write(
        remote.path().join("tests/should_not_install.rs"),
        "fn x() {}\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "shim"]);

    let url = "https://github.com/acme/rust-skills.git";
    sandbox.map_github_url(url, remote.path());

    let first = install_skills_batch_in_session(
        url,
        &["rust".to_string()],
        Some("cursor"),
        &GitOperationSession::public(),
    )
    .expect("cursor harness install");
    assert_eq!(first.len(), 1);
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".cursor/skills/rust")
    );
    let hub = skillstar_core::infra::paths::hub_skills_dir().join("rust");
    assert_eq!(payload_at(&hub), "cursor rust");
    assert!(!hub.join("tests").exists());
    deploy("rust", "cursor");
    assert_eq!(payload_at(&home_skill(".cursor", "rust")), "cursor rust");

    let same = install_skills_batch_in_session(
        url,
        &["rust".to_string()],
        Some("cursor"),
        &GitOperationSession::public(),
    )
    .expect("same harness reuses");
    assert!(
        same.is_empty(),
        "same source_folder must reuse, not retarget: {same:?}"
    );

    let second = install_skills_batch_in_session(
        url,
        &["rust".to_string()],
        Some("deepseek"),
        &GitOperationSession::public(),
    )
    .expect("deepseek harness retarget");
    assert_eq!(
        second.len(),
        1,
        "different harness must install/retarget, not reuse"
    );
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".dsh/skills/rust")
    );
    assert_eq!(payload_at(&hub), "dsh rust");
    deploy("rust", "deepseek");

    assert_eq!(
        payload_at(&home_skill(".dsh", "rust")),
        "dsh rust",
        "deepseek deploy must be the dsh folder, not the cursor copy"
    );
    assert_eq!(
        payload_at(&home_skill(".cursor", "rust")),
        "cursor rust",
        "already-linked cursor must stay on the cursor payload"
    );
}

/// Live leftover from the first harness bug: dsh is already linked to the
/// cursor folder. Retarget + deploy must rewrite that stale link, not skip it.
#[test]
fn stale_dsh_link_is_rewritten_to_requested_harness() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[
        (".cursor/skills/rust", "rust", "cursor rust"),
        (".dsh/skills/rust", "rust", "dsh rust"),
    ]);
    let url = "https://github.com/acme/rust-skills.git";
    sandbox.map_github_url(url, remote.path());

    install_skills_batch_in_session(
        url,
        &["rust".to_string()],
        Some("cursor"),
        &GitOperationSession::public(),
    )
    .expect("cursor hub");
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".cursor/skills/rust")
    );
    deploy("rust", "cursor");
    assert_eq!(payload_at(&home_skill(".cursor", "rust")), "cursor rust");

    let cursor_payload = std::fs::canonicalize(home_skill(".cursor", "rust")).unwrap();
    let dsh = home_skill(".dsh", "rust");
    std::fs::create_dir_all(dsh.parent().unwrap()).unwrap();
    skillstar_core::infra::fs_ops::create_symlink(&cursor_payload, &dsh).unwrap();
    assert_eq!(
        payload_at(&dsh),
        "cursor rust",
        "precondition: stale dsh link points at the cursor folder"
    );

    let retargeted = install_skills_batch_in_session(
        url,
        &["rust".to_string()],
        Some("deepseek"),
        &GitOperationSession::public(),
    )
    .expect("deepseek retarget");
    assert_eq!(retargeted.len(), 1);
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".dsh/skills/rust")
    );

    deploy("rust", "deepseek");
    assert_eq!(
        payload_at(&home_skill(".dsh", "rust")),
        "dsh rust",
        "stale dsh symlink must be rewritten to the dsh harness folder"
    );
    assert_eq!(
        payload_at(&home_skill(".cursor", "rust")),
        "cursor rust",
        "pinned cursor deploy must stay on the cursor payload"
    );
}

/// In-place hub at `.agents/skills/impeccable` plus `--agent cursor` must
/// retarget to `.cursor/skills/impeccable`, not silently reuse.
#[test]
fn agents_hub_retargets_to_cursor_harness_folder() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[
        (".agents/skills/impeccable", "impeccable", "agents copy"),
        (".cursor/skills/impeccable", "impeccable", "cursor copy"),
    ]);
    let url = "https://github.com/acme/impeccable.git";
    sandbox.map_github_url(url, remote.path());

    let first = install_skill_for_agent(url.to_string(), Some("impeccable".into()), "codex")
        .expect("codex / .agents install");
    assert_eq!(first.name, "impeccable");
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".agents/skills/impeccable")
    );
    let hub = skillstar_core::infra::paths::hub_skills_dir().join("impeccable");
    assert_eq!(payload_at(&hub), "agents copy");

    let second = install_skill_for_agent(url.to_string(), Some("impeccable".into()), "cursor")
        .expect("cursor harness must retarget");
    assert_eq!(second.name, "impeccable");
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".cursor/skills/impeccable")
    );
    assert_eq!(payload_at(&hub), "cursor copy");
}

#[test]
fn missing_harness_folder_does_not_reuse_another_copy() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[(".cursor/skills/impeccable", "impeccable", "cursor copy")]);
    let url = "https://github.com/acme/impeccable.git";
    sandbox.map_github_url(url, remote.path());

    install_skill_for_agent(url.to_string(), Some("impeccable".into()), "cursor")
        .expect("cursor install");
    let error = install_skill_for_agent(url.to_string(), Some("impeccable".into()), "deepseek")
        .expect_err("missing .dsh must fail");
    assert!(
        error.contains(".dsh") || error.contains("harness"),
        "{error}"
    );
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".cursor/skills/impeccable")
    );
    assert!(!home_skill(".dsh", "impeccable").exists());
}
