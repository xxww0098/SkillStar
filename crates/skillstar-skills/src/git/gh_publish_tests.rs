//! Publish-path tests: the three-state readiness contract the UI branches on,
//! the REST calls that replaced the `gh` CLI, and the secret-visibility rules
//! both sides have to keep.

use std::collections::VecDeque;
use std::sync::Mutex;

use super::gh_manager::{GhStatus, PublishIdentity, map_publish_status, publish_copies_content};
use super::gh_rest::{
    GhRestClient, GhRestError, GhRestErrorCode, GhRestResponse, GhRestTransport, REPO_AFFILIATION,
    block_on_blocking_context,
};

const TOKEN: &str = "ghu_publish_rest_canary_token";

// ── Test double ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedCall {
    method: &'static str,
    url: String,
    token: String,
    body: Option<serde_json::Value>,
}

#[derive(Default)]
struct FakeState {
    responses: Mutex<VecDeque<GhRestResponse>>,
    calls: Mutex<Vec<RecordedCall>>,
}

/// Cloneable so a test keeps a handle on what the client sent after handing
/// the transport to the client.
#[derive(Clone, Default)]
struct FakeTransport(std::sync::Arc<FakeState>);

impl FakeTransport {
    fn with(responses: Vec<(u16, serde_json::Value)>) -> Self {
        Self(std::sync::Arc::new(FakeState {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(status, body)| GhRestResponse {
                        status,
                        body: body.to_string(),
                    })
                    .collect(),
            ),
            calls: Mutex::new(Vec::new()),
        }))
    }

    fn raw(status: u16, body: &str) -> Self {
        Self::with(vec![(
            status,
            serde_json::from_str(body).expect("test fixture is valid JSON"),
        )])
    }

    fn next(&self, call: RecordedCall) -> Result<GhRestResponse, GhRestError> {
        self.0.calls.lock().unwrap().push(call);
        self.0
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| GhRestError::new(GhRestErrorCode::Protocol, "no scripted response"))
    }

    fn calls(&self) -> Vec<RecordedCall> {
        self.0.calls.lock().unwrap().clone()
    }
}

impl GhRestTransport for FakeTransport {
    fn get(&self, url: &str, token: &str) -> Result<GhRestResponse, GhRestError> {
        self.next(RecordedCall {
            method: "GET",
            url: url.to_string(),
            token: token.to_string(),
            body: None,
        })
    }

    fn post_json(
        &self,
        url: &str,
        token: &str,
        body: &serde_json::Value,
    ) -> Result<GhRestResponse, GhRestError> {
        self.next(RecordedCall {
            method: "POST",
            url: url.to_string(),
            token: token.to_string(),
            body: Some(body.clone()),
        })
    }
}

fn repo_page(count: usize, prefix: &str) -> serde_json::Value {
    serde_json::Value::Array(
        (0..count)
            .map(|index| {
                serde_json::json!({
                    "full_name": format!("{prefix}/repo-{index}"),
                    "html_url": format!("https://github.com/{prefix}/repo-{index}"),
                    "clone_url": format!("https://github.com/{prefix}/repo-{index}.git"),
                    "description": serde_json::Value::Null,
                    "private": false,
                })
            })
            .collect(),
    )
}

// ── Readiness contract ──────────────────────────────────────────────

/// The three states survived the move off `gh`, with new meanings: the binary
/// publishing still needs is `git`, and the identity is SkillStar's GitHub App
/// user. All three frontend branches must stay reachable.
#[test]
fn publish_readiness_maps_git_and_app_identity_onto_the_three_frontend_states() {
    let missing_git = map_publish_status(
        false,
        PublishIdentity::SignedIn {
            login: "octocat".into(),
        },
    );
    assert!(matches!(missing_git, GhStatus::NotInstalled));

    let signed_out = map_publish_status(true, PublishIdentity::SignedOut);
    assert!(matches!(signed_out, GhStatus::NotAuthenticated));

    let ready = map_publish_status(
        true,
        PublishIdentity::SignedIn {
            login: "octocat".into(),
        },
    );
    match ready {
        GhStatus::Ready { username } => assert_eq!(username, "octocat"),
        other => panic!("expected Ready, got {other:?}"),
    }

    // The serde tag is the discriminant the UI switches on.
    let encoded = serde_json::to_value(map_publish_status(true, PublishIdentity::SignedOut))
        .expect("status serializes");
    assert_eq!(encoded["status"], "NotAuthenticated");
}

