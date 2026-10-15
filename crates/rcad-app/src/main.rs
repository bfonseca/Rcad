//! rcad desktop application entry point.

mod app;
mod camera;
mod settings;
mod shortcuts;
mod undo;
mod viewport;
mod ui;

use std::path::PathBuf;

use app::{CaptureRequest, RcadApp};
use tracing_subscriber::EnvFilter;

/// Which eframe renderer to use.
///
/// `--renderer glow` plus Mesa software rendering is the escape hatch for
/// machines (and CI runners) without a usable GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum RendererChoice {
    #[default]
    Default,
    Glow,
    Wgpu,
}

#[derive(Debug, Default)]
struct Cli {
    screenshot: Option<PathBuf>,
    frames: u32,
    demo: bool,
    renderer: RendererChoice,
    /// Standard view to open in: iso, front, top, right, ...
    view: Option<String>,
    /// Open the feature dialog on startup (for screenshots and docs).
    dialog: bool,
}

const USAGE: &str = "\
rcad - Open Source CAD in Rust

Usage: rcad [options]

Options:
  --screenshot <path>   Render the UI, write a PNG to <path>, then exit
  --frames <n>          Frames to render before capturing (default 30)
  --demo                Start with a sample bracket instead of an empty part
  --renderer <backend>  default | glow | wgpu
  --view <name>         iso | front | back | left | right | top | bottom
  --dialog              Open the feature dialog for the first feature
  --help                Show this message
";

fn parse_args() -> Result<Cli, String> {
    let mut cli = Cli {
        frames: 30,
        ..Default::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshot" => {
                let value = args.next().ok_or("--screenshot needs a path")?;
                cli.screenshot = Some(PathBuf::from(value));
            }
            "--frames" => {
                let value = args.next().ok_or("--frames needs a number")?;
                let frames: u32 = value
                    .parse()
                    .map_err(|_| format!("invalid --frames value: {value}"))?;
                if frames == 0 {
                    return Err("--frames must be at least 1".to_string());
                }
                cli.frames = frames;
            }
            "--demo" => cli.demo = true,
            "--dialog" => cli.dialog = true,
            "--view" => {
                cli.view = Some(args.next().ok_or("--view needs a name")?);
            }
            "--renderer" => {
                let value = args.next().ok_or("--renderer needs a value")?;
                cli.renderer = match value.as_str() {
                    "default" => RendererChoice::Default,
                    "glow" => RendererChoice::Glow,
                    "wgpu" => RendererChoice::Wgpu,
                    other => return Err(format!("unknown renderer: {other}")),
                };
            }
            "--help" | "-h" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(cli)
}

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

    let cli = match parse_args() {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("rcad: {err}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1400.0, 900.0])
        .with_title("rcad");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: match cli.renderer {
            RendererChoice::Default => eframe::Renderer::default(),
            RendererChoice::Glow => eframe::Renderer::Glow,
            RendererChoice::Wgpu => eframe::Renderer::Wgpu,
        },
        ..Default::default()
    };

    let capture = cli
        .screenshot
        .clone()
        .map(|path| CaptureRequest::new(path, cli.frames));
    let demo = cli.demo;
    let view = cli.view.clone();
    let dialog = cli.dialog;

    eframe::run_native(
        "rcad",
        options,
        Box::new(move |cc| {
            let mut app = RcadApp::new(cc);
            match view.as_deref() {
                Some("iso") => app.set_view_iso(),
                Some("front") => app.set_view_front(),
                Some("back") => app.set_view_back(),
                Some("left") => app.set_view_left(),
                Some("right") => app.set_view_right(),
                Some("top") => app.set_view_top(),
                Some("bottom") => app.set_view_bottom(),
                Some(other) => tracing::warn!("unknown --view value: {other}"),
                None => {}
            }
            if demo {
                app.load_demo_part();
            }
            if dialog {
                app.open_feature_dialog_for_first_feature();
            }
            app.capture = capture;
            Ok(Box::new(app))
        }),
    )
}