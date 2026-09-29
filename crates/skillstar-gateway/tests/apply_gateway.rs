//! File agents point at the loopback gateway and nowhere else.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use skillstar_gateway::{ApplyError, FILE_AGENTS, apply_gateway};

const REF: &str = "deepseek/pro";
const ADDR: &str = "127.0.0.1:21847";

const ROSTER: &[(&str, &[(&str, &str)])] = &[
    (
        "gemini",
        &[
            (".gemini/.env", "gemini.env"),
            (".gemini/settings.json", "gemini.settings.json"),
        ],
    ),
    ("opencode", &[(".config/opencode/opencode.json", "opencode.json")]),
    (
        "mimocode",
        &[(".config/mimocode/mimocode.json", "mimocode.json")],
    ),
    (
        "pi",
        &[
            (".pi/agent/models.json", "pi.models.json"),
            (".pi/agent/settings.json", "pi.settings.json"),
        ],
    ),
    (
        "crush",
        &[(".config/crush/crush.json", "crush.json")],
    ),
    ("dsh", &[(".dsh/config.yaml", "dsh.config.yaml")]),
    (
        "commandcode",
        &[
            (".commandcode/settings.json", "commandcode.settings.json"),
            (".commandcode/providers.json", "commandcode.providers.json"),
        ],
    ),
    ("fx", &[(".fx/settings.json", "fx.settings.json")]),
    (
        "omp",
        &[
            (".omp/agent/config.yml", "omp.config.yml"),
            (".omp/agent/models.yml", "omp.models.yml"),
        ],
    ),
    ("hermes", &[(".hermes/config.yaml", "hermes.config.yaml")]),
    (
        "cline",
        &[
            (
                ".cline/data/settings/providers.json",
                "cline.providers.json",
            ),
            (".cline/data/settings/models.json", "cline.models.json"),
        ],
    ),
    ("qoder", &[(".qoder/settings.json", "qoder.settings.json")]),
    (
        "qoder-cn",
        &[(".qoder-cn/settings.json", "qoder-cn.settings.json")],
    ),
    ("grok", &[(".grok/config.toml", "grok.config.toml")]),
    (
        "zcode",
        &[
            (".zcode/v2/config.json", "zcode.config.json"),
            (
                ".zcode/v2/provider_config.json",
                "zcode.provider_config.json",
            ),
        ],
    ),
    (
        "workbuddy",
        &[(".workbuddy/models.json", "workbuddy.models.json")],
    ),
];

const HOSTS: &[&str] = &[
    "api.openai.com",
    "api.anthropic.com",
    "generativelanguage.googleapis.com",
    "api.deepseek.com",
    "openrouter.ai",
    "usemagpie.ai",
];

const SECRET_SHAPES: &[&str] = &["sk-ant-", "sk-proj-", "vendor-secret-should-not-leak"];

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
            "skillstar-agents-{label}-{}-{nanos}",
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
    fn set(pairs: &[(&str, &OsStr)]) -> Self {
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

fn with_sandbox(label: &str, body: impl FnOnce(&Path, &Path)) {
    let _gate = gate();
    let root = Tmp::new(label);
    let home = root.path.join("home");
    let data = root.path.join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    let wrong = root.path.join("wrong-override");
    fs::create_dir_all(&wrong).unwrap();
    let addr = OsString::from(ADDR);
    let home_os = home.as_os_str();
    let data_os = data.as_os_str();
    let wrong_os = wrong.as_os_str();
    let _env = EnvRestore::set(&[
        ("HOME", home_os),
        ("USERPROFILE", home_os),
        ("SKILLSTAR_TOOL_SYNC_HOME", home_os),
        ("SKILLSTAR_DATA_DIR", data_os),
        ("SKILLSTAR_GATEWAY_ADDR", addr.as_os_str()),
        ("APPDATA", home_os),
        ("LOCALAPPDATA", home_os),
        ("XDG_CONFIG_HOME", wrong_os),
        ("GROK_HOME", wrong_os),
        ("HERMES_HOME", wrong_os),
        ("DSH_HOME", wrong_os),
        ("CLINE_DIR", wrong_os),
        ("QODER_CONFIG_DIR", wrong_os),
        ("QODERCN_CONFIG_DIR", wrong_os),
        ("WORKBUDDY_CONFIG_DIR", wrong_os),
    ]);
    body(&home, &data);
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/agents")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("fixture {name}: {error}"))
}

fn rows(id: &str) -> &'static [(&'static str, &'static str)] {
    ROSTER
        .iter()
        .find(|(agent, _)| *agent == id)
        .map(|(_, rows)| *rows)
        .unwrap_or_else(|| panic!("no roster row for {id}"))
}

