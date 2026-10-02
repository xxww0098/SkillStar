use super::{install_skill_for_agent, install_skills_batch_in_session};
use crate::deployment::{self, batch_deploy_skills_to_agents};
use crate::git::transport::GitOperationSession;
use crate::lockfile;
use crate::pack_fixture::Sandbox;
use crate::projects::ProjectDeployMode;
use crate::repo_scanner;
use std::path::{Path, PathBuf};

fn run_git(repo: &Path, args: &[&str]) {
    let output = skillstar_core::infra::path_env::command_with_path("git")
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

fn pin_lf_repo(repo: &Path) {
    run_git(repo, &["config", "core.autocrlf", "false"]);
    run_git(repo, &["config", "core.eol", "lf"]);
    std::fs::write(repo.join(".gitattributes"), "* -text\n").unwrap();
}

fn init_pack(layout: &[(&str, &str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    run_git(dir.path(), &["init", "--initial-branch=main"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "SkillStar Tests"]);
    pin_lf_repo(dir.path());
    for (folder, name, marker) in layout {
        write_skill(&dir.path().join(folder), name, marker);
    }
    run_git(dir.path(), &["add", "."]);
    run_git(dir.path(), &["commit", "-m", "init"]);
    dir
}

fn lock_source_folder(name: &str) -> Option<String> {
    lock_entry(name).and_then(|entry| entry.source_folder)
}

fn lock_entry(name: &str) -> Option<lockfile::LockEntry> {
    lockfile::Lockfile::load(&lockfile::lockfile_path())
        .unwrap()
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
}

fn lock_git_ref(name: &str) -> Option<String> {
    lock_entry(name).and_then(|entry| entry.git_ref)
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

fn poison_cached_remotes() {
    let cache = skillstar_core::infra::paths::repos_cache_dir();
    let Ok(entries) = std::fs::read_dir(&cache) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.path().join(".git").exists() {
            continue;
        }
        let output = skillstar_core::infra::path_env::command_with_path("git")
            .current_dir(entry.path())
            .args([
                "remote",
                "set-url",
                "origin",
                "file:///nonexistent/skillstar-disconnected-remote",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "failed to poison origin: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn wipe_repo_cache() {
    let cache = skillstar_core::infra::paths::repos_cache_dir();
    if cache.exists() {
        std::fs::remove_dir_all(&cache).unwrap();
    }
}

fn assert_skill_folder(path: &Path) {
    assert!(
        path.join("SKILL.md").is_file(),
        "expected a skill folder with SKILL.md at {}",
        path.display()
    );
    assert!(
        !path.join(".cursor").exists()
            && !path.join(".dsh").exists()
            && !path.join("tests").exists(),
        "deployed the monorepo root instead of a skill folder: {}",
        path.display()
    );
}

/// Already-installed rust-skills: DeepSeek must retarget from the existing
/// clone (no git fetch/clone) and leave the Cursor link on `.cursor`.
#[test]
fn installed_rust_skills_deepseek_retargets_from_cache_without_clone() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[
        (".cursor/skills/rust", "rust", "cursor rust"),
        (".dsh/skills/rust", "rust", "dsh rust"),
    ]);
    let url = "https://github.com/acme/rust-skills.git";
    sandbox.map_github_url(url, remote.path());

    install_skill_for_agent(url.to_string(), Some("rust".into()), "cursor").expect("cursor hub");
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".cursor/skills/rust")
    );
    deploy("rust", "cursor");
    assert_eq!(payload_at(&home_skill(".cursor", "rust")), "cursor rust");

    sandbox.map_github_url(url, Path::new("/nonexistent/skillstar-disconnected-remote"));
    poison_cached_remotes();

    let retargeted = install_skill_for_agent(url.to_string(), Some("rust".into()), "deepseek")
        .expect("deepseek must use the existing clone, not fetch");
    assert_eq!(retargeted.name, "rust");
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".dsh/skills/rust")
    );
    deploy("rust", "deepseek");

    assert_eq!(payload_at(&home_skill(".dsh", "rust")), "dsh rust");
    assert_eq!(
        payload_at(&home_skill(".cursor", "rust")),
        "cursor rust",
        "already-linked cursor must stay on the cursor payload"
    );
}

