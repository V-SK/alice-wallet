//! `NodeSupervisor` — owns the embedded Alice node child process and exposes a
//! cloneable [`ProcStatus`] snapshot. Runs entirely on the wallet's worker side
//! (the tokio runtime + worker thread set up in `app.rs`); the GUI only ever
//! reads snapshots over the existing `AsyncResult` channel.
//!
//! Responsibilities (plan §1.2):
//! - spawn / own / stop the node child via [`super::child`]
//! - track lifecycle state + sanitised log tail
//! - apply the bounded [`super::RestartPolicy`] on unexpected exit
//! - guarantee a node crash can never lock/corrupt the wallet (this type holds
//!   NO custody state)

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::mpsc::unbounded_channel;

use super::child::{spawn_supervised, LogLine, OwnedChild};
use super::{LogRing, ProcState, ProcStatus, RestartPolicy};
use crate::node::NodeLaunchPlan;

/// Grace period for a graceful node stop before SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(8);

/// Shared, lock-guarded supervisor state. Cloneable handle; the heavy bits live
/// behind the mutex so both the supervision loop and the snapshot reader share
/// one source of truth.
#[derive(Clone)]
pub struct NodeSupervisor {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    state: ProcState,
    pid: Option<u32>,
    last_exit_code: Option<i32>,
    message: Option<String>,
    logs: LogRing,
    restarts: RestartPolicy,
    /// The launch plan in effect (so we can auto-restart with the same args).
    plan: Option<NodeLaunchPlan>,
    /// Extra non-secret env to pass to the child.
    envs: Vec<(String, String)>,
    /// PID file path.
    pid_file: Option<PathBuf>,
    /// User explicitly requested stop — suppresses auto-restart.
    stop_requested: bool,
    /// Generation counter; bumped on every start/stop so stale supervision
    /// loops from a previous child exit without touching newer state.
    generation: u64,
}

