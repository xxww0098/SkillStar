use super::*;
use crate::identity::{
    ContentRevision, GitTrackingRef, ResolvedSkill, SkillIdentity, SkillRevision,
};
use crate::tutorial::store::legacy_artifact_key;
use crate::tutorial::validator::{REQUIRED_CSP, escape_html_attribute};

fn sha(nibble: char) -> String {
    format!("sha256:{}", nibble.to_string().repeat(64))
}

fn commit(nibble: char) -> String {
    nibble.to_string().repeat(40)
}

fn resolved(hash: &str, name: &str) -> ResolvedSkill {
    let identity = SkillIdentity::local(uuid::Uuid::from_u128(42)).unwrap();
    let revision = SkillRevision::local(&identity, ContentRevision::new(2, hash).unwrap()).unwrap();
    ResolvedSkill::new(identity, revision, name, Some(name.to_string())).unwrap()
}

fn valid_html(path: &str) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta http-equiv="Content-Security-Policy" content="{REQUIRED_CSP}"><style>body{{color:#111}}</style></head>
<body><svg role="img" aria-label="flow" viewBox="0 0 10 10"><title>Flow</title><path d="M0 0L10 10"/></svg>
<table><tr data-skillstar-file="{}"><td>{}</td><td>core</td><td>overview</td><td>read</td></tr></table></body></html>"#,
        escape_html_attribute(path),
        path
    )
}

struct DataDir {
    _guard: std::sync::MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
    _temp: tempfile::TempDir,
}

impl DataDir {
    fn new() -> Self {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
        Self {
            _guard,
            previous,
            _temp: temp,
        }
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
    }
}

#[test]
fn validates_offline_html_and_complete_file_coverage() {
    let html = valid_html("SKILL.md");
    assert!(validate_html(&html, &["SKILL.md".to_string()]).is_ok());
}

#[test]
fn rejects_active_content_external_resources_and_missing_coverage() {
    let expected = ["SKILL.md".to_string()];
    let active = valid_html("SKILL.md").replace("</body>", "<script>alert(1)</script></body>");
    assert!(validate_html(&active, &expected).is_err());
    let external =
        valid_html("SKILL.md").replace("<body>", "<body><img src=\"https://example.com/x.png\">");
    assert!(validate_html(&external, &expected).is_err());
    assert!(validate_html(&valid_html("SKILL.md"), &["scripts/run.sh".to_string()]).is_err());
}

#[test]
fn save_load_and_freshness_roundtrip_preserves_last_good_artifact() {
    let _dir = DataDir::new();
    let original = resolved(&sha('a'), "demo");
    let inventory = vec!["SKILL.md".to_string()];
    let generator = GeneratorFingerprint::new("guided.v1", "artifact.v1");
    let saved = commit_private_tutorial(
        &original,
        &inventory,
        3,
        &generator,
        "guided",
        "Test Agent",
        &valid_html("SKILL.md"),
    )
    .unwrap();
    assert_eq!(saved.state, TutorialState::Fresh);
    assert!(saved.bound);

    let fresh = load_private_tutorial(&original, &inventory, 3, &generator).unwrap();
    assert_eq!(fresh.state, TutorialState::Fresh);

    let changed = resolved(&sha('b'), "demo");
    let stale = load_private_tutorial(&changed, &inventory, 3, &generator).unwrap();
    assert_eq!(stale.state, TutorialState::Stale);
    assert_eq!(
        stale.stale_reason,
        Some(TutorialStaleReason::ContentChanged)
    );
    assert!(stale.html.is_some());

    let generator_stale = load_private_tutorial(
        &original,
        &inventory,
        3,
        &GeneratorFingerprint::new("guided.v2", "artifact.v1"),
    )
    .unwrap();
    assert_eq!(generator_stale.state, TutorialState::Stale);
    assert_eq!(
        generator_stale.stale_reason,
        Some(TutorialStaleReason::GeneratorChanged)
    );
}

#[test]
fn dual_read_uses_legacy_name_path_but_new_writes_only_identity_path() {
    let _dir = DataDir::new();
    let skill = resolved(&sha('a'), "demo");
    let inventory = vec!["SKILL.md".to_string()];
    let generator = GeneratorFingerprint::new("guided.v1", "artifact.v1");
    let legacy_dir =
        skillstar_core::infra::paths::tutorials_dir().join(legacy_artifact_key("demo"));
    std::fs::create_dir_all(&legacy_dir).unwrap();
    std::fs::write(legacy_dir.join("tutorial.html"), valid_html("SKILL.md")).unwrap();
    std::fs::write(
        legacy_dir.join("metadata.json"),
        serde_json::json!({
            "skillName": "demo",
            "contentHash": sha('a'),
            "promptVersion": "guided.v1",
            "schemaVersion": "artifact.v1",
            "tutorialStyle": "guided",
            "agentLabel": "legacy",
            "generatedAt": "2026-01-01T00:00:00Z",
            "fileCount": 1,
            "totalBytes": 3,
            "sourceFiles": ["SKILL.md"]
        })
        .to_string(),
    )
    .unwrap();

    let loaded = load_private_tutorial(&skill, &inventory, 3, &generator).unwrap();
    assert_eq!(loaded.state, TutorialState::Fresh);
    assert!(!loaded.bound);
    assert!(create_guide_draft_from_tutorial(&loaded).is_err());

    commit_private_tutorial(
        &skill,
        &inventory,
        3,
        &generator,
        "guided",
        "Test Agent",
        &valid_html("SKILL.md"),
    )
    .unwrap();
    let bound = load_private_tutorial(&skill, &inventory, 3, &generator).unwrap();
    assert!(bound.bound);
    assert!(legacy_dir.join("tutorial.html").is_file());
}

#[test]
fn load_recovers_the_last_committed_directory_after_an_interrupted_swap() {
    let _dir = DataDir::new();
    let skill = resolved(&sha('a'), "demo");
    let inventory = vec!["SKILL.md".to_string()];
    let generator = GeneratorFingerprint::new("guided.v1", "artifact.v1");
    commit_private_tutorial(
        &skill,
        &inventory,
        3,
        &generator,
        "guided",
        "Test Agent",
        &valid_html("SKILL.md"),
    )
    .unwrap();

    let final_directory = skillstar_core::infra::paths::learning_tutorials_dir()
        .join(skill.identity.key.storage_segment());
    let key = final_directory.file_name().unwrap().to_string_lossy();
    let backup = final_directory
        .parent()
        .unwrap()
        .join(format!(".{key}.crash-test.bak"));
    std::fs::rename(&final_directory, &backup).unwrap();

    let recovered = load_private_tutorial(&skill, &inventory, 3, &generator).unwrap();
    assert_eq!(recovered.state, TutorialState::Fresh);
    assert!(final_directory.is_dir());
    assert!(!backup.exists());
}

#[test]
fn artifact_key_never_uses_the_skill_name_as_a_path() {
    let key = legacy_artifact_key("../../demo skill");
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|character| character.is_ascii_hexdigit()));
}

#[test]
fn git_revision_uses_content_hash_not_name() {
    let identity = SkillIdentity::git(
        "https://github.com/owner/repo.git",
        GitTrackingRef::DefaultBranch,
        "skills/demo",
    )
    .unwrap();
    let revision = SkillRevision::git(
        &identity,
        commit('1'),
        commit('2'),
        ContentRevision::new(2, sha('a')).unwrap(),
    )
    .unwrap();
    assert_eq!(revision.skill_key, identity.key);
    assert!(revision.key.as_str().starts_with("skr:v1:"));
}
