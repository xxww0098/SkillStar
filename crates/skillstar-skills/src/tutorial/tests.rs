use super::*;
use crate::content::{SkillSnapshotFile, SnapshotFileKind};

fn snapshot(hash: &str, body: &[u8]) -> SkillSnapshot {
    SkillSnapshot {
        name: "demo".to_string(),
        root: PathBuf::from("/not-serialized"),
        content_hash: hash.to_string(),
        files: vec![SkillSnapshotFile {
            relative_path: "SKILL.md".to_string(),
            kind: SnapshotFileKind::Regular,
            content: body.to_vec(),
        }],
        total_bytes: body.len() as u64,
    }
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

#[test]
fn validates_offline_html_and_complete_file_coverage() {
    let html = valid_html("SKILL.md");
    assert!(validate_html(&html, &["SKILL.md".to_string()]).is_ok());
}

#[test]
fn rejects_active_content_external_resources_and_missing_coverage() {
    let active = valid_html("SKILL.md").replace("</body>", "<script>alert(1)</script></body>");
    assert!(validate_html(&active, &["SKILL.md".to_string()]).is_err());

    let external =
        valid_html("SKILL.md").replace("<body>", "<body><img src=\"https://example.com/x.png\">");
    assert!(validate_html(&external, &["SKILL.md".to_string()]).is_err());

    let unquoted_external =
        valid_html("SKILL.md").replace("<body>", "<body><img src=https://example.com/x.png>");
    assert!(validate_html(&unquoted_external, &["SKILL.md".to_string()]).is_err());

    let srcset = valid_html("SKILL.md").replace(
        "<body>",
        "<body><img srcset=\"https://example.com/x.png 1x\">",
    );
    assert!(validate_html(&srcset, &["SKILL.md".to_string()]).is_err());

    let encoded_refresh = valid_html("SKILL.md").replace(
        "<body>",
        "<body><meta http-equiv=\"ref&#x72;esh\" content=\"0;url=https://example.com\">",
    );
    assert!(validate_html(&encoded_refresh, &["SKILL.md".to_string()]).is_err());

    let delayed_csp = valid_html("SKILL.md").replace(
        &format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{REQUIRED_CSP}\"><style>"),
        &format!(
            "<meta data-x='&lt;meta http-equiv=\"Content-Security-Policy\" content=\"{REQUIRED_CSP}\"&gt;'><style>@\\69mport \"https://example.com/x.css\";</style><meta http-equiv=\"Content-Security-Policy\" content=\"{REQUIRED_CSP}\"><style>"
        ),
    );
    assert!(validate_html(&delayed_csp, &["SKILL.md".to_string()]).is_err());

    assert!(validate_html(&valid_html("SKILL.md"), &["scripts/run.sh".to_string()]).is_err());

    let comment_only = valid_html("SKILL.md")
        .replace(" data-skillstar-file=\"SKILL.md\"", "")
        .replace(
            "</body>",
            "<!-- <div data-skillstar-file=\"SKILL.md\"></div> --></body>",
        );
    assert!(validate_html(&comment_only, &["SKILL.md".to_string()]).is_err());
}

#[test]
fn requires_a_real_exact_csp_meta_and_a_closed_document_boundary() {
    let expected = ["SKILL.md".to_string()];
    let csp_as_text = valid_html("SKILL.md").replace(
        &format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{REQUIRED_CSP}\">"),
        &format!("<meta name=\"description\" content=\"guide\"><p>{REQUIRED_CSP}</p>"),
    );
    assert!(validate_html(&csp_as_text, &expected).is_err());

    let weakened = valid_html("SKILL.md").replace(REQUIRED_CSP, "default-src *");
    assert!(validate_html(&weakened, &expected).is_err());

    let csp_tag =
        format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{REQUIRED_CSP}\">");
    let csp_in_body = valid_html("SKILL.md")
        .replace(&csp_tag, "")
        .replace("<body>", &format!("<body>{csp_tag}"));
    assert!(validate_html(&csp_in_body, &expected).is_err());

    let trailing_markup = format!("{}<p>outside</p>", valid_html("SKILL.md"));
    assert!(validate_html(&trailing_markup, &expected).is_err());
}

#[test]
fn save_load_and_freshness_roundtrip_preserves_last_good_artifact() {
    let _guard = crate::lock_test_env();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    let temp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };

    let original = snapshot("sha256:one", b"one");
    let validated = validate_html(&valid_html("SKILL.md"), &["SKILL.md".to_string()]).unwrap();
    let saved = save(
        &original,
        "guided.v1",
        "artifact.v1",
        "guided",
        "Test Agent",
        validated,
    )
    .unwrap();
    assert_eq!(saved.state, TutorialState::Fresh);

    let fresh = load(&original, "guided.v1", "artifact.v1").unwrap();
    assert_eq!(fresh.state, TutorialState::Fresh);
    assert_eq!(fresh.metadata.unwrap().tutorial_style, "guided");

    let changed = snapshot("sha256:two", b"two");
    let stale = load(&changed, "guided.v1", "artifact.v1").unwrap();
    assert_eq!(stale.state, TutorialState::Stale);
    assert_eq!(
        stale.stale_reason,
        Some(TutorialStaleReason::ContentChanged)
    );
    assert!(stale.html.is_some());

    let generator_stale = load(&original, "guided.v2", "artifact.v1").unwrap();
    assert_eq!(generator_stale.state, TutorialState::Stale);
    assert_eq!(
        generator_stale.stale_reason,
        Some(TutorialStaleReason::GeneratorChanged)
    );

    let metadata_path = artifact_directory(&original.name).join("metadata.json");
    let mut tampered: TutorialMetadata =
        serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    tampered.source_files.clear();
    tampered.file_count = 0;
    std::fs::write(&metadata_path, serde_json::to_string(&tampered).unwrap()).unwrap();
    assert!(load(&original, "guided.v1", "artifact.v1").is_err());

    unsafe {
        match previous {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
    }
}

#[test]
fn artifact_key_never_uses_the_skill_name_as_a_path() {
    let key = artifact_key("../../demo skill");
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|character| character.is_ascii_hexdigit()));
}

#[test]
fn load_recovers_the_last_committed_directory_after_an_interrupted_swap() {
    let _guard = crate::lock_test_env();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    let temp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };

    let source = snapshot("sha256:one", b"one");
    let validated = validate_html(&valid_html("SKILL.md"), &["SKILL.md".to_string()]).unwrap();
    save(
        &source,
        "guided.v1",
        "artifact.v1",
        "guided",
        "Test Agent",
        validated,
    )
    .unwrap();

    let final_directory = artifact_directory(&source.name);
    let key = final_directory.file_name().unwrap().to_string_lossy();
    let backup = final_directory
        .parent()
        .unwrap()
        .join(format!(".{key}.crash-test.bak"));
    std::fs::rename(&final_directory, &backup).unwrap();

    let recovered = load(&source, "guided.v1", "artifact.v1").unwrap();
    assert_eq!(recovered.state, TutorialState::Fresh);
    assert!(final_directory.is_dir());
    assert!(!backup.exists());

    unsafe {
        match previous {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
    }
}
