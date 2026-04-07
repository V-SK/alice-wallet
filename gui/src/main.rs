mod app;
mod chain;
mod config;
mod crypto;
mod history;
mod i18n;
mod ui;

use eframe::egui::IconData;

fn load_icon() -> Option<IconData> {
    use usvg::TreeParsing;
    let svg_data = include_bytes!("../alice-logo-traced.svg");

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &opt).ok()?;

    // Render at 64x64 resolution for the window icon
    let width = 64;
    let height = 64;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;

    let size = tree.size;
    let sx = width as f32 / size.width() as f32;
    let sy = height as f32 / size.height() as f32;
    let scale = sx.min(sy);
    let transform = tiny_skia::Transform::from_scale(scale, scale);

    resvg::Tree::from_usvg(&tree).render(transform, &mut pixmap.as_mut());

    let image_data = pixmap.data().to_vec();

    Some(IconData {
        rgba: image_data,
        width,
        height,
    })
}

fn main() -> eframe::Result {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([1040.0, 680.0])
        .with_title("Alice Wallet");

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Alice Wallet",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            ui::theme::install_fonts(&cc.egui_ctx);
            Ok(Box::new(app::AliceWalletApp::new(rt)))
        }),
    )
}
