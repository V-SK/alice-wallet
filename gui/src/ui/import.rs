use super::theme::{paint_backdrop, THEME};
use super::widgets::*;
use crate::app::{AliceWalletApp, AsyncAction, ImportTab, Phase};
use eframe::egui::{self, RichText};

pub fn render(ctx: &egui::Context, app: &mut AliceWalletApp) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(THEME.bg_base))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            paint_backdrop(ui, rect);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(28.0);
                    ui.add(
                        egui::Image::new(egui::include_image!("../../alice-logo-traced.svg"))
                            .max_height(32.0),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("IMPORT WALLET")
                            .size(11.0)
                            .extra_letter_spacing(2.8)
                            .color(THEME.primary),
                    );
                    ui.add_space(18.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(620.0);
                            card_accent(ui, |ui| {
                                heading(ui, app.t("auth.import_heading"));
                                ui.add_space(6.0);
                                if app.detected_wallet_path.is_some() {
                                    error_banner(ui, app.t("auth.import_overwrite_warn"));
                                    ui.add_space(10.0);
                                } else {
                                    subtle(ui, app.t("auth.import_subtitle"));
                                    ui.add_space(10.0);
                                }

                                // Tabs
                                ui.horizontal(|ui| {
                                    tab_btn(ui, app, ImportTab::Mnemonic, app.t("auth.tab_mnemonic"));
                                    ui.add_space(8.0);
                                    tab_btn(ui, app, ImportTab::SeedHex, app.t("auth.tab_seed"));
                                });
                                ui.add_space(14.0);

                                match app.import_tab {
                                    ImportTab::Mnemonic => render_mnemonic_tab(ui, app),
                                    ImportTab::SeedHex => render_seed_tab(ui, app),
                                }

                                ui.add_space(16.0);
                                ui.separator();
                                ui.add_space(14.0);

                                field_label(ui, app.t("auth.new_password"));
                                let pw_hint = app.t("auth.password_min_hint");
                                password_input(ui, &mut app.password_input, &mut app.password_visible, pw_hint);
                                ui.add_space(8.0);
                                strength_bar(ui, &app.password_input);
                                ui.add_space(12.0);

                                field_label(ui, app.t("auth.confirm_password"));
                                let mut confirm_vis = false;
                                let pw_hint2 = app.t("auth.password_repeat_hint");
                                password_input(ui, &mut app.confirm_password_input, &mut confirm_vis, pw_hint2);
                                ui.add_space(18.0);

                                let btn_label = if app.auth_busy { app.t("auth.importing") } else { app.t("auth.import_btn") };
                                if primary_button(ui, btn_label, !app.auth_busy, true).clicked() {
                                    submit_import(app);
                                }

                                if !app.auth_error.is_empty() {
                                    ui.add_space(12.0);
                                    error_banner(ui, &app.auth_error);
                                }
                            });

                            ui.add_space(14.0);
                            if ghost_button(ui, app.t("auth.back")).clicked() && !app.auth_busy {
                                if app.detected_wallet_path.is_some() {
                                    app.clear_mnemonic_inputs();
                                    app.clear_password_inputs();
                                    app.auth_error.clear();
                                    app.phase = Phase::WalletChoice;
                                } else {
                                    app.prepare_new_wallet();
                                }
                            }
                        },
                    );
                });
            });
        });
}

fn tab_btn(ui: &mut egui::Ui, app: &mut AliceWalletApp, tab: ImportTab, label: &str) {
    let active = app.import_tab == tab;
    let bg = if active { THEME.primary_dim } else { THEME.bg_panel_hi };
    let stroke = if active { THEME.border_accent } else { THEME.border };
    let text_color = if active { THEME.text_hi } else { THEME.text_mid };
    let resp = ui.add(
        egui::Button::new(RichText::new(label).size(12.5).strong().color(text_color))
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, stroke))
            .corner_radius(10)
            .min_size(egui::vec2(160.0, 34.0)),
    );
    if resp.clicked() {
        app.import_tab = tab;
        app.auth_error.clear();
    }
}

