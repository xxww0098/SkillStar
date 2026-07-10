//! Remote transport for SkillStar: S3 cloud sync + SSH remote skills.
//!
//! Wave 2B merged former `skillstar-ssh` into this crate as [`ssh`].
//!
//! Layout:
//! - [`types`] / [`store`] / [`client`] / [`manifest`] / [`local_pack`] / [`sync`] — S3
//! - [`progress`] — S3 progress sink
//! - [`ssh`] — SSH hosts, SFTP, remote skill push/list/delete (Tauri-agnostic)

pub mod client;
pub mod ssh;
pub mod local_pack;
pub mod manifest;
pub mod progress;
pub mod store;
pub mod sync;
pub mod types;

pub use client::{build_client, test_connection, test_connection_quiet};
pub use progress::{
    NoopSink, Phase, ProgressSink, S3ProgressEvent, Status, event, event_with_detail,
};
pub use store::{KeyringSecretStore, SecretStore, TargetsStore, load_targets};

#[cfg(test)]
pub use store::MemSecretStore;
pub use sync::{
    pull_manifest, push_all, resolve_client, resolve_target, restore_entries,
};
pub use types::{
    ConnectionTestResult, InstallOutcome, InstallSummary, Manifest, ManifestEntry,
    ManifestEntryView, PushSummary, S3TargetDef,
};

#[cfg(test)]
pub(crate) mod test_support {
    //! Tests across this crate mutate `SKILLSTAR_DATA_DIR` (to point the
    //! target/device stores at a temp dir). Those env mutations race when
    //! tests run in parallel, so every such test must hold this lock for its
    //! whole body — mirroring `skillstar_core::config::test_env_lock`.
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
