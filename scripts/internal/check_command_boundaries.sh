#!/usr/bin/env bash
# Ratchet direct filesystem/infrastructure access in Tauri command production
# code. Commands may parse DTOs, adapt State/events, schedule blocking work,
# and call domain facades; path/file ownership belongs below the adapter seam.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
from pathlib import Path
import re
import sys

baseline_path = Path("scripts/internal/command_boundary_baseline.txt")
baseline: dict[str, int] = {}
if baseline_path.exists():
    for raw in baseline_path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        path, count = line.rsplit(maxsplit=1)
        baseline[path] = int(count)

patterns = (
    re.compile(r"\b(?:std|tokio)::fs::"),
    re.compile(r"skillstar_core::infra::(?:paths|fs_ops)\b"),
    re.compile(r"skillstar_core::infra::\{[^}]*\b(?:paths|fs_ops)\b"),
)

current: dict[str, int] = {}
for path in sorted(Path("src-tauri/src/commands").rglob("*.rs")):
    # Command unit tests may construct fixtures directly; this guard protects
    # the production adapter above the conventional trailing test module.
    production = path.read_text().split("#[cfg(test)]", 1)[0]
    count = sum(
        any(pattern.search(line) for pattern in patterns)
        for line in production.splitlines()
    )
    if count:
        current[path.as_posix()] = count

new_violations = 0
stale = 0
for path, count in current.items():
    allowed = baseline.get(path)
    if allowed is None:
        print(f"FAIL  {count:3}  {path}  (new command-layer filesystem access)")
        new_violations += 1
    elif count > allowed:
        print(f"FAIL  {count:3}  {path}  (baseline {allowed}, command boundary regressed)")
        new_violations += 1
    else:
        print(f"WARN  {count:3}  {path}  (baselined debt, max {allowed})")
        if count < allowed:
            print(f"STALE      {path}  (lower baseline from {allowed} to {count})")
            stale += 1

for path in sorted(set(baseline) - set(current)):
    print(f"STALE      {path}  (filesystem access removed; delete baseline entry)")
    stale += 1

print(
    f"\nsummary: {new_violations} command-boundary regression(s), "
    f"{len(current)} baselined file(s), {stale} stale baseline entry/update(s)."
)
if new_violations:
    sys.exit(1)
print("✓ No new command-layer filesystem ownership.")
PY
