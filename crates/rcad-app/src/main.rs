//! rcad desktop application entry point.

mod app;
mod camera;
mod selection;
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
    /// Tag two demo faces with the project colours (screenshots, docs).
    demo_colours: bool,
    /// Select the tagged demo face (screenshots, docs).
    demo_select: bool,
    /// Hide datum planes, axes and points (screenshots, docs).
    hide_datums: bool,
    renderer: RendererChoice,
    /// Standard view to open in: iso, front, top, right, ...
    view: Option<String>,
    /// Open the feature dialog on startup (for screenshots and docs).
    dialog: bool,
    /// Open this project folder directly, skipping the project picker.
    project: Option<PathBuf>,
    /// Open the Options dialog on startup.
    options: bool,
    /// Open this document directly (headless, for docs and tests).
    open: Option<PathBuf>,
    /// Write the document to this path once the model is built.
    save: Option<PathBuf>,
}

const USAGE: &str = "\
rcad - Open Source CAD in Rust

Usage: rcad [options]

Options:
  --screenshot <path>   Render the UI, write a PNG to <path>, then exit
                        (uses X11 on a Wayland session, so it works headless)
  --frames <n>          Frames to render before capturing (default 30)
  --demo                Start with a sample bracket instead of an empty part
  --demo-colours        Tag two demo faces with the first two project colours
  --select-demo-face    Select the tagged demo face (with --demo-colours)
  --hide-datums         Do not draw datum planes, axes and points
  --renderer <backend>  default | glow | wgpu
  --view <name>         iso | front | back | left | right | top | bottom
  --dialog              Open the feature dialog for the first feature
  --open <file>         Open a .rcad document directly
  --save <file>         Write the .rcad document once the model is built
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
            "--demo-colours" => cli.demo_colours = true,
            "--select-demo-face" => cli.demo_select = true,
            "--hide-datums" => cli.hide_datums = true,
            "--dialog" => cli.dialog = true,
            "--options" => cli.options = true,
            "--project" => {
                cli.project = Some(PathBuf::from(args.next().ok_or("--project needs a folder")?));
            }
            "--open" => {
                cli.open = Some(PathBuf::from(args.next().ok_or("--open needs a file")?));
            }
            "--save" => {
                cli.save = Some(PathBuf::from(args.next().ok_or("--save needs a file")?));
            }
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

/// Whether a headless capture should go through X11 instead of Wayland.
///
/// On Wayland the compositor drives frame callbacks: a window that is not being
/// presented - screen locked, another workspace, a headless runner - never
/// redraws, so `--screenshot` waits for a frame that never comes and the run
/// hangs without a PNG. XWayland delivers redraws to the client, so forcing the
/// X11 backend makes captures reliable. Captures only: an interactive run keeps
/// Wayland.
fn force_x11_for_capture(screenshot: bool, wayland: Option<&str>, x11: Option<&str>) -> bool {
    screenshot && wayland.is_some() && x11.is_some()
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

    if force_x11_for_capture(
        cli.screenshot.is_some(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        std::env::var("DISPLAY").ok().as_deref(),
    ) {
        // SAFETY: this runs before the event loop exists and nothing else is
        // touching the environment; winit reads the variable when the event
        // loop is built, further down this function.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
        tracing::info!(
            "capturing through X11: Wayland needs a presented window to send frame callbacks"
        );
    }

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
    let demo_colours = cli.demo_colours;
    let demo_select = cli.demo_select;
    let hide_datums = cli.hide_datums;
    let view = cli.view.clone();
    let dialog = cli.dialog;
    let project = cli.project.clone();
    let open_document = cli.open.clone();
    let save_document = cli.save.clone();
    let open_options = cli.options;

    eframe::run_native(
        "rcad",
        options,
        Box::new(move |cc| {
            let mut app = RcadApp::new(cc);
            app.restore_last_project();
            if let Some(folder) = project.clone() {
                app.open_project_folder(folder);
            }
            if let Some(path) = open_document.clone() {
                if let Err(err) = app.open_document_at(path) {
                    tracing::error!("{err}");
                }
            }
            if open_options {
                app.open_options();
            }
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
            if demo_colours {
                app.tag_demo_faces_with_project_colours();
            }
            if demo_select {
                app.select_demo_face_for_screenshot();
            }
            if hide_datums {
                app.set_datums_visible(None, false);
            }
            if let Some(path) = save_document.clone() {
                app.request_save(path);
            }
            if dialog {
                app.open_feature_dialog_for_first_feature();
            }
            app.capture = capture;
            Ok(Box::new(app))
        }),
    )
}
#[cfg(test)]
mod tests {
    use super::force_x11_for_capture;

    /// The bug this fixes: on Wayland the compositor drives frame callbacks, so
    /// an unpresented window never redraws and the capture hangs. XWayland
    /// delivers redraws to the client, so a capture must switch to X11.
    #[test]
    fn a_capture_on_a_wayland_session_goes_through_x11() {
        assert!(force_x11_for_capture(true, Some("wayland-0"), Some(":0")));
    }

    #[test]
    fn an_interactive_run_keeps_wayland() {
        assert!(!force_x11_for_capture(false, Some("wayland-0"), Some(":0")));
    }

    /// A pure Wayland machine has no X11 to switch to; leave it alone.
    #[test]
    fn a_wayland_session_without_x11_is_left_alone() {
        assert!(!force_x11_for_capture(true, Some("wayland-0"), None));
    }

    #[test]
    fn a_plain_x11_session_needs_no_change() {
        assert!(!force_x11_for_capture(true, None, Some(":0")));
    }
}
