use super::*;
use super::seed::{SEED_CONTENT_ROOT, SEED_REPOSITORY};
use crate::identity::{ContentRevision, ResolvedSkill, SkillIdentity, SkillRevision};
use crate::tutorial::{GeneratorFingerprint, TutorialState, commit_private_tutorial, validate_html};

const REQUIRED_CSP: &str =
    "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:";

fn sha(nibble: char) -> String {
    format!("sha256:{}", nibble.to_string().repeat(64))
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

fn resolved() -> ResolvedSkill {
    let identity = SkillIdentity::local(uuid::Uuid::from_u128(7)).unwrap();
    let revision =
        SkillRevision::local(&identity, ContentRevision::new(2, sha('a')).unwrap()).unwrap();
    ResolvedSkill::new(identity, revision, "demo", Some("demo".into())).unwrap()
}

fn tutorial_html(title: &str, extra: &str) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta http-equiv="Content-Security-Policy" content="{REQUIRED_CSP}"><style>body{{color:#111}}</style></head>
<body><svg role="img" aria-label="flow" viewBox="0 0 10 10"><title>Flow</title><path d="M0 0L10 10"/></svg>
<h1>{title}</h1>
<p>Intro for {title}.</p>
<ul><li>First point</li><li>Second point</li></ul>
{extra}
<table><tr data-skillstar-file="SKILL.md"><td>SKILL.md</td><td>core</td><td>overview</td><td>read</td></tr></table>
</body></html>"#,
    )
}

fn commit_style(title: &str, extra: &str, style: &str) -> crate::tutorial::PrivateTutorial {
    let html = tutorial_html(title, extra);
    validate_html(&html, &["SKILL.md".into()]).unwrap();
    commit_private_tutorial(
        &resolved(),
        &["SKILL.md".into()],
        12,
        &GeneratorFingerprint::new("p1", "s1"),
        style,
        "Codex",
        &html,
    )
    .unwrap()
}

#[test]
fn seed_guide_is_readable_without_install_and_practice_is_gated() {
    let listed = list_guides().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id.as_str(), SEED_GUIDE_ID);
    assert_eq!(listed[0].display_name, SEED_DISPLAY_NAME);
    let guide = get_guide(SEED_GUIDE_ID).unwrap().unwrap();
    assert_eq!(guide.steps.len(), 4);
    assert!(!guide.steps[0].requires_skill);
    assert!(!guide.steps[1].requires_skill);
    assert!(guide.steps[2].requires_skill);
    assert_eq!(guide.steps[2].kind, GuideStepKind::Practice);
    assert_eq!(
        guide.skill_identity.source,
        crate::identity::SkillIdentitySource::Git {
            repository: SEED_REPOSITORY.into(),
            tracking_ref: crate::identity::GitTrackingRef::DefaultBranch,
            content_root: SEED_CONTENT_ROOT.into(),
        }
    );
    let preview = preview_practice_install(SEED_GUIDE_ID, "s3-practice").unwrap();
    assert!(preview.required);
    assert!(!preview.runs_author_commands);
    assert_eq!(preview.install_url, SEED_REPOSITORY);
}

