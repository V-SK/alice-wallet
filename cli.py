#!/usr/bin/env python3
"""
Alice Wallet CLI
Usage: alice-wallet <command> [options]
"""
import argparse
import sys
from wallet import SecureWallet

def cmd_create(args):
    w = SecureWallet()
    mnemonic = w.generate()
    print(f'Address: {w.address}')
    print(f'Mnemonic: {mnemonic}')
    print('IMPORTANT: Save your mnemonic phrase securely!')

def cmd_balance(args):
    from substrateinterface import SubstrateInterface
    si = SubstrateInterface(url=args.rpc, ss58_format=300)
    result = si.query('System', 'Account', [args.address])
    balance = result['data']['free'].value / 10**12
    print(f'Balance: {balance:.4f} ALICE')

def cmd_transfer(args):
    w = SecureWallet()
    w.load(args.wallet_file, args.password)
    from substrateinterface import SubstrateInterface, Keypair
    si = SubstrateInterface(url=args.rpc, ss58_format=300)
    call = si.compose_call('Balances', 'transfer_allow_death', {
        'dest': args.to,
        'value': int(args.amount * 10**12)
    })
    extrinsic = si.create_signed_extrinsic(call=call, keypair=w.keypair)
    receipt = si.submit_extrinsic(extrinsic, wait_for_inclusion=True)
    print(f'Transfer sent! TX: {receipt.extrinsic_hash}')

def main():
    parser = argparse.ArgumentParser(prog='alice-wallet')
    parser.add_argument('--rpc', default='wss://rpc.aliceprotocol.org', help='RPC endpoint')
    sub = parser.add_subparsers(dest='command')

    sub.add_parser('create', help='Create new wallet')

    p_bal = sub.add_parser('balance', help='Check balance')
    p_bal.add_argument('address', help='Wallet address')

    p_tx = sub.add_parser('transfer', help='Send ALICE')
    p_tx.add_argument('--to', required=True, help='Recipient address')
    p_tx.add_argument('--amount', type=float, required=True, help='Amount in ALICE')
    p_tx.add_argument('--wallet-file', default='wallet.json', help='Wallet file')
    p_tx.add_argument('--password', help='Wallet password')

    args = parser.parse_args()
    if args.command == 'create': cmd_create(args)
    elif args.command == 'balance': cmd_balance(args)
    elif args.command == 'transfer': cmd_transfer(args)
    else: parser.print_help()

if __name__ == '__main__':
    main()
