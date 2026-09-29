//! Oh My Pi sync no longer writes YAML. Unsync still strips managed blocks.

use super::*;
use crate::providers::{Effort, ModelRef};

#[test]
fn omp_binding_sync_leaves_seeded_files() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");
    let models =
        "providers:\n  ollama:\n    apiKey: ollama\n  skillstar_dead0000:\n    apiKey: sk-dead\n";
    let config = "theme:\n  light: light\nmodelRoles:\n  slow: aiproxy/deepseek-v4-flash:xhigh\n";
    std::fs::write(&models_path, models).unwrap();
    std::fs::write(&config_path, config).unwrap();

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    let binding = AgentBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        roles: Default::default(),
        active_index: 1,
        settings: None,
    };

    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    assert_eq!(std::fs::read_to_string(&models_path).unwrap(), models);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), config);
}

#[test]
fn omp_binding_sync_does_not_create_files() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");
    let providers = vec![flat("aaaa1111", "alpha")];
    let binding = AgentBinding::single(entry("aaaa1111", "model-a"));

    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    assert!(!models_path.exists());
    assert!(!config_path.exists());
}

#[test]
fn omp_unsync_removes_managed_blocks_and_managed_pointer_only() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");
    std::fs::write(
        &models_path,
        "providers:\n  skillstar_aaaa1111:\n    apiKey: sk-aaaa1111\n    baseUrl: https://alpha.example.com/v1\n  mine:\n    baseUrl: https://mine\n",
    )
    .unwrap();
    std::fs::write(
        &config_path,
        "modelRoles:\n  default: skillstar_aaaa1111/model-a\n  slow: aiproxy/deepseek-v4-flash:xhigh\n",
    )
    .unwrap();

    unsync_omp_all_at(&models_path, &config_path).unwrap();

    let after: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&models_path).unwrap()).unwrap();
    let provider_map = after.get("providers").unwrap().as_mapping().unwrap();
    assert!(provider_map.contains_key(serde_yaml::Value::String("mine".into())));
    assert!(
        !provider_map
            .keys()
            .any(|k| k.as_str().is_some_and(is_skillstar_managed_key))
    );

    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let roles = config.get("modelRoles").unwrap().as_mapping().unwrap();
    assert!(!roles.contains_key(serde_yaml::Value::String("default".into())));
    assert_eq!(
        roles
            .get(serde_yaml::Value::String("slow".into()))
            .unwrap()
            .as_str()
            .unwrap(),
        "aiproxy/deepseek-v4-flash:xhigh"
    );
}

#[test]
fn omp_unsync_removes_every_managed_role() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");
    std::fs::write(
        &config_path,
        "modelRoles:\n  default: skillstar_aaaa1111/model-a\n  smol: skillstar_aaaa1111/model-b\n  slow: skillstar_aaaa1111/model-b:xhigh\n  plan: aiproxy/gpt-5.6-sol:max\n",
    )
    .unwrap();

    unsync_omp_all_at(&models_path, &config_path).unwrap();

    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let roles = config.get("modelRoles").unwrap().as_mapping().unwrap();
    for role in ["default", "smol", "slow"] {
        assert!(
            !roles.contains_key(serde_yaml::Value::String(role.into())),
            "managed role {role} must be cleared"
        );
    }
    assert_eq!(
        roles
            .get(serde_yaml::Value::String("plan".into()))
            .unwrap()
            .as_str()
            .unwrap(),
        "aiproxy/gpt-5.6-sol:max"
    );
}

#[test]
fn an_unknown_thinking_level_never_reaches_the_role_value() {
    assert_eq!(Effort::from_omp_thinking("turbo"), None);

    let target = ModelRef {
        provider_id: "aaaa1111".to_string(),
        model: "model-a".to_string(),
        effort: Effort::from_omp_thinking("turbo"),
        ext: None,
    };
    assert_eq!(
        omp_role_value(&target, "skillstar_aaaa1111").as_deref(),
        Some("skillstar_aaaa1111/model-a")
    );

    let valid = ModelRef {
        effort: Effort::from_omp_thinking("xhigh"),
        ..target.clone()
    };
    assert_eq!(
        omp_role_value(&valid, "skillstar_aaaa1111").as_deref(),
        Some("skillstar_aaaa1111/model-a:xhigh")
    );

    let modelless = ModelRef {
        model: String::new(),
        ..target
    };
    assert_eq!(
        omp_role_value(&modelless, "skillstar_aaaa1111"),
        None,
        "an incomplete role must not overwrite what the user has on disk"
    );
}

#[test]
fn omp_role_names_are_validated() {
    for good in ["default", "smol", "slow", "plan", "my-role", "my_role2"] {
        assert!(is_valid_omp_role_name(good), "{good} should be valid");
    }
    for bad in ["", "@smol", "a/b", "with space", "emoji🙂"] {
        assert!(!is_valid_omp_role_name(bad), "{bad} should be rejected");
    }
}

#[test]
fn omp_role_and_thinking_registries_match_the_frontend() {
    assert_eq!(
        OMP_MODEL_ROLES,
        [
            "default", "smol", "slow", "plan", "vision", "designer", "commit", "tiny", "task",
            "advisor"
        ]
    );
    assert_eq!(
        OMP_THINKING_LEVELS,
        [
            "inherit", "off", "minimal", "low", "medium", "high", "xhigh", "max", "auto"
        ]
    );
    for role in OMP_MODEL_ROLES {
        assert!(
            is_valid_omp_role_name(role),
            "built-in role {role} must be writable"
        );
    }
}

#[test]
fn omp_unsync_leaves_user_owned_default_pointer_alone() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");
    std::fs::write(
        &config_path,
        "modelRoles:\n  default: opencode-go/deepseek-v4-flash:xhigh\n",
    )
    .unwrap();

    unsync_omp_all_at(&models_path, &config_path).unwrap();

    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        config
            .get("modelRoles")
            .unwrap()
            .get("default")
            .unwrap()
            .as_str()
            .unwrap(),
        "opencode-go/deepseek-v4-flash:xhigh"
    );
}