#[test]
fn progress_round_trips_and_isolates_stale_revision() {
    let _dir = DataDir::new();
    let guide = frontend_design_first_success();
    let saved = save_progress(&LearningProgress {
        guide_id: guide.id.clone(),
        guide_revision_key: guide.revision_key.clone(),
        current_step_id: "s2-how".into(),
        completed_step_ids: vec!["s1-when".into()],
        updated_at: String::new(),
    })
    .unwrap();
    assert_eq!(saved.current_step_id, "s2-how");
    let loaded = load_progress(&guide.id, &guide.revision_key).unwrap();
    assert_eq!(loaded.current.unwrap().completed_step_ids, vec!["s1-when"]);
    assert!(loaded.stale.is_none());

    let other_key = GuideRevisionKey::from_digest(&[9u8; 32]);
    let stale_path = skillstar_core::infra::paths::learning_progress_dir()
        .join(guide.id.storage_segment())
        .join(format!("{}.json", other_key.storage_segment()));
    std::fs::write(
        &stale_path,
        serde_json::to_vec(&LearningProgress {
            guide_id: guide.id.clone(),
            guide_revision_key: other_key.clone(),
            current_step_id: "s1-when".into(),
            completed_step_ids: vec!["s1-when".into()],
            updated_at: "2020-01-01T00:00:00Z".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let mixed = load_progress(&guide.id, &guide.revision_key).unwrap();
    assert!(mixed.current.is_some());
    assert_eq!(
        mixed.stale.unwrap().guide_revision_key.as_str(),
        other_key.as_str()
    );
}

#[test]
fn save_progress_refuses_unknown_steps_and_does_not_write() {
    let _dir = DataDir::new();
    let guide = frontend_design_first_success();
    let err = save_progress(&LearningProgress {
        guide_id: guide.id.clone(),
        guide_revision_key: guide.revision_key.clone(),
        current_step_id: "nope".into(),
        completed_step_ids: vec![],
        updated_at: String::new(),
    })
    .unwrap_err();
    assert!(format!("{err:#}").contains("not in the Guide"));
    let loaded = load_progress(&guide.id, &guide.revision_key).unwrap();
    assert!(loaded.current.is_none());
}

#[test]
fn converts_guided_reference_and_workshop_html() {
    let _dir = DataDir::new();
    for (style, title, extra) in [
        (
            "guided",
            "Guided frontend path",
            "<h2>How</h2><p>Do it in order.</p>",
        ),
        (
            "reference",
            "Reference map",
            "<h2>API surface</h2><pre><code>export function paint()</code></pre>",
        ),
        (
            "workshop",
            "Workshop 实践",
            "<h2>实践</h2><ol><li>Change one screen</li></ol>",
        ),
    ] {
        let tutorial = commit_style(title, extra, style);
        let draft = create_guide_draft_from_tutorial(&tutorial, "zh-CN").unwrap();
        assert_eq!(draft.locale, "zh-CN");
        assert!(!draft.steps.is_empty());
        assert_eq!(
            draft.source_tutorial_key,
            tutorial.metadata.as_ref().unwrap().identity.as_ref().unwrap().key.storage_segment()
        );
        if style == "workshop" {
            assert!(
                draft
                    .steps
                    .iter()
                    .any(|step| step.kind == GuideStepKind::Practice)
            );
        }
    }
    assert_eq!(list_guide_drafts().unwrap().len(), 3);
}

#[test]
fn conversion_fails_closed_on_unbound_unknown_and_script() {
    let _dir = DataDir::new();
    let missing = crate::tutorial::PrivateTutorial {
        state: TutorialState::Missing,
        bound: true,
        html: None,
        metadata: None,
        stale_reason: None,
        stale_reasons: vec![],
    };
    assert!(create_guide_draft_from_tutorial(&missing, "zh-CN").is_err());

    let unbound = crate::tutorial::PrivateTutorial {
        state: TutorialState::Fresh,
        bound: false,
        html: Some("<html></html>".into()),
        metadata: None,
        stale_reason: None,
        stale_reasons: vec![],
    };
    assert!(create_guide_draft_from_tutorial(&unbound, "en").is_err());

    let mut poisoned = commit_style("Poison", "", "guided");
    poisoned.html = Some(
        poisoned
            .html
            .unwrap()
            .replace("</body>", "<marquee>nope</marquee></body>"),
    );
    let err = create_guide_draft_from_tutorial(&poisoned, "zh-CN").unwrap_err();
    assert!(format!("{err:#}").contains("unknown <marquee>"));
}

#[test]
fn repeated_conversion_keeps_the_previous_draft() {
    let _dir = DataDir::new();
    let tutorial = commit_style("Keep me", "", "guided");
    let first = create_guide_draft_from_tutorial(&tutorial, "zh-CN").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let second = create_guide_draft_from_tutorial(&tutorial, "zh-CN").unwrap();
    assert_ne!(first.revision_key, second.revision_key);
    assert_eq!(list_guide_drafts().unwrap().len(), 2);
}
