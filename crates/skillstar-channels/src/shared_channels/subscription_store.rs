use super::{
    CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION, CHANNEL_SUBSCRIPTION_STORE_VERSION,
    ChannelReleaseTarget, ChannelSubscriptionRegistry, ChannelSubscriptionStore,
    ChannelSubscriptionView, DiskSharedChannelRegistry, SharedChannelError, SharedChannelErrorCode,
    SharedChannelRegistry,
};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct DiskChannelSubscriptionRegistry;

pub(crate) fn managed_repository_for_skill(
    skill_id: &str,
) -> Result<Option<u64>, SharedChannelError> {
    Ok(DiskChannelSubscriptionRegistry
        .list_views()?
        .into_iter()
        .find(|subscription| {
            subscription
                .selected_skill_ids
                .iter()
                .any(|selected| selected.eq_ignore_ascii_case(skill_id))
        })
        .map(|subscription| subscription.repository_id))
}

pub(crate) fn managed_repository_for_url(
    repository_url: &str,
) -> Result<Option<u64>, SharedChannelError> {
    let subscriptions = DiskChannelSubscriptionRegistry
        .load_mutable()?
        .subscriptions;
    if let Some(subscription) =
        subscriptions.iter().find(|subscription| {
            subscription.repository_url_aliases.iter().any(|alias| {
                skillstar_skills::source_resolver::same_remote_url(alias, repository_url)
            }) || subscription.skills.iter().any(|skill| {
                skillstar_skills::source_resolver::same_remote_url(
                    &skill.provenance.repository_url,
                    repository_url,
                )
            })
        })
    {
        return Ok(Some(subscription.repository_id));
    }

    let subscribed_ids = subscriptions
        .iter()
        .map(|subscription| subscription.repository_id)
        .collect::<std::collections::BTreeSet<_>>();
    Ok(DiskSharedChannelRegistry
        .load()?
        .channels
        .into_iter()
        .find(|channel| {
            subscribed_ids.contains(&channel.repository_id)
                && skillstar_skills::source_resolver::same_remote_url(
                    &channel.clone_url,
                    repository_url,
                )
        })
        .map(|channel| channel.repository_id))
}

impl DiskChannelSubscriptionRegistry {
    pub fn path() -> PathBuf {
        skillstar_core::infra::paths::config_dir().join("shared_channel_subscriptions.json")
    }

    fn lock_path() -> PathBuf {
        skillstar_core::infra::paths::config_dir().join("shared_channel_subscriptions.lock")
    }

    fn auto_update_lock_path(repository_id: u64) -> PathBuf {
        skillstar_core::infra::paths::config_dir()
            .join(format!("shared_channel_auto_update_{repository_id}.lock"))
    }
}

#[async_trait::async_trait]
impl ChannelSubscriptionRegistry for DiskChannelSubscriptionRegistry {
    fn auto_update_scope_key(&self) -> String {
        Self::path().to_string_lossy().into_owned()
    }

    fn try_acquire_auto_update_run_lease(
        &self,
        repository_id: u64,
    ) -> Result<Option<Box<dyn super::SharedChannelMutationLease>>, SharedChannelError> {
        let path = Self::auto_update_lock_path(repository_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| storage_error("create automatic update lock directory"))?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(|_| storage_error("open automatic update lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Box::new(file))),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(_)) => Err(storage_error("lock automatic update run")),
        }
    }

    async fn acquire_mutation_lease(
        &self,
    ) -> Result<Box<dyn super::SharedChannelMutationLease>, SharedChannelError> {
        Ok(Box::new(acquire_lock_file(Self::lock_path()).await?))
    }

    fn list_views(&self) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(&path).map_err(|_| storage_error("read"))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| storage_error("parse"))?;
        let schema_version = value
            .get("schema_version")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| storage_error("parse schema version"))?;
        if schema_version == CHANNEL_SUBSCRIPTION_STORE_VERSION {
            return current_schema_views(schema_version, &value);
        }
        read_only_views(schema_version, &value)
    }

    fn load_mutable(&self) -> Result<ChannelSubscriptionStore, SharedChannelError> {
        let path = Self::path();
        if !path.exists() {
            return Ok(ChannelSubscriptionStore::default());
        }
        let bytes = std::fs::read(&path).map_err(|_| storage_error("read"))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| storage_error("parse"))?;
        let schema_version = value
            .get("schema_version")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| storage_error("parse schema version"))?;
        if schema_version != CHANNEL_SUBSCRIPTION_STORE_VERSION {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSchemaUnsupported,
                format!(
                    "Shared channel subscriptions use unsupported schema {schema_version}; they are available read-only"
                ),
            ));
        }
        if value
            .get("subscriptions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|subscription| {
                subscription
                    .get("descriptor_version")
                    .and_then(Value::as_u64)
                    .is_none_or(|version| !supported_descriptor_version(version))
            })
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSchemaUnsupported,
                "One or more shared channel subscriptions use a newer descriptor schema; they are available read-only",
            ));
        }
        let mut store: ChannelSubscriptionStore =
            serde_json::from_value(value).map_err(|_| storage_error("parse"))?;
        for subscription in &mut store.subscriptions {
            normalize_subscription(subscription);
        }
        validate_store(&store)?;
        Ok(store)
    }

    fn save(&self, store: &ChannelSubscriptionStore) -> Result<(), SharedChannelError> {
        validate_store(store)?;
        let content = serde_json::to_vec_pretty(store).map_err(|_| storage_error("serialize"))?;
        skillstar_core::infra::fs_ops::atomic_write(&Self::path(), &content)
            .map_err(|_| storage_error("write"))
    }

    fn repository_route_lockfile(&self) -> Option<std::path::PathBuf> {
        Some(skillstar_skills::lockfile::lockfile_path())
    }
}

