// The display-independent wallet core now lives in the `gui` LIBRARY crate
// (`src/lib.rs`). Re-import those modules at the bin's crate root so the
// unchanged `crate::chain` / `crate::config` / … paths inside `app` and `ui`
// keep resolving here, while a future display-free binary can depend on the
// same library without linking eframe/egui.
use gui::{chain, config, crypto, history, i18n, miner, node, supervise, update, wallet_profiles};

// GUI-only modules — the SOLE consumers of eframe/egui. Kept in the binary so
// the library stays display-free.
mod app;
mod ui;

use eframe::egui::IconData;

fn load_icon() -> Option<IconData> {
    use usvg::TreeParsing;
    // Canonical Alice mark, bundled in the wallet's own assets dir (byte-identical
    // to alice-website/assets/alice-logo.svg). Rasterised here for the OS window
    // icon; egui needs raster for the title-bar/dock icon.
    let svg_data = include_bytes!("../assets/brand/alice-logo.svg");

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &opt).ok()?;

    // Render at 64x64 resolution for the window icon
    let width = 64;
    let height = 64;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;

    let size = tree.size;
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
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
        .with_title("Alice Wallet")
        // Draw our own dark header flush to the window top instead of a
        // system-colored title bar clashing with the dark theme. The title bar
        // stays present (so the window is still OS-draggable + the traffic
        // lights work), just transparent with the title text hidden. macOS-only
        // effect; no-op elsewhere.
        .with_fullsize_content_view(true)
        .with_title_shown(false)
        .with_titlebar_buttons_shown(true);

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
