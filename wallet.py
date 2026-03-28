#!/usr/bin/env python3
"""Secure wallet utilities shared by miner and wallet CLI."""
from __future__ import annotations

import base64
import gc
import getpass
import json
import os
import secrets
import time
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Optional, Tuple

from cryptography.fernet import Fernet, InvalidToken
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
from mnemonic import Mnemonic
from substrateinterface import Keypair, KeypairType

DEFAULT_WALLET_PATH = Path.home() / ".alice" / "wallet.json"
SS58_FORMAT = 300
WALLET_VERSION_V2 = 2
PBKDF2_ITERATIONS = 600_000
SALT_BYTES = 32
NONCE_BYTES = 12
VALID_MNEMONIC_WORD_COUNTS = {12, 15, 18, 21, 24}


@dataclass
class WalletSecrets:
    address: str
    public_key_hex: str
    seed_bytes: bytes
    mnemonic: Optional[str]
    version: int
    has_mnemonic_backup: bool

    def to_keypair(self) -> Keypair:
        return Keypair.create_from_seed(
            self.seed_bytes,
            ss58_format=SS58_FORMAT,
            crypto_type=KeypairType.SR25519,
        )


def _derive_key(password: str, salt: bytes, iterations: int = PBKDF2_ITERATIONS) -> bytes:
    kdf = PBKDF2HMAC(
        algorithm=hashes.SHA256(),
        length=32,
        salt=salt,
        iterations=iterations,
    )
    return kdf.derive(password.encode("utf-8"))


def _encrypt_blob(plaintext: bytes, key: bytes) -> Tuple[str, str]:
    nonce = os.urandom(NONCE_BYTES)
    ciphertext = AESGCM(key).encrypt(nonce, plaintext, None)
    return (
        base64.b64encode(ciphertext).decode("utf-8"),
        base64.b64encode(nonce).decode("utf-8"),
    )


def _decrypt_blob(ciphertext_b64: str, nonce_b64: str, key: bytes) -> bytes:
    ciphertext = base64.b64decode(ciphertext_b64)
    nonce = base64.b64decode(nonce_b64)
    return AESGCM(key).decrypt(nonce, ciphertext, None)


def _decrypt_legacy_fernet(payload: Dict[str, Any], password: str) -> str:
    if payload.get("cipher") != "fernet":
        raise ValueError("Unsupported legacy wallet cipher")
    salt = base64.b64decode(payload["salt"])
    iterations = int(payload.get("iterations", 200_000))
    kdf = PBKDF2HMAC(
        algorithm=hashes.SHA256(),
        length=32,
        salt=salt,
        iterations=iterations,
    )
    key = base64.urlsafe_b64encode(kdf.derive(password.encode("utf-8")))
    token = payload["ciphertext"].encode("utf-8")
    return Fernet(key).decrypt(token).decode("utf-8")


def _ensure_wallet_dir(wallet_path: Path) -> None:
    wallet_path.parent.mkdir(parents=True, exist_ok=True)


def _write_wallet(wallet_path: Path, payload: Dict[str, Any]) -> None:
    _ensure_wallet_dir(wallet_path)
    wallet_path.write_text(json.dumps(payload, indent=2))
    try:
        os.chmod(wallet_path, 0o600)
    except Exception:
        pass


def _load_wallet_json(wallet_path: Path) -> Dict[str, Any]:
    if not wallet_path.exists():
        raise FileNotFoundError(f"❌ No wallet found at {wallet_path}")
    return json.loads(wallet_path.read_text())


def _normalize_seed(seed_value: Any) -> bytes:
    if isinstance(seed_value, bytes):
        return seed_value
    if isinstance(seed_value, bytearray):
        return bytes(seed_value)
    if isinstance(seed_value, str):
        seed_text = seed_value[2:] if seed_value.startswith("0x") else seed_value
        return bytes.fromhex(seed_text)
    raise ValueError("Unsupported seed value")