/// impeccable has no `.dsh`. Clicking DeepSeek must still land a skill
/// folder in `~/.dsh/skills/impeccable`, never the monorepo and never error.
#[test]
fn installed_impeccable_deepseek_falls_back_to_a_skill_folder() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[(".cursor/skills/impeccable", "impeccable", "cursor copy")]);
    std::fs::write(
        remote.path().join("SKILL.md"),
        "---\nname: impeccable\ndescription: root shim\n---\n\n# shim\n",
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

    let url = "https://github.com/acme/impeccable.git";
    sandbox.map_github_url(url, remote.path());

    install_skill_for_agent(url.to_string(), Some("impeccable".into()), "cursor")
        .expect("cursor install");
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".cursor/skills/impeccable")
    );

    sandbox.map_github_url(url, Path::new("/nonexistent/skillstar-disconnected-remote"));
    poison_cached_remotes();

    let second = install_skill_for_agent(url.to_string(), Some("impeccable".into()), "deepseek")
        .expect("missing .dsh must fall back to a skill folder");
    assert_eq!(second.name, "impeccable");
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".cursor/skills/impeccable"),
        "fallback to the existing hub folder must not rewrite the lock"
    );

    deploy("impeccable", "deepseek");
    let dsh = home_skill(".dsh", "impeccable");
    assert!(
        dsh.exists(),
        "DeepSeek skill path must exist: {}",
        dsh.display()
    );
    assert_skill_folder(&dsh);
    assert_eq!(payload_at(&dsh), "cursor copy");
}

/// cua-driver-shaped repo: the Skill lives under a monorepo path like
/// `libs/cua-driver/rust/Skills/cua-driver` — outside every priority scan
/// directory, so the shallow rescan can never see it. A second harness click
/// must reuse the recorded payload already materialized in this checkout
/// instead of fetching the repository again.
#[test]
fn installed_monorepo_skill_reuses_cached_checkout_without_fetch() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[(
        "libs/cua-driver/rust/Skills/cua-driver",
        "cua-driver",
        "deep payload",
    )]);
    let url = "https://github.com/acme/cua.git";
    sandbox.map_github_url(url, remote.path());

    install_skill_for_agent(url.to_string(), Some("cua-driver".into()), "cursor")
        .expect("deep monorepo folder must install on first click");
    assert_eq!(
        lock_source_folder("cua-driver").as_deref(),
        Some("libs/cua-driver/rust/Skills/cua-driver")
    );

    sandbox.map_github_url(url, Path::new("/nonexistent/skillstar-disconnected-remote"));
    poison_cached_remotes();

    let second = install_skill_for_agent(url.to_string(), Some("cua-driver".into()), "deepseek")
        .expect("payload is already materialized; the shallow rescan must not force a fetch");
    assert_eq!(second.name, "cua-driver");
    assert_eq!(
        lock_source_folder("cua-driver").as_deref(),
        Some("libs/cua-driver/rust/Skills/cua-driver"),
        "no harness copy exists, so the lock must keep the recorded folder"
    );
    deploy("cua-driver", "deepseek");
    assert_eq!(
        payload_at(&home_skill(".dsh", "cua-driver")),
        "deep payload"
    );
}

/// Wiping the repo cache after a hub install must still fetch on the next
/// harness click. First-time install keeps the slow path.
#[test]
fn missing_git_cache_still_fetches_for_harness_install() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[
        (".cursor/skills/rust", "rust", "cursor rust"),
        (".dsh/skills/rust", "rust", "dsh rust"),
    ]);
    let url = "https://github.com/acme/rust-skills.git";
    sandbox.map_github_url(url, remote.path());

    install_skill_for_agent(url.to_string(), Some("rust".into()), "cursor").expect("cursor hub");
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".cursor/skills/rust")
    );
    wipe_repo_cache();
    assert!(
        repo_scanner_cache_empty(),
        "precondition: repo cache must be gone"
    );

    let fetched = install_skill_for_agent(url.to_string(), Some("rust".into()), "deepseek")
        .expect("missing cache must clone/fetch again");
    assert_eq!(fetched.name, "rust");
    assert_eq!(
        lock_source_folder("rust").as_deref(),
        Some(".dsh/skills/rust")
    );
    deploy("rust", "deepseek");
    assert_eq!(payload_at(&home_skill(".dsh", "rust")), "dsh rust");
}

