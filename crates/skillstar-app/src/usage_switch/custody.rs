//! Symlink custody: the CLI's live credential path becomes a link to a
//! per-account snapshot, and the snapshot is the only copy.
//!
//! The thing SkillStar used to do — copy a credential into the CLI's live file
//! — creates a *second* copy of something the CLI also rotates, so the two
//! diverge by construction, and every lease / revision / read-back-compare /
//! rollback mechanism after that exists to manage divergence SkillStar caused.
//! Pointing the live path at the snapshot instead means the CLI's own rotation
//! writes straight through into the snapshot: it is never stale, and there is
//! nothing to reconcile back.
//!
//! What a symlink does **not** solve, and this module therefore still does:
//!
//! 1. **macOS Codex reads its keychain first.** A link to a file it never
//!    opens would be decorative, so [`CliCredentialTarget::external_root`] /
//!    [`CliCredentialTarget::publish_external`] carry that second store
//!    through the same transaction.
//! 2. **A CLI that writes with `rename()` replaces the link with a real file.**
//!    Reconciliation therefore compares *access tokens*, never file types
//!    ([`Custody::probe`]), and silently re-links when the content agrees.
//! 3. **Windows may refuse symlinks.** [`LinkMode::Copy`] is an explicit,
//!    reported degradation — never a silent change of meaning.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use skillstar_models::tool_sync::create_rolling_backup;
use skillstar_usage::storage;
use skillstar_usage::subscription::Subscription;

use super::error::{ActivationError, CustodyError, CustodyResult, Stage};
use super::target::{
    AccountIdentity, CliCredentialTarget, secret, subscription_expiry, subscription_identity,
};

/// Snapshots that belong to no subscription row: a login SkillStar found in
/// the CLI and could not attribute. Kept rather than discarded — it is the
/// user's session — but never reported as "this account is current".
const ORPHAN_PREFIX: &str = "external-";

/// How the live path is bound to the snapshot.
///
/// Carried out to the UI on [`super::SwitchOutcome`], because the two modes
/// do not mean the same thing to a user: under `Symlink` the CLI's own token
/// rotation lands in the snapshot, and under `Copy` it does not, so a switch
/// that silently degraded would leave someone believing in a pass-through
/// that isn't there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkMode {
    /// The live path is a symlink; the CLI writes straight into the snapshot.
    Symlink,
    /// Degraded: a byte copy, re-synced on every reconcile. Windows without
    /// developer mode. Reported, never silent.
    Copy,
}

impl LinkMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            LinkMode::Symlink => "symlink",
            LinkMode::Copy => "copy",
        }
    }
}

/// Which account the CLI is actually serving right now — the only honest
/// answer to "is this account current", and the reason the pin is a cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LinkState {
    /// Serving this subscription, whether through a link or a byte-identical
    /// real file.
    LinkedTo(String),
    /// Serving credentials that belong to nobody SkillStar knows.
    Diverged,
    /// Nobody is logged in.
    Missing,
}

/// Result of a successful activation.
pub(super) struct Activated {
    pub(super) backup: Option<PathBuf>,
    pub(super) mode: LinkMode,
    pub(super) external_updated: bool,
}

/// What the live path looked like before we touched it, so a failure after
/// the swap can put it back.
#[derive(Default)]
struct LivePrior {
    backup: Option<PathBuf>,
    linked_to: Option<String>,
    existed: bool,
}

pub(super) struct Custody {
    target: &'static dyn CliCredentialTarget,
    live: PathBuf,
    dir: PathBuf,
}

/// Held for the duration of a custody operation; also excludes the CLI itself
/// where the CLI has a lock we can share.
pub(super) struct CustodyLease {
    _file: fs::File,
}

impl Custody {
    pub(super) fn open(target: &'static dyn CliCredentialTarget) -> CustodyResult<Self> {
        let live = target.live_path()?;
        let dir = skillstar_core::infra::paths::data_root()
            .join("accounts")
            .join(target.catalog_id());
        Ok(Self { target, live, dir })
    }

    pub(super) fn live_path(&self) -> &Path {
        &self.live
    }

