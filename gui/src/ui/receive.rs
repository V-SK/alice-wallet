use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use crate::chain::NodeSyncState;
use eframe::egui::{self, Color32, ColorImage, RichText, Stroke, TextureOptions};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(secrets) = app.secrets.clone() else {
        return;
    };

    section_title(ui, app.t("receive.title"));
    heading(ui, app.t("receive.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("receive.subtitle"));
    ui.add_space(16.0);

    if app.node_sync.status != NodeSyncState::Synced {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(12.0).color(THEME.primary));
                ui.label(
                    RichText::new(app.t("receive.sync_warning"))
                        .size(12.5)
                        .color(THEME.text_mid),
                );
            });
        });
        ui.add_space(14.0);
    }

    card_accent(ui, |ui| {
        section_title(ui, app.t("receive.account_label"));
        ui.label(
            RichText::new(app.t("receive.primary_account"))
                .size(15.0)
                .strong()
                .color(THEME.text_hi),
        );
        ui.add_space(14.0);

        field_label(ui, app.t("receive.address_label"));
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .stroke(Stroke::new(1.0, THEME.border_accent))
            .show(ui, |ui| {
                copy_label(
                    ui,
                    &secrets.address,
                    &secrets.address,
                    &mut app.address_copied_at,
                    true,
                );
            });
        ui.add_space(12.0);
        subtle(ui, app.t("receive.copy_hint"));
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, app.t("receive.qr_title"));
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            if let Some(img) = render_qr(&secrets.address) {
                let tex = ui
                    .ctx()
                    .load_texture("receive_address_qr", img, TextureOptions::NEAREST);
                ui.image((tex.id(), egui::vec2(220.0, 220.0)));
            } else {
                ui.label(
                    RichText::new(app.t("receive.qr_unavailable"))
                        .size(12.0)
                        .color(THEME.danger),
                );
            }
        });
    });

    ui.add_space(14.0);
    card(ui, |ui| {
        section_title(ui, "Request labels");
        let requests = app.active_receive_requests();
        if requests.is_empty() {
            ui.label(
                RichText::new("No local request labels yet")
                    .size(12.5)
                    .color(THEME.text_mid),
            );
        } else {
            for request in requests {
                ui.add_space(6.0);
                request_row(
                    ui,
                    &request.label,
                    request.amount_hint.as_deref().unwrap_or("Open amount"),
                );
            }
        }
    });
}

fn request_row(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .stroke(Stroke::new(1.0, THEME.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(12.0).color(THEME.text_hi));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(value).size(12.0).color(THEME.text_mid));
                });
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
