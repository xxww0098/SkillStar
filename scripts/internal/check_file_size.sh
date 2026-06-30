#!/usr/bin/env bash
# Structural-governance guard: fail when a source file exceeds the project's
# ~1000-line limit (see AGENTS.md / docs/ROADMAP.md). Run in CI so growth is
# caught at PR time instead of during a painful later refactor.
#
# Ratchet model: files already over the limit are listed in
# `file_size_baseline.txt` and only produce a WARNING (tracked as debt in
# docs/ROADMAP.md P2). Any NEW file over the limit FAILS the build, so the
# situation can only improve. When a baselined file is split below the limit,
# the guard tells you to drop its stale baseline entry.
#
# Usage: scripts/internal/check_file_size.sh [max_lines]
#   max_lines defaults to 1000.

set -euo pipefail

MAX="${1:-1000}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASELINE_FILE="scripts/internal/file_size_baseline.txt"

# Load baseline paths (ignore comments / blank lines) into a lookup string.
baseline=""
if [ -f "$BASELINE_FILE" ]; then
  baseline="$(grep -vE '^\s*(#|$)' "$BASELINE_FILE" || true)"
fi
is_baselined() { printf '%s\n' "$baseline" | grep -qxF "$1"; }

EXTS=(rs ts tsx)

is_excluded() {
  case "$1" in
    */target/*|*/node_modules/*|*/dist/*) return 0 ;;
    *.test.ts|*.test.tsx|*.spec.ts|*.spec.tsx) return 0 ;;
    */tests/*|*/test/*) return 0 ;;
    *_tests.rs|*/devMock.ts|*/devMockData.ts) return 0 ;;
    *) return 1 ;;
  esac
}

new_violations=0
warn_violations=0
seen_baselined=""

while IFS= read -r -d '' file; do
  file="${file#./}"
  is_excluded "$file" && continue
  lines=$(wc -l < "$file" | tr -d ' ')
  [ "$lines" -le "$MAX" ] && continue
  if is_baselined "$file"; then
    printf 'WARN  %6s  %s  (baselined debt)\n' "$lines" "$file"
    warn_violations=$((warn_violations + 1))
    seen_baselined="$seen_baselined$file"$'\n'
  else
    printf 'FAIL  %6s  %s  (NEW over-limit file)\n' "$lines" "$file"
    new_violations=$((new_violations + 1))
  fi
done < <(
  for ext in "${EXTS[@]}"; do
    find src src-tauri/src crates -name "*.${ext}" -type f -print0 2>/dev/null
  done
)

# Report baseline entries that are now under the limit (stale → should be removed).
stale=0
if [ -n "$baseline" ]; then
  while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    if ! printf '%s' "$seen_baselined" | grep -qxF "$entry"; then
      echo "STALE       -  $entry  (now under limit — remove from baseline)"
      stale=$((stale + 1))
    fi
  done <<< "$baseline"
fi

echo ""
echo "summary: ${new_violations} new over-limit, ${warn_violations} baselined debt, ${stale} stale baseline entr$([ "$stale" = 1 ] && echo y || echo ies)."

if [ "$new_violations" -gt 0 ]; then
  echo "✗ A new file exceeds the ${MAX}-line limit. Split it into smaller modules (see docs/ROADMAP.md)."
  exit 1
fi

echo "✓ No new over-limit files."
