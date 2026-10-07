use super::*;
use crate::git::transport::{GitAuthMaterial, NoopGitProgressSink};
use crate::skill_lock::SourceType;
use crate::update_api::{FastPathFailureKind, parse_tree_response};
use std::sync::Mutex;

const REPO_URL: &str = "https://github.com/acme/skills.git";

/// Recorded REST bodies keyed by the rev a request names.
#[derive(Default)]
struct RecordedApi {
    trees: HashMap<String, String>,
    commits: HashMap<String, String>,
    rate_limited_until: Option<u64>,
    calls: Mutex<Vec<String>>,
}

impl RecordedApi {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn failure(&self, rev: &str, kind: FastPathFailureKind) -> FastPathFailure {
        FastPathFailure {
            owner: "acme".into(),
            repo: "skills".into(),
            git_ref: rev.into(),
            kind,
        }
    }
}

impl TreeApi for RecordedApi {
    fn tree(
        &self,
        _owner: &str,
        _repo: &str,
        rev: &str,
    ) -> impl Future<Output = Result<ApiRemoteTree, FastPathFailure>> + Send {
        self.calls.lock().unwrap().push(format!("tree:{rev}"));
        let result = match (self.rate_limited_until, self.trees.get(rev)) {
            (Some(reset_unix), _) => {
                Err(self.failure(rev, FastPathFailureKind::RateLimited { reset_unix }))
            }
            (None, Some(body)) => Ok(parse_tree_response(body).unwrap()),
            (None, None) => Err(self.failure(rev, FastPathFailureKind::Http { status: 404 })),
        };
        async move { result }
    }

    fn commit_tree(
        &self,
        _owner: &str,
        _repo: &str,
        commit: &str,
    ) -> impl Future<Output = Result<String, FastPathFailure>> + Send {
        self.calls.lock().unwrap().push(format!("commit:{commit}"));
        let result = match self.commits.get(commit) {
            Some(body) => Ok(crate::update_api::parse_commit_tree_response(body).unwrap()),
            None => Err(self.failure(commit, FastPathFailureKind::Http { status: 404 })),
        };
        async move { result }
    }
}

/// `GET git/trees/{rev}` as GitHub returns it (non-recursive). At a branch
/// or `HEAD` the top-level `sha` is the commit; at a tree SHA it is the tree.
fn tree_body(sha: &str, dirs: &[(&str, &str)]) -> String {
    let entries = dirs
        .iter()
        .map(|(path, sha)| {
            format!(
                r#"{{"path":"{path}","mode":"040000","type":"tree","sha":"{sha}","url":"https://api.github.com/repos/acme/skills/git/trees/{sha}"}}"#
            )
        })
        .chain(std::iter::once(
            r#"{"path":"README.md","mode":"100644","type":"blob","sha":"b0","size":12,"url":"https://api.github.com/repos/acme/skills/git/blobs/b0"}"#
                .to_string(),
        ))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"sha":"{sha}","url":"https://api.github.com/repos/acme/skills/git/trees/{sha}","tree":[{entries}],"truncated":false}}"#
    )
}

fn commit_body(commit: &str, tree: &str) -> String {
    format!(
        r#"{{"sha":"{commit}","message":"m","tree":{{"sha":"{tree}","url":"https://api.github.com/repos/acme/skills/git/trees/{tree}"}},"parents":[]}}"#
    )
}

fn entry(path: &str, git_ref: Option<&str>) -> SkillLockEntry {
    SkillLockEntry {
        source: "acme/skills".into(),
        source_type: SourceType::Github,
        source_url: REPO_URL.into(),
        git_ref: git_ref.map(str::to_string),
        skill_path: (!path.is_empty()).then(|| path.to_string()),
        skill_folder_hash: Some("old".into()),
        installed_at: String::new(),
        updated_at: String::new(),
        extra: Default::default(),
    }
}

