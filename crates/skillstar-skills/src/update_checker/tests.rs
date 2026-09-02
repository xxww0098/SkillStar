use super::*;
use anyhow::anyhow;
use std::fs;

fn run_git(repo: &Path, args: &[&str]) {
    let output = skillstar_core::infra::path_env::command_with_path("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    run_git(dir.path(), &["init", "--initial-branch=main"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "SkillStar Tests"]);
    dir
}

// Unix-only: fixtures use std::os::unix::fs::symlink.
#[cfg(unix)]
#[test]
fn subtree_hash_and_local_update_detection_work() {
    let remote = init_repo();
    fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
    fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "initial"]);

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_path = clone_parent.path().join("clone");
    run_git(
        clone_parent.path(),
        &[
            "clone",
            remote.path().to_str().unwrap(),
            clone_path.to_str().unwrap(),
        ],
    );

    let initial_hash = git_ops::compute_subtree_hash(&clone_path, "skills/demo").unwrap();
    assert!(!initial_hash.is_empty());

    fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "update"]);

    run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

    let skill_link_parent = tempfile::tempdir().unwrap();
    let skill_link = skill_link_parent.path().join("demo");
    std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();

    let result = check_update_local_with(
        &skill_link,
        &HashSet::new(),
        |path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        },
        None,
    );
    assert_eq!(result, Some(true));
}

// Unix-only: fixtures use std::os::unix::fs::symlink.
#[cfg(unix)]
#[test]
fn api_remote_hashes_drive_update_detection_without_fetching() {
    let remote = init_repo();
    fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
    fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
    fs::write(remote.path().join("README.md"), "readme v1").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "initial"]);

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_path = clone_parent.path().join("clone");
    run_git(
        clone_parent.path(),
        &[
            "clone",
            remote.path().to_str().unwrap(),
            clone_path.to_str().unwrap(),
        ],
    );

    let skill_link_parent = tempfile::tempdir().unwrap();
    let skill_link = skill_link_parent.path().join("demo");
    std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();
    let repo_root_of = |path: &Path| {
        let real = std::fs::read_link(path).ok()?;
        Some(real.parent()?.parent()?.to_path_buf())
    };

    let entry = LockEntry {
        name: "demo".into(),
        git_url: "https://github.com/example/demo".into(),
        git_ref: None,
        tree_hash: "tree".into(),
        content_hash: None,
        content_hash_version: None,
        installed_at: String::new(),
        source_folder: Some("skills/demo".into()),
    };

    // API says "same as local HEAD" → no update, and no fetch happened.
    let v1_hash = git_ops::compute_subtree_hash(&clone_path, "skills/demo").unwrap();
    let mut folders = std::collections::HashMap::new();
    folders.insert("skills/demo".to_string(), v1_hash.clone());
    let api = crate::update_api::ApiRemoteTree { folders };
    assert_eq!(
        check_update_local_with_api(
            &skill_link,
            &HashSet::new(),
            Some(&api),
            repo_root_of,
            Some(&entry)
        ),
        Some(false)
    );

    // Remote advances without the local clone fetching anything.
    fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
    fs::create_dir_all(remote.path().join("skills/other")).unwrap();
    fs::write(remote.path().join("skills/other/SKILL.md"), "other").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "update"]);

    let v2_hash = git_ops::compute_subtree_hash(remote.path(), "skills/demo").unwrap();
    assert_ne!(v1_hash, v2_hash);
    let mut folders = std::collections::HashMap::new();
    folders.insert("skills/demo".to_string(), v2_hash.clone());
    let api = crate::update_api::ApiRemoteTree { folders };
    assert_eq!(
        check_update_local_with_api(
            &skill_link,
            &HashSet::new(),
            Some(&api),
            repo_root_of,
            Some(&entry)
        ),
        Some(true)
    );

    // An unrelated repo change must NOT light the badge: same folder hash.
    let mut folders = std::collections::HashMap::new();
    folders.insert("skills/demo".to_string(), v1_hash.clone());
    let api = crate::update_api::ApiRemoteTree { folders };
    assert_eq!(
        check_update_local_with_api(
            &skill_link,
            &HashSet::new(),
            Some(&api),
            repo_root_of,
            Some(&entry)
        ),
        Some(false)
    );

    // The API listing is shallow (non-recursive): a folder missing from it
    // resolves from the local tracked ref instead — the tip gate guarantees
    // both hold identical hashes, and here the un-fetched tracked ref still
    // matches HEAD.
    let api = crate::update_api::ApiRemoteTree::default();
    assert_eq!(
        check_update_local_with_api(
            &skill_link,
            &HashSet::new(),
            Some(&api),
            repo_root_of,
            Some(&entry)
        ),
        Some(false)
    );

    // A folder that resolves nowhere — absent from the API listing AND from
    // the local tracked ref — is unknown, not "no update" (the previous
    // badge must survive).
    let ghost_entry = LockEntry {
        source_folder: Some("skills/ghost".into()),
        ..entry.clone()
    };
    assert_eq!(
        check_update_local_with_api(
            &skill_link,
            &HashSet::new(),
            Some(&api),
            repo_root_of,
            Some(&ghost_entry)
        ),
        None
    );

    // A failed fetch for this repo overrides the API answer.
    let failed: HashSet<PathBuf> = [clone_path.clone()].into_iter().collect();
    let mut folders = std::collections::HashMap::new();
    folders.insert("skills/demo".to_string(), v2_hash);
    let api = crate::update_api::ApiRemoteTree { folders };
    assert_eq!(
        check_update_local_with_api(&skill_link, &failed, Some(&api), repo_root_of, Some(&entry)),
        None
    );
}