def _new_password_interactive() -> str:
    while True:
        password = getpass.getpass("Password: ")
        if len(password) < 8:
            print("Password must be at least 8 characters")
            continue
        confirm = getpass.getpass("Confirm password: ")
        if password != confirm:
            print("Passwords do not match")
            continue
        return password


def _unlock_password_interactive(prompt: str = "Wallet password: ") -> str:
    # 支持环境变量传递密码（用于非交互式启动）
    env_password = os.environ.get("ALICE_WALLET_PASSWORD")
    if env_password:
        print("[Wallet] Using password from ALICE_WALLET_PASSWORD environment variable")
        return env_password
    return getpass.getpass(prompt)


def generate_bip39_mnemonic_24() -> str:
    entropy = secrets.token_bytes(32)  # 256-bit entropy
    mnemo = Mnemonic("english")
    mnemonic = mnemo.to_mnemonic(entropy)
    words = mnemonic.strip().split()
    if len(words) != 24 or not mnemo.check(mnemonic):
        raise RuntimeError("Failed to generate a valid 24-word mnemonic")
    return mnemonic


def _print_mnemonic_grid(words: list[str], columns: int = 4) -> None:
    total = len(words)
    for i in range(0, total, columns):
        row = []
        for j in range(columns):
            idx0 = i + j
            if idx0 >= total:
                break
            row.append(f"{idx0 + 1:>2}. {words[idx0]}")
        print("   ".join(row))


def create_wallet_payload_v2(mnemonic: str, password: str) -> Dict[str, Any]:
    keypair = Keypair.create_from_mnemonic(
        mnemonic,
        ss58_format=SS58_FORMAT,
        crypto_type=KeypairType.SR25519,
    )
    seed_bytes = _normalize_seed(keypair.seed_hex)
    salt = os.urandom(SALT_BYTES)
    key = _derive_key(password, salt, iterations=PBKDF2_ITERATIONS)

    encrypted_seed, nonce_seed = _encrypt_blob(seed_bytes, key)
    encrypted_mnemonic, nonce_mnemonic = _encrypt_blob(mnemonic.encode("utf-8"), key)

    payload = {
        "version": WALLET_VERSION_V2,
        "address": keypair.ss58_address,
        "public_key": "0x" + keypair.public_key.hex(),
        "encrypted_seed": encrypted_seed,
        "encrypted_mnemonic": encrypted_mnemonic,
        "salt": base64.b64encode(salt).decode("utf-8"),
        "nonce_seed": nonce_seed,
        "nonce_mnemonic": nonce_mnemonic,
        "kdf": "pbkdf2-sha256",
        "kdf_iterations": PBKDF2_ITERATIONS,
    }

    del keypair, seed_bytes, salt, key, encrypted_seed, encrypted_mnemonic
    gc.collect()
    return payload


def create_wallet_interactive(wallet_path: Path = DEFAULT_WALLET_PATH) -> WalletSecrets:
    if wallet_path.exists():
        raise RuntimeError(f"❌ Wallet already exists at {wallet_path}")

    print("No wallet found at ~/.alice/wallet.json")
    print("")
    print("🔐 Creating new Alice wallet...")
    print("")
    print("Set a password to encrypt your wallet.")
    print("This password is needed every time you start mining or use the wallet.")
    password = _new_password_interactive()
    print("")

    mnemonic = generate_bip39_mnemonic_24()
    words = mnemonic.split()
    print("⚠️ IMPORTANT - BACKUP YOUR MNEMONIC ⚠️")
    print("════════════════════════════════════════")
    print("Write down these 24 words IN ORDER on paper.")
    print("This is the ONLY way to recover your wallet.")
    print("NEVER save digitally. NEVER screenshot. NEVER share.")
    print("")
    _print_mnemonic_grid(words, columns=4)
    print("")
    print("════════════════════════════════════════")
    print("")

    while True:
        confirm = input('Type "I have saved my mnemonic" to continue: ').strip()
        if confirm == "I have saved my mnemonic":
            break
        print('Please type exactly: I have saved my mnemonic')

    payload = create_wallet_payload_v2(mnemonic=mnemonic, password=password)
    _write_wallet(wallet_path, payload)

    keypair = Keypair.create_from_mnemonic(
        mnemonic,
        ss58_format=SS58_FORMAT,
        crypto_type=KeypairType.SR25519,
    )
    wallet = WalletSecrets(
        address=keypair.ss58_address,
        public_key_hex="0x" + keypair.public_key.hex(),
        seed_bytes=_normalize_seed(keypair.seed_hex),
        mnemonic=mnemonic,
        version=WALLET_VERSION_V2,
        has_mnemonic_backup=True,
    )

    print("")
    print("✅ Wallet created!")
    print(f"🔑 Address: {wallet.address}")
    print("\n" * 50)

    del password, mnemonic, words, payload, keypair
    gc.collect()
    return wallet


