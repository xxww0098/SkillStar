//! Where accounts live on disk.
//!
//! secrets/accounts/accounts.json holds the index (metadata only, no secrets);
//! secrets/accounts/records/<id>.json holds one record with its encrypted
//! secrets. Writes are atomic, 0600, and serialised by
//! runtime/locks/accounts/storage.lock.
//!
//! The index is a cache of the records, never the truth: when the two disagree
//! the records win and the index is rebuilt from them. That is what makes a
//! crash between "record written" and "index written" harmless, and what makes
//! a hand-edited or half-deleted index self-healing.
//!
//! This is a deliberate break from the old layout (secrets/accounts/usage,
//! secrets/accounts/cli): the old files stay on disk untouched and are never
//! read.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use crate::usage::UsageSnapshot;

use crate::error::{AccountError, AccountResult};
use crate::record::Account;

/// The index file's name.
pub const INDEX_FILE: &str = "accounts.json";

/// The directory records live in.
pub const RECORDS_DIR: &str = "records";

/// How long a writer waits for another SkillStar process to release the store.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// How often a blocked writer re-tries the file lock.
const LOCK_POLL: Duration = Duration::from_millis(20);

/// The accounts root directory.
pub fn root() -> PathBuf {
    ss_core::infra::paths::secrets_dir().join("accounts")
}

/// The index path.
pub fn index_path() -> PathBuf {
    root().join(INDEX_FILE)
}

/// The record directory.
pub fn records_dir() -> PathBuf {
    root().join(RECORDS_DIR)
}

/// One record's path.
pub fn record_path(id: &str) -> PathBuf {
    records_dir().join(format!("{id}.json"))
}

/// The storage lock file, opened for locking.
pub fn lock() -> AccountResult<std::fs::File> {
    let dir = ss_core::infra::paths::runtime_locks_dir().join("accounts");
    std::fs::create_dir_all(&dir)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("storage.lock"))?;
    Ok(file)
}

/// Exclusive hold on the accounts store.
///
/// One hold per operation, never nested: every public function takes the lock,
/// every private helper assumes it. A second hold in the same process would
/// block on the OS file lock, so acquisition gives up after LOCK_WAIT instead
/// of hanging the account page.
pub struct StorageLock {
    file: std::fs::File,
}

impl StorageLock {
    /// Wait for the store lock.
    pub fn acquire() -> AccountResult<Self> {
        let file = wait_for_lock(lock()?, "accounts storage")?;
        Ok(Self { file })
    }
}

impl Drop for StorageLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// The per-agent refresh lock file (catalog-<agent>.lock).
pub fn refresh_lock_path(agent: &str) -> PathBuf {
    ss_core::infra::paths::runtime_locks_dir()
        .join("accounts")
        .join(format!("catalog-{agent}.lock"))
}

/// One refresh per agent at a time.
///
/// Two concurrent refreshes of the same account would both spend the same
/// refresh token; the loser would store a token the provider already rotated
/// away, and the account would need a new login for no reason.
pub struct AgentRefreshLock {
    file: std::fs::File,
}

impl AgentRefreshLock {
    /// Wait for the agent's refresh turn.
    pub fn acquire(agent: &str) -> AccountResult<Self> {
        let path = refresh_lock_path(agent);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        let file = wait_for_lock(
            file,
            &format!("the {} refresh (another SkillStar window is refreshing it)", agent),
        )?;
        Ok(Self { file })
    }
}

impl Drop for AgentRefreshLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Take an exclusive OS lock on an open file, or give up after LOCK_WAIT.
fn wait_for_lock(file: std::fs::File, label: &str) -> AccountResult<std::fs::File> {
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() >= deadline => {
                return Err(AccountError::Client(format!("{label} is busy")));
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(LOCK_POLL),
            Err(std::fs::TryLockError::Error(error)) => return Err(AccountError::Io(error)),
        }
    }
}

