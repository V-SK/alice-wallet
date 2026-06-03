#!/usr/bin/env bash
#
# adhoc_sign_macos.sh — ad-hoc codesign an Alice Wallet macOS bundle (or a bare
# Mach-O binary) INNER-FIRST, then the bundle, WITHOUT `--deep`.
#
# Why inner-first and not `--deep`:
#   `codesign --deep` is deprecated by Apple and signs nested code in an order
#   that does not reliably seal helper Mach-O binaries (e.g. the embedded node)
#   before the outer bundle. We instead find every Mach-O under the bundle, sign
#   the deepest ones first, and seal the bundle last so its signature covers
#   already-signed insides. This matches `gui/src/update.rs::adhoc_codesign`, so
#   an in-app self-update produces a byte-for-byte equivalent signing result.
#
# Trust model: this is an AD-HOC signature (`-s -`), NOT a Developer ID identity.
# It carries no Apple certificate; it exists only so the binary is runnable on
# Apple Silicon (which refuses unsigned arm64 Mach-O) after the user clears
# quarantine. The real integrity anchor is the ed25519-signed release manifest
# (see docs/UPDATE-SCHEME.md), verified before any artifact is signed or run.
#
# Usage:
#   scripts/adhoc_sign_macos.sh path/to/AliceWallet.app
#   scripts/adhoc_sign_macos.sh path/to/bare-binary
#
set -euo pipefail

TARGET="${1:-}"
if [[ -z "${TARGET}" ]]; then
  echo "usage: $0 <AliceWallet.app | binary>" >&2
  exit 2
fi
if [[ ! -e "${TARGET}" ]]; then
  echo "error: no such path: ${TARGET}" >&2
  exit 1
fi
if ! command -v codesign >/dev/null 2>&1; then
  echo "error: codesign not found (run on macOS)" >&2
  exit 1
fi

# Ad-hoc identity; no timestamp (offline / no network dependency).
SIGN_ARGS=(--force --timestamp=none -s -)

sign_one() {
  local path="$1"
  echo "  codesign ${path}"
  codesign "${SIGN_ARGS[@]}" "${path}"
}

if [[ -d "${TARGET}" ]]; then
  # Bundle: sign every nested Mach-O deepest-first, then the bundle itself.
  # Collect Mach-O paths with their depth (slash count), sort by depth desc so
  # children sign before parents, then strip the depth prefix. `file` identifies
  # Mach-O (thin or universal). Newline-delimited is fine: bundle paths produced
  # by our release pipeline contain no newlines.
  echo "Signing nested Mach-O (inner-first) under ${TARGET}…"
  machos=""
  while IFS= read -r f; do
    if file -b "${f}" | grep -q 'Mach-O'; then
      depth=$(printf '%s' "${f}" | tr -cd '/' | wc -c | tr -d ' ')
      machos+="${depth} ${f}"$'\n'
    fi
  done < <(find "${TARGET}" -type f)

  if [[ -n "${machos}" ]]; then
    # Sort by leading depth number (descending), then drop it and sign.
    printf '%s' "${machos}" | sort -rn | while IFS=' ' read -r _depth path; do
      [[ -n "${path}" ]] && sign_one "${path}"
    done
  fi

  echo "Sealing bundle ${TARGET}…"
  sign_one "${TARGET}"
else
  # Bare binary.
  sign_one "${TARGET}"
fi

echo "Verifying signature…"
codesign --verify --verbose=2 "${TARGET}"
echo "OK: ad-hoc signed ${TARGET}"
