use super::*;

/// Serializes env mutation and keeps `SKILLSTAR_DATA_DIR` pointed at a temp
/// dir for the test's duration, restoring the previous value after.
struct Sandbox {
    _temp: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
}

impl Sandbox {
    fn new() -> Self {
        let guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::TempDir::new().unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        Self {
            _temp: temp,
            _guard: guard,
            previous,
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
        }
    }
}

#[test]
fn newer_version_requires_a_strictly_higher_triple() {
    assert!(is_newer_version("0.0.1", "v0.0.2"));
    assert!(is_newer_version("v1.2.3", "V1.2.4"));
    assert!(is_newer_version("0.0.1", "0.1.0"));
    assert!(is_newer_version("0.0.1", "1.0.0"));
    assert!(!is_newer_version("0.0.2", "0.0.1"));
    assert!(!is_newer_version("0.0.2", "0.0.2"));
}

#[test]
fn unparsable_tags_are_never_newer() {
    // Prerelease suffixes and malformed tags must not produce an upgrade
    // nudge; comparing a dev build against them stays quiet instead.
    assert!(!is_newer_version("0.0.1", "v0.0.2-rc.1"));
    assert!(!is_newer_version("0.0.1-rc.1", "v0.0.2"));
    assert!(!is_newer_version("0.0.1", "v0.0"));
    assert!(!is_newer_version("0.0.1", "latest"));
    assert!(!is_newer_version("0.0.1", ""));
}

#[test]
fn evaluate_body_reports_available_up_to_date_and_failures() {
    let body = r#"{"tag_name":"v0.0.2","html_url":"https://github.com/xxww0098/SkillStar/releases/tag/v0.0.2"}"#;
    assert_eq!(
        evaluate_body("0.0.1", body),
        ReleaseCheckOutcome::Available {
            tag: "v0.0.2".into(),
            url: "https://github.com/xxww0098/SkillStar/releases/tag/v0.0.2".into(),
        }
    );
    assert_eq!(evaluate_body("0.0.2", body), ReleaseCheckOutcome::UpToDate);

    let no_url = r#"{"tag_name":"v0.0.2"}"#;
    assert_eq!(
        evaluate_body("0.0.1", no_url),
        ReleaseCheckOutcome::Available {
            tag: "v0.0.2".into(),
            url: RELEASES_PAGE_URL.into(),
        }
    );

    assert_eq!(
        evaluate_body("0.0.1", r#"{"html_url":"https://example.com"}"#),
        failed("release payload had no tag_name")
    );
    assert_eq!(
        evaluate_body("0.0.1", "not json"),
        failed("release payload was not valid JSON")
    );
}

#[test]
fn records_roundtrip_and_gate_the_auto_check() {
    let _sandbox = Sandbox::new();

    assert_eq!(last_record(), None);
    assert!(should_auto_check(1_000));

    persist_record(
        "0.0.1",
        ReleaseCheckOutcome::Available {
            tag: "v0.0.2".into(),
            url: RELEASES_PAGE_URL.into(),
        },
    );
    let record = last_record().expect("record persisted");
    assert_eq!(record.current_version, "0.0.1");
    assert_eq!(
        record.outcome,
        ReleaseCheckOutcome::Available {
            tag: "v0.0.2".into(),
            url: RELEASES_PAGE_URL.into(),
        }
    );
    assert!(!should_auto_check(record.last_checked_unix + 60));
    assert!(should_auto_check(record.last_checked_unix + 24 * 60 * 60));
}
