use scale_value::{Primitive, Value, ValueDef};
use serde::Deserialize;
use std::str::FromStr;
use subxt::rpcs::{rpc_params, RpcClient};
use subxt::{OnlineClient, PolkadotConfig};

pub const TOKEN_DECIMALS: u32 = 12;
/// Conservative reserve (0.01 ALICE) kept back on a send to cover the network fee +
/// existential-deposit keep-alive headroom, since the wallet does not yet do a live
/// `payment_queryInfo` fee estimate (audit S1). Prevents a "send max" that would pass
/// local review then fail on-chain after consuming the fee.
pub const FEE_ED_MARGIN_PLANCK: u128 = 10_000_000_000;
pub const NODE_SYNC_FRESHNESS_TTL_SECONDS: u64 = 90;
pub const PRODUCTION_TRANSFER_ALLOWED: bool = true;
pub const PAYOUT_ALLOWED: bool = false;
pub const SETTLEMENT_ALLOWED: bool = false;
pub const MINT_ALLOWED: bool = false;
pub const ALICE_MAINNET_CHAIN_NAME: &str = "Alice Mainnet";
pub const ALICE_MAINNET_GENESIS_HASH: &str =
    "0x7746a1d14736a95e00a617a11094b6e86bbf91cd4e7e64c0e748e3c0d2ad54b0";
pub const ALICE_RUNTIME_SPEC_NAME: &str = "solochain-template-runtime";
/// MINIMUM accepted runtime spec version. The chain is forkless-upgradeable (110 -> 111 on
/// 2026-06-16, more to come), and the subxt `OnlineClient` fetches metadata DYNAMICALLY, so a
/// newer runtime is fine — chain identity is pinned by the genesis hash above. An EXACT
/// allowlist (was `[110]`) rejected the chain the moment it upgraded to 111 ("wrong_runtime_spec_version",
/// looked like an RPC outage), and would re-break every future upgrade. Accept `>=` the minimum.
pub const ALICE_MIN_RUNTIME_SPEC_VERSION: u32 = 110;

pub type Client = OnlineClient<PolkadotConfig>;

/// Return `true` when `host` is a loopback host (`127.0.0.1`, `::1`,
/// `localhost`) — i.e. traffic that never leaves the local machine.
///
/// `host` is the authority portion of a URL (`host[:port]`); the optional port
/// and any IPv6 brackets are stripped before comparison.
fn host_is_loopback(host: &str) -> bool {
    // Strip credentials if somehow present ("user:pass@host" is not expected
    // for node URLs, but be defensive).
    let host = host.rsplit('@').next().unwrap_or(host);

    // IPv6 literal: "[::1]:9944" / "[::1]".
    if let Some(rest) = host.strip_prefix('[') {
        let inner = rest.split(']').next().unwrap_or("");
        return matches!(inner, "::1" | "0:0:0:0:0:0:0:1");
    }

    // IPv4 / hostname: split off ":port".
    let hostname = host.split(':').next().unwrap_or(host);
    if hostname == "localhost" {
        return true;
    }
    // The entire 127.0.0.0/8 block is loopback, but ONLY when `hostname` is a
    // real dotted-quad IPv4 literal — a hostname like "127.0.0.1.evil.example"
    // must NOT be treated as loopback.
    let octets: Vec<&str> = hostname.split('.').collect();
    if octets.len() == 4
        && octets
            .iter()
            .all(|o| !o.is_empty() && o.len() <= 3 && o.chars().all(|c| c.is_ascii_digit()))
    {
        if let Ok(first) = octets[0].parse::<u8>() {
            // Confirm the remaining octets are valid u8 too (well-formed IPv4).
            let well_formed = octets[1..].iter().all(|o| o.parse::<u8>().is_ok());
            return well_formed && first == 127;
        }
    }
    false
}

