//! models.dev catalog cache.
//!
//! The file is the response body from [`MODELS_DEV_URL`], stored at
//! `<data_root>/cache/gateway-catalog/models.dev.json`. A failed refresh
//! leaves the previous file in place. A missing file reads as empty: this
//! module does not ship a handwritten model table, and it does not write
//! `cache/model_catalog/` or a provider store.

use std::path::PathBuf;
use std::time::Duration;

use skillstar_core::infra::fs_ops::atomic_write;
use skillstar_core::infra::paths::data_root;

/// Catalog document. Tests may fetch another URL; this constant stays.
pub const MODELS_DEV_URL: &str = "https://models.dev/api.json";

const TIMEOUT: Duration = Duration::from_secs(30);
const BODY_CAP: usize = 64 << 20;

/// Why a refresh did not replace the cache.
#[derive(Debug)]
pub struct ModelsDevError;

impl std::fmt::Display for ModelsDevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("models_dev_unavailable")
    }
}

impl std::error::Error for ModelsDevError {}

/// `<data_root>/cache/gateway-catalog/models.dev.json`.
pub fn models_dev_cache_path() -> PathBuf {
    data_root()
        .join("cache")
        .join("gateway-catalog")
        .join("models.dev.json")
}

/// Cached body, or empty when the file is missing or unreadable.
pub fn models_dev_load() -> Vec<u8> {
    std::fs::read(models_dev_cache_path()).unwrap_or_default()
}

/// Download `url` and replace the cache with that body.
///
/// The body must be a non-empty JSON object. Anything else, including a
/// transport error, leaves the file that is already there.
pub fn models_dev_sync(url: &str) -> Result<(), ModelsDevError> {
    let body = fetch(url)?;
    if !acceptable(&body) {
        return Err(ModelsDevError);
    }
    atomic_write(&models_dev_cache_path(), &body).map_err(|_| ModelsDevError)
}

fn acceptable(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    matches!(value, serde_json::Value::Object(map) if !map.is_empty())
}

fn fetch(url: &str) -> Result<Vec<u8>, ModelsDevError> {
    let url = url.to_string();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|_| ModelsDevError)?;
    runtime.block_on(async move {
        let client = skillstar_core::infra::http_client::probe_http_client(TIMEOUT)
            .map_err(|_| ModelsDevError)?;
        let pending = client.get(&url).timeout(TIMEOUT).send();
        let mut response = match tokio::time::timeout(TIMEOUT, pending).await {
            Ok(Ok(response)) => response,
            _ => return Err(ModelsDevError),
        };
        if response.status().as_u16() / 100 != 2 {
            return Err(ModelsDevError);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ModelsDevError)? {
            if body.len() + chunk.len() > BODY_CAP {
                return Err(ModelsDevError);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    })
}
