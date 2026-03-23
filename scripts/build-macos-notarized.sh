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

python_runtime_dir_name() {
  case "$1" in
    x86_64-apple-darwin)
      echo "python-runtime-x86_64-apple-darwin"
      ;;
    *)
      echo "python-runtime-aarch64-apple-darwin"
      ;;
  esac
}

ffmpeg_resource_dir_name() {
  case "$1" in
    x86_64-apple-darwin)
      echo "ffmpeg-libs-x86_64-apple-darwin"
      ;;
    *)
      echo "ffmpeg-libs"
      ;;
  esac
}

sign_macho_tree() {
  local tree_root="$1"

  if [[ ! -d "$tree_root" ]]; then
    return
  fi

  while IFS= read -r -d '' target; do
    local description
    description="$(file -b "$target")"

    if [[ "$description" != *Mach-O* ]]; then
      continue
    fi

    if [[ "$target" == *.dylib || "$target" == *.so ]]; then
      codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$target"
      continue
    fi

    if [[ "$target" == *"/bin/"* || -x "$target" ]]; then
      codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp --options runtime "$target"
    else
      codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$target"
    fi
  done < <(find "$tree_root" -type f -print0)
}

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

bash scripts/fetch-platform-assets.sh "$TARGET_TRIPLE"

bash scripts/validate-platform-assets.sh "$TARGET_TRIPLE"

npm run tauri build -- --target "$TARGET_TRIPLE"

BUNDLE_DIR="$BUILD_DIR/src-tauri/target/$TARGET_TRIPLE/release/bundle"

APP_PATH="$(find "$BUNDLE_DIR/macos" -maxdepth 1 -name '*.app' -print -quit)"
if [[ -z "$APP_PATH" ]]; then
  echo "Signed app bundle not found after build." >&2
  exit 1
fi

SIGNED_APP_PATH="$BUILD_DIR/notarize/$(basename "$APP_PATH")"
rm -rf "$SIGNED_APP_PATH"
mkdir -p "$(dirname "$SIGNED_APP_PATH")"
ditto "$APP_PATH" "$SIGNED_APP_PATH"
xattr -cr "$SIGNED_APP_PATH"

TARGET_PYTHON_RUNTIME="$(python_runtime_dir_name "$TARGET_TRIPLE")"
TARGET_FFMPEG_LIBS="$(ffmpeg_resource_dir_name "$TARGET_TRIPLE")"
RESOURCES_DIR="$SIGNED_APP_PATH/Contents/Resources"

find "$RESOURCES_DIR" -maxdepth 1 -type d -name 'python-runtime-*' ! -name "$TARGET_PYTHON_RUNTIME" -exec rm -rf {} +

if [[ "$TARGET_FFMPEG_LIBS" == "ffmpeg-libs" ]]; then
  rm -rf "$RESOURCES_DIR/ffmpeg-libs-x86_64-apple-darwin"
else
  rm -rf "$RESOURCES_DIR/ffmpeg-libs"
fi

sign_macho_tree "$RESOURCES_DIR/$TARGET_PYTHON_RUNTIME"
sign_macho_tree "$RESOURCES_DIR/$TARGET_FFMPEG_LIBS"

VERSION="$(node -p "require('./package.json').version")"
PRODUCT_NAME="$(python3 - <<'PY'
import json
from pathlib import Path
config = json.loads(Path("src-tauri/tauri.conf.json").read_text())
print(config["productName"])
PY
)"

codesign --force --deep --sign "$APPLE_SIGNING_IDENTITY" --timestamp --options runtime "$SIGNED_APP_PATH"
codesign --verify --deep --strict --verbose=2 "$SIGNED_APP_PATH"

ZIP_PATH="$BUNDLE_DIR/macos/${PRODUCT_NAME}-notarize.zip"
rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$SIGNED_APP_PATH" "$ZIP_PATH"

xcrun notarytool submit "$ZIP_PATH" \
  --key "$NOTARY_APPLE_API_KEY_PATH" \
  --key-id "$NOTARY_APPLE_API_KEY" \
  --issuer "$NOTARY_APPLE_API_ISSUER" \
  --wait

xcrun stapler staple "$SIGNED_APP_PATH"

DMG_PATH="$BUNDLE_DIR/dmg/${PRODUCT_NAME}_${VERSION}_${ARTIFACT_ARCH}_notarized.dmg"
STAGING_DIR="$BUILD_DIR/dmg-staging"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
ditto "$SIGNED_APP_PATH" "$STAGING_DIR/$(basename "$SIGNED_APP_PATH")"
rm -f "$DMG_PATH"
hdiutil create -volname "$PRODUCT_NAME" -srcfolder "$STAGING_DIR" -ov -format UDZO "$DMG_PATH" >/dev/null
codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$DMG_PATH"

xcrun notarytool submit "$DMG_PATH" \
  --key "$NOTARY_APPLE_API_KEY_PATH" \
  --key-id "$NOTARY_APPLE_API_KEY" \
  --issuer "$NOTARY_APPLE_API_ISSUER" \
  --wait

xcrun stapler staple "$DMG_PATH"

spctl -a -vv "$SIGNED_APP_PATH"

rm -rf "$OUTPUT_DIR/$(basename "$SIGNED_APP_PATH")"
rm -f "$OUTPUT_DIR/$(basename "$DMG_PATH")"
ditto "$SIGNED_APP_PATH" "$OUTPUT_DIR/$(basename "$SIGNED_APP_PATH")"
cp "$DMG_PATH" "$OUTPUT_DIR/"

popd >/dev/null

echo
echo "Finished."
echo "App: $OUTPUT_DIR/$(basename "$SIGNED_APP_PATH")"
echo "DMG: $OUTPUT_DIR/$(basename "$DMG_PATH")"
