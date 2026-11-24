use super::*;
use crate::facades::TEST_ENV_LOCK;

/// Point `SKILLSTAR_DATA_DIR` at a temp dir for the test's lifetime so installs
/// never touch the real `~/.skillstar`.
struct IsolatedDataDir {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
    root: std::path::PathBuf,
}

impl IsolatedDataDir {
    fn new() -> Self {
        let lock = TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!(
            "nsh-import-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("sandbox dir");
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        // SAFETY: every suite that flips SKILLSTAR_DATA_DIR serializes on TEST_ENV_LOCK.
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
        Self {
            _lock: lock,
            previous,
            root,
        }
    }
}

impl Drop for IsolatedDataDir {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write_skill(repo: &std::path::Path, folder: &str, name: &str, description: &str) {
    let dir = repo.join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
    )
    .unwrap();
}

#[test]
fn sniffs_share_codes_the_way_the_banner_needs() {
    let code = format!("agd-{}", "A".repeat(64));
    assert!(MySkillsImport::looks_like_share_code(code.clone()));
    assert!(MySkillsImport::looks_like_share_code(format!(
        "Deck: pack\nsee {code} thanks"
    )));
    assert!(!MySkillsImport::looks_like_share_code("ags-short".into()));
    assert!(!MySkillsImport::looks_like_share_code(
        "https://github.com/o/r".into()
    ));
    assert!(!MySkillsImport::looks_like_share_code(String::new()));
}

#[test]
fn parse_round_trips_a_fresh_code() {
    let code = ss_skills::share_install::encode_share_code(
        ShareCodeKind::Deck,
        &ss_skills::share_install::ShareCodePayload {
            n: "Pack".into(),
            d: "Tools".into(),
            i: "📦".into(),
            s: vec![ss_skills::share_install::ShareCodeSkill {
                n: "demo".into(),
                u: "https://github.com/o/demo".into(),
                c: None,
                p: None,
            }],
        },
    );
    let preview = MySkillsImport::parse_share_code(code).expect("fresh code parses");
    assert_eq!(preview.kind, "deck");
    assert_eq!(preview.name, "Pack");
    assert_eq!(preview.icon, "📦");
    assert_eq!(preview.skills.len(), 1);
    assert_eq!(preview.skills[0].n, "demo");
}

#[test]
fn parse_rejects_garbage_with_the_domain_reason() {
    let err = MySkillsImport::parse_share_code("not a code".into())
        .expect_err("plain text is not a share code");
    assert!(err.contains("prefix"));
}

#[test]
fn progress_events_map_to_the_raw_phase_stage_shape() {
    let preparing = GitOperationProgress {
        session_id: "s".into(),
        phase: GitOperationPhase::Preparing,
        repository: "o/r".into(),
        stage: None,
        skill: None,
    };
    assert_eq!(progress_json(&preparing)["phase"], "preparing");
    let discovering = GitOperationProgress {
        session_id: "s".into(),
        phase: GitOperationPhase::Running,
        repository: "o/r".into(),
        stage: Some(InstallStage::Discovering),
        skill: Some("demo".into()),
    };
    let json = progress_json(&discovering);
    assert_eq!(json["phase"], "running");
    assert_eq!(json["stage"], "discovering");
    assert_eq!(json["skill"], "demo");
    let cancelled = GitOperationProgress {
        session_id: "s".into(),
        phase: GitOperationPhase::Cancelled,
        repository: "o/r".into(),
        stage: None,
        skill: None,
    };
    assert_eq!(progress_json(&cancelled)["phase"], "cancelled");
}

#[test]
fn progress_channel_closes_after_worker_finishes() {
    use ss_skills::git::transport::{GitAuthMaterial, GitOperationSession};

    let (tx, rx) = mpsc::channel();
    let tx = Arc::new(tx);
    let sink = Arc::new(DialogProgressSink {
        tx: Arc::downgrade(&tx),
    });
    let facade = GitSkillFacade::new(GitOperationSession::new(
        "test",
        GitAuthMaterial::missing(),
        sink,
    ));
    let worker = facade.clone();
    worker
        .session()
        .emit_stage(InstallStage::Discovering, "test/repo", None);
    let drained = drain_latest(&rx);
    assert!(drained.latest.is_some());
    assert!(!drained.disconnected);

    facade.cancel();
    assert!(worker.session().is_cancelled());
    drop(worker);
    drop(tx);
    assert!(drain_latest(&rx).disconnected);
}

#[test]
fn local_scan_to_install_round_trip() {
    let _sandbox = IsolatedDataDir::new();
    let parent = std::env::temp_dir().join(format!(
        "nsh-import-repo-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let repo = parent.join("demo-repo");
    write_skill(&repo, "demo-skill", "demo-skill", "A demo skill for the import chain");
    let facade = GitSkillFacade::from_file_store();

    let (scan, preselected) = scan_with_command(&facade, repo.to_str().unwrap(), false, false)
        .expect("local folder scans");
    assert_eq!(scan.spec.short, "local/demo-repo");
    assert_eq!(scan.skills.len(), 1);
    assert!(scan.skills[0].installable);
    assert!(preselected.is_empty());

    cache_scan(&scan);
    let info = scan_info(&scan, preselected);
    assert_eq!(info.skills[0].id, "demo-skill");
    assert!(!info.plugin);

    let cached = take_cached_scan(&info.short).expect("scan cached by short id");
    let targets = vec![SkillInstallTarget {
        id: "demo-skill".into(),
        folder_path: "demo-skill".into(),
        pinned: false,
    }];
    let installed = install_body(&facade, &cached, &targets).expect("installs from scan");
    assert_eq!(installed, vec!["demo-skill".to_owned()]);
    assert!(take_cached_scan(&info.short).is_none());
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn pasted_cli_command_scans_and_computes_preselected_skills() {
    let _sandbox = IsolatedDataDir::new();
    let parent = std::env::temp_dir().join(format!(
        "nsh-import-cmd-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let repo = parent.join("multi-repo");
    write_skill(&repo, "skills/alpha", "alpha", "Alpha skill");
    write_skill(&repo, "skills/beta", "beta", "Beta skill");

    let facade = GitSkillFacade::from_file_store();
    let pasted = format!("npx skills add \"{}\" --skill BETA", repo.display());
    let (scan, preselected) =
        scan_with_command(&facade, &pasted, false, false).expect("pasted cli scans");
    assert_eq!(preselected, vec!["beta".to_string()]);
    let info = scan_info(&scan, preselected);
    assert_eq!(info.preselected_skills, vec!["beta".to_string()]);

    let pasted_all = format!("skillstar install \"{}\" --all", repo.display());
    let (_scan2, all_pre) =
        scan_with_command(&facade, &pasted_all, false, false).expect("pasted --all scans");
    assert_eq!(all_pre, vec!["alpha".to_string(), "beta".to_string()]);
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn adopt_and_share_bodies_shape_their_outcomes() {
    let _sandbox = IsolatedDataDir::new();
    let repo = std::env::temp_dir().join(format!(
        "nsh-import-adopt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_skill(&repo, "adopt-me", "adopt-me", "Adopted by the folder path");
    let names = adopt_folder_body(repo.to_str().unwrap()).expect("folder adopts");
    assert_eq!(names, vec!["adopt-me".to_owned()]);

    let (summary, _deck) = install_share_body(
        false,
        "n/a",
        String::new(),
        String::new(),
        vec![ss_skills::share_install::ShareCodeSkill {
            n: "adopt-me".into(),
            u: String::new(),
            c: None,
            p: None,
        }],
    )
    .expect("share body runs");
    assert_eq!(summary.requested_count, 1);
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn install_audits_gate_non_github_sources_without_a_request() {
    let _sandbox = IsolatedDataDir::new();
    let mut future = std::pin::pin!(MySkillsImport::install_audits(
        "https://gitlab.com/owner/repo".to_owned(),
        vec!["demo".to_owned()],
    ));
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    let rows = match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(rows) => rows,
        std::task::Poll::Pending => unreachable!("the gated path finishes on first poll"),
    };
    assert!(rows.is_empty(), "absence is not a pass — the well hides");
}
