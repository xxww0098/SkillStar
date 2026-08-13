//! Remote transport for SkillStar: SSH remote skills.
//!
//! Wave 2B merged former `skillstar-ssh` into this crate. The former S3 cloud
//! sync (client/store/manifest/local_pack/sync/types) was removed — shared
//! channels on GitHub are the collaboration path, SSH remains the
//! per-machine deployment path (see docs/decisions.md).

pub mod ssh;

#[cfg(test)]
pub(crate) mod test_support {
    //! Tests across this crate mutate `SKILLSTAR_DATA_DIR` (to point the
    //! host stores at a temp dir). Those env mutations race when tests run
    //! in parallel, so every such test must hold this lock for its whole
    //! body — mirroring `skillstar_core::config::test_env_lock`.
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