/// Enforce the wallet's transport-security policy on a node URL.
///
/// A wallet that signs transactions requires transport encryption on every
/// connection that leaves the machine, so **remote** endpoints must use
/// `wss://`. We reject plain `ws://`, `http://` and `https://` to a remote
/// host before we ever attempt to connect.
///
/// The one carefully-scoped exception is the wallet's **own embedded local
/// node**: a bundled `solochain-template-node` serves plaintext
/// `ws://127.0.0.1:<rpc>` on loopback only (its RPC is never bound to an
/// external interface — `--rpc-methods safe`, no `--rpc-external`). Traffic to
/// `127.0.0.1` / `::1` / `localhost` never crosses the network, so a TLS
/// requirement there would add no security while making the embedded-node
/// feature impossible. We therefore allow plaintext `ws://` for loopback hosts
/// ONLY, and continue to require `wss://` for everything else.
///
/// Note: subxt's own `from_url` helper applies a similar localhost exception,
/// but only for `ws://`; we keep our own guard so the policy is explicit and
/// independently tested (remote `ws://` is rejected; loopback `ws://` allowed;
/// `http(s)://` always rejected).
fn require_wss_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return Err(
            "Node URL must use wss:// (encrypted WebSocket); insecure schemes are rejected".into(),
        );
    };

    if scheme.eq_ignore_ascii_case("wss") {
        return Ok(());
    }

    // Loopback-only exception for the wallet's own embedded node over plain ws.
    if scheme.eq_ignore_ascii_case("ws") {
        // Authority is everything up to the first '/', '?' or '#'.
        let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
        if host_is_loopback(authority) {
            return Ok(());
        }
    }

    Err("Node URL must use wss:// (encrypted WebSocket); plaintext ws:// is allowed only for a local loopback node".into())
}

/// Public predicate form of [`require_wss_url`] for callers that need to gate
/// UI/availability on transport policy without attempting a connection
/// (e.g. the embedded-node module validating its own loopback URL).
pub fn wss_url_is_allowed(url: &str) -> bool {
    require_wss_url(url).is_ok()
}

pub async fn get_client(url: &str) -> Result<Client, String> {
    require_wss_url(url)?;
    // `from_url` (not `from_insecure_url`) performs subxt's own TLS/secure-URL
    // validation in addition to our wss-only guard above.
    OnlineClient::<PolkadotConfig>::from_url(url)
        .await
        .map_err(|e| format!("Failed to connect to node: {:?}", e))
}

pub fn validate_address(address: &str) -> Result<(), String> {
    subxt::utils::AccountId32::from_str(address.trim())
        .map(|_| ())
        .map_err(|_| "Invalid address".into())
}

pub fn parse_token_amount(input: &str, decimals: u32) -> Result<u128, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Amount is required".into());
    }
    if trimmed.starts_with('-') {
        return Err("Amount must be positive".into());
    }

    let normalized = trimmed.replace('_', "");
    let parts: Vec<&str> = normalized.split('.').collect();
    if parts.len() > 2 {
        return Err("Amount has too many decimal points".into());
    }

    let whole = parts[0];
    let fractional = parts.get(1).copied().unwrap_or("");
    if whole.is_empty() && fractional.is_empty() {
        return Err("Amount is required".into());
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fractional.chars().all(|c| c.is_ascii_digit())
    {
        return Err("Amount must be a decimal number".into());
    }
    if fractional.len() > decimals as usize {
        return Err(format!(
            "Amount supports at most {} decimal places",
            decimals
        ));
    }

    let multiplier = 10u128
        .checked_pow(decimals)
        .ok_or_else(|| "Unsupported token decimals".to_string())?;
    let whole_units = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<u128>()
            .map_err(|_| "Amount is too large".to_string())?
    };
    let fractional_units = if fractional.is_empty() {
        0
    } else {
        let padded = format!("{:0<width$}", fractional, width = decimals as usize);
        padded
            .parse::<u128>()
            .map_err(|_| "Amount is too large".to_string())?
    };

    let amount = whole_units
        .checked_mul(multiplier)
        .and_then(|value| value.checked_add(fractional_units))
        .ok_or_else(|| "Amount is too large".to_string())?;

    if amount == 0 {
        return Err("Amount must be greater than zero".into());
    }

    Ok(amount)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSyncMode {
    LocalNode,
    RemoteNode,
    Unavailable,
}

