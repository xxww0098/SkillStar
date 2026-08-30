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
pub mod hub_scripts;
pub mod progress;
pub mod remote_fs;
pub mod sftp;
pub mod store;
pub mod system_config;
pub mod types;

pub use client::{ConnectionTestResult, HostKeyState};
pub use hub::MigrateResult;
pub use progress::{
    NoopSink, Phase, ProgressSink, SshProgressEvent, Status, event, event_with_detail,
};
pub use sftp::{
    DiscoveryResult, KNOWN_AGENT_SKILL_DIRS, PushResult, RemoteAgentDir, RemoteAgentSkills,
    discover_remote_skills,
};
pub use store::HostsStore;
pub use system_config::{find_host_by_alias, parse_system_hosts};
pub use types::{AuthMethod, KnownHost, RemoteSkill, RemoteSkillLayout, SshHostDef, SystemHost};

/// The russh session handle returned by [`client::connect`].
pub type Session = russh::client::Handle<client::SshHandler>;