// ── Repository listing ──────────────────────────────────────────────

/// The unlock: `gh repo list <login>` could only ever return personal repos,
/// so an organization repository was unreachable as a publish target.
#[test]
fn repository_listing_requests_organization_affiliation_and_pages_up_to_the_limit() {
    let transport = FakeTransport::with(vec![
        (200, repo_page(100, "octocat")),
        (200, repo_page(100, "acme-org")),
    ]);
    let client = GhRestClient::with_transport(TOKEN, transport);

    let repos = client.list_repositories(150).expect("listing succeeds");

    assert_eq!(repos.len(), 150, "limit truncates the last page");
    assert!(
        repos.iter().any(|repo| repo.full_name.starts_with("acme-org/")),
        "organization repositories must reach the publish picker"
    );
}

#[test]
fn repository_listing_sends_affiliation_and_paging_query_parameters() {
    let transport = FakeTransport::with(vec![
        (200, repo_page(100, "octocat")),
        (200, repo_page(100, "acme-org")),
    ]);
    let client = GhRestClient::with_transport(TOKEN, transport.clone());
    client.list_repositories(150).expect("listing succeeds");

    let calls = transport.calls();
    assert_eq!(calls.len(), 2, "a full page must be followed by the next");
    assert_eq!(REPO_AFFILIATION, "owner,collaborator,organization_member");
    for (index, call) in calls.iter().enumerate() {
        assert_eq!(call.method, "GET");
        assert!(
            call.url
                .contains("affiliation=owner,collaborator,organization_member"),
            "every page must keep the affiliation filter: {}",
            call.url
        );
        assert!(call.url.contains("per_page=100"), "url: {}", call.url);
        assert!(
            call.url.contains(&format!("&page={}&", index + 1)),
            "pages must advance: {}",
            call.url
        );
    }
}

/// A short page means GitHub has nothing more; asking for another one would
/// burn a request (and rate-limit budget) on every publish dialog.
#[test]
fn repository_listing_stops_at_a_short_page_instead_of_paging_to_the_limit() {
    let transport = FakeTransport::with(vec![(200, repo_page(3, "octocat"))]);
    let client = GhRestClient::with_transport(TOKEN, transport.clone());

    let repos = client.list_repositories(200).expect("listing succeeds");

    assert_eq!(repos.len(), 3);
    assert_eq!(transport.calls().len(), 1);
}

// ── Repository contents ─────────────────────────────────────────────

#[test]
fn skill_folder_inspection_keeps_only_sorted_visible_directories() {
    let transport = FakeTransport::with(vec![(
        200,
        serde_json::json!([
            {"name": "zeta", "type": "dir"},
            {"name": "README.md", "type": "file"},
            {"name": ".hidden", "type": "dir"},
            {"name": "alpha", "type": "dir"},
        ]),
    )]);
    let client = GhRestClient::with_transport(TOKEN, transport.clone());

    let folders = client
        .list_skill_folders("acme-org/skills")
        .expect("inspection succeeds");

    assert_eq!(folders, vec!["alpha".to_string(), "zeta".to_string()]);
    assert_eq!(
        transport.calls()[0].url,
        "https://api.github.com/repos/acme-org/skills/contents/skills"
    );
}

/// A repository with no `skills/` directory — including a brand new empty one —
/// is a normal publish target, not a failure.
#[test]
fn skill_folder_inspection_treats_missing_and_empty_repositories_as_no_folders() {
    for status in [404, 409] {
        let transport = FakeTransport::with(vec![(
            status,
            serde_json::json!({"message": "This repository is empty."}),
        )]);
        let client = GhRestClient::with_transport(TOKEN, transport);
        assert_eq!(
            client.list_skill_folders("octocat/fresh").unwrap(),
            Vec::<String>::new()
        );
    }
}

