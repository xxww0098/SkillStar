//! Shared channels desktop bridge (`MySkillsChannels` in TSX).
//!
//! Routes all GitHub organization channel admin, repository registration,
//! immutable release publishing, membership/inbox, and subscriber lifecycle
//! calls through `ss_app::channel_ops` so `ss-gui-bridge` never takes a direct
//! `ss-channels` dependency edge.

use std::path::PathBuf;

use nsh_core::JsonValue;
use nsh_core::file_dialog::OpenDialog;
use nsh_core::prelude::*;
use ss_app::channel_ops::{
    self, ApplyChannelUpdateRequest, ChannelInviteRole, ChannelReleaseTarget,
    ChannelSkillUpdateResolution, ConvertRemovedChannelSkillRequest,
    CreateChannelInvitationRequest, CreateSharedChannelRequest, ExistingChannelScanRequest,
    LocalDivergenceResolution, RollbackChannelSkillRequest, SharedChannelError,
    SharedChannelErrorCode, SubscribeChannelRequest,
};

fn format_channel_error(err: SharedChannelError) -> String {
    if err.code == SharedChannelErrorCode::NotAuthenticated {
        format!("AUTH_REQUIRED: {}", err.message)
    } else {
        err.message
    }
}

fn to_json_err(err: serde_json::Error) -> String {
    format!("failed to serialize channel payload: {err}")
}

macro_rules! to_json {
    ($value:expr) => {
        serde_json::to_value($value).map_err(to_json_err)
    };
}

fn parse_target(target: JsonValue) -> Result<ChannelReleaseTarget, String> {
    serde_json::from_value(target).map_err(|err| format!("invalid release target: {err}"))
}

fn parse_resolutions(resolutions: JsonValue) -> Result<Vec<ChannelSkillUpdateResolution>, String> {
    if resolutions.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(resolutions).map_err(|err| format!("invalid update resolutions: {err}"))
}

fn parse_opt_resolution(resolution: JsonValue) -> Result<Option<LocalDivergenceResolution>, String> {
    if resolution.is_null() {
        return Ok(None);
    }
    serde_json::from_value(resolution)
        .map(Some)
        .map_err(|err| format!("invalid divergence resolution: {err}"))
}

fn parse_invite_role(role: &str) -> Result<ChannelInviteRole, String> {
    match role.trim().to_ascii_lowercase().as_str() {
        "publisher" => Ok(ChannelInviteRole::Publisher),
        "subscriber" => Ok(ChannelInviteRole::Subscriber),
        other => Err(format!("unsupported channel invite role '{other}'")),
    }
}

async fn make_shared_facade() -> Result<channel_ops::ProdSharedChannelFacade, String> {
    blocking(|| channel_ops::shared_channel_facade().map_err(format_channel_error))
        .await
        .map_err(|join| format!("shared channel init failed: {join}"))?
}

async fn make_registration_facade() -> Result<channel_ops::ProdRegistrationFacade, String> {
    blocking(|| channel_ops::registration_facade().map_err(format_channel_error))
        .await
        .map_err(|join| format!("channel registration init failed: {join}"))?
}

async fn make_publication_facade() -> Result<channel_ops::ProdPublicationFacade, String> {
    blocking(|| channel_ops::publication_facade().map_err(format_channel_error))
        .await
        .map_err(|join| format!("channel publication init failed: {join}"))?
}

async fn make_membership_facade() -> Result<channel_ops::ProdMembershipFacade, String> {
    blocking(|| channel_ops::membership_facade().map_err(format_channel_error))
        .await
        .map_err(|join| format!("channel membership init failed: {join}"))?
}

async fn make_subscription_facade() -> Result<channel_ops::ProdSubscriptionFacade, String> {
    blocking(|| channel_ops::subscription_facade().map_err(format_channel_error))
        .await
        .map_err(|join| format!("channel subscription init failed: {join}"))?
}

/// Shared channels facade exposed to TSX as `MySkillsChannels`.
#[module]
pub struct MySkillsChannels;

#[module]
impl MySkillsChannels {
    /// Local read-only channel registry & subscription snapshot (works offline and signed out).
    pub async fn snapshot() -> Result<JsonValue, String> {
        blocking(|| {
            let snap = channel_ops::local_channels_snapshot().map_err(format_channel_error)?;
            to_json!(&snap)
        })
        .await
        .map_err(|join| format!("channels snapshot task failed: {join}"))?
    }

