use super::transport::{
    GitAuthMaterial, GitOperationPhase, GitOperationSession, GitProgressSink, classify_git_failure,
    configure_remote_command, execute_remote_command, internal_askpass_response, redact_git_output,
};
use std::process::Command;
use std::sync::{Arc, Mutex};

const TOKEN: &str = "github_pat_private_transport_canary";

#[derive(Default)]
struct RecordingSink(Mutex<Vec<super::transport::GitOperationProgress>>);

impl GitProgressSink for RecordingSink {
    fn emit(&self, progress: super::transport::GitOperationProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

#[test]
fn authenticated_command_exposes_secret_only_through_exact_askpass_environment() {
    let session = GitOperationSession::new(
        "scan-private-1",
        GitAuthMaterial::available(TOKEN),
        Arc::new(RecordingSink::default()),
    );
    let mut command = Command::new("git");

    configure_remote_command(
        &mut command,
        "https://github.com/acme/private-skills.git",
        &session,
    )
    .unwrap();

    let args = command
        .get_args()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let env = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    assert!(!args.contains(TOKEN), "token must never enter argv");
    assert_eq!(env.get("GIT_TERMINAL_PROMPT"), Some(&Some("0".into())));
    assert_eq!(env.get("GCM_INTERACTIVE"), Some(&Some("Never".into())));
    assert!(env.contains_key("GIT_ASKPASS"));
    assert_eq!(
        env.get("SKILLSTAR_GIT_ASKPASS_TOKEN"),
        Some(&Some(TOKEN.into()))
    );
    assert!(
        !env.keys().any(|key| key.starts_with("GIT_CONFIG_VALUE_")),
        "token must not use Git's config environment"
    );
}

#[test]
fn credential_is_not_attached_to_non_github_or_public_sessions() {
    let sink = Arc::new(RecordingSink::default());
    let authenticated = GitOperationSession::new(
        "scan-other-host",
        GitAuthMaterial::available(TOKEN),
        sink.clone(),
    );
    let public = GitOperationSession::new("scan-public", GitAuthMaterial::missing(), sink);

    for (remote, session) in [
        ("https://gitlab.com/acme/skills.git", &authenticated),
        ("https://github.com/acme/public-skills.git", &public),
    ] {
        let mut command = Command::new("git");
        configure_remote_command(&mut command, remote, session).unwrap();
        let env = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert_ne!(
            env.get("SKILLSTAR_GIT_ASKPASS_TOKEN"),
            Some(&Some(TOKEN.into()))
        );
        assert_ne!(
            env.get("GIT_ASKPASS"),
            Some(&Some("inherited-helper".into()))
        );
    }
}

#[test]
fn credential_bearing_remote_is_rejected_before_git_can_persist_or_log_it() {
    let session = GitOperationSession::new(
        "unsafe-url",
        GitAuthMaterial::available(TOKEN),
        Arc::new(RecordingSink::default()),
    );
    let mut command = Command::new("git");
    let error = configure_remote_command(
        &mut command,
        "https://already-secret@github.com/acme/repo.git",
        &session,
    )
    .expect_err("userinfo must be rejected");

    assert_eq!(error.code.as_str(), "unsafe_remote");
    assert!(
        !command
            .get_args()
            .any(|arg| arg.to_string_lossy().contains("already-secret"))
    );
}

#[test]
fn configured_proxy_is_operation_local_and_its_password_is_redacted() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }
    skillstar_core::config::proxy::save_config(&skillstar_core::config::proxy::ProxyConfig {
        enabled: true,
        proxy_type: skillstar_core::config::proxy::ProxyType::Http,
        host: "127.0.0.1".into(),
        port: 7890,
        username: Some("alice".into()),
        password: Some("proxy-password-canary".into()),
        bypass: Some("localhost".into()),
    })
    .unwrap();

    let session = GitOperationSession::new(
        "proxy",
        GitAuthMaterial::available(TOKEN),
        Arc::new(RecordingSink::default()),
    );
    let mut command = Command::new("git");
    configure_remote_command(
        &mut command,
        "https://github.com/acme/private-skills.git",
        &session,
    )
    .unwrap();
    let env = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        env.get("HTTPS_PROXY"),
        Some(&Some(
            "http://alice:proxy-password-canary@127.0.0.1:7890/".into()
        ))
    );
    assert_eq!(env.get("NO_PROXY"), Some(&Some("localhost".into())));
    let redacted = redact_git_output(
        "fatal: unable to access http://alice:proxy-password-canary@127.0.0.1:7890/",
        &session,
    );
    assert!(!redacted.contains("proxy-password-canary"));

    unsafe {
        match previous {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
    }
}

