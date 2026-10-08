#!/usr/bin/env bash
# Guard: single workspace lockfile, forbidden edges, library-only app, feature policy.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if [ ! -f Cargo.lock ]; then
  echo "workspace dep guard FAILED: missing root Cargo.lock"
  exit 1
fi

nested_locks="$(find crates -name Cargo.lock -type f -print 2>/dev/null || true)"
if [ -n "$nested_locks" ]; then
  echo "workspace dep guard FAILED: nested Cargo.lock files are forbidden; use the workspace root lockfile"
  printf ' - %s\n' $nested_locks
  exit 1
fi

META=$(cargo metadata --no-deps --format-version 1 --locked)
PYTHONIOENCODING=utf-8 python3 - "$META" <<'PY'
import json, sys

meta = json.loads(sys.argv[1])
errors = []

# Every workspace edge is opt-in, including dev/build/target dependencies.
# Unknown packages must first establish ownership here and in boundaries.md.
ALLOWED = {
    "ss-core": set(),
    "claude-marketplace": set(),
    "ss-gpui": {"ss-core", "ss-app", "ss-skills", "ss-marketplace", "ss-usage"},
    "ss-git": {"ss-core"},
    "ss-skills": {"ss-core", "ss-git", "claude-marketplace"},
    "ss-marketplace": {"ss-core"},
    "ss-usage": {"ss-core"},
    "ss-sync": {"ss-core"},
    "ss-app": {"ss-core", "ss-git", "ss-skills", "ss-marketplace"},
    "skillstar": {"ss-app", "ss-git", "ss-gpui"},
}
workspace_ids = set(meta["workspace_members"])
workspace_packages = {p["name"]: p for p in meta["packages"] if p["id"] in workspace_ids}
for name, package in workspace_packages.items():
    if name not in ALLOWED:
        errors.append(f"unclassified workspace package: {name}; declare its domain boundary")
        continue
    for dep in package["dependencies"]:
        target = dep["name"]
        if target in workspace_packages and target not in ALLOWED[name]:
            errors.append(f"forbidden edge: {name} -> {target} ({dep.get('kind') or 'normal'})")

app = workspace_packages.get("ss-app")
if app:
    bins = [t for t in app.get("targets", []) if "bin" in t.get("kind", [])]
    if bins:
        errors.append(f"ss-app still has bin targets: {[b['name'] for b in bins]}")

if errors:
    print("workspace dep guard FAILED:")
    for e in errors:
        print(" -", e)
    sys.exit(1)
print("workspace dep guard OK")
PY

# Keep the whitelist honest for aliased, dev/build, and unclassified edges.
PYTHONIOENCODING=utf-8 python3 "$ROOT/scripts/internal/test_workspace_deps.py"
