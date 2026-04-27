# skillstar-security-scan: decisions

## Decision 1: Crate name and path

- **Crate name**: `skillstar-scan` (avoids name collision with `scan` stdlib, clear domain)
- **Path**: `src-tauri/crates/skillstar-scan/`
- **Version**: `0.1.0` to match the workspace schema

## Decision 2: Dependencies on existing extracted crates

- `skillstar-scan` depends on: `skillstar-ai`, `skillstar-model-config`, `skillstar-core-types`
- `skillstar-scan` does NOT depend on `skillstar-infra` (avoids circular dep)
- `skillstar-scan` manages its own SQLite cache directly via `rusqlite` (owns `~/.skillstar/db/scan_cache.db`)
- `skillstar-scan` does NOT depend on `skillstar-skills` (scan is orthogonal to skill lifecycle management)

## Decision 3: What stays in app crate (thin compatibility shims)

- `security_scan/mod.rs` stays in app crate temporarily during transition, re-exports from `skillstar-scan`
- Tauri commands in `commands/` that call `security_scan` functions stay in app crate
- Path resolution shim: `infra::paths` functions that forward to `skillstar-scan` path helpers
- Event name constants (`ai://scan-stream`, etc.) stay in app crate command layer

## Decision 4: Prompt resources

- Prompts currently loaded at compile time from `src-tauri/prompts/security/`
- Move prompts into `skillstar-scan/src/prompts/` as `include_str!` data
- Do NOT make prompts loadable from arbitrary paths (keeps crate self-contained)

## Decision 5: No feature flags for analyzers

- All built-in analyzers (pattern, doc_consistency, secrets, semantic, dynamic, semgrep, trivy, osv, grype, gitleaks, shellcheck, bandit, sbom, virustotal) are compiled in
- The `enabled_analyzers` list in policy controls which run at runtime, not which are compiled
- Eliminates complexity of feature-gated compilation

## Decision 6: Path configuration

- Scan crate accepts a `ScanConfig { data_root: PathBuf }` struct
- Data root defaults to `dirs::data_dir().join("skillstar")` if not set
- All scan-specific paths (logs/, policy.yaml, scan_cache.db) are computed relative to data_root
- App crate creates and passes the config at scan initialization time

## Decision 7: Verification approach

- Unit tests in `skillstar-scan` for orchestrator, types, policy, and analyzer logic
- Integration tests use a temp skill directory fixture (like ` tempfile::tempdir()`)
- App crate integration: verify `cargo check --manifest-path src-tauri/Cargo.toml` passes with new dependency
- Existing `cargo test` suite in app crate should not break

## Update: chat_completion visibility confirmed

`chat_completion` and `chat_completion_capped` are `pub async fn` at `skillstar_ai::ai_provider::`. The security scan code uses `super::ai_provider` (i.e., from `core/ai_provider`), which maps to `skillstar_ai::ai_provider::` when consumed as a crate dependency.

**Confirmed**: `skillstar-scan` can depend on `skillstar-ai` and use `ai_provider::chat_completion` directly. No compatibility shim needed.
