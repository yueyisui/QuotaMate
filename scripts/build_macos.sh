#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_VERSION="$(node -p "require('$ROOT_DIR/package.json').version")"
STAGE_DIR="$(mktemp -d /private/tmp/quotamate-macos-stage.XXXXXX)"
TARGET_DIR="/private/tmp/quotamate-macos-target"
OUTPUT_DIR="$ROOT_DIR/artifacts/macos-arm64"
VERIFY_MOUNT="$STAGE_DIR/verify-mount"

cleanup() {
  if mount | grep -Fq " on $VERIFY_MOUNT "; then
    hdiutil detach "$VERIFY_MOUNT" >/dev/null || true
  fi
  case "$STAGE_DIR" in
    /private/tmp/quotamate-macos-stage.*) rm -rf -- "$STAGE_DIR" ;;
  esac
}
trap cleanup EXIT

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This build script must run on macOS." >&2
  exit 1
fi

if ! command -v pnpm >/dev/null 2>&1; then
  echo "pnpm is required. Install it before building QuotaMate." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1 && [[ -x /opt/homebrew/opt/rustup/bin/cargo ]]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is required. Install rustup and the stable toolchain first." >&2
  exit 1
fi

cd "$ROOT_DIR"
pnpm build

rsync -a \
  --exclude='._*' \
  --exclude='.git' \
  --exclude='.pnpm-store' \
  --exclude='artifacts' \
  --exclude='node_modules' \
  --exclude='src-tauri/target' \
  "$ROOT_DIR/" "$STAGE_DIR/"

cd "$STAGE_DIR"
CARGO_TARGET_DIR="$TARGET_DIR" node "$ROOT_DIR/node_modules/@tauri-apps/cli/tauri.js" \
  build \
  --bundles app \
  --no-sign \
  --ci \
  --config "$ROOT_DIR/src-tauri/tauri.build.macos.conf.json"

# --no-sign skips Tauri's paid identity flow. The linker signature alone does
# not seal an app bundle: sign the completed app, including Info.plist/resources.
BUILT_APP="$TARGET_DIR/release/bundle/macos/QuotaMate.app"
codesign --force --sign - "$BUILT_APP"
codesign --verify --deep --strict --verbose=2 "$BUILT_APP"
ACTUAL_VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$BUILT_APP/Contents/Info.plist")
if [[ "$ACTUAL_VERSION" != "$APP_VERSION" ]]; then
  echo "App version mismatch: expected $APP_VERSION, got $ACTUAL_VERSION" >&2
  exit 1
fi

DMG_SOURCE="$STAGE_DIR/installer"
DMG_PATH="$STAGE_DIR/QuotaMate_${APP_VERSION}_aarch64.dmg"
mkdir -p "$DMG_SOURCE" "$VERIFY_MOUNT" "$OUTPUT_DIR"
ditto "$BUILT_APP" "$DMG_SOURCE/QuotaMate.app"
ln -s /Applications "$DMG_SOURCE/Applications"
cp "$ROOT_DIR/docs/macos-first-run.txt" "$DMG_SOURCE/首次打开说明 - First Launch.txt"
hdiutil create -volname QuotaMate -srcfolder "$DMG_SOURCE" -format UDZO "$DMG_PATH"
hdiutil verify "$DMG_PATH"
# Validate what users actually install, not just the pre-packaged source app.
hdiutil attach -readonly -nobrowse -mountpoint "$VERIFY_MOUNT" "$DMG_PATH"
codesign --verify --deep --strict --verbose=2 "$VERIFY_MOUNT/QuotaMate.app"
hdiutil detach "$VERIFY_MOUNT"
ditto "$BUILT_APP" "$OUTPUT_DIR/QuotaMate.app"
cp "$DMG_PATH" "$OUTPUT_DIR/QuotaMate_${APP_VERSION}_aarch64.dmg"
shasum -a 256 "$OUTPUT_DIR/QuotaMate_${APP_VERSION}_aarch64.dmg"

echo "macOS bundles are available in $OUTPUT_DIR"
