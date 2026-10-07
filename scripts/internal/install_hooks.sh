#!/usr/bin/env bash
# Install SkillStar's git hooks into this clone.
#
#   bash scripts/internal/install_hooks.sh              # install / update
#   bash scripts/internal/install_hooks.sh --uninstall   # remove them again
#   bash scripts/internal/install_hooks.sh --force       # overwrite a foreign hook
#
# ## Why this exists
#
# The eight structural ratchets under scripts/internal/ were all correctly
# wired into .github/workflows/ci.yml, all passing, and had still not run once
# against ten days of work: on 2026-08-14 `main` sat 22 commits ahead of
# origin/main with the most recent CI run dated 2026-08-04, and .git/hooks
# contained nothing but the 14 stock .sample files. One of those unverified
# commits is literally titled "green the CI ratchets". The checks were not
# missing and were not broken — the path that triggers them was simply not
# being walked. These hooks put the same checks on the path a developer
# actually takes.
#
# ## Why a script and not husky
#
# husky would mean a new devDependency, and therefore a package.json +
# bun.lock + package-lock.json change. Failure lesson #1 in
# .github/workflows/windows-ci.yml is that dual-lockfile drift has historically
# produced *zero* green Windows runs — `npm ci` rejects a lockfile that
# disagrees with package.json. Spending that risk to obtain a `prepare` script
# is a bad trade when a 40-line installer does the same job with no dependency,
# no lockfile churn, and no Node required to install. It also keeps the hooks
# working for contributors who use npm rather than Bun.
#
# The cost of this choice is that installation is manual and per-clone; that is
# why README documents it and why the hooks print their own escape hatch.
#
# ## The ladder (measured on an M-series Mac, warm cargo target/)
#
#   pre-commit  structural ratchets
#   pre-push    the above + cargo test --workspace --locked + clippy ratchet
#
# Both hooks are advisory in the sense that `git commit --no-verify` and
# `git push --no-verify` bypass them. That is deliberate and documented: if the
# only way past a hook during an incident is to delete it, people delete it,
# and deleted hooks do not come back.
set -uo pipefail

MARKER="# skillstar-managed-hook v1"

die() { printf 'install_hooks: %s\n' "$1" >&2; exit 1; }

command -v git >/dev/null 2>&1 || die "git not found on PATH"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 \
  || die "not inside a git work tree — run this from the SkillStar clone"

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT" || die "cannot cd to repo root"

