#!/usr/bin/env bash
# Anti-staleness guard for the Rust -> TypeScript generated MCP types.
#
# `src/types/generated/*.ts` is produced by ts-rs from
# `crates/skillstar-models/src/mcp/types.rs` (`bun run types:gen`, i.e.
# `cargo test -p skillstar-models export_bindings`). Nothing enforces that a
# developer who edits a `#[derive(TS)]` struct actually reruns and commits the
# generator, so this script regenerates into a scratch directory and diffs it
# against the committed output. Any difference means the committed bindings
# are stale relative to the Rust source.
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
# ts-rs resolves `export_to` relative to the test binary's CWD
# (crates/skillstar-models/), so this must be an absolute path.
echo "regenerating TS bindings into scratch dir..."
if ! TS_RS_EXPORT_DIR="$SCRATCH_DIR" cargo test -p skillstar-models export_bindings --quiet 2>&1; then
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
  echo "✓ $COMMITTED_DIR is up to date with crates/skillstar-models/src/mcp/types.rs."
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
