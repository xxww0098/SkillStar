#!/usr/bin/env bash
# Anti-staleness guard for the Rust -> TypeScript generated types.
#
# `src/types/generated/*.ts` is produced by ts-rs from five crates
# (`bun run types:gen`, i.e. `cargo test -p skillstar-models
# -p skillstar-marketplace -p skillstar-usage -p skillstar-app -p skillstar
# export_bindings`).
# The source files below are the ones that currently carry `#[derive(TS)]`;
# the list is a navigation aid, not an SSOT — the authority is the derives
# themselves, and this script fails on any drift regardless of what is listed
# here:
#   - `crates/skillstar-models/src/mcp/types.rs` (unified MCP store types)
#     and `crates/skillstar-models/src/providers/types.rs` (ProviderPreset)
#   - `crates/skillstar-marketplace/src/mcp_models/` (MCP registry/
#     marketplace types) and `src/mcp_remote/`, `src/mcp_snapshot/filters.rs`
#     (catalog sources and the parameterized card query)
#   - `crates/skillstar-marketplace/src/snapshot/mod.rs` (LocalFirstResult,
#     SnapshotStatus, SyncStateEntry — the local-first read envelope every
#     marketplace command returns)
#   - `crates/skillstar-marketplace/src/remote/skill_details.rs`
#     (MarketplaceSkillDetails, SecurityAudit — the skill-detail payload)
#   - `crates/skillstar-usage/src/{catalog,subscription}.rs` (the usage
#     domain enums and the usage-snapshot tree the DTOs embed)
#   - `crates/skillstar-app/src/mcp/` (the MCP cross-domain use cases:
#     runtime-shape candidates and the pre-install confirmation plan)
#   - `crates/skillstar-app/src/usage/dto.rs` (the /usage page's frontend
#     contract; `src/features/usage/types.ts` only re-exports it)
#   - `src-tauri/src/commands/mcp_commands.rs` (McpServerWithSync — the
#     Tauri-command-layer DTO wrapping a synced server; package name is
#     `skillstar`, not `src-tauri`, since that's what its Cargo.toml declares)
# Nothing enforces that a developer who edits a `#[derive(TS)]` struct
# actually reruns and commits the generator, so this script regenerates into
# a scratch directory and diffs it against the committed output. Any
# difference means the committed bindings are stale relative to the Rust
# source.
#
# Perf note: `skillstar` (src-tauri) is by far the heaviest compile unit in
# the workspace — it's the Tauri binary and links nearly every other crate.
# Measured locally: `-p skillstar-models -p skillstar-marketplace` alone
# reruns in ~15s warm; adding `-p skillstar` roughly doubles-to-triples
# that (~40s warm, ~23s for a from-scratch rebuild of just the skillstar
# package with its deps already cached). If this materially hurts your
# local edit-generate-check loop, prefer running the three lighter crates'
# `export_bindings` directly while iterating on MCP/marketplace types, and
# only run the full four-package command (this script, and CI) before
# committing.
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
if ! TS_RS_EXPORT_DIR="$SCRATCH_DIR" cargo test -p skillstar-models -p skillstar-marketplace -p skillstar-usage -p skillstar-app -p skillstar export_bindings --quiet 2>&1; then
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
  echo "✓ $COMMITTED_DIR is up to date with every #[derive(TS)] in skillstar-models, skillstar-marketplace, skillstar-usage, skillstar-app, and skillstar (src-tauri)."
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
