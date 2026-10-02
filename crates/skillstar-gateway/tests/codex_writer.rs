//! Codex save writes the loopback gateway and nothing else.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use skillstar_gateway::{
    ApplyError, CodexRoute, PLACEHOLDER_BEARER, apply_agent, apply_agent_with_model, release_agent,
};

fn gate() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "skillstar-codex-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct EnvRestore {
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvRestore {
    fn sandbox(home: &Path, data: &Path) -> Self {
        let pairs = [
            ("HOME", home),
            ("USERPROFILE", home),
            ("SKILLSTAR_TOOL_SYNC_HOME", home),
            ("SKILLSTAR_DATA_DIR", data),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
                (key.to_string(), previous)
            })
            .collect();
        Self { saved }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}

fn with_sandbox(label: &str, body: impl FnOnce(&Path)) {
    let _gate = gate();
    let root = Tmp::new(label);
    let home = root.path.join("home");
    let data = root.path.join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    let _env = EnvRestore::sandbox(&home, &data);
    body(&home);
}

fn config_path(home: &Path) -> PathBuf {
    home.join(".codex").join("config.toml")
}

fn write_config(home: &Path, body: &str) {
    let path = config_path(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, body).unwrap();
}

fn read_config(home: &Path) -> String {
    fs::read_to_string(config_path(home)).unwrap()
}

const ORIGIN: &str = "http://127.0.0.1:9";

#[test]
fn codex_writer_logged_in_writes_backend_api_url() {
    with_sandbox("logged-in", |home| {
        write_config(home, "model = \"gpt-5\"\n\n[tui]\ntheme = \"dark\"\n");
        apply_agent("codex", CodexRoute::LoggedIn, ORIGIN, home).unwrap();
        let text = read_config(home);
        assert!(
            text.contains("openai_base_url = \"http://127.0.0.1:9/backend-api/codex\"\n"),
            "{text}"
        );
        assert!(text.contains("model = \"gpt-5\"\n"), "{text}");
        assert!(text.contains("[tui]\ntheme = \"dark\"\n"), "{text}");
        assert!(!text.contains("model_providers.skillstar"), "{text}");
        assert!(!text.contains(PLACEHOLDER_BEARER), "{text}");
    });
}

#[test]
fn codex_writer_api_mode_writes_skillstar_provider_table() {
    with_sandbox("api", |home| {
        write_config(home, "model = \"gpt-5\"\n\n[tui]\ntheme = \"dark\"\n");
        apply_agent("codex", CodexRoute::Api, ORIGIN, home).unwrap();
        let text = read_config(home);
        assert!(text.contains("model = \"gpt-5\"\n"), "{text}");
        assert!(text.contains("[tui]\ntheme = \"dark\"\n"), "{text}");
        assert!(text.contains("model_provider = \"skillstar\"\n"), "{text}");
        let catalog = home.join(".codex").join("skillstar-models.json");
        let catalog_text = fs::read_to_string(&catalog).unwrap();
        assert_eq!(catalog_text, "{\n \"models\": []\n}");
        let quoted = catalog
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        assert!(
            text.contains(&format!("model_catalog_json = \"{quoted}\"\n")),
            "{text}"
        );
        let table = "\
[model_providers.skillstar]
name = \"skillstar\"
base_url = \"http://127.0.0.1:9/v1\"
wire_api = \"responses\"
experimental_bearer_token = \"skillstar\"
";
        assert!(text.contains(table), "{text}");
        assert!(!text.contains("openai_base_url"), "{text}");
    });
}

#[test]
fn codex_writer_never_writes_vendor_key() {
    with_sandbox("no-key", |home| {
        let original = "\
# user note
user_note = \"keep\"

[other]
kept = \"yes\"
";
        write_config(home, original);
        apply_agent("codex", CodexRoute::LoggedIn, ORIGIN, home).unwrap();
        apply_agent("codex", CodexRoute::Api, ORIGIN, home).unwrap();
        let text = read_config(home);
        for forbidden in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "api_key",
            "api.openai.com",
            "api.anthropic.com",
            "wire_api = \"chat\"",
        ] {
            assert!(!text.contains(forbidden), "{forbidden} in {text}");
        }
        assert!(
            text.contains("experimental_bearer_token = \"skillstar\"\n"),
            "{text}"
        );
        assert!(text.contains("user_note = \"keep\"\n"), "{text}");
        assert!(text.contains("[other]\nkept = \"yes\"\n"), "{text}");
        let table = text.split("[model_providers.skillstar]").nth(1).unwrap();
        assert!(!table.contains("user_note"), "{text}");
        assert!(!table.contains("kept"), "{text}");
    });
}

#[test]
fn codex_writer_unmanaged_agent_writes_zero_bytes() {
    with_sandbox("unmanaged", |home| {
        let body = "model = \"gpt-5\"\n";
        write_config(home, body);
        let path = config_path(home);
        let before = fs::metadata(&path).unwrap();
        let error = apply_agent("gemini", CodexRoute::LoggedIn, ORIGIN, home).unwrap_err();
        assert_eq!(error.to_string(), "agent_not_managed");
        assert!(matches!(error, ApplyError::NotManaged));
        let release = release_agent("claude-code", home).unwrap_err();
        assert_eq!(release.to_string(), "agent_not_managed");
        let after = fs::metadata(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), body.as_bytes());
        assert_eq!(before.len(), after.len());
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        let stash = std::env::var("SKILLSTAR_DATA_DIR").unwrap();
        assert!(
            !Path::new(&stash)
                .join("config")
                .join("agent_stash.json")
                .exists()
        );
    });
}

