use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use chrono::{TimeZone, Utc};

use super::*;
use crate::lock_test_env;

fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
    unsafe { std::env::set_var(key, value) }
}

fn remove_env<K: AsRef<OsStr>>(key: K) {
    unsafe { std::env::remove_var(key) }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap()
}

fn with_team_env<T>(f: impl FnOnce(&Path) -> T) -> T {
    let _guard = lock_test_env();
    let tmp = tempfile::tempdir().expect("tempdir");
    let data = tmp.path().join(".skillstar");
    fs::create_dir_all(data.join("hub/skills")).unwrap();
    fs::create_dir_all(data.join("state")).unwrap();

    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    set_env("SKILLSTAR_DATA_DIR", &data);
    remove_env("SKILLSTAR_HUB_DIR");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(tmp.path())));

    match previous_data {
        Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
        None => remove_env("SKILLSTAR_DATA_DIR"),
    }
    match previous_hub {
        Some(value) => set_env("SKILLSTAR_HUB_DIR", value),
        None => remove_env("SKILLSTAR_HUB_DIR"),
    }

    match result {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn write_skill(name: &str, description: &str, body: &str) {
    let dir = skillstar_core::infra::paths::hub_skills_dir().join(name);
    fs::create_dir_all(&dir).unwrap();
    let content =
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\n{body}\n");
    fs::write(dir.join("SKILL.md"), content).unwrap();
}

#[test]
fn tokenize_keeps_latin_words_and_cjk_bigrams() {
    let tokens = super::recall::tokenize("Review the 代码审查 pipeline");
    assert!(tokens.contains(&"review".to_string()));
    assert!(!tokens.iter().any(|token| token == "the"));
    assert!(
        tokens.contains(&"代码".to_string()) || tokens.iter().any(|token| token.contains('代'))
    );
    assert!(
        tokens
            .iter()
            .any(|token| token == "审查" || token.contains("审"))
    );
}

#[test]
fn bm25_ranks_the_more_specific_skill_first() {
    with_team_env(|_| {
        write_skill(
            "pr-review",
            "Review pull requests and leave blocking comments",
            "Always request tests for API changes. Flag missing coverage.",
        );
        write_skill(
            "frontend-design",
            "UI layout and visual hierarchy for marketing pages",
            "Use a restrained palette. Avoid decorative gradients.",
        );

        let hits = recall("pull request review tests", 8, now()).unwrap();
        assert!(!hits.is_empty(), "expected at least one hit");
        assert_eq!(hits[0].id, "pr-review");
        assert_eq!(hits[0].kind, RecallKind::Skill);
    });
}

#[test]
fn search_installed_skills_orders_the_more_specific_skill_first() {
    with_team_env(|_| {
        write_skill(
            "pr-review",
            "Review pull requests and leave blocking comments",
            "Always request tests for API changes. Flag missing coverage.",
        );
        write_skill(
            "frontend-design",
            "UI layout and visual hierarchy for marketing pages",
            "Use a restrained palette. Avoid decorative gradients.",
        );
        let store = skillstar_core::infra::paths::team_store_path();
        let before = fs::read(&store).unwrap_or_default();
        let hits = search_installed_skills("pull request review tests", 8).unwrap();
        assert_eq!(hits[0].id, "pr-review");
        assert_eq!(fs::read(&store).unwrap_or_default(), before);
    });
}

#[test]
fn search_installed_skills_omits_learnings() {
    with_team_env(|_| {
        write_skill(
            "pr-review",
            "Review pull requests",
            "Leave comments on tests.",
        );
        let learning = share_learning(
            LearningDraft {
                title: "PR review missed a failing CI job".into(),
                body: "The reviewer skipped CI status and merged a red build.".into(),
                tags: vec!["ci".into()],
                skill_name: Some("pr-review".into()),
                confidence: 0.8,
            },
            now(),
        )
        .unwrap();
        let hits = search_installed_skills("failing CI merge", 8).unwrap();
        assert!(hits.iter().all(|hit| hit.id != learning.id));
        assert!(hits.iter().all(|hit| hit.title != learning.title));
    });
}

#[test]
fn search_installed_skills_does_not_append_recall_events() {
    with_team_env(|_| {
        write_skill(
            "pr-review",
            "Review pull requests and leave blocking comments",
            "Always request tests for API changes.",
        );
        let _ = recall("pull request review", 8, now()).unwrap();
        let store = skillstar_core::infra::paths::team_store_path();
        let before = fs::read(&store).unwrap();
        let _ = search_installed_skills("pull request review", 8).unwrap();
        assert_eq!(fs::read(&store).unwrap(), before);
        assert!(search_installed_skills("the", 8).unwrap().is_empty());
    });
}

#[test]
fn learning_neighbor_boosts_linked_skill() {
    with_team_env(|_| {
        write_skill(
            "pr-review",
            "Review pull requests",
            "Leave comments on tests.",
        );
        write_skill(
            "release-notes",
            "Write product release notes",
            "Summarize user-facing changes.",
        );
        share_learning(
            LearningDraft {
                title: "PR review missed a failing CI job".into(),
                body: "The reviewer skipped CI status and merged a red build.".into(),
                tags: vec!["ci".into()],
                skill_name: Some("pr-review".into()),
                confidence: 0.8,
            },
            now(),
        )
        .unwrap();

        let hits = recall("failing CI merge", 8, now()).unwrap();
        let learning = hits
            .iter()
            .find(|hit| hit.kind == RecallKind::Learning)
            .expect("learning should match CI query");
        assert!(hits.iter().any(|hit| hit.id == "pr-review"));
        assert!(learning.score > 0.0);
    });
}

#[test]
fn friction_threshold_marks_sessions_worth_documenting() {
    assert!(!is_worth_documenting(score_friction(0, 0, 2, 0)));
    assert!(is_worth_documenting(score_friction(2, 0, 0, 0)));
    with_team_env(|_| {
        let record = record_friction(
            FrictionInput {
                interrupts: 2,
                rejects: 0,
                retries: 8,
                corrections: 0,
                task: Some("Fix duplicate hook injection".into()),
            },
            now(),
        )
        .unwrap();
        assert!(record.worth_documenting);
        assert_eq!(record.score, 12);
        assert_eq!(record.task.as_deref(), Some("Fix duplicate hook injection"));
    });
}

#[test]
fn health_marks_unused_skills_silent_and_used_skills_higher() {
    with_team_env(|_| {
        write_skill("pr-review", "Review pull requests", "Check tests.");
        write_skill("silent-skill", "Never invoked helper", "Placeholder body.");
        record_usage("pr-review", now()).unwrap();
        let _ = recall("pull request", 4, now()).unwrap();

        let rows = health(now()).unwrap();
        let used = rows.iter().find(|row| row.name == "pr-review").unwrap();
        let silent = rows.iter().find(|row| row.name == "silent-skill").unwrap();
        assert!(!used.silent);
        assert!(used.usage_count >= 1);
        assert!(silent.silent);
        assert_eq!(silent.status, SkillHealthStatus::Silent);
        assert!(used.score > silent.score);
    });
}

#[test]
fn digest_covers_silent_skills_and_recent_notes() {
    with_team_env(|_| {
        write_skill("alpha", "Alpha helper", "Does alpha things.");
        write_skill("beta", "Beta helper", "Does beta things.");
        record_usage("alpha", now()).unwrap();
        share_learning(
            LearningDraft {
                title: "Alpha needs a dry-run flag".into(),
                body: "Agents keep applying alpha without preview.".into(),
                tags: vec!["cli".into()],
                skill_name: Some("alpha".into()),
                confidence: 0.6,
            },
            now(),
        )
        .unwrap();

        let report = digest(now()).unwrap();
        assert_eq!(report.skill_count, 2);
        assert_eq!(report.learning_count, 1);
        assert!(report.silent_skills.contains(&"beta".to_string()));
        assert_eq!(report.top_skills[0].name, "alpha");
        assert_eq!(
            report.recent_learnings[0].title,
            "Alpha needs a dry-run flag"
        );
    });
}

#[test]
fn unknown_store_schema_is_fail_closed() {
    with_team_env(|_| {
        let path = skillstar_core::infra::paths::team_store_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, r#"{"schema_version": 99, "learnings": []}"#).unwrap();
        let error = list_learnings().unwrap_err();
        assert!(
            error.to_string().contains("schema 99"),
            "unexpected error: {error}"
        );
        let error = share_learning(
            LearningDraft {
                title: "should fail".into(),
                body: "must not overwrite a future schema".into(),
                tags: Vec::new(),
                skill_name: None,
                confidence: 0.5,
            },
            now(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("schema 99"));
    });
}

#[test]
fn empty_query_returns_no_hits() {
    with_team_env(|_| {
        write_skill("pr-review", "Review pull requests", "Check tests.");
        let hits = recall("   ", 8, now()).unwrap();
        assert!(hits.is_empty());
    });
}

#[test]
fn usage_for_missing_skill_is_not_found() {
    with_team_env(|_| {
        let error = record_usage("missing-skill", now()).unwrap_err();
        assert!(error.to_string().contains("missing-skill"));
    });
}
