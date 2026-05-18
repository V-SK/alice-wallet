use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use crate::miner::{self, RewardEvidenceStatus, WalletMiningStatus};
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(wallet) = app.secrets.clone() else {
        return;
    };

    let packet = match miner::rehearsal_status_packet(&wallet.address, None) {
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
    heading(ui, app.t("mining.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("mining.subtitle"));
    ui.add_space(16.0);

    card_accent(ui, |ui| {
        section_title(ui, app.t("mining.route_title"));
        status_row(ui, app.t("mining.route"), app.t("mining.route_xmr"));
        ui.add_space(8.0);
        status_row(
            ui,
            app.t("mining.status"),
            mining_status_label(packet.mining_status, app),
        );
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

    card(ui, |ui| {
        section_title(ui, app.t("mining.controls_title"));
        ui.label(
            RichText::new(app.t("mining.controls_body"))
                .size(12.5)
                .color(THEME.text_mid),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let _ = secondary_button(ui, app.t("mining.start_unavailable"), false, false);
            let _ = secondary_button(ui, app.t("mining.stop_unavailable"), false, false);
        });
    });

    ui.add_space(14.0);
    rewards_panel(ui, app, &packet.rewards);
    ui.add_space(14.0);
    daily_history(ui, app);
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
