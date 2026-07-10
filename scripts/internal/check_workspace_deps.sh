#!/usr/bin/env bash
# Guard: forbidden reverse edges, phantom edges, library-only app, feature policy.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

META=$(cargo metadata --no-deps --format-version 1)
python3 - "$META" <<'PY'
import json, sys, re, pathlib

meta = json.loads(sys.argv[1])
packages = {p["name"]: p for p in meta["packages"] if p["name"].startswith("skillstar") or p["name"] == "skillstar"}
root = pathlib.Path(".")

def deps(name):
    p = packages.get(name)
    if not p:
        return set()
    return {d["name"] for d in p["dependencies"] if d["name"].startswith("skillstar") or d["name"] == "skillstar"}

def read_default_features(crate_dir_name: str):
    toml = root / "crates" / crate_dir_name / "Cargo.toml"
    if not toml.exists():
        return None
    text = toml.read_text()
    m = re.search(r'(?m)^default\s*=\s*\[([^\]]*)\]', text)
    if not m:
        return []
    inner = m.group(1).strip()
    if not inner:
        return []
    return [x.strip().strip('"').strip("'") for x in inner.split(",") if x.strip()]

errors = []

forbidden = [
    ("skillstar-skills", "skillstar-marketplace"),
    ("skillstar-marketplace", "skillstar-skills"),
    ("skillstar-models", "skillstar-ai"),
    ("skillstar-usage", "skillstar-models"),
    ("skillstar-core", "skillstar-skills"),
    ("skillstar-core", "skillstar-app"),
    ("skillstar-fingerprint", "skillstar-core"),
    ("skillstar-skills", "skillstar-projects"),
]

for a, b in forbidden:
    if b in deps(a):
        errors.append(f"forbidden edge: {a} -> {b}")

if "skillstar-projects" in packages:
    errors.append("skillstar-projects crate still present — should be absorbed into skillstar-skills")

# Wave 2A: fingerprint and ai absorbed
if "skillstar-fingerprint" in packages:
    errors.append("skillstar-fingerprint must be absorbed into skillstar-usage")
if "skillstar-ai" in packages:
    errors.append("skillstar-ai must be absorbed into skillstar-models")

app = packages.get("skillstar-app")
if app:
    bins = [t for t in app.get("targets", []) if "bin" in t.get("kind", [])]
    if bins:
        errors.append(f"skillstar-app still has bin targets: {[b['name'] for b in bins]}")

# Heavy features must not be default-on in leaf crates
for crate, forbidden_feat in [
    ("skillstar-usage", "impersonate"),
]:
    defaults = read_default_features(crate)
    if defaults is None:
        errors.append(f"missing Cargo.toml for {crate}")
    elif forbidden_feat in defaults:
        errors.append(
            f"{crate} default features must not include {forbidden_feat!r} "
            f"(binary root must opt in); got default={defaults}"
        )

# Binary root (src-tauri / skillstar) must enable impersonate on usage so production builds keep it
tauri_toml = (root / "src-tauri" / "Cargo.toml").read_text()
if 'skillstar-usage' in tauri_toml:
    # require features = ["impersonate"] somewhere on the usage dep line
    if not re.search(r'skillstar-usage\s*=\s*\{[^}]*features\s*=\s*\[[^\]]*impersonate', tauri_toml):
        errors.append(
            'src-tauri must enable skillstar-usage feature "impersonate" at the binary root'
        )

if errors:
    print("workspace dep guard FAILED:")
    for e in errors:
        print(" -", e)
    sys.exit(1)
print("workspace dep guard OK")
PY
