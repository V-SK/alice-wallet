use super::theme::THEME;
use super::widgets::*;
use crate::app::AliceWalletApp;
use crate::chain::{self, TOKEN_DECIMALS};
use eframe::egui::{self, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(wallet) = app.secrets.clone() else {
        return;
    };

    section_title(ui, app.t("send.title"));
    heading(ui, app.t("send.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("send.subtitle"));
    ui.add_space(16.0);

    card(ui, |ui| {
        section_title(ui, app.t("send.form_title"));
        ui.add_space(6.0);

        field_label(ui, app.t("send.from"));
        ui.label(
            RichText::new(shortened_address(&wallet.address))
                .size(12.0)
                .family(egui::FontFamily::Monospace)
                .color(THEME.text_mid),
        );
        ui.add_space(12.0);

        field_label(ui, app.t("send.recipient"));
        let recipient_hint = app.t("send.recipient_hint");
        if text_input(ui, &mut app.send_recipient, recipient_hint).changed() {
            app.reset_send_review();
        }
        ui.add_space(10.0);

        field_label(ui, app.t("send.amount"));
        if text_input(ui, &mut app.send_amount, "0.00").changed() {
            app.reset_send_review();
        }
        ui.add_space(10.0);

        field_label(ui, app.t("send.note"));
        let note_hint = app.t("send.note_hint");
        if text_input(ui, &mut app.send_note, note_hint).changed() {
            app.reset_send_review();
        }

        ui.add_space(14.0);
        if primary_button(ui, app.t("send.check_details"), true, false).clicked() {
            prepare_review(app);
        }

        if let Some(error) = &app.send_review_error {
            ui.add_space(12.0);
            error_banner(ui, error);
        }
    });

    ui.add_space(14.0);

    if app.send_review_ready {
        review_card(ui, app);
    } else {
        card(ui, |ui| {
            section_title(ui, app.t("send.status_title"));
            ui.label(
                RichText::new(app.t("send.status_body"))
                    .size(12.5)
                    .color(THEME.text_mid),
            );
        });
    }
}

fn prepare_review(app: &mut AliceWalletApp) {
    app.send_review_ready = false;
    app.send_review_error = None;

    if app.send_recipient.trim().is_empty() {
        app.send_review_error = Some(app.t("send.error_recipient_required").to_string());
        return;
    }
    if chain::validate_address(&app.send_recipient).is_err() {
        app.send_review_error = Some(app.t("send.invalid_address").to_string());
        return;
    }

    let amount = match chain::parse_token_amount(&app.send_amount, TOKEN_DECIMALS) {
        Ok(value) => value,
        Err(_) => {
            app.send_review_error = Some(app.t("send.error_amount_invalid").to_string());
            return;
        }
    };

    // S2: an unknown balance is NOT a pass — block review until we have a real figure.
    let Some(balance) = app.balance else {
        app.send_review_error = Some(app.t("send.error_balance_unknown").to_string());
        return;
    };
    // S1: require room for the amount PLUS a fee/ED reserve (not just amount <= balance).
    if amount.saturating_add(chain::FEE_ED_MARGIN_PLANCK) > balance {
        app.send_review_error = Some(app.t("send.error_amount_balance").to_string());
        return;
    }

    app.send_review_ready = true;
}

fn review_card(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let amount = chain::parse_token_amount(&app.send_amount, TOKEN_DECIMALS).ok();
    card_accent(ui, |ui| {
        section_title(ui, app.t("send.review_title"));
        ui.label(
            RichText::new(app.t("send.review_subtitle"))
                .size(12.5)
                .color(THEME.text_mid),
        );
        ui.add_space(14.0);

        review_row(
            ui,
            app.t("send.to"),
            &shortened_address(&app.send_recipient),
        );
        ui.add_space(8.0);
        let amount_text = amount
            .map(|value| format!("{} ALICE", format_token(value)))
            .unwrap_or_else(|| app.send_amount.trim().to_string());
        review_row(ui, app.t("send.amount"), &amount_text);
        ui.add_space(8.0);
        let note = app.send_note.trim();
        if !note.is_empty() {
            review_row(ui, app.t("send.note"), note);
            ui.add_space(8.0);
        }
        review_row(
            ui,
            app.t("send.network_fee"),
            app.t("send.network_fee_value"),
        );

        ui.add_space(14.0);
        if app.send_uncertain {
            // B2: a prior send was broadcast but not confirmed — it MAY still finalize.
            // Block a clean retry (double-spend guard); require an explicit reset after
            // the user verifies in Activity/history.
            egui::Frame::NONE
                .fill(THEME.warning_bg)
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .stroke(Stroke::new(1.0, THEME.border_accent))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(app.t("send.uncertain_body"))
                            .size(12.5)
                            .color(THEME.text_hi),
                    );
                });
            ui.add_space(12.0);
            if primary_button(ui, app.t("send.uncertain_reset"), true, false).clicked() {
                app.send_uncertain = false;
                app.pending_send = None;
                app.send_recipient.clear();
                app.send_amount.clear();
                app.send_note.clear();
                app.reset_send_review();
            }
        } else {
            let ready = app.can_submit_transfer();
            let busy = app.send_in_flight;
            // Explain why send is blocked, so the disabled button isn't a dead end.
            if !ready && !busy {
                let reason = if !app.node_sync.allows_balance_refresh() {
                    app.t("send.blocked_not_synced")
                } else {
                    app.t("send.blocked_locked")
                };
                egui::Frame::NONE
                    .fill(THEME.warning_bg)
                    .corner_radius(10)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .stroke(Stroke::new(1.0, THEME.border_accent))
                    .show(ui, |ui| {
                        ui.label(RichText::new(reason).size(12.5).color(THEME.text_hi));
                    });
                ui.add_space(12.0);
            }

            ui.horizontal(|ui| {
                if primary_button(ui, app.t("send.confirm_send"), ready, busy).clicked() {
                    app.submit_send();
                }
                if ghost_button(ui, app.t("send.cancel")).clicked() {
                    app.reset_send_review();
                }
            });
        }
    });
}

fn review_row(ui: &mut egui::Ui, label: &str, value: &str) {
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
                    ui.label(
                        RichText::new(value)
                            .size(12.0)
                            .color(THEME.text_hi)
                            .family(egui::FontFamily::Monospace),
                    );
                });
            });
        });
}
