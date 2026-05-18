#![allow(dead_code)]

use crate::chain;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinerProfile {
    LocalCpu,
    LocalGpu,
    Pool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeMiningReadiness {
    NotRequired,
    RequiredReady,
    RequiredDisconnected,
    RequiredUnsynced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinerConfig {
    pub profile: MinerProfile,
    pub binary_path: PathBuf,
    pub payout_address: String,
    pub threads: u16,
    pub endpoint: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinerStartPolicy {
    pub binary_exists: bool,
    pub binary_verified: bool,
    pub wallet_backup_complete: bool,
    pub remote_trust_acknowledged: bool,
    pub node_readiness: NodeMiningReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinerCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MinerStartBlock {
    BinaryMissing,
    BinaryUnverified,
    InvalidPayoutAddress,
    InvalidThreadCount,
    MissingPoolEndpoint,
    LocalNodeDisconnected,
    LocalNodeUnsynced,
    WalletBackupRequired,
    RemoteTrustNotAcknowledged,
    SensitiveArgument(String),
}

pub fn build_miner_command(
    config: &MinerConfig,
    policy: &MinerStartPolicy,
) -> Result<MinerCommand, MinerStartBlock> {
    validate_start(config, policy)?;

    let mut args = vec![
        "--payout-address".to_string(),
        config.payout_address.trim().to_string(),
        "--threads".to_string(),
        config.threads.to_string(),
    ];

    match config.profile {
        MinerProfile::LocalCpu => {
            args.push("--mode".to_string());
            args.push("local-cpu".to_string());
        }
        MinerProfile::LocalGpu => {
            args.push("--mode".to_string());
            args.push("local-gpu".to_string());
        }
        MinerProfile::Pool => {
            args.push("--mode".to_string());
            args.push("pool".to_string());
            args.push("--endpoint".to_string());
            args.push(
                config
                    .endpoint
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            );
        }
    }

    if let Some(data_dir) = &config.data_dir {
        args.push("--data-dir".to_string());
        args.push(data_dir.display().to_string());
    }

    args.extend(config.extra_args.iter().cloned());

    Ok(MinerCommand {
        program: config.binary_path.clone(),
        args,
    })
}

pub fn validate_start(
    config: &MinerConfig,
    policy: &MinerStartPolicy,
) -> Result<(), MinerStartBlock> {
    if !policy.binary_exists {
        return Err(MinerStartBlock::BinaryMissing);
    }
    if !policy.binary_verified {
        return Err(MinerStartBlock::BinaryUnverified);
    }
    if config.threads == 0 {
        return Err(MinerStartBlock::InvalidThreadCount);
    }
    if chain::validate_address(config.payout_address.trim()).is_err() {
        return Err(MinerStartBlock::InvalidPayoutAddress);
    }
    if !policy.wallet_backup_complete {
        return Err(MinerStartBlock::WalletBackupRequired);
    }
    reject_sensitive_args(&config.extra_args)?;

    match config.profile {
        MinerProfile::LocalCpu | MinerProfile::LocalGpu => match policy.node_readiness {
            NodeMiningReadiness::RequiredReady => {}
            NodeMiningReadiness::RequiredDisconnected => {
                return Err(MinerStartBlock::LocalNodeDisconnected);
            }
            NodeMiningReadiness::RequiredUnsynced => {
                return Err(MinerStartBlock::LocalNodeUnsynced);
            }
            NodeMiningReadiness::NotRequired => {}
        },
        MinerProfile::Pool => {
            let endpoint = config.endpoint.as_deref().unwrap_or_default().trim();
            if endpoint.is_empty() {
                return Err(MinerStartBlock::MissingPoolEndpoint);
            }
            if !policy.remote_trust_acknowledged {
                return Err(MinerStartBlock::RemoteTrustNotAcknowledged);
            }
        }
    }

    Ok(())
}

pub fn payout_address_change_allowed(wallet_unlocked: bool, manual_entry_confirmed: bool) -> bool {
    wallet_unlocked || manual_entry_confirmed
}

pub fn redact_miner_log_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if sensitive_markers()
        .iter()
        .any(|marker| lower.contains(marker))
    {
        "[redacted sensitive miner log]".to_string()
    } else {
        line.to_string()
    }
}

fn reject_sensitive_args(args: &[String]) -> Result<(), MinerStartBlock> {
    for arg in args {
        let lower = arg.to_ascii_lowercase();
        if let Some(marker) = sensitive_markers()
            .iter()
            .find(|marker| lower.contains(*marker))
        {
            return Err(MinerStartBlock::SensitiveArgument((*marker).to_string()));
        }
    }
    Ok(())
}

fn sensitive_markers() -> &'static [&'static str] {
    &[
        "mnemonic",
        "private-key",
        "private_key",
        "secret-key",
        "secret_key",
        "wallet-password",
        "wallet_password",
        "password",
        "seed",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn valid_address() -> &'static str {
        static ADDRESS: OnceLock<String> = OnceLock::new();
        ADDRESS.get_or_init(|| {
            crate::crypto::create_wallet_payload(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
                "miner-test-password",
            )
            .expect("test wallet payload")
            .address
        })
    }

    fn base_config(profile: MinerProfile) -> MinerConfig {
        MinerConfig {
            profile,
            binary_path: PathBuf::from("/opt/alice/bin/alice-miner"),
            payout_address: valid_address().to_string(),
            threads: 4,
            endpoint: None,
            data_dir: Some(PathBuf::from("/tmp/alice-miner-data")),
            extra_args: vec![],
        }
    }

    fn base_policy() -> MinerStartPolicy {
        MinerStartPolicy {
            binary_exists: true,
            binary_verified: true,
            wallet_backup_complete: true,
            remote_trust_acknowledged: false,
            node_readiness: NodeMiningReadiness::RequiredReady,
        }
    }

    #[test]
    fn local_command_uses_address_only_payout() {
        let command = build_miner_command(&base_config(MinerProfile::LocalCpu), &base_policy())
            .expect("local miner command");

        let joined = command.args.join(" ");
        assert!(joined.contains("--payout-address"));
        assert!(joined.contains(valid_address()));
        assert!(joined.contains("--threads 4"));
        assert!(!joined.contains("mnemonic"));
        assert!(!joined.contains("private"));
        assert!(!joined.contains("password"));
        assert!(!joined.contains("seed"));
    }

    #[test]
    fn invalid_payout_address_blocks_start() {
        let mut config = base_config(MinerProfile::LocalCpu);
        config.payout_address = "not-an-address".to_string();

        assert_eq!(
            validate_start(&config, &base_policy()),
            Err(MinerStartBlock::InvalidPayoutAddress)
        );
    }

    #[test]
    fn missing_or_unverified_binary_blocks_start() {
        let config = base_config(MinerProfile::LocalCpu);
        let mut policy = base_policy();
        policy.binary_exists = false;
        assert_eq!(
            validate_start(&config, &policy),
            Err(MinerStartBlock::BinaryMissing)
        );

        policy.binary_exists = true;
        policy.binary_verified = false;
        assert_eq!(
            validate_start(&config, &policy),
            Err(MinerStartBlock::BinaryUnverified)
        );
    }

    #[test]
    fn local_mining_requires_ready_node_when_required() {
        let config = base_config(MinerProfile::LocalCpu);
        let mut policy = base_policy();

        policy.node_readiness = NodeMiningReadiness::RequiredDisconnected;
        assert_eq!(
            validate_start(&config, &policy),
            Err(MinerStartBlock::LocalNodeDisconnected)
        );

        policy.node_readiness = NodeMiningReadiness::RequiredUnsynced;
        assert_eq!(
            validate_start(&config, &policy),
            Err(MinerStartBlock::LocalNodeUnsynced)
        );
    }

    #[test]
    fn pool_mining_requires_endpoint_and_trust_ack() {
        let mut config = base_config(MinerProfile::Pool);
        let mut policy = base_policy();
        policy.node_readiness = NodeMiningReadiness::NotRequired;

        assert_eq!(
            validate_start(&config, &policy),
            Err(MinerStartBlock::MissingPoolEndpoint)
        );

        config.endpoint = Some("stratum+tcp://pool.example:3333".to_string());
        assert_eq!(
            validate_start(&config, &policy),
            Err(MinerStartBlock::RemoteTrustNotAcknowledged)
        );

        policy.remote_trust_acknowledged = true;
        assert_eq!(validate_start(&config, &policy), Ok(()));
    }

    #[test]
    fn payout_address_change_is_locked_down() {
        assert!(!payout_address_change_allowed(false, false));
        assert!(payout_address_change_allowed(true, false));
        assert!(payout_address_change_allowed(false, true));
    }

    #[test]
    fn unbacked_wallet_blocks_start() {
        let config = base_config(MinerProfile::LocalCpu);
        let mut policy = base_policy();
        policy.wallet_backup_complete = false;

        assert_eq!(
            validate_start(&config, &policy),
            Err(MinerStartBlock::WalletBackupRequired)
        );
    }

    #[test]
    fn sensitive_extra_args_are_rejected() {
        let mut config = base_config(MinerProfile::LocalCpu);
        config.extra_args = vec!["--seed=0xabc123".to_string()];

        assert_eq!(
            validate_start(&config, &base_policy()),
            Err(MinerStartBlock::SensitiveArgument("seed".to_string()))
        );
    }

    #[test]
    fn miner_logs_are_redacted_before_gui_use() {
        assert_eq!(
            redact_miner_log_line("starting with wallet-password hunter2"),
            "[redacted sensitive miner log]"
        );
        assert_eq!(redact_miner_log_line("hashrate 42 H/s"), "hashrate 42 H/s");
    }
}