fn check_agent(home: &Path, id: &str) {
    apply_gateway(id, REF).unwrap();
    for (rel, name) in rows(id) {
        let actual = fs::read_to_string(home.join(rel)).unwrap_or_else(|error| {
            panic!("{id} {rel}: {error}");
        });
        assert_eq!(actual, fixture(name), "{id} {rel}");
    }
}

fn assert_unmanaged(home: &Path, data: &Path, id: &str) {
    let before = walk(home);
    let err = apply_gateway(id, REF).unwrap_err();
    assert!(
        matches!(err, ApplyError::NotManaged),
        "{id}: {err}"
    );
    assert_eq!(err.to_string(), "agent_not_managed");
    assert_eq!(walk(home), before, "{id} wrote a file");
    assert!(!data.join("config").join("agent_stash.json").exists(), "{id}");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    walk_into(dir, &mut out);
    out.sort();
    out
}

fn walk_into(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk_into(&path, out);
        } else {
            out.push(path);
        }
    }
}

macro_rules! fixture_test {
    ($name:ident, $id:expr) => {
        #[test]
        fn $name() {
            with_sandbox(stringify!($name), |home, _data| {
                check_agent(home, $id);
            });
        }
    };
}

fixture_test!(apply_gateway_gemini_matches_fixture, "gemini");
fixture_test!(apply_gateway_opencode_matches_fixture, "opencode");
fixture_test!(apply_gateway_mimocode_matches_fixture, "mimocode");
fixture_test!(apply_gateway_pi_matches_fixture, "pi");
fixture_test!(apply_gateway_crush_matches_fixture, "crush");
fixture_test!(apply_gateway_dsh_matches_fixture, "dsh");
fixture_test!(apply_gateway_commandcode_matches_fixture, "commandcode");
fixture_test!(apply_gateway_fx_matches_fixture, "fx");
fixture_test!(apply_gateway_omp_matches_fixture, "omp");
fixture_test!(apply_gateway_hermes_matches_fixture, "hermes");
fixture_test!(apply_gateway_cline_matches_fixture, "cline");
fixture_test!(apply_gateway_qoder_matches_fixture, "qoder");
fixture_test!(apply_gateway_qoder_cn_matches_fixture, "qoder-cn");
fixture_test!(apply_gateway_grok_matches_fixture, "grok");
fixture_test!(apply_gateway_zcode_matches_fixture, "zcode");
fixture_test!(apply_gateway_workbuddy_matches_fixture, "workbuddy");

#[test]
fn apply_gateway_specials_still_unmanaged() {
    with_sandbox("specials", |home, data| {
        for id in [
            "codex",
            "claude",
            "claude-desktop",
            "hanako",
            "alma",
            "cindy",
            "codex@wsl:debian",
            "not-an-agent",
        ] {
            assert_unmanaged(home, data, id);
        }
    });
}

#[test]
fn apply_gateway_name_only_still_unmanaged() {
    with_sandbox("name-only", |home, data| {
        for id in ["goose", "cursor", "copilot", "devin"] {
            assert_unmanaged(home, data, id);
        }
    });
}

#[test]
fn apply_gateway_writes_no_vendor_secret() {
    with_sandbox("secrets", |home, _data| {
        let ids: Vec<&str> = ROSTER.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, FILE_AGENTS);
        for id in FILE_AGENTS {
            apply_gateway(id, REF).unwrap();
        }
        let files = walk(home);
        assert!(!files.is_empty());
        for path in files {
            let text = String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned();
            for host in HOSTS {
                assert!(!text.contains(host), "{} contains {host}", path.display());
            }
            for shape in SECRET_SHAPES {
                assert!(
                    !text.contains(shape),
                    "{} contains {shape}",
                    path.display()
                );
            }
            assert!(!text.contains(":3425"), "{}", path.display());
            assert!(!text.contains("magpie"), "{}", path.display());
        }
    });
}