/// The index entry the list view renders before opening records.
///
/// Metadata only: it carries everything a card shows and everything identity
/// matching needs, and never a token or a token-derived secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub id: String,
    pub agent: String,
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    pub created_at: i64,
    #[serde(default)]
    pub last_used_at: Option<i64>,
    #[serde(default)]
    pub requires_reauth: bool,
    #[serde(default)]
    pub usage: Option<UsageSnapshot>,
    /// Cached non-secret token prefixes so live matching skips record reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_fingerprint: Option<String>,
}

impl IndexEntry {
    /// Project one record into its cheap metadata.
    pub fn from_account(account: &Account) -> Self {
        Self {
            id: account.id.clone(),
            agent: account.agent.clone(),
            display_name: account.display_name.clone(),
            email: account.email.clone(),
            plan: account.plan.clone(),
            account_id: account.account_id.clone(),
            created_at: account.created_at,
            last_used_at: account.last_used_at,
            requires_reauth: account.requires_reauth,
            usage: account.usage.clone(),
            access_fingerprint: account.access_fingerprint(),
            refresh_fingerprint: account.refresh_fingerprint(),
        }
    }

    /// Whether a live identity from a client names this row.
    pub fn matches_identity(&self, identity: &str) -> bool {
        crate::identity::same_identity(Some(identity), self.account_id.as_deref())
            || crate::identity::same_identity(Some(identity), self.email.as_deref())
    }
}

// ---------------------------------------------------------------------------
// Public API: every entry point takes the lock, then uses the helpers below.
// ---------------------------------------------------------------------------

/// The index, rebuilt from the records when it disagrees with them.
pub fn index() -> AccountResult<Vec<IndexEntry>> {
    let _guard = StorageLock::acquire()?;
    ensure_index_locked()
}

/// Ensure the index is fresh, purge any retired agent records under the same
/// lock, and return supported entries (oldest first) without touching records
/// in steady state.
pub(crate) fn index_supported() -> AccountResult<Vec<IndexEntry>> {
    let _guard = StorageLock::acquire()?;
    let mut entries = ensure_index_locked()?;
    if entries
        .iter()
        .any(|entry| crate::Agent::from_id(&entry.agent).is_err())
    {
        purge_unsupported_entries_locked(&mut entries)?;
    }
    Ok(entries)
}

/// Every stored account, oldest first, read from the records (the truth).
pub fn list() -> AccountResult<Vec<Account>> {
    let _guard = StorageLock::acquire()?;
    let accounts = read_all_records_locked()?;
    let entries: Vec<IndexEntry> = accounts.iter().map(IndexEntry::from_account).collect();
    if read_index_locked().as_deref() != Some(entries.as_slice()) {
        write_index_locked(&entries)?;
    }
    Ok(accounts)
}

/// One account by id.
pub fn get(id: &str) -> AccountResult<Account> {
    let _guard = StorageLock::acquire()?;
    read_record_locked(id)
}

/// Insert or replace one account, and keep the index in step.
pub fn upsert(account: &Account) -> AccountResult<()> {
    let _guard = StorageLock::acquire()?;
    write_record_locked(account)?;
    sync_index_locked(Some(account))
}

/// Apply a narrow change to one record under the lock.
///
/// The closure sees the record as it is on disk right now, so a refresh can
/// rewrite tokens and usage without clobbering a rename the user did between
/// the read and the write.
pub fn update<F>(id: &str, apply: F) -> AccountResult<Account>
where
    F: FnOnce(&mut Account) -> AccountResult<()>,
{
    let _guard = StorageLock::acquire()?;
    let mut account = read_record_locked(id)?;
    apply(&mut account)?;
    write_record_locked(&account)?;
    sync_index_locked(Some(&account))?;
    Ok(account)
}

