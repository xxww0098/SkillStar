#!/usr/bin/env bash
# Structural-governance guard: every `.rs` file inside a workspace member must
# be reachable from one of that member's cargo targets.
#
# Why this one matters more than it looks: an unreachable file is not
# compiled, not linted, not tested, and not type-checked — yet it reads
# exactly like delivered code. A file full of `#[test]` fns that no `mod`
# declaration points at is a test suite that has never run once, and no code
# review can see the difference. This is the one defect class that only a
# reachability walk can find.
#
# Reachability model (mirrors rustc's own module resolution):
#   * roots  — every `src_path` cargo reports for the member's targets, which
#              covers lib.rs, main.rs, src/bin/*.rs, tests/*.rs, benches/*.rs,
#              examples/*.rs and build.rs without hardcoding those paths.
#   * edges  — `mod name;` resolves to `<owned dir>/name.rs` or
#              `<owned dir>/name/mod.rs`, where the owned dir is the file's
#              own directory for lib.rs/main.rs/mod.rs and `<stem>/` otherwise.
#   * `#[path = "..."]` overrides that, relative to the directory of the file
#              containing the declaration (14 such declarations today; the
#              audit's first attempt reported 11 false orphans purely by
#              mishandling them).
#   * `#[cfg(test)] mod name;` is a normal edge — the 39 file-backed test
#              modules must count as reached, not as debt.
#   * inline `mod name { ... }` declares no file, but declarations *inside*
#              the block are still scanned.
#
# Lexing, not grepping: the source is tokenised so that `mod` / `#[path]`
# inside line comments, doc comments, block comments, string literals, raw
# strings and char literals cannot register as declarations. The audit hit
# exactly this class of false positive three times with a naive grep over
# `#[test]`; the same trap applies here and is closed by construction.
#
# Ratchet model (same shape as check_file_size.sh): a file that is knowingly
# kept unreachable can be parked in `orphan_modules_baseline.txt` for a WARN
# instead of a FAIL. That file is empty and should stay that way — today all
# .rs files in the workspace are reachable, so this gate is blocking from day
# one and the baseline exists only as the documented escape hatch.
#
# Usage: scripts/internal/check_no_orphan_modules.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

META="$(cargo metadata --no-deps --format-version 1 --locked)"

python3 - "$META" <<'PY'
from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

ROOT = Path.cwd()
meta = json.loads(sys.argv[1])

# --- tokenizer -------------------------------------------------------------
# Replace comments with whitespace and string bodies with an opaque index
# token, so that neither can masquerade as a `mod` declaration while
# `#[path = "..."]` values stay recoverable.

CHAR_LIT = re.compile(r"'(?:\\.|[^'\\])'")


def tokenize(src: str) -> tuple[str, list[str]]:
    out: list[str] = []
    strings: list[str] = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "/" and src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif c == "/" and src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
        elif c in "rb" and (m := re.match(r'[rb]{1,2}(#*)"', src[i:])):
            # raw string: r"…", r#"…"#, br#"…"#
            hashes = m.group(1)
            start = i + m.end()
            close = f'"{hashes}'
            j = src.find(close, start)
            end = n if j < 0 else j + len(close)
            strings.append(src[start : (j if j >= 0 else n)])
            out.append(f'"\x00{len(strings) - 1}\x00"')
            i = end
        elif c == '"':
            i += 1
            buf: list[str] = []
            while i < n:
                if src[i] == "\\":
                    buf.append(src[i : i + 2])
                    i += 2
                    continue
                if src[i] == '"':
                    break
                buf.append(src[i])
                i += 1
            i += 1
            strings.append("".join(buf))
            out.append(f'"\x00{len(strings) - 1}\x00"')
        elif c == "'" and (m := CHAR_LIT.match(src, i)):
            # char literal (not a lifetime) — swallow it whole so that '"'
            # cannot open a phantom string.
            out.append("' '")
            i = m.end()
        else:
            out.append(c)
            i += 1
    return "".join(out), strings


