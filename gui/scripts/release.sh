#!/usr/bin/env bash
#
# release.sh — cut an Alice Wallet release for the unsigned-distribution +
# ed25519 auto-update scheme (see docs/UPDATE-SCHEME.md).
#
# Pipeline:
#   1. Build the GUI for each requested target.
#   2. Package per-OS (macOS .app -> .zip via ditto; Linux dir -> .tar.gz;
#      Windows dir -> .zip), AD-HOC signing the macOS bundle inner-first
#      (scripts/adhoc_sign_macos.sh — NO --deep).
#   3. Write SHA256SUMS over the artifacts.
#   4. Generate latest.json (the signed update manifest) from the artifacts.
#   5. *** OFFLINE, ON A TRUSTED MACHINE ***: ed25519-sign SHA256SUMS and
#      latest.json with the release key. This script DOES NOT sign by default;
#      it prints the exact commands and only signs if --sign is passed AND the
#      offline key is present (so CI can never sign).
#   6. Upload artifacts + SHA256SUMS + latest.json + latest.json.sig to a
#      GitHub Release via `gh`.
#
# The ONLY trust anchor is the ed25519 release key. There are NO Apple/Windows
# code-signing certificates. The wallet verifies latest.json.sig against the
# embedded public key (gui/src/update.rs::RELEASE_PUBKEY_B64) before acting.
#
# Usage:
#   scripts/release.sh --version 1.4.0 [--targets "macos-arm64 linux-x86_64 windows-x86_64"]
#                      [--min-supported 1.0.0] [--notes-file NOTES.md]
#                      [--out dist] [--sign] [--publish] [--repo owner/name]
#
# Safe by default: with neither --sign nor --publish it only builds + packages +
# writes SHA256SUMS + latest.json locally, and prints the signing/publish steps.
#
set -euo pipefail

# ── Defaults ────────────────────────────────────────────────────────────────
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_TRIPLE="$(rustc -vV 2>/dev/null | awk -F': ' '/host/ {print $2}')"
VERSION=""
MIN_SUPPORTED=""
NOTES_FILE=""
OUT_DIR="${ROOT_DIR}/dist"
TARGETS=""          # platform keys; empty => just the host
DO_SIGN=0
DO_PUBLISH=0
REPO=""             # owner/name for gh; default: gh infers from git remote
PRODUCT="alice-wallet"
# Offline private key location (NEVER committed; NEVER read by CI).
RELEASE_KEY="${ALICE_RELEASE_KEY:-${HOME}/.alice-release/alice-update-ed25519.key}"
# Public URL prefix where these artifacts will be downloadable (pinned by V).
# Default mirrors gui/src/update.rs::DEFAULT_UPDATE_URL's directory.
BASE_URL="${ALICE_RELEASE_BASE_URL:-}"

# ── Embedded-node bundling ("bundle monerod") ───────────────────────────────
# The wallet manages a child `solochain-template-node`. The canonical raw chain
# spec is committed and shared across all OSes; only the per-OS node binary
# differs. Each per-OS wallet artifact ships the matching binary + the spec as
# siblings of the wallet exe (see gui/src/node.rs::resolve_node_binary /
# resolve_chain_spec). The spec is verified fail-closed against the SHA-256
# pinned in gui/src/node.rs before it is ever packaged.
#
# Committed spec (shared): gui/release-assets/alice-mainnet-raw.json
CHAIN_SPEC_SRC="${ALICE_CHAIN_SPEC_FILE:-${ROOT_DIR}/release-assets/alice-mainnet-raw.json}"
# Per-OS node binary: committed under gui/release-assets/<triple>/ OR pointed at
# by ALICE_NODE_BIN_<triple-with-underscores> / ALICE_NODE_BIN (a local path).
# When absent, the artifact still ships (Remote-node mode); a notice is printed.

# Read the pinned spec SHA-256 straight from node.rs so this never drifts.
pinned_spec_sha256() {
  grep -oE '"[0-9a-f]{64}"' "${ROOT_DIR}/src/node.rs" | head -1 | tr -d '"'
}
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}';
  else shasum -a 256 "$1" | awk '{print $1}'; fi
}

