use super::theme::{paint_backdrop, THEME};
use super::widgets::*;
use crate::app::{AliceWalletApp, Page, Phase};
use crate::crypto;
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
                        RichText::new("BACK UP YOUR RECOVERY PHRASE")
                            .size(11.0)
                            .extra_letter_spacing(2.6)
                            .color(THEME.primary),
                    );
                    ui.add_space(18.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(620.0);
                            card_accent(ui, |ui| {
                                heading(ui, "Save these 24 words in order");
                                ui.add_space(6.0);
                                subtle(ui, "This is the ONLY way to recover your wallet. Anyone with these words owns the wallet. Write them down offline.");
                                ui.add_space(18.0);

                                if !app.auth_error.is_empty() {
                                    error_banner(ui, &app.auth_error);
                                    ui.add_space(14.0);
                                }

                                let words: Vec<String> = app.mnemonic_backup.split_whitespace().map(|s| s.to_string()).collect();

                                egui::Frame::NONE
                                    .fill(THEME.bg_panel_hi)
                                    .corner_radius(12)
                                    .inner_margin(egui::Margin::same(16))
                                    .stroke(egui::Stroke::new(1.0, THEME.border_accent))
                                    .show(ui, |ui| {
                                        egui::Grid::new("backup_grid")
                                            .num_columns(4)
                                            .spacing([12.0, 10.0])
                                            .show(ui, |ui| {
                                                for (idx, w) in words.iter().enumerate() {
                                                    egui::Frame::NONE
                                                        .fill(THEME.bg_input)
                                                        .corner_radius(8)
                                                        .inner_margin(egui::Margin::symmetric(10, 8))
                                                        .stroke(egui::Stroke::new(1.0, THEME.border))
                                                        .show(ui, |ui| {
                                                            ui.set_min_width(110.0);
                                                            ui.vertical(|ui| {
                                                                ui.label(
                                                                    RichText::new(format!("{:02}", idx + 1))
                                                                        .size(10.0)
                                                                        .color(THEME.text_dim),
                                                                );
                                                                ui.label(
                                                                    RichText::new(w.as_str())
                                                                        .size(14.0)
                                                                        .family(egui::FontFamily::Monospace)
                                                                        .strong()
                                                                        .color(THEME.text_hi),
                                                                );
                                                            });
                                                        });
                                                    if (idx + 1) % 4 == 0 {
                                                        ui.end_row();
                                                    }
                                                }
                                            });
                                    });

                                ui.add_space(16.0);
                                ui.vertical_centered(|ui| {
                                    let copy_lbl = if app
                                        .mnemonic_copied_at
                                        .map(|t| t.elapsed().as_secs() < 2)
                                        .unwrap_or(false)
                                    {
                                        "Copied — clipboard will clear in 30s"
                                    } else {
                                        "◎ Click to copy full phrase (auto-clears 30s)"
                                    };
                                    if ui
                                        .add(
                                            egui::Label::new(
                                                RichText::new(copy_lbl)
                                                    .size(12.5)
                                                    .color(THEME.text_hi),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                        .clicked()
                                    {
                                        let phrase = app.mnemonic_backup.clone();
                                        app.copy_sensitive(ui.ctx(), &phrase);
                                        app.mnemonic_copied_at = Some(std::time::Instant::now());
                                    }
                                });

                                // Verification drill
                                ui.add_space(20.0);
                                ui.separator();
                                ui.add_space(14.0);
                                section_title(ui, "Verify your backup");
                                subtle(ui, "Type the words at the positions below to confirm you wrote them down.");
                                ui.add_space(10.0);

                                let expected: Vec<String> = words.clone();
                                let mut all_ok = expected.len() == 24;
                                ui.horizontal(|ui| {
                                    for slot in 0..3 {
                                        let idx = app.backup_quiz_indices[slot];
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(format!("Word #{:02}", idx + 1))
                                                    .size(10.0)
                                                    .color(THEME.text_dim),
                                            );
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut app.backup_quiz_inputs[slot],
                                                )
                                                .desired_width(120.0)
                                                .hint_text("…"),
                                            );
                                        });
                                        ui.add_space(10.0);
                                        let typed = app.backup_quiz_inputs[slot]
                                            .trim()
                                            .to_lowercase();
                                        let exp = expected
                                            .get(idx)
                                            .map(|s| s.trim().to_lowercase())
                                            .unwrap_or_default();
                                        if typed != exp || typed.is_empty() {
                                            all_ok = false;
                                        }
                                    }
                                });

                                ui.add_space(22.0);
                                let btn_enabled = all_ok;
                                if primary_button(
                                    ui,
                                    "I have saved my recovery phrase",
                                    btn_enabled,
                                    true,
                                )
                                .clicked()
                                    && btn_enabled
                                {
                                    if let Some(payload) = &app.payload {
                                        match crypto::write_wallet_payload(&app.wallet_path, payload) {
                                            Ok(()) => {
                                                app.clear_mnemonic_backup();
                                                app.phase = Phase::Main;
                                                app.page = Page::Dashboard;
                                                app.auth_error.clear();
                                                if let Some(s) = app.secrets.clone() {
                                                    app.start_refresh(&s.address);
                                                }
                                            }
                                            Err(e) => {
                                                app.auth_error = format!("Wallet save still failing: {}", e);
                                            }
                                        }
                                    }
                                }
                            });
                        },
                    );
                });
            });
        });
}
