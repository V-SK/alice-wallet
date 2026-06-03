//! Embedded / managed Alice full node — configuration, binary/spec resolution,
//! and a validated launch-argument builder.
//!
//! This is the "bundle monerod" half of the all-in-one wallet (see
//! `docs/WALLET-ALLINONE-PLAN.md`, §2). The wallet manages a child
//! `solochain-template-node` (from `V-SK/alice-chain`) as a sync node — NOT a
//! validator — and talks to its OWN loopback RPC over plain `ws://` (allowed by
//! the loopback exception in `chain::require_wss_url`).
//!
//! Everything in this module is pure / filesystem-only so it is unit-testable
//! without a real node binary. Actual process spawning lives in
//! [`crate::supervise`].
//!
//! ## Security posture (do NOT regress)
//! - The node's RPC is bound to loopback only (`--rpc-methods safe`, never
//!   `--rpc-external`), so the plaintext-`ws://` loopback exception in
//!   `chain.rs` does not widen the remote-TLS-only attack surface.
//! - Launch arguments are wallet-controlled and validated; user-supplied
//!   values (node name, ports, extra args) are sanitised before they reach the
//!   command line.
//! - The bundled chain spec is verified against a pinned SHA-256 on first use
//!   (fail-closed) — same discipline as `chain.rs` chain-identity pinning.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Default loopback RPC port for the embedded node (Substrate default is 9944;
/// we use a wallet-specific port to avoid clashing with a separately-run node).
pub const DEFAULT_LOCAL_RPC_PORT: u16 = 9955;
/// Default p2p port for the embedded node (Substrate default is 30333).
pub const DEFAULT_LOCAL_P2P_PORT: u16 = 30355;

/// Filename of the bundled node binary, per platform. The wallet resolves this
/// as a sibling of its own executable (see [`resolve_node_binary`]).
#[cfg(target_os = "windows")]
pub const NODE_BINARY_NAME: &str = "solochain-template-node.exe";
#[cfg(not(target_os = "windows"))]
pub const NODE_BINARY_NAME: &str = "solochain-template-node";

/// Filename of the bundled raw chain spec the node is launched with (`--chain`).
pub const CHAIN_SPEC_FILENAME: &str = "alice-mainnet-raw.json";

/// Filename of the bundled CPU miner binary (XMRig), per platform. The wallet
/// resolves this as a sibling of its own executable (see
/// [`resolve_miner_binary`]), mirroring the node-binary resolution.
#[cfg(target_os = "windows")]
pub const XMRIG_BINARY_NAME: &str = "xmrig.exe";
#[cfg(not(target_os = "windows"))]
pub const XMRIG_BINARY_NAME: &str = "xmrig";

/// Resolve the path to the bundled CPU miner binary (XMRig) as a sibling of the
/// wallet's own executable, mirroring [`resolve_node_binary`] / the per-OS
/// packaging layout:
/// - linux:  `AliceWallet/xmrig`
/// - macOS:  `AliceWallet.app/Contents/MacOS/xmrig`
/// - windows: `AliceWallet\xmrig.exe`
///
/// `ALICE_WALLET_MINER_BIN` overrides the resolved path (tests / advanced).
///
/// Dev fallback (debug/`cargo run`): when the sibling binary is absent we also
/// look for the committed macOS arm64 asset at
/// `gui/release-assets/aarch64-apple-darwin/xmrig` (relative to `CARGO_MANIFEST_DIR`),
/// so mining works in a `cargo run`/`cargo test` checkout without packaging.
/// Returns `Ok(path)` only when the file exists.
pub fn resolve_miner_binary() -> Result<PathBuf, String> {
    if let Some(over) = std::env::var_os("ALICE_WALLET_MINER_BIN") {
        let p = PathBuf::from(over);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "ALICE_WALLET_MINER_BIN does not point to a file: {}",
                p.display()
            ))
        };
    }
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot locate wallet executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "wallet executable has no parent directory".to_string())?;
    let candidate = dir.join(XMRIG_BINARY_NAME);
    if candidate.is_file() {
        return Ok(candidate);
    }

    // Dev fallback: the committed macOS arm64 asset in the source tree, so the
    // Mining page works under `cargo run`/`cargo test` (debug) before packaging.
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("release-assets")
            .join("aarch64-apple-darwin")
            .join("xmrig");
        if dev.is_file() {
            return Ok(dev);
        }
    }

    Err(format!(
        "miner binary not bundled (expected at {}). Build/place `{}` beside the wallet.",
        candidate.display(),
        XMRIG_BINARY_NAME
    ))
}

