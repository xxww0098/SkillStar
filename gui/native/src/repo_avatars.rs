//! Disk-cached GitHub repository owner avatars (`cache_dir()/avatars/{owner}.png`).
//!
//! Matches GPUI `my_skills/skill_card/avatar.rs`: reads existing cached PNG/JPEG/
//! WebP/GIF files off-thread, falls back to fetching `github.com/{owner}.png?size=120`
//! through `ss_core::infra::http_client::probe_http_client` when running in the
//! live app, and memoizes `data:image/…;base64,…` URIs per `(cache_dir, owner)`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nsh_core::prelude::blocking;

const MIN_AVATAR_BYTES: usize = 32;
const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;
const MAX_CONCURRENT_AVATAR_FETCHES: usize = 4;

static AVATAR_URI_CACHE: OnceLock<Mutex<HashMap<(PathBuf, String), String>>> = OnceLock::new();
static FAILED_OWNERS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static FETCH_SEMAPHORE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();

fn uri_cache() -> &'static Mutex<HashMap<(PathBuf, String), String>> {
    AVATAR_URI_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn failed_owners() -> &'static Mutex<HashSet<String>> {
    FAILED_OWNERS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn fetch_semaphore() -> &'static tokio::sync::Semaphore {
    FETCH_SEMAPHORE.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_AVATAR_FETCHES))
}

/// Validate a GitHub owner/org handle, trimming whitespace and leading `@`.
/// Rejects empty values, leading hyphens, >39-char handles, and path characters.
pub fn sanitize_github_owner(raw: &str) -> Option<String> {
    let clean = raw.trim().trim_start_matches('@');
    if clean.is_empty() || clean.len() > 39 {
        return None;
    }
    let mut chars = clean.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return None;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') {
        return None;
    }
    Some(clean.to_owned())
}

/// Sniff magic bytes (PNG, JPEG, WebP, GIF) and encode as a `data:image/…` URI.
pub fn image_bytes_to_data_uri(bytes: &[u8]) -> Option<String> {
    if !(MIN_AVATAR_BYTES..=MAX_AVATAR_BYTES).contains(&bytes.len()) {
        return None;
    }
    let prefix = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "data:image/png;base64,"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "data:image/jpeg;base64,"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "data:image/webp;base64,"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "data:image/gif;base64,"
    } else {
        return None;
    };
    Some(crate::encode_data_uri(prefix, bytes))
}

fn avatar_cache_path(avatars_dir: &std::path::Path, clean_owner: &str) -> PathBuf {
    avatars_dir.join(format!("{clean_owner}.png"))
}

/// Pure in-memory lookup in the process cache (zero disk or network I/O).
pub fn peek_github_avatar_data_uri(owner: &str) -> Option<String> {
    let clean = sanitize_github_owner(owner)?;
    let key = (ss_core::infra::paths::avatars_cache_dir(), clean);
    uri_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .get(&key)
        .cloned()
}

/// Read an already-cached avatar file from `cache/avatars/{owner}.png`,
/// validate its image bytes, and store it in the in-memory cache.
pub fn read_cached_github_avatar_data_uri(owner: &str) -> Option<String> {
    let clean = sanitize_github_owner(owner)?;
    let avatars_dir = ss_core::infra::paths::avatars_cache_dir();
    let key = (avatars_dir.clone(), clean.clone());
    if let Some(existing) = uri_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .get(&key)
        .cloned()
    {
        return Some(existing);
    }

    let path = avatar_cache_path(&avatars_dir, &clean);
    let bytes = std::fs::read(&path).ok()?;
    let uri = image_bytes_to_data_uri(&bytes)?;
    uri_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .insert(key, uri.clone());
    Some(uri)
}

fn store_in_uri_cache(clean_owner: &str, uri: &str) {
    let key = (ss_core::infra::paths::avatars_cache_dir(), clean_owner.to_owned());
    uri_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .insert(key, uri.to_owned());
}

fn write_cached_github_avatar(clean_owner: &str, bytes: &[u8]) {
    let avatars_dir = ss_core::infra::paths::avatars_cache_dir();
    let dest = avatar_cache_path(&avatars_dir, clean_owner);
    if let Some(parent) = dest.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = dest.with_extension(format!("png.{}-{nanos}.part", std::process::id()));
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &dest);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn running_in_headless_test() -> bool {
    cfg!(test)
        || std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(|stem| stem.ends_with("-test"))
            })
            .unwrap_or(false)
}

