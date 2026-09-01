#!/usr/bin/env bash
# dev.sh — pull the latest code and launch Tauri dev mode.
#
#   ./dev.sh               pull + bun install + hooks + bun tauri dev
#   ./dev.sh --no-pull     skip the pull, just install and launch
#   ./dev.sh --no-install  skip bun install
#
# Bun is the local toolchain (package.json scripts assume it). A dependency
# change must keep bun.lock and package-lock.json in sync, so after pulling a
# commit that touched package.json this script runs `bun install` so
# node_modules matches the lock you actually build with.
set -euo pipefail

die() { printf 'dev.sh: %s\n' "$1" >&2; exit 1; }

command -v git >/dev/null 2>&1 || die "git not found on PATH"
command -v bun  >/dev/null 2>&1 || die "bun not found on PATH (https://bun.sh)"

ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" \
  || die "not inside a git work tree — run this from the SkillStar clone"
cd "$ROOT"

PULL=1
INSTALL=1
for arg in "$@"; do
  case "$arg" in
    --no-pull)    PULL=0 ;;
    --no-install) INSTALL=0 ;;
    -h|--help)    sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)            die "unknown argument: $arg (try --help)" ;;
  esac
done

if [ "$PULL" -eq 1 ]; then
  branch="$(git branch --show-current)"
  printf '==> git pull --ff-only (%s)\n' "${branch:-detached HEAD}"
  # A diverged branch stops here instead of silently creating a merge commit;
  # rebase or merge is the developer's explicit choice, not this script's.
  git pull --ff-only
fi

if [ "$INSTALL" -eq 1 ]; then
  printf '==> bun install\n'
  bun install
fi

# Hooks are per-clone and do not survive a checkout of a branch that has never
# carried them. Reinstall whenever the managed marker is missing so a pull
# always ends up guarded.
HOOKS_DIR="$(git rev-parse --git-path hooks)"
if ! grep -qF "# skillstar-managed-hook v1" "$HOOKS_DIR/pre-commit" 2>/dev/null; then
  printf '==> git hooks missing, installing\n'
  bash scripts/internal/install_hooks.sh
fi

printf '==> bun tauri dev\n'
exec bun tauri dev
