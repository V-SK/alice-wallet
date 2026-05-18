#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SVG_PATH="${ROOT_DIR}/alice-logo-traced.svg"
OUT_DIR="${ROOT_DIR}/assets/macos"
ICONSET_DIR="${OUT_DIR}/AliceWallet.iconset"
BASE_PNG="${OUT_DIR}/AliceWallet-1024.png"

mkdir -p "${OUT_DIR}"
rm -rf "${ICONSET_DIR}"
mkdir -p "${ICONSET_DIR}"

sips -s format png "${SVG_PATH}" --out "${BASE_PNG}" >/dev/null

make_icon() {
  local px="$1"
  local name="$2"
  sips -z "${px}" "${px}" "${BASE_PNG}" --out "${ICONSET_DIR}/${name}" >/dev/null
}

make_icon 16 icon_16x16.png
make_icon 32 icon_16x16@2x.png
make_icon 32 icon_32x32.png
make_icon 64 icon_32x32@2x.png
make_icon 128 icon_128x128.png
make_icon 256 icon_128x128@2x.png
make_icon 256 icon_256x256.png
make_icon 512 icon_256x256@2x.png
make_icon 512 icon_512x512.png
make_icon 1024 icon_512x512@2x.png

iconutil -c icns "${ICONSET_DIR}" -o "${OUT_DIR}/AliceWallet.icns"
rm -rf "${ICONSET_DIR}" "${BASE_PNG}"

echo "${OUT_DIR}/AliceWallet.icns"
