#![allow(dead_code)]

use crate::chain;
use std::path::PathBuf;

/// EXPERIMENTAL / "测试中": the wallet is now allowed to RUN a bundled CPU miner
/// (XMRig) against Alice's own re-hash relay. This is OPT-IN — nothing mines
/// until the user clicks Start on the Mining page. It is a CREDIT-ONLY work
/// substrate: there is NO payout, settlement, mint, or chain write here; the
/// relay credits the user's Alice reward identity via the worker name. All the
/// OTHER capability gates below remain `false`.
pub const MINING_EXECUTION_ALLOWED: bool = true;

/// The wallet's mining feature is testing/experimental (surfaced as a badge in
/// the UI). Keep this until the feature graduates out of the experimental phase.
pub const MINING_EXPERIMENTAL: bool = true;

pub const CUSTOM_POOL_ALLOWED: bool = false;
pub const LTC_DOGE_ALLOWED: bool = false;
pub const AI_JOBS_ALLOWED: bool = false;
pub const POOL_CONFIG_VISIBLE: bool = false;
pub const PAYOUT_RELEASE_ALLOWED: bool = false;
pub const SETTLEMENT_ALLOWED: bool = false;
pub const MINT_ALLOWED: bool = false;
pub const REWARD_EVIDENCE_TTL_SECONDS: u64 = 180;

// ── Mining engine wiring (Alice re-hash relay, standard stratum) ────────────

/// Alice's own re-hash relay host (the friend's HK relay → core). Standard
/// stratum; the wallet mines RandomX/XMR work against it.
pub const ALICE_POOL_HOST: &str = "hk.aliceprotocol.org";
/// Stratum port for the XMR/RandomX lane on the relay.
pub const ALICE_POOL_PORT: u16 = 3333;

/// OUR XMR collection address — the on-chain XMR destination the RELAY mines to
/// UPSTREAM (server-side), NOT a value the wallet sends. The wallet logs in to
/// the proxy with the user's OWN Alice address (open enrollment); the relay maps
/// that credit identity and forwards the work to this single collection wallet
/// upstream. Kept for reference/documentation only.
pub const ALICE_XMR_COLLECTION_ADDRESS: &str =
    "46knTVDfa5CMtFLvVuFdHWPSv7FCnfSbQbaPTFai7Mt6PbfTGhKmBkVETSjYvF9wwfbdHdeSAxuee9Ha7T4baBzLKBaq9LG";

/// High sanity ceiling on miner threads — NOT a throttle. V wants it "拉满"
/// (full power), so this only bounds an absurd `available_parallelism` value.
const MINER_MAX_THREADS: usize = 256;

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

// ── Stratum worker id (matches the proven worker-client pipeline) ───────────

/// The Alice SS58 network / format id (must match `crypto::SS58_FORMAT` and the
/// vendored Python `ALICE_SS58_FORMAT = 300`).
const ALICE_SS58_FORMAT: u16 = 300;
/// Substrate account-id (public key) length in bytes.
const ALICE_PUBKEY_LENGTH: usize = 32;
/// SS58 checksum length (bytes) for a 32-byte account id.
const SS58_CHECKSUM_LENGTH: usize = 2;
/// Max stratum worker-name length (matches the Python `derive_worker_id`).
const WORKER_ID_MAX_LENGTH: usize = 64;

/// Encode an SS58 network ident to its on-wire prefix bytes (idents 64..16383
/// use the 2-byte encoding; ident 300 → `0x4b 0x01`). Mirrors
/// `crypto::account_id_to_ss58` and the Python `_ss58_prefix_bytes`.
fn ss58_prefix_bytes(ident: u16) -> Vec<u8> {
    if ident < 64 {
        vec![ident as u8]
    } else {
        let first = ((ident & 0b0000_0000_1111_1100) as u8 >> 2) | 0b0100_0000;
        let second = ((ident >> 8) as u8) | (((ident & 0b0000_0000_0000_0011) as u8) << 6);
        vec![first, second]
    }
}

