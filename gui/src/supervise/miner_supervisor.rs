//! `MinerSupervisor` — owns the bundled CPU miner (XMRig) child process and
//! exposes a cloneable [`MinerStats`] snapshot (hashrate + accepted/rejected
//! shares + status). Modeled on [`super::node_supervisor::NodeSupervisor`].
//!
//! EXPERIMENTAL / "测试中": the wallet may RUN this miner, but only when the user
//! opts in (clicks Start), and it is CREDIT-ONLY — the miner points at Alice's
//! own re-hash relay and the relay credits the user's Alice reward identity via
//! the stratum worker name. No payout/settlement/mint/chain symbol is involved.
//!
//! Responsibilities (mirrors the node supervisor, minus auto-restart — a wallet
//! miner that exits should simply stop, not restart-loop the user's CPU):
//! - spawn / own / stop the XMRig child via [`super::child`]
//! - kill the child on stop AND on drop (`kill_on_drop` + explicit SIGTERM→KILL)
//! - drain the stdout/stderr `LogLine` channel on a background task and parse
//!   hashrate + accepted/rejected shares from XMRig's output
//! - guarantee a miner crash can never lock/corrupt the wallet (no custody state)
//!
//! The wallet seed/private key is NEVER passed to the child (see
//! [`crate::miner::build_miner_launch_plan`]); the miner only sees the PUBLIC
//! Alice address.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc::unbounded_channel;

use super::child::{spawn_supervised, LogLine, OwnedChild};
use super::ProcState;
use crate::miner::MinerLaunchPlan;

/// Grace period for a graceful miner stop before SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// A point-in-time, UI-safe snapshot of the miner. Cloneable and free of any
/// handle / secret so it can be read every frame by the GUI.
#[derive(Debug, Clone, PartialEq)]
pub struct MinerStats {
    /// Whether the miner process is currently active (starting/running/stopping).
    pub running: bool,
    /// Lifecycle state (drives the status pill).
    pub state: ProcState,
    /// Most recent hashrate in H/s (the 10s, else 60s figure from XMRig). `None`
    /// until the first speed line arrives (XMRig prints `n/a` early on).
    pub hashrate_hs: Option<f64>,
    /// Running count of accepted shares (latest XMRig `(A/R)` figure).
    pub accepted: u64,
    /// Running count of rejected shares (latest XMRig `(A/R)` figure).
    pub rejected: u64,
    /// Last process exit code, when it has exited.
    pub last_exit_code: Option<i32>,
    /// Short, sanitised reason for the current state, if any (e.g. spawn error).
    pub message: Option<String>,
    /// Last sanitised output line (for an at-a-glance "what is it doing" hint).
    pub last_line: String,
}

impl Default for MinerStats {
    fn default() -> Self {
        Self {
            running: false,
            state: ProcState::Stopped,
            hashrate_hs: None,
            accepted: 0,
            rejected: 0,
            last_exit_code: None,
            message: None,
            last_line: String::new(),
        }
    }
}

/// Shared, lock-guarded supervisor state. Cloneable handle.
#[derive(Clone)]
pub struct MinerSupervisor {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    state: ProcState,
    pid: Option<u32>,
    last_exit_code: Option<i32>,
    message: Option<String>,
    hashrate_hs: Option<f64>,
    accepted: u64,
    rejected: u64,
    last_line: String,
    /// User explicitly requested stop — used by the supervision loop.
    stop_requested: bool,
    /// Generation counter; bumped on every start/stop so a stale supervision
    /// loop from a previous child can't clobber newer state.
    generation: u64,
}

