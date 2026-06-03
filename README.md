# Alice Wallet

Native desktop wallet for the **Alice Protocol** — macOS, Windows, and Linux.

Alice Wallet is a self-custody wallet for the Alice on-chain token (SS58 prefix
300). It can **embed and manage a full Alice node** (Monero-GUI / Bitcoin-Core
style — bundle, launch, and show honest sync state), keeps your keys encrypted
on your own machine, and ships **signed auto-updates** so you stay current
without re-downloading. The UI is bilingual (English / 中文).

## Features

- **Create or import a wallet** — generate a new wallet (with a recovery phrase),
  or import an existing **12/24-word BIP39 mnemonic** or a **raw private key**.
- **Encrypted at rest** — Argon2id (t=3) key derivation + AES-256-GCM; the seed
  is zeroized in memory after use. Your keystore lives in your OS data directory
  (`~/Library/Application Support/AliceWallet`, `%APPDATA%`, `~/.local/share`),
  **not** inside the app bundle — so updates never touch your keys.
- **Embedded full node** — start/stop a managed Alice node, with a productized,
  fail-closed sync view (height / target / peers / freshness); or connect to a
  remote RPC.
- **Balances, receive (QR), send review, and transaction history.**
- **Light XMR mining status** view (uses your wallet address as the reward
  identity; execution stays off until you opt in).
- **Signed auto-updates** — the wallet checks for new releases, verifies an
  **ed25519 signature** over the release manifest with an embedded public key,
  verifies each artifact's SHA-256, then applies the update and keeps the
  previous version as a last-known-good rollback. You are always prompted before
  an update is applied.
- **Auto-lock**, copy-to-clipboard with auto-clear, and re-auth-gated private-key
  export.

## Status

The Alice chain launches with the protocol's reward phase. Until then, on-chain
actions (live transfers, confirmed balances) are inert by design — the wallet is
fully usable for **key management, address generation, and node management**, and
will activate on-chain features automatically once the network is live.

## Install

Download the build for your OS from the
[**Releases**](https://github.com/V-SK/alice-wallet/releases) page, then verify
integrity and follow the per-OS run steps in
[`docs/INSTALL.md`](docs/INSTALL.md):

- **Verify** the download against the published `SHA256SUMS` (+ ed25519
  signature) before running — never run a wallet binary that fails verification.
- **Run** — the builds are ad-hoc signed (so they run on Apple Silicon) but are
  **not** Apple/Windows certificate-signed; `docs/INSTALL.md` has the one-time
  "open anyway" steps for each OS (terminal install avoids macOS quarantine).

## Build from source

```bash
git clone https://github.com/V-SK/alice-wallet.git
cd alice-wallet/gui
cargo run
```

Requires a stable Rust toolchain. `cargo test` runs the wallet test suite.

## Releasing (maintainers)

Releases are built per-OS, ad-hoc signed, checksummed, and the manifest +
`SHA256SUMS` are signed **offline** with the project's ed25519 release key (never
in CI). See [`gui/scripts/release.sh`](gui/scripts/release.sh) and
[`docs/UPDATE-SCHEME.md`](docs/UPDATE-SCHEME.md) for the signing scheme and the
auto-update manifest format.

## Security

Self-custody: you hold the keys. Back up your recovery phrase — it is the only
way to restore your wallet. The wallet never transmits your seed or private key;
private-key export is re-authentication-gated and cleared on screen exit.
