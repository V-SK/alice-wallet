use super::theme::THEME;
use super::widgets::*;
use crate::app::{AliceWalletApp, AsyncAction, ReviewStake, StakeKind};
use crate::chain;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

pub fn render(ui: &mut egui::Ui, app: &mut AliceWalletApp) {
    if app.secrets.is_none() {
        return;
    }

    section_title(ui, app.t("stake.title"));
    heading(ui, app.t("stake.heading"));
    ui.add_space(4.0);
    subtle(ui, app.t("stake.subtitle"));
    ui.add_space(40.0);

    // Coming-soon placeholder. Staking pallet is not yet live on mainnet.
    card(ui, |ui| {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("◆")
                    .size(48.0)
                    .color(THEME.primary),
            );
            ui.add_space(14.0);
            ui.label(
                RichText::new(app.t("stake.coming_soon_title"))
                    .size(20.0)
                    .strong()
                    .color(THEME.text_hi),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(app.t("stake.coming_soon_body"))
                    .size(13.0)
                    .color(THEME.text_mid),
            );
        });
        ui.add_space(40.0);
    });
    return;

    #[allow(unreachable_code)]
    ui.horizontal_top(|ui| {
        let w = (ui.available_width() - 18.0) / 2.0;
        ui.allocate_ui_with_layout(
            egui::vec2(w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(w);
                stake_role_card(ui, app, RoleKind::Scorer);
            },
        );
        ui.add_space(18.0);
        ui.allocate_ui_with_layout(
            egui::vec2(w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(w);
                stake_role_card(ui, app, RoleKind::Aggregator);
            },
        );
    });

    if let Some(err) = &app.stake_error.clone() {
        ui.add_space(14.0);
        error_banner(ui, err);
    }
}

#[derive(Clone, Copy)]
enum RoleKind {
    Scorer,
    Aggregator,
}

impl RoleKind {
    fn label(self, app: &AliceWalletApp) -> &'static str {
        match self {
            RoleKind::Scorer => app.t("dash.scorer"),
            RoleKind::Aggregator => app.t("dash.aggregator"),
        }
    }
    fn stake_kind(self) -> StakeKind {
        match self {
            RoleKind::Scorer => StakeKind::ScorerStake,
            RoleKind::Aggregator => StakeKind::AggregatorStake,
        }
    }
    fn unstake_kind(self) -> StakeKind {
        match self {
            RoleKind::Scorer => StakeKind::ScorerUnstake,
            RoleKind::Aggregator => StakeKind::AggregatorUnstake,
        }
    }
}

fn stake_role_card(ui: &mut egui::Ui, app: &mut AliceWalletApp, role: RoleKind) {
    card_accent(ui, |ui| {
        let role_label = role.label(app).to_string();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(role_label.to_uppercase())
                    .size(11.0)
                    .extra_letter_spacing(1.8)
                    .color(THEME.primary)
                    .strong(),
            );
        });
        ui.add_space(4.0);

        let info = match role {
            RoleKind::Scorer => app.scorer_stake.as_ref(),
            RoleKind::Aggregator => app.agg_stake.as_ref(),
        };

        match info {
            Some(s) => {
                ui.label(
                    RichText::new(format_token(s.stake))
                        .size(28.0)
                        .strong()
                        .color(THEME.text_hi),
                );
                ui.label(
                    RichText::new(format!("Status · {}", s.status))
                        .size(12.0)
                        .color(THEME.primary_hi),
                );
            }
            None => {
                ui.label(
                    RichText::new(app.t("dash.not_staked"))
                        .size(20.0)
                        .color(THEME.text_dim),
                );
            }
        }

        ui.add_space(16.0);

        let amount_lbl = app.t("stake.amount");
        let endpoint_lbl = app.t("stake.endpoint");
        let (amount, endpoint) = match role {
            RoleKind::Scorer => (&mut app.scorer_amount, &mut app.scorer_endpoint),
            RoleKind::Aggregator => (&mut app.aggregator_amount, &mut app.aggregator_endpoint),
        };

        field_label(ui, amount_lbl);
        text_input(ui, amount, "0.0");
        ui.add_space(10.0);
        field_label(ui, endpoint_lbl);
        text_input(ui, endpoint, "https://…");
        ui.add_space(16.0);

        ui.horizontal(|ui| {
            let bw = (ui.available_width() - 12.0) / 2.0;
            if ui
                .add_sized(
                    egui::vec2(bw, 42.0),
                    egui::Button::new(
                        RichText::new(app.t("stake.stake_btn"))
                            .size(13.5)
                            .strong()
                            .color(Color32::from_rgb(10, 6, 2)),
                    )
                    .fill(THEME.primary)
                    .corner_radius(10),
                )
                .clicked()
                && !app.busy
            {
                let amt_s = match role {
                    RoleKind::Scorer => app.scorer_amount.clone(),
                    RoleKind::Aggregator => app.aggregator_amount.clone(),
                };
                let ep = match role {
                    RoleKind::Scorer => app.scorer_endpoint.trim().to_string(),
                    RoleKind::Aggregator => app.aggregator_endpoint.trim().to_string(),
                };
                if ep.is_empty() {
                    app.stake_error = Some(app.t("stake.endpoint_required").into());
                } else {
                    match chain::parse_token_amount(&amt_s, chain::TOKEN_DECIMALS) {
                        Ok(amount) => {
                            app.stake_error = None;
                            app.review_stake = Some(ReviewStake {
                                kind: role.stake_kind(),
                                amount: Some(amount),
                                endpoint: Some(ep),
                                hold_progress: 0.0,
                            });
                        }
                        Err(e) => app.stake_error = Some(e),
                    }
                }
            }
            ui.add_space(12.0);
            if ui
                .add_sized(
                    egui::vec2(bw, 42.0),
                    egui::Button::new(
                        RichText::new(app.t("stake.unstake_btn"))
                            .size(13.5)
                            .color(THEME.danger),
                    )
                    .fill(THEME.bg_panel_hi)
                    .stroke(Stroke::new(1.0, THEME.danger))
                    .corner_radius(10),
                )
                .clicked()
                && !app.busy
            {
                app.stake_error = None;
                app.review_stake = Some(ReviewStake {
                    kind: role.unstake_kind(),
                    amount: None,
                    endpoint: None,
                    hold_progress: 0.0,
                });
            }
        });
    });
}

