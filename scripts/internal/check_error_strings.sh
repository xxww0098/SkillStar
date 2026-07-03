#!/usr/bin/env bash
# Structural-governance guard: count `Result<..., String>` signatures across
# the Rust backend. Stringly-typed errors erase structure (no matching on
# variants, no source chains) — new code should return a proper error enum
# (thiserror) instead. Scans src-tauri/src and crates/, skipping test code
# (paths containing /tests/, and files named *_tests.rs or tests.rs).
#
# Ratchet model (same as check_file_size.sh, but a single number instead of
# per-file entries): the current total lives in `error_strings_baseline.txt`.
# The guard FAILS if the live count exceeds that number, so the total can only
# go down as `Result<_, String>` call sites get migrated to real error types.
# Update the baseline file down whenever you fix one.
#
# Usage: scripts/internal/check_error_strings.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASELINE_FILE="scripts/internal/error_strings_baseline.txt"

baseline=0
if [ -f "$BASELINE_FILE" ]; then
  baseline="$(grep -vE '^\s*(#|$)' "$BASELINE_FILE" | head -1 | tr -d '[:space:]')"
  [ -z "$baseline" ] && baseline=0
fi

is_excluded() {
  case "$1" in
    */tests/*) return 0 ;;
    *_tests.rs|*/tests.rs) return 0 ;;
    *) return 1 ;;
  esac
}

total=0
declare_hits=""

while IFS= read -r -d '' file; do
  file="${file#./}"
  is_excluded "$file" && continue
  count="$(grep -cE 'Result<[^>]*,\s*String>' "$file" || true)"
  [ "$count" -eq 0 ] && continue
  total=$((total + count))
  declare_hits="$declare_hits$(printf '%4s  %s\n' "$count" "$file")"$'\n'
done < <(
  for dir in src-tauri/src crates; do
    [ -d "$dir" ] || continue
    find "$dir" -name "*.rs" -type f -print0 2>/dev/null
  done
)

if [ -n "$declare_hits" ]; then
  printf '%s' "$declare_hits" | sort -rn
fi

echo ""
echo "summary: ${total} Result<_, String> signatures (baseline: ${baseline})."

if [ "$total" -gt "$baseline" ]; then
  echo "✗ Result<_, String> count grew from ${baseline} to ${total}. Use a real error enum (thiserror) for new fallible functions instead of stringly-typed errors, or if intentional, raise the number in scripts/internal/error_strings_baseline.txt."
  exit 1
fi

if [ "$total" -lt "$baseline" ]; then
  echo "note: count dropped below baseline (${total} < ${baseline}) — lower scripts/internal/error_strings_baseline.txt to lock in the improvement."
fi

echo "✓ Result<_, String> count within baseline."