impl Default for MinerSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl MinerSupervisor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: ProcState::Stopped,
                pid: None,
                last_exit_code: None,
                message: None,
                hashrate_hs: None,
                accepted: 0,
                rejected: 0,
                last_line: String::new(),
                stop_requested: false,
                generation: 0,
            })),
        }
    }

    /// Current UI-safe snapshot.
    pub fn stats(&self) -> MinerStats {
        let g = self.inner.lock().expect("miner supervisor mutex");
        MinerStats {
            running: g.state.is_active(),
            state: g.state,
            hashrate_hs: g.hashrate_hs,
            accepted: g.accepted,
            rejected: g.rejected,
            last_exit_code: g.last_exit_code,
            message: g.message.clone(),
            last_line: g.last_line.clone(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().expect("mutex").state.is_active()
    }

    /// Start the miner from a validated launch plan. Spawns the child on the
    /// current tokio runtime plus a supervision task that watches for exit /
    /// honours a stop request. Resets the per-run stats counters.
    pub fn start(&self, plan: MinerLaunchPlan) -> Result<(), String> {
        let gen = {
            let mut g = self.inner.lock().expect("mutex");
            if matches!(g.state, ProcState::Running | ProcState::Starting | ProcState::Stopping) {
                return Err("miner is already running".into());
            }
            g.state = ProcState::Starting;
            g.message = None;
            g.stop_requested = false;
            // Fresh run: reset the observed stats.
            g.hashrate_hs = None;
            g.accepted = 0;
            g.rejected = 0;
            g.last_line.clear();
            g.last_exit_code = None;
            g.generation += 1;
            g.generation
        };

        let (log_tx, mut log_rx) = unbounded_channel::<LogLine>();
        // No extra env, no PID file — the miner is fully ephemeral.
        let owned = match spawn_supervised(&plan.program, &plan.args, &[], None, log_tx) {
            Ok(c) => c,
            Err(e) => {
                let mut g = self.inner.lock().expect("mutex");
                g.state = ProcState::Error;
                g.message = Some(format!("failed to start miner: {e}"));
                return Err(g.message.clone().unwrap());
            }
        };

        let pid = owned.pid();
        {
            let mut g = self.inner.lock().expect("mutex");
            g.pid = Some(pid);
            g.state = ProcState::Running;
        }

        // Log pump → parse hashrate / shares into the snapshot.
        let inner_for_logs = self.inner.clone();
        tokio::spawn(async move {
            while let Some(line) = log_rx.recv().await {
                let mut g = inner_for_logs.lock().expect("mutex");
                if g.generation != gen {
                    break; // superseded by a newer run
                }
                apply_log_line(&mut g, &line.text);
            }
        });

        // Supervision task: wait for exit OR a stop request, then tear down.
        let this = self.clone();
        tokio::spawn(async move {
            this.supervise_until_exit(owned, gen).await;
        });

        Ok(())
    }

    async fn supervise_until_exit(&self, mut owned: OwnedChild, gen: u64) {
        loop {
            if let Some(code) = owned.try_exit_code() {
                let mut g = self.inner.lock().expect("mutex");
                if g.generation == gen {
                    g.last_exit_code = Some(code);
                    g.pid = None;
                    g.hashrate_hs = None;
                    // A user stop lands as Stopped; an unexpected exit as Error.
                    g.state = if g.stop_requested {
                        ProcState::Stopped
                    } else {
                        ProcState::Error
                    };
                    if !g.stop_requested && g.message.is_none() {
                        g.message = Some(format!("miner exited (code {code})"));
                    }
                }
                return;
            }
            let should_stop = {
                let g = self.inner.lock().expect("mutex");
                g.stop_requested && g.generation == gen
            };
            if should_stop {
                let code = owned.stop(STOP_GRACE).await.ok().flatten();
                let mut g = self.inner.lock().expect("mutex");
                if g.generation == gen {
                    g.state = ProcState::Stopped;
                    g.pid = None;
                    g.hashrate_hs = None;
                    g.last_exit_code = code;
                    g.message = None;
                }
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }

    /// Request a graceful stop. The supervision loop performs the actual
    /// SIGTERM→SIGKILL teardown on its next tick; `kill_on_drop` is the backstop.
    pub fn request_stop(&self) {
        let mut g = self.inner.lock().expect("mutex");
        if matches!(g.state, ProcState::Running | ProcState::Starting) {
            g.stop_requested = true;
            g.state = ProcState::Stopping;
        }
    }
}

/// Update the snapshot from one raw XMRig output line: parse hashrate + shares.
fn apply_log_line(g: &mut Inner, raw: &str) {
    let line = super::sanitize_log_line(raw);
    if line.is_empty() {
        return;
    }
    if let Some(hr) = parse_hashrate_hs(&line) {
        g.hashrate_hs = Some(hr);
    }
    if let Some((accepted, rejected)) = parse_share_counts(&line) {
        g.accepted = accepted;
        g.rejected = rejected;
    }
    g.last_line = line;
}

/// Parse the 10s hashrate (H/s) from an XMRig speed line, e.g.
/// `miner    speed 10s/60s/15m 1234.5 1200.0 n/a H/s max 1300.0 H/s`.
/// Returns the first numeric figure after `10s/60s/15m` (the 10s rate); falls
/// back to the next numeric figure (60s) when the 10s slot is `n/a`. `None` when
/// the line is not a speed line or all figures are `n/a`.
fn parse_hashrate_hs(line: &str) -> Option<f64> {
    // Must be a speed line that also carries the H/s unit.
    if !line.contains("speed") || !line.contains("10s/60s/15m") {
        return None;
    }
    let after = line.split("10s/60s/15m").nth(1)?;
    // Tokens up to the unit; XMRig prints up to three figures then `H/s`.
    for tok in after.split_whitespace() {
        if tok.eq_ignore_ascii_case("h/s")
            || tok.eq_ignore_ascii_case("kh/s")
            || tok.eq_ignore_ascii_case("mh/s")
        {
            break;
        }
        if tok.eq_ignore_ascii_case("n/a") {
            continue; // 10s (or 60s) not available yet — try the next figure
        }
        if let Ok(v) = tok.parse::<f64>() {
            return Some(v);
        }
    }
    None
}

/// Parse cumulative `(accepted/rejected)` counts from an XMRig share line, e.g.
/// `net      accepted (12/0) diff 1234 (45 ms)` or `... rejected (12/1) ...`.
/// XMRig prints the running accepted/rejected totals in the `(A/R)` pair on
/// every share submit. Returns `None` for non-share lines.
fn parse_share_counts(line: &str) -> Option<(u64, u64)> {
    if !line.contains("accepted") && !line.contains("rejected") {
        return None;
    }
    // Find the first `(<digits>/<digits>)` group.
    let open = line.find('(')?;
    let rest = &line[open + 1..];
    let close = rest.find(')')?;
    let inside = &rest[..close];
    let (a, r) = inside.split_once('/')?;
    let accepted = a.trim().parse::<u64>().ok()?;
    let rejected = r.trim().parse::<u64>().ok()?;
    Some((accepted, rejected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::miner::MinerLaunchPlan;
    use std::time::Instant;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    #[test]
    fn fresh_supervisor_is_stopped_with_zeroed_stats() {
        let s = MinerSupervisor::new();
        let st = s.stats();
        assert!(!st.running);
        assert_eq!(st.state, ProcState::Stopped);
        assert!(st.hashrate_hs.is_none());
        assert_eq!(st.accepted, 0);
        assert_eq!(st.rejected, 0);
        assert!(st.last_line.is_empty());
    }

    #[test]
    fn parses_10s_hashrate_from_speed_line() {
        // 10s figure present.
        assert_eq!(
            parse_hashrate_hs("miner    speed 10s/60s/15m 1234.5 1200.0 n/a H/s max 1300.0 H/s"),
            Some(1234.5)
        );
        // 10s is n/a → fall back to the 60s figure.
        assert_eq!(
            parse_hashrate_hs("miner    speed 10s/60s/15m n/a 980.0 n/a H/s"),
            Some(980.0)
        );
        // All n/a → no reading yet.
        assert_eq!(
            parse_hashrate_hs("miner    speed 10s/60s/15m n/a n/a n/a H/s"),
            None
        );
        // Non-speed line → ignored.
        assert_eq!(parse_hashrate_hs("net      new job from pool"), None);
    }

    #[test]
    fn parses_accepted_and_rejected_share_counts() {
        assert_eq!(
            parse_share_counts("net      accepted (12/0) diff 1234 (45 ms)"),
            Some((12, 0))
        );
        assert_eq!(
            parse_share_counts("net      rejected (30/2) diff 5000 (60 ms)"),
            Some((30, 2))
        );
        // No (A/R) group → None.
        assert_eq!(parse_share_counts("net      new job from pool diff 1000"), None);
        // Non-share line → None even with parens.
        assert_eq!(parse_share_counts("cpu      using profile (rx)"), None);
    }

    #[test]
    fn apply_log_line_updates_snapshot_via_sanitised_input() {
        let s = MinerSupervisor::new();
        {
            let mut g = s.inner.lock().unwrap();
            // ANSI-coloured share line is sanitised then parsed.
            apply_log_line(&mut g, "\u{1b}[1;32maccepted\u{1b}[0m (7/1) diff 900 (40 ms)");
            apply_log_line(&mut g, "miner    speed 10s/60s/15m 555.5 540.0 n/a H/s");
        }
        let st = s.stats();
        assert_eq!(st.accepted, 7);
        assert_eq!(st.rejected, 1);
        assert_eq!(st.hashrate_hs, Some(555.5));
        assert!(!st.last_line.contains('\u{1b}'));
    }

    #[cfg(unix)]
    #[test]
    fn start_then_stop_transitions_and_captures_shares() {
        let rt = rt();
        rt.block_on(async {
            // Stand-in "miner": emit an accepted-share line then idle, so we can
            // observe Running + parsed stats, then stop it cleanly.
            let plan = MinerLaunchPlan {
                program: std::path::PathBuf::from("/bin/sh"),
                args: vec![
                    "-c".into(),
                    "echo 'net      accepted (3/0) diff 100 (10 ms)'; \
                     echo 'miner    speed 10s/60s/15m 42.0 40.0 n/a H/s'; sleep 10"
                        .into(),
                ],
            };
            let s = MinerSupervisor::new();
            s.start(plan).expect("start");
            assert!(s.is_active());

            // Observe parsed stats from the emitted lines.
            let mut saw = false;
            for _ in 0..30 {
                let st = s.stats();
                if st.accepted == 3 && st.hashrate_hs == Some(42.0) {
                    saw = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(saw, "expected parsed accepted-share + hashrate from miner output");

            // Graceful stop.
            s.request_stop();
            let mut stopped = false;
            for _ in 0..40 {
                if s.stats().state == ProcState::Stopped {
                    stopped = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(stopped, "miner should reach Stopped after request_stop");
            assert!(!s.is_active());
            // On stop the live hashrate is cleared.
            assert!(s.stats().hashrate_hs.is_none());
        });
    }

    #[cfg(unix)]
    #[test]
    fn double_start_is_rejected_while_active() {
        let rt = rt();
        rt.block_on(async {
            let plan = MinerLaunchPlan {
                program: std::path::PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), "sleep 5".into()],
            };
            let s = MinerSupervisor::new();
            s.start(plan.clone()).expect("first start");
            assert!(s.start(plan).is_err(), "second start while active rejected");
            s.request_stop();
            for _ in 0..30 {
                if s.stats().state == ProcState::Stopped {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    #[cfg(unix)]
    #[test]
    fn unexpected_exit_lands_in_error_not_restart_loop() {
        let rt = rt();
        rt.block_on(async {
            let _ = Instant::now();
            let plan = MinerLaunchPlan {
                program: std::path::PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), "echo starting; exit 1".into()],
            };
            let s = MinerSupervisor::new();
            s.start(plan).expect("start");
            // It should settle in Error (no auto-restart) and stay there.
            let mut reached_error = false;
            for _ in 0..40 {
                if s.stats().state == ProcState::Error {
                    reached_error = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(reached_error, "unexpected miner exit should land in Error");
            assert!(!s.is_active());
        });
    }
}