def _unlock_v2(raw: Dict[str, Any], password: str) -> WalletSecrets:
    salt = base64.b64decode(raw["salt"])
    kdf_iterations = int(raw.get("kdf_iterations", PBKDF2_ITERATIONS))
    key = _derive_key(password, salt, iterations=kdf_iterations)

    seed_bytes = _decrypt_blob(raw["encrypted_seed"], raw["nonce_seed"], key)
    mnemonic: Optional[str] = None
    has_backup = "encrypted_mnemonic" in raw and "nonce_mnemonic" in raw
    if has_backup:
        try:
            mnemonic = _decrypt_blob(raw["encrypted_mnemonic"], raw["nonce_mnemonic"], key).decode("utf-8")
        except Exception:
            mnemonic = None
            has_backup = False

    keypair = Keypair.create_from_seed(
        seed_bytes,
        ss58_format=SS58_FORMAT,
        crypto_type=KeypairType.SR25519,
    )
    address = raw.get("address")
    if keypair.ss58_address != address:
        raise RuntimeError("❌ Wallet address mismatch")

    wallet = WalletSecrets(
        address=address,
        public_key_hex=raw.get("public_key", "0x" + keypair.public_key.hex()),
        seed_bytes=seed_bytes,
        mnemonic=mnemonic,
        version=WALLET_VERSION_V2,
        has_mnemonic_backup=has_backup,
    )
    del salt, key, keypair
    gc.collect()
    return wallet


def _unlock_legacy(raw: Dict[str, Any], password: str) -> WalletSecrets:
    address = raw.get("address")
    if not address:
        raise RuntimeError("❌ Wallet file is missing address")

    if "crypto" in raw:
        secret = _decrypt_legacy_fernet(raw["crypto"], password)
    elif "encrypted_seed" in raw:
        # Some legacy payloads use fernet fields at top-level.
        payload = {
            "cipher": "fernet",
            "ciphertext": raw["encrypted_seed"],
            "salt": raw.get("salt"),
            "iterations": str(raw.get("iterations", 200_000)),
        }
        secret = _decrypt_legacy_fernet(payload, password)
    else:
        raise RuntimeError("❌ Wallet file has no encrypted secret")

    try:
        keypair = Keypair.create_from_mnemonic(secret, ss58_format=SS58_FORMAT, crypto_type=KeypairType.SR25519)
        mnemonic = secret
    except Exception:
        keypair = Keypair.create_from_uri(secret, ss58_format=SS58_FORMAT, crypto_type=KeypairType.SR25519)
        mnemonic = None

    if keypair.ss58_address != address:
        raise RuntimeError("❌ Wallet address mismatch")

    wallet = WalletSecrets(
        address=address,
        public_key_hex=raw.get("public_key", "0x" + keypair.public_key.hex()),
        seed_bytes=_normalize_seed(keypair.seed_hex),
        mnemonic=mnemonic,
        version=int(raw.get("version", 1)),
        has_mnemonic_backup=False,
    )
    del keypair, secret
    gc.collect()
    return wallet


