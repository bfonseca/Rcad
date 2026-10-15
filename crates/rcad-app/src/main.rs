//! rcad desktop application entry point.

mod app;
mod camera;
mod shortcuts;
mod undo;
mod viewport;
mod ui;

use app::RcadApp;
use tracing_subscriber::EnvFilter;

/// Window icon, decoded from the committed artwork in `Images/`.
///
/// Downscaled to 256px: the 512px asset is 56 KB on disk but a full RGBA
/// bitmap in memory, and winit only ever needs a small icon.
fn window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../../Images/rcad-app-icon-512.png");
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
    let rgba = decoded.to_rgba8();
    let resized = image::imageops::resize(&rgba, 256, 256, image::imageops::FilterType::Lanczos3);
    Some(egui::IconData {
        rgba: resized.into_raw(),
        width: 256,
        height: 256,
    })
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("rcad=info".parse().unwrap()))
        .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1400.0, 900.0])
        .with_title("rcad");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "rcad",
        options,
        Box::new(|cc| Ok(Box::new(RcadApp::new(cc)))),
    )
}
