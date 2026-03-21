#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_TRIPLE="${1:-}"

if [[ -z "$TARGET_TRIPLE" ]]; then
  echo "Usage: $0 <target-triple>" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
PYTHON_BIN="${PYTHON_BIN:-$(command -v python3 || command -v python || true)}"
DENO_VERSION="${DENO_VERSION:-$(curl -fsSL https://dl.deno.land/release-latest.txt)}"
PYTHON_STANDALONE_RELEASE="${PYTHON_STANDALONE_RELEASE:-20260320}"
PYTHON_STANDALONE_VERSION="${PYTHON_STANDALONE_VERSION:-3.12.13}"

if [[ -z "$PYTHON_BIN" ]]; then
  echo "Python is required to extract release archives." >&2
  exit 1
fi

download() {
  local url="$1"
  local output="$2"
  curl -fL --retry 3 --retry-delay 1 -o "$output" "$url"
}

extract_zip_entry() {
  local archive="$1"
  local matcher="$2"
  local output="$3"

  "$PYTHON_BIN" - "$archive" "$matcher" "$output" <<'PY'
import pathlib
import posixpath
import sys
import zipfile

archive_path = pathlib.Path(sys.argv[1])
matcher = sys.argv[2].lower()
output_path = pathlib.Path(sys.argv[3])

with zipfile.ZipFile(archive_path) as archive:
    for name in archive.namelist():
        normalized = name.lower().rstrip("/")
        basename = posixpath.basename(normalized)
        if normalized == matcher or basename == matcher:
            output_path.write_bytes(archive.read(name))
            break
    else:
        raise SystemExit(f"Could not find {matcher} in {archive_path}")
PY
}

ensure_parent() {
  mkdir -p "$(dirname "$1")"
}

have_file() {
  local path="$1"
  [[ -s "$path" ]]
}

prepare_macos_x64() {
  local yt_dlp_path="$ROOT_DIR/src-tauri/binaries/yt-dlp-x86_64-apple-darwin"
  local ffmpeg_path="$ROOT_DIR/src-tauri/binaries/ffmpeg-x86_64-apple-darwin"
  local ffprobe_path="$ROOT_DIR/src-tauri/binaries/ffprobe-x86_64-apple-darwin"
  local deno_path="$ROOT_DIR/src-tauri/binaries/deno-x86_64-apple-darwin"
  local python_dir="$ROOT_DIR/src-tauri/resources/python-runtime-x86_64-apple-darwin"

  ensure_parent "$yt_dlp_path"

  echo "Fetching yt-dlp script for Intel Mac..."
  download "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp" "$yt_dlp_path"
  chmod +x "$yt_dlp_path"

  if ! have_file "$ffmpeg_path"; then
    echo "Fetching ffmpeg for Intel Mac..."
    download "https://evermeet.cx/ffmpeg/getrelease/zip" "$TMP_DIR/ffmpeg-macos.zip"
    extract_zip_entry "$TMP_DIR/ffmpeg-macos.zip" "ffmpeg" "$ffmpeg_path"
    chmod +x "$ffmpeg_path"
  fi

  if ! have_file "$ffprobe_path"; then
    echo "Fetching ffprobe for Intel Mac..."
    download "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip" "$TMP_DIR/ffprobe-macos.zip"
    extract_zip_entry "$TMP_DIR/ffprobe-macos.zip" "ffprobe" "$ffprobe_path"
    chmod +x "$ffprobe_path"
  fi

  if ! have_file "$deno_path"; then
    echo "Fetching Deno for Intel Mac..."
    download "https://dl.deno.land/release/$DENO_VERSION/deno-x86_64-apple-darwin.zip" "$TMP_DIR/deno-macos-x64.zip"
    extract_zip_entry "$TMP_DIR/deno-macos-x64.zip" "deno" "$deno_path"
    chmod +x "$deno_path"
  fi

  if [[ ! -x "$python_dir/python/bin/python3" ]]; then
    echo "Fetching Python runtime for Intel Mac..."
    rm -rf "$python_dir"
    mkdir -p "$python_dir"
    download \
      "https://github.com/astral-sh/python-build-standalone/releases/download/$PYTHON_STANDALONE_RELEASE/cpython-$PYTHON_STANDALONE_VERSION%2B$PYTHON_STANDALONE_RELEASE-x86_64-apple-darwin-install_only_stripped.tar.gz" \
      "$TMP_DIR/python-macos-x64.tar.gz"
    tar -xzf "$TMP_DIR/python-macos-x64.tar.gz" -C "$python_dir"
  fi
}