def unlock_wallet_interactive(
    wallet_path: Path = DEFAULT_WALLET_PATH,
    max_attempts: int = 3,
) -> WalletSecrets:
    raw = _load_wallet_json(wallet_path)
    version = int(raw.get("version", 1))

    for attempt in range(1, max_attempts + 1):
        try:
            prompt = "🔐 Wallet found. Enter password to unlock: "
            password = _unlock_password_interactive(prompt=prompt)
            if version >= WALLET_VERSION_V2 and "encrypted_seed" in raw and "nonce_seed" in raw:
                wallet = _unlock_v2(raw, password)
            else:
                wallet = _unlock_legacy(raw, password)
                print(
                    "⚠️ Old wallet format detected. No mnemonic backup available.\n"
                    "   Consider creating a new wallet and transferring funds."
                )
            print(f"✅ Wallet unlocked: {wallet.address}")
            del password
            gc.collect()
            return wallet
        except (InvalidToken, RuntimeError, ValueError):
            if attempt < max_attempts:
                print("❌ Invalid wallet password")
            else:
                print("❌ Wrong password 3 times. Exiting.")
                raise RuntimeError("❌ Wrong password 3 times. Exiting.")
    raise RuntimeError("❌ Wrong password 3 times. Exiting.")


def get_or_create_wallet_for_miner(wallet_path: Path = DEFAULT_WALLET_PATH) -> WalletSecrets:
    if wallet_path.exists():
        return unlock_wallet_interactive(wallet_path=wallet_path, max_attempts=3)
    wallet = create_wallet_interactive(wallet_path=wallet_path)
    # Do not keep mnemonic around longer than necessary.
    wallet.mnemonic = None
    gc.collect()
    return wallet


def load_wallet_public(wallet_path: Path = DEFAULT_WALLET_PATH) -> Dict[str, Any]:
    raw = _load_wallet_json(wallet_path)
    address = raw.get("address")
    public_key = raw.get("public_key")
    if not address:
        raise RuntimeError("❌ Wallet file is missing address")
    return {
        "address": address,
        "public_key": public_key,
        "version": int(raw.get("version", 1)),
        "has_mnemonic_backup": bool(raw.get("encrypted_mnemonic")),
    }


def import_wallet_interactive(wallet_path: Path = DEFAULT_WALLET_PATH) -> WalletSecrets:
    print("🔐 Import wallet from mnemonic")
    print("Enter your mnemonic words separated by spaces:")
    mnemonic = input("> ").strip()
    words = mnemonic.split()
    mnemo = Mnemonic("english")
    if len(words) not in VALID_MNEMONIC_WORD_COUNTS or not mnemo.check(mnemonic):
        raise RuntimeError("❌ Invalid mnemonic. Please enter valid BIP39 words (12/15/18/21/24).")

    password = _new_password_interactive()
    keypair = Keypair.create_from_mnemonic(
        mnemonic,
        ss58_format=SS58_FORMAT,
        crypto_type=KeypairType.SR25519,
    )

    print("")
    print("✅ Wallet imported!")
    print(f"🔑 Address: {keypair.ss58_address}")
    confirm = input("Is this the correct address? [y/N]: ").strip().lower()
    if confirm != "y":
        del mnemonic, words, password, keypair
        gc.collect()
        raise RuntimeError("Import cancelled")

    payload = create_wallet_payload_v2(mnemonic=mnemonic, password=password)
    _write_wallet(wallet_path, payload)
    print(f"✅ Wallet saved to {wallet_path}")

    wallet = WalletSecrets(
        address=keypair.ss58_address,
        public_key_hex="0x" + keypair.public_key.hex(),
        seed_bytes=_normalize_seed(keypair.seed_hex),
        mnemonic=mnemonic,
        version=WALLET_VERSION_V2,
        has_mnemonic_backup=True,
    )
    del mnemonic, words, password, keypair, payload
    gc.collect()
    return wallet