#[test]
fn repository_names_that_could_escape_the_api_path_are_rejected() {
    let client = GhRestClient::with_transport(TOKEN, FakeTransport::default());
    for candidate in ["octocat", "octocat/../../secrets", "octocat/repo?x=1"] {
        let error = client
            .list_skill_folders(candidate)
            .expect_err("malformed repository names must not reach a URL");
        assert_eq!(error.code, GhRestErrorCode::Protocol);
    }
}

// ── Repository creation ─────────────────────────────────────────────

#[test]
fn repository_creation_posts_a_non_initialized_repository_and_returns_its_clone_url() {
    let transport = FakeTransport::with(vec![(
        201,
        serde_json::json!({
            "full_name": "octocat/my-skills",
            "html_url": "https://github.com/octocat/my-skills",
            "clone_url": "https://github.com/octocat/my-skills.git",
            "private": true,
        }),
    )]);
    let client = GhRestClient::with_transport(TOKEN, transport.clone());

    let created = client
        .create_repository("my-skills", "A collection", true)
        .expect("creation succeeds");

    assert_eq!(
        created.clone_url,
        "https://github.com/octocat/my-skills.git"
    );
    let calls = transport.calls();
    let call = &calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "https://api.github.com/user/repos");
    let body = call.body.as_ref().expect("create sends a body");
    assert_eq!(body["name"], "my-skills");
    assert_eq!(body["private"], true);
    // GitHub seeding a README would create a history the local push cannot
    // fast-forward.
    assert_eq!(body["auto_init"], false);
}