    pub(super) fn tool_id(&self) -> &'static str {
        self.target.tool_id()
    }

    /// Take the CLI's lock. Blocking; callers on an async path wrap this in
    /// `spawn_blocking`.
    pub(super) fn lock(&self) -> CustodyResult<CustodyLease> {
        let path = self.target.lock_path(&self.live);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| CustodyError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|source| CustodyError::OpenLock {
                path: path.clone(),
                source,
            })?;
        set_private_mode(&path)?;
        file.lock().map_err(|source| CustodyError::AcquireLock {
            path: path.clone(),
            source,
        })?;
        if self.target.lock_holder_line() {
            write_holder_line(&mut file)?;
        }
        Ok(CustodyLease { _file: file })
    }

    // ── snapshots ────────────────────────────────────────────────────────

    fn snapshot_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub(super) fn read_snapshot(&self, id: &str) -> Option<Value> {
        read_json(&self.snapshot_path(id))
    }

    pub(super) fn write_snapshot(&self, id: &str, root: &Value) -> CustodyResult<()> {
        fs::create_dir_all(&self.dir).map_err(|source| CustodyError::CreateDir {
            path: self.dir.clone(),
            source,
        })?;
        write_private_json(&self.snapshot_path(id), root)
    }

    /// Every snapshot in this catalog, as `(id, root)`.
    fn snapshots(&self) -> Vec<(String, Value)> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()? != "json" {
                    return None;
                }
                let id = path.file_stem()?.to_str()?.to_string();
                Some((id, read_json(&path)?))
            })
            .collect()
    }

    // ── the live path ────────────────────────────────────────────────────

    /// The file root the CLI would read, following the link. `None` when the
    /// path is absent, dangling, or not JSON.
    fn live_root(&self) -> Option<Value> {
        read_json(&self.live)
    }

    /// The root that actually governs the CLI's behaviour: on macOS Codex the
    /// keychain outranks the file it links to.
    fn authoritative_root(&self) -> Option<Value> {
        self.target
            .external_root(&self.live)
            .or_else(|| self.live_root())
    }

    fn link_target_id(&self) -> Option<String> {
        let target = fs::read_link(&self.live).ok()?;
        (target.parent() == Some(self.dir.as_path()))
            .then(|| target.file_stem()?.to_str().map(str::to_string))
            .flatten()
    }

    // ── reconciliation (§ compare content, never file type) ──────────────

    /// Whether this CLI has any credential to reconcile at all.
    ///
    /// Lets a read-only caller skip [`Self::lock`] for a CLI that is not
    /// installed — the lock would otherwise *create* that CLI's home directory
    /// and a lock file just to discover there is nothing in it.
    pub(super) fn has_live_credential(&self) -> bool {
        self.authoritative_root().is_some()
    }

    /// Who the CLI is serving. Pure: takes no lock, writes nothing.
    pub(super) fn probe(&self) -> CustodyResult<LinkState> {
        let Some(root) = self.authoritative_root() else {
            return Ok(LinkState::Missing);
        };
        match self.attribute(&root, None)? {
            // A file that parses but carries no credential is not divergence,
            // it is a logged-out CLI.
            None => Ok(LinkState::Missing),
            Some(id) if id.starts_with(ORPHAN_PREFIX) => Ok(LinkState::Diverged),
            Some(id) => Ok(LinkState::LinkedTo(id)),
        }
    }

    /// [`Self::probe`], plus the two repairs that keep custody true:
    /// absorb a credential the CLI rotated behind our back, and rebuild a link
    /// the CLI clobbered with a real file. Caller must hold [`Self::lock`].
    pub(super) fn reconcile(&self) -> CustodyResult<LinkState> {
        let Some(root) = self.authoritative_root() else {
            return Ok(LinkState::Missing);
        };
        let Some(id) = self.attribute(&root, None)? else {
            return Ok(LinkState::Missing);
        };
        self.absorb_into_snapshot(&id, &root)?;
        if id.starts_with(ORPHAN_PREFIX) {
            return Ok(LinkState::Diverged);
        }
        // Content already agrees; the link is just bookkeeping, so a failed
        // repair (a read-only home, say) must not fail the reconcile.
        if self.link_target_id().as_deref() != Some(id.as_str())
            && let Err(error) = self.bind(&id)
        {
            tracing::warn!(catalog = self.target.catalog_id(), %error, "relink after CLI clobber failed");
        }
        Ok(LinkState::LinkedTo(id))
    }

    /// Attribute a file root to an account.
    ///
    /// Token equality first (cheap and exact), then identity (survives the
    /// CLI rotating the token), then — only while activating — adoption by the
    /// account the user just named, which is how a `codex login` / `opencode
    /// auth login` done in the terminal gets captured instead of discarded.
    fn attribute(&self, root: &Value, adopt_for: Option<&str>) -> CustodyResult<Option<String>> {
        let Some(token) = self.target.access_token(root) else {
            return Ok(None);
        };
        for (id, snapshot) in self.snapshots() {
            if self.target.access_token(&snapshot).as_deref() == Some(token.as_str()) {
                return Ok(Some(id));
            }
        }

        let identity = self.target.identity(root);
        if !identity.is_empty() {
            let mut owners = self
                .catalog_subscriptions()?
                .into_iter()
                .filter(|sub| identity.matches(&subscription_identity(sub)));
            match (owners.next(), owners.next()) {
                (Some(owner), None) => return Ok(Some(owner.id)),
                // Fail closed rather than guess which row owns a session.
                (Some(_), Some(_)) => {
                    return Err(CustodyError::AmbiguousOwner {
                        tool: self.target.tool_id(),
                    });
                }
                _ => {}
            }
        }

        if let Some(candidate) = adopt_for
            && self.read_snapshot(candidate).is_none()
            && let Ok(sub) = storage::get_subscription(candidate)
            && !identity.conflicts(&subscription_identity(&sub))
        {
            return Ok(Some(candidate.to_string()));
        }

        // Nobody owns it, so keep it anyway: a session SkillStar cannot name
        // is still a session the user logged into.
        Ok(Some(orphan_id(&identity, &token)))
    }

    fn catalog_subscriptions(&self) -> CustodyResult<Vec<Subscription>> {
        Ok(storage::list_subscriptions()?
            .into_iter()
            .filter(|sub| sub.catalog_id == self.target.catalog_id())
            .collect())
    }

    /// Write `root` into `id`'s snapshot when it is at least as fresh, and
    /// mirror the credential back into the subscription row.
    ///
    /// The row half runs even when the snapshot needed no write: once the live
    /// path is a symlink, a CLI rotation lands *in* the snapshot without ever
    /// passing through SkillStar, so "snapshot already current" is the normal
    /// case — and the row is the only thing still holding a revoked token.
    fn absorb_into_snapshot(&self, id: &str, root: &Value) -> CustodyResult<()> {
        let observed = self.target.access_token(root);
        let previous = self.read_snapshot(id);
        let (needs_write, current) = match &previous {
            Some(previous) => {
                let same = self.target.access_token(previous) == observed;
                (
                    !same && self.root_outranks(root, previous),
                    same || self.root_outranks(root, previous),
                )
            }
            None => (true, true),
        };
        if needs_write {
            self.write_snapshot(id, root)?;
        }
        if !current {
            return Ok(());
        }
        if let Ok(mut sub) = storage::get_subscription(id) {
            let row_token = secret(sub.access_token_encrypted.as_deref())
                .or_else(|| secret(sub.api_key_encrypted.as_deref()));
            if row_token != observed {
                self.target.absorb(root, &mut sub);
                storage::patch_oauth_credentials(&sub)?;
            }
        }
        Ok(())
    }

    fn root_outranks(&self, candidate: &Value, incumbent: &Value) -> bool {
        match (
            self.target.expires_at(candidate),
            self.target.expires_at(incumbent),
        ) {
            (Some(new), Some(old)) => new >= old,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => true,
        }
    }

    // ── the three operations ─────────────────────────────────────────────

    /// Point the CLI at `sub`. Caller must hold [`Self::lock`].
    ///
    /// The pin is deliberately **not** written here: the caller sets it only
    /// after this returns `Ok`, so a failure leaves both the previous pin and
    /// the previous live file untouched.
    ///
    /// Failures are staged ([`Stage`]) rather than flattened, because the two
    /// halves need opposite treatment: everything up to [`Self::bind`] leaves
    /// the previous credential exactly where it was and must *not* be rolled
    /// back over, while everything from `bind` onwards may have left the CLI
    /// holding a half-applied switch and must be.
    pub(super) fn activate(&self, sub: &mut Subscription) -> Result<Activated, ActivationError> {
        let prior = self
            .capture_live(Some(&sub.id))
            .map_err(ActivationError::before_replace)?;

        let root = self
            .prepare_snapshot(sub)
            .map_err(ActivationError::before_replace)?;
        self.write_snapshot(&sub.id, &root)
            .map_err(ActivationError::before_replace)?;

        // Past this point the live path may no longer be what it was.
        let installed = self
            .bind(&sub.id)
            // A failed bind never reached the second store, so a rollback has
            // only the file side to put back.
            .map_err(|reason| ActivationError::after_replace(reason, false))
            .and_then(|mode| {
                self.finish_activation(sub, &root)
                    .map(|external_updated| (mode, external_updated))
                    .map_err(|reason| ActivationError::after_replace(reason, true))
            });
        match installed {
            Ok((mode, external_updated)) => {
                self.target.absorb(&root, sub);
                Ok(Activated {
                    backup: prior.backup,
                    mode,
                    external_updated,
                })
            }
            Err(error) => {
                if error.stage.needs_rollback() {
                    self.rollback(&prior, error.stage);
                }
                Err(error)
            }
        }
    }

    /// The side store, then read-back verification — as one step, so either
    /// failure rolls the live path back.
    ///
    /// Order matters: where a second store exists it *outranks* the file
    /// ([`Self::authoritative_root`]), so verifying before publishing would
    /// read the account we are switching away from and fail every switch.
    fn finish_activation(&self, sub: &Subscription, root: &Value) -> CustodyResult<bool> {
        let external_updated = self.target.publish_external(&self.live, root)?;
        match self.probe()? {
            LinkState::LinkedTo(id) if id == sub.id => Ok(external_updated),
            observed => Err(CustodyError::ReadBackMismatch {
                tool: self.target.tool_id(),
                observed,
            }),
        }
    }

    /// The snapshot this activation should install: the captured one unless
    /// the SkillStar row genuinely holds something newer.
    fn prepare_snapshot(&self, sub: &Subscription) -> CustodyResult<Value> {
        let existing = self.read_snapshot(&sub.id);
        let Some(snapshot) = existing else {
            // Nothing captured yet: build one, seeded with whatever the CLI
            // has so sibling providers / legacy blocks survive the swap.
            return self
                .target
                .materialize(sub, self.live_root().as_ref())
                .map_err(CustodyError::from);
        };
        let row_token = secret(sub.access_token_encrypted.as_deref())
            .or_else(|| secret(sub.api_key_encrypted.as_deref()));
        let snapshot_token = self.target.access_token(&snapshot);
        if row_token.is_none() || row_token == snapshot_token {
            return Ok(snapshot);
        }
        if !row_outranks(subscription_expiry(sub), self.target.expires_at(&snapshot)) {
            return Ok(snapshot);
        }
        // A rebuild that cannot be produced is not a reason to fail the
        // switch — the captured session still works.
        Ok(self
            .target
            .materialize(sub, Some(&snapshot))
            .unwrap_or(snapshot))
    }

    /// Write a generation SkillStar just refreshed into the snapshot, which
    /// *is* the live file. Caller must hold [`Self::lock`].
    ///
    /// Refuses when the CLI is serving somebody else: the pin is a cache, and
    /// overwriting another account's live session because a cache disagreed is
    /// exactly the failure custody exists to prevent.
    pub(super) fn project_refreshed(&self, sub: &Subscription) -> CustodyResult<Activated> {
        let state = self.probe()?;
        let serving = matches!(&state, LinkState::LinkedTo(id) if id == &sub.id);
        if !serving && state != LinkState::Missing {
            return Err(CustodyError::ServingAnotherAccount {
                tool: self.target.tool_id(),
            });
        }
        let base = self.read_snapshot(&sub.id).or_else(|| self.live_root());
        let root = self.target.materialize(sub, base.as_ref())?;
        self.write_snapshot(&sub.id, &root)?;
        let mode = if self.link_target_id().as_deref() == Some(sub.id.as_str()) {
            LinkMode::Symlink
        } else {
            self.bind(&sub.id)?
        };
        Ok(Activated {
            backup: None,
            mode,
            external_updated: self.target.publish_external(&self.live, &root)?,
        })
    }

    /// Absorb whatever the CLI is holding before we replace it.
    fn capture_live(&self, adopt_for: Option<&str>) -> CustodyResult<LivePrior> {
        if let Some(id) = self.link_target_id() {
            return Ok(LivePrior {
                backup: None,
                linked_to: Some(id),
                existed: true,
            });
        }
        let Some(root) = self.live_root() else {
            return Ok(LivePrior {
                existed: self.live.symlink_metadata().is_ok(),
                ..LivePrior::default()
            });
        };
        let backup = create_rolling_backup(&self.live).ok().inspect(|path| {
            let _ = set_private_mode(path);
        });
        if let Some(id) = self.attribute(&root, adopt_for)? {
            self.absorb_into_snapshot(&id, &root)?;
        }
        Ok(LivePrior {
            backup,
            linked_to: None,
            existed: true,
        })
    }

    /// Drop `id`'s snapshot. If the CLI is currently served by it, leave a
    /// real file behind first — otherwise the link dangles and the user cannot
    /// log in at all.
    pub(super) fn forget(&self, id: &str) -> CustodyResult<()> {
        let serving = matches!(self.probe()?, LinkState::LinkedTo(active) if active == id);
        if serving && let Some(root) = self.read_snapshot(id) {
            self.write_live_copy(&root)?;
        }
        let path = self.snapshot_path(id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(CustodyError::Remove { path, source }),
        }
    }

    // ── binding the live path ────────────────────────────────────────────

    /// Make the live path serve `id`'s snapshot.
    fn bind(&self, id: &str) -> CustodyResult<LinkMode> {
        let snapshot = self.snapshot_path(id);
        if let Some(parent) = self.live.parent() {
            fs::create_dir_all(parent).map_err(|source| CustodyError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        match self.symlink_atomically(&snapshot) {
            Ok(()) => Ok(LinkMode::Symlink),
            Err(error) => {
                // Explicit, reported degradation: the caller surfaces
                // `LinkMode::Copy`, and reconcile keeps the two sides in step.
                // `denied` separates the expected Windows-without-developer-mode
                // refusal from a symlink that failed for a real reason.
                tracing::warn!(
                    catalog = self.target.catalog_id(),
                    denied = error.is_symlink_denied(),
                    %error,
                    "symlink unavailable; falling back to a synced copy"
                );
                let root = read_json(&snapshot).ok_or(CustodyError::SnapshotUnreadable {
                    path: snapshot.clone(),
                })?;
                self.write_live_copy(&root)?;
                Ok(LinkMode::Copy)
            }
        }
    }

    fn symlink_atomically(&self, snapshot: &Path) -> CustodyResult<()> {
        let mut name = self.live.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".skillstar-{}.link", std::process::id()));
        let tmp = self.live.with_file_name(name);
        let _ = fs::remove_file(&tmp);
        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(snapshot, &tmp);
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_file(snapshot, &tmp);
        created.map_err(|source| CustodyError::Symlink { source })?;
        // rename() replaces the directory entry, so the old real file or link
        // never briefly disappears — the CLI can always open something.
        fs::rename(&tmp, &self.live).map_err(|source| {
            let _ = fs::remove_file(&tmp);
            CustodyError::ReplaceLive {
                path: self.live.clone(),
                source,
            }
        })
    }

    /// Write the live path as a real, private file. Replaces a link, because
    /// `atomic_write` finishes with `rename()` — which swaps the directory
    /// entry, not the symlink's target.
    fn write_live_copy(&self, root: &Value) -> CustodyResult<()> {
        write_private_json(&self.live, root)
    }

    /// Undo a half-applied activation. `stage` is the whole input: it is the
    /// caller's evidence that there is anything to undo at all, and whether
    /// the second store was ever reached.
    fn rollback(&self, prior: &LivePrior, stage: Stage) {
        let restored: CustodyResult<Option<Value>> = match (&prior.linked_to, &prior.backup) {
            (Some(id), _) => self.bind(id).map(|_| self.read_snapshot(id)),
            (None, Some(backup)) => read_json(backup)
                .ok_or_else(|| CustodyError::BackupUnreadable {
                    path: backup.clone(),
                })
                .and_then(|root| self.write_live_copy(&root).map(|()| Some(root))),
            // Nothing was there before, so "restore" means remove — and a bind
            // that failed before creating anything leaves nothing to remove.
            (None, None) if !prior.existed => match fs::remove_file(&self.live) {
                Ok(()) => Ok(None),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(source) => Err(CustodyError::Remove {
                    path: self.live.clone(),
                    source,
                }),
            },
            (None, None) => Ok(None),
        };
        match restored {
            // Putting the file back is not enough where a second store
            // outranks it: an un-rolled-back keychain would keep serving the
            // half-applied switch. When the bind itself failed the second
            // store was never written, so rewriting it here would be a
            // keychain round-trip that undoes nothing.
            Ok(Some(root)) if stage.target_installed() => {
                if let Err(error) = self.target.publish_external(&self.live, &root) {
                    tracing::error!(
                        catalog = self.target.catalog_id(),
                        %error,
                        "restored the CLI credential file but not its keychain mirror"
                    );
                }
            }
            Ok(_) => {}
            Err(error) => tracing::error!(
                catalog = self.target.catalog_id(),
                %error,
                "failed to restore the previous CLI credential file"
            ),
        }
    }
}

