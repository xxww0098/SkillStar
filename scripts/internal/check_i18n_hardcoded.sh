#!/usr/bin/env bash
# Structural-governance guard: catch hardcoded Chinese (CJK) UI strings landing
# in .ts/.tsx source instead of going through i18next (src/i18n/locales/*.json).
# Comment lines (// ... and /* ... */, including JSDoc) and `import`/re-export
# lines are excluded — only literal CJK characters appearing in actual code
# (string literals, JSX text, etc.) count as a hit.
#
# Ratchet model (same as check_file_size.sh): current hardcoded strings are
# grandfathered per-file in `i18n_hardcoded_baseline.txt` as `path:count` and
# only WARN. The count for a file going UP from its baseline value, or a
# brand-new file appearing with hits, FAILS the build. Baseline entries only
# shrink as strings get moved into locale files — when the count for a file
# drops to 0, the guard flags the stale baseline line for removal.
#
# Usage: scripts/internal/check_i18n_hardcoded.sh

set -euo pipefail

# Force a UTF-8 locale for this script's regex matching. Under LC_CTYPE=C,
# bash's `[[ =~ [一-鿿] ]]` character-class matching degrades to a byte-range
# comparison and produces false positives on unrelated multi-byte characters
# (verified: it wrongly matched a plain "…" ellipsis, U+2026, which sits nowhere
# near the U+4E00-U+9FFF CJK range). C.UTF-8 is present on virtually every
# Linux CI image; en_US.UTF-8 is a broadly-available fallback.
export LC_ALL="${LC_ALL:-}"
if [ -z "$LC_ALL" ] || [[ "$LC_ALL" != *"UTF-8"* && "$LC_ALL" != *"utf8"* ]]; then
  # Capture into a variable rather than piping straight into `grep -q`: grep -q
  # exits as soon as it finds a match and closes its stdin, which can SIGPIPE
  # (exit 141) the still-writing `locale -a` on the other end of the pipe — under
  # `set -o pipefail` that makes the whole `if pipeline; then` look like a
  # failure even though grep matched. (Verified failure mode while writing this.)
  available_locales="$(locale -a 2>/dev/null || true)"
  if printf '%s\n' "$available_locales" | grep -qi '^C\.UTF-8$'; then
    export LC_ALL="C.UTF-8"
  elif printf '%s\n' "$available_locales" | grep -qi '^en_US\.UTF-8$'; then
    export LC_ALL="en_US.UTF-8"
  fi
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASELINE_FILE="scripts/internal/i18n_hardcoded_baseline.txt"

# Load baseline as "path" -> count into a lookup string (avoids bash 4
# associative-array dependency so this also runs on macOS's stock bash 3.2).
baseline_raw=""
if [ -f "$BASELINE_FILE" ]; then
  baseline_raw="$(grep -vE '^\s*(#|$)' "$BASELINE_FILE" || true)"
fi
baseline_count_for() {
  local path="$1"
  printf '%s\n' "$baseline_raw" | awk -F: -v p="$path" '$1 == p { print $2; found=1 } END { if (!found) print "" }'
}

is_excluded_path() {
  case "$1" in
    src/test/*|src/i18n/*) return 0 ;;
    *.test.ts|*.test.tsx|*.spec.ts|*.spec.tsx) return 0 ;;
    *) return 1 ;;
  esac
}

# Count CJK-bearing code lines in a single file, skipping // and /* */
# comments (state machine tracks block-comment continuation across lines)
# and skipping `import ...` / `export { ... } from ...` lines.
count_hardcoded_in_file() {
  local file="$1"
  local in_block=0
  local file_count=0
  local line

  while IFS= read -r line || [ -n "$line" ]; do
    if [ "$in_block" = "1" ]; then
      if [[ "$line" == *'*/'* ]]; then
        line="${line#*\*/}"
        in_block=0
      else
        continue
      fi
    fi

    # Strip every complete /* ... */ span on this line; if one is left
    # unterminated, cut the line there and remember we're inside a block.
    while [[ "$line" == *'/*'* ]]; do
      local before="${line%%/\**}"
      local after="${line#*/\*}"
      if [[ "$after" == *'*/'* ]]; then
        line="$before${after#*\*/}"
      else
        line="$before"
        in_block=1
        break
      fi
    done

    # Trim leading whitespace for the line-start checks below.
    local trimmed="${line#"${line%%[![:space:]]*}"}"
    [[ -z "$trimmed" ]] && continue
    [[ "$trimmed" == //* ]] && continue
    [[ "$trimmed" == \** ]] && continue  # stray JSDoc continuation asterisk
    [[ "$trimmed" == import[[:space:]]* ]] && continue
    [[ "$trimmed" =~ ^export[[:space:]]+(type[[:space:]]+)?\{.*\}[[:space:]]*from ]] && continue

    if [[ "$line" =~ [一-鿿] ]]; then
      file_count=$((file_count + 1))
    fi
  done < "$file"

  echo "$file_count"
}

new_violations=0
warn_violations=0
total_hits=0
seen_baselined=""

while IFS= read -r -d '' file; do
  file="${file#./}"
  is_excluded_path "$file" && continue
  count="$(count_hardcoded_in_file "$file")"
  [ "$count" -eq 0 ] && continue

  total_hits=$((total_hits + count))
  base="$(baseline_count_for "$file")"

  if [ -z "$base" ]; then
    printf 'FAIL  %4s  %s  (NEW file with hardcoded CJK — not in baseline)\n' "$count" "$file"
    new_violations=$((new_violations + 1))
  elif [ "$count" -gt "$base" ]; then
    printf 'FAIL  %4s  %s  (grew from baseline %s — new hardcoded string added)\n' "$count" "$file" "$base"
    new_violations=$((new_violations + 1))
    seen_baselined="$seen_baselined$file"$'\n'
  else
    printf 'WARN  %4s  %s  (baselined debt, budget %s)\n' "$count" "$file" "$base"
    warn_violations=$((warn_violations + 1))
    seen_baselined="$seen_baselined$file"$'\n'
  fi
done < <(
  # NOTE: `-name a -o -name b -print0` binds -print0 to only the LAST
  # alternative (the -o operator in find has lower precedence than an
  # implicit -print) -- a real bug caught while writing this: .ts files were
  # silently dropped. Looping per-extension (as check_file_size.sh does)
  # avoids the trap.
  for ext in ts tsx; do
    find src -name "*.${ext}" -type f -print0 2>/dev/null
  done
)

# Report baseline entries whose file no longer has any hits (stale → remove).
stale=0
if [ -n "$baseline_raw" ]; then
  while IFS=: read -r entry_path _entry_count; do
    [ -z "$entry_path" ] && continue
    if ! printf '%s' "$seen_baselined" | grep -qxF "$entry_path"; then
      echo "STALE       -  $entry_path  (no longer has hardcoded CJK — remove from baseline)"
      stale=$((stale + 1))
    fi
  done <<< "$baseline_raw"
fi

echo ""
echo "summary: ${total_hits} hardcoded CJK code lines across baselined+new files, ${new_violations} new violations, ${warn_violations} baselined debt, ${stale} stale baseline entr$([ "$stale" = 1 ] && echo y || echo ies)."

if [ "$new_violations" -gt 0 ]; then
  echo "✗ New hardcoded Chinese UI text found. Move it into src/i18n/locales/{en,zh-CN}.json and use useTranslation(), or if intentional (e.g. debug-only), bump the count for this file in scripts/internal/i18n_hardcoded_baseline.txt."
  exit 1
fi

echo "✓ No new hardcoded CJK strings."
