//! Backend-owned user configuration for SkillStar.

pub mod github_health;
pub mod github_mirror;
pub mod github_rewrite;
pub mod network_doctor;
pub mod proxy;
pub mod skill_updates;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
