use std::path::Path;

use ss_skills::workflows::agent_managed_skills::AgentManagedSkillsState;

use super::notice::cleared_links_notice;
use super::rows::pause_snapshot;
use super::state::{
    AgentTone, ManagedSkillsUi, PauseAction, PauseStatus, global_skills_target_key,
};

fn state(active: &[&str], suspended: &[&str]) -> AgentManagedSkillsState {
    AgentManagedSkillsState {
        active_skill_names: active.iter().map(|name| (*name).to_string()).collect(),
        suspended_skill_names: suspended.iter().map(|name| (*name).to_string()).collect(),
    }
}

#[test]
fn missing_state_waits() {
    let snapshot = pause_snapshot(None);
    assert_eq!(snapshot.status, PauseStatus::Loading);
    assert!(snapshot.action.is_none());
    assert!(!snapshot.checked);
}

#[test]
fn active_names_offer_pause() {
    let snapshot = pause_snapshot(Some(&state(&["alpha", "beta", "alpha", " "], &[])));
    assert_eq!(snapshot.status, PauseStatus::Active);
    assert_eq!(snapshot.action, Some(PauseAction::Pause));
    assert!(snapshot.checked);
    assert_eq!(snapshot.active, ["alpha", "beta"]);
}

#[test]
fn suspended_names_offer_restore() {
    let snapshot = pause_snapshot(Some(&state(&[], &["alpha", "retired-skill", "alpha"])));
    assert_eq!(snapshot.status, PauseStatus::Paused);
    assert_eq!(snapshot.action, Some(PauseAction::Restore));
    assert!(!snapshot.checked);
    assert_eq!(snapshot.suspended, ["alpha", "retired-skill"]);
}

#[test]
fn mixed_directory_stays_in_recovery() {
    let snapshot = pause_snapshot(Some(&state(&["still-active"], &["needs-retry"])));
    assert_eq!(snapshot.status, PauseStatus::Partial);
    assert_eq!(snapshot.action, Some(PauseAction::Restore));
    assert!(!snapshot.checked);
}

#[test]
fn empty_directory_has_no_action() {
    let snapshot = pause_snapshot(Some(&state(&[], &[])));
    assert_eq!(snapshot.status, PauseStatus::Empty);
    assert!(snapshot.action.is_none());
    assert!(!snapshot.checked);
}

#[test]
fn target_key_folds_equivalent_spellings() {
    assert_eq!(
        global_skills_target_key(Path::new("/Users/test/.agent/skills/")),
        "/Users/test/.agent/skills"
    );
    assert_eq!(
        global_skills_target_key(Path::new(r"C:\Users\test\.agent\skills")),
        "C:/Users/test/.agent/skills"
    );
}

#[test]
fn clear_all_notice_reports_the_removed_count() {
    let notice = cleared_links_notice(3, "Devin");
    assert!(notice.tone == AgentTone::Ok);
    assert!(notice.text.contains('3'));
    assert!(notice.text.contains("Devin"));
    assert!(notice.open_path.is_none());
}

#[test]
fn clear_all_notice_without_removals_skips_the_agent_name() {
    let notice = cleared_links_notice(0, "Devin");
    assert!(notice.tone == AgentTone::Ok);
    assert!(!notice.text.contains("Devin"));
}

#[test]
fn stale_read_cannot_overwrite_a_newer_epoch() {
    let mut ui = ManagedSkillsUi::default();
    let (epoch, request) = ui.begin("dir");
    assert!(ui.accepts("dir", epoch, request));
    ui.invalidate("dir");
    assert!(!ui.accepts("dir", epoch, request));
    let (next_epoch, next_request) = ui.begin("dir");
    assert!(ui.accepts("dir", next_epoch, next_request));
    assert!(!ui.accepts("dir", epoch, request));
}