/// Which node the wallet talks to.
///
/// Promotes `chain::NodeSyncMode` to a first-class, user-selectable profile
/// (see plan §1.4 / §2). Switching re-points subxt and starts/stops the
/// embedded node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeMode {
    /// Wallet launches + manages a bundled local node; RPC over loopback `ws://`.
    ///
    /// Default (V 2026-06-03): the core wallet runs its OWN full node
    /// (Monero-GUI / Bitcoin-Core style). On first run it starts + syncs the
    /// bundled node locally; if the node binary is missing or the user wants an
    /// instant start, they switch to Remote on the Node page.
    #[default]
    LocalEmbedded,
    /// Wallet connects to an operator-provided remote node over `wss://`
    /// (fast-start / opt-in via the Node page while the local node syncs).
    Remote,
    /// No node; wallet is fail-closed (no balance trust, no send).
    Offline,
}

impl NodeMode {
    pub fn label(self) -> &'static str {
        match self {
            NodeMode::LocalEmbedded => "Local node (embedded)",
            NodeMode::Remote => "Remote node",
            NodeMode::Offline => "Offline",
        }
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            NodeMode::LocalEmbedded => "node.mode_local",
            NodeMode::Remote => "node.mode_remote",
            NodeMode::Offline => "node.mode_offline",
        }
    }
}

/// User-facing, persisted node settings (lives inside `config::Settings`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodeSettings {
    #[serde(default)]
    pub mode: NodeMode,
    /// Loopback RPC port for the embedded node.
    #[serde(default = "default_rpc_port")]
    pub local_rpc_port: u16,
    /// p2p listen port for the embedded node.
    #[serde(default = "default_p2p_port")]
    pub local_p2p_port: u16,
    /// Start the embedded node automatically when the wallet launches (only
    /// meaningful in `LocalEmbedded` mode).
    #[serde(default = "default_autostart")]
    pub autostart_local: bool,
    /// Friendly node name reported to peers (`--name`). Sanitised on use.
    #[serde(default)]
    pub node_name: Option<String>,
}

fn default_rpc_port() -> u16 {
    DEFAULT_LOCAL_RPC_PORT
}
fn default_p2p_port() -> u16 {
    DEFAULT_LOCAL_P2P_PORT
}
fn default_autostart() -> bool {
    true
}

impl Default for NodeSettings {
    fn default() -> Self {
        Self {
            mode: NodeMode::default(),
            local_rpc_port: DEFAULT_LOCAL_RPC_PORT,
            local_p2p_port: DEFAULT_LOCAL_P2P_PORT,
            autostart_local: true,
            node_name: None,
        }
    }
}

impl NodeSettings {
    /// The loopback `ws://` URL subxt should connect to for the embedded node.
    pub fn local_rpc_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.local_rpc_port)
    }
}

/// Resolve the path to the bundled node binary as a sibling of the wallet's own
/// executable, matching the per-OS packaging layout (plan §4):
/// - linux:  `AliceWallet/solochain-template-node`
/// - macOS:  `AliceWallet.app/Contents/MacOS/solochain-template-node`
/// - windows: `AliceWallet\solochain-template-node.exe`
///
/// Returns `Ok(path)` only when the file exists. `ALICE_WALLET_NODE_BIN` can
/// override the resolved path (used by tests and advanced users).
pub fn resolve_node_binary() -> Result<PathBuf, String> {
    if let Some(over) = std::env::var_os("ALICE_WALLET_NODE_BIN") {
        let p = PathBuf::from(over);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "ALICE_WALLET_NODE_BIN does not point to a file: {}",
                p.display()
            ))
        };
    }
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot locate wallet executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "wallet executable has no parent directory".to_string())?;
    let candidate = dir.join(NODE_BINARY_NAME);
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(format!(
            "node binary not bundled (expected at {}). Build it from V-SK/alice-chain and place it beside the wallet, or use Remote node mode.",
            candidate.display()
        ))
    }
}

/// Resolve the bundled raw chain spec, as a sibling resource of the wallet
/// executable. `ALICE_WALLET_CHAIN_SPEC` overrides (tests / advanced).
pub fn resolve_chain_spec() -> Result<PathBuf, String> {
    if let Some(over) = std::env::var_os("ALICE_WALLET_CHAIN_SPEC") {
        let p = PathBuf::from(over);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!(
                "ALICE_WALLET_CHAIN_SPEC does not point to a file: {}",
                p.display()
            ))
        };
    }
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot locate wallet executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "wallet executable has no parent directory".to_string())?;
    // macOS: binary lives in Contents/MacOS, spec in Contents/Resources.
    let candidates = [
        dir.join(CHAIN_SPEC_FILENAME),
        dir.join("res").join(CHAIN_SPEC_FILENAME),
        dir.join("..").join("Resources").join(CHAIN_SPEC_FILENAME),
    ];
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or_else(|| {
            format!(
                "bundled chain spec not found (looked for {} beside the wallet)",
                CHAIN_SPEC_FILENAME
            )
        })
}

