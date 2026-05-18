use scale_value::{Primitive, Value, ValueDef};
use serde::Deserialize;
use std::str::FromStr;
use subxt::rpcs::{rpc_params, RpcClient};
use subxt::{OnlineClient, PolkadotConfig};

pub const TOKEN_DECIMALS: u32 = 12;
pub const NODE_SYNC_FRESHNESS_TTL_SECONDS: u64 = 90;
pub const PRODUCTION_TRANSFER_ALLOWED: bool = false;
pub const PAYOUT_ALLOWED: bool = false;
pub const SETTLEMENT_ALLOWED: bool = false;
pub const MINT_ALLOWED: bool = false;

pub type Client = OnlineClient<PolkadotConfig>;

pub async fn get_client(url: &str) -> Result<Client, String> {
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
    pub current_height: Option<u64>,
    pub target_height: Option<u64>,
    pub peers_count: Option<u32>,
    pub last_updated_unix: Option<i64>,
    pub observed_at_unix: i64,
    pub error: Option<String>,
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

pub fn sync_mode_from_url(url: &str) -> NodeSyncMode {
    let lower = url.to_ascii_lowercase();
    if lower.contains("127.0.0.1") || lower.contains("localhost") || lower.contains("[::1]") {
        NodeSyncMode::LocalNode
    } else if lower.starts_with("ws://") || lower.starts_with("wss://") {
        NodeSyncMode::RemoteNode
    } else {
        NodeSyncMode::Unavailable
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
    let rpc = match RpcClient::from_insecure_url(rpc_url).await {
        Ok(rpc) => rpc,
        Err(e) => {
            return evaluate_node_sync(NodeSyncEvidence {
                sync_mode,
                connected: false,
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
        .try_fetch(storage_query, (Value::from_bytes(&account_id.0),))
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn production_execution_flags_remain_false() {
        assert!(!PRODUCTION_TRANSFER_ALLOWED);
        assert!(!PAYOUT_ALLOWED);
        assert!(!SETTLEMENT_ALLOWED);
        assert!(!MINT_ALLOWED);
    }

    #[test]
    fn node_sync_missing_target_fails_closed() {
        let snapshot = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: true,
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
    fn node_sync_offline_and_error_are_not_ready() {
        let offline = evaluate_node_sync(NodeSyncEvidence {
            sync_mode: NodeSyncMode::RemoteNode,
            connected: false,
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