/// Return the canonical Alice address IFF `address` is a checksum-valid SS58
/// format-300 one (the miner's reward identity). Fail-closed `None` otherwise.
///
/// Self-contained replica of the vendored
/// `Alice-worker-v1/.../alice_address.py::validate_alice_address`: base58-decode
/// to EXACTLY `prefix(2) ‖ pubkey(32) ‖ checksum(2)`, require the Alice network
/// prefix, and verify the blake2b-512(`SS58PRE` ‖ prefix ‖ pubkey) checksum.
fn validate_alice_address(address: &str) -> Option<String> {
    use blake2::{Blake2b512, Digest};

    if address.is_empty() || address.len() > 64 {
        return None;
    }
    // ASCII printable only (no control / whitespace / non-ASCII).
    if address.chars().any(|ch| (ch as u32) < 0x21 || (ch as u32) > 0x7E) {
        return None;
    }
    let raw = bs58::decode(address).into_vec().ok()?;
    let prefix = ss58_prefix_bytes(ALICE_SS58_FORMAT);
    let expected_len = prefix.len() + ALICE_PUBKEY_LENGTH + SS58_CHECKSUM_LENGTH;
    if raw.len() != expected_len {
        return None;
    }
    if raw[..prefix.len()] != prefix[..] {
        return None;
    }
    let pubkey = &raw[prefix.len()..prefix.len() + ALICE_PUBKEY_LENGTH];
    let checksum = &raw[prefix.len() + ALICE_PUBKEY_LENGTH..];

    let mut hasher = Blake2b512::new();
    hasher.update(b"SS58PRE");
    hasher.update(&prefix);
    hasher.update(pubkey);
    let digest = hasher.finalize();
    if digest[..SS58_CHECKSUM_LENGTH] != checksum[..] {
        return None;
    }
    Some(address.to_string())
}

/// Derive a stable, stratum-safe worker name from a (validated) Alice address —
/// the on-wire `<worker_id>` for the `<XMR_addr>.<worker_id>` stratum user.
///
/// Replicates `derive_worker_id(address)` from the proven worker-client
/// pipeline (`Alice-worker-v1/client_ui/mining_engine/alice_address.py`): SS58
/// base58 chars are a subset of the stratum-safe `[A-Za-z0-9_.-]` charset, so we
/// keep the address verbatim when it fits in `WORKER_ID_MAX_LENGTH` (real
/// format-300 addresses are ~49 chars, so they always do), else take the head
/// plus a 4-byte blake2b tag of the full address so distinct addresses never
/// collide. NON-secret: derived from the PUBLIC address only.
pub fn derive_worker_id(address: &str) -> Result<String, String> {
    use blake2::digest::{Update, VariableOutput};
    use blake2::Blake2bVar;

    let canonical = validate_alice_address(address).ok_or("invalid_alice_address")?;
    if canonical.len() <= WORKER_ID_MAX_LENGTH {
        return Ok(canonical);
    }
    let mut hasher = Blake2bVar::new(4).expect("blake2b-4 is a valid output size");
    hasher.update(canonical.as_bytes());
    let mut tag_bytes = [0u8; 4];
    hasher
        .finalize_variable(&mut tag_bytes)
        .expect("blake2b-4 output");
    let tag = hex::encode(tag_bytes);
    let head: String = canonical
        .chars()
        .take(WORKER_ID_MAX_LENGTH - tag.len() - 1)
        .collect();
    Ok(format!("{head}.{tag}"))
}

/// Everything needed to spawn the bundled XMRig against Alice's relay, fully
/// validated. Pure / testable — actual process spawning lives in
/// [`crate::supervise`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinerLaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Miner thread count = ALL logical cores ("拉满" / full power — V 2026-06-03).
/// Mining is strictly OPT-IN (the Start button), so when the user turns it on
/// they want maximum hash power, not a timid fraction. Bounded only by the high
/// [`MINER_MAX_THREADS`] sanity ceiling.
///
/// Uses `std::thread::available_parallelism` (logical cores) to avoid pulling in
/// a physical-core crate.
fn miner_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, MINER_MAX_THREADS)
}

