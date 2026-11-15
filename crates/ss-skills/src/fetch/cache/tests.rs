use super::*;
use crate::git_skill::GitSkillFacade;
use crate::pack_fixture::{Sandbox, git, impeccable_like};
use crate::repo_scanner::SkillInstallTarget;

#[test]
fn scan_install_refresh_and_offline_retry_share_one_object_store() {
    let sandbox = Sandbox::new();
    let fixture = impeccable_like();
    let remote = "https://github.com/test/cached.git";
    sandbox.map_github_url(remote, fixture.dir.path());
    let input = "https://github.com/test/cached/tree/main/.claude/skills/impeccable";
    let facade = || GitSkillFacade::new(GitOperationSession::public());
    let preview = facade().scan_repo(input, false).unwrap();
    assert!(!preview.cache_hit);
    let repeated = facade().scan_repo(input, true).unwrap();
    assert!(repeated.cache_hit);
    assert_eq!(repeated.revision, preview.revision);
    assert_eq!(repeated.cached_at, preview.cached_at);

    let folder = &preview.skills[0].folder_path;
    let payload = format!("{folder}/scripts/impeccable");
    let before = fs::read(fixture.dir.path().join(&payload)).unwrap();
    fs::write(fixture.dir.path().join(&payload), b"new upstream payload\n").unwrap();
    git(fixture.dir.path(), &["add", "."]);
    git(
        fixture.dir.path(),
        &["commit", "-m", "advance upstream", "--no-verify"],
    );
    let new_blob = git(
        fixture.dir.path(),
        &["rev-parse", &format!("HEAD:{payload}")],
    );
    let target = SkillInstallTarget {
        id: preview.skills[0].id.clone(),
        folder_path: folder.clone(),
        pinned: false,
    };
    let targets = std::slice::from_ref(&target);
    facade().install_from_scan(&preview, targets).unwrap();
    let canonical = paths::agents_skill_dir(&target.id);
    assert_eq!(
        fs::read(canonical.join("scripts/impeccable")).unwrap(),
        before
    );
    assert_eq!(
        crate::skill_lock::load().skills[&target.id]
            .git_ref
            .as_deref(),
        Some("main")
    );
    assert_eq!(
        facade().scan_repo(input, false).unwrap().revision,
        preview.revision
    );

    let refreshed = facade().scan_repo_with_refresh(input, false, true).unwrap();
    assert!(!refreshed.cache_hit);
    assert_ne!(refreshed.revision, preview.revision);
    let cache_dir = paths::skill_import_cache_dir().join(key(&preview.spec));
    assert!(
        !ss_core::infra::path_env::command_with_path("git")
            .current_dir(&cache_dir)
            .env("GIT_NO_LAZY_FETCH", "1")
            .args(["cat-file", "-e", &new_blob])
            .output()
            .unwrap()
            .status
            .success(),
        "refreshing discovery must not fetch the previous installation's new payload"
    );
    facade().install_from_scan(&refreshed, targets).unwrap();
    assert_eq!(
        fs::read(canonical.join("scripts/impeccable")).unwrap(),
        b"new upstream payload\n"
    );

    // Removing the serving repository makes every clone/fetch fail. A new facade
    // still scans and reinstalls both previews, using only the persisted objects.
    fs::rename(
        fixture.dir.path().join(".git"),
        fixture.dir.path().join("offline-git"),
    )
    .unwrap();
    facade().install_from_scan(&preview, targets).unwrap();
    assert_eq!(
        fs::read(canonical.join("scripts/impeccable")).unwrap(),
        before
    );
    let offline = facade().scan_repo(input, true).unwrap();
    assert_eq!(offline.revision, refreshed.revision);
    assert!(offline.cache_hit);
    facade().install_from_scan(&refreshed, targets).unwrap();
    assert!(facade().scan_repo_with_refresh(input, false, true).is_err());
    assert_eq!(
        facade().scan_repo(input, false).unwrap().revision,
        refreshed.revision
    );
    assert_eq!(
        fs::read_dir(paths::skill_import_cache_dir())
            .unwrap()
            .count(),
        1
    );

    assert_eq!(clear_import_cache().unwrap(), 1);
    assert!(
        canonical.join("SKILL.md").is_file(),
        "cache cleanup must not remove installed content"
    );
    let error = facade().install_from_scan(&preview, targets).unwrap_err();
    assert!(
        error.to_string().contains("scan the repository again"),
        "{error:#}"
    );
}

#[test]
fn concurrent_scans_deduplicate_and_locked_cache_can_be_cancelled_or_cleared() {
    let sandbox = Sandbox::new();
    let fixture = impeccable_like();
    let remote = "https://github.com/test/concurrent.git";
    sandbox.map_github_url(remote, fixture.dir.path());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                GitSkillFacade::new(GitOperationSession::public())
                    .scan_repo(remote, false)
                    .unwrap()
                    .cache_hit
            })
        })
        .collect::<Vec<_>>();
    let hits = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(|hit| *hit)
        .count();
    assert_eq!(hits, 1, "one clone, one cache hit");

    let spec = Source::parse(remote).unwrap();
    let checkout = crate::fetch::fetch_for_scan(&spec, &GitOperationSession::public()).unwrap();
    assert_eq!(
        clear_import_cache().unwrap(),
        0,
        "busy checkouts are never deleted"
    );
    let cancel = GitOperationSession::public();
    let waiting = cancel.clone();
    let worker = std::thread::spawn(move || {
        crate::fetch::fetch_for_scan(&spec, &waiting)
            .err()
            .unwrap()
            .to_string()
    });
    std::thread::sleep(Duration::from_millis(50));
    cancel.cancel();
    assert!(worker.join().unwrap().contains("cancelled"));
    drop(checkout);
    assert_eq!(clear_import_cache().unwrap(), 1);
}