impl Default for NodeSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeSupervisor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: ProcState::Stopped,
                pid: None,
                last_exit_code: None,
                message: None,
                logs: LogRing::new(),
                restarts: RestartPolicy::new(),
                plan: None,
                envs: Vec::new(),
                pid_file: None,
                stop_requested: false,
                generation: 0,
            })),
        }
    }

    /// Current UI-safe snapshot.
    pub fn status(&self) -> ProcStatus {
        let mut g = self.inner.lock().expect("node supervisor mutex");
        let restarts_used = g.restarts.used(Instant::now());
        ProcStatus {
            state: g.state,
            pid: g.pid,
            last_exit_code: g.last_exit_code,
            message: g.message.clone(),
            log_tail: g.logs.tail(),
            restarts_used,
        }
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().expect("mutex").state.is_active()
    }

    /// Start (or restart) the node from a validated launch plan. Spawns the
    /// child on the current tokio runtime and a supervision task that watches
    /// for exit + applies the restart policy. `manual` resets the restart
    /// budget (a user-initiated start).
    pub fn start(
        &self,
        plan: NodeLaunchPlan,
        envs: Vec<(String, String)>,
        pid_file: Option<PathBuf>,
        manual: bool,
    ) -> Result<(), String> {
        {
            let mut g = self.inner.lock().expect("mutex");
            // Reject an external start while a process is genuinely live
            // (Running) or being torn down (Stopping). The internal restart
            // path uses [`spawn_now`] directly and is exempt.
            if matches!(g.state, ProcState::Running | ProcState::Stopping) {
                return Err("node is already running".into());
            }
            if manual {
                g.restarts.reset();
            }
        }
        self.spawn_now(plan, envs, pid_file)
    }

    /// Internal spawn: transitions to `Starting`, bumps generation, launches the
    /// child, and starts the supervision + log tasks. Used by both the public
    /// [`start`] and the auto-restart path (which has already accounted for the
    /// restart budget and must not be blocked by the active-state check).
    fn spawn_now(
        &self,
        plan: NodeLaunchPlan,
        envs: Vec<(String, String)>,
        pid_file: Option<PathBuf>,
    ) -> Result<(), String> {
        {
            let mut g = self.inner.lock().expect("mutex");
            g.state = ProcState::Starting;
            g.message = None;
            g.stop_requested = false;
            g.plan = Some(plan.clone());
            g.envs = envs.clone();
            g.pid_file = pid_file.clone();
            g.generation += 1;
        }

        // Ensure base-path exists before launch.
        if let Err(e) = std::fs::create_dir_all(&plan.base_path) {
            let mut g = self.inner.lock().expect("mutex");
            g.state = ProcState::Error;
            g.message = Some(format!("cannot create node data dir: {e}"));
            return Err(g.message.clone().unwrap());
        }

        let (log_tx, mut log_rx) = unbounded_channel::<LogLine>();
        let owned = match spawn_supervised(
            &plan.program,
            &plan.args,
            &envs,
            pid_file.as_deref(),
            log_tx,
        ) {
            Ok(c) => c,
            Err(e) => {
                let mut g = self.inner.lock().expect("mutex");
                g.state = ProcState::Error;
                g.message = Some(format!("failed to start node: {e}"));
                return Err(g.message.clone().unwrap());
            }
        };

        let pid = owned.pid();
        let gen = {
            let mut g = self.inner.lock().expect("mutex");
            g.pid = Some(pid);
            g.state = ProcState::Running;
            g.generation
        };

        // Log pump → ring.
        let inner_for_logs = self.inner.clone();
        tokio::spawn(async move {
            while let Some(line) = log_rx.recv().await {
                let mut g = inner_for_logs.lock().expect("mutex");
                g.logs.push_raw(&line.text);
            }
        });

        // Supervision task: wait for exit, then decide restart vs error.
        let this = self.clone();
        tokio::spawn(async move {
            this.supervise_until_exit(owned, gen).await;
        });

        Ok(())
    }

    async fn supervise_until_exit(&self, mut owned: OwnedChild, gen: u64) {
        // Poll for exit (try_wait avoids consuming the child so stop() can also
        // operate; but here we own it, so we can just await a wait loop).
        loop {
            if let Some(code) = owned.try_exit_code() {
                self.on_child_exit(code, gen).await;
                return;
            }
            // If a stop was requested, perform the graceful stop and return.
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
                    g.last_exit_code = code;
                    g.message = None;
                }
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }

    async fn on_child_exit(&self, code: i32, gen: u64) {
        let (should_restart, backoff, plan, envs, pid_file) = {
            let mut g = self.inner.lock().expect("mutex");
            if g.generation != gen {
                return; // superseded by a newer start
            }
            g.last_exit_code = Some(code);
            g.pid = None;
            if g.stop_requested {
                g.state = ProcState::Stopped;
                g.message = None;
                return;
            }
            let now = Instant::now();
            if g.restarts.may_restart(now) {
                let backoff = g.restarts.record(now);
                g.state = ProcState::Starting;
                g.message = Some(format!(
                    "node exited (code {code}); restarting (attempt {}/{})",
                    g.restarts.used(now),
                    super::MAX_RESTARTS
                ));
                (
                    true,
                    backoff,
                    g.plan.clone(),
                    g.envs.clone(),
                    g.pid_file.clone(),
                )
            } else {
                g.state = ProcState::Error;
                g.message = Some(format!(
                    "node exited (code {code}); restart budget exhausted — restart manually"
                ));
                (false, Duration::ZERO, None, Vec::new(), None)
            }
        };

        if should_restart {
            if let Some(plan) = plan {
                tokio::time::sleep(backoff).await;
                // Re-check we weren't stopped during backoff.
                {
                    let g = self.inner.lock().expect("mutex");
                    if g.stop_requested || g.generation != gen {
                        return;
                    }
                }
                // Re-spawn directly (NOT start(), which would reject because we
                // are in the `Starting`-for-restart state). spawn_now bumps the
                // generation and starts a fresh supervision loop.
                let _ = self.spawn_now(plan, envs, pid_file);
            }
        }
    }

    /// Request a graceful stop. The supervision loop performs the actual
    /// SIGTERM→SIGKILL teardown on its next tick.
    pub fn request_stop(&self) {
        let mut g = self.inner.lock().expect("mutex");
        if matches!(g.state, ProcState::Running | ProcState::Starting) {
            g.stop_requested = true;
            g.state = ProcState::Stopping;
            g.restarts.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeLaunchPlan;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    fn sleep_plan(secs: u32, base: PathBuf) -> NodeLaunchPlan {
        // Use /bin/sh as a stand-in "node" so the supervisor is testable
        // without a real solochain binary.
        NodeLaunchPlan {
            program: PathBuf::from("/bin/sh"),
            args: vec!["-c".into(), format!("echo node-booting; sleep {secs}")],
            base_path: base,
            rpc_url: "ws://127.0.0.1:9955".into(),
        }
    }

    #[test]
    fn fresh_supervisor_is_stopped() {
        let s = NodeSupervisor::new();
        let st = s.status();
        assert_eq!(st.state, ProcState::Stopped);
        assert!(st.pid.is_none());
        assert!(st.log_tail.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn start_then_stop_transitions_and_captures_log() {
        let rt = rt();
        rt.block_on(async {
            let base = std::env::temp_dir().join(format!(
                "alice-nodesup-{}-{}",
                std::process::id(),
                Instant::now().elapsed().as_nanos()
            ));
            let s = NodeSupervisor::new();
            let pid_file = base.join("node.pid");
            s.start(
                sleep_plan(10, base.clone()),
                vec![],
                Some(pid_file.clone()),
                true,
            )
            .expect("start");

            // Becomes Running with a PID.
            assert!(s.is_active());
            let st = s.status();
            assert_eq!(st.state, ProcState::Running);
            assert!(st.pid.is_some());

            // Captures the boot log line.
            let mut saw_log = false;
            for _ in 0..20 {
                if s.status()
                    .log_tail
                    .iter()
                    .any(|l| l.contains("node-booting"))
                {
                    saw_log = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(saw_log, "expected captured boot log line");

            // Graceful stop.
            s.request_stop();
            let mut stopped = false;
            for _ in 0..40 {
                if s.status().state == ProcState::Stopped {
                    stopped = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(stopped, "node should reach Stopped after request_stop");
            assert!(s.status().pid.is_none());
            // PID file cleaned up.
            assert!(super::super::child::read_pid_file(&pid_file).is_none());
            let _ = std::fs::remove_dir_all(&base);
        });
    }

    #[cfg(unix)]
    #[test]
    fn double_start_is_rejected_while_active() {
        let rt = rt();
        rt.block_on(async {
            let base = std::env::temp_dir().join(format!(
                "alice-nodesup2-{}-{}",
                std::process::id(),
                Instant::now().elapsed().as_nanos()
            ));
            let s = NodeSupervisor::new();
            s.start(sleep_plan(5, base.clone()), vec![], None, true)
                .expect("first start");
            let err = s.start(sleep_plan(5, base.clone()), vec![], None, true);
            assert!(err.is_err(), "second start while active must be rejected");
            s.request_stop();
            // allow teardown
            for _ in 0..30 {
                if s.status().state == ProcState::Stopped {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let _ = std::fs::remove_dir_all(&base);
        });
    }

    #[cfg(unix)]
    #[test]
    fn crash_within_budget_auto_restarts() {
        let rt = rt();
        rt.block_on(async {
            let base = std::env::temp_dir().join(format!(
                "alice-nodesup3-{}-{}",
                std::process::id(),
                Instant::now().elapsed().as_nanos()
            ));
            let s = NodeSupervisor::new();
            // A process that exits almost immediately => triggers restart path.
            let plan = NodeLaunchPlan {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".into(), "echo crashing; exit 1".into()],
                base_path: base.clone(),
                rpc_url: "ws://127.0.0.1:9955".into(),
            };
            s.start(plan, vec![], None, true).expect("start");

            // It should consume restart budget and eventually land in Error
            // (budget = MAX_RESTARTS), never locking up. With backoff 2+4+8s the
            // cumulative time to give up is ~14s, so poll generously (>20s).
            let mut reached_error = false;
            for _ in 0..240 {
                let st = s.status();
                if st.state == ProcState::Error {
                    reached_error = true;
                    assert!(st.message.unwrap_or_default().contains("budget"));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            assert!(
                reached_error,
                "repeated crash should exhaust budget and land in Error"
            );
            let _ = std::fs::remove_dir_all(&base);
        });
    }
}
