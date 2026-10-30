#!/usr/bin/env bash
set -euo pipefail
umask 022

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This build script must run on Linux." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
for tool in node pnpm cargo rustc pkg-config python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required. See README.md for Ubuntu build dependencies." >&2
    exit 1
  fi
done
pkg-config --exists gtk+-3.0 webkit2gtk-4.1 ayatana-appindicator3-0.1 || {
  echo "Install the Ubuntu development packages listed in README.md." >&2
  exit 1
}

APP_VERSION="$(node -p "require('./package.json').version")"
case "$(uname -m)" in
  x86_64) ARCH=x64 ;;
  aarch64) ARCH=arm64 ;;
  *) echo "Unsupported Linux architecture: $(uname -m)" >&2; exit 1 ;;
esac

# A native filesystem avoids permission/symlink problems on external NTFS disks.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${TMPDIR:-/tmp}/quotamate-linux-target}"
mkdir -p "$CARGO_TARGET_DIR"
export CARGO_TARGET_DIR="$(cd "$CARGO_TARGET_DIR" && pwd)"
OUTPUT_DIR="$ROOT_DIR/artifacts/linux-$ARCH"
BUNDLES="${TAURI_BUNDLES:-deb,appimage}"
if [[ ",$BUNDLES," == *,appimage,* ]] && ! command -v unsquashfs >/dev/null 2>&1; then
  echo "Install squashfs-tools to verify AppImage permissions before publishing." >&2
  exit 1
fi
pnpm tauri build --ci --bundles "$BUNDLES"

BUILD_DIR="$CARGO_TARGET_DIR/${CARGO_BUILD_TARGET:+$CARGO_BUILD_TARGET/}release"
mkdir -p "$OUTPUT_DIR"
shopt -s nullglob
PACKAGES=("$BUILD_DIR"/bundle/deb/*_"$APP_VERSION"_*.deb "$BUILD_DIR"/bundle/appimage/*_"$APP_VERSION"_*.AppImage)
if [[ ${#PACKAGES[@]} -eq 0 ]]; then
  echo "No Linux bundles were produced." >&2
  exit 1
fi
for package in "${PACKAGES[@]}"; do
  if [[ "$package" == *.AppImage ]]; then
    python3 "$ROOT_DIR/scripts/check_appimage.py" "$package"
  fi
  cp "$package" "$OUTPUT_DIR/"
done
cp "$ROOT_DIR/docs/ubuntu-first-run.md" "$OUTPUT_DIR/ubuntu-first-run.md"
(
  cd "$OUTPUT_DIR"
  sha256sum "${PACKAGES[@]##*/}" > SHA256SUMS
)
echo "Linux bundles are available in $OUTPUT_DIR"
