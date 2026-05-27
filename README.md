# Alice Wallet

Native desktop and command-line wallet for Alice Protocol.

This feature branch is hardened for Phase40 internal review. It supports local
encrypted wallet creation/import, backup verification, unlock, balance refresh,
receive QR, safe transfer review, transaction history, account/address views,
sanitized node sync status, wallet XMR mining status/reward display, settings,
auto-lock, and English / Chinese UI.

Live transfers, old reward-role actions, staking, governance, DeFi, approval
grants, authorization grants, payout authority, settlement authority, mint
authority, wallet publication, notarization, and release upload are not enabled
in this branch.

The product direction follows the Monero GUI wallet information architecture for
wallet/node separation and honest sync state, while keeping Alice-specific UI
language and avoiding direct code or UI copying. See
[`docs/ALICE_WALLET_XMR_STYLE_UPGRADE_PLAN.md`](docs/ALICE_WALLET_XMR_STYLE_UPGRADE_PLAN.md)
and [`docs/XMR_GUI_REFERENCE_AUDIT.md`](docs/XMR_GUI_REFERENCE_AUDIT.md).

## Install

```bash
git clone https://github.com/V-SK/alice-wallet.git
cd alice-wallet
# Requirements: Python 3.8+
pip install -r requirements.txt
```

## Desktop GUI

```bash
cd gui
cargo run
```

Release packaging is intentionally outside this Phase40 branch scope.

For local QA smoke only, the GUI supports a display-only mock mode that avoids
loading local wallet files, saved settings, RPC refreshes, transfer execution,
and mining execution:

```bash
cd gui
TMPDIR=/tmp CARGO_TARGET_DIR="/Volumes/Z Slim/AliceWork/cargo-target" cargo build
./scripts/build_macos_icon.sh
./scripts/build_qa_app_bundle.sh "/Volumes/Z Slim/AliceWork/phase40r/AliceWalletQA.app"
ALICE_WALLET_QA_MOCK=1 "/Volumes/Z Slim/AliceWork/phase40r/AliceWalletQA.app/Contents/MacOS/AliceWalletQA"
```

The QA bundle includes `assets/macos/AliceWallet.icns` for the macOS app icon.
It is not a signed, notarized, or release-ready package.

## Phase40U Release Ops Readiness

Phase40U adds descriptor-only release-ops packets for owner review. The helper
builds an unsigned, unpublished release manifest candidate plus HF distribution,
Storage Box archive, website download metadata, and leak-audit handoff files.
It does not sign, notarize, upload, execute an updater, mutate HF or
`/mnt/storage`, edit the website repo, or enable public distribution.

```bash
python3 release_ops.py \
  --out-dir "/Volumes/Z Slim/AliceWork/phase40u" \
  --source-commit "$(git rev-parse HEAD)" \
  --app-version "0.1.0"
```

## L6 Public Release Signing Readiness

L6 adds metadata-only validators for public Mac/Windows release signing
readiness. It records Developer ID, hardened runtime, notarization, stapling,
Gatekeeper, Authenticode, timestamp, SmartScreen, and HF-only signed-manifest
requirements without using real credentials or executing release actions.

No public release is allowed until packages are signed, notarized where
applicable, hashes are frozen, and HF metadata is approved. See
[`docs/L6_PUBLIC_CLIENT_RELEASE_SIGNING_READINESS.md`](docs/L6_PUBLIC_CLIENT_RELEASE_SIGNING_READINESS.md).

## Quick Start

```bash
# Create wallet
python cli.py create

# Check balance
python cli.py balance YOUR_ADDRESS
```

## Commands

| Command | Description |
|---------|-------------|
| `create` | Create a local wallet |
| `balance <address>` | Check ALICE balance |

The CLI does not expose live transfer, old reward-role, staking, payout,
settlement, or mint commands.

The desktop Send view is a local review surface only. It checks address and
amount formatting and does not send funds from this branch.

## Wallet Mining

The desktop Mining view is scoped to Alice-approved XMR contribution status
only. It uses the selected wallet account/address as the Alice reward identity,
does not expose pool configuration, and does not start a miner from this branch.
Estimated rewards are display-only and daily confirmed rewards require
accepted-share evidence.

## Node Sync

The desktop wallet shows a productized sync state with current height, target
height, remaining blocks, progress, connection mode, peer/network status,
freshness, and fail-closed status. Missing height, missing target height, stale
data, and offline nodes do not display as ready or synced.
