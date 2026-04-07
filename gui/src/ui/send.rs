use super::theme::THEME;
use super::widgets::*;
use crate::app::{AliceWalletApp, AsyncAction, ReviewSend};
use crate::chain;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    let Some(secrets) = app.secrets.clone() else {
        return;
    };

    section_title(ui, app.t("send.title"));
    heading(ui, app.t("send.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("send.subtitle"));
    ui.add_space(16.0);

    ui.horizontal_top(|ui| {
        let left_w = ui.available_width() * 0.62;
        ui.allocate_ui_with_layout(
            egui::vec2(left_w.max(440.0), 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(left_w.max(440.0));
                card(ui, |ui| {
                    field_label(ui, app.t("send.recipient"));
                    text_input(ui, &mut app.transfer_to, "a2…");
                    // Live validation indicator
                    if !app.transfer_to.trim().is_empty() {
                        let ok = chain::validate_address(app.transfer_to.trim()).is_ok();
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(if ok { app.t("send.address_ok") } else { app.t("send.invalid_address") })
                                .size(11.0)
                                .color(if ok { THEME.primary_hi } else { THEME.danger }),
                        );
                    }
                    ui.add_space(12.0);

                    field_label(ui, app.t("send.amount"));
                    ui.horizontal(|ui| {
                        let w = ui.available_width() - 90.0;
                        ui.add_sized(
                            egui::vec2(w, 40.0),
                            egui::TextEdit::singleline(&mut app.transfer_amount)
                                .hint_text("0.0")
                                .margin(egui::vec2(12.0, 10.0))
                                .background_color(THEME.bg_input),
                        );
                        if ui
                            .add(
                                egui::Button::new(RichText::new("MAX").size(12.0).color(THEME.primary))
                                    .fill(THEME.bg_panel_hi)
                                    .stroke(Stroke::new(1.0, THEME.border_accent))
                                    .corner_radius(10)
                                    .min_size(egui::vec2(78.0, 40.0)),
                            )
                            .clicked()
                        {
                            if let Some(b) = app.balance {
                                // Keep a small buffer for fee (best-effort; network fee is tiny).
                                let keep = 10u128.pow(10); // 0.01 ALICE
                                let max = b.saturating_sub(keep);
                                let whole = max / 1_000_000_000_000;
                                let frac = max % 1_000_000_000_000;
                                app.transfer_amount = if frac == 0 {
                                    format!("{}", whole)
                                } else {
                                    format!("{}.{}", whole, format!("{:012}", frac).trim_end_matches('0'))
                                };
                            }
                        }
                    });

                    ui.add_space(8.0);
                    if let Some(b) = app.balance {
                        ui.label(
                            RichText::new(format!("{}: {} ALICE", app.t("send.available"), format_token(b)))
                                .size(11.5)
                                .color(THEME.text_dim),
                        );
                    }

                    if let Some(err) = &app.transfer_error.clone() {
                        ui.add_space(12.0);
                        error_banner(ui, err);
                    }

                    ui.add_space(16.0);
                    if primary_button(ui, app.t("send.review"), !app.busy, true).clicked() {
                        let recipient = app.transfer_to.trim().to_string();
                        match chain::validate_address(&recipient) {
                            Ok(()) => match chain::parse_token_amount(&app.transfer_amount, chain::TOKEN_DECIMALS) {
                                Ok(amount) => {
                                    app.transfer_error = None;
                                    app.review_send = Some(ReviewSend {
                                        to: recipient,
                                        amount,
                                        amount_raw: app.transfer_amount.clone(),
                                        hold_progress: 0.0,
                                    });
                                }
                                Err(e) => app.transfer_error = Some(e),
                            },
                            Err(e) => app.transfer_error = Some(e),
                        }
                    }
                });
            },
        );

        ui.add_space(16.0);
        ui.vertical(|ui| {
            card(ui, |ui| {
                section_title(ui, app.t("send.sender"));
                ui.label(
                    RichText::new(shortened_address(&secrets.address))
                        .size(13.0)
                        .family(egui::FontFamily::Monospace)
                        .color(THEME.text_hi),
                );
                ui.add_space(16.0);
                section_title(ui, app.t("send.tips"));
                ui.label(
                    RichText::new(app.t("send.tips_body"))
                        .size(11.5)
                        .color(THEME.text_mid),
                );
            });
        });
    });
}

