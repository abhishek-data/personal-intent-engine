#!/usr/bin/env bash
# Build PIE, give it a STABLE code-signing requirement, and install it locally.
#
# Why the re-sign step exists
# ---------------------------
# `cargo tauri build` ad-hoc signs the bundle, and an ad-hoc signature's
# designated requirement is the binary's own content hash:
#
#     designated => cdhash H"d1aa8f2e..."
#
# macOS TCC keys the Accessibility grant on that requirement, so EVERY rebuild
# looks like a brand-new application and the grant you just gave stops
# matching. The symptom is the "PIE would like to control this computer"
# prompt reappearing after every install, forever.
#
# Re-signing with an identifier-only requirement makes the grant survive
# rebuilds:
#
#     designated => identifier "com.pie.desktop"
#
# This is for LOCAL DEVELOPMENT ONLY. Real releases are signed in CI with the
# stable `PIE Developers` certificate — see docs/signing.md. Nothing here
# touches or replaces that identity.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="com.pie.desktop"
BUILT="$REPO/target/release/bundle/macos/PIE.app"
INSTALLED="/Applications/PIE.app"

cd "$REPO"

echo "==> building"
cargo tauri build "$@"

echo "==> re-signing with a stable designated requirement"
codesign --force --sign - --identifier "$APP_ID" \
  -r="designated => identifier \"$APP_ID\"" \
  "$BUILT"
codesign --verify --verbose=2 "$BUILT"
codesign -d --requirements - "$BUILT" 2>&1 | grep designated

echo "==> installing to $INSTALLED"
pkill -f "PIE.app/Contents/MacOS/pie-desktop" 2>/dev/null || true
sleep 1
rm -rf "$INSTALLED"
ditto "$BUILT" "$INSTALLED"
xattr -cr "$INSTALLED"

echo "==> launching"
open -a "$INSTALLED"

cat <<'EOF'

Installed. If this is the first install after switching to the stable
requirement, macOS still holds the old hash-based grants — clear them once:

    tccutil reset Accessibility com.pie.desktop

Then grant Accessibility when PIE next asks. From that point the grant
persists across rebuilds.
EOF