#[test]
fn apply_gateway_cancel_restores_previous_file() {
    with_sandbox("cancel", |home, data| {
        let prior = "{\"theme\":\"kept\",\"apiKey\":\"vendor-secret-should-not-leak\"}\n";
        let path = home.join(".config/opencode/opencode.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, prior).unwrap();

        apply_gateway("opencode", REF).unwrap();
        let managed = fs::read_to_string(&path).unwrap();
        assert_eq!(managed, fixture("opencode.json"));
        assert!(!managed.contains("vendor-secret-should-not-leak"));

        fs::write(&path, "mutated-while-managed\n").unwrap();
        apply_gateway("opencode", "other/model").unwrap();
        let second = fs::read_to_string(&path).unwrap();
        assert!(second.contains("other/model"), "{second}");
        assert!(!second.contains("mutated-while-managed"));

        apply_gateway("opencode", "").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), prior);
        assert!(!data.join("config").join("agent_stash.json").exists());
    });
}

#[test]
fn apply_gateway_cancel_of_missing_file_removes_it() {
    with_sandbox("cancel-missing", |home, data| {
        apply_gateway("gemini", "").unwrap();
        assert!(walk(home).is_empty());
        apply_gateway("gemini", REF).unwrap();
        assert!(home.join(".gemini/.env").exists());
        apply_gateway("gemini", "").unwrap();
        assert!(walk(home).is_empty());
        assert!(!data.join("config").join("agent_stash.json").exists());
    });
}

#[test]
fn apply_gateway_blank_ref_is_not_cancel() {
    with_sandbox("blank", |home, _data| {
        apply_gateway("fx", " ").unwrap();
        let text = fs::read_to_string(home.join(".fx/settings.json")).unwrap();
        assert!(text.contains("\"skillstar\": \" \""), "{text}");
        assert!(home.join(".fx/settings.json").exists());
    });
}

#[test]
fn apply_gateway_opencode_keeps_jsonc_path() {
    with_sandbox("jsonc", |home, _data| {
        let jsonc = home.join(".config/opencode/opencode.jsonc");
        fs::create_dir_all(jsonc.parent().unwrap()).unwrap();
        fs::write(&jsonc, "{\"keep\":true}\n").unwrap();
        apply_gateway("opencode", REF).unwrap();
        assert!(!home.join(".config/opencode/opencode.json").exists());
        assert_eq!(fs::read_to_string(&jsonc).unwrap(), fixture("opencode.json"));
        apply_gateway("opencode", "").unwrap();
        assert_eq!(fs::read_to_string(&jsonc).unwrap(), "{\"keep\":true}\n");
    });
}

#[test]
fn apply_gateway_omp_keeps_yaml_suffix() {
    with_sandbox("yaml", |home, _data| {
        let yaml = home.join(".omp/agent/models.yaml");
        fs::create_dir_all(yaml.parent().unwrap()).unwrap();
        fs::write(&yaml, "old: true\n").unwrap();
        apply_gateway("omp", REF).unwrap();
        assert!(!home.join(".omp/agent/models.yml").exists());
        assert_eq!(fs::read_to_string(&yaml).unwrap(), fixture("omp.models.yml"));
        assert_eq!(
            fs::read_to_string(home.join(".omp/agent/config.yml")).unwrap(),
            fixture("omp.config.yml")
        );
    });
}

struct SyncHome {
    previous: Option<OsString>,
}

impl SyncHome {
    fn clear() -> Self {
        let previous = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
        unsafe { std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME") };
        Self { previous }
    }
}

impl Drop for SyncHome {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
            }
        }
    }
}

#[test]
fn apply_gateway_grok_home_outside_sandbox() {
    let _gate = gate();
    let root = Tmp::new("grok-home");
    let home = root.path.join("home");
    let data = root.path.join("data");
    let grok = root.path.join("grok-dir");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&grok).unwrap();
    let addr = OsString::from(ADDR);
    let _sync = SyncHome::clear();
    let _env = EnvRestore::set(&[
        ("HOME", home.as_os_str()),
        ("USERPROFILE", home.as_os_str()),
        ("SKILLSTAR_DATA_DIR", data.as_os_str()),
        ("SKILLSTAR_GATEWAY_ADDR", addr.as_os_str()),
        ("GROK_HOME", grok.as_os_str()),
    ]);
    apply_gateway("grok", REF).unwrap();
    let text = fs::read_to_string(grok.join("config.toml")).unwrap();
    assert!(
        text.contains("base_url = \"http://127.0.0.1:21847/v1\""),
        "{text}"
    );
    assert!(!home.join(".grok").exists());
}
