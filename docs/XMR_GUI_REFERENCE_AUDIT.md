# XMR GUI Reference Audit For Alice Wallet

Status: source-based reference audit v0.1
Repository audited locally: `/Applications/monero-wallet-gui.app`
Source reference: `monero-project/monero-gui` tag `v0.18.4.7`, commit `541c895`
Date: 2026-05-17

## Scope

This is not a byte-for-byte clone plan and not a proprietary decompilation note.
Monero GUI is open source, so the highest-signal path is:

1. inspect the local macOS app bundle to confirm what is actually installed;
2. inspect the matching official source tag for product structure;
3. translate the product boundaries into Alice-native wallet, node, and miner modules.

No local wallet files, wallet caches, seeds, keys, or user logs were inspected for
this audit.

## Local Bundle Findings

Installed app:

- Path: `/Applications/monero-wallet-gui.app`
- Bundle executable: `monero-wallet-gui`
- Bundle identifier: `org.monero-project.monero-wallet-gui`
- Bundle short version: `0.18.4.7`
- URL schemes: `monero`, `moneroseed`
- Signing identity: `Developer ID Application: Monero Distribution Company Pty Ltd (R6LK4Q3MJT)`
- The app bundle includes `Contents/MacOS/monerod`.
- The included `monerod --version` reported `Monero 'Fluorine Fermi' (v0.18.4.6-release)`.
- `monero-wallet-gui` and `monerod` are arm64 Mach-O executables on this machine.
- The GUI links Qt/QML, QtWidgets, QtNetwork, protobuf, unbound, libusb, hidapi,
  sodium, boost, and OpenSSL components.

Binary strings confirmed that the installed wallet contains product surfaces for:

- `DaemonManager`
- `WalletManager`
- `Subaddress`
- `Mining`
- `P2Pool`
- local node / remote node warnings
- wallet and daemon sync progress
- restore height and rescan language
- receive requests and address-book flows

## Source Files Studied

### Application wiring

`src/main/main.cpp` registers wallet, daemon, and mining-facing managers into QML.
The important product signal is that daemon and mining managers are not hidden in
the wallet object:

- `DaemonManager daemonManager`
- `P2PoolManager p2poolManager`
- QML context properties for `daemonManager` and `p2poolManager`
- `idealThreadCount` exposed to the UI for mining controls

Alice lesson: the GUI needs explicit Rust modules for wallet state, node state,
and miner state. Mining must not be bolted onto the send/sign path.

### Mode selection

`wizard/WizardModeSelection.qml` separates product modes:

- Simple mode: remote node, easiest start.
- Simple mode bootstrap: local chain download with bootstrap help.
- Advanced mode: local node plus features such as mining and message verification.

`wizard/WizardDaemonSettings.qml` then asks the user to choose:

- start a local node automatically in the background;
- choose blockchain data location;
- prune blockchain;
- optionally configure a bootstrap node;
- or connect to a remote node.

Alice lesson: node mode belongs in onboarding and settings, not only in a raw RPC
URL text box.

### Daemon manager

`src/daemon/DaemonManager.h` and `src/daemon/DaemonManager.cpp` provide the local
node process boundary:

- `start(...)`
- `stopAsync(...)`
- `running(...)`
- `runningAsync(...)`
- `sendCommand(...)`
- `validateDataDir(...)`
- `checkLmdbExists(...)`
- `getArgs(...)`

The manager starts bundled `monerod` with explicit arguments:

- network selection;
- custom flags;
- data directory;
- bootstrap daemon;
- pruning;
- `--no-sync` for simple mode;
- `--check-updates disabled`;
- `--non-interactive`;
- default `--max-concurrency` based on half of ideal thread count.

It checks readiness by sending a daemon command and looking for sync information.
It also has start and stop watchers.

Alice lesson: the local node needs its own process manager and health model. A
GUI setting like `wss://...` is not enough to express local, remote, offline,
starting, synced, lagged, or failed.

### Main sync model

`main.qml` keeps separate high-level booleans:

- `daemonSynced`
- `walletSynced`
- `daemonRunning`
- `daemonStartStopInProgress`
- `isMining`

`onHeightRefreshed(bcHeight, dCurrentBlock, dTargetBlock)` updates daemon and
wallet progress separately. If the daemon is behind, the wallet progress is shown
as waiting for daemon sync. The side panel carries both wallet and daemon progress
bars.