fn session() -> GitOperationSession {
    GitOperationSession::new(
        "update-check-test",
        GitAuthMaterial::missing(),
        Arc::new(NoopGitProgressSink),
    )
}

fn run<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

/// A nested Skill used to come back `None` from the single-level lookup and
/// was then reported as removed upstream. It must walk the subtrees.
#[test]
fn nested_paths_walk_subtrees_and_only_missing_folders_are_removed() {
    let _sandbox = crate::pack_fixture::Sandbox::new();
    let mut api = RecordedApi::default();
    api.trees.insert(
        "HEAD".into(),
        tree_body("c0ffee", &[("skills", "t-skills")]),
    );
    api.trees.insert(
        "t-skills".into(),
        tree_body("t-skills", &[("demo", "t-demo"), ("other", "t-other")]),
    );
    let api = Arc::new(api);
    let entries = vec![
        ("demo".to_string(), entry("skills/demo", None)),
        ("other".to_string(), entry("skills/other", None)),
        ("gone".to_string(), entry("skills/gone", None)),
    ];

    let verdicts = run(check_upstream_with(&entries, api.clone(), &session()));

    assert_eq!(verdicts["demo"], Upstream::Hash("t-demo".into()));
    assert_eq!(verdicts["other"], Upstream::Hash("t-other".into()));
    assert_eq!(verdicts["gone"], Upstream::Removed);
    // No ref in the lock → the default branch (`HEAD`), and the shared
    // `skills` subtree is fetched once.
    assert_eq!(api.calls(), ["tree:HEAD", "tree:t-skills"]);
}

/// The lock stores `HEAD^{tree}` for a root Skill while the Trees API reports
/// the commit SHA at a ref; the commit must be resolved to its tree.
#[test]
fn root_skill_compares_the_commit_tree_not_the_commit() {
    let _sandbox = crate::pack_fixture::Sandbox::new();
    let mut api = RecordedApi::default();
    api.trees
        .insert("v2".into(), tree_body("commit-sha", &[("docs", "t-docs")]));
    api.commits
        .insert("commit-sha".into(), commit_body("commit-sha", "root-tree"));
    let entries = vec![("root".to_string(), entry("", Some("v2")))];

    let verdicts = run(check_upstream_with(&entries, Arc::new(api), &session()));

    assert_eq!(verdicts["root"], Upstream::Hash("root-tree".into()));
}

fn fixture_repo(root: &std::path::Path) -> String {
    let skill = root.join("skills/demo");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: d\n---\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        let mut full = vec![
            "-c",
            "user.name=SkillStar Tests",
            "-c",
            "user.email=tests@example.com",
            "-c",
            "commit.gpgsign=false",
        ];
        full.extend_from_slice(args);
        crate::pack_fixture::git(root, &full)
    };
    git(&["init", "--quiet"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "init"]);
    git(&["rev-parse", "HEAD:skills/demo"])
}

/// A rate limit is persisted with its reset time: the group falls back to a
/// clone in the caller's session, and later checks skip the API entirely.
#[test]
fn rate_limit_persists_a_cooldown_and_falls_back_to_clone() {
    let sandbox = crate::pack_fixture::Sandbox::new();
    let upstream = tempfile::tempdir().unwrap();
    let demo_tree = fixture_repo(upstream.path());
    sandbox.map_github_url(REPO_URL, upstream.path());
    let reset = now_unix() + 3600;
    let entries = vec![("demo".to_string(), entry("skills/demo", None))];

    let limited = Arc::new(RecordedApi {
        rate_limited_until: Some(reset),
        ..Default::default()
    });
    let verdicts = run(check_upstream_with(&entries, limited.clone(), &session()));
    assert_eq!(verdicts["demo"], Upstream::Hash(demo_tree.clone()));
    assert_eq!(limited.calls(), ["tree:HEAD"]);
    assert!(cooldown_active(now_unix()));
    assert!(!cooldown_active(reset));

    let healthy = Arc::new(RecordedApi::default());
    let verdicts = run(check_upstream_with(&entries, healthy.clone(), &session()));
    assert_eq!(verdicts["demo"], Upstream::Hash(demo_tree));
    assert!(healthy.calls().is_empty(), "{:?}", healthy.calls());
}

