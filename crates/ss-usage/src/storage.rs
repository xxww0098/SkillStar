//! JSON-file persistence for subscriptions.
//!
//! Storage layout (under `~/.skillstar/secrets/accounts/usage/`, D-087 —
//! subscriptions carry encrypted tokens, so the whole store is secrets):
//! ```text
//! ├── subscriptions.json     # list of Subscription
//! ├── usage_snapshots.json   # subscription_id → SubscriptionUsage
//! └── alerts_dismissed.json  # Set<alert_id>
//! ```
//!
//! All file I/O is synchronous; callers should wrap in `spawn_blocking` if
//! invoked from an async hot path. For 18-row datasets the overhead is
//! negligible.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Coarse-grained in-process mutex paired with an OS file lock. The volume is
/// tiny, and the pair prevents OAuth callbacks and a second SkillStar process
/// from losing each other's read-modify-write updates.
static STORAGE_LOCK: Mutex<()> = Mutex::new(());

struct StorageWriteGuard {
    _process: MutexGuard<'static, ()>,
    _file: File,
}

fn storage_write_guard() -> UsageResult<StorageWriteGuard> {
    let process = STORAGE_LOCK
        .lock()
        .map_err(|_| UsageError::Other("usage storage lock poisoned".into()))?;
    let path = ss_core::infra::paths::usage_storage_lock_path();
    // The v3 lock lives under runtime/locks/ which the startup migration
    // pre-creates in production; sandboxes (tests, CLI-first runs) may not
    // have it yet.
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    file.lock()?;
    Ok(StorageWriteGuard {
        _process: process,
        _file: file,
    })
}