fn repo_scanner_cache_empty() -> bool {
    let cache = skillstar_core::infra::paths::repos_cache_dir();
    !cache.exists()
        || std::fs::read_dir(&cache)
            .map(|entries| {
                entries
                    .flatten()
                    .all(|entry| !entry.path().join(".git").exists())
            })
            .unwrap_or(true)
}

fn delete_remote_branch(remote: &Path, branch: &str) {
    run_git(remote, &["checkout", "main"]);
    run_git(remote, &["branch", "-D", branch]);
}

fn public_session() -> GitOperationSession {
    GitOperationSession::public()
}

/// Lock pins a branch the remote has deleted; cache still has SKILL.md.
/// Fetch of that ref must not fail install/deploy, and the lock must retarget.
#[test]
fn deleted_lock_ref_does_not_block_install_and_retargets_to_default_branch() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[
        (".cursor/skills/rust", "rust", "cursor rust"),
        (".dsh/skills/rust", "rust", "dsh rust"),
    ]);
    run_git(remote.path(), &["checkout", "-b", "cursor/gone-branch"]);
    run_git(remote.path(), &["checkout", "main"]);

    let url = "https://github.com/acme/rust-skills.git";
    sandbox.map_github_url(url, remote.path());
    let pinned = format!("{url}#cursor/gone-branch");

    install_skill_for_agent(pinned.clone(), Some("rust".into()), "cursor")
        .expect("install from the live pin");
    assert_eq!(
        lock_git_ref("rust").as_deref(),
        Some("cursor/gone-branch"),
        "precondition: lock must record the pin"
    );
    deploy("rust", "cursor");

    delete_remote_branch(remote.path(), "cursor/gone-branch");

    let parsed = crate::source_resolver::Source::parse(&pinned).unwrap();
    repo_scanner::clone_or_fetch_repo_at_in_session(
        &parsed.repo_url,
        &parsed.short,
        parsed.git_ref.as_deref(),
        &public_session(),
    )
    .expect("missing remote ref must use the existing cache");

    assert_eq!(
        lock_git_ref("rust").as_deref(),
        Some("main"),
        "lock must retarget to the repo default branch"
    );
    assert_eq!(
        crate::update_checker::configured_git_ref(
            &repo_scanner::existing_hub_checkout(&parsed.repo_url, Some("rust")).unwrap()
        )
        .as_deref(),
        Some("main")
    );

    let installed = install_skill_for_agent(url.to_string(), Some("rust".into()), "deepseek")
        .expect("carousel deploy must succeed from cache after the gone-ref fetch");
    assert_eq!(installed.name, "rust");
    deploy("rust", "deepseek");
    assert_eq!(payload_at(&home_skill(".dsh", "rust")), "dsh rust");
    assert_eq!(
        lock_git_ref("rust").as_deref(),
        Some("main"),
        "harness retarget must not re-pin the deleted branch"
    );
}