/// A blank lock ref is the remote default branch. `main` and `master` are
/// decoys: asking for either of them must not decide the verdict.
#[test]
fn blank_ref_asks_head_and_ignores_main_and_master() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let mut api = RecordedApi::default();
    api.trees.insert(
        "HEAD".into(),
        tree_body("commit-head", &[("skills", "t-skills")]),
    );
    api.trees.insert(
        "t-skills".into(),
        tree_body("t-skills", &[("demo", "t-demo")]),
    );
    for decoy in ["", "main", "master"] {
        api.trees.insert(
            decoy.into(),
            tree_body("commit-decoy", &[("skills", "t-decoy")]),
        );
    }
    api.trees.insert(
        "t-decoy".into(),
        tree_body("t-decoy", &[("demo", "t-wrong")]),
    );
    let api = Arc::new(api);
    let mut blank = entry("skills/demo", Some(""));
    blank.git_ref = Some("   ".into());

    let verdicts = run(check_upstream_with(
        &[("demo".to_string(), blank)],
        api.clone(),
        &session(),
    ));

    assert_eq!(verdicts["demo"], Upstream::Hash("t-demo".into()));
    assert_eq!(api.calls(), ["tree:HEAD", "tree:t-skills"]);
}

/// The clone fallback must follow the remote HEAD, not a `main` branch that
/// happens to exist beside a different default branch.
#[test]
fn blank_ref_clone_follows_the_default_branch_not_main() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let root = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let mut full = vec![
            "-c",
            "user.name=SkillStar Tests",
            "-c",
            "user.email=tests@example.com",
            "-c",
            "commit.gpgsign=false",
        ];
        full.extend_from_slice(args);
        crate::pack_fixture::git(root.path(), &full)
    };
    git(&["init", "--quiet", "-b", "trunk"]);
    let skill = root.path().join("skills/demo");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: trunk\n---\n",
    )
    .unwrap();
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "trunk"]);
    let trunk = git(&["rev-parse", "HEAD:skills/demo"]);
    git(&["branch", "main"]);
    git(&["checkout", "--quiet", "main"]);
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: main\n---\n",
    )
    .unwrap();
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "main"]);
    let main = git(&["rev-parse", "HEAD:skills/demo"]);
    git(&["checkout", "--quiet", "trunk"]);
    assert_ne!(trunk, main);

    let url = crate::git::ops::local_file_url(root.path());
    let mut omitted = entry("skills/demo", None);
    omitted.source_type = SourceType::Git;
    omitted.source_url = url.clone();
    let mut blank = omitted.clone();
    blank.git_ref = Some(String::new());

    let api = Arc::new(RecordedApi::default());
    let verdicts = run(check_upstream_with(
        &[
            ("omitted".to_string(), omitted),
            ("blank".to_string(), blank),
        ],
        api.clone(),
        &session(),
    ));

    assert_eq!(verdicts["omitted"], Upstream::Hash(trunk.clone()));
    assert_eq!(verdicts["blank"], Upstream::Hash(trunk));
    assert!(api.calls().is_empty(), "{:?}", api.calls());
}

#[test]
fn local_and_bundle_entries_are_never_checked() {
    let _sandbox = crate::pack_fixture::Sandbox::new();
    let api = Arc::new(RecordedApi::default());
    let mut local = entry("", None);
    local.source_type = SourceType::Local;
    let entries = vec![("mine".to_string(), local)];

    let verdicts = run(check_upstream_with(&entries, api.clone(), &session()));

    assert_eq!(verdicts["mine"], Upstream::Unknown);
    assert!(api.calls().is_empty());
}