async fn acquire_lock_file(path: PathBuf) -> Result<File, SharedChannelError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| storage_error("create lock directory"))?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|_| storage_error("open mutation lock"))?;
    let file = tokio::task::spawn_blocking(move || file.lock().map(|()| file))
        .await
        .map_err(|_| storage_error("join mutation lock task"))?
        .map_err(|_| storage_error("lock subscription mutation"))?;
    Ok(file)
}

fn current_schema_views(
    schema_version: u32,
    value: &Value,
) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
    let subscriptions = value
        .get("subscriptions")
        .and_then(Value::as_array)
        .ok_or_else(|| storage_error("parse"))?;
    let mut repositories = std::collections::BTreeSet::new();
    let mut current = Vec::new();
    for value in subscriptions {
        let repository_id = value
            .get("repository_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| storage_error("parse repository identity"))?;
        if !repositories.insert(repository_id) {
            return Err(storage_error("validate"));
        }
        let descriptor_version = value
            .get("descriptor_version")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| storage_error("parse descriptor version"))?;
        if supported_descriptor_version(u64::from(descriptor_version)) {
            let mut subscription: super::ChannelSubscription =
                serde_json::from_value(value.clone()).map_err(|_| storage_error("parse"))?;
            normalize_subscription(&mut subscription);
            current.push(subscription);
        }
    }
    validate_store(&ChannelSubscriptionStore {
        schema_version,
        subscriptions: current,
    })?;

    let mut views = Vec::with_capacity(subscriptions.len());
    for value in subscriptions {
        let descriptor_version = value
            .get("descriptor_version")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| storage_error("parse descriptor version"))?;
        if !supported_descriptor_version(u64::from(descriptor_version)) {
            let view = read_only_view(schema_version, value)
                .ok_or_else(|| storage_error("parse read-only subscription"))?;
            views.push(view);
            continue;
        }
        let mut subscription: super::ChannelSubscription =
            serde_json::from_value(value.clone()).map_err(|_| storage_error("parse"))?;
        normalize_subscription(&mut subscription);
        views.push(ChannelSubscriptionView::from_subscription(&subscription));
    }
    Ok(views)
}

