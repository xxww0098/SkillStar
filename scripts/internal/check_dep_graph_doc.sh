#!/usr/bin/env bash
# Structural-governance guard: the mermaid dependency graph in
# docs/boundaries.md must match the workspace's real internal dependencies,
# edge for edge.
#
# Why: dependency direction is the one structural fact this repo states
# twice — once in the Cargo.toml files, once as a picture in boundaries.md.
# AGENTS.md says a fact may have exactly one SSOT, and the only honest way to
# keep a restatement is to have a machine prove the two agree. Without that,
# the diagram degrades into the shape the project had a year ago while
# everyone keeps reading it as current, and `check_workspace_deps.sh` will not
# save you: that guard knows seven specific forbidden edges, not the graph.
#
# The comparison is both ways round, because the two failures mean opposite
# things:
#   * documented but absent from cargo  → the doc is stale; a dependency was
#     removed and the picture still shows it.
#   * present in cargo but undocumented → a new edge was added to a manifest
#     without anyone deciding it belonged in the architecture.
# Only `kind: null` (normal) dependencies count — dev-dependencies and
# build-dependencies do not constrain runtime layering and boundaries.md's
# graph does not claim to show them.
#
# Node labels resolve to packages by name (`skillstar-core`) or by manifest
# directory (`src-tauri`, whose package is named `skillstar`), so the diagram
# can keep using the name a reader recognises.
#
# Ratchet model (same shape as check_file_size.sh): a mismatch that is
# knowingly tolerated can be parked in `dep_graph_doc_baseline.txt` for a WARN
# instead of a FAIL. It should stay empty — the fix for a mismatch is to edit
# one line of the diagram, which is cheaper than writing the baseline entry.
#
# Usage: scripts/internal/check_dep_graph_doc.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

META="$(cargo metadata --no-deps --format-version 1 --locked)"

PYTHONIOENCODING=utf-8 python3 - "$META" <<'PY'
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path.cwd()
DOC = Path("docs/boundaries.md")
meta = json.loads(sys.argv[1])

members = {p["name"] for p in meta["packages"]}
# Label -> package name. Both the crate name and the manifest directory
# (relative to the repo root) are accepted, so `src-tauri` resolves to the
# `skillstar` package without hardcoding that pair here.
alias: dict[str, str] = {name: name for name in members}
for pkg in meta["packages"]:
    rel = Path(pkg["manifest_path"]).parent.relative_to(ROOT).as_posix()
    alias.setdefault(rel, pkg["name"])

cargo_edges = {
    (pkg["name"], dep["name"])
    for pkg in meta["packages"]
    for dep in pkg["dependencies"]
    if dep["name"] in members and dep["kind"] is None
}

# --- parse the mermaid flowchart out of docs/boundaries.md -----------------

if not DOC.is_file():
    print(f"✗ {DOC} is missing — the dependency graph has no home.")
    sys.exit(1)

blocks, current, inside = [], [], False
for line in DOC.read_text(encoding="utf-8").splitlines():
    if line.strip().startswith("```mermaid"):
        inside, current = True, []
        continue
    if inside and line.strip().startswith("```"):
        inside = False
        blocks.append(current)
        continue
    if inside:
        current.append(line)

graphs = [b for b in blocks if any(re.match(r"\s*(flowchart|graph)\b", ln) for ln in b)]
if len(graphs) != 1:
    print(
        f"✗ expected exactly one mermaid flowchart in {DOC}, found {len(graphs)}. "
        "This guard compares a single dependency graph; adjust it if the doc "
        "legitimately grew a second diagram."
    )
    sys.exit(1)

NODE_DECL = re.compile(r'\b([A-Za-z_][\w-]*)\s*\[\s*"([^"]+)"\s*\]')
EDGE = re.compile(r"^\s*([A-Za-z_][\w-]*)\s*-->\s*([A-Za-z_][\w-]*)\s*$")

labels: dict[str, str] = {}
doc_edges: set[tuple[str, str]] = set()
unresolved: list[str] = []

ARROWISH = re.compile(r"-{2,}|-\.+-|={2,}|~{3,}")

for raw in graphs[0]:
    line = raw.strip()
    if not line or line.startswith("%%") or re.match(r"(flowchart|graph)\b", line):
        continue
    decls = list(NODE_DECL.finditer(line))
    for m in decls:
        labels[m.group(1)] = m.group(2)
    # Strip the bracketed labels so `a["x"] --> b` still parses as an edge.
    stripped = NODE_DECL.sub(lambda m: m.group(1), line)
    m = EDGE.match(stripped)
    if m:
        doc_edges.add((m.group(1), m.group(2)))
    elif ARROWISH.search(stripped):
        unresolved.append(line)

if unresolved:
    print("✗ mermaid lines this guard could not parse as `a --> b` edges:")
    for line in unresolved:
        print(f"    {line}")
    print(
        "Rewrite them as plain `-->` edges, or teach this script the new arrow "
        "form — an unparsed line is an unchecked edge."
    )
    sys.exit(1)


def to_package(node: str) -> str | None:
    for key in (labels.get(node, ""), node):
        if key in alias:
            return alias[key]
    return None


unknown = sorted({n for edge in doc_edges for n in edge if to_package(n) is None})
if unknown:
    print("✗ mermaid nodes that match no workspace member (by crate name or manifest dir):")
    for node in unknown:
        print(f"    {node}  (label: {labels.get(node, '<none>')!r})")
    sys.exit(1)

documented = {(to_package(a), to_package(b)) for a, b in doc_edges}

# Every member must at least appear in the picture, or a brand-new leaf crate
# with no edges yet would slip in unnoticed.
drawn = {to_package(n) for n in labels} | {n for edge in documented for n in edge}
missing_nodes = sorted(members - {n for n in drawn if n})

baseline_path = Path("scripts/internal/dep_graph_doc_baseline.txt")
baseline: set[str] = set()
if baseline_path.exists():
    for raw in baseline_path.read_text().splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            baseline.add(" ".join(line.split()))

findings: list[tuple[str, str]] = []
for a, b in sorted(documented - cargo_edges):
    findings.append((f"doc-only {a} -> {b}", "documented edge no longer exists in Cargo.toml"))
for a, b in sorted(cargo_edges - documented):
    findings.append((f"cargo-only {a} -> {b}", "dependency missing from the boundaries.md graph"))
for name in missing_nodes:
    findings.append((f"missing-node {name}", "workspace member absent from the boundaries.md graph"))

new_violations = 0
warned: set[str] = set()
for key, why in findings:
    if key in baseline:
        print(f"WARN  {key}  (baselined mismatch; {why})")
        warned.add(key)
    else:
        print(f"FAIL  {key}  ({why})")
        new_violations += 1

stale = 0
for key in sorted(baseline - warned):
    print(f"STALE      {key}  (mismatch resolved — remove from baseline)")
    stale += 1

print(
    f"\nsummary: {new_violations} new dependency-graph mismatch(es), "
    f"{len(warned)} baselined mismatch(es), {stale} stale baseline entry/entries "
    f"({len(cargo_edges)} cargo edges vs {len(documented)} documented edges)."
)
if new_violations:
    print(
        "✗ docs/boundaries.md no longer describes the real dependency graph. "
        "Update the mermaid block (and the surrounding prose if the layering "
        "changed) so the picture matches the manifests."
    )
    sys.exit(1)
print("✓ docs/boundaries.md matches the workspace dependency graph.")
PY