# `--git-path hooks` resolves correctly inside linked worktrees, where
# .git is a file rather than a directory.
HOOKS_DIR="$(git rev-parse --git-path hooks)"
case "$HOOKS_DIR" in
  /*) ;;
  *) HOOKS_DIR="$REPO_ROOT/$HOOKS_DIR" ;;
esac

# If core.hooksPath is set, git ignores .git/hooks entirely and writing there
# would silently do nothing — the worst possible outcome for a guard rail.
CUSTOM_HOOKS_PATH="$(git config --get core.hooksPath 2>/dev/null || true)"
if [ -n "$CUSTOM_HOOKS_PATH" ]; then
  case "$CUSTOM_HOOKS_PATH" in
    /*) HOOKS_DIR="$CUSTOM_HOOKS_PATH" ;;
    *)  HOOKS_DIR="$REPO_ROOT/$CUSTOM_HOOKS_PATH" ;;
  esac
  printf 'install_hooks: core.hooksPath is set, installing into %s instead of .git/hooks\n' \
    "$HOOKS_DIR" >&2
fi

MODE="install"
FORCE=0
for arg in "$@"; do
  case "$arg" in
    --uninstall) MODE="uninstall" ;;
    --force)     FORCE=1 ;;
    -h|--help)
      sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) die "unknown argument: $arg (try --help)" ;;
  esac
done

mkdir -p "$HOOKS_DIR" || die "cannot create $HOOKS_DIR"

if [ "$MODE" = "uninstall" ]; then
  removed=0
  for hook in pre-commit pre-push; do
    target="$HOOKS_DIR/$hook"
    [ -e "$target" ] || continue
    if grep -qF "$MARKER" "$target" 2>/dev/null; then
      rm -f "$target" && printf 'removed %s\n' "$target" && removed=$((removed + 1))
    else
      printf 'skipped %s (not installed by this script)\n' "$target" >&2
    fi
  done
  printf 'install_hooks: removed %d hook(s).\n' "$removed"
  exit 0
fi

# Refuse to clobber a hook somebody else wrote unless explicitly forced.
for hook in pre-commit pre-push; do
  target="$HOOKS_DIR/$hook"
  if [ -e "$target" ] && ! grep -qF "$MARKER" "$target" 2>/dev/null && [ "$FORCE" -eq 0 ]; then
    die "$target already exists and was not written by this script.
       Inspect it, then re-run with --force to replace it."
  fi
done

# ---------------------------------------------------------------------------
# Shared preamble. Written into both hooks so each one is self-contained: a
# hook that sources a repo file breaks the moment you check out a branch where
# that file does not exist yet, which is precisely when you least want your
# commit path to explode.
# ---------------------------------------------------------------------------
read -r -d '' HOOK_PREAMBLE <<'PREAMBLE_EOF'
# Run from the repo root regardless of where git invoked us or which
# subdirectory the developer happened to be standing in.
if ! ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  echo "[hook] not inside a git work tree; skipping." >&2
  exit 0
fi
cd "$ROOT" || { echo "[hook] cannot cd to $ROOT; skipping." >&2; exit 0; }

FAILED=""
SKIPPED=""
START=$(date +%s)

# Run one scripts/internal gate. A gate that is absent on this branch is
# skipped with a warning rather than failing the commit: hooks outlive
# branches, and blocking a checkout of older history helps nobody.
gate() {
  script="scripts/internal/$1"
  if [ ! -f "$script" ]; then
    echo "  - $1 ... not present on this branch, skipped"
    SKIPPED="$SKIPPED $1"
    return 0
  fi
  printf '  - %s ... ' "$1"
  if out="$(bash "$script" 2>&1)"; then
    echo "ok"
  else
    echo "FAILED"
    echo "$out" | sed 's/^/      /'
    FAILED="$FAILED $1"
    [ -n "${FAIL_FAST:-}" ] && summary_and_exit
  fi
}

# Run an arbitrary command, skipping it if the tool is not installed.
step() {
  label="$1"; tool="$2"; shift 2
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "  - $label ... $tool not installed, skipped"
    SKIPPED="$SKIPPED $label"
    return 0
  fi
  printf '  - %s ... ' "$label"
  if out="$("$@" 2>&1)"; then
    echo "ok"
  else
    echo "FAILED"
    echo "$out" | tail -40 | sed 's/^/      /'
    FAILED="$FAILED $label"
    [ -n "${FAIL_FAST:-}" ] && summary_and_exit
  fi
}

summary_and_exit() {
  elapsed=$(( $(date +%s) - START ))
  if [ -n "$SKIPPED" ]; then
    echo "[$HOOK_NAME] skipped:$SKIPPED"
  fi
  if [ -n "$FAILED" ]; then
    echo ""
    echo "[$HOOK_NAME] FAILED:$FAILED  (${elapsed}s)"
    echo ""
    echo "  Fix the above, or bypass this hook deliberately with:"
    echo "      git $BYPASS_CMD --no-verify"
    echo "  Bypassing is fine for an emergency; the same checks run in CI."
    echo ""
    exit 1
  fi
  echo "[$HOOK_NAME] all checks passed (${elapsed}s)"
  exit 0
}
PREAMBLE_EOF

# ---------------------------------------------------------------------------
# pre-commit — budget <= 6 s. Measured 2026-08-14, warm: 5.4 s total.
#
# Contains every remaining structural ratchet. The React/Tauri gates
# (feature imports, command boundaries, TS orphans, biome) left with that
# stack. See D-091.
# ---------------------------------------------------------------------------
write_pre_commit() {
  cat > "$HOOKS_DIR/pre-commit" <<HOOK_EOF
#!/usr/bin/env bash
$MARKER
# Fast structural ratchets. Budget <= 6 s; measured ~5.4 s (2026-08-14).
# Bypass with: git commit --no-verify
HOOK_EOF
  cat >> "$HOOKS_DIR/pre-commit" <<'HOOK_EOF'
set -uo pipefail

HOOK_NAME="pre-commit"
BYPASS_CMD="commit"
HOOK_EOF
  printf '%s\n' "$HOOK_PREAMBLE" >> "$HOOKS_DIR/pre-commit"
  cat >> "$HOOKS_DIR/pre-commit" <<'HOOK_EOF'

echo "[pre-commit] fast ratchets (~5s)"

gate check_workspace_deps.sh
gate check_file_size.sh
gate check_error_strings.sh
gate check_no_orphan_modules.sh
gate check_dep_graph_doc.sh

summary_and_exit
HOOK_EOF
  chmod +x "$HOOKS_DIR/pre-commit"
}

# ---------------------------------------------------------------------------
# pre-push — budget <= 3 min. Measured 2026-08-14, warm target/: ~56 s.
#
# Cold-cache caveat: with an empty target/ the cargo steps dominate — CI
# (which has its own cache) measures cargo test at 4 m 37 s. First push after a
# `cargo clean` or a dependency bump will therefore blow the budget. That is
# the correct place for the cost to land: it is exactly the change that most
# needs compiling before it leaves the machine.
#
# Note there is NO separate `cargo check --workspace` step. ci.yml records the
# measurement: check (85 s) then test --no-run (115 s) = 200 s, versus 125 s
# for test --no-run alone, so check buys the later compile only ~10 s while
# costing ~75 s. `cargo test --workspace --locked` reports the same compile
# errors and strictly more of them. Do not re-add it here either.
# ---------------------------------------------------------------------------
write_pre_push() {
  cat > "$HOOKS_DIR/pre-push" <<HOOK_EOF
#!/usr/bin/env bash
$MARKER
# Full local gate before code leaves the machine. Budget <= 3 min;
# measured ~56 s warm (2026-08-14), several minutes on a cold cargo cache.
# Bypass with: git push --no-verify
HOOK_EOF
  cat >> "$HOOKS_DIR/pre-push" <<'HOOK_EOF'
set -uo pipefail

HOOK_NAME="pre-push"
BYPASS_CMD="push"
FAIL_FAST=1   # steps here are expensive; stop at the first failure.

# git feeds "<local ref> <local sha> <remote ref> <remote sha>" on stdin. A
# pure deletion (`git push --delete`) has an all-zero local sha and compiles
# nothing, so there is nothing worth gating.
DELETIONS_ONLY=1
SAW_REF=0
while read -r _lref lsha _rref _rsha; do
  [ -z "${lsha:-}" ] && continue
  SAW_REF=1
  case "$lsha" in
    *[!0]*) DELETIONS_ONLY=0 ;;
  esac
done
if [ "$SAW_REF" -eq 1 ] && [ "$DELETIONS_ONLY" -eq 1 ]; then
  echo "[pre-push] deletion-only push; nothing to check."
  exit 0
fi
HOOK_EOF
  printf '%s\n' "$HOOK_PREAMBLE" >> "$HOOKS_DIR/pre-push"
  cat >> "$HOOKS_DIR/pre-push" <<'HOOK_EOF'

echo "[pre-push] full gate (~1 min warm, several minutes on a cold cargo cache)"

# Everything pre-commit runs — a commit may have been made with --no-verify.
gate check_workspace_deps.sh
gate check_file_size.sh
gate check_error_strings.sh
gate check_no_orphan_modules.sh
gate check_dep_graph_doc.sh

step "cargo test --workspace --locked" cargo cargo test --workspace --locked
gate check_clippy_ratchet.sh

summary_and_exit
HOOK_EOF
  chmod +x "$HOOKS_DIR/pre-push"
}

write_pre_commit
write_pre_push

printf 'install_hooks: installed pre-commit and pre-push into %s\n' "$HOOKS_DIR"
printf '  pre-commit  structural ratchets\n'
printf '  pre-push    + cargo test --workspace --locked + clippy ratchet\n'
printf '  bypass      git commit --no-verify  /  git push --no-verify\n'
printf '  remove      bash scripts/internal/install_hooks.sh --uninstall\n'
