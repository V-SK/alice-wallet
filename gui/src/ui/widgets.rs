use super::theme::THEME;
use eframe::egui::{self, Color32, CornerRadius, Response, RichText, Stroke, Ui};

pub fn card<R>(ui: &mut Ui, inner: impl FnOnce(&mut Ui) -> R) -> R {
    let t = THEME;
    egui::Frame::NONE
        .fill(t.bg_panel)
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::same(22))
        .stroke(Stroke::new(1.0, t.border))
        .show(ui, inner)
        .inner
}

pub fn card_accent<R>(ui: &mut Ui, inner: impl FnOnce(&mut Ui) -> R) -> R {
    let t = THEME;
    egui::Frame::NONE
        .fill(t.bg_panel)
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::same(22))
        .stroke(Stroke::new(1.0, t.border_accent))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 28,
            spread: 0,
            color: Color32::from_rgba_premultiplied(255, 119, 24, 24),
        })
        .show(ui, inner)
        .inner
}

pub fn section_title(ui: &mut Ui, label: &str) {
    ui.label(
        RichText::new(label.to_uppercase())
            .size(11.0)
            .extra_letter_spacing(1.4)
            .color(THEME.text_dim)
            .strong(),
    );
    ui.add_space(6.0);
}

pub fn heading(ui: &mut Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .size(22.0)
            .color(THEME.text_hi)
            .strong(),
    );
}

pub fn subtle(ui: &mut Ui, label: &str) {
    ui.label(RichText::new(label).size(12.5).color(THEME.text_mid));
}

pub fn field_label(ui: &mut Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .size(11.5)
            .color(THEME.text_mid)
            .strong(),
    );
    ui.add_space(4.0);
}

pub fn text_input(ui: &mut Ui, value: &mut String, hint: &str) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text(hint)
            .margin(egui::vec2(12.0, 10.0))
            .background_color(THEME.bg_input),
    )
}

pub fn password_input(ui: &mut Ui, value: &mut String, visible: &mut bool, hint: &str) -> Response {
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(value)
                .password(!*visible)
                .desired_width(ui.available_width() - 44.0)
                .hint_text(hint)
                .margin(egui::vec2(12.0, 10.0))
                .background_color(THEME.bg_input),
        );
        let eye = if *visible { "◉" } else { "◎" };
        if ui
            .add(
                egui::Button::new(RichText::new(eye).size(15.0).color(THEME.text_mid))
                    .fill(THEME.bg_input)
                    .stroke(Stroke::new(1.0, THEME.border))
                    .corner_radius(10)
                    .min_size(egui::vec2(36.0, 36.0)),
            )
            .clicked()
        {
            *visible = !*visible;
        }
        resp
    })
    .inner
}

pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool, full: bool) -> Response {
    let mut btn = egui::Button::new(
        RichText::new(label)
            .size(14.5)
            .strong()
            .color(Color32::from_rgb(10, 6, 2)),
    )
    .fill(THEME.primary)
    .corner_radius(10);
    if full {
        btn = btn.min_size(egui::vec2(ui.available_width(), 44.0));
    } else {
        btn = btn.min_size(egui::vec2(130.0, 40.0));
    }
    ui.add_enabled(enabled, btn)
}

pub fn secondary_button(ui: &mut Ui, label: &str, enabled: bool, full: bool) -> Response {
    let mut btn = egui::Button::new(RichText::new(label).size(14.0).color(THEME.text_hi))
        .fill(THEME.bg_panel_hi)
        .stroke(Stroke::new(1.0, THEME.border_accent))
        .corner_radius(10);
    if full {
        btn = btn.min_size(egui::vec2(ui.available_width(), 44.0));
    } else {
        btn = btn.min_size(egui::vec2(130.0, 40.0));
    }
    ui.add_enabled(enabled, btn)
}

pub fn danger_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let btn = egui::Button::new(RichText::new(label).size(13.5).color(THEME.danger))
        .fill(THEME.bg_panel_hi)
        .stroke(Stroke::new(1.0, THEME.danger))
        .corner_radius(10)
        .min_size(egui::vec2(ui.available_width(), 40.0));
    ui.add_enabled(enabled, btn)
}

pub fn ghost_button(ui: &mut Ui, label: &str) -> Response {
    let btn = egui::Button::new(RichText::new(label).size(13.0).color(THEME.text_mid))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, THEME.border))
        .corner_radius(10);
    ui.add(btn)
}

/// Semantic tone for a status pill.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Healthy / online / synced.
    Live,
    /// In-progress / pending / syncing.
    Warn,
    /// Error / failed.
    Danger,
    /// Inactive / stopped / offline.
    Off,
}

impl Tone {
    fn fg(self) -> Color32 {
        match self {
            Tone::Live => THEME.live,
            Tone::Warn => THEME.warn,
            Tone::Danger => THEME.danger,
            Tone::Off => THEME.off,
        }
    }
}

