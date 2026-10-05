//! Tests for the shared Claude Code credential-store addressing.
//!
//! `CLAUDE_CONFIG_DIR` pins the file store into a temp dir and the keychain
//! is always a fake runner, so nothing here touches a real `$HOME` or login
//! keychain. Env-mutating tests hold the crate's env lock.

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::json;

use super::*;

// ── addressing ───────────────────────────────────────────────────────────

#[test]
fn service_name_scopes_the_config_dir() {
    assert_eq!(service_for_config_dir(None), "Claude Code-credentials");
    assert_eq!(service_for_config_dir(Some("")), "Claude Code-credentials");
    assert_eq!(service_for_config_dir(Some("  ")), "Claude Code-credentials");
    // sha256("/tmp/claude-alt") starts with 04923786 (precomputed, so this
    // pins the derivation instead of re-deriving it with the same code).
    assert_eq!(
        service_for_config_dir(Some("/tmp/claude-alt")),
        "Claude Code-credentials-04923786"
    );
    // The env value is hashed as given, trimmed.
    assert_eq!(
        service_for_config_dir(Some(" /tmp/claude-alt ")),
        "Claude Code-credentials-04923786"
    );
}

#[test]
fn service_and_file_path_follow_the_same_config_dir() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("scoped");
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = crate::test_support::EnvGuard::set(&[("CLAUDE_CONFIG_DIR", &dir)]);

    assert_eq!(keychain_service(), service_for_config_dir(dir.to_str()));
    assert_eq!(credentials_file_path(), dir.join(".credentials.json"));
}

#[test]
fn keychain_account_prefers_the_login_name() {
    let _guard = crate::test_support::EnvGuard::set(&[("USER", "zz-alice".as_ref())]);
    assert_eq!(keychain_account(), "zz-alice");
}

#[test]
fn keychain_account_is_never_blank() {
    // Whatever the runner's environment is, the label is never blank —
    // Claude Code itself falls back to this exact string.
    let account = keychain_account();
    assert!(!account.trim().is_empty());
    if std::env::var("USER").is_err() && std::env::var("LOGNAME").is_err() {
        assert_eq!(account, KEYCHAIN_ACCOUNT_FALLBACK);
    }
}

// ── file store ───────────────────────────────────────────────────────────

#[test]
fn file_blob_reads_present_empty_and_missing() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("file");
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = crate::test_support::EnvGuard::set(&[("CLAUDE_CONFIG_DIR", &dir)]);

    assert_eq!(read_file_blob().unwrap(), None);

    std::fs::write(dir.join(".credentials.json"), "   ").unwrap();
    assert_eq!(read_file_blob().unwrap(), Some(json!({})));

    std::fs::write(
        dir.join(".credentials.json"),
        r#"{"mcpOAuth":{"github":"gho_x"},"claudeAiOauth":{"accessToken":"t"}}"#,
    )
    .unwrap();
    assert_eq!(
        read_file_blob().unwrap().unwrap()["mcpOAuth"]["github"].as_str(),
        Some("gho_x")
    );

    std::fs::write(dir.join(".credentials.json"), "not json").unwrap();
    assert!(read_file_blob().is_err());
}

// ── keychain IO via the runner seam ──────────────────────────────────────

/// In-memory keychain: one item, addressed by whatever service/account the
/// caller passes. Records every invocation for argument assertions.
#[derive(Default)]
struct FakeKeychain {
    state: Rc<RefCell<FakeState>>,
}

#[derive(Default)]
struct FakeState {
    blob: Option<String>,
    fail_find: Option<String>,
    fail_write: Option<String>,
    calls: Vec<Vec<String>>,
}

