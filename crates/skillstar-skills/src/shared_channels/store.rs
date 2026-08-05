use super::{
    SHARED_CHANNEL_STORE_VERSION, SharedChannelError, SharedChannelErrorCode,
    SharedChannelRegistry, SharedChannelStore,
};
use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct DiskSharedChannelRegistry;

impl DiskSharedChannelRegistry {
    pub fn path() -> PathBuf {
        skillstar_core::infra::paths::config_dir().join("shared_channels.json")
    }
}

impl SharedChannelRegistry for DiskSharedChannelRegistry {
    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        let path = Self::path();
        if !path.exists() {
            return Ok(SharedChannelStore::default());
        }
        let bytes = std::fs::read(&path).map_err(|_| storage_error("read"))?;
        let store: SharedChannelStore =
            serde_json::from_slice(&bytes).map_err(|_| storage_error("parse"))?;
        if store.schema_version != SHARED_CHANNEL_STORE_VERSION {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                format!(
                    "Unsupported shared channel registry schema {}",
                    store.schema_version
                ),
            ));
        }
        Ok(store)
    }

    fn save(&self, store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        if store.schema_version != SHARED_CHANNEL_STORE_VERSION {
            return Err(storage_error("save an unsupported schema"));
        }
        let content = serde_json::to_vec_pretty(store).map_err(|_| storage_error("serialize"))?;
        skillstar_core::infra::fs_ops::atomic_write(&Self::path(), &content)
            .map_err(|_| storage_error("write"))
    }
}

fn storage_error(action: &str) -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Storage,
        format!("Unable to {action} the shared channel registry"),
    )
}
