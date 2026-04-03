#!/usr/bin/env bash
set -euo pipefail

# Pre-commit gate:
# - runs static scan only when staged changes touch skill content
# - blocks commit on High/Critical risk or scan execution failure

if ! command -v git >/dev/null 2>&1; then
  echo "[security-scan] git not found." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "[security-scan] cargo not found." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "[security-scan] python3 not found." >&2
  exit 1
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

is_likely_skill_path() {
  local rel="$1"
  case "$rel" in
    SKILL.md|*/SKILL.md|skills/*|.agents/skills/*|.agents/skills-local/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

find_skill_dir() {
  local rel="$1"
  local probe
  if [[ -d "$rel" ]]; then
    probe="$rel"
  else
    probe="$(dirname "$rel")"
  fi

  while [[ "$probe" != "." && "$probe" != "/" ]]; do
    if [[ -f "$probe/SKILL.md" ]]; then
      printf "%s\n" "$probe"
      return 0
    fi

    if [[ "$probe" == "$REPO_ROOT" ]]; then
      break
    fi
    probe="$(dirname "$probe")"
  done

  return 1
}

STAGED_FILES=()
while IFS= read -r -d '' file_path; do
  STAGED_FILES+=("$file_path")
done < <(git diff --cached --name-only --diff-filter=ACMR -z)

if [[ ${#STAGED_FILES[@]} -eq 0 ]]; then
  echo "[security-scan] No staged files; skipping."
  exit 0
fi

declare -A scan_dirs_map=()
for raw_path in "${STAGED_FILES[@]}"; do
  rel_path="${raw_path#./}"
  rel_path="${rel_path%/}"
  [[ -z "$rel_path" ]] && continue

  if ! is_likely_skill_path "$rel_path"; then
    continue
  fi

  if [[ "$rel_path" == SKILL.md || "$rel_path" == */SKILL.md ]]; then
    skill_dir="$(dirname "$rel_path")"
    [[ -z "$skill_dir" ]] && skill_dir="."
    scan_dirs_map["$skill_dir"]=1
    continue
  fi

  if skill_dir="$(find_skill_dir "$rel_path")"; then
    scan_dirs_map["$skill_dir"]=1
  fi
done

if [[ ${#scan_dirs_map[@]} -eq 0 ]]; then
  echo "[security-scan] No staged skill changes detected; skipping."
  exit 0
fi

mapfile -t scan_dirs < <(printf '%s\n' "${!scan_dirs_map[@]}" | sort)
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

blocked=0
for rel_dir in "${scan_dirs[@]}"; do
  if [[ "$rel_dir" == "." ]]; then
    abs_dir="$REPO_ROOT"
  else
    abs_dir="$REPO_ROOT/$rel_dir"
  fi

  if [[ ! -d "$abs_dir" ]]; then
    echo "[security-scan] Skip missing directory: $abs_dir"
    continue
  fi

  report_json="$tmp_dir/$(echo "$rel_dir" | tr '/ ' '__').json"
  echo "[security-scan] Scanning $abs_dir (static)"
  if ! cargo run --quiet --manifest-path src-tauri/Cargo.toml -- scan "$abs_dir" --static-only >"$report_json"; then
    echo "[security-scan] Scan command failed for $abs_dir" >&2
    blocked=1
    continue
  fi

  if ! python3 - "$report_json" "$abs_dir" <<'PY'
import json
import sys

report_path = sys.argv[1]
scan_dir = sys.argv[2]

with open(report_path, "r", encoding="utf-8") as fp:
    report = json.load(fp)

risk = str(report.get("risk_level", "Safe")).strip().lower()
findings = report.get("static_findings") or []
print(f"[security-scan] Result {scan_dir}: risk={risk} findings={len(findings)}")

if risk in {"high", "critical"}:
    print(
        f"[security-scan] Blocked {scan_dir}: risk={risk} findings={len(findings)}",
        file=sys.stderr,
    )
    raise SystemExit(2)
PY
  then
    blocked=1
  fi
done

if [[ "$blocked" -ne 0 ]]; then
  echo "[security-scan] Commit blocked by security gate." >&2
  exit 1
fi

echo "[security-scan] Gate passed."
