#!/usr/bin/env bash
# Structural-governance guard: every `.ts`/`.tsx` file under `src/features/`
# must be reachable from a real application entry point.
#
# Why this exists: `check_no_orphan_modules.sh` is the Rust twin of this gate
# and its header says so explicitly — it only walks `.rs`. Nothing walked the
# TypeScript side, and the cost was measured: ~4000 lines inside
# `src/features/models/` (27% of the feature) sat unreferenced while
# `bun run lint`, `bun run build` and `bun run test` all stayed green. tsc does
# not complain about a file nobody imports, ESLint happily lints it, and vitest
# happily runs its co-located tests — so the dead island even looked *covered*.
# A reachability walk is the only check that can see the difference.
#
# Reachability model:
#   * roots  — `src/main.tsx` plus every file under `src/pages/`. Pages are the
#              route targets `src/App.tsx` reaches through `lazy(() =>
#              import(...))`; rooting at them directly means a page that is
#              temporarily unrouted does not cascade a hundred false orphans.
#   * edges  — every module specifier in the source: `import … from "X"`,
#              `export … from "X"`, bare `import "X"`, dynamic `import("X")`
#              and `require("X")`. Dynamic `import()` matters: `App.tsx` routes
#              exclusively through it, and `ScopeDetailDrawer` / `Markdown`
#              lazy-load real components that way.
#   * paths  — relative specifiers resolve against the importing file's
#              directory; `@/…` resolves against `src/` (tsconfig.json paths).
#              Anything else is a package and is ignored.
#   * suffix — a specifier resolves through the same candidate ladder Vite and
#              tsc use: exact, `.ts`, `.tsx`, `.d.ts`, `.js`→`.ts`/`.tsx`,
#              then `index.ts` / `index.tsx` inside a directory.
#
# Deliberately conservative in two directions, because a false orphan invites
# someone to delete live code:
#   * Specifiers are matched textually, so a path that only appears inside a
#     comment still counts as an edge. Over-reaching keeps live code alive.
#   * Test files (`*.test.*`, `*.spec.*`, `__tests__/`, `__mocks__/`) are
#     neither roots nor reported. A module reachable *only* from its own test
#     is still an orphan — that is precisely how `ProviderGalleryCard.tsx` and
#     `lib/agentStatus.ts` survived: co-located tests kept them looking used.
#
# Ratchet model (same shape as check_no_orphan_modules.sh): a file knowingly
# kept unreachable can be parked in `ts_orphan_modules_baseline.txt` for a WARN
# instead of a FAIL. That file is empty and should stay that way.
#
# Usage:
#   scripts/internal/check_ts_orphan_modules.sh            # gate (exit 1 on new orphans)
#   scripts/internal/check_ts_orphan_modules.sh --report   # list only, always exit 0
#   SCAN_ROOT=src scripts/internal/check_ts_orphan_modules.sh --report
#          # widen the *reported* scope beyond src/features (reachability is
#          # always computed over all of src/); useful when auditing.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

MODE="gate"
if [ "${1:-}" = "--report" ]; then
  MODE="report"
fi

SCAN_ROOT="${SCAN_ROOT:-src/features}"

MODE="$MODE" SCAN_ROOT="$SCAN_ROOT" python3 - <<'PY'
from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path.cwd()
SRC = ROOT / "src"
MODE = os.environ["MODE"]
SCAN_ROOT = ROOT / os.environ["SCAN_ROOT"]

SOURCE_EXTS = (".ts", ".tsx")
TEST_MARKERS = (".test.", ".spec.")
TEST_DIRS = ("__tests__", "__mocks__")

# One pattern per specifier shape. `from "X"` covers both `import … from` and
# `export … from`; the bare/dynamic/require forms need their own anchors.
SPECIFIER_PATTERNS = [
    re.compile(r"""\bfrom\s*["']([^"']+)["']"""),
    re.compile(r"""\bimport\s*\(\s*["']([^"']+)["']"""),
    re.compile(r"""\bimport\s+["']([^"']+)["']"""),
    re.compile(r"""\brequire\s*\(\s*["']([^"']+)["']"""),
]


def is_test_path(path: Path) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    if any(f"/{d}/" in f"/{rel}" for d in TEST_DIRS):
        return True
    return any(marker in path.name for marker in TEST_MARKERS)