# Resolve the node binary for a given rust triple. Echoes a path or empty.
resolve_node_bin_for() {
  local triple="$1" want_exe="$2"   # want_exe=1 => windows .exe
  local committed_dir="${ROOT_DIR}/release-assets/${triple}"
  local fname="solochain-template-node"; [[ "${want_exe}" == "1" ]] && fname="solochain-template-node.exe"
  # 1) committed asset
  if [[ -f "${committed_dir}/${fname}" ]]; then echo "${committed_dir}/${fname}"; return; fi
  # 2) explicit per-triple env override (a local path)
  local var="ALICE_NODE_BIN_$(echo "${triple}" | tr '-' '_')"
  local val="${!var:-}"
  [[ -z "${val}" ]] && val="${ALICE_NODE_BIN:-}"
  if [[ -n "${val}" && -f "${val}" ]]; then echo "${val}"; return; fi
  echo ""
}

# ── Bundled CPU miner (XMRig) — same optional, sibling-of-the-wallet layout as
#    the node (gui/src/node.rs::resolve_miner_binary, XMRIG_BINARY_NAME="xmrig").
#    macOS arm64 ships its xmrig COMMITTED under release-assets/<triple>/xmrig.
#    TODO(windows/linux): supply per-OS xmrig (committed under the matching
#    release-assets/<triple>/, or via ALICE_XMRIG_BIN[_<triple>]); macOS is what
#    we test now. When absent the artifact still ships (the Mining page surfaces
#    a "miner not bundled" error on Start) — we never block the build on it.
resolve_xmrig_bin_for() {
  local triple="$1" want_exe="$2"   # want_exe=1 => windows .exe
  local committed_dir="${ROOT_DIR}/release-assets/${triple}"
  local fname="xmrig"; [[ "${want_exe}" == "1" ]] && fname="xmrig.exe"
  # 1) committed asset
  if [[ -f "${committed_dir}/${fname}" ]]; then echo "${committed_dir}/${fname}"; return; fi
  # 2) explicit per-triple env override (a local path)
  local var="ALICE_XMRIG_BIN_$(echo "${triple}" | tr '-' '_')"
  local val="${!var:-}"
  [[ -z "${val}" ]] && val="${ALICE_XMRIG_BIN:-}"
  if [[ -n "${val}" && -f "${val}" ]]; then echo "${val}"; return; fi
  echo ""
}

# Verify + copy the bundled spec into a staging dir (fail-closed on SHA pin).
stage_chain_spec() {
  local dest="$1"
  [[ -f "${CHAIN_SPEC_SRC}" ]] || { echo ""; return; }
  local pin act; pin="$(pinned_spec_sha256)"; act="$(sha256_of "${CHAIN_SPEC_SRC}")"
  if [[ -z "${pin}" ]]; then echo "  ! could not read pinned spec SHA from src/node.rs" >&2; exit 1; fi
  if [[ "${pin}" != "${act}" ]]; then
    echo "  !! chain spec SHA-256 mismatch: pinned=${pin} actual=${act} — refusing to bundle" >&2
    exit 1
  fi
  cp "${CHAIN_SPEC_SRC}" "${dest}/alice-mainnet-raw.json"
  echo "${dest}/alice-mainnet-raw.json"
}

# Map a platform key -> rust target triple + artifact filename.
target_triple() {
  case "$1" in
    macos-arm64)     echo "aarch64-apple-darwin" ;;
    macos-x86_64)    echo "x86_64-apple-darwin" ;;
    linux-x86_64)    echo "x86_64-unknown-linux-gnu" ;;
    windows-x86_64)  echo "x86_64-pc-windows-msvc" ;;
    *) echo "" ;;
  esac
}
artifact_name() {
  case "$1" in
    macos-arm64)     echo "AliceWallet-macos-arm64.zip" ;;
    macos-x86_64)    echo "AliceWallet-macos-x86_64.zip" ;;
    linux-x86_64)    echo "AliceWallet-linux-x86_64.tar.gz" ;;
    windows-x86_64)  echo "AliceWallet-windows-x86_64.zip" ;;
    *) echo "" ;;
  esac
}