Alice lesson: users need to know whether the node is synced, whether the wallet
view is synced, and whether the displayed balance is trusted. A single spinner is
not good enough.

### Remote/local node switching

`main.qml` has explicit `connectRemoteNode()` and `disconnectRemoteNode()` flows.
Switching nodes:

- exits P2Pool;
- updates current daemon address;
- clears or sets daemon login;
- re-initializes the current wallet against the selected node;
- updates wallet manager daemon address.

Alice lesson: switching node profile must also update action availability. Send,
rescan, mining, and balance trust should react deterministically.

### Wallet manager

`src/libwalletqt/WalletManager.h` exposes the wallet facade:

- create wallet;
- open wallet;
- restore from seed;
- create from keys;
- create from device;
- close wallet;
- wallet exists / find wallets;
- address validation;
- daemon address changes;
- connected / heights / network difficulty;
- mining status / start mining / stop mining;
- URI generation and parsing;
- QR code helpers;
- wallet cache clearing.

Alice lesson: the GUI should not directly stitch together unrelated low-level
calls. It should call a narrow wallet facade that reports state and accepts
explicit commands.

### Wallet object

`src/libwalletqt/Wallet.h` exposes wallet state and lifecycle surfaces:

- disconnected / refreshing / synchronized;
- seed and seed language;
- status and error string;
- history;
- address book;
- subaddress and subaddress accounts;
- view-only flag;
- wallet keys;
- wallet creation height;
- scan / import / export flows;
- rescan and spent-output repair surfaces;
- message and transaction proof functions.

Alice lesson: Alice does not need to copy Monero's UTXO scanning mechanics, but
it does need the same promise: the wallet can say what it knows, what is stale,
what can be rebuilt, and what is dangerous to export.

### Mining page

`pages/Mining.qml` is the core one-click mining reference.

Important product gates:

- mining warns when the user is on a remote node;
- mining warns when the daemon is not synchronized;
- mining warns about computer performance impact;
- the start button checks daemon readiness;
- solo mining calls `walletManager.startMining(address, threads, background, battery)`;
- P2Pool mining uses `P2PoolManager`;
- P2Pool can trigger a local daemon restart with a required ZMQ publish flag;
- the page polls status every two seconds while active;
- status text reports hashrate or startup/suspended state.

Important controls:

- mining mode: Solo or P2Pool;
- CPU threads with minus / plus buttons;
- use half of threads as recommended;
- use all threads;
- background mining option;
- P2Pool chain selection;
- optional advanced P2Pool flags;
- start and stop commands.

Alice lesson: the wallet can host a mining control panel, but mining must remain
an address-payout process, not a wallet-secret process.

### P2Pool manager

`src/p2pool/P2PoolManager.h` and `src/p2pool/P2PoolManager.cpp` manage an
external miner-like helper:

- check whether the helper binary is installed;
- download a platform-specific binary;
- verify the downloaded archive hash;
- extract the binary;
- start the helper with explicit arguments;
- write/read local status JSON;
- stop the helper and remove transient stats.

The start command uses address-only payout:

- `--wallet <address>`
- `--start-mining <threads>`
- `--local-api`
- `--data-api <stats-dir>`

Alice lesson: this is the closest reference for Alice one-click mining. The
wallet should start an Alice miner process with a payout address, read structured
status, and stop only the process it owns.

### Receive, address, account, and recovery surfaces

The relevant QML pages show a mature wallet lifecycle:

- `pages/Receive.qml`: address tab, payment request tab, URI and QR generation,
  amount, description, recipient name, copy/save QR.
- `pages/AddressBook.qml`: saved contacts, labels/descriptions, send-to action,
  edit/delete, search.
- `pages/Account.qml`: account/subaddress model, labels, balances, create account.
- `pages/settings/SettingsWallet.qml`: show seed/keys in advanced settings,
  rescan balance, scan transaction, password changes, warnings around remote
  nodes and privacy.

Alice lesson: even if Alice starts with one account address, it still needs
receive-request history, labels, and explicit privacy warnings around address
reuse and public miner payout addresses.

## Product Structure To Copy

Copy the structure, not the code or visual skin:

1. Wallet facade
   Owns wallet lifecycle, encryption, recovery, address derivation, signing, and
   export gates.

2. Node manager
   Owns local/remote/offline profiles, node process start/stop, health, lag, and
   trust status.

3. Sync trust model
   Separates node sync, wallet refresh, balance freshness, history freshness, and
   send readiness.

4. Miner manager
   Owns mining binary discovery, checksum/signature verification, process
   lifecycle, sanitized logs, hashrate/status, and payout address configuration.

5. GUI shell
   Shows wallet state first: balance, sync trust, node profile, receive, send,
   history, backup warning, mining status.

6. Advanced surfaces
   Seed/key export, raw restore, rescan, custom node flags, and miner flags live
   behind stronger warnings and confirmations.

## Alice One-Click Mining Design

Alice should not require miners to store wallet secrets. The wallet should only
provide a payout address and an operator control plane.

Target module shape:

- `MinerProfile`: disabled, local CPU, local GPU, pool/remote worker if supported.
- `MinerConfig`: payout address, binary path, endpoint/pool URL, thread or device
  limit, intensity, data directory, battery policy, autostart preference.
- `MinerStatus`: not installed, verifying, ready, starting, running, stopping,
  stopped, error.
- `MinerStats`: hashrate, accepted shares or submitted work, rejected work,
  uptime, last payout/checkpoint when available, last status update.
- `MinerCommand`: pure command builder that can be unit-tested without starting
  the miner.
- `MinerManager`: process owner that starts, stops, polls, and redacts logs.

Safety rules:

- The miner command must never include mnemonic, seed, private key, wallet
  password, or decrypted signing material.
- Changing the payout address requires an unlocked wallet or explicit manual
  address entry plus validation.
- Starting mining from a newly created unbacked wallet should be blocked or
  require a strong backup-first confirmation.
- Solo/local mining should require the local node profile to be running and
  synced, unless the selected mining backend explicitly does not need it.
- Pool/remote mining should show a payment-trust warning.
- Downloaded miner binaries must be checksum or signature verified before use.
- The GUI must stop only the process it owns, using a saved child process handle
  or PID file, not broad `pkill` behavior.
- Logs shown in the GUI must be sanitized and must not print secrets or full
  sensitive command environments.
- Crashes must leave the wallet usable and must not corrupt wallet files.

Recommended first Alice mining slice:

1. Add a pure `miner` module with config, status, and command-builder tests.
2. Add a placeholder GUI mining page that can show disabled / not-installed /
   ready / running states without starting real mining.
3. Wire start/stop behind a feature flag or dev-only setting.
4. Add binary verification and process ownership.
5. Add structured stats polling.
6. Enable one-click start only after RED tests prove no wallet secrets cross the
   miner boundary.

## What Alice Should Improve Over XMR GUI

- Track and stop owned miner processes only. Avoid broad process-name kills.
- Avoid free-form daemon/miner flags in the default UI. Put advanced flags behind
  an expert drawer with validation.
- Use structured status JSON or RPC rather than parsing human console text where
  possible.
- Keep warnings short and actionable: "balance stale", "node lagged by N blocks",
  "mining needs local synced node", "payout address is public".
- Keep mining available as an operator tool, not a custody requirement.

## RED-First Mining Tests

- RED: miner command builder never serializes seed, private key, wallet password,
  or mnemonic fields.
- RED: start is blocked when the miner binary is missing or unverified.
- RED: checksum mismatch leaves the installed miner disabled.
- RED: local solo mining is blocked when local node is disconnected or unsynced.
- RED: remote/pool mining displays a trust/payment warning before first start.
- RED: changing payout address is blocked while wallet is locked.
- RED: start from an unbacked new wallet is blocked or requires explicit
  backup-first confirmation according to policy.
- RED: stop kills only the owned miner process.
- RED: crash recovery returns status to stopped/error without locking the wallet.
- RED: miner logs are redacted before entering GUI state or persisted logs.

## Decision For Alice

The one-click mining feature should be built, but as a separate
`MinerManager`/`MiningPage` lane under LAUNCH-A. It should sit beside wallet and
node state, not inside signing code. The first successful milestone is not "mine
a block from the GUI"; it is "prove the GUI can start, monitor, and stop a miner
with only a payout address, while wallet custody remains sealed."
