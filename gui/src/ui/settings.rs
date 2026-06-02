use super::theme::THEME;
use super::widgets::*;
use crate::app::{AliceWalletApp, Toast};
use crate::config::{Lang, DEFAULT_AUTO_LOCK_MINUTES};
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let t_title = app.t("set.title");
    let t_heading = app.t("set.heading");
    let t_subtitle = app.t("set.subtitle");
    section_title(ui, t_title);
    heading(ui, t_heading);
    ui.add_space(4.0);
    subtle(ui, t_subtitle);
    ui.add_space(18.0);

    about_card(ui, app);
    ui.add_space(14.0);

    // Language card
    card(ui, |ui| {
        let l_title = app.t("set.language");
        section_title(ui, l_title);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (lang, label) in [(Lang::En, "English"), (Lang::Zh, "中文")] {
                let active = app.settings.lang == lang;
                let bg = if active {
                    THEME.primary_dim
                } else {
                    THEME.bg_panel_hi
                };
                let stroke = if active {
                    THEME.border_accent
                } else {
                    THEME.border
                };
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).size(12.5).strong().color(
                            if active {
                                THEME.text_hi
                            } else {
                                THEME.text_mid
                            },
                        ))
                        .fill(bg)
                        .stroke(Stroke::new(1.0, stroke))
                        .corner_radius(10)
                        .min_size(egui::vec2(120.0, 34.0)),
                    )
                    .clicked()
                {
                    app.settings.lang = lang;
                    let _ = app.save_settings();
                }
                ui.add_space(8.0);
            }
        });
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, app.t("set.connection"));
        ui.add_space(8.0);
        ui.label(
            RichText::new(app.t("set.connection_body"))
                .size(12.0)
                .color(THEME.text_mid),
        );
        ui.add_space(10.0);
        settings_row(
            ui,
            app.t("sync.status"),
            app.t(app.node_sync.status_i18n_key()),
        );
        ui.add_space(8.0);
        settings_row(ui, app.t("sync.mode"), app.node_sync.sync_mode.label());
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, app.t("set.autolock"));
        field_label(ui, app.t("set.autolock_label"));
        text_input(ui, &mut app.settings_lock_draft, "10");
        ui.add_space(8.0);
        ui.label(
            RichText::new(app.t("set.autolock_hint"))
                .size(11.0)
                .color(THEME.text_dim),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if primary_button(ui, app.t("set.save_autolock"), true, false).clicked() {
                match app.settings_lock_draft.trim().parse::<u32>() {
                    Ok(m) if m <= 1440 => {
                        app.settings.auto_lock_minutes = m;
                        match app.save_settings() {
                            Ok(()) => {
                                app.toast = Some(Toast::ok(
                                    app.t("toast.saved"),
                                    app.t("set.autolock_saved"),
                                ));
                                app.bump_interaction();
                            }
                            Err(e) => {
                                let _ = e;
                                app.toast = Some(Toast::err(
                                    app.t("toast.save_failed"),
                                    app.t("set.save_failed_body"),
                                ));
                            }
                        }
                    }
                    _ => {
                        app.toast = Some(Toast::err(
                            app.t("set.invalid_value"),
                            app.t("set.invalid_autolock_body"),
                        ));
                    }
                }
            }
            ui.add_space(10.0);
            if secondary_button(ui, app.t("set.reset_default"), true, false).clicked() {
                app.settings_lock_draft = DEFAULT_AUTO_LOCK_MINUTES.to_string();
            }
        });
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, app.t("set.wallet_data"));
        ui.label(
            RichText::new(app.t("set.wallet_data_body"))
                .size(12.0)
                .color(THEME.text_mid),
        );
        ui.add_space(10.0);
        if let Some(p) = &app.payload {
            ui.label(
                RichText::new(format!(
                    "Format v{} · {} · iters={}",
                    p.version, p.kdf, p.kdf_iterations
                ))
                .size(11.0)
                .color(THEME.text_dim),
            );
        }
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, app.t("set.security"));
        ui.add_space(4.0);
        if danger_button(ui, app.t("set.lock_now"), true).clicked() {
            app.lock_now();
        }
    });
}

/// About / brand card: the Alice logo, app identity, version, security posture.
fn about_card(ui: &mut egui::Ui, app: &AliceWalletApp) {
    card_accent(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(
                egui::Image::new(egui::include_image!("../../assets/brand/alice-logo.svg"))
                    .fit_to_exact_size(egui::vec2(44.0, 44.0)),
            );
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Alice Wallet")
                        .size(20.0)
                        .strong()
                        .color(THEME.text_hi),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(app.t("about.tagline"))
                        .size(12.0)
                        .italics()
                        .color(THEME.primary_hi),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                egui::Frame::NONE
                    .fill(THEME.bg_panel_hi)
                    .corner_radius(255)
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .stroke(Stroke::new(1.0, THEME.border_accent))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(12.0)
                                .strong()
                                .family(egui::FontFamily::Monospace)
                                .color(THEME.primary),
                        );
                    });
            });
        });

        ui.add_space(14.0);
        ui.label(
            RichText::new(app.t("about.blurb"))
                .size(12.5)
                .color(THEME.text_mid),
        );

        ui.add_space(14.0);
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::same(14))
            .stroke(Stroke::new(1.0, THEME.border))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(app.t("about.security_title").to_uppercase())
                        .size(10.0)
                        .strong()
                        .extra_letter_spacing(1.0)
                        .color(THEME.text_dim),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(app.t("about.security_body"))
                        .size(12.0)
                        .color(THEME.text_mid),
                );
            });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(app.t("about.website").to_uppercase())
                    .size(10.0)
                    .strong()
                    .color(THEME.text_dim),
            );
            ui.add_space(8.0);
            ui.hyperlink_to(
                RichText::new(app.t("about.website_url"))
                    .size(12.0)
                    .family(egui::FontFamily::Monospace)
                    .color(THEME.primary_hi),
                "https://aliceprotocol.org",
            );
        });
    });
}

fn settings_row(ui: &mut egui::Ui, label: &str, value: &str) {
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
                    ui.label(RichText::new(value).size(12.0).color(THEME.text_hi));
                });
            });
        });
}
