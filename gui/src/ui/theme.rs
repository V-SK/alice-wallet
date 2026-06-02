use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Stroke};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg_base: Color32,
    pub bg_panel: Color32,
    pub bg_panel_hi: Color32,
    pub bg_input: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub border_accent: Color32,
    pub primary: Color32,
    pub primary_hi: Color32,
    pub primary_dim: Color32,
    pub text_hi: Color32,
    pub text_mid: Color32,
    pub text_dim: Color32,
    pub danger: Color32,
    pub danger_bg: Color32,
    pub warning_bg: Color32,
    // Semantic status palette (mirrors alice-theme.html --a-live/--a-warn/--a-off).
    // Orange stays the brand spine; these are used only for at-a-glance state
    // (node sync, mining on/off) where colour-as-meaning aids readability.
    pub live: Color32,
    pub warn: Color32,
    pub off: Color32,
}

// Brand-aligned dark theme (see alice-website/assets/alice-theme.html):
//   ink #050505, zinc-900/800 surfaces, zinc-400/500 muted text, orange #F97316.
pub const THEME: Theme = Theme {
    // #050505 ink — the canonical Alice page background (was pure #000).
    bg_base: Color32::from_rgb(5, 5, 5),
    // zinc-900 (#18181B) raised surface, with a slightly lighter hover tier.
    bg_panel: Color32::from_rgb(24, 24, 27),
    bg_panel_hi: Color32::from_rgb(39, 39, 42), // zinc-800
    bg_input: Color32::from_rgb(9, 9, 11),      // zinc-950 — recessed inputs
    border: Color32::from_rgb(39, 39, 42),      // zinc-800 hairline
    border_strong: Color32::from_rgb(63, 63, 70), // zinc-700
    border_accent: Color32::from_rgb(249, 115, 22),
    primary: Color32::from_rgb(249, 115, 22), // orange-500 (logo)
    primary_hi: Color32::from_rgb(251, 146, 60), // orange-400
    primary_dim: Color32::from_rgb(124, 45, 18), // orange-900 — subtle active fill
    text_hi: Color32::from_rgb(250, 250, 250), // zinc-50
    text_mid: Color32::from_rgb(161, 161, 170), // zinc-400
    text_dim: Color32::from_rgb(113, 113, 122), // zinc-500 (was zinc-600)
    danger: Color32::from_rgb(239, 68, 68),   // red-500
    danger_bg: Color32::from_rgb(38, 16, 16),
    warning_bg: Color32::from_rgb(38, 26, 10),
    live: Color32::from_rgb(34, 197, 94), // green-500 — synced / online
    warn: Color32::from_rgb(245, 158, 11), // amber-500 — syncing / pending
    off: Color32::from_rgb(113, 113, 122), // zinc-500 — stopped / offline
};

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "Inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "Inter-Bold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Bold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "JBMono".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/JetBrainsMono-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "NotoSC".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansSC-Subset.ttf"
        ))),
    );

    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "Inter".into());
    prop.insert(1, "Inter-Bold".into());
    prop.insert(2, "NotoSC".into());

    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.insert(0, "JBMono".into());
    mono.insert(1, "NotoSC".into());

    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &egui::Context) {
    let t = THEME;
    let mut style = (*ctx.global_style()).clone();

    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 10.0);
    style.spacing.interact_size.y = 36.0;

    style.visuals.override_text_color = Some(t.text_hi);
    style.visuals.panel_fill = t.bg_base;
    style.visuals.window_fill = t.bg_panel;
    style.visuals.extreme_bg_color = t.bg_input;
    style.visuals.window_stroke = Stroke::new(1.0, t.border);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 16],
        blur: 48,
        spread: 0,
        color: Color32::from_rgba_premultiplied(0, 0, 0, 140),
    };
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_rgba_premultiplied(0, 0, 0, 120),
    };
    // Text-selection highlight: a translucent brand orange (matches the brand
    // `::selection { background: rgba(249,115,22,0.30) }`).
    style.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(249, 115, 22, 64);
    style.visuals.selection.stroke = Stroke::new(1.0, t.primary);
    style.visuals.hyperlink_color = t.primary;

    let w = &mut style.visuals.widgets;
    w.noninteractive.bg_fill = t.bg_panel;
    w.noninteractive.weak_bg_fill = t.bg_panel;
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text_hi);
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    w.noninteractive.corner_radius = 10.into();

    w.inactive.bg_fill = t.bg_panel_hi;
    w.inactive.weak_bg_fill = t.bg_panel_hi;
    w.inactive.fg_stroke = Stroke::new(1.0, t.text_hi);
    w.inactive.bg_stroke = Stroke::new(1.0, t.border);
    w.inactive.corner_radius = 10.into();

    w.hovered.bg_fill = t.bg_panel_hi;
    w.hovered.weak_bg_fill = t.bg_panel_hi;
    w.hovered.fg_stroke = Stroke::new(1.0, t.text_hi);
    w.hovered.bg_stroke = Stroke::new(1.0, t.border_strong);
    w.hovered.corner_radius = 10.into();

    w.active.bg_fill = t.bg_panel_hi;
    w.active.weak_bg_fill = t.bg_panel_hi;
    w.active.fg_stroke = Stroke::new(1.0, t.text_hi);
    w.active.bg_stroke = Stroke::new(1.0, t.primary);
    w.active.corner_radius = 10.into();

    w.open.bg_fill = t.bg_panel_hi;
    w.open.weak_bg_fill = t.bg_panel_hi;
    w.open.fg_stroke = Stroke::new(1.0, t.text_hi);
    w.open.bg_stroke = Stroke::new(1.0, t.primary);
    w.open.corner_radius = 10.into();

    ctx.set_global_style(style);
}

/// Pure-black backdrop. No glows. The official site is just `#000`.
pub fn paint_backdrop(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter_at(rect).rect_filled(rect, 0, THEME.bg_base);
}
