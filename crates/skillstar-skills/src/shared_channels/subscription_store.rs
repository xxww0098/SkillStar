use super::{
    CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION, CHANNEL_SUBSCRIPTION_STORE_VERSION,
    ChannelReleaseTarget, ChannelSubscriptionRegistry, ChannelSubscriptionStore,
    ChannelSubscriptionView, SharedChannelError, SharedChannelErrorCode,
};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct DiskChannelSubscriptionRegistry;

impl DiskChannelSubscriptionRegistry {
    pub fn path() -> PathBuf {
        skillstar_core::infra::paths::config_dir().join("shared_channel_subscriptions.json")
    }

    fn lock_path() -> PathBuf {
        skillstar_core::infra::paths::config_dir().join("shared_channel_subscriptions.lock")
    }
}

#[async_trait::async_trait]
impl ChannelSubscriptionRegistry for DiskChannelSubscriptionRegistry {
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
        Ok(read_only_views(schema_version, &value))
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
                    != Some(u64::from(CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION))
            })
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSchemaUnsupported,
                "One or more shared channel subscriptions use a newer descriptor schema; they are available read-only",
            ));
        }
        let store: ChannelSubscriptionStore =
            serde_json::from_value(value).map_err(|_| storage_error("parse"))?;
        validate_store(&store)?;
        Ok(store)
    }

    fn save(&self, store: &ChannelSubscriptionStore) -> Result<(), SharedChannelError> {
        validate_store(store)?;
        let content = serde_json::to_vec_pretty(store).map_err(|_| storage_error("serialize"))?;
        skillstar_core::infra::fs_ops::atomic_write(&Self::path(), &content)
            .map_err(|_| storage_error("write"))
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
        if descriptor_version == CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION {
            current
                .push(serde_json::from_value(value.clone()).map_err(|_| storage_error("parse"))?);
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
        if descriptor_version != CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION {
            let view = read_only_view(schema_version, value)
                .ok_or_else(|| storage_error("parse read-only subscription"))?;
            views.push(view);
            continue;
        }
        let subscription: super::ChannelSubscription =
            serde_json::from_value(value.clone()).map_err(|_| storage_error("parse"))?;
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
        let mut skills = std::collections::BTreeSet::new();
        for skill in &subscription.skills {
            if crate::content::validate_skill_name(&skill.id).is_err()
                || !skills.insert(skill.id.to_ascii_lowercase())
                || !super::release::valid_content_root(&skill.content_root)
                || !valid_hash(&skill.release_content_hash)
                || !valid_hash(&skill.baseline_hash)
                || skill.baseline_hash != skill.release_content_hash
                || skill.release_content_hash_version != crate::content::SNAPSHOT_HASH_VERSION
                || skill.baseline_hash_version != crate::content::SNAPSHOT_HASH_VERSION
                || skill.provenance.repository_id != subscription.repository_id
                || !valid_repository_url(&skill.provenance.repository_url)
                || skill.provenance.git_ref != subscription.target.commit_sha
                || skill.provenance.source_folder != skill.content_root
            {
                return Err(storage_error("validate"));
            }
        }
    }
    Ok(())
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

fn read_only_views(schema_version: u32, value: &Value) -> Vec<ChannelSubscriptionView> {
    value
        .get("subscriptions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|subscription| read_only_view(schema_version, subscription))
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
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|skill| skill.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    Some(ChannelSubscriptionView {
        schema_version,
        descriptor_version,
        repository_id,
        organization_id,
        target,
        selected_skill_ids,
        read_only: true,
    })
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
mod tests {
    use super::*;
    use crate::shared_channels::{
        ChannelSkillProvenance, ChannelSubscribedSkill, ChannelSubscription,
    };

    fn subscription() -> ChannelSubscription {
        ChannelSubscription {
            descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
            repository_id: 42,
            organization_id: 7,
            target: ChannelReleaseTarget {
                revision: 1,
                tag_name: "channel-v000001".into(),
                commit_sha: "a".repeat(40),
            },
            skills: vec![ChannelSubscribedSkill {
                id: "writer".into(),
                content_root: "skills/writer".into(),
                release_content_hash: format!("sha256:{}", "b".repeat(64)),
                release_content_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
                baseline_hash: format!("sha256:{}", "b".repeat(64)),
                baseline_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
                provenance: ChannelSkillProvenance {
                    repository_id: 42,
                    repository_url: "https://github.com/acme/channel.git".into(),
                    git_ref: "a".repeat(40),
                    source_folder: "skills/writer".into(),
                },
            }],
            created_at: "2026-08-05T00:00:00Z".into(),
            updated_at: "2026-08-05T00:00:00Z".into(),
        }
    }

    #[test]
    fn unknown_schema_is_listed_read_only_but_mutation_is_rejected() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
        let mut value = serde_json::to_value(ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![subscription()],
        })
        .unwrap();
        value["schema_version"] = Value::from(99);
        skillstar_core::infra::fs_ops::atomic_write(
            &DiskChannelSubscriptionRegistry::path(),
            &serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let registry = DiskChannelSubscriptionRegistry;
        let views = registry.list_views().unwrap();
        assert_eq!(views.len(), 1);
        assert!(views[0].read_only);
        assert_eq!(views[0].selected_skill_ids, vec!["writer"]);
        assert_eq!(
            registry.load_mutable().unwrap_err().code,
            SharedChannelErrorCode::SubscriptionSchemaUnsupported
        );
        match previous {
            Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
        }
    }

    #[test]
    fn unknown_descriptor_is_listed_read_only_but_mutation_is_rejected() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
        let mut value = serde_json::to_value(ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![subscription()],
        })
        .unwrap();
        value["subscriptions"][0]["descriptor_version"] = Value::from(99);
        skillstar_core::infra::fs_ops::atomic_write(
            &DiskChannelSubscriptionRegistry::path(),
            &serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();

        let registry = DiskChannelSubscriptionRegistry;
        let views = registry.list_views().unwrap();
        assert_eq!(views.len(), 1);
        assert!(views[0].read_only);
        assert_eq!(views[0].descriptor_version, 99);
        assert_eq!(
            registry.load_mutable().unwrap_err().code,
            SharedChannelErrorCode::SubscriptionSchemaUnsupported
        );

        match previous {
            Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
        }
    }

    #[test]
    fn current_schema_rejects_duplicate_repository_views() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
        let duplicate = ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![subscription(), subscription()],
        };
        skillstar_core::infra::fs_ops::atomic_write(
            &DiskChannelSubscriptionRegistry::path(),
            &serde_json::to_vec(&duplicate).unwrap(),
        )
        .unwrap();

        assert_eq!(
            DiskChannelSubscriptionRegistry
                .list_views()
                .unwrap_err()
                .code,
            SharedChannelErrorCode::Storage
        );

        match previous {
            Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
        }
    }

    #[test]
    fn known_store_round_trips_after_restart() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
        let registry = DiskChannelSubscriptionRegistry;
        let store = ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![subscription()],
        };
        registry.save(&store).unwrap();

        let restarted = DiskChannelSubscriptionRegistry;
        assert_eq!(restarted.load_mutable().unwrap(), store);
        assert_eq!(
            restarted.list_views().unwrap()[0].selected_skill_ids,
            vec!["writer"]
        );
        match previous {
            Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
        }
    }
}
