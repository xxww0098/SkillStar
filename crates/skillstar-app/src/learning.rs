//! Project installed Skills into learning-domain identity and revision.

pub mod dto;

use skillstar_channels::shared_channels::{
    ChannelSkillPin, ChannelSubscribedSkill, ChannelSubscription, ChannelSubscriptionRegistry,
    ChannelSubscriptionStore, DiskChannelSubscriptionRegistry,
};
use skillstar_core::infra::error::AppError;
use skillstar_core::types::Skill;
use skillstar_learning::{
    ChannelReleaseRef, ContentRevision, GeneratorFingerprint, GitTrackingRef, GuideId,
    GuideRevisionKey, LearningProgress, PrivateTutorial, ResolvedSkill, SkillIdentity,
    SkillRevision, commit_private_tutorial, load_private_tutorial,
};
use skillstar_skills::source_identity::{self, InstalledSkillFacts};

use self::dto::{
    GuideDraftDto, GuideDto, GuideSummaryDto, LearningProgressDto, PracticeInstallPreviewDto,
    ProgressSnapshotDto,
};

/// Resolve an installed Skill handle into a source-compound identity.
///
/// `Skill.name` is only used to look up hub/lock/subscription/local sidecar
/// facts. It never becomes a stable key.
pub fn resolve_skill(skill: &Skill) -> Result<ResolvedSkill, AppError> {
    resolve_installed_name(&skill.name)
}

pub fn resolve_installed_name(name: &str) -> Result<ResolvedSkill, AppError> {
    let facts = source_identity::inspect_installed(name)?;
    if let Some((subscription, skill)) = channel_match(name)? {
        return resolve_channel(name, &facts, subscription, skill);
    }
    if facts.is_local {
        return resolve_local(name, &facts);
    }
    resolve_git(name, &facts)
}

pub fn load_tutorial(
    name: &str,
    generator: &GeneratorFingerprint,
) -> Result<PrivateTutorial, AppError> {
    let resolved = resolve_installed_name(name)?;
    let facts = source_identity::inspect_installed(name)?;
    load_private_tutorial(
        &resolved,
        &facts.snapshot.source_files,
        facts.snapshot.total_bytes,
        generator,
    )
}

pub fn commit_tutorial(
    name: &str,
    generator: &GeneratorFingerprint,
    tutorial_style: &str,
    agent_label: &str,
    raw_html: &str,
) -> Result<PrivateTutorial, AppError> {
    let resolved = resolve_installed_name(name)?;
    let facts = source_identity::inspect_installed(name)?;
    commit_private_tutorial(
        &resolved,
        &facts.snapshot.source_files,
        facts.snapshot.total_bytes,
        generator,
        tutorial_style,
        agent_label,
        raw_html,
    )
}

fn resolve_local(name: &str, facts: &InstalledSkillFacts) -> Result<ResolvedSkill, AppError> {
    let local_id = facts.local_id.ok_or_else(|| {
        AppError::Other(format!(
            "Local Skill '{name}' is missing a durable identity sidecar"
        ))
    })?;
    let identity = SkillIdentity::local(local_id)?;
    let revision = SkillRevision::local(&identity, content_revision(&facts.snapshot)?)?;
    ResolvedSkill::new(identity, revision, name, Some(name.to_string()))
}

fn resolve_git(name: &str, facts: &InstalledSkillFacts) -> Result<ResolvedSkill, AppError> {
    let lock = facts.lock.as_ref().ok_or_else(|| {
        AppError::Other(format!(
            "Git-backed Skill '{name}' has no lockfile provenance"
        ))
    })?;
    let head = facts.git_head.as_ref().ok_or_else(|| {
        AppError::Other(format!(
            "Git-backed Skill '{name}' has no readable HEAD commit"
        ))
    })?;
    let identity = SkillIdentity::git(
        lock.canonical_repository.clone(),
        GitTrackingRef::from_lock_ref(lock.git_ref.as_deref())?,
        lock.source_folder.clone().unwrap_or_default(),
    )?;
    let revision = SkillRevision::git(
        &identity,
        head.commit_sha.clone(),
        head.content_root_tree.clone(),
        content_revision(&facts.snapshot)?,
    )?;
    ResolvedSkill::new(identity, revision, name, Some(name.to_string()))
}

fn resolve_channel(
    name: &str,
    facts: &InstalledSkillFacts,
    subscription: ChannelSubscription,
    skill: ChannelSubscribedSkill,
) -> Result<ResolvedSkill, AppError> {
    if skill.release_content_hash_version != 2 {
        return Err(AppError::Other(format!(
            "Channel Skill '{name}' has unsupported content hash version {}",
            skill.release_content_hash_version
        )));
    }
    let identity = SkillIdentity::channel(subscription.repository_id, skill.content_root.clone())?;
    let release = channel_release_label(&subscription, &skill, &facts.snapshot.content_hash);
    let revision = SkillRevision::channel(
        &identity,
        skill.provenance.git_ref.clone(),
        release,
        content_revision(&facts.snapshot)?,
    )?;
    ResolvedSkill::new(identity, revision, name, Some(name.to_string()))
}