pub fn render_review_modal(ctx: &egui::Context, app: &mut AliceWalletApp) {
    let Some(_review) = app.review_send.clone() else {
        return;
    };

    // Dim background
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("send_review_dim"))
        .fixed_pos(screen.min)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                screen,
                0,
                Color32::from_rgba_premultiplied(0, 0, 0, 180),
            );
        });

    egui::Window::new("review_send")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(520.0)
        .frame(
            egui::Frame::NONE
                .fill(THEME.bg_panel)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::same(26))
                .stroke(Stroke::new(1.0, THEME.border_accent))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 20],
                    blur: 52,
                    spread: 0,
                    color: Color32::from_rgba_premultiplied(0, 0, 0, 180),
                }),
        )
        .show(ctx, |ui| {
            ui.set_width(520.0);
            let review = app.review_send.clone().unwrap();
            heading(ui, app.t("send.review_title"));
            ui.add_space(4.0);
            subtle(ui, app.t("send.review_subtitle"));
            ui.add_space(18.0);

            detail_row(ui, app.t("send.to"), &review.to, true);
            ui.add_space(10.0);
            detail_row(
                ui,
                app.t("send.amount"),
                &format!("{} ALICE", format_token(review.amount)),
                false,
            );
            ui.add_space(10.0);
            if let Some(s) = &app.secrets {
                detail_row(ui, app.t("send.from"), &s.address, true);
                ui.add_space(10.0);
            }
            if let Some(b) = app.balance {
                let after = b.saturating_sub(review.amount);
                detail_row(
                    ui,
                    app.t("send.balance_after"),
                    &format!("{} ALICE", format_token(after)),
                    false,
                );
                ui.add_space(10.0);
            }
            detail_row(
                ui,
                app.t("send.network_fee"),
                app.t("send.network_fee_value"),
                false,
            );
            ui.add_space(22.0);

            // Hold-to-confirm progress bar
            let pressed = ui.input(|i| i.pointer.primary_down());
            let hold_rect = ui.available_rect_before_wrap();
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(hold_rect.width(), 48.0),
                egui::Sense::click_and_drag(),
            );
            let hovered = resp.hovered();
            let is_busy = app.busy;

            // Track progress
            if is_busy {
                // freeze
            } else if pressed && hovered {
                if let Some(r) = app.review_send.as_mut() {
                    r.hold_progress = (r.hold_progress + ui.input(|i| i.stable_dt) / 3.0).min(1.0);
                }
                ctx.request_repaint();
            } else if let Some(r) = app.review_send.as_mut() {
                r.hold_progress = (r.hold_progress - ui.input(|i| i.stable_dt)).max(0.0);
            }

            let progress = app
                .review_send
                .as_ref()
                .map(|r| r.hold_progress)
                .unwrap_or(0.0);

            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 10.0, THEME.bg_panel_hi);
            let fill = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width() * progress, rect.height()),
            );
            painter.rect_filled(fill, 10.0, THEME.primary);
            painter.rect_stroke(
                rect,
                10.0,
                Stroke::new(1.0, THEME.border_accent),
                egui::epaint::StrokeKind::Outside,
            );
            let label = if is_busy {
                app.t("send.broadcasting").to_string()
            } else if progress >= 1.0 {
                app.t("send.release").to_string()
            } else {
                format!("{} · {:.0}%", app.t("send.hold"), progress * 100.0)
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(14.0),
                if progress > 0.5 {
                    Color32::from_rgb(10, 6, 2)
                } else {
                    THEME.text_hi
                },
            );

            if progress >= 1.0 && !is_busy {
                if let (Some(review), Some(secrets)) =
                    (app.review_send.clone(), app.secrets.clone())
                {
                    app.busy = true;
                    let _ = app.tx.send(AsyncAction::Transfer(
                        app.settings.rpc_url.clone(),
                        secrets,
                        review.to,
                        review.amount,
                    ));
                }
            }

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ghost_button(ui, app.t("send.cancel")).clicked() && !is_busy {
                    app.review_send = None;
                }
            });
        });
}

fn detail_row(ui: &mut egui::Ui, label: &str, value: &str, mono: bool) {
    egui::Frame::NONE
        .fill(THEME.bg_panel_hi)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 10))
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
                    let mut rt = RichText::new(value).size(12.5).color(THEME.text_hi);
                    if mono {
                        rt = rt.family(egui::FontFamily::Monospace);
                    }
                    ui.label(rt);
                });
            });
        });
}
