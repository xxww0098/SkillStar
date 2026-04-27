# T14 Issues & Gotchas

- `projects/mod.rs` has `pub use super::project_manifest as manifest` — becomes a re-export of the new crate
- `agents.rs` calls `crate::core::infra::paths::home_dir()` and `crate::core::infra::paths::profiles_config_path()` — wired through infra
- `project_manifest/mod.rs` imports `crate::core::{local_skill, projects::agents}` — these become external crate deps
- `sync.rs` has `PROFILE_CACHE_TTL` and `OnceLock` profile cache — fine to extract, no app-specific state
- Test helpers in `project_manifest/mod.rs` use `crate::core::lock_test_env()` — this test infra stays in app crate