/// Insert one account, replacing the row that already holds the same identity
/// (same agent, same email or provider account id) instead of adding a second
/// card for one login.
///
/// The replacement keeps the old row's id, creation time and last-used time so
/// nothing the page shows jumps around, and it keeps the old display name: a
/// user may have renamed the row, and a re-login must not undo that.
pub fn upsert_replacing_identity(account: Account) -> AccountResult<Account> {
    let _guard = StorageLock::acquire()?;
    let mut incoming = account;
    if let Some(existing) = find_same_identity_locked(&incoming)? {
        incoming.id = existing.id.clone();
        incoming.created_at = existing.created_at;
        incoming.last_used_at = existing.last_used_at;
        let existing_trimmed = existing.display_name.trim();
        let existing_is_placeholder = existing_trimmed.is_empty()
            || existing_trimmed.eq_ignore_ascii_case(&existing.agent)
            || existing.email.as_deref().is_some_and(|email| {
                existing_trimmed.eq_ignore_ascii_case(email.trim())
            });
        let incoming_name = incoming.display_name.trim();
        let incoming_is_real =
            !incoming_name.is_empty() && !incoming_name.eq_ignore_ascii_case(&incoming.agent);
        if (!existing_is_placeholder || !incoming_is_real) && !existing_trimmed.is_empty() {
            incoming.display_name = existing.display_name;
        }
        if incoming.email.is_none() {
            incoming.email = existing.email;
        }
        if incoming.account_id.is_none() {
            incoming.account_id = existing.account_id;
        }
    }
    write_record_locked(&incoming)?;
    sync_index_locked(Some(&incoming))?;
    Ok(incoming)
}

/// Remove one account (the client is left as it is).
pub fn remove(id: &str) -> AccountResult<()> {
    let _guard = StorageLock::acquire()?;
    let path = record_path(id);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AccountError::Io(error)),
    }
    sync_index_locked(None)
}

// ---------------------------------------------------------------------------
// Locked helpers: no function here takes the lock again.
// ---------------------------------------------------------------------------

fn ensure_index_locked() -> AccountResult<Vec<IndexEntry>> {
    match read_index_locked() {
        Some(mut entries) => {
            if same_ids(&entries, &record_ids_locked()) {
                backfill_fingerprints_locked(&mut entries)?;
                sort_entries(&mut entries);
                return Ok(entries);
            }
            rebuild_index_locked()
        }
        None => rebuild_index_locked(),
    }
}

fn backfill_fingerprints_locked(entries: &mut [IndexEntry]) -> AccountResult<()> {
    if entries
        .iter()
        .all(|entry| entry.access_fingerprint.is_some())
    {
        return Ok(());
    }
    let mut changed = false;
    for entry in entries.iter_mut() {
        if entry.access_fingerprint.is_some() {
            continue;
        }
        if let Ok(account) = read_record_locked(&entry.id) {
            let next = IndexEntry::from_account(&account);
            if next != *entry {
                *entry = next;
                changed = true;
            }
        }
    }
    if changed {
        write_index_locked(entries)?;
    }
    Ok(())
}

fn rebuild_index_locked() -> AccountResult<Vec<IndexEntry>> {
    let accounts = read_all_records_locked()?;
    let entries: Vec<IndexEntry> = accounts.iter().map(IndexEntry::from_account).collect();
    write_index_locked(&entries)?;
    Ok(entries)
}

/// Keep the index in step after a record write. A touched record is patched
/// into an otherwise consistent index; any disagreement rebuilds from records.
fn sync_index_locked(touched: Option<&Account>) -> AccountResult<()> {
    let ids = record_ids_locked();
    if let Some(mut entries) = read_index_locked()
        && same_ids(&entries, &ids)
    {
        let Some(account) = touched else {
            return Ok(());
        };
        let entry = IndexEntry::from_account(account);
        match entries.iter_mut().find(|candidate| candidate.id == entry.id) {
            Some(slot) => *slot = entry,
            None => entries.push(entry),
        }
        sort_entries(&mut entries);
        return write_index_locked(&entries);
    }
    rebuild_index_locked().map(|_| ())
}