fn usage_dir() -> PathBuf {
    let dir = ss_core::infra::paths::usage_store_dir();
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

fn subscriptions_path() -> PathBuf {
    usage_dir().join("subscriptions.json")
}

fn usage_snapshots_path() -> PathBuf {
    usage_dir().join("usage_snapshots.json")
}

fn alerts_dismissed_path() -> PathBuf {
    usage_dir().join("alerts_dismissed.json")
}

fn active_per_catalog_path() -> PathBuf {
    usage_dir().join("active_per_catalog.json")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SubscriptionsFile {
    #[serde(default)]
    subscriptions: Vec<Subscription>,
}

/// One-shot catalog id renames applied when loading subscription storage, so
/// rows written before a rebrand keep their catalog identity instead of being
/// dropped by the unknown-catalog retention filter. Values map persisted
/// catalog id → current catalog id.
const CATALOG_ID_RENAMES: &[(&str, &str)] = &[("windsurf", "devin-desktop")];

fn current_catalog_id(catalog_id: &str) -> &str {
    CATALOG_ID_RENAMES
        .iter()
        .find(|(old, _)| *old == catalog_id)
        .map(|(_, new)| *new)
        .unwrap_or(catalog_id)
}

/// Read subscriptions with catalog id renames applied; persists the rewritten
/// file when a rename actually happened. Callers hold `storage_write_guard`.
fn read_subscriptions_file(path: &PathBuf) -> UsageResult<SubscriptionsFile> {
    let mut file: SubscriptionsFile = read_json(path)?;
    let mut changed = false;
    for sub in &mut file.subscriptions {
        let renamed = current_catalog_id(&sub.catalog_id);
        if renamed != sub.catalog_id {
            sub.catalog_id = renamed.to_string();
            changed = true;
        }
    }
    if changed {
        write_json_unlocked(path, &file)?;
    }
    Ok(file)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UsageSnapshotsFile {
    #[serde(default)]
    snapshots: HashMap<String, SubscriptionUsage>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AlertsDismissedFile {
    #[serde(default)]
    dismissed: HashSet<String>,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> UsageResult<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(T::default());
    }
    Ok(serde_json::from_str(&raw)?)
}

/// Pretty-print JSON and replace `path` via `fs_ops::atomic_write`
/// (same-directory temp file, fsync, rename). Subscription callers hold
/// `storage_write_guard` first; other JSON files can call this directly.
pub(crate) fn write_json_unlocked<T: Serialize>(path: &Path, value: &T) -> UsageResult<()> {
    let raw = serde_json::to_string_pretty(value)?;
    ss_core::infra::fs_ops::atomic_write(path, raw.as_bytes())?;
    Ok(())
}

// ── Subscriptions ─────────────────────────────────────────────────────

pub fn list_subscriptions() -> UsageResult<Vec<Subscription>> {
    let _guard = storage_write_guard()?;
    let path = subscriptions_path();
    let file = read_subscriptions_file(&path)?;
    let mut subs = file.subscriptions;
    // Removed families remain on disk for recovery; reads never destroy credentials.
    subs.retain(|s| crate::catalog::find(&s.catalog_id).is_some());
    subs.sort_by_key(|s| s.sort_index);
    Ok(subs)
}

pub fn get_subscription(id: &str) -> UsageResult<Subscription> {
    list_subscriptions()?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| UsageError::NotFound(id.to_string()))
}

pub fn upsert_subscription(mut sub: Subscription) -> UsageResult<Subscription> {
    // Keep the full read-modify-write under the same lock used by narrow
    // credential patches. Otherwise an OAuth callback could read a stale row,
    // wait for a switch patch to finish, then overwrite the rotated token.
    let _guard = storage_write_guard()?;
    let path = subscriptions_path();
    let mut file = read_subscriptions_file(&path)?;
    let subs = &mut file.subscriptions;
    let now = Utc::now().timestamp();
    if let Some(existing) = subs.iter_mut().find(|s| s.id == sub.id) {
        sub.created_at = existing.created_at;
        sub.updated_at = now;
        *existing = sub.clone();
    } else {
        if sub.created_at == 0 {
            sub.created_at = now;
        }
        sub.updated_at = now;
        if sub.sort_index == 0 {
            sub.sort_index = subs
                .iter()
                .map(|s| s.sort_index)
                .max()
                .map(|m| m + 1)
                .unwrap_or(0);
        }
        subs.push(sub.clone());
    }
    write_json_unlocked(&path, &file)?;
    Ok(sub)
}

/// Persist only OAuth credential fields from `source` onto its stored row.
///
/// Account switching and token refresh can race with edits to display/note/
/// sort metadata. Replacing the whole [`Subscription`] from a stale clone
/// would lose those unrelated changes, so this patch performs one locked
/// read-modify-write and deliberately leaves every non-credential field
/// untouched.
pub fn patch_oauth_credentials(source: &Subscription) -> UsageResult<Subscription> {
    let _guard = storage_write_guard()?;
    let path = subscriptions_path();
    let mut file = read_subscriptions_file(&path)?;
    let target = file
        .subscriptions
        .iter_mut()
        .find(|sub| sub.id == source.id)
        .ok_or_else(|| UsageError::NotFound(source.id.clone()))?;

    apply_oauth_credentials(target, source);
    target.updated_at = Utc::now().timestamp();
    let saved = target.clone();

    write_json_unlocked(&path, &file)?;
    Ok(saved)
}

/// Persist only fields a fetcher is allowed to rotate or normalize.
///
/// A refresh can spend seconds on the network. Writing its stale full row
/// afterwards would undo concurrent note/price/order edits, so refresh callers
/// use this locked patch instead of [`upsert_subscription`].
pub fn patch_fetcher_state(source: &Subscription) -> UsageResult<Subscription> {
    let _guard = storage_write_guard()?;
    let path = subscriptions_path();
    let mut file = read_subscriptions_file(&path)?;
    let target = file
        .subscriptions
        .iter_mut()
        .find(|sub| sub.id == source.id)
        .ok_or_else(|| UsageError::NotFound(source.id.clone()))?;

    apply_fetcher_state(target, source);
    target.updated_at = Utc::now().timestamp();
    let saved = target.clone();

    write_json_unlocked(&path, &file)?;
    Ok(saved)
}

fn apply_fetcher_state(target: &mut Subscription, source: &Subscription) {
    apply_oauth_credentials(target, source);
    target.display_name = source.display_name.clone();
    target.note = source.note.clone();
    target.cookie_session_expires_at = source.cookie_session_expires_at;
}

fn apply_oauth_credentials(target: &mut Subscription, source: &Subscription) {
    target.access_token_encrypted = source.access_token_encrypted.clone();
    target.refresh_token_encrypted = source.refresh_token_encrypted.clone();
    target.access_token_expires_at = source.access_token_expires_at;
    target.id_token_encrypted = source.id_token_encrypted.clone();
    target.oauth_account_id = source.oauth_account_id.clone();
    target.requires_reauth = source.requires_reauth;
    target.provider_state_encrypted = source.provider_state_encrypted.clone();
}

pub fn delete_subscription(id: &str) -> UsageResult<()> {
    let _guard = storage_write_guard()?;
    let subscriptions_path = subscriptions_path();
    let mut subscriptions = read_subscriptions_file(&subscriptions_path)?;
    let len_before = subscriptions.subscriptions.len();
    subscriptions.subscriptions.retain(|s| s.id != id);
    if subscriptions.subscriptions.len() == len_before {
        return Err(UsageError::NotFound(id.to_string()));
    }
    write_json_unlocked(&subscriptions_path, &subscriptions)?;

    let mut snapshots = read_usage_snapshots_file()?;
    snapshots.snapshots.remove(id);
    write_json_unlocked(&usage_snapshots_path(), &snapshots)?;

    let mut active = read_active_per_catalog_file()?;
    active.active.retain(|_, sub_id| sub_id != id);
    write_json_unlocked(&active_per_catalog_path(), &active)?;
    Ok(())
}

/// Reorder subscriptions by the given id sequence. Ids missing from the slice
/// keep their existing sort_index but are pushed to the end.
pub fn reorder_subscriptions(ordered_ids: &[String]) -> UsageResult<()> {
    let _guard = storage_write_guard()?;
    let path = subscriptions_path();
    let mut file = read_subscriptions_file(&path)?;
    let now = Utc::now().timestamp();
    for (idx, id) in ordered_ids.iter().enumerate() {
        if let Some(s) = file.subscriptions.iter_mut().find(|s| &s.id == id) {
            s.sort_index = idx as i32;
            s.updated_at = now;
        }
    }
    write_json_unlocked(&path, &file)?;
    Ok(())
}

// ── Usage snapshots ───────────────────────────────────────────────────

fn read_usage_snapshots_file() -> UsageResult<UsageSnapshotsFile> {
    read_json(&usage_snapshots_path())
}

pub fn get_usage_snapshot(id: &str) -> UsageResult<Option<SubscriptionUsage>> {
    Ok(read_usage_snapshots_file()?.snapshots.remove(id))
}

pub fn list_usage_snapshots() -> UsageResult<HashMap<String, SubscriptionUsage>> {
    Ok(read_usage_snapshots_file()?.snapshots)
}

pub fn save_usage_snapshot(usage: SubscriptionUsage) -> UsageResult<()> {
    let _guard = storage_write_guard()?;
    let path = usage_snapshots_path();
    let mut file: UsageSnapshotsFile = read_json(&path)?;
    file.snapshots.insert(usage.subscription_id.clone(), usage);
    write_json_unlocked(&path, &file)?;
    Ok(())
}

pub fn delete_usage_snapshot(subscription_id: &str) -> UsageResult<()> {
    let _guard = storage_write_guard()?;
    let path = usage_snapshots_path();
    let mut file: UsageSnapshotsFile = read_json(&path)?;
    if file.snapshots.remove(subscription_id).is_some() {
        write_json_unlocked(&path, &file)?;
    }
    Ok(())
}

// ── Alert dismissals ──────────────────────────────────────────────────

pub fn dismissed_alert_ids() -> UsageResult<HashSet<String>> {
    let file: AlertsDismissedFile = read_json(&alerts_dismissed_path())?;
    Ok(file.dismissed)
}

pub fn dismiss_alert(alert_id: &str) -> UsageResult<()> {
    let _guard = storage_write_guard()?;
    let path = alerts_dismissed_path();
    let mut file: AlertsDismissedFile = read_json(&path)?;
    file.dismissed.insert(alert_id.to_string());
    write_json_unlocked(&path, &file)?;
    Ok(())
}

// ── Active subscription per catalog (Phase 7 — multi-account) ─────────

/// `catalog_id → subscription_id` map persisted to
/// `~/.skillstar/secrets/accounts/usage/active_per_catalog.json`.
///
/// SkillStar lets the user maintain multiple subscriptions sharing the
/// same `catalog_id` (e.g. two DeepSeek accounts). This map records which
/// one is currently "the active one" for that catalog — used by the UI to
/// render an active-account badge on the right card.
///
/// For CLI-backed catalogs this map is a **cache**, not a second source of
/// truth: which account the CLI actually serves is a property of the CLI's
/// own credential file, and `crate::usage_switch` can rebuild this
/// map from disk at any time (see D-033). It is only written after a switch
/// has been verified, so a rejected switch leaves the previous pin standing.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ActivePerCatalogFile {
    #[serde(default)]
    active: HashMap<String, String>,
}

fn read_active_per_catalog_file() -> UsageResult<ActivePerCatalogFile> {
    let path = active_per_catalog_path();
    let mut file: ActivePerCatalogFile = read_json(&path)?;
    let stale_keys: Vec<String> = file
        .active
        .keys()
        .filter(|key| current_catalog_id(key) != key.as_str())
        .cloned()
        .collect();
    if stale_keys.is_empty() {
        return Ok(file);
    }
    for old in stale_keys {
        if let Some(value) = file.active.remove(&old) {
            let new = current_catalog_id(&old).to_string();
            file.active.entry(new).or_insert(value);
        }
    }
    write_json_unlocked(&path, &file)?;
    Ok(file)
}

/// All `catalog_id → subscription_id` bindings currently set.
pub fn list_active_per_catalog() -> UsageResult<HashMap<String, String>> {
    Ok(read_active_per_catalog_file()?.active)
}

/// Resolve a single catalog's active subscription id, or `None` if no
/// preference is set.
pub fn get_active_subscription(catalog_id: &str) -> UsageResult<Option<String>> {
    Ok(read_active_per_catalog_file()?.active.remove(catalog_id))
}

/// Pin `subscription_id` as the active account for `catalog_id`. Verifies
/// that the subscription exists and actually belongs to this catalog so
/// the store can't get out-of-sync with reality.
pub fn set_active_subscription(catalog_id: &str, subscription_id: &str) -> UsageResult<()> {
    // Validate + patch the shared map under one storage lock. Activations for
    // two different catalogs use different refresh guards and may run in
    // parallel; a split read/write here would let one erase the other's pin.
    let _guard = storage_write_guard()?;
    let subscriptions = read_subscriptions_file(&subscriptions_path())?;
    let sub = subscriptions
        .subscriptions
        .iter()
        .find(|sub| sub.id == subscription_id)
        .ok_or_else(|| UsageError::NotFound(subscription_id.to_string()))?;
    if sub.catalog_id != catalog_id {
        return Err(UsageError::Other(format!(
            "订阅 {} 的 catalog 是 {}，不匹配 {}",
            subscription_id, sub.catalog_id, catalog_id
        )));
    }
    let mut file = read_active_per_catalog_file()?;
    file.active
        .insert(catalog_id.to_string(), subscription_id.to_string());
    write_json_unlocked(&active_per_catalog_path(), &file)?;
    Ok(())
}

/// Drop the binding for `catalog_id` (UI defaults back to nothing pinned).
pub fn clear_active_subscription(catalog_id: &str) -> UsageResult<()> {
    let _guard = storage_write_guard()?;
    let mut file = read_active_per_catalog_file()?;
    if file.active.remove(catalog_id).is_some() {
        write_json_unlocked(&active_per_catalog_path(), &file)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AuthMode;
    use crate::subscription::BillingCycle;

    #[test]
    fn legacy_windsurf_catalog_rows_migrate_to_devin_desktop() {
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = crate::test_support::EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);
        let path = subscriptions_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("usage dir");
        let mut row = subscription("windsurf-ada");
        row.catalog_id = "windsurf".into();
        write_json_unlocked(
            &path,
            &SubscriptionsFile {
                subscriptions: vec![row],
            },
        )
        .expect("seed");

        let subs = list_subscriptions().expect("list");

        assert_eq!(subs.len(), 1, "renamed row must survive the catalog filter");
        assert_eq!(subs[0].catalog_id, "devin-desktop");
        let raw = fs::read_to_string(&path).expect("reread");
        assert!(raw.contains("devin-desktop"));
        assert!(
            !raw.contains("\"windsurf\""),
            "rewrite must persist the rename"
        );
    }

    #[test]
    fn legacy_windsurf_pin_migrates_to_devin_desktop() {
        let temp = tempfile::tempdir().expect("tempdir");
        let _env = crate::test_support::EnvGuard::set(&[("SKILLSTAR_DATA_DIR", temp.path())]);
        let path = active_per_catalog_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("usage dir");
        write_json_unlocked(
            &path,
            &ActivePerCatalogFile {
                active: HashMap::from([("windsurf".into(), "windsurf-ada".into())]),
            },
        )
        .expect("seed");

        let active = list_active_per_catalog().expect("list");

        assert_eq!(
            active.get("devin-desktop").map(String::as_str),
            Some("windsurf-ada")
        );
        assert!(!active.contains_key("windsurf"));
    }

    fn subscription(id: &str) -> Subscription {
        Subscription {
            id: id.into(),
            catalog_id: "xai".into(),
            display_name: "alice@example.com".into(),
            auth_mode: AuthMode::OAuth,
            plan_tier: None,
            monthly_price: None,
            currency: "USD".into(),
            billing_cycle: BillingCycle::Monthly,
            start_date: 0,
            renew_date: 0,
            auto_renew: false,
            api_key_encrypted: None,
            platform_token_encrypted: None,
            access_token_encrypted: Some("old-access".into()),
            refresh_token_encrypted: Some("old-refresh".into()),
            access_token_expires_at: Some(1),
            id_token_encrypted: None,
            oauth_account_id: Some("old-user".into()),
            oauth_region: None,
            requires_reauth: false,
            provider_state_encrypted: Some("old-blob".into()),
            cookie_jar_encrypted: None,
            cookie_session_expires_at: None,
            manual_quota: None,
            note: Some("keep this metadata".into()),
            sort_index: 7,
            created_at: 10,
            updated_at: 20,
        }
    }

    #[test]
    fn oauth_credential_patch_keeps_unrelated_subscription_metadata() {
        let mut stored = subscription("sub-1");
        let mut refreshed = stored.clone();
        refreshed.display_name = "stale title".into();
        refreshed.note = Some("stale note".into());
        refreshed.sort_index = 99;
        refreshed.access_token_encrypted = Some("new-access".into());
        refreshed.refresh_token_encrypted = Some("new-refresh".into());
        refreshed.access_token_expires_at = Some(999);
        refreshed.oauth_account_id = Some("new-user".into());
        refreshed.requires_reauth = true;
        refreshed.provider_state_encrypted = Some("rotated-blob".into());

        apply_oauth_credentials(&mut stored, &refreshed);

        assert_eq!(stored.access_token_encrypted.as_deref(), Some("new-access"));
        assert_eq!(
            stored.refresh_token_encrypted.as_deref(),
            Some("new-refresh")
        );
        assert_eq!(stored.access_token_expires_at, Some(999));
        assert_eq!(stored.oauth_account_id.as_deref(), Some("new-user"));
        assert!(stored.requires_reauth);
        assert_eq!(
            stored.provider_state_encrypted.as_deref(),
            Some("rotated-blob")
        );
        assert_eq!(stored.display_name, "alice@example.com");
        assert_eq!(stored.note.as_deref(), Some("keep this metadata"));
        assert_eq!(stored.sort_index, 7);
    }

    #[test]
    fn fetcher_patch_updates_runtime_state_without_replacing_billing_or_order() {
        let mut stored = subscription("sub-1");
        stored.monthly_price = Some(20.0);
        let mut refreshed = stored.clone();
        refreshed.access_token_encrypted = Some("new-access".into());
        refreshed.display_name = "normalized@example.com".into();
        refreshed.note = Some("fetcher-project-id".into());
        refreshed.cookie_session_expires_at = Some(999);
        refreshed.monthly_price = Some(999.0);
        refreshed.sort_index = 99;
        refreshed.provider_state_encrypted = Some("fetcher-blob".into());

        apply_fetcher_state(&mut stored, &refreshed);

        assert_eq!(stored.access_token_encrypted.as_deref(), Some("new-access"));
        assert_eq!(stored.display_name, "normalized@example.com");
        assert_eq!(stored.note.as_deref(), Some("fetcher-project-id"));
        assert_eq!(stored.cookie_session_expires_at, Some(999));
        assert_eq!(
            stored.provider_state_encrypted.as_deref(),
            Some("fetcher-blob")
        );
        assert_eq!(stored.monthly_price, Some(20.0));
        assert_eq!(stored.sort_index, 7);
    }
}
