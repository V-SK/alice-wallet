#![allow(dead_code)]

use crate::chain;

pub const MINING_EXECUTION_ALLOWED: bool = false;
pub const CUSTOM_POOL_ALLOWED: bool = false;
pub const LTC_DOGE_ALLOWED: bool = false;
pub const AI_JOBS_ALLOWED: bool = false;
pub const POOL_CONFIG_VISIBLE: bool = false;
pub const PAYOUT_RELEASE_ALLOWED: bool = false;
pub const SETTLEMENT_ALLOWED: bool = false;
pub const MINT_ALLOWED: bool = false;
pub const REWARD_EVIDENCE_TTL_SECONDS: u64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletMiningRouteKind {
    WalletXmr,
}

impl WalletMiningRouteKind {
    pub fn label(self) -> &'static str {
        match self {
            WalletMiningRouteKind::WalletXmr => "Wallet XMR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletMiningStatus {
    Preparing,
    EvidenceAvailable,
    EvidenceStale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardEvidenceStatus {
    Pending,
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletMiningRoute {
    pub route_kind: WalletMiningRouteKind,
    pub alice_approved_route: bool,
    pub custom_pool_allowed: bool,
    pub ltc_doge_allowed: bool,
    pub ai_jobs_allowed: bool,
    pub mining_execution_allowed: bool,
    pub pool_config_visible: bool,
    pub reward_identity: String,
    pub worker_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedShareEvidence {
    pub accepted_shares: u64,
    pub rejected_shares: u64,
    pub estimated_rewards: String,
    pub confirmed_rewards: String,
    pub freshness_seconds: u64,
    pub daily_window: String,
    pub last_updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletRewardProjection {
    pub estimated_rewards: String,
    pub confirmed_rewards: String,
    pub pending_rewards: String,
    pub held_rewards: String,
    pub released_rewards: String,
    pub accepted_shares: u64,
    pub rejected_shares: u64,
    pub evidence_status: RewardEvidenceStatus,
    pub evidence_freshness_seconds: Option<u64>,
    pub daily_window: String,
    pub last_updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletMiningStatusPacket {
    pub route: WalletMiningRoute,
    pub mining_status: WalletMiningStatus,
    pub rewards: WalletRewardProjection,
}

pub fn wallet_xmr_route(reward_identity: &str) -> Result<WalletMiningRoute, String> {
    let trimmed = reward_identity.trim();
    chain::validate_address(trimmed)?;
    Ok(WalletMiningRoute {
        route_kind: WalletMiningRouteKind::WalletXmr,
        alice_approved_route: true,
        custom_pool_allowed: CUSTOM_POOL_ALLOWED,
        ltc_doge_allowed: LTC_DOGE_ALLOWED,
        ai_jobs_allowed: AI_JOBS_ALLOWED,
        mining_execution_allowed: MINING_EXECUTION_ALLOWED,
        pool_config_visible: POOL_CONFIG_VISIBLE,
        reward_identity: trimmed.to_string(),
        worker_identity: worker_identity(trimmed),
    })
}

pub fn rehearsal_status_packet(
    reward_identity: &str,
    evidence: Option<AcceptedShareEvidence>,
) -> Result<WalletMiningStatusPacket, String> {
    let route = wallet_xmr_route(reward_identity)?;
    let rewards = evaluate_reward_projection(evidence);
    let mining_status = match rewards.evidence_status {
        RewardEvidenceStatus::Fresh => WalletMiningStatus::EvidenceAvailable,
        RewardEvidenceStatus::Stale => WalletMiningStatus::EvidenceStale,
        RewardEvidenceStatus::Pending => WalletMiningStatus::Preparing,
        RewardEvidenceStatus::Unavailable => WalletMiningStatus::Unavailable,
    };
    Ok(WalletMiningStatusPacket {
        route,
        mining_status,
        rewards,
    })
}

pub fn evaluate_reward_projection(
    evidence: Option<AcceptedShareEvidence>,
) -> WalletRewardProjection {
    let Some(evidence) = evidence else {
        return WalletRewardProjection {
            estimated_rewards: "0 ALICE".into(),
            confirmed_rewards: "0 ALICE".into(),
            pending_rewards: "Pending pool evidence".into(),
            held_rewards: "0 ALICE".into(),
            released_rewards: "Unavailable".into(),
            accepted_shares: 0,
            rejected_shares: 0,
            evidence_status: RewardEvidenceStatus::Pending,
            evidence_freshness_seconds: None,
            daily_window: "Daily pool evidence window".into(),
            last_updated_at: "Unavailable".into(),
        };
    };

    if evidence.freshness_seconds > REWARD_EVIDENCE_TTL_SECONDS {
        return WalletRewardProjection {
            estimated_rewards: evidence.estimated_rewards,
            confirmed_rewards: "0 ALICE".into(),
            pending_rewards: "Pending fresh pool evidence".into(),
            held_rewards: "Held for fresh evidence".into(),
            released_rewards: "Unavailable".into(),
            accepted_shares: evidence.accepted_shares,
            rejected_shares: evidence.rejected_shares,
            evidence_status: RewardEvidenceStatus::Stale,
            evidence_freshness_seconds: Some(evidence.freshness_seconds),
            daily_window: evidence.daily_window,
            last_updated_at: evidence.last_updated_at,
        };
    }

    WalletRewardProjection {
        estimated_rewards: evidence.estimated_rewards,
        confirmed_rewards: evidence.confirmed_rewards,
        pending_rewards: "0 ALICE".into(),
        held_rewards: "0 ALICE".into(),
        released_rewards: "Unavailable".into(),
        accepted_shares: evidence.accepted_shares,
        rejected_shares: evidence.rejected_shares,
        evidence_status: RewardEvidenceStatus::Fresh,
        evidence_freshness_seconds: Some(evidence.freshness_seconds),
        daily_window: evidence.daily_window,
        last_updated_at: evidence.last_updated_at,
    }
}

fn worker_identity(address: &str) -> String {
    if address.len() <= 16 {
        return format!("wallet-{}", address);
    }
    let head: String = address.chars().take(8).collect();
    let tail: String = address
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("wallet-{}{}", head, tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn valid_address() -> &'static str {
        static ADDRESS: OnceLock<String> = OnceLock::new();
        ADDRESS.get_or_init(|| {
            let unlock_phrase = "miner-test-passphrase";
            crate::crypto::create_wallet_payload(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
                unlock_phrase,
            )
            .expect("test wallet payload")
            .address
        })
    }

    #[test]
    fn wallet_route_is_xmr_only_and_default_off() {
        let route = wallet_xmr_route(valid_address()).expect("wallet route");
        assert_eq!(route.route_kind, WalletMiningRouteKind::WalletXmr);
        assert!(route.alice_approved_route);
        assert!(!route.custom_pool_allowed);
        assert!(!route.ltc_doge_allowed);
        assert!(!route.ai_jobs_allowed);
        assert!(!route.mining_execution_allowed);
        assert!(!route.pool_config_visible);
        assert_eq!(route.reward_identity, valid_address());
        assert!(route.worker_identity.starts_with("wallet-"));
    }

    #[test]
    fn invalid_reward_identity_fails_closed() {
        assert!(wallet_xmr_route("not-an-address").is_err());
    }

    #[test]
    fn missing_evidence_is_pending_not_confirmed() {
        let rewards = evaluate_reward_projection(None);
        assert_eq!(rewards.evidence_status, RewardEvidenceStatus::Pending);
        assert_eq!(rewards.confirmed_rewards, "0 ALICE");
        assert_eq!(rewards.released_rewards, "Unavailable");
        assert_eq!(rewards.accepted_shares, 0);
    }

    #[test]
    fn stale_evidence_does_not_become_confirmed() {
        let rewards = evaluate_reward_projection(Some(AcceptedShareEvidence {
            accepted_shares: 42,
            rejected_shares: 1,
            estimated_rewards: "0.70 ALICE".into(),
            confirmed_rewards: "0.60 ALICE".into(),
            freshness_seconds: REWARD_EVIDENCE_TTL_SECONDS + 1,
            daily_window: "2026-05-18".into(),
            last_updated_at: "2026-05-18T00:00:00Z".into(),
        }));
        assert_eq!(rewards.evidence_status, RewardEvidenceStatus::Stale);
        assert_eq!(rewards.confirmed_rewards, "0 ALICE");
        assert_eq!(rewards.held_rewards, "Held for fresh evidence");
        assert_eq!(rewards.accepted_shares, 42);
    }

    #[test]
    fn fresh_evidence_can_display_projection_without_release() {
        let rewards = evaluate_reward_projection(Some(AcceptedShareEvidence {
            accepted_shares: 8,
            rejected_shares: 0,
            estimated_rewards: "0.15 ALICE".into(),
            confirmed_rewards: "0.10 ALICE".into(),
            freshness_seconds: 60,
            daily_window: "2026-05-18".into(),
            last_updated_at: "2026-05-18T00:01:00Z".into(),
        }));
        assert_eq!(rewards.evidence_status, RewardEvidenceStatus::Fresh);
        assert_eq!(rewards.estimated_rewards, "0.15 ALICE");
        assert_eq!(rewards.confirmed_rewards, "0.10 ALICE");
        assert_eq!(rewards.released_rewards, "Unavailable");
    }

    #[test]
    fn execution_flags_remain_false() {
        assert!(!MINING_EXECUTION_ALLOWED);
        assert!(!CUSTOM_POOL_ALLOWED);
        assert!(!LTC_DOGE_ALLOWED);
        assert!(!AI_JOBS_ALLOWED);
        assert!(!POOL_CONFIG_VISIBLE);
        assert!(!PAYOUT_RELEASE_ALLOWED);
        assert!(!SETTLEMENT_ALLOWED);
        assert!(!MINT_ALLOWED);
    }
}
