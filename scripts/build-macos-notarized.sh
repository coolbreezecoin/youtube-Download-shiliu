#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="$(mktemp -d /tmp/shiliu-build.XXXXXX)"
OUTPUT_DIR="$ROOT_DIR/release-artifacts"
HOST_ARCH="$(uname -m)"

if [[ -d "/opt/homebrew/opt/rustup/bin" ]]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

case "${TARGET_TRIPLE:-}" in
  "")
    case "$HOST_ARCH" in
      arm64)
        TARGET_TRIPLE="aarch64-apple-darwin"
        ARTIFACT_ARCH="aarch64"
        ;;
      x86_64)
        TARGET_TRIPLE="x86_64-apple-darwin"
        ARTIFACT_ARCH="x64"
        ;;
      *)
        echo "Unsupported macOS host architecture: $HOST_ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  aarch64-apple-darwin)
    ARTIFACT_ARCH="aarch64"
    ;;
  x86_64-apple-darwin)
    ARTIFACT_ARCH="x64"
    ;;
  *)
    echo "Unsupported TARGET_TRIPLE: ${TARGET_TRIPLE}" >&2
    exit 1
    ;;
esac

case "$TARGET_TRIPLE" in
  x86_64-apple-darwin)
    FFMPEG_LIB_DIR_NAME="ffmpeg-libs-x86_64-apple-darwin"
    PYTHON_RUNTIME_DIR_NAME="python-runtime-x86_64-apple-darwin"
    ;;
  *)
    FFMPEG_LIB_DIR_NAME="ffmpeg-libs"
    PYTHON_RUNTIME_DIR_NAME="python-runtime-aarch64-apple-darwin"
    ;;
esac

required_envs=(
  APPLE_SIGNING_IDENTITY
  APPLE_API_ISSUER
  APPLE_API_KEY
  APPLE_API_KEY_PATH
)

for var_name in "${required_envs[@]}"; do
  if [[ -z "${!var_name:-}" ]]; then
    echo "Missing required environment variable: $var_name" >&2
    exit 1
  fi
done

if [[ ! -f "$APPLE_API_KEY_PATH" ]]; then
  echo "APPLE_API_KEY_PATH does not exist: $APPLE_API_KEY_PATH" >&2
  exit 1
fi

NOTARY_APPLE_API_ISSUER="$APPLE_API_ISSUER"
NOTARY_APPLE_API_KEY="$APPLE_API_KEY"
NOTARY_APPLE_API_KEY_PATH="$APPLE_API_KEY_PATH"

echo "Using temporary build directory: $BUILD_DIR"
echo "Target triple: $TARGET_TRIPLE"
mkdir -p "$OUTPUT_DIR"

cleanup() {
  rm -rf "$BUILD_DIR"
}
trap cleanup EXIT

rsync -a \
  --exclude ".git" \
  --exclude "src-tauri/target" \
  --exclude "release-artifacts" \
  "$ROOT_DIR/" "$BUILD_DIR/"

xattr -cr "$BUILD_DIR"

pushd "$BUILD_DIR" >/dev/null

case "$TARGET_TRIPLE" in
  aarch64-apple-darwin)
    rm -rf src-tauri/resources/python-runtime-x86_64-apple-darwin
    mkdir -p src-tauri/resources/python-runtime-x86_64-apple-darwin
    ;;
  x86_64-apple-darwin)
    rm -rf src-tauri/resources/python-runtime-aarch64-apple-darwin
    mkdir -p src-tauri/resources/python-runtime-aarch64-apple-darwin
    rm -rf src-tauri/resources/ffmpeg-libs
    mkdir -p src-tauri/resources/ffmpeg-libs
    ;;
esac

export APPLE_SIGNING_IDENTITY
unset APPLE_API_ISSUER
unset APPLE_API_KEY
unset APPLE_API_KEY_PATH

bash scripts/fetch-platform-assets.sh "$TARGET_TRIPLE"

bash scripts/validate-platform-assets.sh "$TARGET_TRIPLE"

npm run tauri build -- --target "$TARGET_TRIPLE"

BUNDLE_DIR="$BUILD_DIR/src-tauri/target/$TARGET_TRIPLE/release/bundle"

APP_PATH="$(find "$BUNDLE_DIR/macos" -maxdepth 1 -name '*.app' -print -quit)"
if [[ -z "$APP_PATH" ]]; then
  echo "Signed app bundle not found after build." >&2
  exit 1
fi

VERSION="$(node -p "require('./package.json').version")"
PRODUCT_NAME="$(python3 - <<'PY'
import json
from pathlib import Path
config = json.loads(Path("src-tauri/tauri.conf.json").read_text())
print(config["productName"])
PY
)"

LIB_DIR="$APP_PATH/Contents/Resources/$FFMPEG_LIB_DIR_NAME"
if [[ -d "$LIB_DIR" ]]; then
  while IFS= read -r -d '' dylib; do
    codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$dylib"
  done < <(find "$LIB_DIR" -type f -name '*.dylib' -print0)
fi

PYTHON_RUNTIME_DIR="$(find "$APP_PATH/Contents/Resources" -type d -name "$PYTHON_RUNTIME_DIR_NAME" -print -quit)"
if [[ -n "$PYTHON_RUNTIME_DIR" ]]; then
  while IFS= read -r -d '' macho; do
    if file -b "$macho" | grep -q 'Mach-O'; then
      if [[ -x "$macho" && "$macho" != *.dylib && "$macho" != *.so ]]; then
        codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp --options runtime "$macho"
      else
        codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$macho"
      fi
    fi
  done < <(find "$PYTHON_RUNTIME_DIR" -type f -print0)
fi

codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp --options runtime "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"

ZIP_PATH="$BUNDLE_DIR/macos/${PRODUCT_NAME}-notarize.zip"
rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"

xcrun notarytool submit "$ZIP_PATH" \
  --key "$NOTARY_APPLE_API_KEY_PATH" \
  --key-id "$NOTARY_APPLE_API_KEY" \
  --issuer "$NOTARY_APPLE_API_ISSUER" \
  --wait

xcrun stapler staple "$APP_PATH"

DMG_PATH="$BUNDLE_DIR/dmg/${PRODUCT_NAME}_${VERSION}_${ARTIFACT_ARCH}_notarized.dmg"
mkdir -p "$(dirname "$DMG_PATH")"
rm -f "$DMG_PATH"
hdiutil create -volname "$PRODUCT_NAME" -srcfolder "$APP_PATH" -ov -format UDZO "$DMG_PATH"

xcrun notarytool submit "$DMG_PATH" \
  --key "$NOTARY_APPLE_API_KEY_PATH" \
  --key-id "$NOTARY_APPLE_API_KEY" \
  --issuer "$NOTARY_APPLE_API_ISSUER" \
  --wait

xcrun stapler staple "$DMG_PATH"

spctl -a -vv "$APP_PATH"

cp -R "$APP_PATH" "$OUTPUT_DIR/"
cp "$DMG_PATH" "$OUTPUT_DIR/"

popd >/dev/null

echo
echo "Finished."
echo "App: $OUTPUT_DIR/$(basename "$APP_PATH")"
echo "DMG: $OUTPUT_DIR/$(basename "$DMG_PATH")"
