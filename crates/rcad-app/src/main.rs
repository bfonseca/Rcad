//! rcad desktop application entry point.

mod app;
mod camera;
mod undo;
mod viewport;

use app::RcadApp;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("rcad=info".parse().unwrap()))
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("rcad"),
        ..Default::default()
    };

    eframe::run_native(
        "rcad",
        options,
        Box::new(|cc| Ok(Box::new(RcadApp::new(cc)))),
    )
}
