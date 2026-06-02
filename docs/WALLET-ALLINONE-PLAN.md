# Alice Wallet — All-in-One Desktop App Implementation Plan

Status: plan v1 (planning only — no code, no commits)
Date: 2026-05-31
Target: rework the egui Alice Wallet into a Monero-GUI / Bitcoin-Core-style all-in-one desktop app that **bundles + manages the Alice full node** and runs **built-in CPU mining that earns ALICE credit** via the ACP credit path.

## 0. Grounding summary (what exists today)

**Wallet** (`/Users/v/alice/alice-wallet/gui`, egui/eframe Rust):
- `src/main.rs:44` creates one `tokio::runtime::Runtime`; `src/app.rs:218-222` wires a `spawn_worker` actor: GUI sends `AsyncAction` over an mpsc channel, a worker thread runs async work and returns `AsyncResult` (`app.rs:1270`). This is the exact seam for node + miner supervision.
- `src/chain.rs` is a thin **subxt** client (`OnlineClient<PolkadotConfig>`), with a mature fail-closed node-sync model (`NodeSyncSnapshot`, `evaluate_node_sync`, chain-identity pinning to genesis `0x7746…`, runtime `solochain-template-runtime` v108, `fetch_node_sync_snapshot` at `chain.rs:400`). **Critical: `require_wss_url` (`chain.rs:28`) rejects every non-`wss://` URL — including `ws://127.0.0.1` on loopback.** A bundled local node serves plaintext `ws://127.0.0.1:9944`, so this guard is a direct blocker (see Phase 2, Risk R1).
- `src/miner.rs` is execution-OFF: `MINING_EXECUTION_ALLOWED=false` and friends; it only computes a reward *projection*. `src/ui/mining.rs` renders disabled Start/Stop. There is **no subprocess spawning anywhere** (grep for `Command::new`/`Child` finds nothing).
- Nav is a `Page` enum + left sidebar (`ui/shell.rs:193-200`): Dashboard / Receive / Send / Mining / History / Accounts / AddressBook / Settings. Adding a **Node** page is a localized change.
- Config: `src/config.rs` — `Settings { rpc_url, auto_lock_minutes, lang }`, data root `dirs::data_local_dir()/AliceWallet` overridable via `ALICE_WALLET_DATA_ROOT` (`config.rs:53`). Default RPC `wss://rpc.aliceprotocol.org`.
- CI: `.github/workflows/release.yml` builds only the `gui` binary for linux-tar / mac-.app / win-zip and emits `SHA256SUMS`. macOS uses ad-hoc `codesign -s -` (unsigned).

**Node** (`Alice-Node/alice-chain`): a stock Substrate **solochain-template-node**.
- Binary: `solochain-template-node`. Build: `cargo build --release` (or `--profile production`, LTO).
- Needs the **raw mainnet chain spec** (`alice-mainnet-raw.generated.json`) at runtime via `--chain`.
- **Consensus is Aura + Grandpa (authority-based), NOT Proof-of-Work.** Node "sync" = downloading blocks over p2p from authority/boot nodes. Linchpin: **chain sync and "mining for credit" are fully decoupled** — the node does no CPU mining; CPU mining is the separate XMR→pool→ACP-credit path.
- Launch (prod ref): `solochain-template-node --chain <spec> --validator --name … --rpc-methods safe`. Ports: p2p 30333, RPC 9944 (ws+http unified), prometheus 9615. Data dir: Substrate base-path (`--base-path`).
- Chain props: `tokenSymbol=ALICE`, `tokenDecimals=12`, `ss58Format=300` — matches wallet `chain.rs:7 TOKEN_DECIMALS=12`.
- Source→binary attestation discipline exists (`docs/CHAIN_SOURCE_TO_BINARY_ATTESTATION.md`): Cargo.lock SHA, spec SHA, runtime code hash, binary hash.