/// Build the validated XMRig launch plan for the active reward identity.
///
/// argv (RandomX/XMR against the Alice re-hash relay):
/// `-o hk.aliceprotocol.org:3333 -u <alice_addr> -p x --rig-id <worker_id>
///  --coin monero --no-color --print-time 10 --donate-level 0 --cpu-priority 1
///  --threads <N>`
///
/// The login USER is the user's OWN Alice reward identity (SS58-300) — the proxy
/// open-enrollment credits that address (a non-Alice login is NACKed as
/// "stratum_login_open_bad_address"). `<worker_id>` ([`derive_worker_id`]) is a
/// stable per-device rig id. OUR XMR collection address + the upstream pool are
/// handled SERVER-SIDE by the relay; the wallet seed/private key is NEVER passed.
pub fn build_miner_launch_plan(
    program: PathBuf,
    reward_identity: &str,
) -> Result<MinerLaunchPlan, String> {
    if !MINING_EXECUTION_ALLOWED {
        return Err("mining execution is not enabled in this build".into());
    }
    // Proxy login (VERIFIED against the live relay 2026-06-03): the stratum USER
    // is the user's OWN Alice reward identity (SS58-300). The proxy's open
    // enrollment credits that address and NACKs a non-Alice login as
    // "stratum_login_open_bad_address"; password is the conventional "x". The
    // upstream XMR pool + OUR collection address are handled SERVER-SIDE by the
    // relay — the wallet only ever sends the user's PUBLIC Alice address, never
    // the seed/key. `derive_worker_id` doubles as the fail-closed Alice-address
    // validator and a stable per-device rig id.
    let reward = reward_identity.trim();
    let rig_id = derive_worker_id(reward)?;
    let pool = format!("{ALICE_POOL_HOST}:{ALICE_POOL_PORT}");
    let threads = miner_thread_count();
    let args = vec![
        "-o".into(),
        pool,
        "-u".into(),
        reward.to_string(),
        "-p".into(),
        "x".into(),
        "--rig-id".into(),
        rig_id,
        "--coin".into(),
        "monero".into(),
        "--no-color".into(),
        "--print-time".into(),
        "10".into(),
        "--donate-level".into(),
        "0".into(),
        "--cpu-priority".into(),
        "1".into(),
        "--threads".into(),
        threads.to_string(),
    ];
    Ok(MinerLaunchPlan { program, args })
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
    fn wallet_route_is_xmr_only_and_run_is_opt_in() {
        let route = wallet_xmr_route(valid_address()).expect("wallet route");
        assert_eq!(route.route_kind, WalletMiningRouteKind::WalletXmr);
        assert!(route.alice_approved_route);
        // RUN capability is now ON (the user can opt in by clicking Start), but
        // every OTHER capability gate stays closed (credit-only posture).
        assert!(route.mining_execution_allowed);
        assert!(!route.custom_pool_allowed);
        assert!(!route.ltc_doge_allowed);
        assert!(!route.ai_jobs_allowed);
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
    fn run_is_enabled_but_all_other_gates_remain_false() {
        // The wallet may RUN the miner now (opt-in), and the feature is flagged
        // experimental — but NO payout / settlement / mint / custom-pool / other
        // coin / AI-job capability is unlocked (credit-only posture). These are
        // compile-time constants, so assert them in a const block (the assertion
        // then fails the BUILD if a gate is ever flipped the wrong way).
        const _: () = {
            assert!(MINING_EXECUTION_ALLOWED);
            assert!(MINING_EXPERIMENTAL);
            assert!(!CUSTOM_POOL_ALLOWED);
            assert!(!LTC_DOGE_ALLOWED);
            assert!(!AI_JOBS_ALLOWED);
            assert!(!POOL_CONFIG_VISIBLE);
            assert!(!PAYOUT_RELEASE_ALLOWED);
            assert!(!SETTLEMENT_ALLOWED);
            assert!(!MINT_ALLOWED);
        };
    }

    #[test]
    fn worker_id_matches_validated_address_verbatim() {
        // A real format-300 Alice address is ~49 base58 chars (< 64), so the
        // worker id IS the address verbatim — matching the proven worker-client
        // pipeline's `derive_worker_id` (which keeps short addresses as-is).
        let addr = valid_address();
        let worker = derive_worker_id(addr).expect("worker id");
        assert_eq!(worker, addr);
        assert!(worker.len() <= WORKER_ID_MAX_LENGTH);
        // Stratum-safe charset only.
        assert!(worker
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')));
    }

    #[test]
    fn worker_id_fails_closed_on_non_alice_address() {
        // Not an SS58-300 address → reject (never an accept under an un-ownable
        // key). Polkadot/garbage strings are rejected.
        assert!(derive_worker_id("not-an-address").is_err());
        assert!(derive_worker_id("").is_err());
        // A generic-substrate (network 42) address is a valid SS58 string but
        // the WRONG network — must be rejected.
        assert!(derive_worker_id("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").is_err());
    }

    #[test]
    fn miner_launch_plan_targets_relay_with_xmr_login_convention() {
        let addr = valid_address();
        let plan = build_miner_launch_plan(PathBuf::from("/usr/local/bin/xmrig"), addr)
            .expect("launch plan");

        // Pool target is OUR relay on the XMR/RandomX port.
        let o = plan.args.iter().position(|a| a == "-o").expect("-o present");
        assert_eq!(plan.args[o + 1], format!("{ALICE_POOL_HOST}:{ALICE_POOL_PORT}"));

        // Login USER = the user's OWN Alice reward identity (the open-enrollment
        // credit identity the proxy expects); password "x". (NOT our collection
        // address — verified against the live relay: a non-Alice login is NACKed
        // as "stratum_login_open_bad_address".)
        let u = plan.args.iter().position(|a| a == "-u").expect("-u present");
        assert_eq!(plan.args[u + 1].as_str(), addr);
        let p = plan.args.iter().position(|a| a == "-p").expect("-p present");
        assert_eq!(plan.args[p + 1].as_str(), "x");
        // The per-device rig id is derive_worker_id of the reward identity.
        let r = plan.args.iter().position(|a| a == "--rig-id").expect("--rig-id present");
        assert_eq!(plan.args[r + 1], derive_worker_id(addr).unwrap());

        // Monero coin, no-color, donate-level 0, cpu-priority 1, print-time 10.
        assert!(plan.args.windows(2).any(|w| w[0] == "--coin" && w[1] == "monero"));
        assert!(plan.args.iter().any(|a| a == "--no-color"));
        assert!(plan.args.windows(2).any(|w| w[0] == "--donate-level" && w[1] == "0"));
        assert!(plan.args.windows(2).any(|w| w[0] == "--cpu-priority" && w[1] == "1"));
        assert!(plan.args.windows(2).any(|w| w[0] == "--print-time" && w[1] == "10"));

        // Full power ("拉满"): --threads == all logical cores.
        let t = plan.args.iter().position(|a| a == "--threads").expect("--threads");
        let n: usize = plan.args[t + 1].parse().expect("thread count");
        assert_eq!(n, miner_thread_count());
        assert!(n >= 1);

        // The wallet seed/private key NEVER appears anywhere in the argv.
        assert!(!plan.args.iter().any(|a| a.contains("seed") || a.contains("priv")));
    }

    #[test]
    fn miner_launch_plan_fails_closed_on_bad_reward_identity() {
        assert!(build_miner_launch_plan(PathBuf::from("xmrig"), "not-an-address").is_err());
    }

    #[test]
    fn collection_address_is_the_pinned_xmr_wallet() {
        // Guard the hardcoded collection wallet against accidental edits.
        assert!(ALICE_XMR_COLLECTION_ADDRESS.starts_with("46knTVDfa5CMtFLvVuFdHWPSv7FCnfSbQ"));
        assert_eq!(ALICE_XMR_COLLECTION_ADDRESS.len(), 95);
    }
}
