//! Remote skill operations over SFTP, split by operation:
//!
//! - [`common`] — shared SFTP plumbing (open session, `mkdir -p`, file IO)
//! - [`list`]   — discovery + listing (read-only)
//! - [`push`]   — upload a local skill tree to a remote agent dir
//! - [`delete`] — recursive, guarded delete of a remote skill dir
//!
//! The public surface is re-exported here so callers keep using
//! `skillstar_ssh::sftp::*` regardless of which submodule a symbol lives in.

mod common;
mod delete;
mod list;
mod push;

pub use common::{ensure_remote_dir_pub, open_sftp, read_remote_file, write_remote_file};
pub use delete::delete_remote_skill;
pub use list::{
    DiscoveryResult, KNOWN_AGENT_SKILL_DIRS, RemoteAgentDir, RemoteAgentSkills,
    discover_remote_skills, list_remote_skills,
};
pub use push::{PushResult, push_skill, upload_local_skill_tree};
