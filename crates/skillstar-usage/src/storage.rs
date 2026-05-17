//! JSON-file persistence for subscriptions.
//!
//! Storage layout (under `~/.skillstar/config/usage/`):
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
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Coarse-grained mutex serializing all file writes. The volume is tiny so
/// a single global is fine; OAuth callback handlers may race otherwise.
static STORAGE_LOCK: Mutex<()> = Mutex::new(());

fn usage_dir() -> PathBuf {
    let dir = skillstar_core::infra::paths::config_dir().join("usage");
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

#[derive(Debug, Default, Serialize, Deserialize)]
struct SubscriptionsFile {
    #[serde(default)]
    subscriptions: Vec<Subscription>,
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

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> UsageResult<()> {
    let _guard = STORAGE_LOCK.lock().expect("storage lock poisoned");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(value)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw.as_bytes())?;
    fs::rename(&tmp, path)?;
    Ok(())
}

// ── Subscriptions ─────────────────────────────────────────────────────

pub fn list_subscriptions() -> UsageResult<Vec<Subscription>> {
    let file: SubscriptionsFile = read_json(&subscriptions_path())?;
    let mut subs = file.subscriptions;
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
    let mut subs = list_subscriptions()?;
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
    write_json(&subscriptions_path(), &SubscriptionsFile { subscriptions: subs })?;
    Ok(sub)
}

pub fn delete_subscription(id: &str) -> UsageResult<()> {
    let mut subs = list_subscriptions()?;
    let len_before = subs.len();
    subs.retain(|s| s.id != id);
    if subs.len() == len_before {
        return Err(UsageError::NotFound(id.to_string()));
    }
    write_json(&subscriptions_path(), &SubscriptionsFile { subscriptions: subs })?;
    // Also drop any cached usage snapshot.
    let mut snapshots = read_usage_snapshots_file()?;
    snapshots.snapshots.remove(id);
    write_json(&usage_snapshots_path(), &snapshots)?;
    Ok(())
}

/// Reorder subscriptions by the given id sequence. Ids missing from the slice
/// keep their existing sort_index but are pushed to the end.
pub fn reorder_subscriptions(ordered_ids: &[String]) -> UsageResult<()> {
    let mut subs = list_subscriptions()?;
    let now = Utc::now().timestamp();
    for (idx, id) in ordered_ids.iter().enumerate() {
        if let Some(s) = subs.iter_mut().find(|s| &s.id == id) {
            s.sort_index = idx as i32;
            s.updated_at = now;
        }
    }
    write_json(&subscriptions_path(), &SubscriptionsFile { subscriptions: subs })?;
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
    let mut file = read_usage_snapshots_file()?;
    file.snapshots.insert(usage.subscription_id.clone(), usage);
    write_json(&usage_snapshots_path(), &file)?;
    Ok(())
}

// ── Alert dismissals ──────────────────────────────────────────────────

pub fn dismissed_alert_ids() -> UsageResult<HashSet<String>> {
    let file: AlertsDismissedFile = read_json(&alerts_dismissed_path())?;
    Ok(file.dismissed)
}

pub fn dismiss_alert(alert_id: &str) -> UsageResult<()> {
    let mut file: AlertsDismissedFile = read_json(&alerts_dismissed_path())?;
    file.dismissed.insert(alert_id.to_string());
    write_json(&alerts_dismissed_path(), &file)?;
    Ok(())
}
