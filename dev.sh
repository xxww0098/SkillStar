#!/usr/bin/env bash
# dev.sh — pull the latest code and launch the GPUI shell.
#
#   ./dev.sh               pull + hooks + cargo run -p skillstar
#   ./dev.sh --no-pull     skip the pull, just launch
#
# The product binary is skillstar. No arguments open the GPUI shell.
# skillstar list / find / install and the other CLI subcommands stay on
# that same binary. There is no Tauri or Bun path.
set -euo pipefail

die() { printf 'dev.sh: %s\n' "$1" >&2; exit 1; }

command -v git >/dev/null 2>&1 || die "git not found on PATH"
command -v cargo >/dev/null 2>&1 || die "cargo not found on PATH (https://rustup.rs)"

ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" \
  || die "not inside a git work tree — run this from the SkillStar clone"
cd "$ROOT"

PULL=1
for arg in "$@"; do
  case "$arg" in
    --no-pull)    PULL=0 ;;
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

# Hooks are per-clone and do not survive a checkout of a branch that has never
# carried them. Reinstall whenever the managed marker is missing so a pull
# always ends up guarded.
HOOKS_DIR="$(git rev-parse --git-path hooks)"
if ! grep -qF "# skillstar-managed-hook v1" "$HOOKS_DIR/pre-commit" 2>/dev/null; then
  printf '==> git hooks missing, installing\n'
  bash scripts/internal/install_hooks.sh
fi

printf '==> cargo run -p skillstar --locked\n'
exec cargo run -p skillstar --locked
