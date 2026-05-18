# Alice Wallet

Native desktop and command-line wallet for Alice Protocol.

The current desktop wallet lives in `gui/` and uses Rust `eframe` / `egui`.
It supports local encrypted wallet creation/import, backup verification, unlock,
balance refresh, receive QR, send review/signing, transaction history, settings,
auto-lock, and English / Chinese UI.

The next product lane is the XMR-style wallet upgrade: wallet/node separation,
honest sync state, complete backup/recovery, receive-request management, and
one-click mining with address-only payout and RED-first safety tests. See
[`docs/ALICE_WALLET_XMR_STYLE_UPGRADE_PLAN.md`](docs/ALICE_WALLET_XMR_STYLE_UPGRADE_PLAN.md)
and
[`docs/XMR_GUI_REFERENCE_AUDIT.md`](docs/XMR_GUI_REFERENCE_AUDIT.md).

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

Release builds are produced from `.github/workflows/release.yml` for Linux,
Windows, and macOS arm64.

## Quick Start

```bash
# Create wallet
python cli.py create
# ⚠️ Save your mnemonic securely — it cannot be recovered!

# Check balance
python cli.py balance YOUR_ADDRESS

# Transfer
python cli.py transfer --to RECIPIENT --amount 100
```

## Commands

| Command | Description |
|---------|-------------|
| `create` | Create new wallet |
| `balance <address>` | Check ALICE balance |
| `transfer --to --amount` | Send ALICE tokens |
| `stake scorer <amount> --endpoint <url>` | Stake as scorer |
| `stake aggregator <amount> --endpoint <url>` | Stake as aggregator |
| `unstake scorer` | Remove scorer stake |
| `unstake aggregator` | Remove aggregator stake |
| `status` | View staking status |

## Staking

```bash
# Stake as scorer (minimum 5,000 ALICE)
python cli.py stake scorer 5000 --endpoint http://YOUR_IP:8090

# Stake as aggregator (minimum 20,000 ALICE)
python cli.py stake aggregator 20000 --endpoint http://YOUR_IP:8084

# Check staking status
python cli.py status

# Unstake
python cli.py unstake scorer
```

## Role Requirements

| Role | Min Stake | Hardware |
|------|-----------|----------|
| Training Miner | None | 24GB VRAM GPU |
| Scorer | 5,000 ALICE | 24GB RAM |
| Aggregator | 20,000 ALICE | 64GB RAM + 1TB SSD |

## RPC

Default: `wss://rpc.aliceprotocol.org`

Custom: `python cli.py status --rpc wss://YOUR_NODE:9944`
