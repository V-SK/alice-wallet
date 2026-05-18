use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(wallet) = app.secrets.clone() else {
        return;
    };

    section_title(ui, app.t("accounts.title"));
    heading(ui, app.t("accounts.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("accounts.subtitle"));
    ui.add_space(16.0);

    card_accent(ui, |ui| {
        section_title(ui, app.t("accounts.current"));
        ui.label(
            RichText::new(app.t("accounts.primary_label"))
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

    card(ui, |ui| {
        section_title(ui, app.t("accounts.management"));
        account_row(
            ui,
            app.t("accounts.default_address"),
            app.t("accounts.enabled"),
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
