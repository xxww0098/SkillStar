use super::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelInstallReceipt, ChannelReleaseManifest,
    ChannelReleaseSkill, ChannelReleaseTarget, ChannelSkillReleaseStatus, ChannelSubscribedSkill,
    RemoteRepository, SharedChannelDescriptor, SharedChannelError, SharedChannelErrorCode,
    SharedChannelRole, project_role,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn route_migration_error(
    error: SharedChannelError,
    rollback: Option<anyhow::Error>,
) -> SharedChannelError {
    SharedChannelError::new(
        error.code,
        rollback.map_or(error.message.clone(), |rollback| {
            format!(
                "{}; repository route rollback is incomplete: {rollback}",
                error.message
            )
        }),
    )
}

pub(super) fn refreshed_channel_view(
    channel: &SharedChannelDescriptor,
    repository: &RemoteRepository,
) -> SharedChannelDescriptor {
    let mut refreshed = channel.clone();
    refreshed.owner = repository.owner_login.clone();
    refreshed.name = repository.name.clone();
    refreshed.html_url = repository.html_url.clone();
    refreshed.clone_url = repository.clone_url.clone();
    refreshed.role = project_role(&repository.permissions).unwrap_or(SharedChannelRole::Subscriber);
    refreshed
}

pub(super) fn selected_manifest_skills<'a>(
    manifest: &'a ChannelReleaseManifest,
    requested: &[String],
) -> Result<Vec<&'a ChannelReleaseSkill>, SharedChannelError> {
    let active = manifest
        .skills
        .iter()
        .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::with_capacity(requested.len());
    for id in requested {
        let skill = active.get(&id.to_ascii_lowercase()).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                format!("Selected Skill '{id}' is not present in this channel release"),
            )
        })?;
        selected.push(*skill);
    }
    selected.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(selected)
}

pub(super) fn validate_selection(selected: &[String]) -> Result<(), SharedChannelError> {
    let mut seen = BTreeSet::new();
    for id in selected {
        crate::content::validate_skill_name(id).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "The channel subscription contains an invalid Skill identity",
            )
        })?;
        if !seen.insert(id.to_ascii_lowercase()) {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "The channel subscription contains duplicate Skill identities",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_receipt(
    receipt: &ChannelInstallReceipt,
    repository: &RemoteRepository,
    manifest: &ChannelReleaseManifest,
    selected: &[&ChannelReleaseSkill],
) -> Result<(), SharedChannelError> {
    let expected = selected
        .iter()
        .map(|skill| (skill.id.to_ascii_lowercase(), *skill))
        .collect::<BTreeMap<_, _>>();
    if receipt.skills.len() != expected.len() {
        return Err(install_integrity_error());
    }
    let mut seen = BTreeSet::new();
    for installed in &receipt.skills {
        let key = installed.id.to_ascii_lowercase();
        let Some(released) = expected.get(&key) else {
            return Err(install_integrity_error());
        };
        if !seen.insert(key)
            || installed.content_root != released.content_root
            || installed.release_content_hash != released.content_hash
            || installed.release_content_hash_version != released.content_hash_version
            || installed.baseline_hash != released.content_hash
            || installed.baseline_hash_version != CHANNEL_CONTENT_HASH_VERSION
            || installed.provenance.repository_id != manifest.repository_id
            || !crate::source_resolver::same_remote_url(
                &installed.provenance.repository_url,
                &repository.clone_url,
            )
            || installed.provenance.git_ref != manifest.commit_sha
            || installed.provenance.source_folder != released.content_root
        {
            return Err(install_integrity_error());
        }
    }
    Ok(())
}

fn install_integrity_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Integrity,
        "The installed channel Skills do not match the selected release",
    )
}

pub(super) fn release_target(manifest: &ChannelReleaseManifest) -> ChannelReleaseTarget {
    ChannelReleaseTarget {
        revision: manifest.revision,
        tag_name: manifest.tag_name.clone(),
        commit_sha: manifest.commit_sha.clone(),
    }
}

pub(super) fn selected_ids(skills: &[ChannelSubscribedSkill]) -> BTreeSet<String> {
    skills
        .iter()
        .map(|skill| skill.id.to_ascii_lowercase())
        .collect()
}

pub(super) fn selected_ids_from_manifest(skills: &[&ChannelReleaseSkill]) -> BTreeSet<String> {
    skills
        .iter()
        .map(|skill| skill.id.to_ascii_lowercase())
        .collect()
}
