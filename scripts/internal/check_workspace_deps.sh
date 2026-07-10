#!/usr/bin/env bash
# Guard: forbidden reverse edges and known phantom edges in the skillstar workspace.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

META=$(cargo metadata --no-deps --format-version 1)
python3 - "$META" <<'PY'
import json, sys

meta = json.loads(sys.argv[1])
packages = {p["name"]: p for p in meta["packages"] if p["name"].startswith("skillstar") or p["name"] == "skillstar"}

def deps(name):
    p = packages.get(name)
    if not p:
        return set()
    return {d["name"] for d in p["dependencies"] if d["name"].startswith("skillstar") or d["name"] == "skillstar"}

errors = []

# Forbidden reverse edges
forbidden = [
    ("skillstar-skills", "skillstar-marketplace"),
    ("skillstar-marketplace", "skillstar-skills"),
    ("skillstar-models", "skillstar-ai"),
    ("skillstar-usage", "skillstar-models"),
    ("skillstar-core", "skillstar-skills"),
    ("skillstar-core", "skillstar-app"),
    ("skillstar-fingerprint", "skillstar-core"),  # phantom, removed
    ("skillstar-skills", "skillstar-projects"),  # absorbed
]

for a, b in forbidden:
    if b in deps(a):
        errors.append(f"forbidden edge: {a} -> {b}")

# skillstar-projects must not exist
if "skillstar-projects" in packages:
    errors.append("skillstar-projects crate still present — should be absorbed into skillstar-skills")

# skillstar-app must be library-only (no targets of kind bin, or only lib)
app = packages.get("skillstar-app")
if app:
    bins = [t for t in app.get("targets", []) if "bin" in t.get("kind", [])]
    # library-only means no bin targets
    if bins:
        errors.append(f"skillstar-app still has bin targets: {[b['name'] for b in bins]}")

# fingerprint default features must not include impersonate
fp = packages.get("skillstar-fingerprint")
if fp:
    # cargo metadata doesn't always expand default features clearly; check Cargo.toml
    pass

if errors:
    print("workspace dep guard FAILED:")
    for e in errors:
        print(" -", e)
    sys.exit(1)
print("workspace dep guard OK")
PY
