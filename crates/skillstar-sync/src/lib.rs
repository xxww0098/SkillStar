//! S3 cloud sync for SkillStar.
//!
//! Mirrors `skillstar_ssh`'s two-tier structure: all logic lives in this crate
//! (Tauri-agnostic), the command layer in `src-tauri` is a thin forwarder.
//!
//! Layout:
//! - [`types`] — serialisable data model (`S3TargetDef`, `Manifest`, …)
//! - [`store`] — target-config TOML persistence + system-keyring credential store
//! - [`client`] — aws-sdk-s3 client construction + connection test
//! - [`manifest`] — cloud manifest build / parse / annotate
//! - [`local_pack`] — local skill tar.gz pack / unpack
//! - [`progress`] — Tauri-agnostic progress sink (forked from skillstar-ssh)
//! - [`sync`] — push_all / pull_manifest / restore_entries orchestration

pub mod client;
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
mod test_support {
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
