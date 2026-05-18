#!/usr/bin/env python3
"""
Alice Wallet CLI.

Phase40 keeps this CLI read-only except for local wallet creation. Live
transfers and old reward-role actions are intentionally absent from the parser.
"""
import argparse
import os
import sys
from pathlib import Path


def cmd_create(args):
    """Create a new wallet interactively; backup material is shown by wallet.py."""
    from wallet import DEFAULT_WALLET_PATH, create_wallet_interactive

    wallet_path = Path(args.wallet_file) if args.wallet_file != "wallet.json" else DEFAULT_WALLET_PATH
    secrets = create_wallet_interactive(wallet_path=wallet_path)
    if wallet_path.exists():
        os.chmod(wallet_path, 0o600)
    print(f"Address: {secrets.address}")
    print("Wallet created.")


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


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="alice-wallet",
        description="Alice Protocol Wallet CLI",
    )
    parser.add_argument(
        "--rpc",
        default="wss://rpc.aliceprotocol.org",
        help="Node connection URL for read-only balance queries",
    )
    parser.add_argument(
        "--wallet-file",
        default="wallet.json",
        help="Wallet file name for local create flows",
    )

    sub = parser.add_subparsers(dest="command", help="Commands")
    sub.add_parser("create", help="Create a new wallet")

    p_bal = sub.add_parser("balance", help="Check wallet balance")
    p_bal.add_argument("address", help="Wallet address")

    return parser


def main():
    parser = build_parser()
    args = parser.parse_args()

    if args.command == "create":
        cmd_create(args)
    elif args.command == "balance":
        cmd_balance(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