**Engine** (`Alice-Protocol/miner`, Python): the mining engine the wallet runs as a subprocess.
- Entry: `python -m miner.mining_internal.acp_public_entry --loop --passport-id <P> --device-id <D> --device-label <L>` (`acp_public_entry.py:559 main`, `:392 run_public_acp_mining_loop`).
- Auto-detects hardware lanes, runs `PublicMiningLoop` over per-lane runners (`XmrLaneRunner` = XMR-CPU to a pool), drives the ACP flow via `PublicMinerAgent`: `request_session` → `heartbeat` → `upload_*_proof`.
- **Identity + PoP**: `device_identity.py` persists an Ed25519 device key 0600 at `~/.alice/device_ed25519.key` (`$ALICE_HOME`/`$ALICE_DEVICE_KEY_FILE` overrides) and builds the device PoP (`build_device_pop`, scheme `ed25519-shadow-device-pop-v1`) that `/session/issue` requires.
- **Credit-only hard-pinned in the engine**: `validate_public_agent_config` / `validate_forbidden_public_acp_flags` reject `live_reward`, `payout_executor`, miner-payout address, `direct_pool`, any `paid_acu != 0` (`public_agent.py:272-326`, `acp_public_entry.py:139-149`). Structured stdout: `public_acp_miner_loop_done … paid_acu=0 live_reward=false`.
- **`passport_id` is a public identity string (like a username), NOT a secret.** The engine never sees wallet seed/keys.
- Heavy deps include `torch`/`numpy` — but the worker repo proved the **real credit path needs only a 28-module subset + `cryptography`/`requests`/`psutil` (NO torch)**.

**ACP credit path** (`alice-acp/src/alice_acp/shadow_server`):
- HTTP: `POST /session/nonce` → `POST /session/issue` (device PoP) → `POST /heartbeat` → `POST /proof/ingest`; **`POST /device/register`** = credit-only self-enrollment (H_a); **`GET /shadow/balance?passport_id&device_id`** returns earned credit.
- `mining_authority_bridge.py` closes proof→credit CREDIT-ONLY server-side; never reward/payout, never chain, `paid_acu` stays "0". Public base URL allowlist: `https://aliceprotocol.org/api/acp` (+ `https://ps.aliceprotocol.org/acp`).

**Miner client (separate product, shares packaging)** (`Alice-worker-v1`): the "Worker Console GUI + alice-miner CLI".
- Already vendored the engine into `client_ui/mining_engine/` (28-module closure; `README_VENDORED.md`) + `mining_engine_runtime.py::MiningEngineController` (process-singleton, kill-switch, JSON snapshot, `paid_acu:"0"`). Has a loopback-only native server, a macOS Swift/WebKit wrapper, `packaging/worker_packaging_plan.py` (unsigned, HF, forbidden-material scan). Python 3.12.
- **Key delta vs wallet**: the worker runs the engine in `offline_smoke=True` fixture mode (no real binary/network → no real credit). The wallet must run it in **online/real mode** (real XMR-CPU → real credit). This is the biggest net-new mining piece + top spike (R4).

## 1. Architecture

### 1.1 Process model — three processes, one supervisor tree
```
AliceWallet.app  (egui process; owns the tokio Runtime + spawn_worker actor)
│
├── ProcessSupervisor  (NEW Rust module: gui/src/supervise/mod.rs)
│   ├── NodeSupervisor   → child: solochain-template-node  (bundled, sibling binary)
│   └── MinerSupervisor  → child: alice-mining-engine       (bundled embedded-Python runtime)
│
├── subxt client (existing chain.rs) ── ws://127.0.0.1:<rpc> ──▶ local node
└── ACP credit reader (NEW)           ── https GET /shadow/balance ─▶ aliceprotocol.org/api/acp
```
- egui stays the single GUI process + parent of both children. Reuse the existing `tokio::Runtime` + the `AsyncAction`/`AsyncResult` actor rather than a second async system. Supervision/IO runs on the worker thread; UI only reads snapshots.
- **NodeSupervisor** spawns the node as a child (`tokio::process::Command`), captures stdout/stderr, owns the `Child`+PID, exposes start/stop/restart/status. Health = (a) process liveness; (b) chain freshness via existing `chain::fetch_node_sync_snapshot` against local RPC. Mirrors Monero-GUI `DaemonManager`.
- **MinerSupervisor** spawns the embedded-Python engine as `acp_public_entry --loop`, passes identity via flags/env (never secrets), parses structured stdout into a `MinerStatus`. Owns `Child`/PID + a stop path. Supersedes the OFF stub in `miner.rs`.

