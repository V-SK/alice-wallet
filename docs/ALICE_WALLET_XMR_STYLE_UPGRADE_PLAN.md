# Alice Wallet XMR-Style Upgrade Plan

Status: draft v0.2
Owner lane: LAUNCH-A / Wallet Upgrade
Repository: alice-wallet
Date: 2026-05-17

## Goal

Upgrade Alice Wallet from a usable native desktop wallet into a self-custody wallet with XMR-GUI-style product structure:

- wallet and node concerns are clearly separated;
- sync and trust state are honest before balance or send actions are presented as safe;
- create, encrypt, back up, restore, migrate, and rescan flows are complete;
- receiving, labels, request history, and local transaction history are understandable;
- one-click mining is available as an address-payout operator tool without exposing wallet secrets;
- private material is hidden by default and every sensitive export or signing path is gated.

The target is not a decorative wallet. The target is a wallet users can understand under stress: what is safe, what is stale, what can be rebuilt, and what is unrecoverable if lost.

## Current Audit

The current codebase is a good base for this lane.

- Native desktop GUI exists in `gui/` using Rust `eframe` / `egui`.
- The GUI has create, import, unlock, backup verification, dashboard, send, receive QR, history, settings, auto-lock, and English / Chinese UI strings.
- Wallet v3 encrypts the secret seed with Argon2id-derived AES-GCM and does not persist the mnemonic after creation.
- Legacy v2 wallets can be unlocked and upgraded.
- Wallet file writes use a temp file, fsync, and restrictive Unix permissions.
- The send path validates address and amount, uses `transfer_keep_alive`, and requires a hold-to-confirm review step.
- Release packaging exists for Linux, Windows, and macOS arm64.

The gaps are product-structure gaps more than a total rewrite.

- README still describes the project as a command-line wallet even though the desktop GUI exists.
- Node selection is only a raw RPC URL setting. There is no local-node / remote-node / offline profile model.
- Sync state is not explicit enough. The UI shows connection and block number, but not whether balance is trusted, stale, lagged, or read-only.
- Send gating does not yet depend on a first-class wallet-state machine.
- The staking UI is intentionally blocked behind a coming-soon placeholder while dead code still exists below it.
- The GUI has no rescan / reindex concept for rebuilding local history from chain truth.
- Address management is minimal: one receive QR for the primary address, no labels, no saved receive requests, no privacy guidance.
- There is no formal RED-first test suite for wallet lifecycle, sync honesty, node fallback, or locked-wallet signing denial.
- A pure `gui/src/miner.rs` command/safety core now exists, but there is still no GUI mining control plane, real miner process manager, binary verification workflow, or hashrate/status model.

## XMR GUI Reference Lessons

The local Monero GUI app on this machine is version `0.18.4.7`, bundles a local
`monerod`, and maps to the official `monero-gui` source tag `v0.18.4.7`.
The detailed reference audit is in
[`XMR_GUI_REFERENCE_AUDIT.md`](XMR_GUI_REFERENCE_AUDIT.md).

The product lessons Alice should copy are structural:

- `WalletManager` is the wallet facade for create/open/restore/signing/address flows.
- `DaemonManager` is a separate local node process manager.
- QML tracks daemon sync and wallet sync separately.
- local and remote node switching re-initializes wallet connectivity and updates action availability.
- mining is a separate page and manager surface, gated by daemon readiness and local/remote warnings.
- P2Pool-style mining uses address-only payout, binary installation/checksum checks, structured status, and explicit start/stop.

The product lessons Alice should not copy blindly:

- Monero's UTXO/subaddress/rescan mechanics do not map directly to Alice's account model.
- Broad process-name killing is too blunt for Alice; use owned process handles or PID files.
- Free-form miner and daemon flags should be expert-only, validated, and hidden from the default path.

## Product Principles

1. Honest state beats optimistic UX.
   If the wallet cannot prove freshness, the UI says so and blocks or downgrades risky actions.

2. Wallet and node are separate systems.
   A user can choose local node, remote node, or offline / read-only mode without changing wallet custody.

3. Backup status is a first-class safety state.
   Users should know the difference between a mnemonic, encrypted keystore, wallet file, and address-only recovery.

4. Signing is narrow and explicit.
   Locked, read-only, offline, stale, or backup-incomplete states must not silently sign.

5. Miner safety remains separate.
   Miner machines should be able to run with an address only. The desktop wallet is for custody and operator decisions, not a requirement for mining hot keys.

6. Mining is an operator workflow, not a signing workflow.
   Starting or monitoring a miner must never require a seed, private key, wallet password, or decrypted signing material to cross into the miner process.