/// Prefetch of skill A (deleted ref) must not fail an install of skill B.
#[test]
fn prefetch_miss_of_one_repo_does_not_fail_another_skill_install() {
    let sandbox = Sandbox::new();
    let rust_remote = init_pack(&[(".cursor/skills/rust", "rust", "cursor rust")]);
    run_git(
        rust_remote.path(),
        &["checkout", "-b", "cursor/gone-branch"],
    );
    run_git(rust_remote.path(), &["checkout", "main"]);

    let rust_url = "https://github.com/acme/rust-skills.git";
    sandbox.map_github_url(rust_url, rust_remote.path());
    install_skill_for_agent(
        format!("{rust_url}#cursor/gone-branch"),
        Some("rust".into()),
        "cursor",
    )
    .expect("pin rust");
    delete_remote_branch(rust_remote.path(), "cursor/gone-branch");

    let rust_hub = skillstar_core::infra::paths::hub_skills_dir().join("rust");
    let failed =
        crate::update_checker::prefetch_unique_repos_in_session(&[rust_hub], &public_session());
    let _ = failed;

    let banner_remote = init_pack(&[(
        ".claude/skills/banner-design",
        "banner-design",
        "claude copy",
    )]);
    let banner_url = "https://github.com/nextlevelbuilder/ui-ux-pro-max-skill.git";
    sandbox.map_github_url(banner_url, banner_remote.path());

    let installed = install_skill_for_agent(
        banner_url.to_string(),
        Some("banner-design".into()),
        "antigravity",
    )
    .expect("prefetch of rust must not fail banner-design install");
    assert_eq!(installed.name, "banner-design");
    deploy("banner-design", "antigravity");
    assert_eq!(
        payload_at(&home_skill(".gemini/antigravity", "banner-design")),
        "claude copy",
        "missing .agent must fall back to the claude harness copy"
    );
}

/// First-install chooser: one pipeline, harness folders are identity aliases.
#[test]
fn install_pipeline_table_chooses_harness_or_fallback_folder() {
    struct Case {
        name: &'static str,
        url: &'static str,
        layout: &'static [(&'static str, &'static str, &'static str)],
        root_shim: Option<(&'static str, &'static str)>,
        agent: &'static str,
        skill: &'static str,
        expect_folder: &'static str,
        expect_payload: &'static str,
    }
    let cases = [
        Case {
            name: "rust-skills --agent cursor hubs .cursor/skills/rust",
            url: "https://github.com/acme/rust-skills.git",
            layout: &[
                (".cursor/skills/rust", "rust", "cursor rust"),
                (".dsh/skills/rust", "rust", "dsh rust"),
            ],
            root_shim: Some(("rust", "shim")),
            agent: "cursor",
            skill: "rust",
            expect_folder: ".cursor/skills/rust",
            expect_payload: "cursor rust",
        },
        Case {
            name: "impeccable has no .dsh — DeepSeek still installs via fallback",
            url: "https://github.com/acme/impeccable.git",
            layout: &[(".cursor/skills/impeccable", "impeccable", "cursor copy")],
            root_shim: Some(("impeccable", "shim")),
            agent: "deepseek",
            skill: "impeccable",
            expect_folder: ".cursor/skills/impeccable",
            expect_payload: "cursor copy",
        },
        Case {
            name: "ui-ux-pro-max-skill only .claude — Antigravity still deploys that folder",
            url: "https://github.com/nextlevelbuilder/ui-ux-pro-max-skill.git",
            layout: &[(
                ".claude/skills/banner-design",
                "banner-design",
                "claude copy",
            )],
            root_shim: None,
            agent: "antigravity",
            skill: "banner-design",
            expect_folder: ".claude/skills/banner-design",
            expect_payload: "claude copy",
        },
    ];

    for case in cases {
        let sandbox = Sandbox::new();
        let remote = init_pack(case.layout);
        if let Some((name, marker)) = case.root_shim {
            std::fs::write(
                remote.path().join("SKILL.md"),
                format!("---\nname: {name}\ndescription: {marker}\n---\n\n# {marker}\n"),
            )
            .unwrap();
            run_git(remote.path(), &["add", "."]);
            run_git(remote.path(), &["commit", "-m", "shim"]);
        }
        sandbox.map_github_url(case.url, remote.path());
        let installed =
            install_skill_for_agent(case.url.to_string(), Some(case.skill.into()), case.agent)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert_eq!(installed.name, case.skill, "{}", case.name);
        assert_eq!(
            lock_source_folder(case.skill).as_deref(),
            Some(case.expect_folder),
            "{}",
            case.name
        );
        let hub = skillstar_core::infra::paths::hub_skills_dir().join(case.skill);
        assert_eq!(payload_at(&hub), case.expect_payload, "{}", case.name);
        assert!(
            !hub.join("tests").exists()
                && !hub.join(".cursor").exists()
                && !hub.join(".dsh").exists(),
            "{} must not hub the repo root",
            case.name
        );
    }
}