impl SecurityRunner for FakeKeychain {
    fn run(&self, args: &[String]) -> UsageResult<SecurityOutput> {
        self.state.borrow_mut().calls.push(args.to_vec());
        let mut state = self.state.borrow_mut();
        match args.first().map(String::as_str) {
            Some("find-generic-password") => match (state.blob.clone(), &state.fail_find) {
                (Some(blob), None) => Ok(SecurityOutput {
                    success: true,
                    status_code: Some(0),
                    stdout: blob,
                    stderr: String::new(),
                }),
                (_, Some(message)) => Ok(SecurityOutput {
                    success: false,
                    status_code: Some(100),
                    stdout: String::new(),
                    stderr: message.clone(),
                }),
                (None, None) => Ok(SecurityOutput {
                    success: false,
                    status_code: Some(44),
                    stdout: String::new(),
                    stderr: "The specified item could not be found in the keychain.".into(),
                }),
            },
            Some("add-generic-password") => {
                if let Some(message) = &state.fail_write {
                    return Ok(SecurityOutput {
                        success: false,
                        status_code: Some(100),
                        stdout: String::new(),
                        stderr: message.clone(),
                    });
                }
                let hex = args
                    .iter()
                    .position(|arg| arg == "-X")
                    .and_then(|index| args.get(index + 1))
                    .cloned()
                    .unwrap_or_default();
                state.blob = Some(decode_hex(&hex));
                Ok(SecurityOutput {
                    success: true,
                    status_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
            other => Err(UsageError::Other(format!("fake 不认识该命令：{other:?}"))),
        }
    }
}

fn decode_hex(hex: &str) -> String {
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex"))
        .collect::<Vec<u8>>()
        .into_iter()
        .map(|byte| byte as char)
        .collect()
}

#[test]
fn keychain_read_maps_not_found_to_none_and_failures_to_errors() {
    let fake = FakeKeychain::default();

    assert_eq!(read_keychain_blob(&fake).unwrap(), None);

    fake.state.borrow_mut().blob = Some(r#"{"claudeAiOauth":{"accessToken":"t"}}"#.into());
    let blob = read_keychain_blob(&fake).unwrap().unwrap();
    assert_eq!(blob["claudeAiOauth"]["accessToken"].as_str(), Some("t"));

    fake.state.borrow_mut().blob = Some("not json".into());
    assert!(read_keychain_blob(&fake).is_err());

    fake.state.borrow_mut().blob = None;
    fake.state.borrow_mut().fail_find = Some("User interaction is not allowed.".into());
    let error = read_keychain_blob(&fake).unwrap_err();
    assert!(error.to_string().contains("User interaction is not allowed."));
}

#[test]
fn keychain_write_roundtrips_through_hex_and_updates_in_place() {
    // `keychain_account` / `keychain_service` read process env. Hold the same
    // lock the USER / CLAUDE_CONFIG_DIR tests take, or a parallel test can
    // change the label between the write and this assertion.
    let _env = crate::test_support::lock_env();
    let fake = FakeKeychain::default();
    fake.state.borrow_mut().blob = Some(r#"{"mcpOAuth":{"github":"gho_x"}}"#.into());

    let blob = json!({"mcpOAuth": {"github": "gho_x"}, "claudeAiOauth": {"accessToken": "t"}});
    write_keychain_blob(&fake, &blob).unwrap();

    let calls = fake.state.borrow().calls.clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0][0], "add-generic-password");
    assert!(calls[0].contains(&"-U".to_string()));
    assert!(calls[0].contains(&"-X".to_string()));
    assert!(calls[0].contains(&keychain_service()));
    assert!(calls[0].contains(&keychain_account()));

    let read_back = read_keychain_blob(&fake).unwrap().unwrap();
    assert_eq!(read_back, blob, "hex write must decode back to the same blob");

    fake.state.borrow_mut().fail_write = Some("Keychain is locked.".into());
    let error = write_keychain_blob(&fake, &blob).unwrap_err();
    assert!(error.to_string().contains("Keychain is locked."));
}

#[test]
fn real_security_refuses_under_the_sandbox() {
    let temp = tempfile::tempdir().unwrap();
    let _guard = crate::test_support::EnvGuard::set(&[("SKILLSTAR_TOOL_SYNC_HOME", temp.path())]);
    let error = RealSecurity.run(&["find-generic-password".to_string()]).unwrap_err();
    if cfg!(target_os = "macos") {
        assert!(error.to_string().contains("SKILLSTAR_TOOL_SYNC_HOME"));
    }
}