fn purge_unsupported_entries_locked(entries: &mut Vec<IndexEntry>) -> AccountResult<Vec<String>> {
    let mut removed = Vec::new();
    let mut failure = None;
    for entry in entries.iter() {
        if crate::Agent::from_id(&entry.agent).is_ok() {
            continue;
        }
        match std::fs::remove_file(record_path(&entry.id)) {
            Ok(()) => removed.push(entry.id.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    if !removed.is_empty() {
        tracing::info!(
            target: "accounts",
            removed = removed.len(),
            "deleted account records of clients this build no longer manages"
        );
        entries.retain(|entry| !removed.iter().any(|id| id == &entry.id));
        write_index_locked(entries)?;
    }
    match failure {
        Some(error) => Err(AccountError::Io(error)),
        None => Ok(removed),
    }
}

fn find_same_identity_locked(incoming: &Account) -> AccountResult<Option<Account>> {
    let accounts = read_all_records_locked()?;
    Ok(accounts.into_iter().find(|candidate| {
        candidate.agent == incoming.agent
            && (crate::identity::same_identity(
                candidate.email.as_deref(),
                incoming.email.as_deref(),
            ) || crate::identity::same_identity(
                candidate.account_id.as_deref(),
                incoming.account_id.as_deref(),
            ))
    }))
}

fn read_index_locked() -> Option<Vec<IndexEntry>> {
    let body = std::fs::read(index_path()).ok()?;
    match serde_json::from_slice::<Vec<IndexEntry>>(&body) {
        Ok(entries) => Some(entries),
        Err(error) => {
            tracing::warn!(
                target: "accounts",
                error = %error,
                "accounts index is unreadable; rebuilding it from the records"
            );
            None
        }
    }
}

fn write_index_locked(entries: &[IndexEntry]) -> AccountResult<()> {
    let mut sorted = entries.to_vec();
    sort_entries(&mut sorted);
    let body = serde_json::to_vec_pretty(&sorted)?;
    let path = index_path();
    ss_core::infra::fs_ops::atomic_write(&path, &body)?;
    restrict_permissions(&path)?;
    Ok(())
}

fn record_ids_locked() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(records_dir()) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    ids.sort();
    ids
}

fn read_record_locked(id: &str) -> AccountResult<Account> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(AccountError::NotFound(id.to_string()));
    }
    let path = record_path(id);
    let body = match std::fs::read(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(AccountError::NotFound(id.to_string()));
        }
        Err(error) => return Err(AccountError::Io(error)),
    };
    let account: Account = serde_json::from_slice(&body)?;
    if account.id != id {
        return Err(AccountError::Client(format!(
            "record {} claims to be {}",
            path.display(),
            account.id
        )));
    }
    Ok(account)
}

fn read_all_records_locked() -> AccountResult<Vec<Account>> {
    let mut accounts = Vec::new();
    for id in record_ids_locked() {
        match read_record_locked(&id) {
            Ok(account) => accounts.push(account),
            Err(error) => tracing::warn!(
                target: "accounts",
                account = %id,
                error = %error,
                "skipping unreadable account record"
            ),
        }
    }
    sort_accounts(&mut accounts);
    Ok(accounts)
}

fn write_record_locked(account: &Account) -> AccountResult<()> {
    std::fs::create_dir_all(records_dir())?;
    let body = serde_json::to_vec_pretty(account)?;
    let path = record_path(&account.id);
    ss_core::infra::fs_ops::atomic_write(&path, &body)?;
    restrict_permissions(&path)?;
    Ok(())
}

fn same_ids(entries: &[IndexEntry], ids: &[String]) -> bool {
    let mut entry_ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    entry_ids.sort_unstable();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    entry_ids == ids
}

fn sort_entries(entries: &mut [IndexEntry]) {
    entries.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn sort_accounts(accounts: &mut [Account]) {
    accounts.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

/// Sealed files stay 0600 on Unix; Windows inherits the profile ACL.
fn restrict_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;
