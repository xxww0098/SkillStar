#!/usr/bin/env bash
# Structural-governance guard: run `cargo clippy` over the whole workspace and
# fail if the number of individual diagnostics grows beyond the baseline.
#
# Counting method (documented here because there are two plausible ones):
# clippy prints one `warning: <message>` line per diagnostic, PLUS one
# `warning: \`<crate>\` (<target>) generated N warnings` summary line per
# crate/target combination at the end of its run. Both kinds of line start
# with `warning:`, so a naive `grep -c '^warning:'` overcounts by including
# the summaries. This script takes total-`^warning:`-lines minus summary
# lines, which yields the count of actual per-diagnostic lines. Note that
# `--all-targets` compiles lib + tests as separate targets, so a lint that
# fires in code shared by both is counted twice (clippy itself marks these as
# "N duplicates" in the summary lines) — this script does not attempt to
# de-duplicate those, so the count is a conservative (>=) but stable measure.
#
# Ratchet model (same as check_file_size.sh, single number): the current
# count lives in `clippy_baseline.txt`. FAILS if the live count exceeds it;
# lower the baseline whenever warnings get fixed.
#
# Usage: scripts/internal/check_clippy_ratchet.sh
# Takes a few minutes — cargo clippy compiles the whole workspace with lints.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASELINE_FILE="scripts/internal/clippy_baseline.txt"

baseline=0
if [ -f "$BASELINE_FILE" ]; then
  baseline="$(grep -vE '^\s*(#|$)' "$BASELINE_FILE" | head -1 | tr -d '[:space:]')"
  [ -z "$baseline" ] && baseline=0
fi

TMP_OUT="$(mktemp)"
trap 'rm -f "$TMP_OUT"' EXIT

echo "running: cargo clippy --workspace --all-targets --locked (this takes a few minutes)..."
cargo clippy --workspace --all-targets --locked > "$TMP_OUT" 2>&1 || true

total_warning_lines="$(grep -c '^warning:' "$TMP_OUT" || true)"
summary_lines="$(grep -cE '^warning: `[^`]+`.*generated [0-9]+ warning' "$TMP_OUT" || true)"
diagnostic_count=$((total_warning_lines - summary_lines))

if [ "$diagnostic_count" -gt 0 ]; then
  echo "--- diagnostic breakdown (by message, deduplicated) ---"
  grep -E '^warning:' "$TMP_OUT" | grep -vE '^warning: `[^`]+`.*generated [0-9]+ warning' | sort | uniq -c | sort -rn
fi

echo ""
echo "summary: ${diagnostic_count} clippy diagnostics (baseline: ${baseline})."

if [ "$diagnostic_count" -gt "$baseline" ]; then
  echo "✗ Clippy diagnostic count grew from ${baseline} to ${diagnostic_count}. Fix the new warning(s) — see full output above / re-run cargo clippy --workspace --all-targets locally — or if intentional, raise the number in scripts/internal/clippy_baseline.txt."
  echo "full clippy output was written to: $TMP_OUT (removed on exit; re-run without this guard to inspect, e.g. cargo clippy --workspace --all-targets --locked)"
  exit 1
fi

if [ "$diagnostic_count" -lt "$baseline" ]; then
  echo "note: count dropped below baseline (${diagnostic_count} < ${baseline}) — lower scripts/internal/clippy_baseline.txt to lock in the improvement."
fi

echo "✓ Clippy diagnostic count within baseline."
