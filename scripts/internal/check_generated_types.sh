#!/usr/bin/env bash
# Anti-staleness guard for the Rust -> TypeScript generated types.
#
# `src/types/generated/*.ts` is produced by ts-rs from five crates
# (`bun run types:gen`, i.e. `cargo test -p skillstar-models
# -p skillstar-marketplace -p skillstar-usage -p skillstar-app
# -p skillstar-decision export_bindings`).
# The source files below are the ones that currently carry `#[derive(TS)]`;
# the list is a navigation aid, not an SSOT — the authority is the derives
# themselves, and this script fails on any drift regardless of what is listed
# here:
#   - `crates/skillstar-models/src/providers/`. Note `providers/types.rs`
#     carries NO derives — it holds the historical
#     v1/v2/v3 shapes that only the migration reads, and `grep -n 'derive(.*TS'`
#     on it returns nothing. The provider types that DO reach the frontend are
#     `providers/{provider,binding,catalog,credential}.rs` (v4 wire shapes:
#     Endpoints, Tri, ProviderCaps, ModelRef, Effort, the catalog records) and
#     `providers/migrate/report.rs` (the migration report the UI shows once)
#   - `crates/skillstar-marketplace/src/snapshot/mod.rs` (LocalFirstResult,
#     SnapshotStatus, SyncStateEntry — the local-first read envelope every
#     marketplace command returns)
#   - `crates/skillstar-marketplace/src/remote/skill_details.rs`
#     (MarketplaceSkillDetails, SecurityAudit — the skill-detail payload)
#   - `crates/skillstar-usage/src/{catalog,subscription}.rs` (the usage
#     domain enums and the usage-snapshot tree the DTOs embed)
#   - `crates/skillstar-usage/src/instances/` (desktop multi-instance DTOs; moved from skillstar-app, D-077)
#   - `crates/skillstar-app/src/usage/dto.rs` (the /usage page's frontend
#     contract; `src/features/usage/types.ts` only re-exports it)
#   - `crates/skillstar-app/src/models/dto.rs` (the Models page's frontend
#     contract; `ProviderDto` exists so the plaintext API key on `Provider`
#     has nowhere to travel to)
#   - `crates/skillstar-decision/` (the local decision model's checkpoint
#     status, download progress, engine info and typed answers; `src/types/
#     decision.ts` re-exports them and hand-mirrors nothing)
# Nothing enforces that a developer who edits a `#[derive(TS)]` struct
# actually reruns and commits the generator, so this script regenerates into
# a scratch directory and diffs it against the committed output. Any
# difference means the committed bindings are stale relative to the Rust
# source.
#
# Usage: scripts/internal/check_generated_types.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

COMMITTED_DIR="src/types/generated"
SCRATCH_DIR="$(mktemp -d)"
trap 'rm -rf "$SCRATCH_DIR"' EXIT

# Override the .cargo/config.toml TS_RS_EXPORT_DIR for this run only — an
# env var already set at invocation time wins over the `[env]` table value,
# so this redirects ts-rs's output without touching the real generated/ dir.
# This must be an absolute path: ts-rs resolves `export_to` relative to
# whatever TS_RS_EXPORT_DIR holds, joined against the test binary's CWD only
# if that value is itself relative. The four packages here sit at different
# depths from the repo root (crates/skillstar-models/,
# crates/skillstar-marketplace/ and crates/skillstar-app/ are 2 levels deep;
# src-tauri/ is only 1), so a relative override would resolve
# inconsistently between them — an
# absolute path sidesteps that entirely. (See .cargo/config.toml for the
# same concern affecting the committed, non-override TS_RS_EXPORT_DIR.)
echo "regenerating TS bindings into scratch dir..."
if ! TS_RS_EXPORT_DIR="$SCRATCH_DIR" cargo test -p skillstar-models -p skillstar-marketplace -p skillstar-usage -p skillstar-app -p skillstar-decision export_bindings --quiet 2>&1; then
  echo "✗ ts-rs export_bindings tests failed to run — cannot verify freshness."
  exit 1
fi

if [ ! -d "$COMMITTED_DIR" ]; then
  echo "✗ $COMMITTED_DIR does not exist. Run 'bun run types:gen' and commit the output."
  exit 1
fi

# Compare file sets and contents. `diff -r` reports both missing/extra files
# and content differences in one pass.
if diff -r "$COMMITTED_DIR" "$SCRATCH_DIR" >/tmp/check_generated_types.diff 2>&1; then
  echo "✓ $COMMITTED_DIR is up to date with every #[derive(TS)] in skillstar-models, skillstar-marketplace, skillstar-usage, skillstar-app, and skillstar-decision."
  rm -f /tmp/check_generated_types.diff
  exit 0
fi

echo "✗ $COMMITTED_DIR is STALE relative to the Rust source."
echo ""
cat /tmp/check_generated_types.diff
rm -f /tmp/check_generated_types.diff
echo ""
echo "Run 'bun run types:gen' and commit the result under $COMMITTED_DIR."
exit 1