#[cfg(unix)]
#[test]
fn subtree_comparison_ignores_unrelated_repo_changes() {
    let remote = init_repo();
    fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
    fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
    fs::write(remote.path().join("README.md"), "readme v1").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "initial"]);

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_path = clone_parent.path().join("clone");
    run_git(
        clone_parent.path(),
        &[
            "clone",
            remote.path().to_str().unwrap(),
            clone_path.to_str().unwrap(),
        ],
    );

    // The remote moves forward, but only outside the skill's folder.
    fs::write(remote.path().join("README.md"), "readme v2").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "docs only"]);

    run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

    let skill_link_parent = tempfile::tempdir().unwrap();
    let skill_link = skill_link_parent.path().join("demo");
    std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();

    let entry = crate::lockfile::LockEntry {
        name: "demo".to_string(),
        git_url: String::new(),
        git_ref: None,
        tree_hash: String::new(),
        content_hash: None,
        content_hash_version: None,
        installed_at: String::new(),
        source_folder: Some("skills/demo".to_string()),
    };
    let result = check_update_local_with(
        &skill_link,
        &HashSet::new(),
        |path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        },
        Some(&entry),
    );
    assert_eq!(
        result,
        Some(false),
        "a repo-wide HEAD change must not badge a Skill whose folder did not move"
    );
}

#[cfg(unix)]
#[test]
fn subtree_comparison_badges_skills_whose_folder_moved() {
    let remote = init_repo();
    fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
    fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "initial"]);

    let clone_parent = tempfile::tempdir().unwrap();
    let clone_path = clone_parent.path().join("clone");
    run_git(
        clone_parent.path(),
        &[
            "clone",
            remote.path().to_str().unwrap(),
            clone_path.to_str().unwrap(),
        ],
    );

    fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "skill changed"]);

    run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

    let skill_link_parent = tempfile::tempdir().unwrap();
    let skill_link = skill_link_parent.path().join("demo");
    std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();

    let entry = crate::lockfile::LockEntry {
        name: "demo".to_string(),
        git_url: String::new(),
        git_ref: None,
        tree_hash: String::new(),
        content_hash: None,
        content_hash_version: None,
        installed_at: String::new(),
        source_folder: Some("skills/demo".to_string()),
    };
    let result = check_update_local_with(
        &skill_link,
        &HashSet::new(),
        |path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        },
        Some(&entry),
    );
    assert_eq!(result, Some(true));
}

#[test]
fn github_trees_api_commit_ish_sha_does_not_badge_an_up_to_date_root_skill() {
    // GitHub `GET /git/trees/{branch}` puts the *commit* SHA in `sha`,
    // not `commit.tree.sha`. Root-skill detection used to compare that
    // value to `HEAD^{tree}`, so a skill already on the remote tip
    // kept showing "update available" after every successful pull.
    let repo = init_repo();
    fs::write(repo.path().join("SKILL.md"), "root skill").unwrap();
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "initial"]);

    let commit = git_ops::rev_parse(repo.path(), "HEAD").unwrap();
    let tree = git_ops::rev_parse(repo.path(), "HEAD^{tree}").unwrap();
    assert_ne!(
        commit, tree,
        "precondition: a commit SHA is not its tree SHA"
    );

    let entry = LockEntry {
        name: "root-skill".into(),
        git_url: "https://github.com/example/root-skill".into(),
        git_ref: None,
        tree_hash: tree.clone(),
        content_hash: None,
        content_hash_version: None,
        installed_at: String::new(),
        source_folder: None,
    };
    let repo_root = repo.path().to_path_buf();
    let skill_path = repo_root.join("root-skill");

    let mut folders = std::collections::HashMap::new();
    folders.insert(String::new(), commit.clone());
    let api = crate::update_api::ApiRemoteTree { folders };
    assert_eq!(
        check_update_local_with_api(
            &skill_path,
            &HashSet::new(),
            Some(&api),
            |_| Some(repo_root.clone()),
            Some(&entry),
        ),
        Some(false),
        "GitHub's commit-ish Trees `sha` matching HEAD must not badge a root skill"
    );

    let mut folders = std::collections::HashMap::new();
    folders.insert(String::new(), tree.clone());
    let api = crate::update_api::ApiRemoteTree { folders };
    assert_eq!(
        check_update_local_with_api(
            &skill_path,
            &HashSet::new(),
            Some(&api),
            |_| Some(repo_root.clone()),
            Some(&entry),
        ),
        Some(false),
        "a peeled root tree SHA matching HEAD^{{tree}} still means no update"
    );

    let mut folders = std::collections::HashMap::new();
    folders.insert(
        String::new(),
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".into(),
    );
    let api = crate::update_api::ApiRemoteTree { folders };
    assert_eq!(
        check_update_local_with_api(
            &skill_path,
            &HashSet::new(),
            Some(&api),
            |_| Some(repo_root.clone()),
            Some(&entry),
        ),
        Some(true),
        "a different remote commit still badges the root skill"
    );
}