fn render_mnemonic_tab(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::same(12))
        .stroke(egui::Stroke::new(1.0, THEME.border))
        .show(ui, |ui| {
            section_title(ui, app.t("auth.paste_phrase"));
            let mut paste_text = app.mnemonic_words.join(" ");
            let resp = ui.add(
                egui::TextEdit::multiline(&mut paste_text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .hint_text(app.t("auth.paste_hint"))
                    .background_color(THEME.bg_input)
                    .margin(egui::vec2(10.0, 8.0)),
            );
            if resp.changed() {
                let cleaned: String = paste_text
                    .chars()
                    .map(|c| if c.is_alphabetic() || c.is_whitespace() { c } else { ' ' })
                    .collect();
                let words: Vec<String> = cleaned
                    .split_whitespace()
                    .filter(|s| s.chars().all(|c| c.is_alphabetic()))
                    .map(|s| s.to_lowercase())
                    .collect();
                app.mnemonic_words = vec![String::new(); 24];
                for (i, w) in words.iter().enumerate().take(24) {
                    app.mnemonic_words[i] = w.clone();
                }
            }
        });

    ui.add_space(14.0);
    section_title(ui, app.t("auth.or_type_word"));

    egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
        egui::Grid::new("mn_grid")
            .num_columns(4)
            .spacing([10.0, 10.0])
            .show(ui, |ui| {
                for i in 0..24 {
                    egui::Frame::NONE
                        .fill(THEME.bg_input)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .stroke(egui::Stroke::new(1.0, THEME.border))
                        .show(ui, |ui| {
                            ui.set_min_width(118.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("{:02}", i + 1))
                                        .size(10.0)
                                        .color(THEME.text_dim),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.mnemonic_words[i])
                                        .desired_width(85.0)
                                        .font(egui::TextStyle::Body),
                                );
                            });
                        });
                    if (i + 1) % 4 == 0 {
                        ui.end_row();
                    }
                }
            });
    });
}

fn render_seed_tab(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::same(12))
        .stroke(egui::Stroke::new(1.0, THEME.border))
        .show(ui, |ui| {
            let seed_label = app.t("auth.seed_label");
            let seed_hint = app.t("auth.seed_hint");
            section_title(ui, seed_label);
            ui.add(
                egui::TextEdit::multiline(&mut app.seed_hex_input)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .password(true)
                    .hint_text(seed_hint)
                    .font(egui::TextStyle::Monospace)
                    .background_color(THEME.bg_input)
                    .margin(egui::vec2(10.0, 8.0)),
            );
        });
}

fn submit_import(app: &mut AliceWalletApp) {
    if app.password_input.len() < 12 {
        app.auth_error = app.t("auth.password_too_short").to_string();
        return;
    }
    if app.password_input != app.confirm_password_input {
        app.auth_error = app.t("auth.password_mismatch").to_string();
        return;
    }
    let target = app.wallet_path.clone();

    match app.import_tab {
        ImportTab::Mnemonic => {
            let phrase = app
                .mnemonic_words
                .iter()
                .filter(|w| !w.is_empty())
                .cloned()
                .collect::<Vec<String>>()
                .join(" ")
                .trim()
                .to_string();
            let count = phrase.split_whitespace().count();
            if !matches!(count, 12 | 15 | 18 | 21 | 24) {
                app.auth_error = format!(
                    "Mnemonic must be 12, 15, 18, 21 or 24 words (you have {})",
                    count
                );
                return;
            }
            use bip39::Mnemonic;
            match Mnemonic::parse(&phrase) {
                Ok(_) => {
                    app.auth_busy = true;
                    app.auth_error.clear();
                    let _ = app.tx.send(AsyncAction::Import(
                        phrase,
                        app.password_input.clone(),
                        target,
                    ));
                }
                Err(e) => {
                    app.auth_error = format!("{}: {}", app.t("auth.invalid_mnemonic"), e);
                }
            }
        }
        ImportTab::SeedHex => {
            let trimmed = app
                .seed_hex_input
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            if trimmed.len() != 64 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                app.auth_error = app.t("auth.invalid_seed").to_string();
                return;
            }
            app.auth_busy = true;
            app.auth_error.clear();
            let _ = app.tx.send(AsyncAction::ImportSeedHex(
                app.seed_hex_input.clone(),
                app.password_input.clone(),
                target,
            ));
        }
    }
}
