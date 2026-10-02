//! WSL Codex path and URL spelling. `wsl.exe` is the closure these tests pass.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use skillstar_gateway::{
    CodexRoute, WslCodex, apply_wsl_codex, wsl_codex_id, wsl_codex_list, wsl_codex_open_path,
};

const PORT_ADDR: &str = "127.0.0.1:21847";
const PROBE: &str = "home:/home/me\ndir:.codex\nroute:default via 172.20.0.1 dev eth0\n";
const MIRRORED: &str = "[wsl2]\nnetworkingMode=mirrored\n";
const NAT: &str = "[wsl2]\nnetworkingMode=nat\n";

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
            "skillstar-wsl-{label}-{}-{nanos}",
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
            ("HOME", home.as_os_str().to_os_string()),
            ("USERPROFILE", home.as_os_str().to_os_string()),
            ("SKILLSTAR_TOOL_SYNC_HOME", home.as_os_str().to_os_string()),
            ("SKILLSTAR_DATA_DIR", data.as_os_str().to_os_string()),
            ("SKILLSTAR_GATEWAY_ADDR", OsString::from(PORT_ADDR)),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, &value) };
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
    let home = root.path.join("sync-home");
    let data = root.path.join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    let _env = EnvRestore::sandbox(&home, &data);
    body(&root.path);
}

struct Listed {
    distros: Vec<WslCodex>,
    calls: Vec<Vec<String>>,
}

fn list_with(installed: Vec<u8>, running: Vec<u8>, probes: HashMap<String, String>, cfg: &str) -> Listed {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&calls);
    let distros = wsl_codex_list(
        &move |args: &[&str]| {
            log.lock()
                .expect("call log")
                .push(args.iter().map(|arg| (*arg).to_string()).collect());
            match args {
                ["-l", "-q"] => Ok(installed.clone()),
                ["-l", "--running", "-q"] => Ok(running.clone()),
                ["-d", name, ..] => probes.get(*name).map(|body| body.as_bytes().to_vec()).ok_or_else(|| {
                    io::Error::other(format!("wsl.exe was asked about {name}"))
                }),
                other => Err(io::Error::other(format!(
                    "unexpected wsl.exe {}",
                    other.join(" ")
                ))),
            }
        },
        cfg,
    );
    let calls = calls.lock().expect("call log").clone();
    Listed { distros, calls }
}

fn ubuntu(cfg: &str) -> Listed {
    let mut probes = HashMap::new();
    probes.insert("Ubuntu-24.04".to_string(), PROBE.to_string());
    list_with(
        b"Ubuntu-24.04\n".to_vec(),
        b"Ubuntu-24.04\n".to_vec(),
        probes,
        cfg,
    )
}