# ── Args ────────────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)       VERSION="$2"; shift 2 ;;
    --min-supported) MIN_SUPPORTED="$2"; shift 2 ;;
    --notes-file)    NOTES_FILE="$2"; shift 2 ;;
    --targets)       TARGETS="$2"; shift 2 ;;
    --out)           OUT_DIR="$2"; shift 2 ;;
    --repo)          REPO="$2"; shift 2 ;;
    --base-url)      BASE_URL="$2"; shift 2 ;;
    --sign)          DO_SIGN=1; shift ;;
    --publish)       DO_PUBLISH=1; shift ;;
    -h|--help)       sed -n '2,40p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "${VERSION}" ]]; then
  # Default to the crate version so the manifest never drifts from the binary.
  VERSION="$(grep -m1 '^version' "${ROOT_DIR}/Cargo.toml" | sed -E 's/version *= *"([^"]+)".*/\1/')"
fi
[[ -z "${MIN_SUPPORTED}" ]] && MIN_SUPPORTED="${VERSION}"
# Default targets to the platform key matching the build host.
if [[ -z "${TARGETS}" ]]; then
  case "${HOST_TRIPLE}" in
    aarch64-apple-darwin)     TARGETS="macos-arm64" ;;
    x86_64-apple-darwin)      TARGETS="macos-x86_64" ;;
    x86_64-unknown-linux-gnu) TARGETS="linux-x86_64" ;;
    *) echo "could not infer target from host '${HOST_TRIPLE}'; pass --targets" >&2; exit 1 ;;
  esac
fi

RELEASED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
NOTES="$( [[ -n "${NOTES_FILE}" && -f "${NOTES_FILE}" ]] && cat "${NOTES_FILE}" || echo "Alice Wallet ${VERSION}." )"

echo "── Alice Wallet release ${VERSION} ─────────────────────────────────────"
echo "targets       : ${TARGETS}"
echo "min_supported : ${MIN_SUPPORTED}"
echo "out           : ${OUT_DIR}"
echo "base url      : ${BASE_URL:-<unset — set --base-url / ALICE_RELEASE_BASE_URL>}"
echo "sign          : ${DO_SIGN}    publish: ${DO_PUBLISH}"
echo

rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"

