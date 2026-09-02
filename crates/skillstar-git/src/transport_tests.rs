use super::transport::{
    GitAuthMaterial, GitOperationPhase, GitOperationSession, GitProgressSink, classify_git_failure,
    configure_remote_command, execute_remote_command, internal_askpass_response, redact_git_output,
};
use std::ffi::OsString;
use std::process::Command;
use std::sync::{Arc, Mutex};

const PROXY_ENV_KEYS: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
];

struct IsolatedProxyEnv {
    previous_data_dir: Option<OsString>,
    previous_proxy: Vec<(&'static str, Option<OsString>)>,
}

impl IsolatedProxyEnv {
    fn with_inherited_canary() -> Self {
        let previous_proxy = PROXY_ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            for key in PROXY_ENV_KEYS {
                std::env::set_var(key, "http://inherited-canary:1/");
            }
        }
        Self {
            previous_data_dir,
            previous_proxy,
        }
    }
}

impl Drop for IsolatedProxyEnv {
    fn drop(&mut self) {
        unsafe {
            match &self.previous_data_dir {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
            for (key, previous) in self.previous_proxy.drain(..).rev() {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

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
    let mut command = skillstar_core::infra::path_env::command_with_path("git");

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
        let mut command = skillstar_core::infra::path_env::command_with_path("git");
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
fn signed_in_public_probe_uses_the_same_session_without_credential() {
    let authenticated = GitOperationSession::new(
        "public-probe",
        GitAuthMaterial::available(TOKEN),
        Arc::new(RecordingSink::default()),
    );
    let anonymous = authenticated.unauthenticated_view();
    let mut command = skillstar_core::infra::path_env::command_with_path("git");

    configure_remote_command(
        &mut command,
        "https://github.com/acme/public-skills.git",
        &anonymous,
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

    assert_eq!(anonymous.id(), authenticated.id());
    assert!(!anonymous.has_credential());
    assert_eq!(env.get("SKILLSTAR_GIT_ASKPASS_TOKEN"), Some(&None));
}

#[test]
fn recovery_view_retains_auth_without_inheriting_sticky_cancellation() {
    let session = GitOperationSession::new(
        "cancelled-update",
        GitAuthMaterial::available(TOKEN),
        Arc::new(RecordingSink::default()),
    );
    session.cancel();

    let recovery = session.recovery_view();

    assert!(session.is_cancelled());
    assert!(!recovery.is_cancelled());
    assert!(recovery.has_credential());
    assert_eq!(recovery.id(), session.id());
    assert!(!format!("{recovery:?}").contains(TOKEN));
}

#[test]
fn credential_bearing_remote_is_rejected_before_git_can_persist_or_log_it() {
    let session = GitOperationSession::new(
        "unsafe-url",
        GitAuthMaterial::available(TOKEN),
        Arc::new(RecordingSink::default()),
    );
    let mut command = skillstar_core::infra::path_env::command_with_path("git");
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
    let _isolated = IsolatedProxyEnv::with_inherited_canary();
    let temp = tempfile::tempdir().unwrap();
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
    let mut command = skillstar_core::infra::path_env::command_with_path("git");
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
    if !cfg!(windows) {
        for inherited_key in ["http_proxy", "https_proxy", "all_proxy", "no_proxy"] {
            assert_eq!(
                env.get(inherited_key),
                Some(&None),
                "operation must block inherited {inherited_key}"
            );
        }
    }
    let mut probe = if cfg!(windows) {
        Command::new("cmd")
    } else {
        Command::new("env")
    };
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => {
                probe.env(key, value);
            }
            None => {
                probe.env_remove(key);
            }
        }
    }
    if cfg!(windows) {
        probe.args(["/C", "set"]);
    }
    let probe_output = probe.output().unwrap();
    assert!(
        probe_output.status.success(),
        "proxy env probe failed: {}",
        String::from_utf8_lossy(&probe_output.stderr)
    );
    let dumped = String::from_utf8_lossy(&probe_output.stdout);
    assert!(
        !dumped.to_ascii_lowercase().contains("inherited-canary"),
        "child must not inherit process/runner proxy: {dumped}"
    );
    assert!(
        dumped.contains("http://alice:proxy-password-canary@127.0.0.1:7890/"),
        "child must use the SkillStar proxy: {dumped}"
    );
    let redacted = redact_git_output(
        "fatal: unable to access http://alice:proxy-password-canary@127.0.0.1:7890/",
        &session,
    );
    assert!(!redacted.contains("proxy-password-canary"));
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
    for message in [
        "Could not resolve proxy: proxy.example",
        "Failed to connect to proxy.example port 8080",
        "Operation timed out after 300001 milliseconds",
        "Timeout was reached",
    ] {
        assert_eq!(
            classify_git_failure(message, &available).code.as_str(),
            "network"
        );
    }
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    // `configure_remote_command` reads proxy configuration via the global
    // data directory. Serialize with the test that mutates that environment.
    let _guard = crate::lock_test_env();
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
    let mut command = Command::new(script);
    command.env("SKILLSTAR_FAKE_GIT_MARKER", marker.clone());

    // Run the command on this thread. The old worker-plus-poll shape let a
    // busy full-workspace runner spend the whole deadline before scheduling the
    // worker. This watcher cancels only after the child proves it received the
    // operation credential; if the command returns first, it reports failure
    // without ever pre-cancelling the command before spawn.
    let command_finished = Arc::new(AtomicBool::new(false));
    let watcher_finished = command_finished.clone();
    let canceller_session = session.clone();
    let canceller_marker = marker.clone();
    let canceller = std::thread::spawn(move || {
        loop {
            if canceller_marker.exists() {
                canceller_session.cancel();
                return true;
            }
            if watcher_finished.load(Ordering::SeqCst) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    let result = execute_remote_command(
        &mut command,
        None,
        &["fetch", "origin"],
        "https://github.com/acme/private-skills.git",
        &session,
    );
    command_finished.store(true, Ordering::SeqCst);
    let observed_credential = canceller
        .join()
        .expect("credential canceller must not panic");
    assert!(
        observed_credential,
        "fake command never observed the operation credential; command result: {result:?}"
    );
    let error = result.expect_err("cancel should stop child");
    assert_eq!(error.code.as_str(), "cancelled");
    assert!(!format!("{error:?}").contains(TOKEN));
}
