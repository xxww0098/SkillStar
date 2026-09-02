use super::*;
use crate::test_support::{ENV_LOCK, EnvGuard};
use skillstar_learning::{SkillIdentitySource, TutorialState};
use skillstar_skills::local_skill;

#[tokio::test]
async fn local_skill_resolves_to_uuid_identity_not_name() {
    let _lock = ENV_LOCK.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", &temp.path().join("data")),
        ("SKILLSTAR_HUB_DIR", &temp.path().join("hub")),
        ("HOME", &temp.path().join("home")),
        ("USERPROFILE", &temp.path().join("home")),
    ]);

    local_skill::create("demo", Some("---\ndescription: demo skill\n---\n# Demo\n")).unwrap();
    let skill = Skill {
        name: "demo".to_string(),
        description: "demo skill".to_string(),
        localized_description: None,
        skill_type: skillstar_core::types::SkillType::Local,
        stars: 0,
        installed: true,
        update_available: false,
        upstream_change: None,
        last_updated: String::new(),
        git_url: String::new(),
        tree_hash: None,
        category: skillstar_core::types::SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: None,
        rank: None,
        source: None,
    };
    let resolved = resolve_skill(&skill).unwrap();
    match resolved.identity.source {
        SkillIdentitySource::Local { local_id } => assert!(!local_id.is_nil()),
        other => panic!("expected local identity, got {other:?}"),
    }
    assert_eq!(resolved.installed_name.as_deref(), Some("demo"));
    assert_ne!(resolved.identity.key.as_str(), "demo");

    let missing = resolve_installed_name("not-installed");
    assert!(missing.is_err(), "{missing:?}");
}

#[tokio::test]
async fn private_tutorial_roundtrip_for_local_skill() {
    let _lock = ENV_LOCK.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let _guard = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", &temp.path().join("data")),
        ("SKILLSTAR_HUB_DIR", &temp.path().join("hub")),
        ("HOME", &temp.path().join("home")),
        ("USERPROFILE", &temp.path().join("home")),
    ]);

    local_skill::create(
        "writer",
        Some("---\ndescription: writes things\n---\n# Writer\n"),
    )
    .unwrap();
    let generator = GeneratorFingerprint::new("guided.v1", "artifact.v1");
    let loaded = load_tutorial("writer", &generator).unwrap();
    assert_eq!(loaded.state, TutorialState::Missing);
}