/// The impeccable shape without a harness request: the identity collapses to
/// the `.agents` representative, duplicate harness copies stay deferred in
/// the inventory sidecar, and heavy non-skill content never materializes in
/// the cache checkout. A later harness install materializes its copy on
/// demand.
#[test]
fn pack_collapses_duplicate_copies_and_heavy_content() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[
        (".agents/skills/impeccable", "impeccable", "agents copy"),
        (".claude/skills/impeccable", "impeccable", "agents copy"),
        (".cursor/skills/impeccable", "impeccable", "cursor copy"),
    ]);
    std::fs::create_dir_all(remote.path().join("crates/engine/src")).unwrap();
    std::fs::write(
        remote.path().join("crates/engine/src/lib.rs"),
        "pub fn heavy() {}",
    )
    .unwrap();
    std::fs::write(remote.path().join("package.json"), "{}").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "heavy"]);
    let url = "https://github.com/pbakaus/impeccable.git";
    sandbox.map_github_url(url, remote.path());

    let installed = install_skills_batch_in_session(
        url,
        &["impeccable".to_string()],
        None,
        &GitOperationSession::public(),
    )
    .expect("plain install");
    assert_eq!(installed.len(), 1);
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".agents/skills/impeccable")
    );
    let hub = skillstar_core::infra::paths::hub_skills_dir().join("impeccable");
    assert_eq!(payload_at(&hub), "agents copy");

    let repo_dir = skillstar_core::infra::paths::repos_cache_dir()
        .join(repo_scanner::cache_key_for("pbakaus/impeccable", None).unwrap());
    assert!(
        repo_dir
            .join(".agents/skills/impeccable/SKILL.md")
            .is_file(),
        "representative must be materialized"
    );
    assert!(
        !repo_dir.join(".cursor/skills/impeccable").exists(),
        "a same-name harness rewrite is the same identity: deferred"
    );
    assert!(
        !repo_dir.join(".claude/skills/impeccable").exists(),
        "byte-identical duplicate copy must stay deferred"
    );
    assert!(
        !repo_dir.join("crates").exists(),
        "heavy non-skill directories must never materialize (root files like \
         package.json are always present in cone-mode sparse checkouts)"
    );

    // The deferred identical copy comes back on demand for its harness.
    let retargeted = install_skills_batch_in_session(
        url,
        &["impeccable".to_string()],
        Some("claude"),
        &GitOperationSession::public(),
    )
    .expect("claude retarget");
    assert_eq!(retargeted.len(), 1);
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".claude/skills/impeccable")
    );
    assert_eq!(payload_at(&hub), "agents copy");
    assert!(
        repo_dir
            .join(".claude/skills/impeccable/SKILL.md")
            .is_file()
    );
}

#[derive(Default)]
struct RecordingSink(std::sync::Mutex<Vec<String>>);

impl crate::git::transport::GitProgressSink for RecordingSink {
    fn emit(&self, progress: crate::git::transport::GitOperationProgress) {
        if let Some(stage) = progress.stage {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(format!("{stage:?}"));
        }
    }
}

