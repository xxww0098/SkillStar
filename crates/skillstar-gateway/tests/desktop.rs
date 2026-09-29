//! Claude Desktop's native config, alias, and per-OS folders.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use skillstar_gateway::{
    DESKTOP_PROFILE_ID, apply_gateway, desktop_accepts, desktop_alias, desktop_dirs,
    desktop_effort_alias,
};

const SPEC_ID: &str = "00000000-0000-4000-8000-736b696c6c73";
const MAGPIE_ID_TAIL: &str = "6d6167706965";
const REF: &str = "deepseek/pro";

fn goos() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
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
            "skillstar-desktop-{label}-{}-{nanos}",
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

fn gate() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct EnvRestore {
    saved: Vec<(String, Option<std::ffi::OsString>)>,
}

impl EnvRestore {
    fn set(pairs: &[(&str, &std::ffi::OsStr)]) -> Self {
        let saved = pairs
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
                ((*key).to_string(), previous)
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

fn with_home(label: &str, body: impl FnOnce(&Path)) {
    let _gate = gate();
    let root = Tmp::new(label);
    let home = root.path.join("home");
    let data = root.path.join("data");
    let wrong = root.path.join("wrong-override");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&wrong).unwrap();
    let addr = std::ffi::OsString::from("127.0.0.1:21847");
    let _env = EnvRestore::set(&[
        ("HOME", home.as_os_str()),
        ("USERPROFILE", home.as_os_str()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.as_os_str()),
        ("SKILLSTAR_DATA_DIR", data.as_os_str()),
        ("SKILLSTAR_GATEWAY_ADDR", addr.as_os_str()),
        ("APPDATA", home.as_os_str()),
        ("LOCALAPPDATA", home.as_os_str()),
        ("XDG_CONFIG_HOME", wrong.as_os_str()),
    ]);
    body(&home);
}

fn paths(home: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let (dir, dir3p) = desktop_dirs(goos(), home, |_| None);
    let library = dir3p.join("configLibrary");
    (
        dir.join("claude_desktop_config.json"),
        dir3p.join("claude_desktop_config.json"),
        library.join(format!("{SPEC_ID}.json")),
        library.join("_meta.json"),
    )
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn desktop_profile_id() {
    assert_eq!(DESKTOP_PROFILE_ID, SPEC_ID);
    assert!(!SPEC_ID.contains(MAGPIE_ID_TAIL));
    with_home("profile-id", |home| {
        apply_gateway("claude-desktop", REF).unwrap();
        let (_config, _config3p, profile, meta) = paths(home);
        assert!(
            profile.ends_with(format!("{SPEC_ID}.json")),
            "{}",
            profile.display()
        );
        let profile_text = fs::read_to_string(&profile).unwrap();
        let meta_text = fs::read_to_string(&meta).unwrap();
        assert!(profile_text.contains("\"inferenceProvider\": \"gateway\""));
        assert!(meta_text.contains(&format!("\"appliedId\": \"{SPEC_ID}\"")));
        assert!(meta_text.contains(&format!("\"id\": \"{SPEC_ID}\"")));
        assert!(meta_text.contains("\"name\": \"skillstar\""));
        assert!(!profile_text.contains(MAGPIE_ID_TAIL), "{profile_text}");
        assert!(!meta_text.contains(MAGPIE_ID_TAIL), "{meta_text}");
        assert!(!meta_text.contains("magpie"), "{meta_text}");
    });
}

#[test]
fn desktop_alias_fnv64a() {
    let alias = desktop_alias("deepseek/pro");
    assert_eq!(alias, "anthropic/skillstar-4480770935");
    assert!(desktop_accepts(&alias), "{alias}");
    assert!(!desktop_accepts("deepseek/pro"));
    let effort = desktop_effort_alias("deepseek/pro");
    assert_eq!(effort, "mythos-skillstar-4480770935");
    assert!(desktop_accepts(&effort), "{effort}");
    assert_eq!(desktop_alias("a"), "anthropic/skillstar-0555641996");
    assert!(desktop_accepts("opus-4.8"));
    assert!(desktop_accepts("Sonnet"));
    assert!(!desktop_accepts("gpt-4o"));
    assert!(!desktop_accepts("claude-ling"));
    assert!(desktop_accepts("claudeling"));
    assert!(!desktop_accepts("skillstar-1"));
}

#[test]
fn desktop_writes_native_config() {
    with_home("native", |home| {
        let prior = "{\n  \"mcpServers\": {\"kept\": {}},\n  \"theme\": \"dark\"\n}\n";
        let (config, config3p, profile, meta) = paths(home);
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, prior).unwrap();
        apply_gateway("claude-desktop", REF).unwrap();
        let origin = "http://127.0.0.1:21847";
        for path in [&config, &config3p] {
            let text = fs::read_to_string(path).unwrap();
            assert!(text.contains("\"deploymentMode\": \"3p\""), "{text}");
            assert!(!text.contains("theme"), "{text}");
        }
        let profile_text = fs::read_to_string(&profile).unwrap();
        assert!(
            profile_text.contains(&format!("\"inferenceGatewayBaseUrl\": \"{origin}\"")),
            "{profile_text}"
        );
        assert!(!profile_text.contains("/v1"), "{profile_text}");
        assert!(
            profile_text.contains("\"inferenceGatewayApiKey\": \"skillstar-claude-desktop\""),
            "{profile_text}"
        );
        assert!(meta.is_file());
        let wrong = home.parent().unwrap().join("wrong-override");
        assert!(!wrong.join("Claude").exists());
        assert!(!home.join("Claude").exists());
        apply_gateway("claude-desktop", "").unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), prior);
        assert!(!profile.exists());
    });
}

