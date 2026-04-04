#!/bin/bash
# ─────────────────────────────────────────────────────────────
# run-build-windows-cross.sh — Cross-build SkillStar Windows .exe on macOS
# Usage: ./run-build-windows-cross.sh
# Output: src-tauri/target/x86_64-pc-windows-msvc/release/skillstar.exe
# ─────────────────────────────────────────────────────────────
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# ── Colors ───────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info()  { echo -e "${CYAN}ℹ ${NC}$1"; }
ok()    { echo -e "${GREEN}✔ ${NC}$1"; }
warn()  { echo -e "${YELLOW}⚠ ${NC}$1"; }
err()   { echo -e "${RED}✖ ${NC}$1"; exit 1; }

LLVM_PREFIX="/opt/homebrew/opt/llvm"

info "Checking prerequisites..."
command -v bun >/dev/null 2>&1 || err "bun is not installed. Install it: https://bun.sh"
command -v cargo >/dev/null 2>&1 || err "cargo is not installed. Install Rust: https://rustup.rs"
command -v rustup >/dev/null 2>&1 || err "rustup is not installed."
command -v cargo-xwin >/dev/null 2>&1 || err "cargo-xwin is not installed. Run: cargo install cargo-xwin"
[ -x "${LLVM_PREFIX}/bin/llvm-rc" ] || err "llvm-rc not found at ${LLVM_PREFIX}/bin/llvm-rc. Run: brew install llvm"

export PATH="${LLVM_PREFIX}/bin:${PATH}"

if ! rustup target list --installed | grep -q "^x86_64-pc-windows-msvc$"; then
  info "Adding Rust target x86_64-pc-windows-msvc..."
  rustup target add x86_64-pc-windows-msvc
fi

info "Installing frontend dependencies..."
bun install
ok "Frontend dependencies installed"

info "Building frontend..."
bun run build
ok "Frontend build complete"

info "Cross-building Windows release binary (Tauri production mode)..."
bun run tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --no-bundle
ok "Windows cross-build complete"

OUT_EXE="src-tauri/target/x86_64-pc-windows-msvc/release/skillstar.exe"
if [ -f "$OUT_EXE" ]; then
  SIZE="$(du -h "$OUT_EXE" | cut -f1)"
  ok "EXE: $OUT_EXE ($SIZE)"
else
  err "Build finished but EXE not found: $OUT_EXE"
fi

echo ""
info "Done. Copy $OUT_EXE to Windows for testing."