#[test]
fn github_rejections_and_throttling_stay_distinguishable_and_actionable() {
    let cases: Vec<(u16, &str, GhRestErrorCode)> = vec![
        (401, r#"{"message":"Bad credentials"}"#, GhRestErrorCode::NotAuthenticated),
        (403, r#"{"message":"Resource not accessible"}"#, GhRestErrorCode::Unauthorized),
        (403, r#"{"message":"API rate limit exceeded"}"#, GhRestErrorCode::RateLimited),
        (429, r#"{"message":"Too many requests"}"#, GhRestErrorCode::RateLimited),
        (404, r#"{"message":"Not Found"}"#, GhRestErrorCode::Unauthorized),
        (500, r#"{"message":"boom"}"#, GhRestErrorCode::Protocol),
    ];

    for (status, body, expected) in cases {
        let client = GhRestClient::with_transport(TOKEN, FakeTransport::raw(status, body));
        let error = client
            .create_repository("my-skills", "", false)
            .expect_err("non-201 must fail");
        assert_eq!(error.code, expected, "status {status} misclassified");
        assert!(!error.message.is_empty());
    }

    let client = GhRestClient::with_transport(
        TOKEN,
        FakeTransport::raw(
            422,
            r#"{"message":"Repository creation failed.","errors":[{"message":"name already exists on this account"}]}"#,
        ),
    );
    let error = client
        .create_repository("my-skills", "", false)
        .expect_err("422 must fail");
    assert_eq!(error.code, GhRestErrorCode::Rejected);
    assert!(
        error.message.contains("name already exists on this account"),
        "the actionable detail must survive: {}",
        error.message
    );
}

// ── Secret visibility ───────────────────────────────────────────────

/// The credential belongs in the `Authorization` header and nowhere else — not
/// in a URL (which lands in logs and rate-limit reports) and not in a debug
/// rendering of the client.
#[test]
fn the_rest_credential_never_leaves_the_authorization_header() {
    let transport = FakeTransport::with(vec![
        (200, serde_json::json!({"login": "octocat"})),
        (200, repo_page(2, "acme-org")),
        (200, serde_json::json!([])),
    ]);
    let client = GhRestClient::with_transport(TOKEN, transport.clone());

    client.current_login().unwrap();
    client.list_repositories(10).unwrap();
    client.list_skill_folders("acme-org/skills").unwrap();

    for call in transport.calls() {
        assert!(
            !call.url.contains(TOKEN),
            "token must not enter a request URL: {}",
            call.url
        );
        assert_eq!(call.token, TOKEN, "the token belongs in the auth header");
        if let Some(body) = call.body {
            assert!(!body.to_string().contains(TOKEN));
        }
    }
    assert!(!format!("{client:?}").contains(TOKEN));
}

/// Publishing is called from `spawn_blocking` (Tauri) and from the CLI's main
/// thread. Starting a runtime from inside an async task panics, so this pins
/// the context the REST bridge is actually used in.
#[test]
fn rest_requests_run_from_a_blocking_task_inside_a_tokio_runtime() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let answer = runtime.block_on(async {
        tokio::task::spawn_blocking(|| block_on_blocking_context(async { 7u8 }))
            .await
            .unwrap()
    });

    assert_eq!(answer.expect("runtime bridge works on a blocking thread"), 7);
}

/// The Git half of publishing: clone/pull/push now run through the operation
/// session, so the token may only reach the askpass child environment, and any
/// output that echoes it must come back redacted.
#[cfg(unix)]
#[test]
fn publish_git_commands_keep_the_token_out_of_argv_and_output() {
    use super::gh_manager::run_remote_git_command;
    use crate::git::transport::{GitAuthMaterial, GitOperationSession, NoopGitProgressSink};
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::sync::Arc;

    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("fake-git");
    let argv_log = temp.path().join("argv.txt");
    std::fs::write(
        &script,
        r#"#!/bin/sh
printf '%s' "$*" > "$SKILLSTAR_FAKE_GIT_ARGV"
printf 'remote: echoed %s\n' "$SKILLSTAR_GIT_ASKPASS_TOKEN"
"#,
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).unwrap();

    let session = GitOperationSession::new(
        "publish-push",
        GitAuthMaterial::available(TOKEN),
        Arc::new(NoopGitProgressSink),
    );
    let mut command = Command::new(&script);
    command.env("SKILLSTAR_FAKE_GIT_ARGV", &argv_log);

    let stdout = run_remote_git_command(
        &mut command,
        temp.path(),
        &["push", "-u", "origin", "HEAD"],
        "https://github.com/acme-org/skills.git",
        &session,
    )
    .expect("stub push succeeds");

    let argv = std::fs::read_to_string(&argv_log).expect("stub recorded its argv");
    assert!(argv.contains("push"), "the command still runs the push: {argv}");
    assert!(!argv.contains(TOKEN), "token must never enter argv: {argv}");
    assert!(
        !stdout.contains(TOKEN),
        "token echoed by Git must come back redacted: {stdout}"
    );
    assert!(stdout.contains("[REDACTED]"), "stdout: {stdout}");
}

#[test]
fn publishing_an_installed_skill_is_a_copy_and_never_moves_its_provenance() {
    let local_dir = std::path::Path::new("/hub/skills-local");

    // Installed Skills: the hub entry is a link into a repository checkout, so
    // publishing shares a copy and the local one keeps following its source.
    assert!(publish_copies_content(
        true,
        std::path::Path::new("/hub/repos/acme--skills/skills/writer"),
        local_dir,
    ));
    // A checkout that merely sits next to skills-local/ must not be mistaken
    // for it by a prefix that stops mid-component.
    assert!(publish_copies_content(
        true,
        std::path::Path::new("/hub/skills-local-cache/writer"),
        local_dir,
    ));

    // Locally authored Skills: publication is their graduation into Git, so the
    // lockfile write is the point of it.
    assert!(!publish_copies_content(
        true,
        std::path::Path::new("/hub/skills-local/writer"),
        local_dir,
    ));
    // A real directory in the hub was never a link and keeps its old behaviour.
    assert!(!publish_copies_content(
        false,
        std::path::Path::new("/hub/skills/writer"),
        local_dir,
    ));
}
