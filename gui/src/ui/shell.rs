use super::theme::{paint_backdrop, THEME};
use super::widgets;
use crate::app::{AliceWalletApp, ConnectionState, Page};
use crate::config::Lang;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

pub fn render(ctx: &egui::Context, app: &mut AliceWalletApp) {
    // Global keyboard shortcuts: Cmd/Ctrl + 1..5 → switch page
    ctx.input(|i| {
        if i.modifiers.command {
            if i.key_pressed(egui::Key::Num1) { app.page = Page::Dashboard; }
            if i.key_pressed(egui::Key::Num2) { app.page = Page::Send; }
            if i.key_pressed(egui::Key::Num3) { app.page = Page::Stake; }
            if i.key_pressed(egui::Key::Num4) { app.page = Page::History; }
            if i.key_pressed(egui::Key::Num5) { app.page = Page::Settings; }
        }
    });

    // Topbar
    egui::TopBottomPanel::top("topbar")
        .exact_height(52.0)
        .frame(
            egui::Frame::NONE
                .fill(THEME.bg_panel)
                .inner_margin(egui::Margin::symmetric(22, 10))
                .stroke(Stroke::new(1.0, THEME.border)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // Connection dot
                let (dot_color, dot_text) = match &app.connection_status {
                    ConnectionState::Connected => (THEME.primary, app.t("shell.connected")),
                    ConnectionState::Connecting => (Color32::from_rgb(255, 179, 64), app.t("shell.connecting")),
                    ConnectionState::Error => (THEME.danger, app.t("shell.disconnected")),
                };
                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, dot_color);
                ui.add_space(6.0);
                ui.label(
                    RichText::new(dot_text)
                        .size(12.0)
                        .color(THEME.text_mid),
                );
                ui.add_space(10.0);
                ui.label(RichText::new(&app.settings.rpc_url).size(11.5).color(THEME.text_dim).family(egui::FontFamily::Monospace));

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(16.0);

                let block_lbl = app.t("shell.block");
                if let Some(blk) = app.block_height {
                    ui.label(
                        RichText::new(format!("{} #{}", block_lbl, fmt_u64(blk)))
                            .size(12.0)
                            .color(THEME.text_mid)
                            .family(egui::FontFamily::Monospace),
                    );
                } else {
                    ui.label(RichText::new(format!("{} #—", block_lbl)).size(12.0).color(THEME.text_dim));
                }

                let lock_lbl = app.t("shell.lock");
                let lang_lbl = match app.settings.lang { Lang::En => "中文", Lang::Zh => "EN" };
                let auto_lbl = app.t("shell.auto_lock");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(lock_lbl).size(12.5).color(THEME.text_hi),
                            )
                            .fill(THEME.bg_panel_hi)
                            .stroke(Stroke::new(1.0, THEME.border_accent))
                            .corner_radius(10)
                            .min_size(egui::vec2(92.0, 32.0)),
                        )
                        .clicked()
                    {
                        app.lock_now();
                    }
                    ui.add_space(8.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(lang_lbl).size(12.5).color(THEME.primary),
                            )
                            .fill(THEME.bg_panel_hi)
                            .stroke(Stroke::new(1.0, THEME.border_accent))
                            .corner_radius(10)
                            .min_size(egui::vec2(56.0, 32.0)),
                        )
                        .clicked()
                    {
                        app.settings.lang = match app.settings.lang { Lang::En => Lang::Zh, Lang::Zh => Lang::En };
                        let _ = app.settings.save();
                    }
                    ui.add_space(8.0);
                    if let Some(until) = app.auto_lock_remaining() {
                        ui.label(
                            RichText::new(format!("{} {}s", auto_lbl, until))
                                .size(11.0)
                                .color(THEME.text_dim),
                        );
                    }
                });
            });
        });

    // Sidebar
    egui::SidePanel::left("sidebar")
        .exact_width(240.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(THEME.bg_panel)
                .inner_margin(egui::Margin::symmetric(18, 22))
                .stroke(Stroke::new(1.0, THEME.border)),
        )
        .show(ctx, |ui| {
            // Logo header — bare orange triangle, no box.
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::include_image!("../../alice-logo-traced.svg"))
                        .fit_to_exact_size(egui::vec2(34.0, 34.0)),
                );
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Alice Wallet")
                            .size(17.0)
                            .strong()
                            .color(THEME.text_hi),
                    );
                    ui.label(
                        RichText::new("Native · Local-first")
                            .size(10.5)
                            .color(THEME.text_dim),
                    );
                });
            });

            ui.add_space(22.0);
            ui.separator();
            ui.add_space(14.0);

            let l_dash = app.t("nav.dashboard");
            let l_send = app.t("nav.send");
            let l_stake = app.t("nav.stake");
            let l_hist = app.t("nav.history");
            let l_set = app.t("nav.settings");
            nav_item(ui, app, Page::Dashboard, "◈", l_dash);
            nav_item(ui, app, Page::Send, "↗", l_send);
            nav_item(ui, app, Page::Stake, "◆", l_stake);
            nav_item(ui, app, Page::History, "≡", l_hist);
            nav_item(ui, app, Page::Settings, "⚙", l_set);

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .size(10.5)
                        .color(THEME.text_dim),
                );
                if let Some(p) = app.wallet_path.to_str() {
                    ui.label(
                        RichText::new(p)
                            .size(9.5)
                            .color(THEME.text_dim)
                            .family(egui::FontFamily::Monospace),
                    );
                }
                ui.label(
                    RichText::new("Not all intelligence bends the knee.")
                        .size(10.0)
                        .italics()
                        .color(THEME.text_dim),
                );
            });
        });

    // Central content
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(THEME.bg_base))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            paint_backdrop(ui, rect);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(24.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(1000.0);
                            match app.page {
                                Page::Dashboard => super::dashboard::render(ui, app),
                                Page::Send => super::send::render(ui, app),
                                Page::Stake => super::stake::render(ui, app),
                                Page::History => super::history_view::render(ui, app),
                                Page::Settings => super::settings::render(ui, app),
                            }
                            ui.add_space(32.0);
                        },
                    );
                });
        });

    // Render any active modal on top
    super::send::render_review_modal(ctx, app);
    super::stake::render_review_modal(ctx, app);

    // Toast
    render_toast(ctx, app);
    let _ = widgets::format_token; // suppress unused when a page is hidden
}