async fn fetch_github_avatar_bytes(clean_owner: &str) -> Option<Vec<u8>> {
    let client = ss_core::infra::http_client::probe_http_client(Duration::from_secs(10)).ok()?;
    let url = format!("https://github.com/{clean_owner}.png?size=120");
    let response = client.get(&url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    (bytes.len() <= MAX_AVATAR_BYTES).then(|| bytes.to_vec())
}

/// Load the avatar data URI from disk cache off the UI thread, or fetch via the
/// configured HTTP client on cache miss (skipped in headless tests).
pub async fn load_github_avatar_data_uri(owner: &str) -> Option<String> {
    let clean = sanitize_github_owner(owner)?;
    if let Some(uri) = peek_github_avatar_data_uri(&clean) {
        return Some(uri);
    }

    let cached_owner = clean.clone();
    if let Ok(Some(uri)) =
        blocking(move || read_cached_github_avatar_data_uri(&cached_owner)).await
    {
        return Some(uri);
    }

    if running_in_headless_test()
        || failed_owners()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .contains(&clean)
    {
        return None;
    }

    let _permit = fetch_semaphore().acquire().await.ok()?;

    if let Some(uri) = peek_github_avatar_data_uri(&clean) {
        return Some(uri);
    }
    if failed_owners()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .contains(&clean)
    {
        return None;
    }

    let Some(bytes) = fetch_github_avatar_bytes(&clean).await else {
        failed_owners()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(clean);
        return None;
    };

    let Some(uri) = image_bytes_to_data_uri(&bytes) else {
        failed_owners()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(clean);
        return None;
    };

    store_in_uri_cache(&clean, &uri);
    let write_owner = clean.clone();
    let _ = blocking(move || write_cached_github_avatar(&write_owner, &bytes)).await;
    Some(uri)
}

#[cfg(test)]
mod tests {
    use super::{
        image_bytes_to_data_uri, peek_github_avatar_data_uri, read_cached_github_avatar_data_uri,
        sanitize_github_owner,
    };

    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-repo-avatars-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
            Self { _lock: lock, previous, root }
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
    fn sanitize_accepts_github_handles_and_rejects_traversal() {
        assert_eq!(sanitize_github_owner("@octocat").as_deref(), Some("octocat"));
        assert_eq!(
            sanitize_github_owner("  vercel-labs ").as_deref(),
            Some("vercel-labs")
        );
        assert_eq!(sanitize_github_owner("a_b-1").as_deref(), Some("a_b-1"));
        assert_eq!(sanitize_github_owner(""), None);
        assert_eq!(sanitize_github_owner("-leading-dash"), None);
        assert_eq!(sanitize_github_owner("../secret"), None);
        assert_eq!(sanitize_github_owner("a/b"), None);
        assert_eq!(sanitize_github_owner(&"a".repeat(40)), None);
    }

    #[test]
    fn png_and_jpeg_round_trip_through_disk_and_memory_caches() {
        let _iso = IsolatedDataDir::new();
        let avatars_dir = ss_core::infra::paths::cache_dir().join("avatars");
        std::fs::create_dir_all(&avatars_dir).unwrap();

        std::fs::write(avatars_dir.join("png-owner.png"), crate::APP_ICON_PNG).unwrap();
        let png_uri = read_cached_github_avatar_data_uri("png-owner").expect("png cached uri");
        assert!(png_uri.starts_with("data:image/png;base64,"));
        assert_eq!(
            peek_github_avatar_data_uri("png-owner").as_deref(),
            Some(png_uri.as_str())
        );

        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.resize(64, 0xaa);
        std::fs::write(avatars_dir.join("jpeg-owner.png"), &jpeg).unwrap();
        let jpeg_uri = read_cached_github_avatar_data_uri("jpeg-owner").expect("jpeg cached uri");
        assert!(jpeg_uri.starts_with("data:image/jpeg;base64,"));
        assert_eq!(
            peek_github_avatar_data_uri("jpeg-owner").as_deref(),
            Some(jpeg_uri.as_str())
        );

        std::fs::write(avatars_dir.join("corrupt.png"), b"not-an-image-at-all-really-long").unwrap();
        assert_eq!(read_cached_github_avatar_data_uri("corrupt"), None);
        assert_eq!(image_bytes_to_data_uri(b"short"), None);
    }
}