fn channel_release_label(
    subscription: &ChannelSubscription,
    skill: &ChannelSubscribedSkill,
    current_hash: &str,
) -> Option<ChannelReleaseRef> {
    if current_hash != skill.release_content_hash {
        return None;
    }
    let pin = subscription
        .pins
        .iter()
        .find(|pin: &&ChannelSkillPin| pin.skill_id == skill.id);
    let target = pin.map(|pin| &pin.target).unwrap_or(&subscription.target);
    if target.commit_sha != skill.provenance.git_ref {
        return None;
    }
    Some(ChannelReleaseRef {
        revision: target.revision,
        tag_name: target.tag_name.clone(),
    })
}

fn channel_match(
    name: &str,
) -> Result<Option<(ChannelSubscription, ChannelSubscribedSkill)>, AppError> {
    let store = load_subscriptions()?;
    let mut matches = Vec::new();
    for subscription in store.subscriptions {
        for skill in subscription.skills.clone() {
            if skill.id == name || skill.id.eq_ignore_ascii_case(name) {
                matches.push((subscription.clone(), skill));
            }
        }
    }
    if matches.len() > 1 {
        return Err(AppError::Other(format!(
            "Installed Skill '{name}' is claimed by multiple channel subscriptions"
        )));
    }
    Ok(matches.into_iter().next())
}

fn load_subscriptions() -> Result<ChannelSubscriptionStore, AppError> {
    DiskChannelSubscriptionRegistry
        .load_mutable()
        .map_err(|error| AppError::Other(format!("Failed to read channel subscriptions: {error}")))
}

fn content_revision(
    snapshot: &skillstar_skills::source_identity::SnapshotFacts,
) -> Result<ContentRevision, AppError> {
    ContentRevision::new(snapshot.hash_version, snapshot.content_hash.clone())
}

pub fn list_guides() -> Result<Vec<GuideSummaryDto>, AppError> {
    skillstar_learning::list_guides()?
        .into_iter()
        .map(|summary| {
            let (installed, skill_drift) = install_overlay(&summary.skill_identity, &summary.skill_revision);
            Ok(GuideSummaryDto::from_summary(summary, installed, skill_drift))
        })
        .collect()
}

pub fn get_guide(id: &str) -> Result<Option<GuideDto>, AppError> {
    let Some(guide) = skillstar_learning::get_guide(id)? else {
        return Ok(None);
    };
    let (installed, skill_drift) = install_overlay(&guide.skill_identity, &guide.skill_revision);
    Ok(Some(GuideDto::from_guide(guide, installed, skill_drift)))
}

pub fn load_guide_progress(
    guide_id: &str,
    guide_revision_key: &str,
) -> Result<ProgressSnapshotDto, AppError> {
    let id = GuideId::new(guide_id)?;
    let revision = GuideRevisionKey::from_wire(guide_revision_key)?;
    Ok(skillstar_learning::load_progress(&id, &revision)?.into())
}

pub fn save_guide_progress(
    guide_id: String,
    guide_revision_key: String,
    current_step_id: String,
    completed_step_ids: Vec<String>,
) -> Result<LearningProgressDto, AppError> {
    let saved = skillstar_learning::save_progress(&LearningProgress {
        guide_id: GuideId::new(guide_id)?,
        guide_revision_key: GuideRevisionKey::from_wire(&guide_revision_key)?,
        current_step_id,
        completed_step_ids,
        updated_at: String::new(),
    })?;
    Ok(saved.into())
}

pub fn preview_practice_install(
    guide_id: &str,
    step_id: &str,
) -> Result<PracticeInstallPreviewDto, AppError> {
    let preview = skillstar_learning::preview_practice_install(guide_id, step_id)?;
    let (installed, skill_drift) = install_overlay(&preview.skill_identity, &preview.skill_revision);
    Ok(PracticeInstallPreviewDto::from_preview(
        preview,
        installed,
        skill_drift,
    ))
}

pub fn preview_guide_draft(name: &str, locale: &str) -> Result<GuideDraftDto, AppError> {
    let tutorial = load_tutorial(name, &conversion_fingerprint())?;
    Ok(skillstar_learning::preview_guide_draft_from_tutorial(&tutorial, locale)?.into())
}

pub fn create_guide_draft(name: &str, locale: &str) -> Result<GuideDraftDto, AppError> {
    let tutorial = load_tutorial(name, &conversion_fingerprint())?;
    Ok(skillstar_learning::create_guide_draft_from_tutorial(&tutorial, locale)?.into())
}

pub fn list_guide_drafts() -> Result<Vec<GuideDraftDto>, AppError> {
    Ok(skillstar_learning::list_guide_drafts()?
        .into_iter()
        .map(GuideDraftDto::from)
        .collect())
}

fn conversion_fingerprint() -> GeneratorFingerprint {
    GeneratorFingerprint::new("guided.v1", "artifact.v1")
}

fn install_overlay(identity: &SkillIdentity, bound: &SkillRevision) -> (bool, bool) {
    match resolve_installed_name(skillstar_learning::SEED_DISPLAY_NAME) {
        Ok(resolved) if resolved.identity.key == identity.key => {
            (true, resolved.revision.key != bound.key)
        }
        _ => (false, false),
    }
}

#[cfg(test)]
mod tests;