MOD_DECL = re.compile(
    r"(?:^|[;{}\s])(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*([;{])"
)
PATH_ATTR = re.compile(r'#\s*\[\s*path\s*=\s*"\x00(\d+)\x00"\s*\]')
# Only attributes / whitespace / `pub` may sit between #[path] and `mod`.
ATTR_GAP = re.compile(r"[\s;{}]*(?:#\s*\[[^\]]*\]\s*)*(?:pub(?:\s*\([^)]*\))?\s*)?")


def declarations(path: Path) -> list[tuple[str, bool, str | None]]:
    """(module name, has inline body, #[path] override) for each `mod` decl."""
    try:
        src = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    text, strings = tokenize(src)
    found = []
    for m in MOD_DECL.finditer(text):
        name, inline = m.group(1), m.group(2) == "{"
        back = text[max(0, m.start() - 400) : m.start()]
        override = None
        attrs = list(PATH_ATTR.finditer(back))
        if attrs and ATTR_GAP.fullmatch(back[attrs[-1].end() :]):
            override = strings[int(attrs[-1].group(1))]
        found.append((name, inline, override))
    return found


def candidates(parent: Path, is_dir_owner: bool, name: str, override: str | None):
    here = parent.parent
    owned = here if is_dir_owner else here / parent.stem
    if override is not None:
        # rustc resolves #[path] against the containing file's directory; the
        # owned dir is the fallback for declarations nested in inline modules.
        return [(here / override).resolve(), (owned / override).resolve()]
    return [(owned / f"{name}.rs").resolve(), (owned / name / "mod.rs").resolve()]


def reach(roots: list[Path]) -> set[Path]:
    seen: set[Path] = set()
    stack = [(r.resolve(), True) for r in roots if r.exists()]
    while stack:
        f, is_root = stack.pop()
        if f in seen:
            continue
        seen.add(f)
        dir_owner = is_root or f.name == "mod.rs"
        for name, inline, override in declarations(f):
            if inline and override is None:
                continue
            for cand in candidates(f, dir_owner, name, override):
                if cand.is_file():
                    stack.append((cand, False))
                    break
    return seen


scanned = 0
orphans: list[str] = []
for pkg in sorted(meta["packages"], key=lambda p: p["name"]):
    pkg_dir = Path(pkg["manifest_path"]).parent
    reached = reach([Path(t["src_path"]) for t in pkg["targets"]])
    on_disk: set[Path] = set()
    for sub in ("src", "tests", "benches", "examples"):
        base = pkg_dir / sub
        if not base.is_dir():
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in ("target", "node_modules")]
            for fn in filenames:
                if fn.endswith(".rs"):
                    on_disk.add((Path(dirpath) / fn).resolve())
    scanned += len(on_disk)
    for f in sorted(on_disk - reached):
        orphans.append(os.path.relpath(f, ROOT))

baseline_path = Path("scripts/internal/orphan_modules_baseline.txt")
baseline: set[str] = set()
if baseline_path.exists():
    for raw in baseline_path.read_text().splitlines():
        line = raw.strip()
        if line and not line.startswith("#"):
            baseline.add(line)

TEST_FN = re.compile(r"#\s*\[\s*(?:tokio::)?test\b")

new_violations = 0
warned: set[str] = set()
for rel in orphans:
    try:
        body = Path(rel).read_text(encoding="utf-8", errors="replace")
        text, _ = tokenize(body)
    except OSError:
        text = ""
    hint = f"{len(TEST_FN.findall(text))} #[test] fn(s) never compiled"
    if rel in baseline:
        print(f"WARN  {rel}  (baselined orphan; {hint})")
        warned.add(rel)
    else:
        print(f"FAIL  {rel}  (unreachable from any cargo target; {hint})")
        new_violations += 1

stale = 0
for rel in sorted(baseline - warned):
    print(f"STALE      {rel}  (now reachable — remove from baseline)")
    stale += 1

print(
    f"\nsummary: {new_violations} new orphan module(s), "
    f"{len(warned)} baselined orphan(s), {stale} stale baseline entry/entries "
    f"({scanned} .rs files checked for reachability)."
)
if new_violations:
    print(
        "✗ The file(s) above are not reachable from any cargo target: rustc never "
        "compiles them, so their tests never run. Add the missing `mod` declaration "
        "(or `#[path]`), or delete the file."
    )
    sys.exit(1)
print("✓ No new orphan modules — every .rs file is reachable from a cargo target.")
PY