#[test]
fn unresolvable_repo_root_reports_no_update() {
    let dir = tempfile::tempdir().unwrap();
    let result = check_update_local_with(&dir.path().join("nope"), &HashSet::new(), |_| None, None);
    assert_eq!(
        result,
        Some(false),
        "None is reserved for failed fetches, not for missing repo roots"
    );
}

#[test]
fn failed_prefetch_root_preserves_previous_state() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let failed = HashSet::from([repo.clone()]);
    let result = check_update_local_with(
        &dir.path().join("skill"),
        &failed,
        |_| Some(repo.clone()),
        None,
    );
    assert_eq!(result, None, "failed fetch must not clear the badge");
}

#[test]
fn prefetch_unique_repos_deduplicates_and_tracks_failures() {
    let dir = tempfile::tempdir().unwrap();
    let repo_a = dir.path().join("repo_a");
    let repo_b = dir.path().join("repo_b");
    let skill_a1 = dir.path().join("skill_a1");
    let skill_a2 = dir.path().join("skill_a2");
    let skill_b = dir.path().join("skill_b");

    let repo_a_for_lookup = repo_a.clone();
    let repo_b_for_lookup = repo_b.clone();
    let skill_b_for_lookup = skill_b.clone();
    let repo_root_of = move |path: &Path| -> Option<PathBuf> {
        if path == skill_b_for_lookup {
            Some(repo_b_for_lookup.clone())
        } else {
            Some(repo_a_for_lookup.clone())
        }
    };

    let fetch_calls = std::sync::Mutex::new(Vec::new());
    let fetch_repo = |root: &Path| -> Result<()> {
        fetch_calls
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push(root.to_path_buf());
        if root == repo_b {
            Err(anyhow!("fetch failed"))
        } else {
            Ok(())
        }
    };

    let failed =
        prefetch_unique_repos_with(&[skill_a1, skill_a2, skill_b], repo_root_of, fetch_repo);

    let calls = fetch_calls.lock().unwrap_or_else(|err| err.into_inner());
    assert_eq!(calls.len(), 2, "should fetch only unique repos");
    assert!(calls.contains(&repo_a));
    assert!(calls.contains(&repo_b));
    assert!(failed.contains(&repo_b));
    assert!(!failed.contains(&repo_a));
}

#[test]
fn prefetch_unique_repos_fetches_distinct_roots_concurrently() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let roots: Vec<PathBuf> = (0..4)
        .map(|index| dir.path().join(format!("repo_{index}")))
        .collect();
    let skills: Vec<PathBuf> = roots.iter().map(|root| root.join("skill")).collect();
    let inflight = AtomicUsize::new(0);
    let max_inflight = AtomicUsize::new(0);
    let repo_root_of = {
        let roots = roots.clone();
        let skills = skills.clone();
        move |path: &Path| {
            skills
                .iter()
                .position(|skill| skill == path)
                .map(|index| roots[index].clone())
        }
    };
    let fetch_repo = |_root: &Path| -> Result<()> {
        let n = inflight.fetch_add(1, Ordering::SeqCst) + 1;
        max_inflight.fetch_max(n, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(30));
        inflight.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    };

    let failed = prefetch_unique_repos_with(&skills, repo_root_of, fetch_repo);
    assert!(failed.is_empty());
    assert!(
        max_inflight.load(Ordering::SeqCst) >= 2,
        "distinct repos must be fetched by overlapping git-worker threads"
    );
}
