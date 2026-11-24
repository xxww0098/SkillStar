use super::paths::{BuiltInTargetSpec, McpPathRoots};
use crate::model::McpFormat;
use std::path::PathBuf;

pub(super) fn append_targets_18_to_35(roots: &McpPathRoots, specs: &mut Vec<BuiltInTargetSpec>) {
    // 18. alma
    let alma_path = roots
        .home_dir
        .join(".config")
        .join("alma")
        .join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "alma".into(),
        name: "Alma".into(),
        installed: roots.detect_dir_or_file(&alma_path, Some("alma"), Some("Alma.app")),
        primary_path: alma_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Alma,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 20. hermes
    let hermes_dir = roots
        .env_override("HERMES_HOME")
        .unwrap_or_else(|| roots.home_dir.join(".hermes"));
    let hermes_path = hermes_dir.join("config.yaml");
    specs.push(BuiltInTargetSpec {
        id: "hermes".into(),
        name: "Hermes Agent".into(),
        installed: roots.detect_dir_or_file(&hermes_path, Some("hermes"), None),
        primary_path: hermes_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Hermes,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: true,
        supports_startup_timeout: false,
        notes: None,
    });

    // 21. dsh
    let dsh_dir = roots
        .env_override("DSH_HOME")
        .unwrap_or_else(|| roots.home_dir.join(".dsh"));
    let dsh_primary = dsh_dir.join("config.yaml");
    let mut dsh_also = Vec::new();
    let profiles_dir = dsh_dir.join("profiles");
    if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
        let mut patches: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("cordis.patch.yml"))
            .filter(|p| p.exists())
            .collect();
        patches.sort();
        dsh_also.extend(patches);
    }
    specs.push(BuiltInTargetSpec {
        id: "dsh".into(),
        name: "DeepSeek Harness".into(),
        installed: roots.detect_dir_or_file(&dsh_primary, Some("dsh"), None)
            || !dsh_also.is_empty(),
        primary_path: dsh_primary,
        also_paths: dsh_also,
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Dsh,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 22. devin
    let devin_path = if cfg!(windows) {
        roots
            .win_app_data
            .join("devin")
            .join("mcp_config.json")
    } else {
        roots.xdg_config_home.join("devin").join("mcp_config.json")
    };
    specs.push(BuiltInTargetSpec {
        id: "devin".into(),
        name: "Devin".into(),
        installed: roots.detect_dir_or_file(&devin_path, Some("devin"), Some("Devin.app")),
        primary_path: devin_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Devin,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 23. grok
    let grok_dir = roots
        .env_override("GROK_HOME")
        .unwrap_or_else(|| roots.home_dir.join(".grok"));
    let grok_path = grok_dir.join("config.toml");
    specs.push(BuiltInTargetSpec {
        id: "grok".into(),
        name: "Grok Build".into(),
        installed: roots.detect_dir_or_file(&grok_path, Some("grok"), None),
        primary_path: grok_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Grok,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 24. grok-app
    let grok_app_dir = roots
        .env_override("GROK_APP_HOME")
        .map(|d| d.join("agent-home"))
        .unwrap_or_else(|| {
            if cfg!(windows) {
                roots
                    .win_local_app_data
                    .join("grok-app")
                    .join("agent-home")
            } else if cfg!(target_os = "macos") {
                roots
                    .mac_app_support
                    .join("com.grokapp.grok-app")
                    .join("agent-home")
            } else {
                roots
                    .xdg_config_home
                    .join("grok-app")
                    .join("agent-home")
            }
        });
    let grok_app_path = grok_app_dir.join("config.toml");
    specs.push(BuiltInTargetSpec {
        id: "grok-app".into(),
        name: "Grok App".into(),
        installed: roots.detect_dir_or_file(&grok_app_path, None, Some("Grok.app")),
        primary_path: grok_app_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Grok,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 25. droid
    let droid_dir = roots
        .env_override("FACTORY_HOME_OVERRIDE")
        .unwrap_or_else(|| roots.home_dir.join(".factory"));
    let droid_path = droid_dir.join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "droid".into(),
        name: "Droid".into(),
        installed: roots.detect_dir_or_file(&droid_path, Some("droid"), None),
        primary_path: droid_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Claude,
        no_cmd_refs: true,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 26. qoder
    let qoder_dir = roots
        .env_override("QODER_CONFIG_DIR")
        .unwrap_or_else(|| roots.home_dir.join(".qoder"));
    let qoder_path = qoder_dir.join("settings.json");
    specs.push(BuiltInTargetSpec {
        id: "qoder".into(),
        name: "Qoder".into(),
        installed: roots.detect_dir_or_file(&qoder_path, Some("qoder"), Some("Qoder.app")),
        primary_path: qoder_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Omp,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 27. qoder-cn
    let qoder_cn_dir = roots
        .env_override("QODERCN_CONFIG_DIR")
        .unwrap_or_else(|| roots.home_dir.join(".qoder-cn"));
    let qoder_cn_path = qoder_cn_dir.join("settings.json");
    specs.push(BuiltInTargetSpec {
        id: "qoder-cn".into(),
        name: "Qoder CN".into(),
        installed: roots.detect_dir_or_file(&qoder_cn_path, Some("qoder-cn"), None),
        primary_path: qoder_cn_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Omp,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 28. cline
    let cline_path = roots
        .env_override("CLINE_MCP_SETTINGS_PATH")
        .unwrap_or_else(|| {
            let data_dir = roots.env_override("CLINE_DATA_DIR").unwrap_or_else(|| {
                roots
                    .env_override("CLINE_DIR")
                    .unwrap_or_else(|| roots.home_dir.join(".cline"))
                    .join("data")
            });
            data_dir.join("settings").join("cline_mcp_settings.json")
        });
    specs.push(BuiltInTargetSpec {
        id: "cline".into(),
        name: "Cline".into(),
        installed: roots.detect_dir_or_file(&cline_path, Some("cline"), None),
        primary_path: cline_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Cline,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 29. commandcode
    let commandcode_path = roots.home_dir.join(".commandcode").join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "commandcode".into(),
        name: "Command Code".into(),
        installed: roots.detect_dir_or_file(&commandcode_path, Some("commandcode"), None),
        primary_path: commandcode_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::CommandCode,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 30. workbuddy
    let workbuddy_path = roots
        .env_override("WORKBUDDY_CONFIG_DIR")
        .unwrap_or_else(|| roots.home_dir.join(".workbuddy"))
        .join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "workbuddy".into(),
        name: "WorkBuddy".into(),
        installed: roots.detect_dir_or_file(&workbuddy_path, Some("workbuddy"), None),
        primary_path: workbuddy_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Claude,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 31. workbuddy-ai
    let workbuddy_ai_path = roots.home_dir.join(".workbuddy-ai").join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "workbuddy-ai".into(),
        name: "WorkBuddy AI".into(),
        installed: roots.detect_dir_or_file(&workbuddy_ai_path, None, None),
        primary_path: workbuddy_ai_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Claude,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 32. codebuddy
    let codebuddy_dir = roots
        .env_override("CODEBUDDY_CONFIG_DIR")
        .unwrap_or_else(|| roots.home_dir.join(".codebuddy"));
    let cb_candidates = [
        codebuddy_dir.join(".mcp.json"),
        codebuddy_dir.join("mcp.json"),
        roots.home_dir.join(".codebuddy.json"),
    ];
    let codebuddy_path = cb_candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| cb_candidates[0].clone());
    specs.push(BuiltInTargetSpec {
        id: "codebuddy".into(),
        name: "CodeBuddy".into(),
        installed: roots.detect_dir_or_file(&codebuddy_path, Some("codebuddy"), None),
        primary_path: codebuddy_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Claude,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 33. atomcode
    let atomcode_path = roots
        .env_override("ATOMCODE_HOME")
        .unwrap_or_else(|| roots.home_dir.join(".atomcode"))
        .join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "atomcode".into(),
        name: "AtomCode".into(),
        installed: roots.detect_dir_or_file(&atomcode_path, Some("atomcode"), None),
        primary_path: atomcode_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Omp,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 34. zed
    let zed_path = if cfg!(windows) {
        roots.win_app_data.join("Zed").join("settings.json")
    } else {
        roots
            .env_override("SKILLSTAR_ZED_CONFIG_DIR")
            .unwrap_or_else(|| roots.xdg_config_home.join("zed"))
            .join("settings.json")
    };
    specs.push(BuiltInTargetSpec {
        id: "zed".into(),
        name: "Zed".into(),
        installed: roots.detect_dir_or_file(&zed_path, Some("zed"), Some("Zed.app")),
        primary_path: zed_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Zed,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 35. kiro-cli (require explicit kiro-cli file/binary so generic IDE ~/.kiro isn't misdetected)
    let kiro_path = roots
        .home_dir
        .join(".kiro")
        .join("settings")
        .join("mcp.json");
    let kiro_detected = kiro_path.exists()
        || (!roots.sandboxed && which::which("kiro-cli").is_ok());
    specs.push(BuiltInTargetSpec {
        id: "kiro-cli".into(),
        name: "Kiro CLI".into(),
        installed: kiro_detected,
        primary_path: kiro_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Kiro,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 36. liveagent
    let liveagent_dir = roots
        .env_override("LIVEAGENT_HOME")
        .or_else(|| roots.env_override("K_BRAIN_HOME"))
        .unwrap_or_else(|| roots.home_dir.join(".liveagent"));
    let liveagent_path = liveagent_dir.join("config.json.live-mcp.json");
    let liveagent_seed = liveagent_dir.join("config.json");
    specs.push(BuiltInTargetSpec {
        id: "liveagent".into(),
        name: "LiveAgent".into(),
        installed: liveagent_path.exists()
            || liveagent_seed.exists()
            || liveagent_dir.exists()
            || (!roots.sandboxed && which::which("liveagent").is_ok()),
        primary_path: liveagent_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: Some(liveagent_seed),
        format: McpFormat::LiveAgent,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });
}