#[test]
fn progress_debug_and_failures_never_reveal_credentials() {
    let sink = Arc::new(RecordingSink::default());
    let session = GitOperationSession::new(
        "update-private-1",
        GitAuthMaterial::available(TOKEN),
        sink.clone(),
    );
    session.emit(
        GitOperationPhase::Running,
        "https://github.com/acme/private-skills.git",
    );

    let debug = format!("{session:?}");
    let progress = format!("{:?}", sink.0.lock().unwrap());
    let stderr =
        format!("fatal: auth failed for https://x-access-token:{TOKEN}@github.com/acme/repo");
    let sanitized = redact_git_output(&stderr, &session);
    let classified = classify_git_failure(&sanitized, &session);

    assert!(!debug.contains(TOKEN));
    assert!(!progress.contains(TOKEN));
    assert!(!sanitized.contains(TOKEN));
    assert!(!classified.to_string().contains(TOKEN));
    assert!(!serde_json::to_string(&classified).unwrap().contains(TOKEN));
}

#[test]
fn expired_missing_unauthorized_app_and_network_failures_are_distinct() {
    let sink = Arc::new(RecordingSink::default());
    let expired = GitOperationSession::new("expired", GitAuthMaterial::expired(), sink.clone());
    let missing = GitOperationSession::new("missing", GitAuthMaterial::missing(), sink.clone());
    let available = GitOperationSession::new("available", GitAuthMaterial::available(TOKEN), sink);

    assert_eq!(
        classify_git_failure("Authentication failed", &expired)
            .code
            .as_str(),
        "token_expired"
    );
    assert_eq!(
        classify_git_failure("Authentication failed", &missing)
            .code
            .as_str(),
        "not_authenticated"
    );
    assert_eq!(
        classify_git_failure("The requested URL returned error: 403", &available)
            .code
            .as_str(),
        "unauthorized"
    );
    assert_eq!(
        classify_git_failure("remote: Repository not found", &available)
            .code
            .as_str(),
        "app_not_installed"
    );
    assert_eq!(
        classify_git_failure("Could not resolve host: github.com", &available)
            .code
            .as_str(),
        "network"
    );
}

#[test]
fn internal_askpass_only_answers_marked_child_processes() {
    assert_eq!(
        internal_askpass_response(true, Some(TOKEN), "Username for 'https://github.com':"),
        Some("x-access-token".into())
    );
    assert_eq!(
        internal_askpass_response(true, Some(TOKEN), "Password for 'https://github.com':"),
        Some(TOKEN.into())
    );
    assert_eq!(
        internal_askpass_response(false, Some(TOKEN), "Password:"),
        None
    );
    assert_eq!(
        internal_askpass_response(true, Some(TOKEN), "Password for 'https://evil.example':"),
        None
    );
}

#[cfg(unix)]
#[test]
fn fake_transport_sees_credential_only_while_running_and_is_killed_on_cancel() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("fake-git");
    let marker = temp.path().join("credential-visible-without-argv-leak");
    std::fs::write(
        &script,
        r#"#!/bin/sh
if [ -z "$SKILLSTAR_GIT_ASKPASS_TOKEN" ]; then exit 41; fi
case "$*" in *"$SKILLSTAR_GIT_ASKPASS_TOKEN"*) exit 42;; esac
printf ok > "$SKILLSTAR_FAKE_GIT_MARKER"
sleep 30
"#,
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).unwrap();

    let session = GitOperationSession::new(
        "private-cancel",
        GitAuthMaterial::available(TOKEN),
        Arc::new(RecordingSink::default()),
    );
    let worker_session = session.clone();
    let worker_script = script.clone();
    let worker_marker = marker.clone();
    let worker = std::thread::spawn(move || {
        let mut command = Command::new(worker_script);
        command.env("SKILLSTAR_FAKE_GIT_MARKER", worker_marker);
        execute_remote_command(
            &mut command,
            None,
            &["fetch", "origin"],
            "https://github.com/acme/private-skills.git",
            &worker_session,
        )
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        marker.exists(),
        "fake command never observed the operation credential"
    );
    session.cancel();
    let error = worker
        .join()
        .unwrap()
        .expect_err("cancel should stop child");
    assert_eq!(error.code.as_str(), "cancelled");
    assert!(!format!("{error:?}").contains(TOKEN));
}