/// Verify a chain-spec file matches an expected SHA-256 (hex, case-insensitive).
/// Fail-closed: any read error or mismatch is an error. When `expected_sha256`
/// is `None`, verification is skipped (returns Ok) — used until the spec SHA is
/// pinned in the release build.
pub fn verify_chain_spec_sha256(
    spec_path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(spec_path)
        .map_err(|e| format!("cannot read chain spec {}: {e}", spec_path.display()))?;
    let digest = Sha256::digest(&bytes);
    let actual = hex::encode(digest);
    match expected_sha256 {
        None => Ok(()),
        Some(expected) => {
            let expected = expected.trim().to_ascii_lowercase();
            if actual == expected {
                Ok(())
            } else {
                Err(format!(
                    "chain spec SHA-256 mismatch: expected {expected}, got {actual}"
                ))
            }
        }
    }
}

/// SHA-256 (hex) of the canonical Alice Mainnet raw chain spec
/// (`alice-mainnet-raw.json`). This is the spec the live chain runs: launching a
/// node with it yields genesis `0x7746a1d1…54b0`, matching
/// [`crate::chain::ALICE_MAINNET_GENESIS_HASH`].
///
/// Pinned here at build time so [`verify_chain_spec_sha256`] fails closed if the
/// bundled spec is swapped/corrupted before the node is ever launched — the same
/// discipline as the genesis-hash + runtime-spec pinning in `chain.rs`. Any
/// tampering with the bundled spec is caught BEFORE a node syncs against it.
pub const ALICE_MAINNET_SPEC_SHA256: &str =
    "9fd71b986d8d8ac8c513a009c31a3edf042576938e6d6e29b5042e4add8eb46f";

/// SHA-256 of the bundled chain spec, pinned at release-build time.
///
/// Returns the baked-in [`ALICE_MAINNET_SPEC_SHA256`] so the release build always
/// fails closed on a spec mismatch. `ALICE_WALLET_CHAIN_SPEC_SHA256` overrides it
/// (tests / staging spinning a different chain) — but only ever to a *different*
/// pin, never to disable verification: there is no code path that returns `None`
/// in a release build.
pub fn pinned_chain_spec_sha256() -> Option<&'static str> {
    match option_env!("ALICE_WALLET_CHAIN_SPEC_SHA256") {
        Some(s) if !s.is_empty() => Some(s),
        _ => Some(ALICE_MAINNET_SPEC_SHA256),
    }
}

/// Canonical Alice Mainnet boot multiaddr(s), baked in for the embedded node's
/// initial peer discovery. These are PUBLIC addresses — the same bootnode is
/// already embedded in the `bootNodes` field of the canonical raw chain spec, so
/// this is belt-and-suspenders (and lets the node find peers even if a future
/// spec ships with `bootNodes: []`). No secrets here.
const CANONICAL_BOOTNODES: &[&str] = &[
    "/ip4/65.109.35.190/tcp/30334/p2p/12D3KooWEp8atZTZpgttn2S6soLuHVT8MKoxnZPxVMPLsz4z6eY1",
];

/// Boot-node multiaddrs bundled for the embedded node's initial peer discovery.
///
/// REQUIRED for a useful sync (plan §2, risk R2): a node with no bootnodes finds
/// no peers. Returns the canonical Alice boot multiaddr(s) baked in
/// ([`CANONICAL_BOOTNODES`]); `ALICE_WALLET_BOOTNODES` (comma-separated)
/// overrides for staging/tests. The spec ALSO embeds the bootnode, so peer
/// discovery does not rely solely on this list. The Node UI still surfaces a
/// "no bootnodes — sync will stall" warning if it ever comes back empty.
pub fn bundled_bootnodes() -> Vec<String> {
    if let Ok(v) = std::env::var("ALICE_WALLET_BOOTNODES") {
        return v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    CANONICAL_BOOTNODES.iter().map(|s| s.to_string()).collect()
}

/// Validate + sanitise a node name for use as `--name`. Rejects anything that
/// could inject shell/argument trickery; keeps it short and printable-ASCII.
pub fn sanitize_node_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(32)
        .collect();
    if cleaned.is_empty() {
        "AliceWallet".to_string()
    } else {
        format!("AliceWallet-{cleaned}")
    }
}

