#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
STAGE_DIR="$(mktemp -d /private/tmp/quotamate-macos-stage.XXXXXX)"
TARGET_DIR="/private/tmp/quotamate-macos-target"
OUTPUT_DIR="$ROOT_DIR/artifacts/macos-arm64"

cleanup() {
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
  --exclude='.pnpm-store' \
  --exclude='artifacts' \
  --exclude='node_modules' \
  --exclude='src-tauri/target' \
  "$ROOT_DIR/" "$STAGE_DIR/"

cd "$STAGE_DIR"
CARGO_TARGET_DIR="$TARGET_DIR" node "$ROOT_DIR/node_modules/@tauri-apps/cli/tauri.js" \
  build \
  --bundles app,dmg \
  --no-sign \
  --ci \
  --config "$ROOT_DIR/src-tauri/tauri.build.macos.conf.json"

mkdir -p "$OUTPUT_DIR"
ditto "$TARGET_DIR/release/bundle/macos/QuotaMate.app" "$OUTPUT_DIR/QuotaMate.app"
cp "$TARGET_DIR/release/bundle/dmg/QuotaMate_0.1.0_aarch64.dmg" "$OUTPUT_DIR/QuotaMate_0.1.0_aarch64.dmg"

echo "macOS bundles are available in $OUTPUT_DIR"
