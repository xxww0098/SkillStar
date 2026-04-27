# T14: Extract skillstar-projects crate

## Context

Existing workspace crates:
- skillstar-infra (fs_ops, paths, error)
- skillstar-git (gix ops)
- skillstar-config
- skillstar-skills (skill lifecycle)
- skillstar-ai, skillstar-model-config, skillstar-core-types

Target: extract `projects/` + `project_manifest/` into `skillstar-projects`.

## What to Extract

**Domain logic (extract to crate):**
- `core/projects/` — agents.rs (profile data table, builtin defs), sync.rs (profile caching, global skill link/unlink)
- `core/project_manifest/` — manifest_paths, helpers, types, mod.rs (all business logic)

**Keep in skillstar_lib (thin shims):**
- `core/mod.rs` re-exports
- `commands/projects.rs` — Tauri command handlers wrapping domain calls

## Dependency Wiring

```
skillstar-projects
├── skillstar-infra        (fs_ops, paths, error)
├── skillstar-skills       (local_skill::adopt_existing_dir, reconcile_hub_symlinks)
├── skillstar-config       (TOML prefs for agents)
└── skillstar-core-types   (for shared enums if needed)
```

Key imports to redirect:
- `crate::core::infra::{fs_ops, paths}` → `skillstar_infra::{fs_ops, paths}`
- `crate::core::projects::agents` → `skillstar_projects::agents`
- `crate::core::local_skill` → `skillstar_skills::local_skill`
- `crate::core::infra::error::AppError` → `skillstar_infra::error::AppError`

## Preserving Call Sites

`commands/projects.rs` currently imports from `crate::core::{infra::error::AppError, project_manifest, projects::sync}`.
After extraction, add re-export shims in `core/mod.rs`:
```rust
// Keep these re-exports so commands/ stays unchanged
pub use projects;              // thin shim over new crate
pub use project_manifest;      // thin shim over new crate
```

## What NOT to Extract (yet)

- `terminal_backend.rs`, `terminal/` — UI/runtime concerns, keep in app
- `patrol.rs` — runtime scheduling, keep in app
- `acp_client.rs` — ACP integration, keep in app

## Verification Checklist

1. `cargo check` in `crates/skillstar-projects` — compiles standalone
2. `cargo check` in `src-tauri` — no broken re-exports
3. `cargo test` in `src-tauri` — all project_manifest tests pass
4. Confirm commands/projects.rs unchanged (only import redirection via re-exports)
5. Confirm no new dependencies on `tauri`, `serde_json` (domain crate stays Tauri-free)
