//! SSH remote skill management for SkillStar.
//!
//! Connects to remote servers (russh + SFTP) so users can push locally
//! installed skills to a remote agent directory and manage remote skills.
//!
//! Layout:
//! - [`types`] — serialisable data model (`SshHostDef`, `RemoteSkill`, …)
//! - [`store`] — host-config TOML persistence + system-keyring credential store
//! - [`client`] — russh connection + known_hosts TOFU + auth
//! - [`sftp`] — push / list / delete remote skills over SFTP
//!
//! This crate is Tauri-agnostic; the command layer in `src-tauri` is a thin
//! forwarder (mirroring `commands/agents.rs`).

pub mod client;
pub mod hub;
pub mod progress;
pub mod remote_fs;
pub mod sftp;
pub mod store;
pub mod system_config;
pub mod types;

pub use client::{ConnectionTestResult, HostKeyState};
pub use progress::{NoopSink, Phase, ProgressSink, Status, SshProgressEvent, event, event_with_detail};
pub use sftp::{
    KNOWN_AGENT_SKILL_DIRS, DiscoveryResult, PushResult, RemoteAgentDir, RemoteAgentSkills,
    discover_remote_skills, read_remote_file, write_remote_file,
};
pub use hub::MigrateResult;
pub use types::{AuthMethod, RemoteSkill, RemoteSkillLayout, SshHostDef, SystemHost};
pub use store::HostsStore;
pub use system_config::{find_host_by_alias, parse_system_hosts};

/// The russh session handle returned by [`client::connect`].
pub type Session = russh::client::Handle<client::SshHandler>;

#[cfg(test)]
mod test_support {
    //! Tests across this crate mutate `SKILLSTAR_DATA_DIR` (to point the
    //! host/known-hosts stores at a temp dir). Those env mutations race when
    //! tests run in parallel, so every such test must hold this lock for its
    //! whole body — mirroring `skillstar_core::config::test_env_lock`.
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
