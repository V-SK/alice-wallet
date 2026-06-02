# Alice Wallet — embedded full-node + all-in-one build notes

Branch: `wallet-fullnode` (off `codex/phase50c-wallet-signed-distribution-readiness`).
Date: 2026-06-02. Status: code on branch; nothing published/pushed/deployed.

This documents the current-state assessment, what was built toward the
Monero-GUI / Bitcoin-Core "embed + manage a full node" vision, and the open
decisions that need V.

---

## 1. Current-state assessment (before this work)

**Wallet (`gui/`, egui/eframe Rust):**
- `chain.rs` — mature **subxt** client with a fail-closed node-sync model
  (`NodeSyncSnapshot` / `evaluate_node_sync`), chain-identity pinning, and
  balance-trust gating. **It was REMOTE-RPC only**: `require_wss_url` rejected
  every non-`wss://` URL, including loopback `ws://127.0.0.1` — a hard blocker
  for an embedded local node (this is the plan's risk R1).
- `app.rs` — one `tokio::Runtime` + a `spawn_worker` actor (mpsc
  `AsyncAction`/`AsyncResult`). Clean seam for node/miner supervision.
- `miner.rs` / `ui/mining.rs` — light XMR mining is **execution-OFF**
  (`MINING_EXECUTION_ALLOWED=false`); it renders a reward *projection* and a
  disabled Start/Stop. **No subprocess spawning existed anywhere.**
- Nav = `Page` enum + sidebar (Dashboard/Receive/Send/Mining/History/Accounts/
  AddressBook/Settings).
- Security-audit fixes present and intact: wss-only TLS for remote RPC, secret
  zeroization, argon2 t=3, AES-GCM, fail-closed sync.
- `release.yml` builds only the `gui` binary per-OS (linux tar / mac .app / win
  zip), ad-hoc-signed macOS, SHA256SUMS over archives.

**Gap to the vision:** no embedded/managed node, no process supervision, no
Node UI, loopback RPC blocked, release pipeline bundles no node binary.

**Baseline test/build:** `cargo build` green; `cargo test` 41 passed; Python
suite 55 source/readiness tests (1 env-only failure: `cryptography` not in the
system Python — not a code issue).

---

## 2. What was built (this branch)

All new Rust is unit-tested and works **without a bundled node binary** (the
binary itself is a release step — see §4). Total Rust tests now **65 passed**
(was 41); Python suite **59 passed** in a venv with `cryptography`.

### 2.1 R1 — loopback-`ws://` exception (`chain.rs`) — the load-bearing fix
- `require_wss_url` now allows plaintext `ws://` **only** for loopback hosts
  (`127.0.0.1`, the whole `127.0.0.0/8` block, `::1`, `localhost`); every remote
  endpoint still requires `wss://`. `http(s)://` is rejected everywhere.
- Hardened host parsing so substring-spoofed hosts (`ws://127.0.0.1.evil.example`,
  `ws://localhost.evil.example`) are correctly classified **remote** and
  rejected — a naive `contains()` check would have leaked. `sync_mode_from_url`
  was hardened the same way.
- RED tests prove: loopback ws allowed, remote ws rejected, spoof rejected,
  http(s) rejected. **The audit invariant (remote = TLS-only) is preserved.**

### 2.2 Embedded-node module (`gui/src/node.rs`)
- `NodeMode` (LocalEmbedded / Remote / Offline) + `NodeSettings` (ports,
  autostart, node name), persisted in `config::Settings` (back-compat: old
  config files load unchanged via `#[serde(default)]`).
- Binary + chain-spec resolution as **siblings of the wallet executable**
  (per-OS layout matching the packaging plan), with `ALICE_WALLET_NODE_BIN` /
  `ALICE_WALLET_CHAIN_SPEC` overrides.
- **Validated launch-plan builder** — produces the exact `solochain-template-node`
  argv as a **sync node, never `--validator`**, loopback `--rpc-methods safe`
  (never `--rpc-external`), `--no-telemetry`, sanitised `--name`, validated
  ports, and bootnode multiaddrs (rejects flag-injection).
- **Chain-spec SHA-256 verification** (fail-closed) + a `pinned_chain_spec_sha256()`
  seam (env-overridable, baked at release time).
- `bundled_bootnodes()` seam (env-overridable; baked at release time).

### 2.3 Process supervision (`gui/src/supervise/`)
- `mod.rs`: `ProcState` lifecycle, UI-safe `ProcStatus` snapshot, **bounded
  `RestartPolicy`** (≤3 restarts / 5 min, exponential backoff capped at 30s),
  a bounded `LogRing`, and **`sanitize_log_line`** (strips ANSI, drops control
  chars, **redacts long hex blobs so a log panel can never leak a key/seed**).
- `child.rs`: `tokio::process` spawn that **owns the `Child` + PID**, writes a
  PID file, captures stdout/stderr line-by-line, puts the child in its own
  process group, `kill_on_drop`, and a graceful **SIGTERM → bounded wait →
  SIGKILL** stop. Ownership rule honored: we only ever signal the process we
  spawned — never `pkill` by name.
- `node_supervisor.rs`: `NodeSupervisor` ties it together — start/stop, liveness
  polling, auto-restart on unexpected exit, generation counter to ignore stale
  loops. **A node crash never touches wallet custody state.** Verified by an
  integration test that crash-loops a stand-in process and asserts it lands in
  `Error` ("budget exhausted") without locking up.

### 2.4 App wiring (`app.rs`)
- New `AsyncAction::{StartNode, StopNode, PollNodeProc}` and
  `AsyncResult::{NodeProc, NodeProcErr}`, handled on the existing worker.
- App holds a `NodeSupervisor` (shared with the worker) + the latest
  `ProcStatus`; polls process status every 2s in LocalEmbedded mode.
- `effective_rpc_url()` routes chain queries to the **embedded node's loopback
  RPC** in LocalEmbedded mode, the remote URL otherwise; Offline mode fails
  closed (no balance/sync).
- `on_exit` tears the node child down on app shutdown (bounded graceful stop;
  `kill_on_drop` backstop).

### 2.5 Node page (`gui/src/ui/node_view.rs`, `Page::Node`)
- Mode selector (Local/Remote/Offline) with descriptions; switching persists +
  re-points the RPC and stops a running local node when leaving local mode.
- Local-node card: process state + PID + RPC endpoint + restart count + message,
  **Start/Stop** wired to the supervisor, gated on binary presence and
  isolation flags; clear **"node binary not bundled"** and **"no bootnodes"**
  warnings.
- Sync card reuses the existing `NodeSyncSnapshot` (height/peers/progress/
  fail-closed reason). Log card shows the sanitised tail.
- Full EN + ZH i18n; sidebar nav entry + Cmd/Ctrl-5 shortcut (others shifted).

### 2.6 Light XMR mining — **kept, not ripped out**
- `miner.rs` / `ui/mining.rs` are **unchanged**. `MINING_EXECUTION_ALLOWED=false`
  is the audited safety posture and is preserved. The mining page still renders
  the route, reward projection, and (intentionally disabled) controls.
- The supervision foundation built here (`supervise/`) is the same machinery the
  plan's Phase 3 uses to drive the real mining engine — but flipping mining to
  **live** requires the R3/R4 spikes (passport↔wallet identity, real ACP credit
  path from a bundled subprocess) and the vendored Python engine, which are NOT
  in scope for a no-regression branch build. See §4.

### 2.7 Release pipeline (`.github/workflows/release.yml`)
- Added a **`cargo test` gate** before build (the new tests run in CI).
- Added an **optional node-binary + chain-spec staging step** (from committed
  `gui/release-assets/<target>/` or `ALICE_NODE_BIN_URL`/`ALICE_CHAIN_SPEC_URL`
  secrets) and per-OS bundling into the sibling layout the wallet expects. When
  absent, the wallet still ships and works in Remote mode (CI stays green before
  V supplies node binaries).
- Fixed the macOS `Info.plist` version drift (now read from `Cargo.toml`) and
  made `codesign` cover the nested node binary.
- SHA256SUMS continues to cover every published archive (which now contains the
  node binary when bundled).

---

## 3. Test / build results (real numbers)

| Suite | Result |
|---|---|
| `cargo build` (gui) | clean (pre-existing dead-code warnings only) |
| `cargo test` (gui) | **65 passed, 0 failed** (was 41) |
| `cargo fmt --check` | clean |
| `cargo clippy` (new modules) | no new lints beyond intentional `#![allow(dead_code)]` |
| Python `unittest` (venv + cryptography) | **59 passed, 0 failed** |

New tests added: chain R1 loopback policy (+ spoof/host hardening), node
launch-plan/spec-SHA/name-sanitisation, supervise restart-policy/log-sanitise/
log-ring, child spawn/capture/stop + already-exited, NodeSupervisor
start/stop/double-start/crash-auto-restart.

---

## 4. What remains for a SIGNED public release — and what needs V

### Needs V (decisions / assets)
1. **CHAIN-IDENTITY DRIFT — design decision (blocker for the local node to
   actually sync the real chain).** The wallet's pinned identity in `chain.rs`
   is now **stale vs the live `alice-chain` source**:
   - Wallet pins: chain name `"Alice Mainnet"`, genesis `0x7746…`, `ss58=300`,
     `tokenDecimals=12`, runtime `solochain-template-runtime` **v108**.
   - Live `alice-chain/runtime/src/lib.rs`: runtime **spec_version 110** (≠108).
   - `node/res/alice-mainnet-raw.json` (the "mainnet" spec the node loads):
     name `"Alice Network"`, **`ss58=42`, `tokenDecimals=18`**, **0 bootNodes**.
   - `node/res/alice-mainnet-staging-raw.json` matches the wallet's
     `ss58=300/dec=12` but is named `"Alice Network"` (not `"Alice Mainnet"`),
     also **0 bootNodes**. The `.fixed` staging spec has 1 (localhost) bootnode.
   - **Decision needed:** which spec is canonical for the public wallet, and the
     correct (chain name, genesis hash, ss58, decimals, approved runtime
     version[s]). I deliberately did **not** silently rewrite the pinned
     constants — picking the wrong network would be worse than failing closed.
     Once chosen, update `chain::ALICE_*` constants + approved runtime versions,
     bake `node::pinned_chain_spec_sha256()` and `node::bundled_bootnodes()`.
   - The node binary will **not find peers** until real bootnodes are pinned
     (risk R2). The DMIT HK relay / core nodes are the obvious bootnode source.
2. **Per-OS Alice node binaries to bundle.** There is **no prebuilt
   `solochain-template-node`** anywhere; the source builds, but CI either needs
   to build it per target (slow) or pull hash-pinned prebuilts. Supply them via
   `gui/release-assets/<target>/` or the `ALICE_NODE_BIN_URL` secret. Flag the
   **artifact size** (a Substrate node binary is tens-to-100+ MB per platform).
3. **Code-signing certs** — Apple Developer ID (+ notarization) and Windows
   Authenticode. The pipeline is currently ad-hoc/unsigned (documented posture).
4. **Live mining (optional, separate decision).** Flipping the built-in XMR
   mining from projection-only to **live credit-earning** needs the R3/R4 spikes
   and the vendored Python engine (28-module subset + relocatable Python 3.12).
   Not done here to avoid regressing the audited `MINING_EXECUTION_ALLOWED=false`
   posture. The `supervise/` foundation is ready to host a `MinerSupervisor`.
5. **Final publish** — none performed (branch only, per instructions).

### Engineering follow-ups (no V needed)
- Optional `--status-json` for richer live node/miner state (plan §1.3).
- Restart/expert-flags drawer in the Node UI; confirm-on-quit while syncing.
- Extend the source→binary attestation doc to cover the node + (future) engine
  binary hashes.

---

## 5. Files touched / added

Added: `gui/src/node.rs`, `gui/src/supervise/{mod,child,node_supervisor}.rs`,
`gui/src/ui/node_view.rs`, this doc.
Modified: `gui/src/chain.rs` (R1 + host hardening + `wss_url_is_allowed`),
`gui/src/config.rs` (NodeSettings + `effective_rpc_url`), `gui/src/app.rs`
(supervisor wiring, node actions, `Page::Node`, `on_exit`, effective-RPC
routing), `gui/src/main.rs` (module decls), `gui/src/ui/{shell,mod}.rs`
(Node nav + page), `gui/src/i18n.rs` (node.* EN/ZH), `.github/workflows/release.yml`
(test gate + node/spec bundling seam + Info.plist version fix).
Unchanged (preserved): `gui/src/miner.rs`, `gui/src/ui/mining.rs`, all crypto /
audit-fix code.