prepare_macos_arm64() {
  local yt_dlp_path="$ROOT_DIR/src-tauri/binaries/yt-dlp-aarch64-apple-darwin"
  local python_dir="$ROOT_DIR/src-tauri/resources/python-runtime-aarch64-apple-darwin"

  ensure_parent "$yt_dlp_path"

  echo "Fetching yt-dlp script for Apple Silicon Mac..."
  download "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp" "$yt_dlp_path"
  chmod +x "$yt_dlp_path"

  if [[ ! -x "$python_dir/python/bin/python3" ]]; then
    echo "Fetching Python runtime for Apple Silicon Mac..."
    rm -rf "$python_dir"
    mkdir -p "$python_dir"
    download \
      "https://github.com/astral-sh/python-build-standalone/releases/download/$PYTHON_STANDALONE_RELEASE/cpython-$PYTHON_STANDALONE_VERSION%2B$PYTHON_STANDALONE_RELEASE-aarch64-apple-darwin-install_only_stripped.tar.gz" \
      "$TMP_DIR/python-macos-arm64.tar.gz"
    tar -xzf "$TMP_DIR/python-macos-arm64.tar.gz" -C "$python_dir"
  fi
}

prepare_windows_x64() {
  local yt_dlp_path="$ROOT_DIR/src-tauri/binaries/yt-dlp-x86_64-pc-windows-msvc.exe"
  local ffmpeg_path="$ROOT_DIR/src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe"
  local ffprobe_path="$ROOT_DIR/src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe"
  local deno_path="$ROOT_DIR/src-tauri/binaries/deno-x86_64-pc-windows-msvc.exe"

  ensure_parent "$yt_dlp_path"

  if ! have_file "$yt_dlp_path"; then
    echo "Fetching yt-dlp for Windows..."
    download "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe" "$yt_dlp_path"
  fi

  if ! have_file "$ffmpeg_path" || ! have_file "$ffprobe_path"; then
    echo "Fetching ffmpeg and ffprobe for Windows..."
    download "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip" "$TMP_DIR/ffmpeg-win64.zip"
    extract_zip_entry "$TMP_DIR/ffmpeg-win64.zip" "ffmpeg.exe" "$ffmpeg_path"
    extract_zip_entry "$TMP_DIR/ffmpeg-win64.zip" "ffprobe.exe" "$ffprobe_path"
  fi

  if ! have_file "$deno_path"; then
    echo "Fetching Deno for Windows..."
    download "https://dl.deno.land/release/$DENO_VERSION/deno-x86_64-pc-windows-msvc.zip" "$TMP_DIR/deno-win64.zip"
    extract_zip_entry "$TMP_DIR/deno-win64.zip" "deno.exe" "$deno_path"
  fi
}

case "$TARGET_TRIPLE" in
  aarch64-apple-darwin)
    prepare_macos_arm64
    ;;
  x86_64-apple-darwin)
    prepare_macos_x64
    ;;
  x86_64-pc-windows-msvc)
    prepare_windows_x64
    ;;
  *)
    echo "No automatic asset fetch configured for $TARGET_TRIPLE" >&2
    exit 1
    ;;
esac

echo "Finished preparing assets for $TARGET_TRIPLE"
