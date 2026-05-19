use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(wallet) = app.secrets.clone() else {
        return;
    };
    let active_profile = app.active_profile_metadata();
    let profile_label = active_profile
        .as_ref()
        .map(|profile| profile.label.as_str())
        .unwrap_or(app.t("accounts.primary_label"));

    section_title(ui, app.t("accounts.title"));
    heading(ui, "Wallet profile");
    ui.add_space(4.0);
    subtle(ui, app.t("accounts.subtitle"));
    ui.add_space(16.0);

    card_accent(ui, |ui| {
        section_title(ui, app.t("accounts.current"));
        ui.label(
            RichText::new(profile_label)
                .size(16.0)
                .strong()
                .color(THEME.text_hi),
        );
        ui.add_space(12.0);
        field_label(ui, app.t("accounts.address"));
        egui::Frame::NONE
            .fill(THEME.bg_panel_hi)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .stroke(Stroke::new(1.0, THEME.border_accent))
            .show(ui, |ui| {
                copy_label(
                    ui,
                    &wallet.address,
                    &wallet.address,
                    &mut app.address_copied_at,
                    true,
                );
            });
    });

    ui.add_space(14.0);

    let profiles = app.profile_manager.safe_profiles();
    if !profiles.is_empty() {
        card(ui, |ui| {
            section_title(ui, app.t("accounts.profiles"));
            ui.add_space(8.0);
            let active_id = app.active_profile_id();
            for profile in profiles {
                let is_active = active_id.as_deref() == Some(profile.profile_id.as_str());
                profile_row(ui, app, &profile, is_active);
                ui.add_space(8.0);
            }
            ui.label(
                RichText::new(app.t("accounts.profile_safety_note"))
                    .size(12.0)
                    .color(THEME.text_mid),
            );
        });
        ui.add_space(14.0);
    }

    card(ui, |ui| {
        section_title(ui, app.t("accounts.private_key_export"));
        ui.add_space(8.0);
        ui.label(
            RichText::new(app.t("accounts.private_key_export_note"))
                .size(12.0)
                .color(THEME.text_mid),
        );
        ui.add_space(12.0);
        if wallet.can_export_private_key() {
            if app.private_key_export.is_empty() {
                field_label(ui, app.t("accounts.export_reauth_label"));
                ui.add_space(4.0);
                let reauth_placeholder = app.t("accounts.export_reauth_placeholder");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.private_key_export_password)
                            .password(!app.private_key_export_password_visible)
                            .desired_width(260.0)
                            .hint_text(reauth_placeholder),
                    );
                    let toggle = if app.private_key_export_password_visible {
                        app.t("auth.hide")
                    } else {
                        app.t("auth.show")
                    };
                    if ghost_button(ui, toggle).clicked() {
                        app.private_key_export_password_visible =
                            !app.private_key_export_password_visible;
                    }
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(app.t("accounts.export_reauth_hint"))
                        .size(11.5)
                        .color(THEME.text_dim),
                );
                ui.add_space(10.0);
                if secondary_button(ui, app.t("accounts.reveal_private_key"), true, true).clicked()
                {
                    app.reveal_private_key_export();
                }
            } else {
                egui::Frame::NONE
                    .fill(THEME.bg_input)
                    .corner_radius(10)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .stroke(Stroke::new(1.0, THEME.border_accent))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&app.private_key_export)
                                .size(12.0)
                                .family(egui::FontFamily::Monospace)
                                .color(THEME.text_hi),
                        );
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if secondary_button(ui, app.t("accounts.copy_private_key"), true, false)
                        .clicked()
                    {
                        let value = app.private_key_export.clone();
                        app.copy_sensitive(ui.ctx(), &value);
                    }
                    if ghost_button(ui, app.t("accounts.hide_private_key")).clicked() {
                        app.clear_private_key_export();
                        app.clear_private_key_export_password();
                    }
                });
            }
        } else {
            ui.label(
                RichText::new(app.t("accounts.export_unavailable"))
                    .size(12.0)
                    .color(THEME.text_mid),
            );
        }
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, app.t("accounts.management"));
        account_row(
            ui,
            app.t("accounts.default_address"),
            &profile_access_label(active_profile.as_ref()),
        );
        ui.add_space(8.0);
        account_row(ui, app.t("accounts.labels"), app.t("accounts.local_only"));
        ui.add_space(8.0);
        account_row(
            ui,
            app.t("accounts.recovery"),
            app.t("accounts.recovery_hidden"),
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new(app.t("accounts.safety_note"))
                .size(12.0)
                .color(THEME.text_mid),
        );
    });
}

fn profile_row(
    ui: &mut egui::Ui,
    app: &mut AliceWalletApp,
    profile: &crate::wallet_profiles::WalletProfileMetadata,
    is_active: bool,
) {
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .stroke(Stroke::new(
            1.0,
            if is_active {
                THEME.border_accent
            } else {
                THEME.border
            },
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&profile.label)
                            .size(13.0)
                            .strong()
                            .color(THEME.text_hi),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            shortened_address(&profile.address),
                            profile_access_label(Some(profile))
                        ))
                        .size(11.0)
                        .color(THEME.text_dim),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_active {
                        ui.label(
                            RichText::new(app.t("accounts.active"))
                                .size(12.0)
                                .strong()
                                .color(THEME.primary),
                        );
                    } else if secondary_button(ui, app.t("accounts.switch"), true, false).clicked()
                    {
                        app.select_wallet_profile(&profile.profile_id);
                    }
                });
            });
        });
}

fn profile_access_label(profile: Option<&crate::wallet_profiles::WalletProfileMetadata>) -> String {
    match profile.map(|profile| profile.access) {
        Some(crate::wallet_profiles::WalletProfileAccess::ReadOnly) => "Read-only".to_string(),
        Some(crate::wallet_profiles::WalletProfileAccess::DisplayOnly) => {
            "Display-only".to_string()
        }
        _ => "Enabled".to_string(),
    }
}

fn shortened_address(address: &str) -> String {
    if address.chars().count() <= 18 {
        return address.to_string();
    }
    let head: String = address.chars().take(8).collect();
    let tail: String = address
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{}…{}", head, tail)
}

fn account_row(ui: &mut egui::Ui, label: &str, value: &str) {
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
