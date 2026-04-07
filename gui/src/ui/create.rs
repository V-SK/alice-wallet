use super::theme::{paint_backdrop, THEME};
use super::widgets::*;
use crate::app::{AliceWalletApp, AsyncAction, Phase};
use eframe::egui::{self, RichText};

pub fn render(ctx: &egui::Context, app: &mut AliceWalletApp) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(THEME.bg_base))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            paint_backdrop(ui, rect);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(36.0);
                    ui.add(
                        egui::Image::new(egui::include_image!("../../alice-logo-traced.svg"))
                            .max_height(34.0),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("CREATE NEW WALLET")
                            .size(11.0)
                            .extra_letter_spacing(2.8)
                            .color(THEME.primary),
                    );
                    ui.add_space(22.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(460.0);
                            card_accent(ui, |ui| {
                                heading(ui, "Choose a password");
                                ui.add_space(6.0);
                                subtle(ui, "This password encrypts your wallet on this machine. Use at least 12 characters; a passphrase is best.");
                                ui.add_space(22.0);

                                field_label(ui, "PASSWORD");
                                password_input(ui, &mut app.password_input, &mut app.password_visible, "at least 12 characters");
                                ui.add_space(8.0);
                                strength_bar(ui, &app.password_input);
                                ui.add_space(14.0);

                                field_label(ui, "CONFIRM PASSWORD");
                                let mut confirm_vis = false;
                                password_input(ui, &mut app.confirm_password_input, &mut confirm_vis, "repeat password");
                                ui.add_space(18.0);

                                let label = if app.auth_busy { "Generating…" } else { "Create Wallet" };
                                if primary_button(ui, label, !app.auth_busy, true).clicked() {
                                    if app.password_input.len() < 12 {
                                        app.auth_error = "Password must be at least 12 characters".into();
                                    } else if app.password_input != app.confirm_password_input {
                                        app.auth_error = "Passwords do not match".into();
                                    } else {
                                        use bip39::Mnemonic;
                                        use rand::RngCore;
                                        let mut entropy = [0u8; 32];
                                        rand::thread_rng().fill_bytes(&mut entropy);
                                        let mnemonic = Mnemonic::from_entropy(&entropy).expect("32 bytes -> mnemonic");
                                        let phrase = mnemonic.words().collect::<Vec<&str>>().join(" ");
                                        app.auth_busy = true;
                                        app.auth_error.clear();
                                        let _ = app.tx.send(AsyncAction::Create(phrase, app.password_input.clone()));
                                    }
                                }

                                if !app.auth_error.is_empty() {
                                    ui.add_space(12.0);
                                    error_banner(ui, &app.auth_error);
                                }

                                ui.add_space(18.0);
                                ui.separator();
                                ui.add_space(12.0);
                                subtle(ui, "Already have a recovery phrase?");
                                ui.add_space(8.0);
                                if secondary_button(ui, "Import Wallet Instead", !app.auth_busy, true).clicked() {
                                    app.prepare_import_wallet();
                                }

                                if app.detected_wallet_path.is_some() {
                                    ui.add_space(10.0);
                                    if ghost_button(ui, "← Back to wallet options").clicked() {
                                        app.clear_password_inputs();
                                        app.auth_error.clear();
                                        app.phase = Phase::WalletChoice;
                                    }
                                }
                            });
                        },
                    );
                });
            });
        });
}
