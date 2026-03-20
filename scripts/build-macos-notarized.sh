#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="$(mktemp -d /tmp/shiliu-build.XXXXXX)"
OUTPUT_DIR="$ROOT_DIR/release-artifacts"

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

echo "Using temporary build directory: $BUILD_DIR"
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

export APPLE_SIGNING_IDENTITY
unset APPLE_API_ISSUER
unset APPLE_API_KEY
unset APPLE_API_KEY_PATH

npm run tauri build

APP_PATH="$(find "$BUILD_DIR/src-tauri/target/release/bundle/macos" -maxdepth 1 -name '*.app' -print -quit)"
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

LIB_DIR="$APP_PATH/Contents/Resources/ffmpeg-libs"
if [[ -d "$LIB_DIR" ]]; then
  while IFS= read -r -d '' dylib; do
    codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$dylib"
  done < <(find "$LIB_DIR" -type f -name '*.dylib' -print0)
fi

codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp --options runtime "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH"

ZIP_PATH="$BUILD_DIR/src-tauri/target/release/bundle/macos/${PRODUCT_NAME}-notarize.zip"
rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"

xcrun notarytool submit "$ZIP_PATH" \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" \
  --wait

xcrun stapler staple "$APP_PATH"

DMG_PATH="$BUILD_DIR/src-tauri/target/release/bundle/dmg/${PRODUCT_NAME}_${VERSION}_aarch64_notarized.dmg"
mkdir -p "$(dirname "$DMG_PATH")"
rm -f "$DMG_PATH"
hdiutil create -volname "$PRODUCT_NAME" -srcfolder "$APP_PATH" -ov -format UDZO "$DMG_PATH"

xcrun notarytool submit "$DMG_PATH" \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" \
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
