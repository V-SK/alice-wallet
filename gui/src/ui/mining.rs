use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use crate::miner::{self, RewardEvidenceStatus, WalletMiningStatus};
use crate::supervise::miner_supervisor::MinerStats;
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(wallet) = app.secrets.clone() else {
        return;
    };
    let reward_identity = app
        .selected_reward_identity()
        .unwrap_or_else(|| wallet.address.clone());

    let packet = match miner::rehearsal_status_packet(&reward_identity, None) {
        Ok(packet) => packet,
        Err(_) => {
            card(ui, |ui| {
                section_title(ui, app.t("mining.title"));
                error_banner(ui, app.t("mining.identity_error"));
            });
            return;
        }
    };

    section_title(ui, app.t("mining.title"));
    ui.horizontal(|ui| {
        heading(ui, app.t("mining.heading"));
        ui.add_space(10.0);
        // Prominent experimental ("测试中") badge next to the heading.
        if miner::MINING_EXPERIMENTAL {
            status_pill(ui, Tone::Warn, app.t("mining.experimental"));
        }
    });
    ui.add_space(4.0);
    subtle(ui, app.t("mining.subtitle"));
    ui.add_space(16.0);

    card_accent(ui, |ui| {
        section_title(ui, app.t("mining.route_title"));
        status_row(ui, app.t("mining.route"), app.t("mining.route_xmr"));
        ui.add_space(8.0);
        // Mining status as a coloured pill. Execution is OFF, so the engine never
        // reports "running"; the pill reflects evidence readiness, not live mining.
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(app.t("mining.status").to_uppercase())
                            .size(10.0)
                            .strong()
                            .color(THEME.text_dim),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_pill(
                            ui,
                            mining_tone(packet.mining_status),
                            mining_status_label(packet.mining_status, app),
                        );
                    });
                });
            });
        ui.add_space(8.0);
        status_row(
            ui,
            app.t("mining.reward_identity"),
            &shortened_address(&packet.route.reward_identity),
        );
        ui.add_space(8.0);
        status_row(
            ui,
            app.t("mining.worker_identity"),
            &packet.route.worker_identity,
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new(app.t("mining.route_note"))
                .size(12.0)
                .color(THEME.text_mid),
        );
    });

    ui.add_space(14.0);

    // Live miner snapshot, polled each frame from the supervisor.
    let stats = app.miner_stats.clone();
    let address_ready = app.selected_reward_identity().is_some();

    let mut want_start = false;
    let mut want_stop = false;
    card(ui, |ui| {
        section_title(ui, app.t("mining.controls_title"));
        ui.label(
            RichText::new(app.t("mining.controls_body"))
                .size(12.5)
                .color(THEME.text_mid),
        );
        ui.add_space(12.0);
        // Start is enabled only when execution is allowed, an Alice address is
        // ready, and the miner is not already active. Stop is enabled while
        // active. Disabled in isolated/mock modes.
        let isolated = app.qa_mock_mode || app.network_disabled;
        let can_run = miner::MINING_EXECUTION_ALLOWED && address_ready && !isolated;
        let start_enabled = can_run && !stats.running;
        let stop_enabled = stats.running;
        ui.horizontal(|ui| {
            if primary_button(ui, app.t("mining.start"), start_enabled, false).clicked() {
                want_start = true;
            }
            ui.add_space(8.0);
            if secondary_button(ui, app.t("mining.stop"), stop_enabled, false).clicked() {
                want_stop = true;
            }
        });
        if !address_ready {
            ui.add_space(8.0);
            ui.label(
                RichText::new(app.t("mining.identity_error"))
                    .size(12.0)
                    .color(THEME.warn),
            );
        }
    });
    if want_start {
        app.start_miner();
    }
    if want_stop {
        app.stop_miner();
    }

    ui.add_space(14.0);
    miner_engine_panel(ui, app, &stats);

    ui.add_space(14.0);
    rewards_panel(ui, app, &packet.rewards);
    ui.add_space(14.0);
    daily_history(ui, app);
}

/// Live miner readout: status pill + hashrate + accepted/rejected shares.
/// Reads the polled [`MinerStats`] snapshot; shows "— " when there is no figure.
fn miner_engine_panel(ui: &mut egui::Ui, app: &AliceWalletApp, stats: &MinerStats) {
    use crate::ui::widgets::proc_tone;

    // Keep the live readout ticking while the miner is active.
    if stats.running {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }

    card(ui, |ui| {
        section_title(ui, app.t("mining.engine_title"));
        ui.add_space(4.0);

        // Status pill.
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(app.t("mining.engine_state").to_uppercase())
                            .size(10.0)
                            .strong()
                            .color(THEME.text_dim),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_pill(ui, proc_tone(stats.state), engine_state_label(stats.state, app));
                    });
                });
            });
        ui.add_space(8.0);

        // Hashrate (H/s) — "— " until the first reading arrives.
        let hashrate = match stats.hashrate_hs {
            Some(hs) => format_hashrate(hs),
            None => "—".to_string(),
        };
        rewards_row(ui, app.t("mining.hashrate"), &hashrate);
        ui.add_space(8.0);
        rewards_row(ui, app.t("mining.accepted_shares"), &stats.accepted.to_string());
        ui.add_space(8.0);
        rewards_row(ui, app.t("mining.rejected_shares"), &stats.rejected.to_string());
    });
}