## Target Architecture

### 1. Wallet Core

Add a first-class wallet-state model:

- `Empty`: no wallet found.
- `Locked`: encrypted wallet exists, no secrets in memory.
- `Unlocking`: password verification in progress.
- `BackupRequired`: newly created wallet has not completed backup verification.
- `Recovering`: import / migration / rescan flow is running.
- `Syncing`: wallet is unlocked but chain state is not yet trusted.
- `Usable`: wallet is unlocked, node is healthy, balance is fresh enough, signing is allowed.
- `ReadOnly`: address is known but secret material is unavailable or intentionally disabled.
- `Offline`: wallet can display local data but cannot refresh or broadcast.
- `Error`: wallet, node, or persistence layer has a blocking error.

Every page should read from this state rather than inferring safety from scattered booleans.

### 2. Node Layer

Introduce `NodeProfile` and `NodeStatus`:

- Local node: safest, slower initial setup, local RPC endpoint, local health checks.
- Remote node: fastest setup, explicit trust warning, configurable endpoint.
- Offline mode: no network, read-only local wallet inspection.

Node status should expose:

- configured profile;
- endpoint;
- connection state;
- local block height;
- best-known network height when available;
- lag in blocks;
- last successful refresh time;
- trust level;
- whether balance is trusted;
- whether sending is allowed.

The GUI should stop treating a websocket URL as the whole node model.

### 2.5. Miner Layer

Introduce `MinerProfile`, `MinerConfig`, `MinerStatus`, `MinerStats`, and a
`MinerManager` separate from wallet signing.

Miner profiles:

- Disabled: no mining controls except setup.
- Local CPU: start a local miner process with a payout address and thread limit.
- Local GPU: start a configured GPU miner profile when supported.
- Pool / remote worker: connect to a pool or remote mining endpoint when supported, with a payout-trust warning.

Miner status should expose:

- installed / missing / unverified binary state;
- selected payout address;
- start readiness and the reason if blocked;
- process state and owned PID or child handle;
- hashrate;
- accepted / rejected work when available;
- uptime;
- last status refresh;
- sanitized recent log lines;
- stop/crash/error reason.

Mining action gates:

- miner binary must be installed and verified;
- payout address must be valid;
- changing payout address requires unlock or explicit manual address validation;
- unbacked newly-created wallet cannot silently start mining;
- local solo mining requires local node readiness if the backend depends on it;
- remote/pool mining requires an explicit trust/payment warning before first start;
- miner command line must never include mnemonic, private key, wallet password, or decrypted signing material.

### 3. Sync And Rescan

Add an explicit sync model:

- balance freshness: unknown / stale / fresh;
- history freshness: unknown / stale / fresh;
- send readiness: blocked / reviewable / broadcastable;
- rescan state: idle / queued / running / complete / failed.

For Alice's account model, "rescan" means rebuilding local wallet-visible state from chain/indexer truth, not copying Monero's UTXO scanning mechanically. The product promise is the same: local history can be rebuilt when local records drift.

### 4. Backup And Recovery

Clarify three recovery surfaces:

- Recovery phrase: ultimate wallet recovery. If lost before device loss, funds can be unrecoverable.
- Encrypted wallet file: local convenience copy. Needs the password and should be backed up carefully.
- Address / public key: safe for receiving and miner payout config, but cannot spend.

Required flows:

- backup verification after wallet creation;
- restore from mnemonic;
- restore from raw seed only behind an advanced warning;
- wallet file migration audit trail;
- password change path;
- recovery dry-run that proves the restored address matches before replacing the current wallet;
- local data rescan after restore.

### 5. Address And Receive Management

Add a receive-request model:

- label;
- optional expected amount;
- optional memo / note stored locally only;
- created timestamp;
- fulfilled / archived status;
- QR display;
- copy status.

Alice may not need Monero-style subaddresses on day one. The first useful step is clean request labeling and history, with explicit privacy guidance around address reuse and public payout addresses.

### 6. GUI Information Architecture

The first screen should be the wallet, not a landing page:

- balance;
- sync trust badge;
- node profile and node health;
- mining status and start/stop entry when configured;
- receive;
- send;
- recent history;
- backup warning if relevant;
- locked / read-only / offline callouts.

Send should be disabled or downgraded when:

- wallet is locked;
- node is disconnected;
- node is lagged beyond policy;
- balance is unknown or stale;
- wallet is read-only;
- backup is incomplete and policy requires backup before first send.

Mining should be visible but gated when:

