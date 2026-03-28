#!/usr/bin/env python3
"""
Alice Wallet CLI - alice-wallet
Commands: create, balance, transfer, stake, unstake, status
"""
import argparse
import os
import sys
from pathlib import Path


def cmd_create(args):
    """Create a new wallet interactively (prompts for password internally)."""
    from wallet import create_wallet_interactive, DEFAULT_WALLET_PATH
    wallet_path = Path(args.wallet_file) if args.wallet_file != "wallet.json" else DEFAULT_WALLET_PATH
    secrets = create_wallet_interactive(wallet_path=wallet_path)
    if wallet_path.exists():
        os.chmod(wallet_path, 0o600)
    print(f"Address: {secrets.address}")
    if secrets.mnemonic:
        print(f"Mnemonic: {secrets.mnemonic}")
        print("IMPORTANT: Save your mnemonic phrase securely and never share it!")
    print(f"Wallet saved to: {wallet_path}")


def cmd_balance(args):
    """Check the balance of a wallet address."""
    try:
        from substrateinterface import SubstrateInterface
        si = SubstrateInterface(url=args.rpc, ss58_format=300)
        result = si.query("System", "Account", [args.address])
        balance = result["data"]["free"].value / 10**12
        print(f"Address: {args.address}")
        print(f"Balance: {balance:.4f} ALICE")
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)


def cmd_transfer(args):
    """Send ALICE tokens (prompts for password interactively)."""
    try:
        from wallet import unlock_wallet_interactive, DEFAULT_WALLET_PATH
        wallet_path = Path(args.wallet_file) if args.wallet_file != "wallet.json" else DEFAULT_WALLET_PATH
        secrets = unlock_wallet_interactive(wallet_path=wallet_path)
        keypair = secrets.to_keypair()

        from substrateinterface import SubstrateInterface
        si = SubstrateInterface(url=args.rpc, ss58_format=300)
        call = si.compose_call(
            "Balances",
            "transfer_allow_death",
            {"dest": args.to, "value": int(args.amount * 10**12)},
        )
        extrinsic = si.create_signed_extrinsic(call=call, keypair=keypair)
        receipt = si.submit_extrinsic(extrinsic, wait_for_inclusion=True)
        print("Transfer sent!")
        print(f"TX: {receipt.extrinsic_hash}")
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)


def cmd_stake(args):
    """Stake ALICE as scorer or aggregator."""
    try:
        from wallet import unlock_wallet_interactive, stake_as_scorer, stake_as_aggregator, DEFAULT_WALLET_PATH
        wallet_path = Path(args.wallet_file) if args.wallet_file != "wallet.json" else DEFAULT_WALLET_PATH
        secrets = unlock_wallet_interactive(wallet_path=wallet_path)
        if args.role == "scorer":
            tx = stake_as_scorer(secrets, args.amount, args.endpoint, rpc_url=args.rpc)
            print(f"✅ Staked {args.amount:,} ALICE as scorer")
        elif args.role == "aggregator":
            tx = stake_as_aggregator(secrets, args.amount, args.endpoint, rpc_url=args.rpc)
            print(f"✅ Staked {args.amount:,} ALICE as aggregator")
        print(f"TX: {tx}")
        print("Stake is Active immediately. You can start your service.")
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)


def cmd_unstake(args):
    """Begin cooldown to unstake from scorer or aggregator role."""
    try:
        from wallet import unlock_wallet_interactive, unstake_scorer, unstake_aggregator, DEFAULT_WALLET_PATH
        wallet_path = Path(args.wallet_file) if args.wallet_file != "wallet.json" else DEFAULT_WALLET_PATH
        secrets = unlock_wallet_interactive(wallet_path=wallet_path)
        if args.role == "scorer":
            tx = unstake_scorer(secrets, rpc_url=args.rpc)
            print("✅ Unstake cooldown started (scorer)")
        elif args.role == "aggregator":
            tx = unstake_aggregator(secrets, rpc_url=args.rpc)
            print("✅ Unstake cooldown started (aggregator)")
        print(f"TX: {tx}")
        print("Note: funds will be released after the cooldown period (~7 days).")
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)


def cmd_status(args):
    """Check on-chain staking status for an address."""
    try:
        from wallet import get_stake_status, unlock_wallet_interactive, DEFAULT_WALLET_PATH
        address = getattr(args, "address", None)
        if not address:
            wallet_path = Path(args.wallet_file) if args.wallet_file != "wallet.json" else DEFAULT_WALLET_PATH
            secrets = unlock_wallet_interactive(wallet_path=wallet_path)
            address = secrets.address
        status = get_stake_status(address, rpc_url=args.rpc)
        print(f"Address   : {address}")
        print(f"Balance   : {status['balance']:,} ALICE")
        if status["scorer"]:
            s = status["scorer"]
            print(f"Scorer    : {s['stake']:,} ALICE  [{s['status']}]")
            if s["endpoint"]:
                print(f"  Endpoint: {s['endpoint']}")
        else:
            print("Scorer    : not staked")
        if status["aggregator"]:
            a = status["aggregator"]
            print(f"Aggregator: {a['stake']:,} ALICE  [{a['status']}]")
            if a["endpoint"]:
                print(f"  Endpoint: {a['endpoint']}")
        else:
            print("Aggregator: not staked")
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(
        prog="alice-wallet",
        description="Alice Protocol Wallet CLI",
    )
    parser.add_argument(
        "--rpc",
        default="wss://rpc.aliceprotocol.org",
        help="RPC endpoint (default: wss://rpc.aliceprotocol.org)",
    )
    parser.add_argument(
        "--wallet-file",
        default="wallet.json",
        help="Wallet file path (default: ~/.alice/wallet.json)",
    )

    sub = parser.add_subparsers(dest="command", help="Commands")

    # create
    sub.add_parser("create", help="Create a new wallet (prompts for password)")

    # balance
    p_bal = sub.add_parser("balance", help="Check wallet balance")
    p_bal.add_argument("address", help="Wallet address (ss58)")

    # transfer
    p_tx = sub.add_parser("transfer", help="Send ALICE tokens")
    p_tx.add_argument("--to", required=True, help="Recipient address")
    p_tx.add_argument("--amount", type=float, required=True, help="Amount in ALICE")

    # stake
    p_stake = sub.add_parser("stake", help="Stake ALICE as scorer or aggregator")
    p_stake.add_argument("role", choices=["scorer", "aggregator"], help="Role to stake for")
    p_stake.add_argument("amount", type=int, help="Amount of ALICE to stake (whole units)")
    p_stake.add_argument("--endpoint", required=True, help="Your service URL (e.g. http://1.2.3.4:8090)")

    # unstake
    p_unstake = sub.add_parser("unstake", help="Begin cooldown to unstake from scorer or aggregator")
    p_unstake.add_argument("role", choices=["scorer", "aggregator"], help="Role to unstake from")

    # status
    p_status = sub.add_parser("status", help="Check on-chain staking status")
    p_status.add_argument("--address", default="", help="Address to check (default: wallet address)")

    args = parser.parse_args()

    if args.command == "create":
        cmd_create(args)
    elif args.command == "balance":
        cmd_balance(args)
    elif args.command == "transfer":
        cmd_transfer(args)
    elif args.command == "stake":
        cmd_stake(args)
    elif args.command == "unstake":
        cmd_unstake(args)
    elif args.command == "status":
        cmd_status(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