pub fn render_review_modal(ctx: &egui::Context, app: &mut AliceWalletApp) {
    let Some(_) = app.review_stake.clone() else {
        return;
    };

    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("stake_review_dim"))
        .fixed_pos(screen.min)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.painter().rect_filled(
                screen,
                0,
                Color32::from_rgba_premultiplied(0, 0, 0, 180),
            );
        });

    egui::Window::new("review_stake")
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
                .stroke(Stroke::new(1.0, THEME.border_accent)),
        )
        .show(ctx, |ui| {
            ui.set_width(520.0);
            let review = app.review_stake.clone().unwrap();

            let title = match review.kind {
                StakeKind::ScorerStake => app.t("stake.confirm_scorer_stake"),
                StakeKind::AggregatorStake => app.t("stake.confirm_agg_stake"),
                StakeKind::ScorerUnstake => app.t("stake.confirm_scorer_unstake"),
                StakeKind::AggregatorUnstake => app.t("stake.confirm_agg_unstake"),
            };
            heading(ui, title);
            ui.add_space(4.0);
            subtle(ui, app.t("send.review_subtitle"));
            ui.add_space(18.0);

            if let Some(amt) = review.amount {
                detail_row(ui, app.t("send.amount"), &format!("{} ALICE", format_token(amt)), false);
                ui.add_space(10.0);
            }
            if let Some(ep) = &review.endpoint {
                detail_row(ui, app.t("stake.endpoint_label"), ep, true);
                ui.add_space(10.0);
            }
            if let Some(s) = &app.secrets {
                detail_row(ui, app.t("send.from"), &s.address, true);
                ui.add_space(10.0);
            }
            detail_row(ui, app.t("send.network_fee"), app.t("send.network_fee_value"), false);
            ui.add_space(22.0);

            let pressed = ui.input(|i| i.pointer.primary_down());
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 48.0),
                egui::Sense::click_and_drag(),
            );
            let hovered = resp.hovered();
            let is_busy = app.busy;

            if is_busy {
                // freeze progress
            } else if pressed && hovered {
                if let Some(r) = app.review_stake.as_mut() {
                    r.hold_progress = (r.hold_progress + ui.input(|i| i.stable_dt) / 3.0).min(1.0);
                }
                ctx.request_repaint();
            } else if let Some(r) = app.review_stake.as_mut() {
                r.hold_progress = (r.hold_progress - ui.input(|i| i.stable_dt)).max(0.0);
            }

            let progress = app
                .review_stake
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
                if let (Some(r), Some(secrets)) = (app.review_stake.clone(), app.secrets.clone()) {
                    app.busy = true;
                    match r.kind {
                        StakeKind::ScorerStake => {
                            let _ = app.tx.send(AsyncAction::Stake(
                                app.settings.rpc_url.clone(),
                                secrets,
                                "scorer".into(),
                                r.amount.unwrap_or(0),
                                r.endpoint.clone().unwrap_or_default(),
                            ));
                        }
                        StakeKind::AggregatorStake => {
                            let _ = app.tx.send(AsyncAction::Stake(
                                app.settings.rpc_url.clone(),
                                secrets,
                                "aggregator".into(),
                                r.amount.unwrap_or(0),
                                r.endpoint.clone().unwrap_or_default(),
                            ));
                        }
                        StakeKind::ScorerUnstake => {
                            let _ = app.tx.send(AsyncAction::Unstake(
                                app.settings.rpc_url.clone(),
                                secrets,
                                "scorer".into(),
                            ));
                        }
                        StakeKind::AggregatorUnstake => {
                            let _ = app.tx.send(AsyncAction::Unstake(
                                app.settings.rpc_url.clone(),
                                secrets,
                                "aggregator".into(),
                            ));
                        }
                    }
                }
            }

            ui.add_space(14.0);
            if ghost_button(ui, app.t("send.cancel")).clicked() && !is_busy {
                app.review_stake = None;
            }
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
