use super::theme::{paint_backdrop, THEME};
use super::widgets::*;
use crate::app::{AliceWalletApp, AsyncAction, Phase};
use eframe::egui::{self, RichText};

fn auth_shell<F: FnOnce(&mut egui::Ui)>(ctx: &egui::Context, content: F) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(THEME.bg_base))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            paint_backdrop(ui, rect);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 36.0).max(0.0) / 2.0);
                        ui.add(
                            egui::Image::new(egui::include_image!("../../alice-logo-traced.svg"))
                                .max_height(36.0),
                        );
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("ALICE WALLET")
                            .size(11.0)
                            .extra_letter_spacing(3.0)
                            .color(THEME.primary),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("Not all intelligence bends the knee.")
                            .size(11.5)
                            .italics()
                            .color(THEME.text_dim),
                    );
                    ui.add_space(24.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(460.0);
                            content(ui);
                        },
                    );
                });
            });
        });
}

pub fn render_choice(ctx: &egui::Context, app: &mut AliceWalletApp) {
    auth_shell(ctx, |ui| {
        card_accent(ui, |ui| {
            heading(ui, "Local wallet detected");
            ui.add_space(6.0);
            subtle(
                ui,
                "Choose whether to unlock the detected wallet file, import a recovery phrase, or create a new wallet.",
            );
            ui.add_space(18.0);

            if let Some(path) = &app.detected_wallet_path {
                egui::Frame::NONE
                    .fill(THEME.bg_panel_hi)
                    .corner_radius(10)
                    .inner_margin(egui::Margin::same(12))
                    .stroke(egui::Stroke::new(1.0, THEME.border))
                    .show(ui, |ui| {
                        field_label(ui, "FILE");
                        ui.label(
                            RichText::new(path.display().to_string())
                                .size(11.5)
                                .family(egui::FontFamily::Monospace)
                                .color(THEME.text_hi),
                        );
                        if let Some(p) = &app.payload {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("Format v{} · {}", p.version, p.kdf))
                                    .size(11.0)
                                    .color(THEME.text_dim),
                            );
                        }
                    });
            }

            ui.add_space(18.0);
            if primary_button(ui, "Unlock Existing Wallet", true, true).clicked() {
                app.use_detected_wallet();
            }
            ui.add_space(8.0);
            if secondary_button(ui, "Import Recovery Phrase", true, true).clicked() {
                app.prepare_import_wallet();
            }
            ui.add_space(8.0);
            if secondary_button(ui, "Create New Wallet", true, true).clicked() {
                app.prepare_new_wallet();
            }
        });
    });
}

pub fn render_unlock(ctx: &egui::Context, app: &mut AliceWalletApp) {
    auth_shell(ctx, |ui| {
        card_accent(ui, |ui| {
            heading(ui, "Unlock wallet");
            ui.add_space(6.0);
            subtle(ui, "Enter the password that encrypts this wallet file.");
            ui.add_space(22.0);

            field_label(ui, "PASSWORD");
            let resp = password_input(
                ui,
                &mut app.password_input,
                &mut app.password_visible,
                "••••••••",
            );
            ui.add_space(18.0);

            let blocked = app
                .unlock_block_until
                .map(|t| t > std::time::Instant::now())
                .unwrap_or(false);
            let btn_label = if app.auth_busy {
                "Decrypting…"
            } else if blocked {
                "Please wait…"
            } else {
                "Unlock"
            };
            let clicked = primary_button(ui, btn_label, !app.auth_busy && !blocked, true).clicked();
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if (clicked || enter) && !app.auth_busy && !blocked {
                if let Some(payload) = &app.payload {
                    app.auth_busy = true;
                    app.auth_error.clear();
                    let _ = app.tx.send(AsyncAction::Unlock(
                        payload.clone(),
                        app.password_input.clone(),
                    ));
                }
            }

            if !app.auth_error.is_empty() {
                ui.add_space(12.0);
                error_banner(ui, &app.auth_error);
            }

            if app.detected_wallet_path.is_some() {
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ghost_button(ui, "← Wallet options").clicked() {
                        app.clear_password_inputs();
                        app.auth_error.clear();
                        app.phase = Phase::WalletChoice;
                    }
                    if ghost_button(ui, "Import phrase").clicked() {
                        app.prepare_import_wallet();
                    }
                });
            }
        });
    });
}
