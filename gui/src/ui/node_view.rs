//! The Node page — manage the embedded / remote Alice node.
//!
//! Mirrors Monero-GUI's "Daemon" / Bitcoin-Core's node management: choose
//! Local-embedded / Remote / Offline; for the local node show process state,
//! PID, Start/Stop/Restart, a sanitised log tail, and the reused
//! `NodeSyncSnapshot` (height / peers / progress / fail-closed reason).

use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use crate::node::{self, NodeMode};
use crate::supervise::ProcState;
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    section_title(ui, app.t("node.title"));
    heading(ui, app.t("node.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("node.subtitle"));
    ui.add_space(16.0);

    mode_card(ui, app);
    ui.add_space(14.0);

    match app.settings.node.mode {
        NodeMode::LocalEmbedded => {
            local_node_card(ui, app);
            ui.add_space(14.0);
            sync_card(ui, app);
            ui.add_space(14.0);
            log_card(ui, app);
        }
        NodeMode::Remote => {
            remote_card(ui, app);
            ui.add_space(14.0);
            sync_card(ui, app);
        }
        NodeMode::Offline => {
            offline_card(ui, app);
        }
    }
}

fn mode_card(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    card_accent(ui, |ui| {
        section_title(ui, app.t("node.mode_title"));
        ui.add_space(8.0);

        let current = app.settings.node.mode;
        let mut selected = current;

        mode_option(
            ui,
            app,
            &mut selected,
            NodeMode::LocalEmbedded,
            "node.mode_local",
            "node.mode_local_desc",
        );
        ui.add_space(6.0);
        mode_option(
            ui,
            app,
            &mut selected,
            NodeMode::Remote,
            "node.mode_remote",
            "node.mode_remote_desc",
        );
        ui.add_space(6.0);
        mode_option(
            ui,
            app,
            &mut selected,
            NodeMode::Offline,
            "node.mode_offline",
            "node.mode_offline_desc",
        );

        if selected != current {
            // Switching mode: stop a running local node if leaving local mode.
            if current == NodeMode::LocalEmbedded && app.node_proc.state.is_active() {
                app.stop_embedded_node();
            }
            app.settings.node.mode = selected;
            let _ = app.save_settings();
            // Force a fresh sync poll against the new endpoint.
            app.last_block_poll = None;
            app.last_data_poll = None;
        }
    });
}

fn mode_option(
    ui: &mut egui::Ui,
    app: &AliceWalletApp,
    selected: &mut NodeMode,
    this: NodeMode,
    label_key: &str,
    desc_key: &str,
) {
    let is_sel = *selected == this;
    let bg = if is_sel {
        THEME.primary_dim
    } else {
        THEME.bg_panel_hi
    };
    let stroke = if is_sel {
        Stroke::new(1.0, THEME.border_accent)
    } else {
        Stroke::new(1.0, THEME.border)
    };
    let resp = egui::Frame::NONE
        .fill(bg)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .stroke(stroke)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let dot = if is_sel { "●" } else { "○" };
                    ui.label(RichText::new(dot).size(13.0).color(if is_sel {
                        THEME.primary
                    } else {
                        THEME.text_dim
                    }));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(app.t(label_key))
                            .size(13.5)
                            .strong()
                            .color(THEME.text_hi),
                    );
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new(app.t(desc_key))
                        .size(11.5)
                        .color(THEME.text_mid),
                );
            });
        })
        .response
        .interact(egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *selected = this;
    }
}

fn local_node_card(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    // Binary-availability check (purely informational; the actual launch
    // re-checks and fails closed).
    let binary_ok = node::resolve_node_binary().is_ok();
    let bootnodes_empty = node::bundled_bootnodes().is_empty();

    card(ui, |ui| {
        section_title(ui, app.t("node.process_title"));
        ui.add_space(8.0);

        if !binary_ok {
            warn_banner(
                ui,
                app.t("node.binary_missing_title"),
                app.t("node.binary_missing_body"),
            );
            ui.add_space(8.0);
        }
        if bootnodes_empty {
            warn_banner(ui, app.t("node.no_bootnodes_warn"), "");
            ui.add_space(8.0);
        }

        // Snapshot the fields we need so we don't hold a borrow of
        // `app.node_proc` across the mutable Start/Stop button handlers.
        let proc_state = app.node_proc.state;
        let proc_pid = app.node_proc.pid;
        let restarts_used = app.node_proc.restarts_used;
        let proc_message = app.node_proc.message.clone();

        // Process state as a status pill (green running / amber starting / red error).
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(app.t("node.proc_state").to_uppercase())
                            .size(10.0)
                            .strong()
                            .color(THEME.text_dim),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_pill(ui, proc_tone(proc_state), app.t(proc_state.i18n_key()));
                    });
                });
            });
        ui.add_space(8.0);
        let pid = proc_pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".into());
        kv_row(ui, app.t("node.pid"), &pid, THEME.text_hi);
        ui.add_space(8.0);
        kv_row(
            ui,
            app.t("node.rpc_endpoint"),
            &app.settings.node.local_rpc_url(),
            THEME.text_hi,
        );
        ui.add_space(8.0);
        if restarts_used > 0 {
            kv_row(
                ui,
                app.t("node.restarts_used"),
                &restarts_used.to_string(),
                THEME.text_mid,
            );
            ui.add_space(8.0);
        }
        if let Some(msg) = &proc_message {
            ui.label(RichText::new(msg).size(11.5).color(THEME.text_mid));
            ui.add_space(8.0);
        }

        ui.label(
            RichText::new(app.t("node.local_warning"))
                .size(11.0)
                .italics()
                .color(THEME.text_dim),
        );
        ui.add_space(12.0);

        let active = proc_state.is_active();
        let can_start = binary_ok && !active && !app.qa_mock_mode && !app.network_disabled;
        let can_stop = matches!(proc_state, ProcState::Running | ProcState::Starting);
        ui.horizontal(|ui| {
            if primary_button(ui, app.t("node.start"), can_start, false).clicked() {
                app.start_embedded_node();
            }
            ui.add_space(8.0);
            if danger_button(ui, app.t("node.stop"), can_stop).clicked() {
                app.stop_embedded_node();
            }
        });
    });
}