fn nav_item(ui: &mut egui::Ui, app: &mut AliceWalletApp, page: Page, icon: &str, label: &str) {
    let active = app.page == page;
    let bg = if active { THEME.primary_dim } else { Color32::TRANSPARENT };
    let stroke = if active {
        Stroke::new(1.0, THEME.border_accent)
    } else {
        Stroke::NONE
    };
    let resp = egui::Frame::NONE
        .fill(bg)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .stroke(stroke)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon)
                        .size(15.0)
                        .color(if active { THEME.primary } else { THEME.text_mid }),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(label)
                        .size(13.5)
                        .strong()
                        .color(if active { THEME.text_hi } else { THEME.text_mid }),
                );
            });
        })
        .response
        .interact(egui::Sense::click());

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        app.page = page;
    }
    ui.add_space(4.0);
}

fn render_toast(ctx: &egui::Context, app: &mut AliceWalletApp) {
    let Some(toast) = app.toast.clone() else {
        return;
    };
    if toast.expires_at <= std::time::Instant::now() {
        app.toast = None;
        return;
    }
    ctx.request_repaint();

    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("toast_area"))
        .fixed_pos(egui::pos2(screen.right() - 380.0, screen.bottom() - 96.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (bg, border, text_c) = if toast.ok {
                (THEME.primary_dim, THEME.primary, THEME.text_hi)
            } else {
                (THEME.danger_bg, THEME.danger, THEME.text_hi)
            };
            egui::Frame::NONE
                .fill(bg)
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(16, 14))
                .stroke(Stroke::new(1.0, border))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_rgba_premultiplied(0, 0, 0, 120),
                })
                .show(ui, |ui| {
                    ui.set_min_width(320.0);
                    ui.label(
                        RichText::new(&toast.title)
                            .size(13.5)
                            .strong()
                            .color(text_c),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new(&toast.body)
                            .size(11.5)
                            .color(THEME.text_mid),
                    );
                });
        });
}

fn fmt_u64(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut parts = Vec::new();
    while n > 0 {
        parts.push(format!("{:03}", n % 1000));
        n /= 1000;
    }
    parts.reverse();
    // Trim leading zeros of the first group
    let first = parts[0].trim_start_matches('0').to_string();
    let first = if first.is_empty() { "0".into() } else { first };
    let mut out = first;
    for p in parts.into_iter().skip(1) {
        out.push(',');
        out.push_str(&p);
    }
    out
}
