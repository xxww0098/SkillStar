#!/bin/bash
# Cross-build the skillstar Windows exe on macOS.
# Usage: ./scripts/build_windows_cross.sh
# Output: target/x86_64-pc-windows-msvc/release/skillstar.exe
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

info() { echo -e "${CYAN}i ${NC}$1"; }
ok()   { echo -e "${GREEN}ok ${NC}$1"; }
err()  { echo -e "${RED}x ${NC}$1"; exit 1; }

LLVM_PREFIX="/opt/homebrew/opt/llvm"

info "Checking prerequisites..."
command -v cargo >/dev/null 2>&1 || err "cargo is not installed. Install Rust: https://rustup.rs"
command -v rustup >/dev/null 2>&1 || err "rustup is not installed."
command -v cargo-xwin >/dev/null 2>&1 || err "cargo-xwin is not installed. Run: cargo install cargo-xwin"
[ -x "${LLVM_PREFIX}/bin/llvm-rc" ] || err "llvm-rc not found at ${LLVM_PREFIX}/bin/llvm-rc. Run: brew install llvm"

export PATH="${LLVM_PREFIX}/bin:${PATH}"

if ! rustup target list --installed | grep -q "^x86_64-pc-windows-msvc$"; then
  info "Adding Rust target x86_64-pc-windows-msvc..."
  rustup target add x86_64-pc-windows-msvc
fi

info "Cross-building skillstar..."
cargo xwin build --release --locked -p skillstar --target x86_64-pc-windows-msvc
ok "Windows cross-build complete"

OUT_EXE="target/x86_64-pc-windows-msvc/release/skillstar.exe"
if [ -f "$OUT_EXE" ]; then
  SIZE="$(du -h "$OUT_EXE" | cut -f1)"
  ok "EXE: $OUT_EXE ($SIZE)"
else
  err "Build finished but EXE not found: $OUT_EXE"
fi
