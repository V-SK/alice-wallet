# Alice Wallet

Native desktop and command-line wallet for Alice Protocol.

This feature branch is hardened for Phase40 internal review. It supports local
encrypted wallet creation/import, backup verification, unlock, balance refresh,
receive QR, safe transfer review, transaction history, account/address views,
sanitized node sync status, settings, auto-lock, and English / Chinese UI.

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

## Node Sync

The desktop wallet shows a productized sync state with current height, target
height, remaining blocks, progress, connection mode, peer/network status,
freshness, and fail-closed status. Missing height, missing target height, stale
data, and offline nodes do not display as ready or synced.