fn remote_card(ui: &mut egui::Ui, app: &AliceWalletApp) {
    card(ui, |ui| {
        section_title(ui, app.t("node.mode_remote"));
        ui.add_space(8.0);
        kv_row(
            ui,
            app.t("node.rpc_endpoint"),
            &app.settings.rpc_url,
            THEME.text_hi,
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(app.t("set.connection_body"))
                .size(11.5)
                .color(THEME.text_mid),
        );
    });
}

fn offline_card(ui: &mut egui::Ui, app: &AliceWalletApp) {
    card(ui, |ui| {
        section_title(ui, app.t("node.mode_offline"));
        ui.add_space(8.0);
        ui.label(
            RichText::new(app.t("node.mode_offline_desc"))
                .size(12.5)
                .color(THEME.text_mid),
        );
    });
}

fn sync_card(ui: &mut egui::Ui, app: &AliceWalletApp) {
    let snap = &app.node_sync;
    card(ui, |ui| {
        section_title(ui, app.t("node.sync_title"));
        ui.add_space(8.0);
        // Sync status as a coloured pill (green synced / amber syncing / red error).
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(app.t("sync.status").to_uppercase())
                            .size(10.0)
                            .strong()
                            .color(THEME.text_dim),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_pill(ui, sync_tone(snap.status), app.t(snap.status_i18n_key()));
                    });
                });
            });
        ui.add_space(8.0);
        kv_row(
            ui,
            app.t("sync.mode"),
            snap.sync_mode.label(),
            THEME.text_mid,
        );
        ui.add_space(8.0);
        let height = snap
            .current_height
            .map(|h| format!("#{h}"))
            .unwrap_or_else(|| "—".into());
        kv_row(ui, app.t("shell.block"), &height, THEME.text_hi);
        ui.add_space(8.0);
        if let Some(target) = snap.target_height {
            kv_row(
                ui,
                app.t("sync.remaining"),
                &format!("{}", snap.remaining_blocks.unwrap_or(0)),
                THEME.text_mid,
            );
            let _ = target;
            ui.add_space(8.0);
        }
        if let Some(p) = snap.progress_percent {
            kv_row(
                ui,
                app.t("sync.progress"),
                &format!("{p:.1}%"),
                THEME.text_mid,
            );
            ui.add_space(8.0);
        }
        let peers = snap
            .peers_count
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".into());
        kv_row(ui, app.t("sync.network"), &peers, THEME.text_mid);
        if let Some(reason) = &snap.fail_closed_reason {
            ui.add_space(8.0);
            ui.label(RichText::new(reason).size(11.0).color(THEME.danger));
        }
    });
}

fn log_card(ui: &mut egui::Ui, app: &AliceWalletApp) {
    card(ui, |ui| {
        section_title(ui, app.t("node.log_title"));
        ui.add_space(8.0);
        let tail = &app.node_proc.log_tail;
        if tail.is_empty() {
            ui.label(
                RichText::new(app.t("node.log_empty"))
                    .size(12.0)
                    .color(THEME.text_dim),
            );
            return;
        }
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in tail.iter().rev().take(80).rev() {
                            ui.label(
                                RichText::new(line)
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(THEME.text_mid),
                            );
                        }
                    });
            });
    });
}

fn kv_row(ui: &mut egui::Ui, label: &str, value: &str, value_color: egui::Color32) {
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
                    ui.label(
                        RichText::new(value)
                            .size(12.0)
                            .family(egui::FontFamily::Monospace)
                            .color(value_color),
                    );
                });
            });
        });
}

fn warn_banner(ui: &mut egui::Ui, title: &str, body: &str) {
    egui::Frame::NONE
        .fill(THEME.danger_bg)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0, THEME.danger))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(title)
                        .size(12.5)
                        .strong()
                        .color(THEME.text_hi),
                );
                if !body.is_empty() {
                    ui.add_space(2.0);
                    ui.label(RichText::new(body).size(11.0).color(THEME.text_mid));
                }
            });
        });
}