/// Human-readable hashrate, e.g. `1234.5 H/s` or `12.3 kH/s`.
fn format_hashrate(hs: f64) -> String {
    if hs >= 1_000_000.0 {
        format!("{:.2} MH/s", hs / 1_000_000.0)
    } else if hs >= 1_000.0 {
        format!("{:.2} kH/s", hs / 1_000.0)
    } else {
        format!("{:.1} H/s", hs)
    }
}

fn engine_state_label(state: crate::supervise::ProcState, app: &AliceWalletApp) -> &'static str {
    use crate::supervise::ProcState as P;
    match state {
        P::Running => app.t("mining.running"),
        P::Starting => app.t("mining.starting"),
        P::Stopping => app.t("mining.stopping"),
        P::Error => app.t("mining.engine_error"),
        P::Stopped => app.t("mining.stopped"),
    }
}

fn rewards_panel(ui: &mut egui::Ui, app: &AliceWalletApp, rewards: &miner::WalletRewardProjection) {
    card(ui, |ui| {
        section_title(ui, app.t("mining.rewards_title"));
        ui.add_space(4.0);
        ui.label(
            RichText::new(app.t("mining.rewards_cadence"))
                .size(12.0)
                .color(THEME.text_mid),
        );
        ui.add_space(12.0);

        rewards_row(
            ui,
            app.t("mining.estimated_rewards"),
            &rewards.estimated_rewards,
        );
        ui.add_space(8.0);
        rewards_row(
            ui,
            app.t("mining.confirmed_rewards"),
            &rewards.confirmed_rewards,
        );
        ui.add_space(8.0);
        rewards_row(
            ui,
            app.t("mining.pending_rewards"),
            &rewards.pending_rewards,
        );
        ui.add_space(8.0);
        rewards_row(ui, app.t("mining.held_rewards"), &rewards.held_rewards);
        ui.add_space(8.0);
        rewards_row(
            ui,
            app.t("mining.released_rewards"),
            &rewards.released_rewards,
        );
        ui.add_space(8.0);
        rewards_row(
            ui,
            app.t("mining.accepted_shares"),
            &rewards.accepted_shares.to_string(),
        );
        ui.add_space(8.0);
        rewards_row(
            ui,
            app.t("mining.rejected_shares"),
            &rewards.rejected_shares.to_string(),
        );
        ui.add_space(8.0);
        rewards_row(
            ui,
            app.t("mining.evidence_status"),
            evidence_status_label(rewards.evidence_status, app),
        );
        ui.add_space(8.0);
        let freshness = rewards
            .evidence_freshness_seconds
            .map(|value| format!("{}s", value))
            .unwrap_or_else(|| app.t("mining.evidence_unavailable").to_string());
        rewards_row(ui, app.t("mining.evidence_freshness"), &freshness);
        ui.add_space(8.0);
        rewards_row(ui, app.t("mining.daily_window"), &rewards.daily_window);
        ui.add_space(8.0);
        rewards_row(ui, app.t("mining.last_updated"), &rewards.last_updated_at);
    });
}

fn daily_history(ui: &mut egui::Ui, app: &AliceWalletApp) {
    card(ui, |ui| {
        section_title(ui, app.t("mining.daily_history"));
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(app.t("mining.daily_empty"))
                    .size(13.5)
                    .strong()
                    .color(THEME.text_hi),
            );
            ui.label(
                RichText::new(app.t("mining.daily_empty_hint"))
                    .size(12.0)
                    .color(THEME.text_mid),
            );
        });
    });
}

fn status_row(ui: &mut egui::Ui, label: &str, value: &str) {
    row(ui, label, value, true);
}

fn rewards_row(ui: &mut egui::Ui, label: &str, value: &str) {
    row(ui, label, value, false);
}

fn row(ui: &mut egui::Ui, label: &str, value: &str, mono_value: bool) {
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .stroke(Stroke::new(1.0, THEME.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(label.to_uppercase())
                        .size(10.0)
                        .strong()
                        .color(THEME.text_dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut text = RichText::new(value).size(12.0).color(THEME.text_hi);
                    if mono_value {
                        text = text.family(egui::FontFamily::Monospace);
                    }
                    ui.label(text);
                });
            });
        });
}

fn mining_tone(status: WalletMiningStatus) -> Tone {
    match status {
        WalletMiningStatus::EvidenceAvailable => Tone::Live,
        WalletMiningStatus::Preparing => Tone::Warn,
        WalletMiningStatus::EvidenceStale => Tone::Warn,
        WalletMiningStatus::Unavailable => Tone::Off,
    }
}

fn mining_status_label(status: WalletMiningStatus, app: &AliceWalletApp) -> &'static str {
    match status {
        WalletMiningStatus::Preparing => app.t("mining.status_preparing"),
        WalletMiningStatus::EvidenceAvailable => app.t("mining.status_available"),
        WalletMiningStatus::EvidenceStale => app.t("mining.status_stale"),
        WalletMiningStatus::Unavailable => app.t("mining.status_unavailable"),
    }
}

fn evidence_status_label(status: RewardEvidenceStatus, app: &AliceWalletApp) -> &'static str {
    match status {
        RewardEvidenceStatus::Pending => app.t("mining.evidence_pending"),
        RewardEvidenceStatus::Fresh => app.t("mining.evidence_fresh"),
        RewardEvidenceStatus::Stale => app.t("mining.evidence_stale"),
        RewardEvidenceStatus::Unavailable => app.t("mining.evidence_unavailable"),
    }
}