impl NodeSyncMode {
    pub fn label(self) -> &'static str {
        match self {
            NodeSyncMode::LocalNode => "Local node",
            NodeSyncMode::RemoteNode => "Remote node",
            NodeSyncMode::Unavailable => "Node unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSyncState {
    Synced,
    Syncing,
    Stale,
    Offline,
    Unavailable,
    Error,
}

impl NodeSyncState {
    pub fn i18n_key(self) -> &'static str {
        match self {
            NodeSyncState::Synced => "sync.state_synced",
            NodeSyncState::Syncing => "sync.state_syncing",
            NodeSyncState::Stale => "sync.state_stale",
            NodeSyncState::Offline => "sync.state_offline",
            NodeSyncState::Unavailable | NodeSyncState::Error => "sync.state_connecting",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeSyncEvidence {
    pub sync_mode: NodeSyncMode,
    pub connected: bool,
    pub chain_identity: Option<ChainIdentityEvidence>,
    pub current_height: Option<u64>,
    pub target_height: Option<u64>,
    pub peers_count: Option<u32>,
    pub last_updated_unix: Option<i64>,
    pub observed_at_unix: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainIdentityEvidence {
    pub chain_name: String,
    pub genesis_hash: String,
    pub runtime_spec_name: String,
    pub runtime_spec_version: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeSyncSnapshot {
    pub sync_mode: NodeSyncMode,
    pub status: NodeSyncState,
    pub current_height: Option<u64>,
    pub target_height: Option<u64>,
    pub remaining_blocks: Option<u64>,
    pub progress_percent: Option<f32>,
    pub peers_count: Option<u32>,
    pub network_status: String,
    pub last_updated_at: Option<String>,
    pub freshness_seconds: Option<u64>,
    pub fail_closed_reason: Option<String>,
}

impl NodeSyncSnapshot {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            sync_mode: NodeSyncMode::Unavailable,
            status: NodeSyncState::Unavailable,
            current_height: None,
            target_height: None,
            remaining_blocks: None,
            progress_percent: None,
            peers_count: None,
            network_status: "not connected".into(),
            last_updated_at: None,
            freshness_seconds: None,
            fail_closed_reason: Some(reason.into()),
        }
    }

    pub fn status_i18n_key(&self) -> &'static str {
        self.status.i18n_key()
    }

    pub fn allows_balance_refresh(&self) -> bool {
        matches!(self.status, NodeSyncState::Synced | NodeSyncState::Syncing)
            && self.fail_closed_reason.is_none()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemSyncState {
    current_block: Option<u64>,
    highest_block: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemHealth {
    peers: Option<u32>,
    is_syncing: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeVersion {
    spec_name: String,
    spec_version: u32,
}

pub fn sync_mode_from_url(url: &str) -> NodeSyncMode {
    let trimmed = url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return NodeSyncMode::Unavailable;
    };
    if !(scheme.eq_ignore_ascii_case("ws") || scheme.eq_ignore_ascii_case("wss")) {
        return NodeSyncMode::Unavailable;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if host_is_loopback(authority) {
        NodeSyncMode::LocalNode
    } else {
        NodeSyncMode::RemoteNode
    }
}

fn format_unix(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unavailable".into())
}

pub fn evaluate_node_sync(evidence: NodeSyncEvidence) -> NodeSyncSnapshot {
    if let Some(error) = evidence.error {
        return NodeSyncSnapshot {
            sync_mode: evidence.sync_mode,
            status: NodeSyncState::Error,
            current_height: evidence.current_height,
            target_height: evidence.target_height,
            remaining_blocks: None,
            progress_percent: None,
            peers_count: evidence.peers_count,
            network_status: "error".into(),
            last_updated_at: evidence.last_updated_unix.map(format_unix),
            freshness_seconds: None,
            fail_closed_reason: Some(error),
        };
    }

    if !evidence.connected {
        return NodeSyncSnapshot {
            sync_mode: evidence.sync_mode,
            status: NodeSyncState::Offline,
            current_height: evidence.current_height,
            target_height: evidence.target_height,
            remaining_blocks: None,
            progress_percent: None,
            peers_count: evidence.peers_count,
            network_status: "offline".into(),
            last_updated_at: evidence.last_updated_unix.map(format_unix),
            freshness_seconds: None,
            fail_closed_reason: Some("node_offline".into()),
        };
    }

    let Some(last_updated) = evidence.last_updated_unix else {
        return fail_closed(evidence, NodeSyncState::Unavailable, "missing_freshness");
    };
    let freshness = evidence
        .observed_at_unix
        .saturating_sub(last_updated)
        .max(0) as u64;
    if freshness > NODE_SYNC_FRESHNESS_TTL_SECONDS {
        return fail_closed(evidence, NodeSyncState::Stale, "stale_node_evidence");
    }

    if let Err(reason) = validate_chain_identity(evidence.chain_identity.as_ref()) {
        return fail_closed(evidence, NodeSyncState::Unavailable, reason);
    }

    let Some(current) = evidence.current_height else {
        return fail_closed(
            evidence,
            NodeSyncState::Unavailable,
            "missing_current_height",
        );
    };
    let Some(target) = evidence.target_height else {
        return fail_closed(
            evidence,
            NodeSyncState::Unavailable,
            "missing_target_height",
        );
    };

    let remaining = target.saturating_sub(current);
    let progress = if target == 0 {
        Some(100.0)
    } else {
        Some(((current.min(target) as f64 / target as f64) * 100.0) as f32)
    };
    let network_status = match evidence.peers_count {
        Some(0) => "no peers",
        Some(_) => "connected",
        None => "peer status unavailable",
    }
    .to_string();
    let status = if remaining == 0 {
        NodeSyncState::Synced
    } else {
        NodeSyncState::Syncing
    };

    NodeSyncSnapshot {
        sync_mode: evidence.sync_mode,
        status,
        current_height: Some(current),
        target_height: Some(target),
        remaining_blocks: Some(remaining),
        progress_percent: progress,
        peers_count: evidence.peers_count,
        network_status,
        last_updated_at: Some(format_unix(last_updated)),
        freshness_seconds: Some(freshness),
        fail_closed_reason: None,
    }
}

fn validate_chain_identity(identity: Option<&ChainIdentityEvidence>) -> Result<(), &'static str> {
    let Some(identity) = identity else {
        return Err("missing_chain_identity");
    };
    if identity.chain_name.trim() != ALICE_MAINNET_CHAIN_NAME {
        return Err("wrong_chain_name");
    }
    if !identity
        .genesis_hash
        .trim()
        .eq_ignore_ascii_case(ALICE_MAINNET_GENESIS_HASH)
    {
        return Err("wrong_genesis_hash");
    }
    if identity.runtime_spec_name.trim() != ALICE_RUNTIME_SPEC_NAME {
        return Err("wrong_runtime_spec_name");
    }
    if identity.runtime_spec_version < ALICE_MIN_RUNTIME_SPEC_VERSION {
        return Err("wrong_runtime_spec_version");
    }
    Ok(())
}

fn fail_closed(
    evidence: NodeSyncEvidence,
    status: NodeSyncState,
    reason: impl Into<String>,
) -> NodeSyncSnapshot {
    let reason = reason.into();
    let last_updated_at = evidence.last_updated_unix.map(format_unix);
    let freshness_seconds = evidence
        .last_updated_unix
        .map(|ts| evidence.observed_at_unix.saturating_sub(ts).max(0) as u64);
    NodeSyncSnapshot {
        sync_mode: evidence.sync_mode,
        status,
        current_height: evidence.current_height,
        target_height: evidence.target_height,
        remaining_blocks: None,
        progress_percent: None,
        peers_count: evidence.peers_count,
        network_status: "not ready".into(),
        last_updated_at,
        freshness_seconds,
        fail_closed_reason: Some(reason),
    }
}

pub async fn fetch_node_sync_snapshot(rpc_url: &str) -> NodeSyncSnapshot {
    let sync_mode = sync_mode_from_url(rpc_url);
    let observed_at_unix = chrono::Utc::now().timestamp();
    if let Err(reason) = require_wss_url(rpc_url) {
        return evaluate_node_sync(NodeSyncEvidence {
            sync_mode,
            connected: false,
            chain_identity: None,
            current_height: None,
            target_height: None,
            peers_count: None,
            last_updated_unix: None,
            observed_at_unix,
            error: Some(format!("insecure_url_rejected: {}", reason)),
        });
    }
    // `from_url` (not `from_insecure_url`) validates the URL is TLS-protected
    // before opening the connection, matching our wss-only guard above.
    let rpc = match RpcClient::from_url(rpc_url).await {
        Ok(rpc) => rpc,
        Err(e) => {
            return evaluate_node_sync(NodeSyncEvidence {
                sync_mode,
                connected: false,
                chain_identity: None,
                current_height: None,
                target_height: None,
                peers_count: None,
                last_updated_unix: None,
                observed_at_unix,
                error: Some(format!("connection_failed: {}", e)),
            });
        }
    };

    let sync_state: Result<SystemSyncState, _> =
        rpc.request("system_syncState", rpc_params![]).await;
    let health: Result<SystemHealth, _> = rpc.request("system_health", rpc_params![]).await;
    let chain_name: Result<String, _> = rpc.request("system_chain", rpc_params![]).await;
    let genesis_hash: Result<String, _> = rpc.request("chain_getBlockHash", rpc_params![0]).await;
    let runtime_version: Result<RuntimeVersion, _> =
        rpc.request("state_getRuntimeVersion", rpc_params![]).await;
    let chain_identity = match (chain_name, genesis_hash, runtime_version) {
        (Ok(chain_name), Ok(genesis_hash), Ok(runtime_version)) => Some(ChainIdentityEvidence {
            chain_name,
            genesis_hash,
            runtime_spec_name: runtime_version.spec_name,
            runtime_spec_version: runtime_version.spec_version,
        }),
        _ => None,
    };

    match sync_state {
        Ok(sync) => {
            let peers = health.as_ref().ok().and_then(|h| h.peers);
            let is_connected = health
                .as_ref()
                .ok()
                .map(|h| h.peers.unwrap_or(0) > 0 || h.is_syncing.unwrap_or(false))
                .unwrap_or(true);
            evaluate_node_sync(NodeSyncEvidence {
                sync_mode,
                connected: is_connected,
                chain_identity,
                current_height: sync.current_block,
                target_height: sync.highest_block,
                peers_count: peers,
                last_updated_unix: Some(observed_at_unix),
                observed_at_unix,
                error: None,
            })
        }
        Err(e) => evaluate_node_sync(NodeSyncEvidence {
            sync_mode,
            connected: true,
            chain_identity,
            current_height: None,
            target_height: None,
            peers_count: health.ok().and_then(|h| h.peers),
            last_updated_unix: Some(observed_at_unix),
            observed_at_unix,
            error: Some(format!("sync_state_unavailable: {}", e)),
        }),
    }
}

pub async fn get_balance(client: &Client, address: &str) -> Result<u128, String> {
    let account_id =
        subxt::utils::AccountId32::from_str(address.trim()).map_err(|_| "Invalid address")?;

    let storage_query = subxt::storage::dynamic("System", "Account");

    let at_block = client.at_current_block().await.map_err(|e| e.to_string())?;
    let result = at_block
        .storage()
        .try_fetch(storage_query, (Value::from_bytes(account_id.0),))
        .await
        .map_err(|e| e.to_string())?;

    if let Some(data) = result {
        let value: Value = data.decode().map_err(|e| e.to_string())?;

        if let ValueDef::Composite(c) = value.value {
            let values = match c {
                scale_value::Composite::Named(v) => v,
                scale_value::Composite::Unnamed(_) => return Ok(0),
            };

            for (key, val) in values {
                if key == "data" {
                    if let ValueDef::Composite(datac) = &val.value {
                        let data_values = match datac {
                            scale_value::Composite::Named(v) => v,
                            scale_value::Composite::Unnamed(_) => continue,
                        };

                        for (dk, dv) in data_values {
                            if dk == "free" {
                                if let ValueDef::Primitive(Primitive::U128(b)) = dv.value {
                                    return Ok(b);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(0)
}

/// Build, sign, and submit a `Balances.transfer_keep_alive` extrinsic and wait for
/// FINALIZED success; returns the extrinsic hash (`0x…`).
///
/// Uses the dynamic API (mirrors `get_balance`) so it stays metadata-driven and never pins
/// a call index — the exact call is resolved against the live spec-111 metadata at submit
/// time. `transfer_keep_alive` (NOT `transfer_allow_death`) so a send can never reap the
/// sender below the existential deposit. The `signer` is derived from the UNLOCKED wallet
/// seed (`crypto::WalletSecrets::to_keypair`); a display-only / locked wallet has no seed
/// and cannot reach here. `amount_planck` is the integer base-unit amount (the UI parses
/// the human amount via `parse_token_amount`).
pub async fn submit_transfer(
    client: &Client,
    signer: &subxt_signer::sr25519::Keypair,
    dest_address: &str,
    amount_planck: u128,
) -> Result<String, String> {
    // --- PRECHECK phase: errors here mean NOTHING was broadcast -> safe to retry.
    // The caller distinguishes "PRECHECK:" (retry-safe) from "PENDING:" (broadcast may
    // have occurred -> do NOT auto-retry, double-spend risk) by the error prefix.
    let dest = subxt::utils::AccountId32::from_str(dest_address.trim())
        .map_err(|_| "PRECHECK: Invalid recipient address".to_string())?;
    if amount_planck == 0 {
        return Err("PRECHECK: Amount must be greater than zero".to_string());
    }
    // B1 defense-in-depth: never sign+broadcast against a node that is not OUR chain.
    // Genesis hash is the immutable chain identity; catches a stale/wrong/malicious endpoint.
    let genesis = format!("{:?}", client.genesis_hash());
    if genesis != ALICE_MAINNET_GENESIS_HASH {
        return Err(format!(
            "PRECHECK: refusing to sign — connected node genesis {genesis} is not Alice mainnet"
        ));
    }
    // dest: MultiAddress::Id(AccountId32); value: Compact<u128> (encoded per live metadata).
    let call = subxt::dynamic::tx(
        "Balances",
        "transfer_keep_alive",
        vec![
            Value::unnamed_variant("Id", [Value::from_bytes(dest.0)]),
            Value::u128(amount_planck),
        ],
    );
    let mut tx_client = client
        .tx()
        .await
        .map_err(|e| format!("PRECHECK: tx client unavailable: {e}"))?;
    // --- BROADCAST phase: from here the extrinsic may be on the wire. Any failure is
    // PENDING (the transfer might still finalize), NOT a clean retry.
    let events = tx_client
        .sign_and_submit_then_watch_default(&call, signer)
        .await
        .map_err(|e| format!("PENDING: broadcast may have occurred: {e}"))?
        .wait_for_finalized_success()
        .await
        .map_err(|e| format!("PENDING: broadcast but not confirmed: {e}"))?;
    Ok(format!("{:?}", events.extrinsic_hash()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MANUAL live smoke (gated by env `ALICE_LIVE_SEND`; inert in normal `cargo test`).
    /// Builds a test wallet from `ALICE_TEST_SEED_HEX` via the REAL wallet crypto (so its
    /// address + signer are exactly what ships) and prints its address. With `ALICE_DO_SEND`
    /// set it submits a real `Balances.transfer_keep_alive` to `ALICE_TEST_DEST` for
    /// `ALICE_AMT` planck via the EXACT `submit_transfer` the GUI uses, and prints the hash.
    #[test]
    fn live_send_smoke() {
        if std::env::var("ALICE_LIVE_SEND").is_err() {
            return;
        }
        let seed_hex = std::env::var("ALICE_TEST_SEED_HEX").expect("ALICE_TEST_SEED_HEX");
        let payload = crate::crypto::create_wallet_payload_from_seed_hex(&seed_hex, "smoke-pw")
            .expect("create payload");
        let secrets = crate::crypto::unlock_wallet(&payload, "smoke-pw")
            .expect("unlock")
            .secrets;
        eprintln!("TEST_ADDRESS={}", secrets.address);
        if std::env::var("ALICE_DO_SEND").is_err() {
            return;
        }
        let dest = std::env::var("ALICE_TEST_DEST").expect("ALICE_TEST_DEST");
        let amount: u128 = std::env::var("ALICE_AMT")
            .expect("ALICE_AMT")
            .parse()
            .expect("amount");
        let signer = secrets.to_keypair().expect("keypair");
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        rt.block_on(async {
            let client = get_client("wss://rpc.aliceprotocol.org")
                .await
                .expect("client");
            let hash = submit_transfer(&client, &signer, &dest, amount)
                .await
                .expect("submit_transfer");
            eprintln!("TRANSFER_HASH={hash}");
        });
    }

    fn valid_chain_identity() -> ChainIdentityEvidence {
        ChainIdentityEvidence {
            chain_name: ALICE_MAINNET_CHAIN_NAME.to_string(),
            genesis_hash: ALICE_MAINNET_GENESIS_HASH.to_string(),
            runtime_spec_name: ALICE_RUNTIME_SPEC_NAME.to_string(),
            runtime_spec_version: ALICE_MIN_RUNTIME_SPEC_VERSION,
        }
    }

    #[test]
    fn parses_whole_and_fractional_amounts() {
        assert_eq!(
            parse_token_amount("1.25", TOKEN_DECIMALS).unwrap(),
            1_250_000_000_000
        );
        assert_eq!(
            parse_token_amount("0.000000000001", TOKEN_DECIMALS).unwrap(),
            1
        );
    }

    #[test]
    fn rejects_invalid_amounts() {
        assert!(parse_token_amount("", TOKEN_DECIMALS).is_err());
        assert!(parse_token_amount("0", TOKEN_DECIMALS).is_err());
        assert!(parse_token_amount("1.0000000000001", TOKEN_DECIMALS).is_err());
        assert!(parse_token_amount("1e3", TOKEN_DECIMALS).is_err());
    }

    #[test]
    fn transfer_enabled_but_payout_settlement_mint_stay_off() {
        // v0.1.3: user-initiated transfers are ON (transfer_keep_alive); the
        // protocol-economic execution flags must remain OFF (credit-only era).
        assert!(PRODUCTION_TRANSFER_ALLOWED);
        assert!(!PAYOUT_ALLOWED);
        assert!(!SETTLEMENT_ALLOWED);
        assert!(!MINT_ALLOWED);
    }

    #[test]
    fn require_wss_url_accepts_wss_anywhere() {
        assert!(require_wss_url("wss://rpc.aliceprotocol.org").is_ok());
        assert!(require_wss_url("WSS://rpc.aliceprotocol.org").is_ok());
        assert!(require_wss_url("  wss://rpc.aliceprotocol.org  ").is_ok());
        assert!(require_wss_url("wss://127.0.0.1:9944").is_ok());
    }

    #[test]
    fn require_wss_url_allows_plaintext_ws_only_on_loopback() {
        // R1 fix: the wallet's own embedded local node serves ws:// on loopback.
        assert!(require_wss_url("ws://127.0.0.1:9944").is_ok());
        assert!(require_wss_url("ws://127.0.0.1").is_ok());
        assert!(require_wss_url("ws://localhost:9944").is_ok());
        assert!(require_wss_url("WS://localhost:9944").is_ok());
        assert!(require_wss_url("ws://[::1]:9944").is_ok());
        assert!(require_wss_url("ws://127.0.0.1:9944/").is_ok());
        // Whole 127.0.0.0/8 loopback block.
        assert!(require_wss_url("ws://127.1.2.3:9944").is_ok());
    }

    #[test]
    fn require_wss_url_rejects_plaintext_ws_to_remote_hosts() {
        // Remote endpoints MUST still be TLS-protected — the audit invariant.
        assert!(require_wss_url("ws://rpc.aliceprotocol.org").is_err());
        assert!(require_wss_url("ws://rpc.aliceprotocol.org:9944").is_err());
        // A remote host that merely embeds "localhost"/"127.0.0.1" as a
        // substring must NOT slip through (defeats naive contains-checks).
        assert!(require_wss_url("ws://localhost.evil.example").is_err());
        assert!(require_wss_url("ws://127.0.0.1.evil.example:9944").is_err());
        assert!(require_wss_url("ws://10.0.0.5:9944").is_err());
        assert!(require_wss_url("ws://[2001:db8::1]:9944").is_err());
    }

    #[test]
    fn require_wss_url_rejects_non_ws_schemes_everywhere() {
        assert!(require_wss_url("http://rpc.aliceprotocol.org").is_err());
        assert!(require_wss_url("https://rpc.aliceprotocol.org").is_err());
        // Even on loopback, http(s) is rejected — only ws/wss are node transports.
        assert!(require_wss_url("http://127.0.0.1:9944").is_err());
        assert!(require_wss_url("https://localhost:9944").is_err());
        assert!(require_wss_url("rpc.aliceprotocol.org").is_err());
        assert!(require_wss_url("").is_err());
    }

    #[test]
    fn sync_mode_classifies_loopback_vs_remote_by_host_not_substring() {
        assert_eq!(
            sync_mode_from_url("ws://127.0.0.1:9944"),
            NodeSyncMode::LocalNode
        );
        assert_eq!(
            sync_mode_from_url("ws://localhost:9944"),
            NodeSyncMode::LocalNode
        );
        assert_eq!(
            sync_mode_from_url("ws://[::1]:9944"),
            NodeSyncMode::LocalNode
        );
        assert_eq!(
            sync_mode_from_url("wss://rpc.aliceprotocol.org"),
            NodeSyncMode::RemoteNode
        );
        // Substring spoofing must classify as Remote, not Local.
        assert_eq!(
            sync_mode_from_url("ws://127.0.0.1.evil.example:9944"),
            NodeSyncMode::RemoteNode
        );
        assert_eq!(
            sync_mode_from_url("wss://localhost.evil.example"),
            NodeSyncMode::RemoteNode
        );
        assert_eq!(
            sync_mode_from_url("http://127.0.0.1"),
            NodeSyncMode::Unavailable
        );
        assert_eq!(sync_mode_from_url("garbage"), NodeSyncMode::Unavailable);
    }

    #[test]
    fn node_sync_missing_target_fails_closed() {
        let snapshot = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: Some(valid_chain_identity()),
            current_height: Some(100),
            target_height: None,
            peers_count: Some(3),
            last_updated_unix: Some(1000),
            observed_at_unix: 1001,
            error: None,
        });
        assert_eq!(snapshot.status, NodeSyncState::Unavailable);
        assert_eq!(
            snapshot.fail_closed_reason.as_deref(),
            Some("missing_target_height")
        );
        assert!(snapshot.progress_percent.is_none());
    }

    #[test]
    fn node_sync_stale_fails_closed() {
        let snapshot = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: None,
            current_height: Some(100),
            target_height: Some(100),
            peers_count: Some(3),
            last_updated_unix: Some(1000),
            observed_at_unix: 1200,
            error: None,
        });
        assert_eq!(snapshot.status, NodeSyncState::Stale);
        assert_eq!(
            snapshot.fail_closed_reason.as_deref(),
            Some("stale_node_evidence")
        );
    }

    #[test]
    fn node_sync_synced_and_syncing_are_explicit() {
        let synced = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::LocalNode,
            connected: true,
            chain_identity: Some(valid_chain_identity()),
            current_height: Some(100),
            target_height: Some(100),
            peers_count: Some(2),
            last_updated_unix: Some(1000),
            observed_at_unix: 1003,
            error: None,
        });
        assert_eq!(synced.status, NodeSyncState::Synced);
        assert_eq!(synced.remaining_blocks, Some(0));
        assert_eq!(synced.fail_closed_reason, None);

        let syncing = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: Some(valid_chain_identity()),
            current_height: Some(90),
            target_height: Some(100),
            peers_count: Some(1),
            last_updated_unix: Some(1000),
            observed_at_unix: 1005,
            error: None,
        });
        assert_eq!(syncing.status, NodeSyncState::Syncing);
        assert_eq!(syncing.remaining_blocks, Some(10));
        assert!(syncing.progress_percent.unwrap() < 100.0);
    }

    #[test]
    fn node_sync_missing_or_wrong_chain_identity_fails_closed() {
        let missing = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: None,
            current_height: Some(100),
            target_height: Some(100),
            peers_count: Some(2),
            last_updated_unix: Some(1000),
            observed_at_unix: 1001,
            error: None,
        });
        assert_eq!(missing.status, NodeSyncState::Unavailable);
        assert_eq!(
            missing.fail_closed_reason.as_deref(),
            Some("missing_chain_identity")
        );
        assert!(!missing.allows_balance_refresh());

        let mut wrong_genesis = valid_chain_identity();
        wrong_genesis.genesis_hash =
            "0x0000000000000000000000000000000000000000000000000000000000000000".into();
        let snapshot = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: Some(wrong_genesis),
            current_height: Some(100),
            target_height: Some(100),
            peers_count: Some(2),
            last_updated_unix: Some(1000),
            observed_at_unix: 1001,
            error: None,
        });
        assert_eq!(snapshot.status, NodeSyncState::Unavailable);
        assert_eq!(
            snapshot.fail_closed_reason.as_deref(),
            Some("wrong_genesis_hash")
        );
        assert!(!snapshot.allows_balance_refresh());
    }

    #[test]
    fn node_sync_wrong_runtime_spec_fails_closed() {
        let mut wrong_spec_name = valid_chain_identity();
        wrong_spec_name.runtime_spec_name = "not-alice".into();
        let snapshot = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: Some(wrong_spec_name),
            current_height: Some(100),
            target_height: Some(100),
            peers_count: Some(2),
            last_updated_unix: Some(1000),
            observed_at_unix: 1001,
            error: None,
        });
        assert_eq!(
            snapshot.fail_closed_reason.as_deref(),
            Some("wrong_runtime_spec_name")
        );
        assert!(!snapshot.allows_balance_refresh());

        let mut wrong_spec_version = valid_chain_identity();
        wrong_spec_version.runtime_spec_version = 109;
        let snapshot = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: Some(wrong_spec_version),
            current_height: Some(100),
            target_height: Some(100),
            peers_count: Some(2),
            last_updated_unix: Some(1000),
            observed_at_unix: 1001,
            error: None,
        });
        assert_eq!(
            snapshot.fail_closed_reason.as_deref(),
            Some("wrong_runtime_spec_version")
        );
        assert!(!snapshot.allows_balance_refresh());
    }

    #[test]
    fn node_sync_offline_and_error_are_not_ready() {
        let offline = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: false,
            chain_identity: None,
            current_height: Some(100),
            target_height: Some(100),
            peers_count: Some(0),
            last_updated_unix: Some(1000),
            observed_at_unix: 1001,
            error: None,
        });
        assert_eq!(offline.status, NodeSyncState::Offline);
        assert_eq!(offline.fail_closed_reason.as_deref(), Some("node_offline"));

        let err = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
            chain_identity: None,
            current_height: None,
            target_height: None,
            peers_count: None,
            last_updated_unix: None,
            observed_at_unix: 1001,
            error: Some("rpc_error".into()),
        });
        assert_eq!(err.status, NodeSyncState::Error);
        assert_eq!(err.fail_closed_reason.as_deref(), Some("rpc_error"));
    }
}