# ── 1+2. Build + package per target ─────────────────────────────────────────
for plat in ${TARGETS}; do
  triple="$(target_triple "${plat}")"
  artifact="$(artifact_name "${plat}")"
  [[ -z "${triple}" || -z "${artifact}" ]] && { echo "unknown platform: ${plat}" >&2; exit 1; }

  echo "Building ${plat} (${triple})…"
  ( cd "${ROOT_DIR}" && cargo build --release --target "${triple}" )

  stage="${OUT_DIR}/stage-${plat}"
  rm -rf "${stage}"; mkdir -p "${stage}"

  case "${plat}" in
    macos-*)
      app="${stage}/AliceWallet.app"
      mkdir -p "${app}/Contents/MacOS" "${app}/Contents/Resources"
      cp "${ROOT_DIR}/target/${triple}/release/gui" "${app}/Contents/MacOS/AliceWallet"
      chmod +x "${app}/Contents/MacOS/AliceWallet"
      # Embedded node: binary beside the wallet in MacOS/, spec in Resources/
      # (matches gui/src/node.rs resolution candidates). Spec is SHA-pinned.
      nb="$(resolve_node_bin_for "${triple}" 0)"
      if [[ -n "${nb}" ]]; then
        cp "${nb}" "${app}/Contents/MacOS/solochain-template-node"
        chmod +x "${app}/Contents/MacOS/solochain-template-node"
        echo "  + bundled node binary ($(sha256_of "${nb}" | cut -c1-12)…)"
      else
        echo "  ~ no node binary for ${triple} — wallet ships in Remote-node mode"
      fi
      [[ -n "$(stage_chain_spec "${app}/Contents/Resources")" ]] && echo "  + bundled chain spec (SHA-pinned)"
      # Bundled CPU miner (XMRig) beside the wallet in MacOS/ (gui/src/node.rs
      # resolve_miner_binary). Optional — committed for macOS arm64.
      xb="$(resolve_xmrig_bin_for "${triple}" 0)"
      if [[ -n "${xb}" ]]; then
        cp "${xb}" "${app}/Contents/MacOS/xmrig"
        chmod +x "${app}/Contents/MacOS/xmrig"
        echo "  + bundled xmrig ($(sha256_of "${xb}" | cut -c1-12)…)"
      else
        echo "  ~ no xmrig for ${triple} — wallet ships without the bundled miner"
      fi
      # App icon → Resources + CFBundleIconFile, so Launchpad / Finder / Dock show
      # the Alice mark instead of a generic icon (committed gui/assets/macos).
      icns="${ROOT_DIR}/assets/macos/AliceWallet.icns"
      if [[ -f "${icns}" ]]; then
        cp "${icns}" "${app}/Contents/Resources/AliceWallet.icns"
        echo "  + bundled app icon (AliceWallet.icns)"
      else
        echo "  ~ no app icon (${icns} missing) — a generic icon will show"
      fi
      cat > "${app}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Alice Wallet</string>
  <key>CFBundleDisplayName</key><string>Alice Wallet</string>
  <key>CFBundleIdentifier</key><string>org.aliceprotocol.wallet</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key><string>AliceWallet</string>
  <key>CFBundleIconFile</key><string>AliceWallet</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
      # Ad-hoc sign inner-first then the bundle (NO --deep).
      "${ROOT_DIR}/scripts/adhoc_sign_macos.sh" "${app}"
      # Zip the bundle preserving metadata (matches the in-app updater's ditto).
      ( cd "${stage}" && ditto -c -k --keepParent "AliceWallet.app" "${OUT_DIR}/${artifact}" )
      ;;
    linux-x86_64)
      d="${stage}/AliceWallet"; mkdir -p "${d}"
      cp "${ROOT_DIR}/target/${triple}/release/gui" "${d}/AliceWallet"
      chmod +x "${d}/AliceWallet"
      # Embedded node + spec as siblings of the wallet exe (gui/src/node.rs).
      nb="$(resolve_node_bin_for "${triple}" 0)"
      if [[ -n "${nb}" ]]; then
        cp "${nb}" "${d}/solochain-template-node"; chmod +x "${d}/solochain-template-node"
        echo "  + bundled node binary ($(sha256_of "${nb}" | cut -c1-12)…)"
      else
        echo "  ~ no node binary for ${triple} — wallet ships in Remote-node mode"
      fi
      [[ -n "$(stage_chain_spec "${d}")" ]] && echo "  + bundled chain spec (SHA-pinned)"
      # Bundled CPU miner (XMRig) as a sibling of the wallet exe. Optional.
      # TODO(linux): supply release-assets/${triple}/xmrig (or ALICE_XMRIG_BIN).
      xb="$(resolve_xmrig_bin_for "${triple}" 0)"
      if [[ -n "${xb}" ]]; then
        cp "${xb}" "${d}/xmrig"; chmod +x "${d}/xmrig"
        echo "  + bundled xmrig ($(sha256_of "${xb}" | cut -c1-12)…)"
      else
        echo "  ~ no xmrig for ${triple} — wallet ships without the bundled miner"
      fi
      ( cd "${stage}" && tar -czf "${OUT_DIR}/${artifact}" "AliceWallet" )
      ;;
    windows-x86_64)
      d="${stage}/AliceWallet"; mkdir -p "${d}"
      cp "${ROOT_DIR}/target/${triple}/release/gui.exe" "${d}/AliceWallet.exe"
      # Embedded node + spec as siblings of the wallet exe (gui/src/node.rs).
      nb="$(resolve_node_bin_for "${triple}" 1)"
      if [[ -n "${nb}" ]]; then
        cp "${nb}" "${d}/solochain-template-node.exe"
        echo "  + bundled node binary ($(sha256_of "${nb}" | cut -c1-12)…)"
      else
        echo "  ~ no node binary for ${triple} — wallet ships in Remote-node mode"
      fi
      [[ -n "$(stage_chain_spec "${d}")" ]] && echo "  + bundled chain spec (SHA-pinned)"
      # Bundled CPU miner (XMRig) as a sibling of the wallet exe. Optional.
      # TODO(windows): supply release-assets/${triple}/xmrig.exe (or ALICE_XMRIG_BIN).
      xb="$(resolve_xmrig_bin_for "${triple}" 1)"
      if [[ -n "${xb}" ]]; then
        cp "${xb}" "${d}/xmrig.exe"
        echo "  + bundled xmrig ($(sha256_of "${xb}" | cut -c1-12)…)"
      else
        echo "  ~ no xmrig for ${triple} — wallet ships without the bundled miner"
      fi
      # zip via `ditto` on macOS host, else `zip`.
      if command -v ditto >/dev/null 2>&1; then
        ( cd "${stage}" && ditto -c -k --keepParent "AliceWallet" "${OUT_DIR}/${artifact}" )
      else
        ( cd "${stage}" && zip -r "${OUT_DIR}/${artifact}" "AliceWallet" >/dev/null )
      fi
      ;;
  esac
  rm -rf "${stage}"
  echo "  -> ${OUT_DIR}/${artifact}"
