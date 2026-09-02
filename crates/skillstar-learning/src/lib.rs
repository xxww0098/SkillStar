//! Private learning artifacts keyed by source-compound skill identity.
//!
//! This crate owns SkillIdentity/SkillRevision, CSP-strict private tutorials,
//! and the Guide/progress/draft persistence seam. It depends only on
//! `skillstar-core`. Source resolution lives in `skillstar-app::learning`.

mod guide;
mod identity;
mod tutorial;

pub use guide::{
    CalloutTone, ConversionPreview, Guide, GuideBlock, GuideDraft, GuideId, GuideRevisionKey,
    GuideStep, GuideStepKind, GuideSummary, LearningProgress, PracticeInstallPreview,
    ProgressSnapshot, create_guide_draft_from_tutorial, frontend_design_first_success, get_guide,
    list_guide_drafts, list_guides, load_progress, preview_guide_draft_from_tutorial,
    preview_practice_install, save_progress, SEED_DISPLAY_NAME, SEED_GUIDE_ID,
};
pub use identity::{
    ChannelReleaseRef, ContentRevision, GitTrackingRef, ResolvedSkill, SkillIdentity,
    SkillIdentityKey, SkillIdentitySource, SkillRevision, SkillRevisionKey, SkillSourceRevision,
    normalize_content_root,
};
pub use tutorial::{
    GeneratorFingerprint, PrivateTutorial, PrivateTutorialMetadata, TutorialStaleReason,
    TutorialState, ValidatedTutorialHtml, commit_private_tutorial, load_private_tutorial,
    validate_html,
};

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