def source_files(base: Path) -> list[Path]:
    found: list[Path] = []
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = [d for d in dirnames if d not in ("node_modules", "dist", "target")]
        for fn in filenames:
            if fn.endswith(SOURCE_EXTS):
                found.append((Path(dirpath) / fn).resolve())
    return found


def resolve(spec: str, importer: Path) -> Path | None:
    """Map a module specifier to a file on disk, or None for a package."""
    if spec.startswith("@/"):
        base = SRC / spec[2:]
    elif spec.startswith("."):
        base = importer.parent / spec
    else:
        return None  # bare package specifier

    # `./foo.js` in TS source means `./foo.ts` / `./foo.tsx` on disk.
    stems = [base]
    if base.suffix in (".js", ".jsx"):
        stems.append(base.with_suffix(""))

    for stem in stems:
        candidates = [
            stem,
            stem.with_name(stem.name + ".ts"),
            stem.with_name(stem.name + ".tsx"),
            stem.with_name(stem.name + ".d.ts"),
            stem / "index.ts",
            stem / "index.tsx",
        ]
        for cand in candidates:
            if cand.is_file() and cand.suffix in (".ts", ".tsx"):
                return cand.resolve()
    return None


def edges(path: Path) -> list[str]:
    try:
        src = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    specs: list[str] = []
    for pattern in SPECIFIER_PATTERNS:
        specs.extend(pattern.findall(src))
    return specs


def reach(roots: list[Path]) -> set[Path]:
    seen: set[Path] = set()
    stack = [r.resolve() for r in roots if r.is_file()]
    while stack:
        f = stack.pop()
        if f in seen:
            continue
        seen.add(f)
        for spec in edges(f):
            target = resolve(spec, f)
            if target is not None and target not in seen:
                stack.append(target)
    return seen


roots: list[Path] = []
main_entry = SRC / "main.tsx"
if main_entry.is_file():
    roots.append(main_entry)
pages_dir = SRC / "pages"
if pages_dir.is_dir():
    roots.extend(p for p in source_files(pages_dir) if not is_test_path(p))

if not roots:
    print("✗ No entry points found (expected src/main.tsx and/or src/pages/).")
    sys.exit(1)

reached = reach(roots)

if not SCAN_ROOT.is_dir():
    print(f"no {SCAN_ROOT.relative_to(ROOT)} directory — nothing to check")
    sys.exit(0)

scanned = [p for p in source_files(SCAN_ROOT) if not is_test_path(p)]
orphans = sorted(os.path.relpath(p, ROOT) for p in set(scanned) - reached)

baseline_path = Path("scripts/internal/ts_orphan_modules_baseline.txt")
baseline: set[str] = set()
if baseline_path.exists():
    for raw in baseline_path.read_text().splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            baseline.add(line)


def line_count(rel: str) -> int:
    try:
        return len(Path(rel).read_text(encoding="utf-8", errors="replace").splitlines())
    except OSError:
        return 0


new_violations = 0
warned: set[str] = set()
dead_lines = 0
for rel in orphans:
    lines = line_count(rel)
    dead_lines += lines
    hint = f"{lines} lines never reached from an entry point"
    if rel in baseline:
        print(f"WARN  {rel}  (baselined orphan; {hint})")
        warned.add(rel)
    else:
        print(f"FAIL  {rel}  ({hint})")
        new_violations += 1

stale = 0
for rel in sorted(baseline - warned):
    print(f"STALE      {rel}  (now reachable — remove from baseline)")
    stale += 1

print(
    f"\nsummary: {new_violations} new orphan module(s) ({dead_lines} lines), "
    f"{len(warned)} baselined orphan(s), {stale} stale baseline entry/entries "
    f"({len(scanned)} .ts/.tsx files checked from {len(roots)} entry point(s); "
    f"{len(reached)} files reachable)."
)

if MODE == "report":
    print("(report mode — not failing)")
    sys.exit(0)

if new_violations:
    print(
        "✗ The file(s) above are not reachable from src/main.tsx or src/pages/. "
        "Nothing renders them, so lint/build/test green says nothing about them. "
        "Wire them into the render tree, or delete them."
    )
    sys.exit(1)
print("✓ No new TS orphan modules — every src/features file is reachable from an entry point.")
PY
