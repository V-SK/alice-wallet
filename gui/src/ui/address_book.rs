use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    section_title(ui, app.t("address_book.title"));
    heading(ui, app.t("address_book.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("address_book.subtitle"));
    ui.add_space(16.0);

    let records = app.active_address_book_records();
    card(ui, |ui| {
        section_title(ui, app.t("address_book.empty_title"));
        ui.add_space(8.0);
        if records.is_empty() {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(app.t("address_book.empty"))
                        .size(13.5)
                        .strong()
                        .color(THEME.text_hi),
                );
                ui.label(
                    RichText::new(app.t("address_book.empty_hint"))
                        .size(12.0)
                        .color(THEME.text_mid),
                );
            });
        } else {
            for record in records {
                book_row(ui, &record.label, &short_address(&record.address));
                if !record.note.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&record.note).size(11.5).color(THEME.text_mid));
                }
                ui.add_space(8.0);
            }
        }
    });

    ui.add_space(14.0);

    card(ui, |ui| {
        section_title(ui, app.t("address_book.schema_title"));
        book_row(
            ui,
            app.t("address_book.field_label"),
            app.t("address_book.field_label_value"),
        );
        ui.add_space(8.0);
        book_row(
            ui,
            app.t("address_book.field_address"),
            app.t("address_book.field_address_value"),
        );
        ui.add_space(8.0);
        book_row(
            ui,
            app.t("address_book.field_note"),
            app.t("address_book.field_note_value"),
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new(app.t("address_book.safety_note"))
                .size(12.0)
                .color(THEME.text_mid),
        );
    });
}

fn short_address(address: &str) -> String {
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

fn book_row(ui: &mut egui::Ui, label: &str, value: &str) {
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
