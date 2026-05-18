use super::theme::THEME;
use super::widgets::*;
use crate::app::{AliceWalletApp, Page};
use eframe::egui::{self, Color32, ColorImage, CornerRadius, RichText, Stroke, TextureOptions};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(secrets) = app.secrets.clone() else {
        return;
    };

    ui.horizontal(|ui| {
        section_title(ui, app.t("dash.overview"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let refresh_label = if app.refresh_pending > 0 {
                app.t("dash.refreshing")
            } else {
                app.t("dash.refresh")
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(refresh_label).size(12.5).color(THEME.text_hi))
                        .fill(THEME.bg_panel_hi)
                        .stroke(Stroke::new(1.0, THEME.border_accent))
                        .corner_radius(10)
                        .min_size(egui::vec2(118.0, 34.0)),
                )
                .clicked()
                && app.refresh_pending == 0
            {
                app.start_refresh(&secrets.address);
            }
        });
    });
    ui.add_space(6.0);

    // Top row: Balance card (big) + node sync summary.
    ui.horizontal_top(|ui| {
        let total_w = ui.available_width();
        let left_w = (total_w * 0.62).max(420.0);
        ui.allocate_ui_with_layout(
            egui::vec2(left_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(left_w);
                balance_card(ui, app, &secrets);
            },
        );
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            node_sync_summary(ui, app);
        });
    });

    if app.show_receive_qr {
        ui.add_space(18.0);
        receive_qr_card(ui, app, &secrets.address);
    }

    ui.add_space(18.0);

    // Recent activity
    card(ui, |ui| {
        ui.horizontal(|ui| {
            section_title(ui, app.t("dash.recent"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ghost_button(ui, app.t("dash.view_all")).clicked() {
                    app.page = Page::History;
                }
            });
        });
        ui.add_space(4.0);

        if app.history.is_empty() {
            ui.add_space(18.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(app.t("dash.no_tx"))
                        .size(13.0)
                        .color(THEME.text_dim),
                );
            });
            ui.add_space(12.0);
        } else {
            let recent: Vec<_> = app.history.iter().take(5).cloned().collect();
            for rec in recent.iter() {
                super::history_view::render_row(ui, rec, app);
            }
        }
    });

    if let Some(err) = &app.sync_error.clone() {
        ui.add_space(14.0);
        error_banner(ui, err);
    }
}

fn balance_card(
    ui: &mut egui::Ui,
    app: &mut AliceWalletApp,
    secrets: &crate::crypto::WalletSecrets,
) {
    card_accent(ui, |ui| {
        section_title(ui, app.t("dash.total_balance"));
        ui.horizontal(|ui| {
            match app.balance {
                Some(b) => {
                    ui.label(
                        RichText::new(format_token(b))
                            .size(44.0)
                            .strong()
                            .color(THEME.primary),
                    );
                }
                None => {
                    if app.refresh_pending > 0 {
                        ui.spinner();
                    } else {
                        ui.label(RichText::new("—").size(44.0).strong().color(THEME.text_dim));
                    }
                }
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new("ALICE")
                    .size(15.0)
                    .color(THEME.text_mid)
                    .strong(),
            );
        });

        ui.add_space(14.0);

        // Address row
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(app.t("dash.address"))
                            .size(10.0)
                            .color(THEME.text_dim)
                            .strong(),
                    );
                    ui.add_space(8.0);
                    copy_label(
                        ui,
                        &secrets.address,
                        &secrets.address,
                        &mut app.address_copied_at,
                        true,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let qr_label = if app.show_receive_qr {
                            app.t("dash.hide_qr")
                        } else {
                            app.t("dash.show_qr")
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(qr_label).size(11.0).color(THEME.primary_hi),
                                )
                                .fill(THEME.bg_panel_hi)
                                .stroke(Stroke::new(1.0, THEME.border_accent))
                                .corner_radius(8)
                                .min_size(egui::vec2(80.0, 26.0)),
                            )
                            .clicked()
                        {
                            app.show_receive_qr = !app.show_receive_qr;
                        }
                    });
                });
            });

        ui.add_space(16.0);

        // Action buttons
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 16.0) / 2.0;
            if ui
                .add_sized(
                    egui::vec2(w, 44.0),
                    egui::Button::new(
                        RichText::new(app.t("dash.receive_btn"))
                            .size(14.0)
                            .strong()
                            .color(Color32::from_rgb(10, 6, 2)),
                    )
                    .fill(THEME.primary)
                    .corner_radius(10),
                )
                .clicked()
            {
                app.page = Page::Receive;
            }
            ui.add_space(12.0);
            if ui
                .add_sized(
                    egui::vec2(w, 44.0),
                    egui::Button::new(
                        RichText::new(app.t("dash.send_review_btn"))
                            .size(14.0)
                            .color(THEME.text_hi),
                    )
                    .fill(THEME.bg_panel_hi)
                    .stroke(Stroke::new(1.0, THEME.border_accent))
                    .corner_radius(10),
                )
                .clicked()
            {
                app.page = Page::Send;
            }
        });
    });
}

