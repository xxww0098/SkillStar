//! Redaction stays off until `model_gateway.json` says otherwise, then a secret
//! in the upstream body is a placeholder and the agent's reply has it back.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use skillstar_gateway::{mask_outbound, unmask_response};

const SECRET: &str = "sk-proj-abcdEFGH1234ijklMNOP5678qrst";
const PHONE: &str = "13812345678";

#[test]
fn redact_off_leaves_body() {
    let _lock = lock_gateway_env();
    let root = scratch("redact-off");
    let _env = EnvRestore::sandbox(&root);
    let body = chat_body();

    let masked = mask_outbound(body.as_bytes());
    assert_eq!(masked, body.as_bytes());
    assert!(!key_path(&root).exists());

    write_gateway(
        &root,
        r#"{
  "redact": false,
  "redact_personal": false,
  "redact_words": false,
  "redact_rules": false,
  "redact_word_list": ["Nightjar"],
  "redact_rule_list": [{ "kind": "GW_KEY", "prefix": "acme-" }]
}
"#,
    );
    let with_lists = format!("{body} Nightjar acme-Zx9ab12cdEF");
    let still = mask_outbound(with_lists.as_bytes());
    assert_eq!(still, with_lists.as_bytes());
    assert!(
        still
            .windows(SECRET.len())
            .any(|window| window == SECRET.as_bytes())
    );
    assert!(!key_path(&root).exists());
}

#[test]
fn redact_on_masks_upstream_and_unmasks_response() {
    let _lock = lock_gateway_env();
    let root = scratch("redact-on");
    let _env = EnvRestore::sandbox(&root);
    write_gateway(&root, "{\"redact\":true}\n");
    let body = chat_body();
    let before = fs::read(gateway_file(&root)).unwrap();

    let masked = mask_outbound(body.as_bytes());
    let masked_text = String::from_utf8(masked.clone()).unwrap();
    assert!(!masked_text.contains(SECRET), "{masked_text}");
    assert!(masked_text.contains(PHONE), "{masked_text}");
    assert!(
        masked_text.contains("\"model\":\"text/m\""),
        "{masked_text}"
    );
    let token_at = masked_text
        .find("{{API_KEY_")
        .unwrap_or_else(|| panic!("upstream lost the placeholder: {masked_text}"));
    let token_end = masked_text[token_at..]
        .find("}}")
        .unwrap_or_else(|| panic!("placeholder did not close: {masked_text}"))
        + token_at
        + 2;
    let token = &masked_text[token_at..token_end];
    assert_eq!(fs::read(gateway_file(&root)).unwrap(), before);

    let reply = format!(r#"{{"text":"it is {token}"}}"#);
    let restored = unmask_response(reply.as_bytes());
    assert_eq!(
        String::from_utf8(restored).unwrap(),
        format!(r#"{{"text":"it is {SECRET}"}}"#)
    );
}

#[test]
fn redact_key_mode_is_0600() {
    let _lock = lock_gateway_env();
    let root = scratch("redact-key");
    let _env = EnvRestore::sandbox(&root);
    write_gateway(&root, "{\"redact\":true}\n");
    let _ = mask_outbound(chat_body().as_bytes());

    let path = key_path(&root);
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 32);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let _ = mask_outbound(chat_body().as_bytes());
    assert_eq!(fs::read(&path).unwrap().len(), 32);
}

fn chat_body() -> String {
    format!(
        r#"{{"model":"text/m","messages":[{{"role":"user","content":"the key is {SECRET} and phone {PHONE}"}}]}}"#
    )
}

fn gateway_file(root: &Path) -> PathBuf {
    root.join("data").join("config").join("model_gateway.json")
}

fn key_path(root: &Path) -> PathBuf {
    root.join("data").join("config").join("redact.key")
}

fn write_gateway(root: &Path, body: &str) {
    let path = gateway_file(root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn lock_gateway_env() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn scratch(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

struct EnvRestore {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    root: PathBuf,
}

impl EnvRestore {
    fn sandbox(root: &Path) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let pairs = [
            ("HOME", home.as_path()),
            ("USERPROFILE", home.as_path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", home.as_path()),
            ("SKILLSTAR_DATA_DIR", data.as_path()),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
                (key, previous)
            })
            .collect();
        Self {
            saved,
            root: root.to_path_buf(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
