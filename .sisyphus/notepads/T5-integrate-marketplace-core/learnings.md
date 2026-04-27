# T5: Integrate existing marketplace-core — Learnings

## Observed Integration Pattern in This Repo

The codebase already follows a layered workspace pattern:

```
crates/                           # Workspace crates (published as separate packages)
  skillstar-core-types/  → shared types (Skill, SkillContent, Lockfile)
  skillstar-infra/       → cross-cutting infra (paths, db_pool, error, fs_ops)
  skillstar-config/      → user-editable config (proxy, github_mirror)

src-tauri/
  crates/
    marketplace-core/     → marketplace domain logic (db, remote, snapshot)
    skill-core/          → skill discovery (underway or planned)
    Markdown-Translator/  → translation pipeline

  src/core/
    marketplace.rs       → THIN wrapper: re-exports + wires infra paths into core
    marketplace_snapshot/ → THIN adapter: configures core with tauri-specific paths/callbacks
    skills/discover.rs   → THIN re-export
```

### Key patterns already established

1. **Re-export over duplication**: `core/marketplace.rs` re-exports types and functions from `skillstar_marketplace_core` rather than duplicating them.

2. **Thin adapters for wiring**: `core/marketplace_snapshot/mod.rs` calls `core_snapshot::configure_runtime(runtime_config())` to inject tauri-specific paths and callbacks. The domain logic stays in the crate.

3. **Dedicated types crate prevents duplication**: `skillstar-core-types` owns `Skill`, `SkillContent`, etc. Both `marketplace-core` and `src-tauri` depend on it, avoiding type duplication across the crate boundary.

4. **Domain crate has zero framework deps**: `marketplace-core` has no `tauri` dep. It uses `reqwest`, `rusqlite`, `serde` — pure infrastructure. Framework binding happens in the app crate.

### When to keep thin wrappers vs direct imports

- **Keep thin wrappers** when you need to inject app-specific context (paths, time, callbacks) into a generic domain function. The wrapper configures the domain function at startup.
- **Use direct imports** when the domain type/function is already parametric over the needed context (e.g., takes a path as arg).

### Avoiding duplicate types across crate boundary

The `skillstar-core-types` crate is the canonical definition. Any `use skillstar_core_types::Skill` in `src-tauri` is the same `Skill` used inside `marketplace-core`. No mirroring needed.

If T5 introduces new types that both the domain crate and app crate need, they go into `skillstar-core-types` or a new shared types crate — NOT into both.

### Verification strategy while adjacent extractions are in flight

Since `skill-core` and `Markdown-Translator` are being integrated in parallel with T5:

1. **Pin domain crate to a specific git commit or version** rather than `path` — ensures integration is tested against a stable surface, not moving code.
2. **Run `cargo build --workspace` frequently** — catches type mismatches early.
3. **Write integration tests in the domain crate itself** (under `tests/` or `#[cfg(test)]`) so the domain is validated before the app crate wires it.
4. **Verify thin adapter compiles but does nothing else**: if the wrapper just passes through to the domain crate, confirm the domain crate tests pass independently.
5. **Check `skillstar-core-types` compatibility** when adding new types — both `marketplace-core` and `skill-core` may need the same new type.

## Applicable Guidance

From Rust workspace best practices research (2026):

- **Dependency rule**: `domain ← infra ← api ← app`. Your domain crates (`marketplace-core`, `skill-core`) should depend only on `skillstar-core-types` (types layer). They must NOT depend on `skillstar-infra` (infra layer).
- **Crate boundary enforcement**: Every `use skillstar_marketplace_core::*` in `src-tauri` should be reviewed — if it's accessing remote/network behavior directly rather than through a configured adapter, that's a leak.
- **When to split a crate**: Not every folder needs its own `Cargo.toml`. Use `mod.rs` within a crate for organization; split to a new crate only when the code has a clearly different rate of change or ownership boundary.
- **Integration test crate**: Consider a `tests/` directory inside `marketplace-core` that imports the crate and exercises public APIs without going through tauri commands.

## Verification Checklist

- [ ] `cargo build --workspace` succeeds without errors
- [ ] Domain crate (`marketplace-core`) compiles with `cargo check -p skillstar-marketplace-core` independently
- [ ] No duplicate type definitions (grep for `struct Skill` across both crate boundaries)
- [ ] Thin wrapper in `src-tauri/src/core/marketplace_snapshot/` only configures + delegates, contains no domain logic
- [ ] New types introduced by T5 land in `skillstar-core-types` or a dedicated types crate, not duplicated
- [ ] Integration tests in domain crate pass before app-level integration is attempted
- [ ] If `skill-core` is being wired simultaneously, confirm no type conflicts with `marketplace-core` on shared `skillstar-core-types` types
