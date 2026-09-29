//! Startup and provider save must not rewrite Agent config files.
//!
//! This is its own test binary so it does not share the process-wide
//! `SKILLSTAR_TOOL_SYNC_HOME` that the property tests install.

use skillstar_models::providers::{AgentBinding, BindingEntry, load_store_and_repair, save_store};
use skillstar_models::tool_sync::{
    resolve_claude_desktop_binding_path, resolve_omp_config_path, resolve_omp_models_path,
    resolve_opencode_config_path, resolve_pi_models_path, resolve_pi_settings_path,
    resolve_tool_config_path, resync_active_tools,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn startup_and_provider_save_do_not_touch_agent_files() {
    let root = std::env::temp_dir().join(format!(
        "skillstar-startup-agents-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let home = root.join("home");
    let data = root.join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    // SAFETY: this binary has one test. The vars are set before any path
    // resolution and are never read from another thread.
    unsafe {
        std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", &home);
        std::env::set_var("SKILLSTAR_DATA_DIR", &data);
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);
    }

    let codex = home.join(".codex").join("config.toml");
    let claude = home.join(".claude").join("settings.json");
    let desktop = resolve_claude_desktop_binding_path().unwrap();
    let opencode = resolve_opencode_config_path().unwrap();
    let pi_models = resolve_pi_models_path().unwrap();
    let pi_settings = resolve_pi_settings_path().unwrap();
    let omp_models = resolve_omp_models_path().unwrap();
    let omp_config = resolve_omp_config_path().unwrap();
    // The registry path must be the file an old repair would have rewritten.
    assert_eq!(
        resolve_tool_config_path("codex").unwrap(),
        codex,
        "the sentinel has to sit on the path unsync would rewrite"
    );

    let sentinels: Vec<(PathBuf, &str)> = vec![
        (
            codex,
            "model = \"gpt-5\"\nopenai_base_url = \"https://vendor.example/v1\"\n",
        ),
        (
            claude,
            "{\"env\":{\"ANTHROPIC_AUTH_TOKEN\":\"sk-user\",\"MY_CUSTOM\":\"keep\"}}\n",
        ),
        (desktop, "{\"provider_name\":\"keep-me\"}\n"),
        (opencode, "{\"model\":\"user/model\"}\n"),
        (
            pi_models,
            "{\"providers\":{\"mine\":{\"apiKey\":\"sk-user\"}}}\n",
        ),
        (pi_settings, "{\"defaultProvider\":\"mine\"}\n"),
        (omp_models, "providers:\n  mine:\n    apiKey: sk-user\n"),
        (omp_config, "modelRoles:\n  default: mine/model\n"),
    ];
    let before: Vec<Vec<u8>> = sentinels
        .iter()
        .map(|(path, body)| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
            fs::read(path).unwrap()
        })
        .collect();

    // An empty Codex activation is what the old repair turned into unsync,
    // which deletes `model` from config.toml. A desktop marker alone does not
    // catch that: migration already drops the desktop binding before repair.
    let store_path = data.join("model_providers.json");
    fs::write(
        &store_path,
        r#"{
  "version": 3,
  "providers": [{
    "id": "p1",
    "name": "Relay",
    "base_url_openai": "https://relay.example.com/v1",
    "base_url_anthropic": "https://relay.example.com/anthropic",
    "models_url": "",
    "api_key": "sk-live-key",
    "models": ["m1"],
    "default_model": "m1",
    "sort_index": 0,
    "codex_wire_api": "chat",
    "codex_auth_mode": "third_party"
  }],
  "tool_activations": {
    "codex": { "entries": [], "active_index": 0 },
    "claude-desktop": {
      "entries": [{ "provider_id": "p1", "model": "m1" }],
      "active_index": 0
    }
  }
}"#,
    )
    .unwrap();

    let loaded = load_store_and_repair(&store_path).expect("v3 store migrates");
    assert!(loaded.report.is_some(), "a v3 file must migrate");

    let mut store = loaded.store;
    for id in [
        "claude-code",
        "claude-desktop",
        "codex",
        "opencode",
        "pi",
        "omp",
    ] {
        store.bindings.insert(
            id.to_string(),
            AgentBinding::single(BindingEntry::new("p1", "m1")),
        );
    }
    let results = resync_active_tools(&store, "p1");
    assert_eq!(results.len(), 6, "{results:?}");
    assert!(results.iter().all(|result| result.success), "{results:?}");
    save_store(&store).expect("provider save writes the store only");

    for ((path, _), bytes) in sentinels.iter().zip(before) {
        assert_eq!(
            fs::read(path).unwrap(),
            bytes,
            "agent file changed: {}",
            path.display()
        );
    }
    let _ = fs::remove_dir_all(&root);
}