/// The subscription row's credential outranks the snapshot's.
///
/// Undated on both sides (plain API keys) resolves to the row: the user just
/// asked for *this* account, and there is no clock to argue with.
fn row_outranks(row: Option<i64>, snapshot: Option<i64>) -> bool {
    match (row, snapshot) {
        (Some(row), Some(snapshot)) => row > snapshot,
        (Some(_), None) | (None, None) => true,
        (None, Some(_)) => false,
    }
}

/// A stable file name for an unattributable session. Keyed on identity where
/// the credential has one, so the CLI rotating its token keeps updating the
/// same orphan file instead of littering a new one per rotation.
fn orphan_id(identity: &AccountIdentity, token: &str) -> String {
    use sha2::{Digest, Sha256};
    let seed = identity
        .subject()
        .or_else(|| identity.email())
        .unwrap_or(token);
    let digest = Sha256::digest(seed.as_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{ORPHAN_PREFIX}{hex}")
}

/// Atomically write a credential file that is 0600 *before* it holds bytes.
///
/// `atomic_write` inherits the target's existing mode, which is right for an
/// established file and wrong for a new one: `File::create` yields 0644, so a
/// first-ever credential would exist world-readable for the length of one
/// rename. Pre-creating at 0600 closes that window instead of chmod-ing after.
fn write_private_json(path: &Path, root: &Value) -> CustodyResult<()> {
    let body =
        serde_json::to_vec_pretty(root).map_err(|source| CustodyError::Serialize { source })?;
    if !path.exists() {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(path)
            .map_err(|source| CustodyError::CreateFile {
                path: path.to_path_buf(),
                source,
            })?;
    }
    skillstar_core::infra::fs_ops::atomic_write(path, &body).map_err(|source| {
        CustodyError::Write {
            path: path.to_path_buf(),
            source,
        }
    })?;
    set_private_mode(path)
}

fn read_json(path: &Path) -> Option<Value> {
    let raw = fs::read(path).ok()?;
    let value: Value = serde_json::from_slice(&raw).ok()?;
    value.is_object().then_some(value)
}

fn set_private_mode(path: &Path) -> CustodyResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
            CustodyError::SetMode {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    let _ = path;
    Ok(())
}

/// Grok's stale-lock recovery reads `PID:unix_seconds` and would otherwise
/// mistake this live lease for a dead owner and recreate the lock inode.
fn write_holder_line(file: &mut fs::File) -> CustodyResult<()> {
    use std::io::{Seek, SeekFrom, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    let step = |action: &'static str| move |source| CustodyError::LockHolder { action, source };

    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    file.set_len(0).map_err(step("清空锁文件"))?;
    file.seek(SeekFrom::Start(0)).map_err(step("定位锁文件"))?;
    write!(file, "{}:{seconds}", std::process::id()).map_err(step("写入锁持有者"))?;
    file.sync_data().map_err(step("同步锁文件"))
}