fn validate_store(store: &ChannelSubscriptionStore) -> Result<(), SharedChannelError> {
    if store.schema_version != CHANNEL_SUBSCRIPTION_STORE_VERSION {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionSchemaUnsupported,
            format!(
                "Shared channel subscriptions use unsupported schema {}; they are available read-only",
                store.schema_version
            ),
        ));
    }
    let mut repositories = std::collections::BTreeSet::new();
    for subscription in &store.subscriptions {
        if subscription.descriptor_version != CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION
            || subscription.repository_id == 0
            || subscription.organization_id == 0
            || !repositories.insert(subscription.repository_id)
            || subscription.target.revision == 0
            || subscription.target.tag_name != super::revision_tag(subscription.target.revision)
            || !valid_commit(&subscription.target.commit_sha)
            || chrono::DateTime::parse_from_rfc3339(&subscription.created_at).is_err()
            || chrono::DateTime::parse_from_rfc3339(&subscription.updated_at).is_err()
        {
            return Err(storage_error("validate"));
        }
        let current_routes = subscription
            .skills
            .iter()
            .map(|skill| {
                skillstar_skills::source_resolver::normalize_remote_url(
                    &skill.provenance.repository_url,
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut aliases = std::collections::BTreeSet::new();
        if subscription.repository_url_aliases.iter().any(|alias| {
            let normalized = skillstar_skills::source_resolver::normalize_remote_url(alias);
            !valid_repository_url(alias)
                || current_routes.contains(&normalized)
                || !aliases.insert(normalized)
        }) {
            return Err(storage_error("validate"));
        }
        let mut skills = std::collections::BTreeSet::new();
        for skill in &subscription.skills {
            if skillstar_skills::content::validate_skill_name(&skill.id).is_err()
                || !skills.insert(skill.id.to_ascii_lowercase())
                || !super::release::valid_content_root(&skill.content_root)
                || !valid_hash(&skill.release_content_hash)
                || !valid_hash(&skill.baseline_hash)
                || skill.baseline_hash != skill.release_content_hash
                || skill.release_content_hash_version
                    != skillstar_skills::content::SNAPSHOT_HASH_VERSION
                || skill.baseline_hash_version != skillstar_skills::content::SNAPSHOT_HASH_VERSION
                || skill.provenance.repository_id != subscription.repository_id
                || !valid_repository_url(&skill.provenance.repository_url)
                || !valid_commit(&skill.provenance.git_ref)
                || skill.provenance.source_folder != skill.content_root
            {
                return Err(storage_error("validate"));
            }
        }
        let mut known = std::collections::BTreeSet::new();
        if subscription.known_skill_ids.iter().any(|id| {
            skillstar_skills::content::validate_skill_name(id).is_err()
                || !known.insert(id.to_ascii_lowercase())
        }) || !skills.is_subset(&known)
        {
            return Err(storage_error("validate"));
        }
        let mut pins = std::collections::BTreeSet::new();
        if subscription.pins.iter().any(|pin| {
            !skills.contains(&pin.skill_id.to_ascii_lowercase())
                || !pins.insert(pin.skill_id.to_ascii_lowercase())
                || !valid_target(&pin.target)
                || pin.target.revision > subscription.target.revision
                || !subscription.skills.iter().any(|skill| {
                    skill.id.eq_ignore_ascii_case(&pin.skill_id)
                        && skill.provenance.git_ref == pin.target.commit_sha
                })
        }) || !valid_auto_update_state(&subscription.auto_update)
            || !valid_remote_state(&subscription.remote_state)
        {
            return Err(storage_error("validate"));
        }
        if let Some(snapshot) = &subscription.last_update {
            super::channel_update_validation::validate_update_snapshot(snapshot)?;
            if snapshot.target.revision < subscription.target.revision
                || (snapshot.target.revision == subscription.target.revision
                    && snapshot.target != subscription.target)
                || snapshot.acknowledgement_required
                    != (snapshot.target.revision > subscription.target.revision)
            {
                return Err(storage_error("validate"));
            }
            for item in &snapshot.items {
                let expected = subscription
                    .pins
                    .iter()
                    .find(|pin| pin.skill_id.eq_ignore_ascii_case(&item.id))
                    .map(|pin| &pin.target);
                if item.pinned_target.as_ref() != expected {
                    return Err(storage_error("validate"));
                }
            }
            if subscription.pins.iter().any(|pin| {
                !snapshot
                    .items
                    .iter()
                    .any(|item| item.id.eq_ignore_ascii_case(&pin.skill_id))
            }) {
                return Err(storage_error("validate"));
            }
        }
    }
    Ok(())
}

fn normalize_subscription(subscription: &mut super::ChannelSubscription) {
    subscription.descriptor_version = CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION;
    if subscription.known_skill_ids.is_empty() && !subscription.skills.is_empty() {
        subscription.known_skill_ids = subscription
            .skills
            .iter()
            .map(|skill| skill.id.clone())
            .collect();
    }
    if let Some(snapshot) = &mut subscription.last_update {
        if snapshot.check_error.is_some() && snapshot.check_error_code.is_none() {
            snapshot.check_error_code = Some(SharedChannelErrorCode::Protocol);
        }
        for item in &mut snapshot.items {
            if item.state == super::ChannelUpdateItemState::Failed && item.error_code.is_none() {
                item.error_code = Some(SharedChannelErrorCode::SubscriptionUpdateFailed);
            }
        }
    }
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn supported_descriptor_version(version: u64) -> bool {
    matches!(version, 1..=3) || version == u64::from(CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION)
}

fn valid_target(target: &ChannelReleaseTarget) -> bool {
    target.revision > 0
        && target.tag_name == super::revision_tag(target.revision)
        && valid_commit(&target.commit_sha)
}

fn valid_auto_update_state(state: &super::ChannelAutoUpdateState) -> bool {
    if state
        .next_check_at
        .as_ref()
        .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err())
    {
        return false;
    }
    let Some(run) = &state.last_run else {
        return true;
    };
    if chrono::DateTime::parse_from_rfc3339(&run.started_at).is_err()
        || run
            .completed_at
            .as_ref()
            .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err())
        || (run.status == super::ChannelAutoUpdateRunStatus::Checking) != run.completed_at.is_none()
        || run.retryable
            && !matches!(
                run.status,
                super::ChannelAutoUpdateRunStatus::RetryableFailure
                    | super::ChannelAutoUpdateRunStatus::PartiallyApplied
            )
        || run
            .target
            .as_ref()
            .is_some_and(|target| !valid_target(target))
    {
        return false;
    }
    let mut applied = std::collections::BTreeSet::new();
    if run.applied_skill_ids.iter().any(|id| {
        skillstar_skills::content::validate_skill_name(id).is_err()
            || !applied.insert(id.to_ascii_lowercase())
    }) {
        return false;
    }
    let mut pauses = std::collections::BTreeSet::new();
    !run.pauses.iter().any(|pause| {
        pause
            .detail
            .as_ref()
            .is_some_and(|detail| detail.contains('\0'))
            || pause.skill_id.as_ref().is_some_and(|id| {
                skillstar_skills::content::validate_skill_name(id).is_err()
                    || !pauses.insert(id.to_ascii_lowercase())
            })
    }) && run.error.as_ref().is_none_or(|error| !error.contains('\0'))
}

fn valid_remote_state(state: &super::ChannelSubscriptionRemoteState) -> bool {
    let valid_fields = state
        .checked_at
        .as_ref()
        .is_none_or(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
        && state
            .message
            .as_ref()
            .is_none_or(|message| !message.contains('\0'));
    valid_fields
        && (state.status == super::ChannelSubscriptionRemoteStatus::Active
            || state.checked_at.is_some()
                && state
                    .message
                    .as_ref()
                    .is_some_and(|message| !message.is_empty()))
}

fn valid_hash(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn valid_repository_url(value: &str) -> bool {
    let Some(path) = value.strip_prefix("https://github.com/") else {
        return false;
    };
    if path.contains(['@', '?', '#', '\\']) {
        return false;
    }
    let mut parts = path.split('/');
    let Some(owner) = parts.next() else {
        return false;
    };
    let Some(repository) = parts.next().and_then(|name| name.strip_suffix(".git")) else {
        return false;
    };
    parts.next().is_none()
        && valid_route_segment(owner, false)
        && valid_route_segment(repository, true)
}

fn valid_route_segment(value: &str, allow_repository_punctuation: bool) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '-'
                || (allow_repository_punctuation && matches!(character, '.' | '_'))
        })
}