/// A rounded status pill: a coloured dot + label on a tinted, bordered chip.
/// Used for at-a-glance state (node sync, mining on/off) — matches the brand's
/// `.a-badge` / `.a-dot` components in alice-theme.html.
pub fn status_pill(ui: &mut Ui, tone: Tone, label: &str) -> Response {
    let fg = tone.fg();
    let tint = Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 26);
    let border = Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 96);
    egui::Frame::NONE
        .fill(tint)
        // u8 max — a fully-rounded ("pill") radius for a short chip.
        .corner_radius(CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .stroke(Stroke::new(1.0, border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, fg);
                ui.add_space(6.0);
                ui.label(RichText::new(label).size(11.5).strong().color(fg));
            });
        })
        .response
}

/// Map a node sync state to a status tone (green=synced, amber=syncing, …).
pub fn sync_tone(state: crate::chain::NodeSyncState) -> Tone {
    use crate::chain::NodeSyncState as S;
    match state {
        S::Synced => Tone::Live,
        S::Syncing => Tone::Warn,
        S::Stale => Tone::Warn,
        S::Offline => Tone::Off,
        S::Unavailable | S::Error => Tone::Danger,
    }
}

/// Map a supervised-process state to a status tone (green=running, …).
pub fn proc_tone(state: crate::supervise::ProcState) -> Tone {
    use crate::supervise::ProcState as P;
    match state {
        P::Running => Tone::Live,
        P::Starting | P::Stopping => Tone::Warn,
        P::Error => Tone::Danger,
        P::Stopped => Tone::Off,
    }
}

pub fn error_banner(ui: &mut Ui, msg: &str) {
    egui::Frame::NONE
        .fill(THEME.danger_bg)
        .corner_radius(10)
        .inner_margin(egui::Margin::same(12))
        .stroke(Stroke::new(1.0, THEME.danger))
        .show(ui, |ui| {
            ui.label(RichText::new(msg).size(12.5).color(THEME.danger));
        });
}

pub fn shortened_address(addr: &str) -> String {
    if addr.len() <= 14 {
        return addr.to_string();
    }
    let head: String = addr.chars().take(6).collect();
    let tail: String = addr
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{}…{}", head, tail)
}

/// Click-to-copy label. Shows "Copied" flash for 2 seconds after click.
pub fn copy_label(
    ui: &mut Ui,
    content: &str,
    display: &str,
    copied_at: &mut Option<std::time::Instant>,
    monospace: bool,
) {
    let is_copied = copied_at
        .map(|t| t.elapsed().as_secs() < 2)
        .unwrap_or(false);
    if is_copied {
        ui.ctx().request_repaint();
    } else if copied_at.is_some() {
        *copied_at = None;
    }

    let text = if is_copied {
        "Copied to clipboard"
    } else {
        display
    };
    let mut rt = RichText::new(text).size(12.5);
    if monospace && !is_copied {
        rt = rt.family(egui::FontFamily::Monospace);
    }
    rt = rt.color(if is_copied {
        THEME.primary_hi
    } else {
        THEME.text_hi
    });

    let resp = ui.add(egui::Label::new(rt).sense(egui::Sense::click()));
    if resp.clicked() {
        ui.ctx().copy_text(content.to_string());
        *copied_at = Some(std::time::Instant::now());
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Click to copy");
}

/// Password strength: 0 weak – 4 strong.
pub fn password_strength(pwd: &str) -> (u8, &'static str, Color32) {
    let len = pwd.len();
    let mut score = 0u8;
    if len >= 12 {
        score += 1;
    }
    if len >= 16 {
        score += 1;
    }
    let classes = [
        pwd.chars().any(|c| c.is_ascii_lowercase()),
        pwd.chars().any(|c| c.is_ascii_uppercase()),
        pwd.chars().any(|c| c.is_ascii_digit()),
        pwd.chars().any(|c| !c.is_ascii_alphanumeric()),
    ];
    let class_count = classes.iter().filter(|b| **b).count();
    if class_count >= 2 {
        score += 1;
    }
    if class_count >= 3 {
        score += 1;
    }
    if class_count == 4 && len >= 16 {
        score = 4;
    }
    match score {
        0 => (0, "Too weak", THEME.danger),
        1 => (1, "Weak", THEME.danger),
        2 => (2, "Fair", THEME.warn),
        3 => (3, "Strong", THEME.primary_hi),
        _ => (4, "Excellent", THEME.primary),
    }
}

pub fn strength_bar(ui: &mut Ui, pwd: &str) {
    if pwd.is_empty() {
        ui.add_space(4.0);
        return;
    }
    let (score, label, color) = password_strength(pwd);
    ui.horizontal(|ui| {
        for i in 0..4 {
            let filled = i < score;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(54.0, 5.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 3, if filled { color } else { THEME.border });
        }
        ui.add_space(6.0);
        ui.label(RichText::new(label).size(11.0).color(color));
    });
}

pub fn format_token(amount: u128) -> String {
    let whole = amount / 1_000_000_000_000;
    let frac = amount % 1_000_000_000_000;
    if frac == 0 {
        with_commas(whole)
    } else {
        let frac_str = format!("{:012}", frac);
        let frac_trimmed = frac_str.trim_end_matches('0');
        format!("{}.{}", with_commas(whole), frac_trimmed)
    }
}

fn with_commas(n: u128) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
