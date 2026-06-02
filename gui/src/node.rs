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

/// Which node the wallet talks to.
///
/// Promotes `chain::NodeSyncMode` to a first-class, user-selectable profile
/// (see plan §1.4 / §2). Switching re-points subxt and starts/stops the
/// embedded node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeMode {
    /// Wallet launches + manages a bundled local node; RPC over loopback `ws://`.
    LocalEmbedded,
    /// Wallet connects to an operator-provided remote node over `wss://`.
    ///
    /// Default: a fresh install is usable immediately while the (large,
    /// slow-to-sync) local node is optional. The headline embedded node is
    /// opt-in via the Node page. (Plan §2: "offer Remote node as fast-start
    /// while local syncs".)
    #[default]
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

/// SHA-256 of the bundled chain spec, pinned at release-build time.
///
/// `None` until the release pipeline bakes it in (Phase 5 of the plan). When
/// set, [`verify_chain_spec_sha256`] fails closed on any mismatch — the same
/// discipline as the genesis-hash pinning in `chain.rs`. Overridable for tests
/// / staging via `ALICE_WALLET_CHAIN_SPEC_SHA256`.
pub fn pinned_chain_spec_sha256() -> Option<&'static str> {
    // Build-baked constant goes here once the canonical spec is chosen (see the
    // chain-identity drift note flagged to V). Until then, an env override lets
    // an operator opt into verification without a rebuild.
    match option_env!("ALICE_WALLET_CHAIN_SPEC_SHA256") {
        Some(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Boot-node multiaddrs bundled for the embedded node's initial peer discovery.
///
/// REQUIRED for a useful sync (plan §2, risk R2): a node with no bootnodes finds
/// no peers. The canonical Alice boot multiaddr(s) are baked here at release
/// time; `ALICE_WALLET_BOOTNODES` (comma-separated) overrides for staging/tests.
/// Returns an empty slice when none are configured — the Node UI surfaces a
/// "no bootnodes — sync will stall" warning in that case.
pub fn bundled_bootnodes() -> Vec<String> {
    if let Ok(v) = std::env::var("ALICE_WALLET_BOOTNODES") {
        return v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    // TODO(release): bake the canonical Alice boot multiaddr(s) here.
    Vec::new()
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