### 1.2 Lifecycle / supervision / crash handling
- **Start order**: node first (must be reachable before balance refresh / meaningful proofs), miner on user action. Node autostart = setting (default on for "local node" profile).
- **Ownership & stop**: every child tracked by owned handle + PID file under `wallet_data_root()/run/{node,miner}.pid`. **Stop only the owned process** — never `pkill` by name. Graceful: SIGTERM (Win: `taskkill`/`CTRL_BREAK`) → bounded join → SIGKILL fallback.
- **Crash handling**: child exit while `running` → that subsystem `Error{code,last_log_tail}` + sanitized banner + Restart. Bounded auto-restart with backoff (≤3 / 5 min, then `Error`). **A node/miner crash must never lock or corrupt the wallet** (custody state independent — acceptance gate).
- **App shutdown**: on `eframe` exit, tear down both children (SIGTERM→SIGKILL); confirm-on-quit when node mid-sync or mining active.

### 1.3 IPC / status
- **Node → wallet**: no custom IPC. Status from (a) owned handle (alive/exit) + (b) JSON-RPC over local RPC reusing `chain.rs` (`system_health`, `system_syncState`, `system_chain`, `chain_getBlockHash(0)`, `state_getRuntimeVersion`). Existing `evaluate_node_sync` fail-closed logic works unchanged against the local node — big reuse win.
- **Miner → wallet**: parse the engine's structured stdout (`public_acp_miner_ready …`, `public_acp_miner_loop_done … paid_acu=0`); for richer live state run with short proof/heartbeat intervals, or add a small additive `--status-json <path>` to the engine.
- **Earned credit → wallet**: wallet does `GET /shadow/balance?passport_id&device_id` over HTTPS (reqwest/ureq or subxt's HTTP client) and renders the credit number. Independent of node RPC.

### 1.4 Data layout
| Data | Location | Owner |
|---|---|---|
| Wallet keystore/config | `dirs::data_local_dir()/AliceWallet` (`ALICE_WALLET_DATA_ROOT` override) | wallet |
| Node DB / chain data | `…/AliceWallet/node` via `--base-path` | node child |
| Bundled raw chain spec | read-only in bundle; SHA-validated first run | wallet/packaging |
| Engine device key (Ed25519, 0600) | `$ALICE_HOME/device_ed25519.key`; set `ALICE_HOME=…/AliceWallet/mining` | engine child |
| Engine logs | `…/AliceWallet/mining/public-loop-logs` | engine child |
| PID files | `…/AliceWallet/run/{node,miner}.pid` | supervisor |

Set `ALICE_HOME`/`ALICE_DEVICE_KEY_FILE` into the miner child's env to keep engine state under the wallet data root.

### 1.5 UI: node sync + mining state + earned credit
- **Node sync**: reuse `NodeSyncSnapshot`/`NodeSyncState`. Show profile (local/remote/offline), height/target, lag, peers, freshness, fail-closed reason. Separate **node-synced** from **wallet-refreshed/balance-trusted** (`chain.rs::allows_balance_refresh` already gates balance on sync).
- **Mining state**: real Start/Stop on the Mining page driven by `MinerStatus` (installed/ready/starting/running/stopping/error, per-lane hashrate/accepted-shares, uptime, sanitized log tail). Replace the projection panel.
- **Earned credit**: a "Mining credit (ALICE, credit-only)" card from `/shadow/balance`, labeled **credit / shadow_only, paid_acu = 0, not a spendable on-chain balance and not self-mined XMR**.

## 2. Node bundling + management (the "bundle monerod" half)
- **Binary/build**: `solochain-template-node` via `cargo build --release` (consider `--profile production`); self-contained per target (linux x86_64 / macOS arm64 / win x86_64) — same matrix as the wallet.
- **Chain spec**: bundle `alice-mainnet-raw.generated.json` read-only; pass `--chain`; verify SHA-256 against a pinned constant on first launch, refuse on mismatch (fail-closed, like `chain.rs` chain-identity pinning).
- **Launch args** (wallet-controlled, validated): `--chain <spec> --base-path <AliceWallet/node> --name "AliceWallet-<id>" --rpc-methods safe --rpc-port <port> --no-telemetry`; **NOT `--validator`** (sync node, not author); **`--bootnodes`** REQUIRED (bundle the genesis/boot multiaddr, else no peers — R2). Expert flags behind an advanced drawer.
- **R1 loopback-TLS conflict (load-bearing)**: `chain.rs::require_wss_url` rejects `ws://127.0.0.1:9944`. **Recommended fix**: allow plaintext `ws://` ONLY for `127.0.0.1`/`::1`/`localhost` to the wallet's own owned node port; keep `wss://`-only for every remote endpoint. Small scoped change to `require_wss_url` + `fetch_node_sync_snapshot` (`from_url`), covered by RED tests (loopback ws allowed; remote ws rejected). First thing to spike.
- **Node profiles**: promote `chain.rs::sync_mode_from_url` (LocalNode/RemoteNode) to a first-class Local-bundled / Remote / Offline setting; switching re-points subxt + updates action availability + starts/stops the bundled node.
- **Size/startup**: +one native binary per artifact (tens-to-100+ MB); fresh full-node first-sync takes minutes-to-longer — show honest progress, block balance-trust until synced; offer Remote node as fast-start while local syncs.

## 3. Built-in mining → ALICE credit, end-to-end
```
[wallet] identity: passport_id (public credit string), device_id (stable per-install, non-secret), device_label
         (NO seed/key/password ever crosses this boundary)
   → [wallet] spawn engine child: acp_public_entry --loop
        env: ALICE_HOME=<data>/mining, ALICE_MINING_PASSPORT_ID/DEVICE_ID/DEVICE_LABEL,
             XMR-CPU lane enabled, ALICE_PUBLIC_ACP_URL=https://aliceprotocol.org/api/acp
   → [engine] device key → /session/nonce → /session/issue (+PoP) → /heartbeat
              → XMR-CPU to pool under Alice-assigned worker → /proof/ingest  [paid_acu pinned 0]
   → [ACP] mining_authority_bridge: pool-evidence authority → ledger → CREDIT-ONLY (reward/payout/chain OFF)
   → [wallet] GET /shadow/balance → render "Mining credit (ALICE, credit-only)"
```
The wallet does NOT re-implement sessions/PoP/proofs/scoring — it **manages the engine and reads the credit** (the exact path proven in #12).

**Reuse verbatim** (engine): session issue + PoP + heartbeat + proof upload + credit-only invariants; lane auto-detect → XMR-CPU runner. **Reuse** (ACP): `/session/*`, `/proof/ingest`, `/heartbeat`, `/device/register`, `/shadow/balance`, `_credit_only_envelope`, `mining_authority_bridge`. **Reuse pattern** (worker-v1): `MiningEngineController` lifecycle/snapshot, the vendored 28-module subset + re-vendor discipline, `worker_packaging_plan.py`, native bundle layout.

**Net-new (Rust)**: `MinerSupervisor` (spawn/own/stop + stdout→`MinerStatus`); identity resolution + the credit-only env/flag envelope + a Rust pre-launch guard mirroring `validate_forbidden_public_acp_flags` (defense-in-depth); `/shadow/balance` reader + credit card; replace `miner.rs`/`ui/mining.rs` OFF stubs with the live control plane. **Net-new, small, engine (optional)**: `--status-json <path>` atomic snapshot per interval (additive, credit-only).

**Credit-only envelope (locked, enforce 4×)**: `live_reward=false`, `payout_executor=false`, `paid_acu=0`, no payout address, `mode != direct_pool`. Enforced in engine config validation + forbidden-flag guard + ACP `_credit_only_envelope` + the new wallet pre-launch guard. Wallet NEVER passes seed/key/password to the child (RED-tested).

**R3 — passport ↔ wallet identity (the one real open question)**: `passport_id` is a public credit-subject minted out-of-band today. Two credit-only routes: (1) **self-enroll via `/device/register`** (H_a built for this) — derive `passport_id` from the wallet address/pubkey, get a server nonce, sign enrollment PoP with the device key, register, mine (lowest friction, recommended to spike); (2) **owner-issued passport** entered in settings (simplest, out-of-band). Spike must confirm which the ACP deployment accepts + whether credit binds to the wallet address.

**Gating**: Mining Start enabled only when engine binary present+verified, identity resolved, credit-only envelope holds, wallet backup complete (policy), node-profile readiness met. Mining is NOT gated on node sync for correctness (XMR→pool credit doesn't need the Alice node) but UI surfaces node state. Stop targets only the owned engine PID.

## 4. Packaging (shared G4 track)
Each artifact gains a **node binary** + an **embedded Python runtime + vendored engine**:
```
linux:  AliceWallet/  { AliceWallet, solochain-template-node, runtime/python3.12/…, engine/, res/<spec>.json, … }   → tar.gz
macOS:  AliceWallet.app/Contents/{ MacOS/{AliceWallet, solochain-template-node}, Resources/{python3.12, engine/, <spec>.json, icon} } → zip
win:    AliceWallet/  { AliceWallet.exe, solochain-template-node.exe, python\, engine\, res\<spec>.json } → zip
```
Wallet resolves sibling paths relative to its own executable.
- **Vendor the engine** = the 28-module import closure of `acp_public_entry` into `gui/engine/mining_internal/` + `README_VENDORED.md` + re-vendor discipline (source of truth = `Alice-Protocol/miner/mining_internal`; verbatim; side-effect-free `__init__`). Do NOT fork engine logic.
- **Embed relocatable Python 3.12** (PEP-604 → 3.12 required) via python-build-standalone per platform (NOT a dev venv); **only** `cryptography`/`requests`/`psutil` (NO torch/numpy). Note macOS `pyexpat`/`libexpat` gotcha. Run child as `<bundled python> -m mining_internal.acp_public_entry --loop …`.
- **Size**: multi-hundred-MB per platform (node + Python + engine). Mitigate: per-platform artifacts, stripping, `--profile production`. **Flag total to owner before CI work.**
- **Unsigned posture (per owner)**: all three binaries unsigned (macOS ad-hoc `codesign -s -` only). Document Gatekeeper/SmartScreen friction; **SHA256SUMS is the only tamper check → manifest must cover EVERY bundled binary**. Widened surface mitigations: (a) verify chain-spec SHA; (b) runtime-pin node+engine hashes vs build-baked constants (extend `CHAIN_SOURCE_TO_BINARY_ATTESTATION`); (c) node RPC loopback-only `--rpc-methods safe` (no `--rpc-external`); (d) engine outbound HTTPS only to allowlisted ACP + pool; (e) reuse worker-v1 `worker_packaging_plan.py` forbidden-material scan as a release gate.
- **Shared with miner client**: factor embedded-Python + vendored-engine + SHA-manifest + forbidden-scan into one shared recipe (worker-v1 owns the base). Wallet's extra piece = the node binary (the miner client does not bundle the node).

## 5. CI / release changes vs current release.yml
1. **Build/obtain node binary per target** — either a job that checks out `Alice-Node/alice-chain` at a pinned ref + `cargo build --release` per matrix (slow; cache hard; `--profile production` for tags), or download a hash-pinned prebuilt per platform. Record node source commit + binary SHA.
2. **Provision embedded Python per target** (standalone 3.12 + the 3 deps, pruned). Vendor/refresh the engine subset (+ a check it matches upstream).
3. **Bundle step** (extend per-OS Package): copy node + python + engine + spec alongside the GUI; keep `.desktop`/`Info.plist`/icon; fix macOS `Info.plist` version to read from `Cargo.toml`.
4. **SHA256SUMS over every bundled binary** (GUI + node + python + engine), not just archives.
5. **New gates**: Rust tests (incl. loopback-ws RED + supervisor); engine `compileall`/import-smoke on 3.12; chain-spec SHA check; forbidden-material scan; an `--offline-smoke` engine run (hermetic import+drive); a launch smoke booting the bundled node to `system_health` on loopback then teardown.
6. macOS keeps ad-hoc `codesign -s -` (add the node binary so it loads).

## 6. Phased sequencing
Phases 2 and 4 can proceed in parallel after Phase 1.

**Phase 0 — Spikes (do first):** R1 loopback `ws://` exception in `chain.rs` (+ confirm subxt `from_url` accepts loopback ws); R2 boot the cloned node locally, confirm it syncs/serves `system_health`+`system_syncState` on loopback, capture the boot-node multiaddr; R3 confirm wallet→passport route + credit-binds-to-address; **R4 drive the REAL (non-offline-smoke) engine loop end-to-end against ACP staging with a test passport/device — confirm PoP, `/proof/ingest`, non-zero `/shadow/balance`, `paid_acu=0` (riskiest unknown — worker only ran fixture mode)**; R5 stand up relocatable Python 3.12 + 3 deps + vendored engine per OS, confirm `--offline-smoke` imports/runs (macOS pyexpat).

**Phase 1 — Supervisor foundation (Rust, no bundled binaries):** add `gui/src/supervise/` (`ProcessSupervisor`/`NodeSupervisor`/`MinerSupervisor`) with owned-handle/PID lifecycle, bounded-restart, sanitized log tail; wire new `AsyncAction`/`AsyncResult` into `spawn_worker`. Land the loopback-ws fix (R1) with RED tests. Command-builders + lifecycle unit-testable without real binaries (reuse worker snapshot shape).

**Phase 2 — Node integration (deps 1, R1, R2):** bundle-path resolution + chain-spec SHA verify; launch the local node with validated args; promote `NodeSyncMode` to Local/Remote/Offline profiles; add a Node page (`ui/node.rs`, `Page::Node`) showing process + sync state (reuse `NodeSyncSnapshot`); gate balance-refresh/send on local-node readiness; node autostart setting. Reuses `chain.rs` wholesale.

**Phase 3 — Mining → credit (deps 1, R3, R4):** replace `miner.rs` OFF constants + `ui/mining.rs` projection with identity resolution, credit-only launch envelope + Rust forbidden-flag guard, `MinerSupervisor` driving `acp_public_entry --loop` (real mode), structured-status parsing, `/shadow/balance` reader, live Start/Stop + credit card. Reuse engine + ACP verbatim.

**Phase 4 — Packaging (shared G4; deps R5; parallel with 2/3):** vendor the engine subset; define the shared embedded-Python+engine recipe (with the miner client); extend per-OS bundling to include node+python+engine+spec; runtime hash-pinning; forbidden-material scan.

**Phase 5 — CI/release (deps 2/3/4):** implement §5; full SHA256SUMS over all binaries; new gates + launch/offline smokes; document unsigned posture.

**Phase 6 — Hardening / honest-state polish:** crash-recovery UX (crash never locks wallet), confirm-on-quit while syncing/mining, advanced drawers, RED-test sweep extended to subprocess supervision + real-credit gating, README/release-notes rewrite (node trust boundary, credit-only/`paid_acu=0`, unsigned, "not self-mined XMR").

### Riskiest unknowns (spike before committing the phase)
1. **R4 — real engine credit path from a bundled subprocess** (everything proven so far is fixture-mode in worker-v1). Highest risk; spike in Phase 0.
2. **R1 — loopback-TLS guard conflict** (`chain.rs` rejects local `ws://`). Blocks all local-node work; cheap to spike.
3. **R3 — passport ↔ wallet-identity** mapping for credit attribution.
4. **R2 — node first-sync + boot node** availability/time.
5. **Artifact size** from bundling node + Python (flag to owner early).

### Critical files
- `gui/src/app.rs` — `tokio::Runtime` + `spawn_worker` actor (`:218`, `:1270`); node/miner `AsyncAction`/`AsyncResult` + supervisor wiring + `Page::Node`.
- `gui/src/chain.rs` — subxt client + `require_wss_url` (`:28`, the loopback-TLS blocker) + `fetch_node_sync_snapshot`/`NodeSyncSnapshot` (reused for the local node).
- `gui/src/miner.rs` (+ `gui/src/ui/mining.rs`) — replace execution-OFF projection with the real MinerSupervisor control plane + `/shadow/balance` credit card.
- `Alice-Protocol/miner/mining_internal/acp_public_entry.py` — the subprocess entrypoint + flags/env/stdout contract + credit-only guards.
- `Alice-worker-v1/client_ui/mining_engine_runtime.py` — reusable engine-lifecycle/snapshot pattern (+ `mining_engine/README_VENDORED.md` + `packaging/worker_packaging_plan.py` for shared G4 vendoring/packaging).
- `alice-wallet/.github/workflows/release.yml` — CI to extend for node + embedded-Python bundling + full SHA256SUMS.