fn read_only_views(
    schema_version: u32,
    value: &Value,
) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
    value
        .get("subscriptions")
        .and_then(Value::as_array)
        .ok_or_else(unsupported_read_only_projection)?
        .iter()
        .map(|subscription| {
            read_only_view(schema_version, subscription)
                .ok_or_else(unsupported_read_only_projection)
        })
        .collect()
}

fn read_only_view(schema_version: u32, subscription: &Value) -> Option<ChannelSubscriptionView> {
    let repository_id = subscription.get("repository_id")?.as_u64()?;
    let descriptor_version = subscription
        .get("descriptor_version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .unwrap_or_default();
    let organization_id = subscription.get("organization_id").and_then(Value::as_u64);
    let target = subscription.get("target").and_then(read_target);
    let selected_skill_ids = subscription
        .get("skills")
        .and_then(Value::as_array)?
        .iter()
        .map(|skill| skill.get("id").and_then(Value::as_str))
        .collect::<Option<Vec<_>>>()?;
    let mut seen = std::collections::BTreeSet::new();
    if selected_skill_ids.iter().any(|id| {
        skillstar_skills::content::validate_skill_name(id).is_err()
            || !seen.insert(id.to_ascii_lowercase())
    }) {
        return None;
    }
    let selected_skill_ids = selected_skill_ids.into_iter().map(str::to_string).collect();
    Some(ChannelSubscriptionView {
        schema_version,
        descriptor_version,
        repository_id,
        organization_id,
        target,
        selected_skill_ids,
        auto_update: super::ChannelAutoUpdateState::default(),
        remote_state: super::ChannelSubscriptionRemoteState::default(),
        read_only: true,
    })
}

fn unsupported_read_only_projection() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::SubscriptionSchemaUnsupported,
        "One or more future shared channel subscriptions cannot safely project managed Skill ownership",
    )
}

fn read_target(value: &Value) -> Option<ChannelReleaseTarget> {
    Some(ChannelReleaseTarget {
        revision: value.get("revision")?.as_u64()?,
        tag_name: value.get("tag_name")?.as_str()?.to_string(),
        commit_sha: value.get("commit_sha")?.as_str()?.to_string(),
    })
}

fn storage_error(action: &str) -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Storage,
        format!("Unable to {action} the shared channel subscription store"),
    )
}

#[cfg(test)]
#[path = "subscription_store_tests.rs"]
mod tests;