#[test]
fn desktop_does_not_write_marker() {
    with_home("marker", |home| {
        apply_gateway("claude-desktop", REF).unwrap();
        let mut files = Vec::new();
        walk(home.parent().unwrap(), &mut files);
        assert!(!files.is_empty());
        for path in files {
            assert_ne!(
                path.file_name().and_then(|name| name.to_str()),
                Some("skillstar-binding.json"),
                "{}",
                path.display()
            );
        }
    });
}

#[test]
fn desktop_paths_per_os() {
    let root = Tmp::new("paths");
    let home = root.path.join("home");
    fs::create_dir_all(&home).unwrap();

    let (claude, claude3p) = desktop_dirs("darwin", &home, |_| Some("/tmp/should-ignore".into()));
    assert_eq!(
        claude,
        home.join("Library").join("Application Support").join("Claude")
    );
    assert_eq!(
        claude3p,
        home.join("Library")
            .join("Application Support")
            .join("Claude-3p")
    );

    let local = root.path.join("local");
    fs::create_dir_all(local.join("Claude-App")).unwrap();
    fs::create_dir_all(local.join("Claude-3p-beta")).unwrap();
    let local_text = local.display().to_string();
    let (scanned, scanned3p) = desktop_dirs("windows", &home, |key| {
        (key == "LOCALAPPDATA").then(|| local_text.clone())
    });
    assert_eq!(scanned, local.join("Claude-App"));
    assert_eq!(scanned3p, local.join("Claude-3p-beta"));

    fs::create_dir_all(local.join("Claude")).unwrap();
    fs::create_dir_all(local.join("Claude-3p")).unwrap();
    let (exact, exact3p) = desktop_dirs("windows", &home, |key| {
        (key == "LOCALAPPDATA").then(|| local_text.clone())
    });
    assert_eq!(exact, local.join("Claude"));
    assert_eq!(exact3p, local.join("Claude-3p"));

    let (fallback, _) = desktop_dirs("windows", &home, |_| None);
    assert_eq!(fallback, home.join("AppData").join("Local").join("Claude"));

    let xdg = root.path.join("xdg");
    let xdg_text = xdg.display().to_string();
    let (xdg_claude, xdg3p) = desktop_dirs("linux", &home, |key| {
        (key == "XDG_CONFIG_HOME").then(|| xdg_text.clone())
    });
    assert_eq!(xdg_claude, xdg.join("Claude"));
    assert_eq!(xdg3p, xdg.join("Claude-3p"));

    let (relative, _) = desktop_dirs("linux", &home, |key| {
        (key == "XDG_CONFIG_HOME").then(|| "relative/xdg".to_string())
    });
    assert_eq!(relative, home.join(".config").join("Claude"));

    let (plain, plain3p) = desktop_dirs("linux", &home, |_| None);
    assert_eq!(plain, home.join(".config").join("Claude"));
    assert_eq!(plain3p, home.join(".config").join("Claude-3p"));
}
