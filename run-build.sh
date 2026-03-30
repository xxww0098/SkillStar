#!/bin/bash
# ─────────────────────────────────────────────────────────────
# run-build.sh — Build SkillStar installer for the current macOS system
# Usage: ./run-build.sh
# ─────────────────────────────────────────────────────────────
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

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

# ── Pre-flight checks ───────────────────────────────────────
info "Checking prerequisites..."

command -v bun  >/dev/null 2>&1 || err "bun is not installed. Install it: https://bun.sh"
command -v cargo >/dev/null 2>&1 || err "cargo is not installed. Install Rust: https://rustup.rs"

# ── Signing key ──────────────────────────────────────────────
SIGNING_KEY_PATH="$HOME/.tauri/skillstar.key"

if [ -f "$SIGNING_KEY_PATH" ]; then
    export TAURI_SIGNING_PRIVATE_KEY="$(cat "$SIGNING_KEY_PATH")"
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
    ok "Signing key loaded from $SIGNING_KEY_PATH"
else
    warn "No signing key found at $SIGNING_KEY_PATH"
    warn "Building WITHOUT updater signing. The .app will work but .sig files won't be generated."
    export TAURI_SIGNING_PRIVATE_KEY=""
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
fi

# ── Detect architecture ─────────────────────────────────────
ARCH="$(uname -m)"
case "$ARCH" in
    arm64)  TARGET="aarch64-apple-darwin" ;;
    x86_64) TARGET="x86_64-apple-darwin"  ;;
    *)      err "Unsupported architecture: $ARCH" ;;
esac

info "Building for macOS $ARCH ($TARGET)..."

# ── Install frontend dependencies ───────────────────────────
info "Installing frontend dependencies..."
bun install
ok "Frontend dependencies installed"

# ── Build ────────────────────────────────────────────────────
# Use --bundles app to skip DMG (AppleScript/Finder cosmetic step
# often fails locally due to permissions). DMG is handled by CI for releases.
info "Starting Tauri build (this may take a few minutes)..."
echo ""

bun run tauri build --target "$TARGET" --bundles app

echo ""
ok "Build complete!"

# ── Output location ──────────────────────────────────────────
BUNDLE_DIR="src-tauri/target/${TARGET}/release/bundle"
APP_PATH="$BUNDLE_DIR/macos/SkillStar.app"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
info "Build artifacts:"
echo ""

if [ -d "$APP_PATH" ]; then
    APP_SIZE=$(du -sh "$APP_PATH" | cut -f1)
    ok "APP:  $APP_PATH  ($APP_SIZE)"
else
    warn "No .app bundle found"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
info "To test: open \"$APP_PATH\""
info "To install: cp -R \"$APP_PATH\" /Applications/"
echo ""
