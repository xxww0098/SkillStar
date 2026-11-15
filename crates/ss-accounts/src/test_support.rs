//! One env sandbox for tests that touch account storage or a client's
//! credential file.
//!
//! Every variable that can route a write into the developer's real home is
//! pointed at a temp directory (and restored on drop), while the process-wide
//! lock keeps env-mutating tests serialised. Nothing in this suite can write a
//! real ~/.skillstar, ~/.codex, ~/.claude or Cursor database.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Serialises every test that mutates the process environment.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A temp root plus a sanitised environment, held for one test.
pub(crate) struct Sandbox {
    previous: Vec<(&'static str, Option<OsString>)>,
    temp: tempfile::TempDir,
    _env: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    /// Point the data root and every tool-config path at a temp directory.
    pub(crate) fn new() -> Self {
        let env = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("temp dir for a test sandbox");
        let home = temp.path().join("home");
        let tool_home = temp.path().join("tool-home");
        std::fs::create_dir_all(&home).expect("sandbox home");
        std::fs::create_dir_all(&tool_home).expect("sandbox tool home");
        let overrides: Vec<(&'static str, Option<PathBuf>)> = vec![
            ("HOME", Some(home.clone())),
            ("USERPROFILE", Some(home)),
            ("SKILLSTAR_DATA_DIR", Some(temp.path().join("data"))),
            ("SKILLSTAR_TOOL_SYNC_HOME", Some(tool_home)),
            ("CODEX_HOME", None),
            ("CLAUDE_CONFIG_DIR", None),
            ("GROK_HOME", None),
            ("SKILLSTAR_XAI_CLIENT_ID", None),
            ("XDG_CONFIG_HOME", None),
            ("APPDATA", None),
            // Antigravity 的 OAuth client 与端点覆盖：测试永远用内置值。
            ("SKILLSTAR_ANTIGRAVITY_CLIENT_ID", None),
            ("SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET", None),
            ("SKILLSTAR_ANTIGRAVITY_CLOUD_CODE_URL", None),
            ("SKILLSTAR_GEMINI_CLI_CLIENT_ID", None),
            ("SKILLSTAR_GEMINI_CLI_CLIENT_SECRET", None),
            ("SKILLSTAR_GEMINI_CLI_CLOUD_CODE_URL", None),
            ("ZCODE_CREDENTIAL_SECRET", None),
        ];
        let previous = overrides
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        unsafe {
            for (key, value) in &overrides {
                match value {
                    Some(path) => std::env::set_var(key, path),
                    None => std::env::remove_var(key),
                }
            }
        }
        Self {
            previous,
            temp,
            _env: env,
        }
    }

    /// The temp directory everything is rooted under.
    pub(crate) fn root(&self) -> &Path {
        self.temp.path()
    }

    /// A path inside the tool-config sandbox root
    /// (SKILLSTAR_TOOL_SYNC_HOME), creating nothing.
    pub(crate) fn tool_path(&self, parts: &[&str]) -> PathBuf {
        parts
            .iter()
            .fold(self.temp.path().join("tool-home"), |path, part| {
                path.join(part)
            })
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        unsafe {
            for (key, value) in &self.previous {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

/// A token shaped like a provider's own: header.payload.signature, payload
/// unsigned. Only the claims are read, and only for identity hints.
pub(crate) fn jwt_with_claims(claims: serde_json::Value) -> String {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("claims json"));
    format!("{header}.{payload}.signature")
}

impl Sandbox {
    /// Write a file inside the sandbox root, creating its directories.
    pub(crate) fn write_file(&self, path: &Path, body: impl AsRef<[u8]>) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("sandbox dir");
        }
        std::fs::write(path, body).expect("sandbox file");
    }
}