fn receive_qr_card(ui: &mut egui::Ui, app: &mut AliceWalletApp, address: &str) {
    card(ui, |ui| {
        section_title(ui, app.t("dash.receive_qr"));
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            if let Some(img) = render_qr(address) {
                let tex = ui
                    .ctx()
                    .load_texture("address_qr", img, TextureOptions::NEAREST);
                ui.image((tex.id(), egui::vec2(220.0, 220.0)));
            } else {
                ui.label(
                    RichText::new("QR generation failed")
                        .size(11.0)
                        .color(THEME.danger),
                );
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(address)
                    .size(11.0)
                    .family(egui::FontFamily::Monospace)
                    .color(THEME.text_mid),
            );
        });
    });
}

fn render_qr(data: &str) -> Option<ColorImage> {
    use qrcode::{EcLevel, QrCode};
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M).ok()?;
    let modules = code.to_colors();
    let width = code.width();
    if modules.len() != width * width {
        return None;
    }
    let scale = 6usize;
    let border = 2usize;
    let img_size = (width + border * 2) * scale;
    let mut pixels = vec![Color32::WHITE; img_size * img_size];
    for y in 0..width {
        for x in 0..width {
            let dark = matches!(modules[y * width + x], qrcode::Color::Dark);
            if !dark {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let py = (y + border) * scale + dy;
                    let px = (x + border) * scale + dx;
                    pixels[py * img_size + px] = Color32::from_rgb(10, 6, 2);
                }
            }
        }
    }
    Some(ColorImage {
        size: [img_size, img_size],
        source_size: egui::vec2(img_size as f32, img_size as f32),
        pixels,
    })
}

fn node_sync_summary(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    card(ui, |ui| {
        section_title(ui, app.t("sync.title"));
        ui.add_space(4.0);

        sync_row(
            ui,
            app.t("sync.status"),
            app.t(app.node_sync.status_i18n_key()),
        );
        ui.add_space(10.0);
        sync_row(ui, app.t("sync.mode"), app.node_sync.sync_mode.label());
        ui.add_space(10.0);
        let progress = app
            .node_sync
            .progress_percent
            .map(|p| format!("{:.1}%", p))
            .unwrap_or_else(|| app.t("sync.unavailable").to_string());
        sync_row(ui, app.t("sync.progress"), &progress);
        ui.add_space(10.0);
        let remaining = app
            .node_sync
            .remaining_blocks
            .map(|n| n.to_string())
            .unwrap_or_else(|| app.t("sync.unavailable").to_string());
        sync_row(ui, app.t("sync.remaining"), &remaining);
        ui.add_space(10.0);
        let peers = app
            .node_sync
            .peers_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| app.node_sync.network_status.clone());
        sync_row(ui, app.t("sync.network"), &peers);

        if let Some(reason) = app.node_sync.fail_closed_reason.as_deref() {
            ui.add_space(12.0);
            error_banner(ui, &product_sync_reason(reason, app));
        }
    });
}

fn sync_row(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::same(12))
        .stroke(Stroke::new(1.0, THEME.border))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(11.0)
                    .strong()
                    .color(THEME.text_dim),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(value)
                    .size(14.0)
                    .strong()
                    .color(THEME.text_hi),
            );
        });
}

fn product_sync_reason(reason: &str, app: &AliceWalletApp) -> String {
    match reason {
        "missing_current_height" | "missing_target_height" | "missing_freshness" => {
            app.t("sync.reason_incomplete").to_string()
        }
        "stale_node_evidence" => app.t("sync.reason_stale").to_string(),
        "node_offline" | "node_status_timeout" => app.t("sync.reason_offline").to_string(),
        _ => app.t("sync.reason_not_ready").to_string(),
    }
}