#[test]
fn codex_writer_stash_roundtrip() {
    with_sandbox("stash", |home| {
        write_config(
            home,
            "\
# keep
openai_base_url = \"https://vendor.example/v1\"
model = \"gpt-5\"

[tui]
theme = \"dark\"
",
        );
        apply_agent("codex", CodexRoute::LoggedIn, ORIGIN, home).unwrap();
        let stashed = fs::read_to_string(
            Path::new(&std::env::var("SKILLSTAR_DATA_DIR").unwrap())
                .join("config")
                .join("agent_stash.json"),
        )
        .unwrap();
        assert!(
            stashed.contains("\"codex.openai_base_url\": \"https://vendor.example/v1\""),
            "{stashed}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(
                Path::new(&std::env::var("SKILLSTAR_DATA_DIR").unwrap())
                    .join("config")
                    .join("agent_stash.json"),
            )
            .unwrap()
            .permissions()
            .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let mid = read_config(home);
        assert!(
            mid.contains("openai_base_url = \"http://127.0.0.1:9/backend-api/codex\"\n"),
            "{mid}"
        );
        assert!(mid.contains("model = \"gpt-5\"\n"), "{mid}");
        assert!(mid.contains("[tui]\ntheme = \"dark\"\n"), "{mid}");

        apply_agent("codex", CodexRoute::Api, ORIGIN, home).unwrap();
        let api = read_config(home);
        assert!(!api.contains("openai_base_url"), "{api}");
        assert!(api.contains("model_provider = \"skillstar\"\n"), "{api}");
        assert!(api.contains("model = \"gpt-5\"\n"), "{api}");
        assert!(api.contains("theme = \"dark\"\n"), "{api}");
        let catalog = home.join(".codex").join("skillstar-models.json");
        assert!(catalog.is_file());

        release_agent("codex", home).unwrap();
        let restored = read_config(home);
        assert!(
            restored.contains("openai_base_url = \"https://vendor.example/v1\"\n"),
            "{restored}"
        );
        assert!(restored.contains("model = \"gpt-5\"\n"), "{restored}");
        assert!(restored.contains("[tui]\ntheme = \"dark\"\n"), "{restored}");
        assert!(!restored.contains("model_provider ="), "{restored}");
        assert!(!restored.contains("model_catalog_json"), "{restored}");
        assert!(
            restored.contains("[model_providers.skillstar]"),
            "{restored}"
        );
        assert!(catalog.is_file(), "catalog file stays after release");
        let stash_path = Path::new(&std::env::var("SKILLSTAR_DATA_DIR").unwrap())
            .join("config")
            .join("agent_stash.json");
        assert!(!stash_path.exists(), "empty stash file is removed");
    });
}

#[test]
fn codex_model_save_selects_the_model_and_release_restores_it() {
    with_sandbox("model-save", |home| {
        write_config(home, "model = \"gpt-5\"\n\n[tui]\ntheme = \"dark\"\n");
        apply_agent_with_model("codex", ORIGIN, home, "deepseek/pro").unwrap();
        let text = read_config(home);
        assert!(
            text.contains("model = \"skillstar/deepseek/pro\"\n"),
            "{text}"
        );
        assert!(text.contains("[model_providers.skillstar]"), "{text}");
        assert!(text.contains("model_provider = \"skillstar\"\n"), "{text}");
        assert!(text.contains("[tui]\ntheme = \"dark\"\n"), "{text}");

        release_agent("codex", home).unwrap();
        let restored = read_config(home);
        assert!(restored.contains("model = \"gpt-5\"\n"), "{restored}");
        assert!(!restored.contains("model_provider ="), "{restored}");
        assert!(!restored.contains("skillstar/deepseek/pro"), "{restored}");
    });
}

#[test]
fn codex_model_save_with_empty_ref_releases() {
    with_sandbox("model-release", |home| {
        write_config(home, "model = \"gpt-5\"\n");
        apply_agent_with_model("codex", ORIGIN, home, "deepseek/pro").unwrap();
        assert!(read_config(home).contains("skillstar/deepseek/pro"));
        apply_agent_with_model("codex", ORIGIN, home, "").unwrap();
        let text = read_config(home);
        assert!(text.contains("model = \"gpt-5\"\n"), "{text}");
        assert!(!text.contains("skillstar/deepseek/pro"), "{text}");
    });
}

#[test]
fn codex_model_save_unmanaged_agent_writes_zero_bytes() {
    with_sandbox("model-unmanaged", |home| {
        let err = apply_agent_with_model("goose", ORIGIN, home, "deepseek/pro").unwrap_err();
        assert!(matches!(err, ApplyError::NotManaged), "{err}");
        assert!(!config_path(home).exists(), "goose wrote a codex file");
    });
}
