use super::theme::THEME;
use super::widgets::*;
use crate::app::{AliceWalletApp, Toast};
use crate::config::{Lang, DEFAULT_AUTO_LOCK_MINUTES, DEFAULT_RPC_URL};
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

    // Language card
    card(ui, |ui| {
        let l_title = app.t("set.language");
        section_title(ui, l_title);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            for (lang, label) in [(Lang::En, "English"), (Lang::Zh, "中文")] {
                let active = app.settings.lang == lang;
                let bg = if active { THEME.primary_dim } else { THEME.bg_panel_hi };
                let stroke = if active { THEME.border_accent } else { THEME.border };
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).size(12.5).strong().color(if active { THEME.text_hi } else { THEME.text_mid }))
                            .fill(bg)
                            .stroke(Stroke::new(1.0, stroke))
                            .corner_radius(10)
                            .min_size(egui::vec2(120.0, 34.0)),
                    )
                    .clicked()
                {
                    app.settings.lang = lang;
                    let _ = app.settings.save();
                }
                ui.add_space(8.0);
            }
        });
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        let t_rpc = app.t("set.rpc");
        let t_ws = app.t("set.ws_url");
        section_title(ui, t_rpc);
        field_label(ui, t_ws);
        text_input(ui, &mut app.settings_rpc_draft, "wss://rpc.aliceprotocol.org");
        ui.add_space(8.0);
        ui.label(
            RichText::new("Default: wss://rpc.aliceprotocol.org")
                .size(11.0)
                .color(THEME.text_dim),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if primary_button(ui, "Save RPC", true, false).clicked() {
                let url = app.settings_rpc_draft.trim().to_string();
                if url.is_empty() || !(url.starts_with("ws://") || url.starts_with("wss://")) {
                    app.toast = Some(Toast::err(
                        "Invalid RPC URL",
                        "URL must start with ws:// or wss://",
                    ));
                } else {
                    app.settings.rpc_url = url;
                    match app.settings.save() {
                        Ok(()) => {
                            app.toast = Some(Toast::ok("Saved", "RPC endpoint updated"));
                            if let Some(s) = app.secrets.clone() {
                                app.start_refresh(&s.address);
                            }
                        }
                        Err(e) => {
                            app.toast = Some(Toast::err("Save failed", e));
                        }
                    }
                }
            }
            ui.add_space(10.0);
            if secondary_button(ui, "Reset to default", true, false).clicked() {
                app.settings_rpc_draft = DEFAULT_RPC_URL.to_string();
            }
        });
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, "Auto-lock");
        field_label(ui, "INACTIVITY TIMEOUT (MINUTES)");
        text_input(ui, &mut app.settings_lock_draft, "10");
        ui.add_space(8.0);
        ui.label(
            RichText::new("0 disables auto-lock. Recommended: 5–15.")
                .size(11.0)
                .color(THEME.text_dim),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if primary_button(ui, "Save auto-lock", true, false).clicked() {
                match app.settings_lock_draft.trim().parse::<u32>() {
                    Ok(m) if m <= 1440 => {
                        app.settings.auto_lock_minutes = m;
                        match app.settings.save() {
                            Ok(()) => {
                                app.toast = Some(Toast::ok("Saved", "Auto-lock updated"));
                                app.bump_interaction();
                            }
                            Err(e) => {
                                app.toast = Some(Toast::err("Save failed", e));
                            }
                        }
                    }
                    _ => {
                        app.toast = Some(Toast::err("Invalid value", "Enter an integer 0-1440"));
                    }
                }
            }
            ui.add_space(10.0);
            if secondary_button(ui, "Reset to default", true, false).clicked() {
                app.settings_lock_draft = DEFAULT_AUTO_LOCK_MINUTES.to_string();
            }
        });
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, "Wallet file");
        ui.label(
            RichText::new(app.wallet_path.display().to_string())
                .size(11.5)
                .family(egui::FontFamily::Monospace)
                .color(THEME.text_hi),
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
        section_title(ui, "Security");
        ui.add_space(4.0);
        if danger_button(ui, "Lock wallet now", true).clicked() {
            app.lock_now();
        }
    });
}