- wallet has no valid payout address;
- payout address is being changed while wallet is locked;
- miner binary is missing or unverified;
- local node is required but disconnected or unsynced;
- backup is incomplete and policy requires backup before first miner start;
- the chosen pool/remote endpoint has not been acknowledged as a trust boundary.

### 7. Release And Packaging

Release packaging should carry product truth:

- app version comes from `Cargo.toml` rather than a hard-coded stale Info.plist version;
- artifacts include a checksum manifest;
- release notes call out node-profile support and custody boundaries;
- macOS signing/notarization remains a separate explicit release gate.

## RED-First Test Plan

Batch 1: Wallet State Machine

- RED: locked wallet cannot produce a signing key.
- RED: backup-required wallet cannot enter normal usable state until verification is complete.
- RED: recovered mnemonic derives the same address before replacement.
- RED: import never overwrites an existing wallet without producing a backup.

Batch 2: Node Layer

- RED: remote node unreachable downgrades wallet to offline or error, never usable.
- RED: unknown network height makes balance untrusted.
- RED: lag above policy blocks send.
- RED: switching local / remote / offline updates action availability deterministically.

Batch 3: Sync And Rescan

- RED: stale balance cannot be displayed as trusted.
- RED: local failed transaction history does not masquerade as chain-confirmed history.
- RED: rescan failure preserves the previous local history and reports the failure.
- RED: successful rescan is idempotent.

Batch 4: Backup And Recovery

- RED: seed export is unavailable from the normal GUI path.
- RED: advanced raw-seed import requires explicit confirmation and never logs the seed.
- RED: password change preserves address and invalidates the old password.
- RED: legacy wallet upgrade removes persisted mnemonic material in the new payload.

Batch 5: Receive And History

- RED: receive request labels are local-only and never sent to RPC.
- RED: archived receive requests remain inspectable.
- RED: transaction history distinguishes local pending, failed, and finalized records.
- RED: copying sensitive content uses clipboard auto-clear where applicable.

Batch 6: Release Readiness

- RED: release workflow versions match `gui/Cargo.toml`.
- RED: packaging includes checksums.
- RED: README and release notes describe node trust boundaries.
- RED: GUI smoke test can launch without an existing wallet and reach create/import choice.

Batch 7: One-Click Mining

- RED: miner command builder never serializes mnemonic, seed, private key, wallet password, or decrypted signing material.
- RED: miner start is blocked when the binary is missing, unsigned, or checksum-unverified.
- RED: checksum mismatch leaves the miner disabled and does not mutate wallet state.
- RED: local solo mining is blocked when the required local node is disconnected or unsynced.
- RED: remote/pool mining displays a trust/payment warning before first start.
- RED: changing payout address is blocked while the wallet is locked.
- RED: unbacked newly-created wallet cannot silently start mining.
- RED: miner stop targets only the process owned by Alice Wallet.
- RED: miner crash reports error status without locking or corrupting the wallet.
- RED: miner logs are redacted before entering GUI state or persisted logs.

## Implementation Slices

1. Add `wallet_state` module and tests.
2. Add `node_profile` / `node_status` module and tests.
3. Replace scattered action gating with wallet-state-derived gating.
4. Add sync trust badges and send-block reasons to the GUI.
5. Add receive request persistence and labels.
6. Add `miner` module with command-builder and redaction tests. Started in `gui/src/miner.rs`.
7. Add miner binary verification and owned-process lifecycle.
8. Add mining page with disabled / ready / running / error states.
9. Add rescan command/state placeholder with no unsafe fake chain claims.
10. Fix stale README and release version packaging.
11. Add CI checks for Rust tests, Python compile, and release metadata.

## Acceptance Gates

The lane is not launch-ready until:

- all RED-first tests above pass;
- send is impossible from locked, read-only, offline, stale, or backup-required states;
- node profile and sync trust are visible on the dashboard;
- restore proves address identity before writing over the active wallet;
- docs distinguish mnemonic, encrypted wallet file, and address-only use;
- miner start/stop works with address-only payout and never receives wallet secrets;
- local mining readiness is tied to node status instead of a blind start button;
- release artifacts carry consistent version and checksum information.

## Non-Goals

- No private-key export in the normal GUI.
- No requirement for miners to store wallet secrets.
- No miner process that receives seed, private key, wallet password, or decrypted signing material.
- No fake local-node manager until Alice local node startup and health checks are real.
- No staking activation until the chain pallet and mainnet policy are confirmed.
- No production signing, deployment, or operator-grant side effects from this plan document.