    /// Toggle per-channel protected automatic updates (local preference, no network).
    pub async fn set_auto_update(repository_id: u64, enabled: bool) -> Result<JsonValue, String> {
        let state = channel_ops::set_channel_auto_update_enabled(repository_id, enabled)
            .await
            .map_err(format_channel_error)?;
        to_json!(&state)
    }

    /// Export a subscribed channel's installed Skills as a Claude Code plugin marketplace directory.
    pub async fn export_marketplace(repository_id: u64, out_dir: String) -> Result<JsonValue, String> {
        blocking(move || {
            let result = channel_ops::export_channel_marketplace(
                repository_id,
                &PathBuf::from(out_dir.trim()),
            )
            .map_err(format_channel_error)?;
            Ok(serde_json::json!({
                "root": result.root.to_string_lossy().into_owned(),
                "repository_id": result.repository_id,
                "revision": result.revision,
                "plugin_names": result.plugin_names,
                "files": result.files,
            }))
        })
        .await
        .map_err(|join| format!("export marketplace task failed: {join}"))?
    }

    /// System folder picker for selecting a Claude Marketplace export destination.
    pub async fn pick_export_folder(prompt: String) -> Option<String> {
        let dialog = OpenDialog::folders().title(prompt);
        match pick_paths(dialog).await {
            Ok(Some(paths)) => paths
                .into_iter()
                .next()
                .map(|path| path.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    /// List GitHub organizations accessible to the signed-in SkillStar App user.
    pub async fn list_organizations() -> Result<JsonValue, String> {
        let orgs = make_shared_facade()
            .await?
            .list_organizations()
            .await
            .map_err(format_channel_error)?;
        to_json!(&orgs)
    }

    /// Create a new organization-private shared channel repository.
    pub async fn create_channel(
        organization: String,
        repository_name: String,
        description: String,
    ) -> Result<JsonValue, String> {
        let descriptor = make_shared_facade()
            .await?
            .create_channel(CreateSharedChannelRequest {
                organization,
                repository_name,
                description,
            })
            .await
            .map_err(format_channel_error)?;
        to_json!(&descriptor)
    }

    /// Resume checking App installation / binding status for a pending created channel.
    pub async fn resume_channel(repository_id: u64) -> Result<JsonValue, String> {
        let descriptor = make_shared_facade()
            .await?
            .resume_channel(repository_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&descriptor)
    }

    /// List organization-private repositories eligible to register as a shared channel.
    pub async fn list_registration_candidates(organization_id: u64) -> Result<JsonValue, String> {
        let candidates = make_registration_facade()
            .await?
            .list_candidates(organization_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&candidates)
    }

    /// Shallow-scan an existing org-private repository before registering it as a shared channel.
    pub async fn scan_existing_repo(
        organization_id: u64,
        repository_id: u64,
        session_id: String,
    ) -> Result<JsonValue, String> {
        let preview = make_registration_facade()
            .await?
            .scan(
                ExistingChannelScanRequest {
                    organization_id,
                    repository_id,
                },
                session_id,
            )
            .await
            .map_err(format_channel_error)?;
        to_json!(&preview)
    }

    /// Confirm registering a scanned existing repository using its `session_id`.
    pub async fn confirm_registration(session_id: String) -> Result<JsonValue, String> {
        let descriptor = make_registration_facade()
            .await?
            .confirm(&session_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&descriptor)
    }

    /// Cancel an active registration scan/preview session.
    pub async fn cancel_registration(session_id: String) -> bool {
        channel_ops::cancel_registration_scan(&session_id)
    }

    /// Preview skills on the channel default branch before publishing an immutable release.
    pub async fn publish_preview(repository_id: u64, session_id: String) -> Result<JsonValue, String> {
        let preview = make_publication_facade()
            .await?
            .preview(repository_id, session_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&preview)
    }

    /// Publish an immutable release using a previously created publish preview `session_id`.
    pub async fn publish_release(
        session_id: String,
        title: String,
        notes: String,
    ) -> Result<JsonValue, String> {
        let result = make_publication_facade()
            .await?
            .publish(&session_id, title, notes)
            .await
            .map_err(format_channel_error)?;
        to_json!(&result)
    }

    /// Cancel an active channel publish preview session.
    pub async fn cancel_publish(session_id: String) -> bool {
        channel_ops::cancel_publish_preview(&session_id)
    }

    /// List channel members and pending invitations.
    pub async fn list_membership(repository_id: u64) -> Result<JsonValue, String> {
        let snap = make_membership_facade()
            .await?
            .list_membership(repository_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&snap)
    }

    /// Invite a GitHub user to a channel (`subscriber` or `publisher`).
    pub async fn invite_member(
        repository_id: u64,
        username: String,
        role: String,
    ) -> Result<JsonValue, String> {
        let role = parse_invite_role(&role)?;
        let action = make_membership_facade()
            .await?
            .invite(CreateChannelInvitationRequest {
                repository_id,
                username,
                role,
            })
            .await
            .map_err(format_channel_error)?;
        to_json!(&action)
    }

    /// Cancel a pending channel invitation.
    pub async fn cancel_invitation(repository_id: u64, invitation_id: u64) -> Result<JsonValue, String> {
        let action = make_membership_facade()
            .await?
            .cancel(repository_id, invitation_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&action)
    }

    /// Resend/recreate a pending non-admin channel invitation.
    pub async fn resend_invitation(repository_id: u64, invitation_id: u64) -> Result<JsonValue, String> {
        let action = make_membership_facade()
            .await?
            .resend(repository_id, invitation_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&action)
    }

    /// Revoke a user's direct repository collaborator grant and re-check effective GitHub access.
    pub async fn revoke_member(repository_id: u64, username: String) -> Result<JsonValue, String> {
        let result = make_membership_facade()
            .await?
            .revoke_member(repository_id, &username)
            .await
            .map_err(format_channel_error)?;
        to_json!(&result)
    }

    /// List pending repository invitations for the signed-in GitHub user.
    pub async fn list_inbox() -> Result<JsonValue, String> {
        let invitations = make_membership_facade()
            .await?
            .list_inbox()
            .await
            .map_err(format_channel_error)?;
        to_json!(&invitations)
    }

    /// Accept a pending repository invitation and import the channel locally.
    pub async fn accept_invitation(invitation_id: u64) -> Result<JsonValue, String> {
        let descriptor = make_membership_facade()
            .await?
            .accept(invitation_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&descriptor)
    }

    /// Resume checking an accepted channel that is still waiting on App installation/access.
    pub async fn resume_accepted_channel(repository_id: u64) -> Result<JsonValue, String> {
        let descriptor = make_membership_facade()
            .await?
            .resume_accepted_channel(repository_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&descriptor)
    }

    /// Decline a pending repository invitation.
    pub async fn decline_invitation(invitation_id: u64) -> Result<JsonValue, String> {
        let action = make_membership_facade()
            .await?
            .decline(invitation_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&action)
    }

    /// Review the latest verified release of an active channel before subscribing.
    pub async fn review_channel(repository_id: u64) -> Result<JsonValue, String> {
        let review = make_subscription_facade()
            .await?
            .review(repository_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&review)
    }

    /// Subscribe to a channel release with the user's reviewed `selected_skill_ids`.
    pub async fn subscribe_channel(
        repository_id: u64,
        target: JsonValue,
        selected_skill_ids: Vec<String>,
    ) -> Result<JsonValue, String> {
        let target = parse_target(target)?;
        let subscription = make_subscription_facade()
            .await?
            .subscribe(SubscribeChannelRequest {
                repository_id,
                target,
                selected_skill_ids,
            })
            .await
            .map_err(format_channel_error)?;
        to_json!(&subscription)
    }

    /// Check remote updates and local divergence state for a subscribed channel.
    pub async fn check_update(repository_id: u64) -> Result<JsonValue, String> {
        let snapshot = make_subscription_facade()
            .await?
            .check_update(repository_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&snapshot)
    }

    /// Apply reviewed updates (including explicit preserve/discard divergence resolutions).
    pub async fn apply_update(
        repository_id: u64,
        target: JsonValue,
        resolutions: JsonValue,
    ) -> Result<JsonValue, String> {
        let target = parse_target(target)?;
        let resolutions = parse_resolutions(resolutions)?;
        let result = make_subscription_facade()
            .await?
            .apply_update(ApplyChannelUpdateRequest {
                repository_id,
                target,
                resolutions,
            })
            .await
            .map_err(format_channel_error)?;
        to_json!(&result)
    }

    /// List earlier published releases containing `skill_id` for rollback selection.
    pub async fn list_rollback_targets(
        repository_id: u64,
        skill_id: String,
    ) -> Result<JsonValue, String> {
        let targets = make_subscription_facade()
            .await?
            .list_skill_rollback_targets(repository_id, &skill_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&targets)
    }

    /// Roll one Skill back to `target` and pin it there until the user resumes following.
    pub async fn rollback_skill(
        repository_id: u64,
        skill_id: String,
        target: JsonValue,
        resolution: JsonValue,
    ) -> Result<JsonValue, String> {
        let target = parse_target(target)?;
        let resolution = parse_opt_resolution(resolution)?;
        let result = make_subscription_facade()
            .await?
            .rollback_skill(RollbackChannelSkillRequest {
                repository_id,
                skill_id,
                target,
                resolution,
            })
            .await
            .map_err(format_channel_error)?;
        to_json!(&result)
    }

    /// Unpin a rolled-back Skill so channel upgrades follow future releases again.
    pub async fn resume_following_skill(
        repository_id: u64,
        skill_id: String,
    ) -> Result<JsonValue, String> {
        let snapshot = make_subscription_facade()
            .await?
            .resume_following_skill(repository_id, &skill_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&snapshot)
    }

    /// Install an unselected/newly added Skill from the latest release of a subscribed channel.
    pub async fn install_channel_skill(
        repository_id: u64,
        skill_id: String,
    ) -> Result<JsonValue, String> {
        let result = make_subscription_facade()
            .await?
            .install_channel_skill(repository_id, &skill_id)
            .await
            .map_err(format_channel_error)?;
        to_json!(&result)
    }

    /// Uninstall a Skill marked `removed_from_channel` or belonging to a `revoked` channel.
    pub async fn uninstall_removed_skill(
        repository_id: u64,
        skill_id: String,
        revoked: bool,
    ) -> Result<JsonValue, String> {
        let facade = make_subscription_facade().await?;
        if revoked {
            let result = facade
                .uninstall_revoked_skill(repository_id, &skill_id)
                .await
                .map_err(format_channel_error)?;
            to_json!(&result)
        } else {
            let result = facade
                .uninstall_removed_skill(repository_id, &skill_id)
                .await
                .map_err(format_channel_error)?;
            to_json!(&result)
        }
    }

    /// Convert a removed or revoked channel Skill into an independent local Skill copy.
    pub async fn convert_removed_skill_to_local(
        repository_id: u64,
        skill_id: String,
        local_name: String,
        revoked: bool,
    ) -> Result<JsonValue, String> {
        let facade = make_subscription_facade().await?;
        let req = ConvertRemovedChannelSkillRequest {
            repository_id,
            skill_id,
            local_name,
        };
        if revoked {
            let result = facade
                .convert_revoked_skill_to_local(req)
                .await
                .map_err(format_channel_error)?;
            to_json!(&result)
        } else {
            let result = facade
                .convert_removed_skill_to_local(req)
                .await
                .map_err(format_channel_error)?;
            to_json!(&result)
        }
    }
}

#[cfg(test)]
mod tests {
    use nsh_core::JsonValue;

    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-channels-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
            Self {
                _lock: lock,
                previous,
                root,
            }
        }
    }

    impl Drop for IsolatedDataDir {
        fn drop(&mut self) {
            unsafe {
                match self.previous.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn offline_snapshot_and_session_cancellations_work_without_github_sign_in() {
        let _iso = IsolatedDataDir::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let snap = rt
            .block_on(super::MySkillsChannels::snapshot())
            .expect("snapshot succeeds signed out");
        assert_eq!(snap["channels"].as_array().map(Vec::len), Some(0));
        assert_eq!(snap["subscriptions"].as_array().map(Vec::len), Some(0));

        assert!(!rt.block_on(super::MySkillsChannels::cancel_registration("nope".into())));
        assert!(!rt.block_on(super::MySkillsChannels::cancel_publish("nope".into())));

        let parsed = super::parse_resolutions(JsonValue::Array(vec![serde_json::json!({
            "skill_id": "demo",
            "resolution": { "type": "preserve", "local_name": "demo.local" }
        })]))
        .expect("resolution payload deserializes");
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn signed_out_remote_calls_report_auth_required_prefix() {
        let _iso = IsolatedDataDir::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(super::MySkillsChannels::list_inbox())
            .expect_err("signed out inbox must fail");
        assert!(err.starts_with("AUTH_REQUIRED:"), "got: {err}");
    }
}
