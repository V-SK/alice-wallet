# Alice Wallet

Command-line wallet for Alice Protocol.

## Install

```bash
git clone https://github.com/V-SK/alice-wallet.git
cd alice-wallet
pip install -r requirements.txt
```

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