done

# ── 3. SHA256SUMS ───────────────────────────────────────────────────────────
echo "Writing SHA256SUMS…"
(
  cd "${OUT_DIR}"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum ./*.zip ./*.tar.gz 2>/dev/null > SHA256SUMS || true
  else
    : > SHA256SUMS
    for f in ./*.zip ./*.tar.gz; do
      [[ -e "$f" ]] || continue
      printf '%s  %s\n' "$(shasum -a 256 "$f" | awk '{print $1}')" "${f#./}" >> SHA256SUMS
    done
  fi
  cat SHA256SUMS
)

# ── 4. latest.json (the signed update manifest) ─────────────────────────────
# The artifacts[] entry per platform: { platform, url, sha256, size }.
echo "Generating latest.json…"
artifacts_json=""
for plat in ${TARGETS}; do
  artifact="$(artifact_name "${plat}")"
  f="${OUT_DIR}/${artifact}"
  [[ -e "${f}" ]] || continue
  if command -v sha256sum >/dev/null 2>&1; then
    sum="$(sha256sum "${f}" | awk '{print $1}')"
  else
    sum="$(shasum -a 256 "${f}" | awk '{print $1}')"
  fi
  size="$(wc -c < "${f}" | tr -d ' ')"
  url="${BASE_URL:+${BASE_URL%/}/}${artifact}"
  entry="$(printf '{"platform":"%s","url":"%s","sha256":"%s","size":%s}' "${plat}" "${url}" "${sum}" "${size}")"
  artifacts_json="${artifacts_json:+${artifacts_json},}${entry}"
done

# NOTE: the manifest BYTES are what gets signed (raw ed25519, no pre-hash). Keep
# this serialization stable; the wallet re-serializes via serde for comparison
# only in tests, never for verification (it verifies the bytes as fetched).
NOTES_ESCAPED="$(printf '%s' "${NOTES}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')"
cat > "${OUT_DIR}/latest.json" <<JSON
{"schema":1,"product":"${PRODUCT}","version":"${VERSION}","min_supported":"${MIN_SUPPORTED}","released":"${RELEASED}","notes":${NOTES_ESCAPED},"artifacts":[${artifacts_json}]}
JSON
echo "  -> ${OUT_DIR}/latest.json"
cat "${OUT_DIR}/latest.json"; echo

# ── 5. OFFLINE ed25519 signing ──────────────────────────────────────────────
# We sign the RAW bytes of latest.json and of SHA256SUMS, producing detached
# base64 signatures. This MUST run on a trusted, offline machine that holds the
# release private key. The matching public key is embedded in the wallet.
#
# The exact, reproducible signing commands (do these by hand if not --sign):
print_sign_steps() {
  cat <<STEPS

  ── OFFLINE SIGNING (run on the trusted machine holding the release key) ──
  KEY=${RELEASE_KEY}

  # ed25519 raw signature over the manifest bytes -> base64 detached .sig
  openssl pkeyutl -sign -inkey "\$KEY" -rawin \\
      -in  "${OUT_DIR}/latest.json" \\
      -out "${OUT_DIR}/latest.json.sig.bin"
  base64 < "${OUT_DIR}/latest.json.sig.bin" | tr -d '\n' > "${OUT_DIR}/latest.json.sig"

  # (Optional but recommended) also sign SHA256SUMS the same way:
  openssl pkeyutl -sign -inkey "\$KEY" -rawin \\
      -in  "${OUT_DIR}/SHA256SUMS" \\
      -out "${OUT_DIR}/SHA256SUMS.sig.bin"
  base64 < "${OUT_DIR}/SHA256SUMS.sig.bin" | tr -d '\n' > "${OUT_DIR}/SHA256SUMS.sig"

  # Sanity check against the embedded public key (prints 'Signature Verified
  # Successfully'). The embedded pubkey is RELEASE_PUBKEY_B64; rebuild a PEM from
  # its raw 32 bytes with the fixed ed25519 SPKI prefix, then verify:
  #   PUB=8P+XmZZFEsUHLmqeB62Xqr5GnwW5K9vf2sQHvRzfi5k=    (RELEASE_PUBKEY_B64)
  #   { printf '\\x30\\x2a\\x30\\x05\\x06\\x03\\x2b\\x65\\x70\\x03\\x21\\x00'; \\
  #     printf '%s' "\$PUB" | base64 -d; } | openssl pkey -pubin -inform DER -out alice-update.pub.pem
  #   openssl pkeyutl -verify -pubin -inkey alice-update.pub.pem -rawin \\
  #       -in "${OUT_DIR}/latest.json" -sigfile "${OUT_DIR}/latest.json.sig.bin"
STEPS
}

if [[ "${DO_SIGN}" -eq 1 ]]; then
  if [[ -n "${CI:-}" ]]; then
    echo "REFUSING to sign in CI (the release key is offline-only)." >&2
    exit 1
  fi
  if [[ ! -f "${RELEASE_KEY}" ]]; then
    echo "REFUSING to sign: release key not found at ${RELEASE_KEY}." >&2
    print_sign_steps
    exit 1
  fi
  echo "Signing latest.json + SHA256SUMS with offline key ${RELEASE_KEY}…"
  openssl pkeyutl -sign -inkey "${RELEASE_KEY}" -rawin \
      -in "${OUT_DIR}/latest.json" -out "${OUT_DIR}/latest.json.sig.bin"
  base64 < "${OUT_DIR}/latest.json.sig.bin" | tr -d '\n' > "${OUT_DIR}/latest.json.sig"
  openssl pkeyutl -sign -inkey "${RELEASE_KEY}" -rawin \
      -in "${OUT_DIR}/SHA256SUMS" -out "${OUT_DIR}/SHA256SUMS.sig.bin"
  base64 < "${OUT_DIR}/SHA256SUMS.sig.bin" | tr -d '\n' > "${OUT_DIR}/SHA256SUMS.sig"
  rm -f "${OUT_DIR}/latest.json.sig.bin" "${OUT_DIR}/SHA256SUMS.sig.bin"
  echo "  -> ${OUT_DIR}/latest.json.sig"
  echo "  -> ${OUT_DIR}/SHA256SUMS.sig"
else
  echo "NOTE: not signing (no --sign). The offline signing steps:"
  print_sign_steps
fi

# ── 6. Publish to GitHub Releases ───────────────────────────────────────────
publish_files=( "${OUT_DIR}"/*.zip "${OUT_DIR}"/*.tar.gz "${OUT_DIR}/SHA256SUMS" "${OUT_DIR}/latest.json" )
[[ -f "${OUT_DIR}/latest.json.sig" ]] && publish_files+=( "${OUT_DIR}/latest.json.sig" )
[[ -f "${OUT_DIR}/SHA256SUMS.sig" ]] && publish_files+=( "${OUT_DIR}/SHA256SUMS.sig" )

if [[ "${DO_PUBLISH}" -eq 1 ]]; then
  if [[ ! -f "${OUT_DIR}/latest.json.sig" ]]; then
    echo "REFUSING to publish: latest.json.sig is missing (sign first)." >&2
    exit 1
  fi
  command -v gh >/dev/null 2>&1 || { echo "gh CLI not found" >&2; exit 1; }
  TAG="v${VERSION}"
  repo_args=(); [[ -n "${REPO}" ]] && repo_args=(--repo "${REPO}")
  echo "Creating GitHub release ${TAG}…"
  gh release create "${TAG}" "${repo_args[@]}" \
     --title "Alice Wallet ${VERSION}" \
     --notes "${NOTES}" \
     "${publish_files[@]}"
  echo "Published ${TAG} with $(printf '%s ' "${publish_files[@]##*/}")"
else
  echo
  echo "NOTE: not publishing (no --publish). To publish after signing:"
  echo "  gh release create v${VERSION} ${REPO:+--repo ${REPO} }\\"
  echo "     --title \"Alice Wallet ${VERSION}\" --notes \"…\" \\"
  echo "     ${publish_files[*]##*/}"
fi

echo
echo "Done. Artifacts in ${OUT_DIR}"
