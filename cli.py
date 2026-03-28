#!/usr/bin/env python3
"""
Alice Wallet CLI - alice-wallet
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
    # Secure the wallet file
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
            {
                "dest": args.to,
                "value": int(args.amount * 10**12),
            },
        )
        extrinsic = si.create_signed_extrinsic(call=call, keypair=keypair)
        receipt = si.submit_extrinsic(extrinsic, wait_for_inclusion=True)
        print(f"Transfer sent!")
        print(f"TX: {receipt.extrinsic_hash}")
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

    args = parser.parse_args()

    if args.command == "create":
        cmd_create(args)
    elif args.command == "balance":
        cmd_balance(args)
    elif args.command == "transfer":
        cmd_transfer(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
