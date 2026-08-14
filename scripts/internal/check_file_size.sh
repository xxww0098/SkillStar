#!/usr/bin/env bash
# Structural-governance guard: fail when a source file exceeds the project's
# ~1000-line limit (see AGENTS.md / docs/ROADMAP.md). Run in CI so growth is
# caught at PR time instead of during a painful later refactor.
#
# Two thresholds, not one exemption. Production code is capped at `max_lines`
# (1000, matching AGENTS.md). Test files used to be excluded as a whole class,
# which made the four >800-line Rust test files and the 1808-line
# SharedChannelsContent.test.tsx completely invisible to this guard — a test
# file is still a file someone has to read. They now get their own, looser cap
# (`max_test_lines`, 1500): deliberately generous, because table-driven test
# modules legitimately run long, but bounded, so they can no longer grow
# without limit. Test files between the two thresholds are reported as NOTE
# lines (visible, non-blocking) so the trend is at least on screen.
#
# Ratchet model: files already over their limit are listed in
# `file_size_baseline.txt` and only produce a WARNING (tracked as debt in
# docs/ROADMAP.md P2). Any NEW file over its limit FAILS the build, so the
# situation can only improve. When a baselined file is split below the limit,
# the guard tells you to drop its stale baseline entry.
#
# Usage: scripts/internal/check_file_size.sh [max_lines] [max_test_lines]
#   max_lines defaults to 1000, max_test_lines to 1500.

set -euo pipefail

MAX="${1:-1000}"
MAX_TEST="${2:-1500}"

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

# Build artefacts and hand-written IPC fixtures are not source we govern.
is_excluded() {
  case "$1" in
    */target/*|*/node_modules/*|*/dist/*) return 0 ;;
    */devMock.ts|*/devMockData.ts) return 0 ;;
    *) return 1 ;;
  esac
}

# Test code: looser cap, never a free pass.
is_test_file() {
  case "$1" in
    *.test.ts|*.test.tsx|*.spec.ts|*.spec.tsx) return 0 ;;
    */tests/*|*/test/*) return 0 ;;
    *_tests.rs|*_test.rs) return 0 ;;
    *) return 1 ;;
  esac
}

new_violations=0
warn_violations=0
notes=0
seen_baselined=""

while IFS= read -r -d '' file; do
  file="${file#./}"
  is_excluded "$file" && continue
  if is_test_file "$file"; then
    limit="$MAX_TEST"
    kind="test"
  else
    limit="$MAX"
    kind="source"
  fi
  lines=$(wc -l < "$file" | tr -d ' ')
  if [ "$lines" -le "$limit" ]; then
    # Test file past the production limit but under its own cap: visible, not blocking.
    if [ "$kind" = test ] && [ "$lines" -gt "$MAX" ]; then
      printf 'NOTE  %6s  %s  (test file over the %s-line source limit, cap %s)\n' \
        "$lines" "$file" "$MAX" "$MAX_TEST"
      notes=$((notes + 1))
    fi
    continue
  fi
  if is_baselined "$file"; then
    printf 'WARN  %6s  %s  (baselined debt, %s limit %s)\n' "$lines" "$file" "$kind" "$limit"
    warn_violations=$((warn_violations + 1))
    seen_baselined="$seen_baselined$file"$'\n'
  else
    printf 'FAIL  %6s  %s  (NEW over-limit %s file, limit %s)\n' "$lines" "$file" "$kind" "$limit"
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
echo "summary: ${new_violations} new over-limit, ${warn_violations} baselined debt, ${notes} oversized test file(s) under cap, ${stale} stale baseline entr$([ "$stale" = 1 ] && echo y || echo ies)."

if [ "$new_violations" -gt 0 ]; then
  echo "✗ A new file exceeds its line limit (${MAX} source / ${MAX_TEST} test). Split it into smaller modules (see docs/ROADMAP.md)."
  exit 1
fi

echo "✓ No new over-limit files."
