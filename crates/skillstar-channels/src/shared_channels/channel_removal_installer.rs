use super::{
    ChannelRemovedSkillHandler, ChannelSubscribedSkill, GitChannelSubscriptionInstaller,
    SharedChannelError, SharedChannelErrorCode,
};
use async_trait::async_trait;

#[async_trait]
impl ChannelRemovedSkillHandler for GitChannelSubscriptionInstaller {
    async fn uninstall_and_commit(
        &self,
        skill: &ChannelSubscribedSkill,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError> {
        let skill = skill.clone();
        tokio::task::spawn_blocking(move || {
            let _guard =
                skillstar_skills::skill_update::acquire_update_transaction_lock().map_err(|error| {
                    removal_error(format!("Unable to lock removed Skill uninstall: {error}"))
                })?;
            let mut metadata_error = None;
            match skillstar_skills::skill_install::uninstall_hub_skill_with_commit(&skill.id, || {
                commit().inspect_err(|error| metadata_error = Some(error.clone()))
            }) {
                Ok(()) => Ok(()),
                Err(failure) => Err(transaction_failure(failure, metadata_error)),
            }
        })
        .await
        .map_err(|_| removal_error("The removed Skill uninstall task stopped unexpectedly"))?
    }

    async fn convert_to_local_and_commit(
        &self,
        skill: &ChannelSubscribedSkill,
        local_name: &str,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError> {
        let skill = skill.clone();
        let local_name = local_name.to_string();
        tokio::task::spawn_blocking(move || {
            let _guard =
                skillstar_skills::skill_update::acquire_update_transaction_lock().map_err(|error| {
                    removal_error(format!("Unable to lock removed Skill conversion: {error}"))
                })?;
            skillstar_skills::local_skill::preserve_installed_copy(&skill.id, &local_name).map_err(
                |error| {
                    removal_error(format!(
                        "Unable to preserve removed Skill as '{local_name}': {error:#}"
                    ))
                },
            )?;
            let mut metadata_error = None;
            match skillstar_skills::skill_install::uninstall_hub_skill_with_commit(&skill.id, || {
                commit().inspect_err(|error| metadata_error = Some(error.clone()))
            }) {
                Ok(()) => Ok(()),
                Err(failure) if failure.committed => {
                    Err(transaction_failure(failure, metadata_error))
                }
                Err(failure) if failure.rollback_complete => {
                    let cleanup_error = skillstar_skills::local_skill::delete(&local_name).err();
                    let mut error = transaction_failure(failure, metadata_error);
                    if let Some(cleanup_error) = cleanup_error {
                        error.message = format!(
                            "{}; removing the uncommitted local copy '{local_name}' also failed: {cleanup_error:#}",
                            error.message
                        );
                    }
                    Err(error)
                }
                Err(failure) => {
                    let mut error = transaction_failure(failure, metadata_error);
                    error.message = format!(
                        "{}; the safety copy '{local_name}' was kept because restoring the channel Skill was incomplete",
                        error.message
                    );
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| removal_error("The removed Skill conversion task stopped unexpectedly"))?
    }
}

fn removal_error(message: impl Into<String>) -> SharedChannelError {
    SharedChannelError::new(SharedChannelErrorCode::SubscriptionUpdateFailed, message)
}

fn transaction_failure(
    failure: skillstar_skills::skill_install::UninstallSkillFailure,
    metadata_error: Option<SharedChannelError>,
) -> SharedChannelError {
    match metadata_error {
        Some(error) => SharedChannelError::new(error.code, failure.message),
        None => removal_error(failure.message),
    }
}
