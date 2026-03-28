# alice-wallet

Wallet CLI and library for Alice Protocol.

## Installation
```bash
pip install -r requirements.txt
```

## Usage

### Create wallet
```bash
python cli.py create
```

### Check balance
```bash
python cli.py balance <ADDRESS>
```

### Transfer ALICE
```bash
python cli.py transfer --to <ADDRESS> --amount 100
```

## Roles & Staking

| Role | Stake Required |
|------|---------------|
| Holder | None |
| Training Miner | None |
| Scorer | 5,000 ALICE |
| Aggregator | 20,000 ALICE |

## Chain Info
- RPC: wss://rpc.aliceprotocol.org
- SS58 Format: 300