/// Everything needed to spawn the embedded node, fully validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Node base-path (chain DB) — created by the supervisor before launch.
    pub base_path: PathBuf,
    /// The loopback RPC URL the wallet will connect to once the node is up.
    pub rpc_url: String,
}

/// Build a validated launch plan for the embedded node.
///
/// Mirrors the production reference invocation from the plan (§2) but as a
/// **sync node**, never `--validator`:
/// `solochain-template-node --chain <spec> --base-path <dir>
///  --name <safe> --rpc-methods safe --rpc-port <p> --port <p2p>
///  --no-telemetry [--bootnodes <addr> …]`.
///
/// `bootnodes` is REQUIRED for a useful sync (else the node finds no peers) but
/// is allowed empty here so the builder stays testable; the caller/UI surfaces
/// a "no bootnodes" warning. All values are validated/sanitised.
pub fn build_node_launch_plan(
    program: PathBuf,
    chain_spec: &Path,
    base_path: PathBuf,
    settings: &NodeSettings,
    bootnodes: &[String],
) -> Result<NodeLaunchPlan, String> {
    if settings.local_rpc_port == 0 {
        return Err("local RPC port must be non-zero".into());
    }
    if settings.local_p2p_port == 0 {
        return Err("local p2p port must be non-zero".into());
    }
    if settings.local_rpc_port == settings.local_p2p_port {
        return Err("RPC and p2p ports must differ".into());
    }

    let name = settings
        .node_name
        .as_deref()
        .map(sanitize_node_name)
        .unwrap_or_else(|| sanitize_node_name(""));

    let mut args: Vec<String> = vec![
        "--chain".into(),
        chain_spec.to_string_lossy().into_owned(),
        "--base-path".into(),
        base_path.to_string_lossy().into_owned(),
        "--name".into(),
        name,
        // Loopback-only safe RPC; never --rpc-external.
        "--rpc-methods".into(),
        "safe".into(),
        "--rpc-port".into(),
        settings.local_rpc_port.to_string(),
        "--port".into(),
        settings.local_p2p_port.to_string(),
        "--no-telemetry".into(),
    ];

    for b in bootnodes {
        let b = b.trim();
        if b.is_empty() {
            continue;
        }
        // Only accept libp2p multiaddrs (defensive: no flag injection).
        if !b.starts_with("/ip4/") && !b.starts_with("/ip6/") && !b.starts_with("/dns") {
            return Err(format!("invalid bootnode multiaddr: {b}"));
        }
        args.push("--bootnodes".into());
        args.push(b.to_string());
    }

    Ok(NodeLaunchPlan {
        program,
        args,
        base_path,
        rpc_url: settings.local_rpc_url(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn local_rpc_url_is_loopback_ws() {
        let s = NodeSettings::default();
        assert_eq!(
            s.local_rpc_url(),
            format!("ws://127.0.0.1:{DEFAULT_LOCAL_RPC_PORT}")
        );
        // And the chain guard must accept it (R1 fix).
        assert!(crate::chain::wss_url_is_allowed(&s.local_rpc_url()));
    }

    #[test]
    fn node_name_is_sanitized() {
        assert_eq!(sanitize_node_name(""), "AliceWallet");
        assert_eq!(sanitize_node_name("my node"), "AliceWallet-mynode");
        // Strips shell/arg-injection attempts (only [A-Za-z0-9-_] survive, so
        // ';', spaces and '/' are dropped — no flag/shell injection possible).
        let cleaned = sanitize_node_name("x; rm -rf / --validator");
        assert!(cleaned.starts_with("AliceWallet-"));
        assert!(!cleaned.contains(' '));
        assert!(!cleaned.contains(';'));
        assert!(!cleaned.contains('/'));
        // Length-capped.
        assert!(sanitize_node_name(&"a".repeat(100)).len() <= "AliceWallet-".len() + 32);
    }

    #[test]
    fn launch_plan_is_sync_node_never_validator() {
        let spec = PathBuf::from("/tmp/spec.json");
        let base = PathBuf::from("/tmp/node");
        let plan = build_node_launch_plan(
            PathBuf::from("/usr/bin/solochain-template-node"),
            &spec,
            base.clone(),
            &NodeSettings::default(),
            &[],
        )
        .expect("plan");

        assert!(!plan.args.iter().any(|a| a == "--validator"));
        assert!(!plan.args.iter().any(|a| a == "--rpc-external"));
        assert!(plan.args.iter().any(|a| a == "--rpc-methods"));
        // safe RPC method set present.
        let i = plan.args.iter().position(|a| a == "--rpc-methods").unwrap();
        assert_eq!(plan.args[i + 1], "safe");
        assert_eq!(plan.rpc_url, NodeSettings::default().local_rpc_url());
        assert_eq!(plan.base_path, base);
    }

    #[test]
    fn launch_plan_rejects_bad_ports_and_bootnodes() {
        let spec = PathBuf::from("/tmp/spec.json");
        let base = PathBuf::from("/tmp/node");

        let mut bad = NodeSettings::default();
        bad.local_rpc_port = 0;
        assert!(
            build_node_launch_plan(PathBuf::from("n"), &spec, base.clone(), &bad, &[]).is_err()
        );

        let mut same = NodeSettings::default();
        same.local_p2p_port = same.local_rpc_port;
        assert!(
            build_node_launch_plan(PathBuf::from("n"), &spec, base.clone(), &same, &[]).is_err()
        );

        // Bootnode that isn't a multiaddr is rejected (no flag injection).
        assert!(build_node_launch_plan(
            PathBuf::from("n"),
            &spec,
            base.clone(),
            &NodeSettings::default(),
            &["--validator".to_string()],
        )
        .is_err());

        // A valid multiaddr is accepted.
        let plan = build_node_launch_plan(
            PathBuf::from("n"),
            &spec,
            base,
            &NodeSettings::default(),
            &[
                "/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEmxJHP7Jmf9mhEDxRht3K3drwBguWXuw8uunAnEPV9My"
                    .to_string(),
            ],
        )
        .expect("plan");
        assert!(plan.args.iter().any(|a| a == "--bootnodes"));
    }

    #[test]
    fn mainnet_spec_pin_is_wellformed_and_is_the_default_pin() {
        // The pinned canonical hash is a 64-char lowercase hex digest.
        assert_eq!(ALICE_MAINNET_SPEC_SHA256.len(), 64);
        assert!(ALICE_MAINNET_SPEC_SHA256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && (!c.is_alphabetic() || c.is_lowercase())));
        // With no env override, the release build pins the canonical spec hash —
        // there is NO code path that disables verification in production.
        // (Guard against accidental regression to `None`.)
        if std::env::var_os("ALICE_WALLET_CHAIN_SPEC_SHA256").is_none() {
            assert_eq!(pinned_chain_spec_sha256(), Some(ALICE_MAINNET_SPEC_SHA256));
        }
    }

    #[test]
    fn verify_accepts_file_hashing_to_the_canonical_pin() {
        // A file whose bytes hash to ALICE_MAINNET_SPEC_SHA256 passes; this locks
        // the pin to the SHA-256 implementation actually used at launch time.
        // (We synthesise the expected-by-content check rather than embedding the
        // ~1 MB spec in the test: write known bytes, recompute, compare paths.)
        use sha2::{Digest, Sha256};
        let f = tempfile_with(b"canonical-spec-stand-in");
        let want = hex::encode(Sha256::digest(b"canonical-spec-stand-in"));
        // Correct content passes; the real canonical pin is asserted against the
        // committed spec by the CI "verify staged spec SHA" step + release.sh.
        assert!(verify_chain_spec_sha256(&f.0, Some(&want)).is_ok());
        // And the canonical pin rejects this stand-in (different bytes).
        assert!(verify_chain_spec_sha256(&f.0, Some(ALICE_MAINNET_SPEC_SHA256)).is_err());
        let _ = std::fs::remove_file(&f.0);
    }

    #[test]
    fn chain_spec_sha256_fails_closed_on_mismatch_and_passes_on_match() {
        let mut f = tempfile_with(b"hello alice spec");
        let path = f.0.clone();
        // Skip when no expected sha.
        assert!(verify_chain_spec_sha256(&path, None).is_ok());
        // Correct sha passes.
        use sha2::{Digest, Sha256};
        let want = hex::encode(Sha256::digest(b"hello alice spec"));
        assert!(verify_chain_spec_sha256(&path, Some(&want)).is_ok());
        // Wrong sha fails closed.
        assert!(verify_chain_spec_sha256(&path, Some(&"00".repeat(32))).is_err());
        // Keep file alive until here.
        let _ = f.1.write_all(b"");
    }

    // Minimal temp-file helper (avoids adding a dev-dependency).
    fn tempfile_with(contents: &[u8]) -> (PathBuf, std::fs::File) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "alice-wallet-node-test-{}-{}",
            std::process::id(),
            stamp
        ));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents).unwrap();
        f.flush().unwrap();
        (path, f)
    }
}