/// A fresh install walks the pipeline stages in order on its session's
/// progress channel, so the UI can replace the bare spinner with a label.
#[test]
fn fresh_install_emits_stage_progress_in_order() {
    let sandbox = Sandbox::new();
    let remote = init_pack(&[("skills/alpha", "alpha", "alpha payload")]);
    let url = "https://github.com/acme/plain-skills.git";
    sandbox.map_github_url(url, remote.path());

    let sink = std::sync::Arc::new(RecordingSink::default());
    let session = GitOperationSession::new(
        "stage-sequence-test",
        crate::git::transport::GitAuthMaterial::missing(),
        sink.clone(),
    );
    let installed = install_skills_batch_in_session(url, &["alpha".to_string()], None, &session)
        .expect("install");
    assert_eq!(installed.len(), 1);

    let stages = sink.0.lock().unwrap().clone();
    assert_eq!(
        stages,
        vec![
            "Resolving".to_string(),
            "Fetching".to_string(),
            "Discovering".to_string(),
            "Materializing".to_string(),
        ],
        "fresh install must emit the four in-crate stages in order"
    );
}

const IMPECCABLE_URL: &str = "https://github.com/pbakaus/impeccable.git";

fn impeccable_cache() -> PathBuf {
    skillstar_core::infra::paths::repos_cache_dir()
        .join(repo_scanner::cache_key_for("pbakaus/impeccable", None).unwrap())
}

/// Published `impeccable` copies currently checked out in the cache.
fn impeccable_copies_on_disk() -> Vec<String> {
    let repo_dir = impeccable_cache();
    let mut on_disk: Vec<String> = crate::pack_fixture::published_copies()
        .into_iter()
        .filter(|dir| repo_dir.join(dir).join("SKILL.md").is_file())
        .collect();
    on_disk.sort();
    on_disk
}

fn install_impeccable(agent: Option<&str>) {
    install_skills_batch_in_session(
        IMPECCABLE_URL,
        &["impeccable".to_string()],
        agent,
        &GitOperationSession::public(),
    )
    .unwrap_or_else(|error| panic!("install for {agent:?}: {error}"));
}

/// A harness click surfaces exactly the copy the shared table picks for it,
/// next to the representative — never every same-name copy.
#[test]
fn harness_install_materializes_only_the_chosen_copy() {
    let fixture = crate::pack_fixture::impeccable_like();
    for (agent, chosen) in [
        ("cursor", ".cursor/skills/impeccable"),
        ("deepseek", ".dsh/skills/impeccable"),
        ("gemini-cli", ".gemini/skills/impeccable"),
        ("antigravity", ".agent/skills/impeccable"),
        ("codex", ".agents/skills/impeccable"),
        // The pack's `.windsurf` folder is a different Skill (the decoy).
        ("windsurf", ".agents/skills/impeccable"),
    ] {
        let sandbox = Sandbox::new();
        sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());
        install_impeccable(Some(agent));

        assert_eq!(
            lock_source_folder("impeccable").as_deref(),
            Some(chosen),
            "{agent}"
        );
        let mut expected = vec![".agents/skills/impeccable".to_string(), chosen.to_string()];
        expected.sort();
        expected.dedup();
        assert_eq!(impeccable_copies_on_disk(), expected, "{agent}");
        assert!(!impeccable_cache().join("tests").exists(), "{agent}");
    }
}

#[test]
fn name_request_without_harness_materializes_nothing_new() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());
    install_impeccable(None);
    install_impeccable(None);
    assert_eq!(
        impeccable_copies_on_disk(),
        vec![".agents/skills/impeccable".to_string()]
    );
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".agents/skills/impeccable")
    );
}

/// A fresh cache for an installed Skill: a click for a harness the pack does
/// not ship falls back to the installed `source_folder`, which must be
/// materialized even though the new inventory deferred it.
#[test]
fn harness_fallback_materializes_the_installed_source_folder() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());
    install_impeccable(Some("deepseek"));
    std::fs::remove_dir_all(impeccable_cache()).unwrap();

    install_impeccable(Some("windsurf"));
    assert_eq!(
        lock_source_folder("impeccable").as_deref(),
        Some(".dsh/skills/impeccable")
    );
    assert!(
        impeccable_cache()
            .join(".dsh/skills/impeccable/SKILL.md")
            .is_file()
    );
    assert!(
        !impeccable_cache()
            .join(".cursor/skills/impeccable")
            .exists()
    );
}

#[cfg(test)]
#[path = "skill_install_pin_tests.rs"]
mod pin_tests;
