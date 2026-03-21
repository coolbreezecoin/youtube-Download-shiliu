#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_TRIPLE="${1:-}"

if [[ -z "$TARGET_TRIPLE" ]]; then
  echo "Usage: $0 <target-triple>" >&2
  exit 1
fi

missing=()

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    missing+=("$path")
  fi
}

require_non_empty_dir() {
  local path="$1"
  if [[ ! -d "$path" ]]; then
    missing+=("$path")
    return
  fi

  if [[ -z "$(find "$path" -type f ! -name '.gitkeep' -print -quit)" ]]; then
    missing+=("$path (directory exists but contains no usable files)")
  fi
}

case "$TARGET_TRIPLE" in
  aarch64-apple-darwin|x86_64-apple-darwin)
    require_file "$ROOT_DIR/src-tauri/binaries/yt-dlp-$TARGET_TRIPLE"
    require_file "$ROOT_DIR/src-tauri/binaries/ffmpeg-$TARGET_TRIPLE"
    require_file "$ROOT_DIR/src-tauri/binaries/ffprobe-$TARGET_TRIPLE"
    require_file "$ROOT_DIR/src-tauri/binaries/deno-$TARGET_TRIPLE"
    require_non_empty_dir "$ROOT_DIR/src-tauri/resources/python-runtime-$TARGET_TRIPLE"

    if [[ "$TARGET_TRIPLE" == "aarch64-apple-darwin" ]]; then
      require_non_empty_dir "$ROOT_DIR/src-tauri/resources/ffmpeg-libs"
    fi
    ;;
  x86_64-pc-windows-msvc)
    require_file "$ROOT_DIR/src-tauri/binaries/yt-dlp-$TARGET_TRIPLE.exe"
    require_file "$ROOT_DIR/src-tauri/binaries/ffmpeg-$TARGET_TRIPLE.exe"
    require_file "$ROOT_DIR/src-tauri/binaries/ffprobe-$TARGET_TRIPLE.exe"
    require_file "$ROOT_DIR/src-tauri/binaries/deno-$TARGET_TRIPLE.exe"
    ;;
  *)
    echo "Unsupported target triple: $TARGET_TRIPLE" >&2
    exit 1
    ;;
esac

if (( ${#missing[@]} > 0 )); then
  echo "Missing platform assets for $TARGET_TRIPLE:" >&2
  printf '  - %s\n' "${missing[@]}" >&2
  exit 1
fi

echo "Platform assets are ready for $TARGET_TRIPLE"
