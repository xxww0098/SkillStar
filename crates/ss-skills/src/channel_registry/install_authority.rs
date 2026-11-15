//! Proof that an install request belongs to one shared channel.
//!
//! Only the channel's own install/update/rollback flows construct this token,
//! and they pass it into the staged repository installer so the generic
//! mutation gate stays closed while one channel updates its own Skills without
//! touching another channel's.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelInstallAuthority {
    repository_id: u64,
}

impl ChannelInstallAuthority {
    pub fn for_repository(repository_id: u64) -> Self {
        Self { repository_id }
    }

    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }
}
