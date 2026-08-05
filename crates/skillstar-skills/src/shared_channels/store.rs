use super::{
    CHANNEL_DESCRIPTOR_VERSION, SHARED_CHANNEL_STORE_VERSION, SharedChannelError,
    SharedChannelErrorCode, SharedChannelMutationLease, SharedChannelRegistry, SharedChannelStore,
};
use async_trait::async_trait;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct DiskSharedChannelRegistry;

impl DiskSharedChannelRegistry {
    pub fn path() -> PathBuf {
        skillstar_core::infra::paths::config_dir().join("shared_channels.json")
    }

    fn lock_path() -> PathBuf {
        skillstar_core::infra::paths::config_dir().join("shared_channels.lock")
    }
}

#[async_trait]
impl SharedChannelRegistry for DiskSharedChannelRegistry {
    async fn acquire_mutation_lease(
        &self,
    ) -> Result<Box<dyn SharedChannelMutationLease>, SharedChannelError> {
        Ok(Box::new(acquire_lock_file(Self::lock_path()).await?))
    }

    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        let path = Self::path();
        if !path.exists() {
            return Ok(SharedChannelStore::default());
        }
        let bytes = std::fs::read(&path).map_err(|_| storage_error("read"))?;
        let store: SharedChannelStore =
            serde_json::from_slice(&bytes).map_err(|_| storage_error("parse"))?;
        validate_versions(&store)?;
        Ok(store)
    }

    fn save(&self, store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        validate_versions(store)?;
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
        .map_err(|_| storage_error("join registry lock task"))?
        .map_err(|_| storage_error("lock registry mutation"))?;
    Ok(file)
}

fn validate_versions(store: &SharedChannelStore) -> Result<(), SharedChannelError> {
    if store.schema_version != SHARED_CHANNEL_STORE_VERSION {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Storage,
            format!(
                "Unsupported shared channel registry schema {}",
                store.schema_version
            ),
        ));
    }
    if let Some(channel) = store
        .channels
        .iter()
        .find(|channel| channel.descriptor_version != CHANNEL_DESCRIPTOR_VERSION)
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Storage,
            format!(
                "Unsupported shared channel descriptor schema {}",
                channel.descriptor_version
            ),
        ));
    }
    Ok(())
}

fn storage_error(action: &str) -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Storage,
        format!("Unable to {action} the shared channel registry"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_channels::{
        SharedChannelAuthorization, SharedChannelDescriptor, SharedChannelRole, SharedChannelStatus,
    };

    #[test]
    fn rejects_unknown_descriptor_versions_even_when_store_version_is_current() {
        let mut store = SharedChannelStore::default();
        store.channels.push(SharedChannelDescriptor {
            descriptor_version: CHANNEL_DESCRIPTOR_VERSION + 1,
            repository_id: 42,
            organization_id: 7,
            owner: "acme".into(),
            name: "channel".into(),
            html_url: "https://github.com/acme/channel".into(),
            clone_url: "https://github.com/acme/channel.git".into(),
            role: SharedChannelRole::Owner,
            status: SharedChannelStatus::Active,
            authorization: SharedChannelAuthorization::default(),
            created_at: String::new(),
            updated_at: String::new(),
        });

        let error = validate_versions(&store).unwrap_err();

        assert_eq!(error.code, SharedChannelErrorCode::Storage);
    }

    #[tokio::test]
    async fn mutation_lock_serializes_independent_file_handles() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shared_channels.lock");
        let first = acquire_lock_file(path.clone()).await.unwrap();
        let waiter = tokio::spawn(acquire_lock_file(path));
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());

        drop(first);

        let second = waiter.await.unwrap().unwrap();
        drop(second);
    }
}
