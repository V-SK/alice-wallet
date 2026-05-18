#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(cd "${ROOT_DIR}/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target}"
OUT_APP="${1:-/Volumes/Z Slim/AliceWork/phase40r/AliceWalletQA.app}"
BIN_PATH="${TARGET_DIR}/debug/gui"
ICON_PATH="${ROOT_DIR}/assets/macos/AliceWallet.icns"

if [[ ! -x "${BIN_PATH}" ]]; then
  echo "Missing GUI binary at ${BIN_PATH}; run cargo build first." >&2
  exit 1
fi

if [[ ! -f "${ICON_PATH}" ]]; then
  "${ROOT_DIR}/scripts/build_macos_icon.sh" >/dev/null
fi

rm -rf "${OUT_APP}"
mkdir -p "${OUT_APP}/Contents/MacOS" "${OUT_APP}/Contents/Resources"
cp "${BIN_PATH}" "${OUT_APP}/Contents/MacOS/AliceWalletQA"
cp "${ICON_PATH}" "${OUT_APP}/Contents/Resources/AliceWallet.icns"

cat >"${OUT_APP}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>Alice Wallet QA</string>
  <key>CFBundleExecutable</key>
  <string>AliceWalletQA</string>
  <key>CFBundleIconFile</key>
  <string>AliceWallet</string>
  <key>CFBundleIdentifier</key>
  <string>org.aliceprotocol.wallet.qa</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Alice Wallet QA</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

plutil -lint "${OUT_APP}/Contents/Info.plist" >/dev/null
echo "${OUT_APP}"
