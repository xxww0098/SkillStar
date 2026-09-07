#!/usr/bin/env bash
# Guard: single workspace lockfile, forbidden edges, library-only app, feature policy.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if [ ! -f Cargo.lock ]; then
  echo "workspace dep guard FAILED: missing root Cargo.lock"
  exit 1
fi

nested_locks="$(find src-tauri crates -name Cargo.lock -type f -print 2>/dev/null || true)"
if [ -n "$nested_locks" ]; then
  echo "workspace dep guard FAILED: nested Cargo.lock files are forbidden; use the workspace root lockfile"
  printf ' - %s\n' $nested_locks
  exit 1
fi

META=$(cargo metadata --no-deps --format-version 1 --locked)
PYTHONIOENCODING=utf-8 python3 - "$META" <<'PY'
import json, sys

meta = json.loads(sys.argv[1])
# Product crates plus protocol leaves. `mcp-registry-spec` is reserved for a
# later extraction; the name is listed so the same no-skillstar-* rule applies
# the moment that package exists.
PROTOCOL_LEAVES = ("skill-spec", "mcp-registry-spec")
packages = {
    p["name"]: p
    for p in meta["packages"]
    if p["name"].startswith("skillstar")
    or p["name"] == "skillstar"
    or p["name"] in PROTOCOL_LEAVES
}

def deps(name):
    p = packages.get(name)
    if not p:
        return set()
    return {d["name"] for d in p["dependencies"] if d["name"].startswith("skillstar") or d["name"] == "skillstar"}

def all_deps(name):
    p = packages.get(name)
    if not p:
        return set()
    return {d["name"] for d in p["dependencies"]}

errors = []

forbidden = [
    ("skillstar-skills", "skillstar-marketplace"),
    ("skillstar-marketplace", "skillstar-skills"),
    ("skillstar-models", "skillstar-ai"),
    ("skillstar-usage", "skillstar-models"),
    ("skillstar-core", "skillstar-skills"),
    ("skillstar-core", "skillstar-app"),
    ("skillstar-skills", "skillstar-projects"),
    # SSH listing talks SFTP, not the skills domain. A stale path dep used to
    # force sync to rebuild whenever skills/git/agents/auth changed.
    ("skillstar-sync", "skillstar-skills"),
]

for a, b in forbidden:
    if b in deps(a):
        errors.append(f"forbidden edge: {a} -> {b}")

if "skillstar-projects" in packages:
    errors.append("skillstar-projects crate still present — should be absorbed into skillstar-skills")

# Wave 2A: ai absorbed
if "skillstar-ai" in packages:
    errors.append("skillstar-ai must be absorbed into skillstar-models")
if "skillstar-ssh" in packages:
    errors.append("skillstar-ssh must be absorbed into skillstar-sync (as ssh module)")
if "skillstar-agents" in packages:
    errors.append("skillstar-agents must be absorbed into skillstar-skills::agents")
if "skillstar-github-auth" in packages:
    errors.append("skillstar-github-auth must be absorbed into skillstar-skills::github_auth")
if "skillstar-providers" in packages:
    errors.append("skillstar-providers must be absorbed into skillstar-core::providers")

for leaf in PROTOCOL_LEAVES:
    if leaf not in packages:
        continue
    product = sorted(d for d in all_deps(leaf) if d.startswith("skillstar") or d == "skillstar")
    if product:
        errors.append(f"{leaf} must not depend on skillstar-* packages: {product}")

app = packages.get("skillstar-app")
if app:
    bins = [t for t in app.get("targets", []) if "bin" in t.get("kind", [])]
    if bins:
        errors.append(f"skillstar-app still has bin targets: {[b['name'] for b in bins]}")

if errors:
    print("workspace dep guard FAILED:")
    for e in errors:
        print(" -", e)
    sys.exit(1)
print("workspace dep guard OK")
PY