fn utf16_bom(text: &str) -> Vec<u8> {
    let mut out = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn write_both(distro: &WslCodex, root: &Path) -> (String, String) {
    let logged = root.join("logged");
    let api = root.join("api");
    for home in [&logged, &api] {
        let path = home.join(".codex").join("config.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "model = \"gpt-5\"\n").unwrap();
    }
    apply_wsl_codex(distro, CodexRoute::LoggedIn, &logged).unwrap();
    apply_wsl_codex(distro, CodexRoute::Api, &api).unwrap();
    (
        fs::read_to_string(logged.join(".codex").join("config.toml")).unwrap(),
        fs::read_to_string(api.join(".codex").join("config.toml")).unwrap(),
    )
}

fn assert_urls(logged: &str, api: &str, host: &str, bearer: &str) {
    let origin = format!("http://{host}:21847");
    assert!(
        logged.contains(&format!("openai_base_url = \"{origin}/backend-api/codex\"\n")),
        "{logged}"
    );
    assert!(
        api.contains(&format!("base_url = \"{origin}/v1\"\n")),
        "{api}"
    );
    assert!(api.contains("[model_providers.skillstar]\n"), "{api}");
    assert!(api.contains("name = \"skillstar\"\n"), "{api}");
    assert!(api.contains("wire_api = \"responses\"\n"), "{api}");
    assert!(
        api.contains(&format!("experimental_bearer_token = \"{bearer}\"\n")),
        "{api}"
    );
    assert!(
        api.contains("model_catalog_json = \"/home/me/.codex/skillstar-models.json\"\n"),
        "{api}"
    );
    for banned in ["magpie", "api.openai.com", "api.anthropic.com", ":3425"] {
        assert!(!logged.contains(banned), "{logged}");
        assert!(!api.contains(banned), "{api}");
    }
}

#[test]
fn wsl_codex_id_spelling() {
    assert_eq!(wsl_codex_id("Ubuntu-24.04"), "codex@wsl:Ubuntu-24.04");
    assert_eq!(
        wsl_codex_open_path("Ubuntu-24.04", "/home/me/.codex/config.toml"),
        "\\\\wsl.localhost\\Ubuntu-24.04\\home\\me\\.codex\\config.toml"
    );
    let listed = ubuntu(MIRRORED);
    assert_eq!(listed.distros.len(), 1);
    assert_eq!(listed.distros[0].id(), "codex@wsl:Ubuntu-24.04");
    assert_eq!(listed.distros[0].home, "/home/me");
}

#[test]
fn wsl_codex_mirrored_url_is_loopback() {
    with_sandbox("mirrored", |root| {
        let stash = root.join("data").join("config").join("agent_stash.json");
        fs::create_dir_all(stash.parent().unwrap()).unwrap();
        fs::write(
            &stash,
            "{\n  \"codex.openai_base_url\": \"https://vendor.example/v1\"\n}\n",
        )
        .unwrap();
        let listed = ubuntu(MIRRORED);
        let distro = &listed.distros[0];
        assert!(distro.mirrored);
        assert_eq!(distro.gateway, "172.20.0.1");
        let (logged, api) = write_both(distro, root);
        assert_urls(&logged, &api, "127.0.0.1", "skillstar");
        assert!(!logged.contains("172.20.0.1"), "{logged}");
        assert!(!api.contains("172.20.0.1"), "{api}");
        // A loopback origin never touches the key file: mirrored keeps the
        // placeholder bearer.
        assert!(
            !root.join("data").join("config").join("gateway.key").exists(),
            "a loopback write must not generate a gateway key"
        );
        let saved = fs::read_to_string(&stash).unwrap();
        assert!(
            saved.contains("\"codex.openai_base_url\": \"https://vendor.example/v1\""),
            "{saved}"
        );
        assert!(
            saved.contains("\"codex@wsl:Ubuntu-24.04.openai_base_url\""),
            "{saved}"
        );
    });
}

#[test]
fn wsl_codex_nat_url_uses_windows_host() {
    with_sandbox("nat", |root| {
        let listed = ubuntu(NAT);
        let distro = &listed.distros[0];
        assert!(!distro.mirrored);
        let (logged, api) = write_both(distro, root);
        // A NAT form's peer is not loopback: the bearer must be the real
        // gateway key generated in the sandbox; the placeholder would not
        // pass the LAN gate.
        let key = fs::read_to_string(root.join("data").join("config").join("gateway.key"))
            .unwrap()
            .trim()
            .to_string();
        assert!(key.len() >= 64, "the gateway key is hex of >=32 bytes: {key}");
        assert_urls(&logged, &api, "172.20.0.1", &key);
        assert!(
            !api.contains("experimental_bearer_token = \"skillstar\"\n"),
            "{api}"
        );
        assert!(!logged.contains("127.0.0.1"), "{logged}");
        assert!(!api.contains("127.0.0.1"), "{api}");
    });
}

#[test]
fn wsl_codex_does_not_start_stopped_distro() {
    with_sandbox("stopped", |root| {
        let mut probes = HashMap::new();
        probes.insert("Ubuntu-24.04".to_string(), PROBE.to_string());
        let listed = list_with(
            utf16_bom("Ubuntu-24.04\r\ndocker-desktop\r\nStopped\r\n"),
            utf16_bom("Ubuntu-24.04\r\n"),
            probes,
            NAT,
        );
        let flat: Vec<&str> = listed
            .calls
            .iter()
            .flat_map(|args| args.iter().map(String::as_str))
            .collect();
        assert!(
            !flat.contains(&"Stopped"),
            "stopped distro was asked: {:?}",
            listed.calls
        );
        assert!(
            !flat.contains(&"docker-desktop"),
            "docker distro was asked: {:?}",
            listed.calls
        );
        assert!(
            listed.calls.iter().all(|args| {
                args.windows(2)
                    .all(|pair| !(pair[0] == "-e" && pair[1] == "true"))
            }),
            "a distro was started: {:?}",
            listed.calls
        );
        assert_eq!(listed.distros.len(), 1);
        assert_eq!(listed.distros[0].name, "Ubuntu-24.04");

        let stopped = WslCodex {
            name: "Stopped".to_string(),
            home: "/home/s".to_string(),
            gateway: "172.20.0.1".to_string(),
            mirrored: false,
            running: false,
            has_codex: true,
        };
        let home = root.join("stopped-home");
        let config = home.join(".codex").join("config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "model = \"on-disk\"\n").unwrap();
        let before = fs::read(&config).unwrap();
        let modified = fs::metadata(&config).unwrap().modified().unwrap();
        apply_wsl_codex(&stopped, CodexRoute::LoggedIn, &home).unwrap();
        apply_wsl_codex(&stopped, CodexRoute::Api, &home).unwrap();
        assert_eq!(fs::read(&config).unwrap(), before);
        assert_eq!(fs::metadata(&config).unwrap().modified().unwrap(), modified);
    });
}