def export_mnemonic_interactive(wallet_path: Path = DEFAULT_WALLET_PATH) -> str:
    raw = _load_wallet_json(wallet_path)
    version = int(raw.get("version", 1))
    if version < WALLET_VERSION_V2 or "encrypted_mnemonic" not in raw:
        raise RuntimeError(
            "⚠️ Old wallet format detected. No mnemonic backup available.\n"
            "   Consider creating a new wallet and transferring funds."
        )

    print("⚠️ WARNING: This will display your mnemonic phrase.")
    print("Anyone with these words can steal your funds.")
    password = _unlock_password_interactive("Enter password: ")
    confirm = input('Type "EXPORT MY MNEMONIC" to continue: ').strip()
    if confirm != "EXPORT MY MNEMONIC":
        del password
        gc.collect()
        raise RuntimeError("Export cancelled")

    wallet = _unlock_v2(raw, password)
    mnemonic = wallet.mnemonic
    if not mnemonic:
        del password, wallet
        gc.collect()
        raise RuntimeError("❌ Wallet does not contain an encrypted mnemonic backup")

    words = mnemonic.split()
    print("")
    print("⚠️ IMPORTANT - BACKUP YOUR MNEMONIC ⚠️")
    print("════════════════════════════════════════")
    print(f"Words: {len(words)}")
    _print_mnemonic_grid(words, columns=4)
    print("════════════════════════════════════════")
    print("Screen will clear in 60 seconds...")
    time.sleep(60)
    print("\n" * 50)

    del password, wallet, words
    gc.collect()
    return mnemonic


def change_password_interactive(wallet_path: Path = DEFAULT_WALLET_PATH) -> None:
    raw = _load_wallet_json(wallet_path)
    version = int(raw.get("version", 1))
    if version < WALLET_VERSION_V2:
        raise RuntimeError(
            "⚠️ Old wallet format detected. No mnemonic backup available.\n"
            "   Consider creating a new wallet and transferring funds."
        )

    wallet = unlock_wallet_interactive(wallet_path=wallet_path, max_attempts=3)
    if not wallet.mnemonic:
        raise RuntimeError("❌ Wallet does not contain mnemonic backup; cannot change password safely")

    print("Set new password:")
    new_password = _new_password_interactive()
    payload = create_wallet_payload_v2(mnemonic=wallet.mnemonic, password=new_password)
    _write_wallet(wallet_path, payload)
    print("✅ Wallet password updated")

    del wallet, new_password, payload
    gc.collect()


def migrate_legacy_wallet_interactive(wallet_path: Path = DEFAULT_WALLET_PATH) -> None:
    raw = _load_wallet_json(wallet_path)
    version = int(raw.get("version", 1))
    if version >= WALLET_VERSION_V2:
        print("✅ Wallet is already version 2. No migration needed.")
        return

    # Unlock legacy wallet with password attempts.
    wallet: Optional[WalletSecrets] = None
    for attempt in range(1, 4):
        try:
            password = _unlock_password_interactive("🔐 Enter current wallet password: ")
            wallet = _unlock_legacy(raw, password)
            break
        except Exception:
            if attempt < 3:
                print("❌ Invalid wallet password")
            else:
                raise RuntimeError("❌ Wrong password 3 times. Exiting.")

    if wallet is None:
        raise RuntimeError("❌ Failed to unlock legacy wallet")

    if not wallet.mnemonic:
        raise RuntimeError(
            "⚠️ Legacy wallet secret is not mnemonic-based.\n"
            "   Cannot migrate to v2 mnemonic backup automatically."
        )

    change_pw = input("Use a new password for migrated wallet? [y/N]: ").strip().lower() == "y"
    if change_pw:
        print("Set new password:")
        new_password = _new_password_interactive()
    else:
        new_password = password

    backup_path = wallet_path.with_name(f"wallet.legacy.backup.{int(time.time())}.json")
    shutil.copy2(wallet_path, backup_path)

    payload = create_wallet_payload_v2(mnemonic=wallet.mnemonic, password=new_password)
    _write_wallet(wallet_path, payload)

    print("✅ Wallet migrated to version 2")
    print(f"🔑 Address: {payload.get('address')}")
    print(f"📦 Legacy backup: {backup_path}")

    del wallet, password, new_password, payload
    gc.collect()
