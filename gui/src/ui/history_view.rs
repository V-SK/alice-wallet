use super::theme::THEME;
use super::widgets::*;
use crate::app::{AliceWalletApp, HistoryFilter};
use crate::history::{TxKind, TxRecord};
use eframe::egui::{self, Color32, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    section_title(ui, app.t("hist.title"));
    heading(ui, app.t("hist.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("hist.subtitle"));
    ui.add_space(16.0);

    // Filter chips
    ui.horizontal(|ui| {
        chip(ui, app, HistoryFilter::All, app.t("hist.filter_all"));
        chip(ui, app, HistoryFilter::Send, app.t("hist.filter_send"));
        chip(ui, app, HistoryFilter::Stake, app.t("hist.filter_stake"));
        chip(ui, app, HistoryFilter::Unstake, app.t("hist.filter_unstake"));
    });
    ui.add_space(12.0);

    let filter = app.history_filter;
    let filtered: Vec<TxRecord> = app
        .history
        .iter()
        .filter(|rec| matches_filter(filter, rec))
        .cloned()
        .collect();

    card(ui, |ui| {
        if filtered.is_empty() {
            ui.add_space(28.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(app.t("hist.empty"))
                        .size(13.5)
                        .color(THEME.text_dim),
                );
                ui.label(
                    RichText::new(app.t("hist.empty_hint"))
                        .size(11.5)
                        .color(THEME.text_dim),
                );
            });
            ui.add_space(28.0);
            return;
        }
        for rec in filtered.iter() {
            render_row(ui, rec);
        }
    });
}

fn matches_filter(f: HistoryFilter, rec: &TxRecord) -> bool {
    match f {
        HistoryFilter::All => true,
        HistoryFilter::Send => matches!(rec.kind, TxKind::Send),
        HistoryFilter::Stake => matches!(rec.kind, TxKind::StakeScorer | TxKind::StakeAggregator),
        HistoryFilter::Unstake => {
            matches!(rec.kind, TxKind::UnstakeScorer | TxKind::UnstakeAggregator)
        }
    }
}

fn chip(ui: &mut egui::Ui, app: &mut AliceWalletApp, filter: HistoryFilter, label: &str) {
    let active = app.history_filter == filter;
    let bg = if active { THEME.primary_dim } else { THEME.bg_panel_hi };
    let stroke = if active { THEME.border_accent } else { THEME.border };
    let color = if active { THEME.text_hi } else { THEME.text_mid };
    let resp = ui.add(
        egui::Button::new(RichText::new(label).size(12.0).strong().color(color))
            .fill(bg)
            .stroke(Stroke::new(1.0, stroke))
            .corner_radius(8)
            .min_size(egui::vec2(78.0, 28.0)),
    );
    if resp.clicked() {
        app.history_filter = filter;
    }
    ui.add_space(6.0);
}

pub fn render_row(ui: &mut egui::Ui, rec: &TxRecord) {
    let (icon, icon_color) = match rec.kind {
        TxKind::Send => ("↗", THEME.primary),
        TxKind::StakeScorer | TxKind::StakeAggregator => ("◆", THEME.primary_hi),
        TxKind::UnstakeScorer | TxKind::UnstakeAggregator => ("◇", THEME.text_mid),
    };
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .stroke(Stroke::new(1.0, if rec.ok { THEME.border } else { THEME.danger }))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).size(18.0).color(icon_color));
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(rec.kind.label())
                            .size(13.0)
                            .strong()
                            .color(THEME.text_hi),
                    );
                    let subline = match (&rec.amount, &rec.counterparty) {
                        (Some(a), Some(c)) => format!(
                            "{} ALICE → {}",
                            format_token(*a),
                            shortened_address(c)
                        ),
                        (Some(a), None) => format!("{} ALICE", format_token(*a)),
                        _ => String::new(),
                    };
                    ui.label(
                        RichText::new(subline)
                            .size(11.5)
                            .color(THEME.text_mid)
                            .family(egui::FontFamily::Monospace),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(rec.ts.format("%Y-%m-%d %H:%M").to_string())
                            .size(11.0)
                            .color(THEME.text_dim),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(if rec.ok { "✓" } else { "✗" })
                            .size(13.0)
                            .color(if rec.ok { THEME.primary } else { THEME.danger }),
                    );
                });
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new(&rec.hash)
                    .size(10.0)
                    .family(egui::FontFamily::Monospace)
                    .color(if rec.ok { THEME.text_dim } else { Color32::from_rgb(255, 120, 120) }),
            );
        });
    ui.add_space(6.0);
}
