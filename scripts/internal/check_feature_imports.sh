#!/usr/bin/env bash
# Structural-governance guard: catch src/features/<a>/ code importing directly
# from src/features/<b>/ (a != b). Cross-feature reach-ins bypass the intended
# slice boundary (each domain owns its own components/hooks/api); shared code
# should move to src/components or src/lib instead. Both import spellings are
# caught: relative (`../../<feature>/...`, any `../` depth) and absolute
# (`@/features/<feature>/...`).
#
# Ratchet model (same as check_file_size.sh): current cross-feature imports are
# grandfathered as exact `file:line` entries in `feature_imports_baseline.txt`
# and only WARN. Any NEW cross-feature import line (not in the baseline) FAILS
# the build. When a baselined import is removed/refactored away, the guard
# tells you to drop its stale baseline entry.
#
# Usage: scripts/internal/check_feature_imports.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASELINE_FILE="scripts/internal/feature_imports_baseline.txt"

baseline=""
if [ -f "$BASELINE_FILE" ]; then
  baseline="$(grep -vE '^\s*(#|$)' "$BASELINE_FILE" || true)"
fi
is_baselined() { printf '%s\n' "$baseline" | grep -qxF "$1"; }

FEATURES_DIR="src/features"
[ -d "$FEATURES_DIR" ] || { echo "no $FEATURES_DIR directory — nothing to check"; exit 0; }

# Build the alternation of known feature names from the actual directory
# listing (not hardcoded) so renaming/adding a feature needs no script edit.
feature_list=""
for d in "$FEATURES_DIR"/*/; do
  name="$(basename "$d")"
  feature_list="${feature_list}${feature_list:+|}${name}"
done
[ -z "$feature_list" ] && { echo "no feature subdirectories found — nothing to check"; exit 0; }

new_violations=0
warn_violations=0
seen_baselined=""

while IFS= read -r -d '' file; do
  file="${file#./}"
  # Which feature does this file itself belong to?
  owner="$(printf '%s\n' "$file" | sed -nE "s#^src/features/([^/]+)/.*#\1#p")"
  [ -z "$owner" ] && continue

  # Grep for both spellings in one pass; -n gives us the line number.
  while IFS=: read -r lineno content; do
    [ -z "$lineno" ] && continue
    # Extract which known feature name the matched import path points at.
    target="$(printf '%s\n' "$content" | grep -oE "(\.\./)+($feature_list)/|@/features/($feature_list)/" | head -1 | grep -oE "($feature_list)")"
    [ -z "$target" ] && continue
    [ "$target" = "$owner" ] && continue  # same-feature self-reference, not a violation

    entry="$file:$lineno"
    if is_baselined "$entry"; then
      printf 'WARN  %s  [%s -> %s]  %s  (baselined debt)\n' "$entry" "$owner" "$target" "$content"
      warn_violations=$((warn_violations + 1))
      seen_baselined="$seen_baselined$entry"$'\n'
    else
      printf 'FAIL  %s  [%s -> %s]  %s  (NEW cross-feature import)\n' "$entry" "$owner" "$target" "$content"
      new_violations=$((new_violations + 1))
    fi
  done < <(grep -nE "from [\"'](\.\./)+($feature_list)/|from [\"']@/features/($feature_list)/" "$file" || true)
done < <(
  for ext in ts tsx; do
    find "$FEATURES_DIR" -name "*.${ext}" -type f -print0 2>/dev/null
  done
)

# Report baseline entries that no longer match anything current (stale).
stale=0
if [ -n "$baseline" ]; then
  while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    if ! printf '%s' "$seen_baselined" | grep -qxF "$entry"; then
      echo "STALE       $entry  (import no longer present — remove from baseline)"
      stale=$((stale + 1))
    fi
  done <<< "$baseline"
fi

echo ""
echo "summary: ${new_violations} new cross-feature imports, ${warn_violations} baselined debt, ${stale} stale baseline entr$([ "$stale" = 1 ] && echo y || echo ies)."

if [ "$new_violations" -gt 0 ]; then
  echo "✗ New import reaches across a feature slice boundary. Move the shared code to src/components or src/lib, or if intentional, add the file:line to scripts/internal/feature_imports_baseline.txt."
  exit 1
fi

echo "✓ No new cross-feature imports."
