/// Proof that a canonical Skill write comes from a shared-channel flow
/// (install-and-track, upgrade, rollback) for one channel repository.
///
/// The generic mutation gate refuses every write to a channel-managed Skill or
/// repository, which is exactly what those flows must do. Only code inside
/// `shared_channels` can construct this, so generic installers cannot borrow
/// it; the installer still refuses Skills another channel manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChannelInstallAuthority {
    repository_id: u64,
}

impl ChannelInstallAuthority {
    pub(super) fn for_repository(repository_id: u64) -> Self {
        Self { repository_id }
    }

    pub(crate) fn repository_id(&self) -> u64 {
        self.repository_id
    }
}
