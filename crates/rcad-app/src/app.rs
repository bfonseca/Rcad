use eframe::egui;
use crate::selection::{ClickMode, Selected, Selection};
use crate::viewport::{draw_viewport, pick_candidates};
use crate::camera::{OrbitCamera, Projection};
use crate::settings::AppSettings;
use crate::undo::UndoStack;
use crate::shortcuts::{Shortcuts, handle_shortcuts, shortcuts_list};
use crate::ui::{
    DatumKind, FloatingAction, FloatingBar, HistoryOverlay, HistoryOverlayAction, ToolbarAction,
    VerticalToolbar,
};
use rcad_geom::{GeometryBackend, TriangleMesh};
use rcad_doc::{Project, EdgeSet, Feature, FeatureKind, ConstructionKind, Part, ProjectManifest};
use rcad_features;
use rcad_occt::OcctBackend;
use rcad_param::{ShapeCache};
use rcad_sketch::Sketch;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use crossbeam_channel::{bounded, Sender, Receiver};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

// Async kernel worker types
pub struct RebuildJob {
    pub version: u64,
    pub part: Part,
}

pub struct RebuildResponse {
    pub version: u64,
    pub mesh: TriangleMesh,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // face/edge/vertex picking is not implemented yet
pub enum SelectionMode {
    Object,
    Face,
    Edge,
    Vertex,
}

/// Headless screenshot mode (see `--screenshot`).
///
/// Renders `frames` frames, asks egui for the framebuffer, writes it to `path`
/// and quits. This is how the UI can be verified without a person watching the
/// screen, and how documentation images are produced.
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub path: PathBuf,
    pub frames: u32,
    requested: bool,
    rendered: u32,
}

impl CaptureRequest {
    pub fn new(path: PathBuf, frames: u32) -> Self {
        Self {
            path,
            frames,
            requested: false,
            rendered: 0,
        }
    }
}

pub struct RcadApp {
    camera: OrbitCamera,
    mesh: TriangleMesh,
    project: Project,
    current_path: Option<PathBuf>,
    occt_backend: OcctBackend,
    sketch: Option<Sketch>,
    last_error: Option<String>,
    undo_stack: UndoStack,
    shortcuts: Shortcuts,
    show_shortcuts_dialog: bool,
    
    // New UI components
    toolbar: VerticalToolbar,
    history_overlay: HistoryOverlay,
    
    // Relations dialog
    show_relations_dialog: Option<uuid::Uuid>,
    
    // Dragged feature for reordering
    dragged_feature: Option<uuid::Uuid>,
    drop_target: Option<uuid::Uuid>,
    
    /// The one ordered selection, shared by the viewport and the model tree.
    selection: Selection,
    /// What the cursor is over, if anything.
    hover: Option<Selected>,
    /// Candidates under the cursor, most specific first: a right-click cycles
    /// through them, the way Creo lets a user reach the face under an edge.
    hover_candidates: Vec<Selected>,
    hover_cycle: usize,
#[allow(dead_code)] // set but not read until picking modes exist
    selection_mode: SelectionMode,
    
    // Async kernel worker
    kernel_worker_tx: Option<Sender<RebuildJob>>,
    kernel_worker_rx: Option<Receiver<RebuildResponse>>,
    kernel_version: Arc<AtomicU64>,
#[allow(dead_code)] // keeps the worker thread alive
    kernel_worker_handle: Option<thread::JoinHandle<()>>,
    /// Set whenever the part tree changes. The kernel worker only rebuilds
    /// when this is set; previously a job was queued on *every frame*.
    needs_rebuild: bool,
    /// Set by `--screenshot` to capture the next rendered frame and exit.
    pub capture: Option<CaptureRequest>,
    /// Frame the model as soon as the first mesh arrives.
    needs_fit: bool,
    /// Which tab the feature dialog shows (0 parameters, 1 relations).
    feature_dialog_tab: usize,
    /// Draft values for the feature dialog's "add parameter" row.
    new_parameter: NewParameterDraft,
    /// Quick-action strip floating over the viewport.
    floating_bar: FloatingBar,
    /// True while placing a component: the floating bar then shows the
    /// assembly commands, which are otherwise hidden.
    placing_component: bool,
    /// User preferences, loaded at startup and saved when they change.
    settings: AppSettings,
    /// Last settings written to disk, so we only save on an actual change.
    saved_settings: AppSettings,
    /// Whether the Options dialog is open.
    show_settings: bool,
    /// Informational message for the status bar (measurement results etc).
    status: Option<String>,
    /// Folder of the active project. Modelling is unreachable without one.
    project_root: Option<PathBuf>,
    /// The active project's rcad.toml: tolerances, globals, functions, colours.
    manifest: ProjectManifest,
    /// Why the last attempt to open a project failed.
    project_error: Option<String>,
    /// Text being typed in Options to add a colour id to the vocabulary.
    new_colour_id: String,
    /// Set by `--demo-colours`: tag two visible demo faces with the project's
    /// first two colour ids once the mesh has been tessellated.
    pending_demo_colours: bool,
    /// Set by `--select-demo-face`: after the demo colours are applied, select
    /// the tagged face so a screenshot documents the selection bar.
    pending_demo_select: bool,
    /// Set by `--save`: write the document once the model is built and the
    /// demo colours (if any) have been applied.
    pending_save: Option<PathBuf>,
    /// True once a rebuild response has been applied, so headless save can wait
    /// for the real model instead of writing an empty one.
    mesh_ready: bool,
}

/// A feature driven by an edge set: a round or a break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeFeature {
    Fillet,
    Chamfer,
}

impl EdgeFeature {
    fn name(self) -> &'static str {
        match self {
            EdgeFeature::Fillet => "Round",
            EdgeFeature::Chamfer => "Chamfer",
        }
    }
}

/// A Creo-style smart-selection query, offered on a selected edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionQuery {
    /// The boundary loop of every face the seed edge bounds.
    Boundary,
    /// Every edge that continues the seed edge tangentially.
    TangentChain,
    /// Every edge of the surfaces the seed edge bounds.
    SurfaceRing,
}

/// Transient state for the feature dialog's "add parameter" row.
#[derive(Debug, Clone, Default)]
pub struct NewParameterDraft {
    pub name: String,
    /// 0 = real, 1 = string, 2 = boolean.
    pub kind: u8,
    pub value: String,
}

impl RcadApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let settings = AppSettings::load();
        let (job_tx, job_rx) = bounded::<RebuildJob>(2);
        let (res_tx, res_rx) = bounded::<RebuildResponse>(2);
        let version_counter = Arc::new(AtomicU64::new(0));
        let version_worker = version_counter.clone();
        
        let kernel_worker_handle = thread::Builder::new()
            .name("rcad-kernel-worker".into())
            .spawn(move || {
                let mut backend = rcad_occt::OcctBackend::default();
                let mut cache = ShapeCache::new();
                while let Ok(job) = job_rx.recv() {
                    // Check if a newer job was already queued; skip stale frames
                    // Versions are 1-based: a queued job is stale only when a
                    // strictly newer one has been requested. The old check used
                    // the pre-increment value, so it discarded every job.
                    if job.version < version_worker.load(Ordering::Relaxed) {
                        continue;
                    }
                    let res = cache.rebuild_part_incremental(&job.part, &mut backend);
                    let _ = res_tx.send(RebuildResponse {
                        version: job.version,
                        mesh: res.display_mesh,
                        errors: res.errors,
                    });
                }
            })
            .expect("Failed to spawn kernel worker thread");
        
        let mut this = Self {
            camera: OrbitCamera::default(),
            mesh: TriangleMesh::default(),
            project: Project::default(),
            current_path: None,
            occt_backend: OcctBackend::default(),
            sketch: None,
            last_error: None,
            undo_stack: UndoStack::default(),
            shortcuts: Shortcuts::default(),
            show_shortcuts_dialog: false,
            
            // New UI components
            toolbar: VerticalToolbar::default(),
            history_overlay: HistoryOverlay::default(),
            
            // Relations dialog
            show_relations_dialog: None,
            
            dragged_feature: None,
            drop_target: None,
            
            // Selection state
            selection: Selection::default(),
            hover: None,
            hover_candidates: Vec::new(),
            hover_cycle: 0,
            selection_mode: SelectionMode::Object,
            
            // Async kernel worker
            kernel_worker_tx: Some(job_tx),
            kernel_worker_rx: Some(res_rx),
            kernel_version: version_counter,
            kernel_worker_handle: Some(kernel_worker_handle),
            needs_rebuild: true,
            capture: None,
            needs_fit: true,
            feature_dialog_tab: 0,
            new_parameter: NewParameterDraft::default(),
            floating_bar: FloatingBar::default(),
            placing_component: false,
            settings: settings.clone(),
            saved_settings: settings,
            show_settings: false,
            status: None,
            project_root: None,
            manifest: ProjectManifest::default(),
            project_error: None,
            new_colour_id: String::new(),
            pending_demo_colours: false,
            pending_demo_select: false,
            pending_save: None,
            mesh_ready: false,
        };
        
        // Create default datum geometry (coordinate system + datum planes) for new part
        let _ = this.create_default_datum_geometry();
        this
    }

    fn create_default_datum_geometry(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(part) = self.get_active_part_mut() {
            // Create coordinate system with axes (X, Y, Z) - these are construction features
            let cs_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "CoordinateSystem",
                FeatureKind::Construction {
                    kind: ConstructionKind::CoordSystem,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 0.0, 1.0],
                },
            ));
            if let Some(f) = part.features.last_mut() {
                f.id = cs_id;
            }
            
            // Create X axis
            let x_axis_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "X_AXIS",
                FeatureKind::Construction {
                    kind: ConstructionKind::Axis,
                    origin: [0.0, 0.0, 0.0],
                    direction: [1.0, 0.0, 0.0],
                },
            ));
            if let Some(f) = part.features.last_mut() {
                f.id = x_axis_id;
            }
            
            // Create Y axis
            let y_axis_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "Y_AXIS",
                FeatureKind::Construction {
                    kind: ConstructionKind::Axis,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 1.0, 0.0],
                },
            ));
            if let Some(f) = part.features.last_mut() {
                f.id = y_axis_id;
            }
            
            // Create Z axis
            let z_axis_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "Z_AXIS",
                FeatureKind::Construction {
                    kind: ConstructionKind::Axis,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 0.0, 1.0],
                },
            ));
            if let Some(f) = part.features.last_mut() {
                f.id = z_axis_id;
            }
            
            // Create datum planes
            // Front plane: the XZ plane, normal Y (Z is up).
            let front_plane_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "FRONT_PLANE",
                FeatureKind::Construction {
                    kind: ConstructionKind::Plane,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 1.0, 0.0], // Normal = Y
                },
            ));
            if let Some(f) = part.features.last_mut() {
                f.id = front_plane_id;
            }
            
            // Right plane (YZ)
            let right_plane_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "RIGHT_PLANE",
                FeatureKind::Construction {
                    kind: ConstructionKind::Plane,
                    origin: [0.0, 0.0, 0.0],
                    direction: [1.0, 0.0, 0.0], // Normal = X
                },
            ));
            if let Some(f) = part.features.last_mut() {
                f.id = right_plane_id;
            }
            
            // Top plane: the XY plane, normal Z.
            let top_plane_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "TOP_PLANE",
                FeatureKind::Construction {
                    kind: ConstructionKind::Plane,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 0.0, 1.0], // Normal = Z
                },
            ));
            if let Some(f) = part.features.last_mut() {
                f.id = top_plane_id;
            }
            
            // Rebuild to show datum geometry
            self.rebuild_active_part();
        }
        Ok(())
    }

    /// Rebuild the active part in the app-owned kernel and return the shape ids
    /// of the resulting body chain (last entry = active body).
    fn export_shape_ids(&mut self) -> Result<Vec<rcad_geom::ShapeId>, String> {
        let part = self
            .get_active_part()
            .cloned()
            .ok_or_else(|| "no active part".to_string())?;
        let result = rcad_param::rebuild_part(&part, &mut self.occt_backend);
        let ids: Vec<rcad_geom::ShapeId> = result.shapes.iter().map(|(_, id)| *id).collect();
        if ids.is_empty() {
            let detail = if result.errors.is_empty() {
                "the part contains no solid body".to_string()
            } else {
                result.errors.join("; ")
            };
            return Err(format!("nothing to export: {detail}"));
        }
        if !result.errors.is_empty() {
            self.last_error = Some(format!(
                "Exported with warnings, some features failed: {}",
                result.errors.join("; ")
            ));
        }
        Ok(ids)
    }

    /// Write the active body as binary/ASCII STL through the OCCT kernel.
    ///
    /// This used to write a hard-coded single-degenerate-triangle file, so
    /// "Export STL" silently produced a useless document.
    fn export_stl_internal(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let ids = self.export_shape_ids()?;
        let body = *ids.last().expect("export_shape_ids checked non-empty");
        self.occt_backend.write_stl(&[body], path)?;
        Ok(())
    }

    /// Write the active body as STEP through the OCCT kernel.
    fn export_step_internal(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let ids = self.export_shape_ids()?;
        let body = *ids.last().expect("export_shape_ids checked non-empty");
        self.occt_backend.write_step(&[body], path)?;
        Ok(())
    }

    /// Draw one frame of the whole application.
    ///
    /// Kept free of eframe types so tests can drive the UI directly with
    /// egui_kittest: click a button, then assert on the resulting state.
    pub fn ui(&mut self, ctx: &egui::Context) {
        // Nothing can be modelled before a project exists: every component
        // reads the project tolerances, variables and colours.
        if self.project_root.is_none() {
            self.draw_project_gate(ctx);
            return;
        }

        // Handle keyboard shortcuts
        handle_shortcuts(self, ctx, &self.shortcuts.clone());
        
        // Show shortcuts dialog if requested
        let show_dialog = self.show_shortcuts_dialog;
        if show_dialog {
            let mut open = true;
            egui::Window::new("Keyboard Shortcuts")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    self.draw_shortcuts_dialog(ui);
                });
            self.show_shortcuts_dialog = open;
        }
        
        // Toolbar action handler
        let mut toolbar_action: Option<ToolbarAction> = None;
        let mut history_action: Option<HistoryOverlayAction> = None;
        let mut floating_action: Option<FloatingAction> = None;
        
        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // File menu
                ui.menu_button("File", |ui| {
                    if ui.button("New  Ctrl+N").clicked() {
                        self.new_document();
                        ui.close();
                    }
                    if ui.button("Open  Ctrl+O").clicked() {
                        self.open_document();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Save  Ctrl+S").clicked() {
                        self.save_document();
                        ui.close();
                    }
                    if ui.button("Save As  Ctrl+Shift+S").clicked() {
                        self.save_document_as();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Export STL").clicked() {
                        self.export_stl();
                        ui.close();
                    }
                    if ui.button("Export STEP").clicked() {
                        self.export_step();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                });
                
                // Edit menu
                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo  Ctrl+Z").clicked() {
                        self.undo();
                        ui.close();
                    }
                    if ui.button("Redo  Ctrl+Y").clicked() {
                        self.redo();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Delete  Del").clicked() {
                        self.delete_selected();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Select All  Ctrl+A").clicked() {
                        self.select_all();
                        ui.close();
                    }
                    if ui.button("Deselect  Esc").clicked() {
                        self.deselect();
                        ui.close();
                    }
                });
                
                // View menu
                ui.menu_button("View", |ui| {
                    if ui.button("Front  1").clicked() {
                        self.set_view_front();
                        ui.close();
                    }
                    if ui.button("Back  2").clicked() {
                        self.set_view_back();
                        ui.close();
                    }
                    if ui.button("Left  3").clicked() {
                        self.set_view_left();
                        ui.close();
                    }
                    if ui.button("Right  4").clicked() {
                        self.set_view_right();
                        ui.close();
                    }
                    if ui.button("Top  5").clicked() {
                        self.set_view_top();
                        ui.close();
                    }
                    if ui.button("Bottom  6").clicked() {
                        self.set_view_bottom();
                        ui.close();
                    }
                    if ui.button("Isometric  7").clicked() {
                        self.set_view_iso();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Reset View  Home").clicked() {
                        self.reset_view();
                        ui.close();
                    }
                    ui.separator();
                    // CAD is normally viewed orthographically; perspective is
                    // for a quick sense of depth.
                    if ui
                        .radio(
                            self.camera.projection == Projection::Orthographic,
                            "Orthographic",
                        )
                        .clicked()
                    {
                        self.camera.projection = Projection::Orthographic;
                        ui.close();
                    }
                    if ui
                        .radio(
                            self.camera.projection == Projection::Perspective,
                            "Perspective",
                        )
                        .clicked()
                    {
                        self.camera.projection = Projection::Perspective;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Shaded").clicked() {
                        // TODO: Set shaded mode
                        ui.close();
                    }
                    if ui.button("Wireframe").clicked() {
                        // TODO: Set wireframe mode
                        ui.close();
                    }
                    if ui.button("Zoom Fit").clicked() {
                        self.zoom_fit();
                        ui.close();
                    }
                });
                
                // Insert menu
                ui.menu_button("Insert", |ui| {
                    if ui.button("Sketch  K").clicked() {
                        self.new_sketch();
                        ui.close();
                    }
                    if ui.button("Box  B").clicked() {
                        self.add_box_feature();
                        ui.close();
                    }
                    if ui.button("Extrude  E").clicked() {
                        self.extrude_sketch();
                        ui.close();
                    }
                    if ui.button("Revolve  V").clicked() {
                        self.revolve_sketch();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Datum Plane").clicked() {
                        // TODO: Datum plane
                        ui.close();
                    }
                    if ui.button("Datum Axis").clicked() {
                        // TODO: Datum axis
                        ui.close();
                    }
                    if ui.button("Datum Point").clicked() {
                        // TODO: Datum point
                        ui.close();
                    }
                    if ui.button("Coordinate System").clicked() {
                        // TODO: Coordinate system
                        ui.close();
                    }
                });
                
                // Tools menu
                ui.menu_button("Tools", |ui| {
                    if ui.button("Parameters").clicked() {
                        // TODO: Parameters dialog
                        ui.close();
                    }
                    if ui.button("Relations").clicked() {
                        // TODO: Relations dialog
                        ui.close();
                    }
                    if ui.button("Family Table").clicked() {
                        // TODO: Family table
                        ui.close();
                    }
                    if ui.button("Options").clicked() {
                        // TODO: Options dialog
                        ui.close();
                    }
                });
                
                // Help menu
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        // TODO: About dialog
                        ui.close();
                    }
                    if ui.button("Shortcuts  F1").clicked() {
                        self.show_shortcuts_help();
                        ui.close();
                    }
                });
            });
        });
        
        // 1) Vertical toolbar (left or right).
        //
        // Resizable rather than fixed: the width is stored in the settings so
        // the user only has to set it once.
        let toolbar_position = self.toolbar.position;
        let panel = if toolbar_position == crate::ui::ToolbarPosition::Left {
            egui::SidePanel::left("vertical_toolbar")
        } else {
            egui::SidePanel::right("vertical_toolbar")
        };
        let panel = panel
            .resizable(true)
            .default_width(self.settings.toolbar_width)
            .width_range(120.0..=420.0);
        let panel = panel.show(ctx, |ui| {
            self.toolbar.show(ui, &mut |action| toolbar_action = Some(action));
        });
        let actual_width = panel.response.rect.width();
        if (actual_width - self.settings.toolbar_width).abs() > 0.5 {
            self.settings.toolbar_width = actual_width;
        }

        
        // 2) Bottom panel: status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let eye = self.camera.eye();
                ui.label(format!("Camera: [{:.1}, {:.1}, {:.1}]", eye.x, eye.y, eye.z));
                ui.separator();
                match &self.last_error {
                    Some(e) => ui.colored_label(egui::Color32::LIGHT_RED, e),
                    None => match &self.status {
                        Some(status) => ui.label(status),
                        None => ui.label("rcad - Open Source CAD in Rust"),
                    },
                };

                // Pick a face in the viewport, then tag it with a project colour.
                self.draw_face_colour_control(ui);
            });
        });
        
        // 2b) Selection bar: the ordered selection, the way Creo shows it.
        self.draw_selection_bar(ctx);

        // 3) Central panel: viewport + history overlay
        egui::CentralPanel::default().show(ctx, |ui| {
            // Viewport with picking
            let part_clone = self.get_active_part().cloned();
            let selection_before = self.selection.clone();

            let response = draw_viewport(
                ui,
                &self.mesh,
                &mut self.camera,
                None,
                &selection_before,
                self.hover.as_ref(),
                part_clone.as_ref(),
                &self.manifest,
                self.settings.show_grid,
                self.settings.grid_spacing,
            );

            // Quick actions floating over the top of the viewport.
            let sketching = self.sketch.is_some();
            self.floating_bar
                .show(
                    ui,
                    part_clone.as_ref(),
                    sketching,
                    self.placing_component,
                    &mut |action| {
                        floating_action = Some(action);
                    },
                );
            
            // Picking: what is under the cursor, and what a click means.
            //
            // Creo behaviour: an edge beats the face it lies on and a datum
            // beats the face behind it, so the candidate list is ordered most
            // specific first. A right-click cycles through it; a left-click
            // selects the highlighted candidate, Ctrl toggles and Shift extends,
            // keeping the order the user selected in.
            if response.hovered() {
                let pos = ui
                    .input(|i| i.pointer.hover_pos())
                    .unwrap_or(response.rect.center());
                let candidates = pick_candidates(
                    pos,
                    response.rect,
                    &self.mesh,
                    &self.camera,
                    part_clone.as_ref(),
                );
                if candidates != self.hover_candidates {
                    self.hover_candidates = candidates;
                    self.hover_cycle = 0;
                }
                if ui.input(|i| i.pointer.secondary_clicked()) && !self.hover_candidates.is_empty() {
                    self.hover_cycle = (self.hover_cycle + 1) % self.hover_candidates.len();
                }
                self.hover = self.hover_candidates.get(self.hover_cycle).cloned();

                if ui.input(|i| i.pointer.primary_clicked()) {
                    let mode = ui.input(|i| ClickMode::from_modifiers(&i.modifiers));
                    match self.hover.clone() {
                        Some(item) => {
                            self.selection.click(item, mode);
                        }
                        // A plain click on empty space clears the selection.
                        None if !mode.toggle && !mode.extend => self.selection.clear(),
                        None => {}
                    }
                }
            } else {
                self.hover = None;
                self.hover_candidates.clear();
                self.hover_cycle = 0;
            }

            // History overlay (right side, over viewport)
            if self.history_overlay.visible {
                if let Some(part) = part_clone {
                    let selection = self.selection.clone();
                    self.history_overlay.show(
                        ui,
                        &part,
                        &selection,
                        &mut |action| history_action = Some(action),
                    );
                }
            }
            // A drag on the tree resize handle is remembered too.
            self.settings.history_width = self.history_overlay.width;
            
            // Handle async kernel worker responses
            if let Some(rx) = &self.kernel_worker_rx {
                while let Ok(response) = rx.try_recv() {
                    // Only apply if this is the latest version
                    if response.version >= self.kernel_version.load(Ordering::Relaxed) {
                        self.mesh = response.mesh;
                        self.mesh_ready = true;
                        // Frame the model the first time it appears: the app
                        // previously opened at a fixed camera distance.
                        if self.needs_fit {
                            if let Some((min, max)) = self.mesh.bounds() {
                                self.camera.fit_to_bounds(min, max);
                                self.needs_fit = false;
                            }
                        }
                        if !response.errors.is_empty() {
                            self.last_error = Some(response.errors.join("\n"));
                        }
                    }
                }
            }
            // Run outside the receiver borrow: neither of these rebuilds
            // geometry, the next frame simply resolves the colours differently.
            self.apply_pending_demo_colours();
            self.report_unmatched_face_colours();
            self.flush_pending_save();

            // Trigger the kernel worker only when the part tree actually
            // changed. This block used to queue a full rebuild on every frame,
            // keeping a core busy for the whole session.
            if self.needs_rebuild {
                let tx = self.kernel_worker_tx.clone();
                if let Some(tx) = tx {
                    match self.get_active_part().cloned() {
                        Some(part) => {
                            // Versions are 1-based so the worker can tell the
                            // newest queued job from a superseded one.
                            let version =
                                self.kernel_version.fetch_add(1, Ordering::Relaxed) + 1;
                            if tx.try_send(RebuildJob { version, part }).is_ok() {
                                self.needs_rebuild = false;
                            }
                        }
                        None => self.needs_rebuild = false,
                    }
                }
            }
        });
        
        // Process toolbar actions
        if let Some(action) = toolbar_action {
            self.handle_toolbar_action(action);
        }
        
        // Process history overlay actions
        if let Some(action) = history_action {
            self.handle_history_action(action);
        }

        // Process floating quick-bar actions
        if let Some(action) = floating_action {
            self.handle_floating_action(action);
        }
        
        // Show relations dialog if requested
        if let Some(feature_id) = self.show_relations_dialog {
            let mut open = true;
            egui::Window::new("Feature")
                .open(&mut open)
                .resizable(true)
                .default_size([500.0, 400.0])
                .show(ctx, |ui| {
                    self.draw_relations_dialog(ui, feature_id);
                });
            if !open {
                self.show_relations_dialog = None;
            }
        }

        if self.show_settings {
            let mut open = true;
            egui::Window::new("Options")
                .open(&mut open)
                .resizable(false)
                .default_width(320.0)
                .show(ctx, |ui| self.draw_settings_dialog(ui));
            if !open {
                self.show_settings = false;
            }
        }

        // Persist preferences as soon as they change.
        if self.settings != self.saved_settings {
            if let Err(err) = self.settings.save() {
                // A read-only config directory (container, sandbox) must not
                // break the app; keep the values for this session.
                tracing::debug!("could not save settings: {err}");
            }
            self.saved_settings = self.settings.clone();
        }

        // Headless screenshot support (see --screenshot).
        self.handle_capture(ctx);
    }

    /// Open the project in `root`, creating an `rcad.toml` there when the folder
    /// has none. Nothing can be modelled until this has succeeded.
    pub fn open_project_folder(&mut self, root: PathBuf) {
        if !ProjectManifest::path_in(&root).exists() {
            let name = Self::project_name_from_folder(&root);
            if let Err(err) = ProjectManifest::create_default(&name).save(&root) {
                self.project_error = Some(format!(
                    "Could not create a project in {}: {err}",
                    root.display()
                ));
                return;
            }
        }

        match ProjectManifest::load(&root) {
            Ok(manifest) => {
                let name = manifest.project.name.clone();
                self.manifest = manifest;
                self.settings.remember_project(&root);
                self.project_root = Some(root);
                self.project_error = None;
                self.status = Some(format!("Project '{name}' is open."));
            }
            Err(err) => self.project_error = Some(err),
        }
    }

    /// Reopen the last project, if it still exists and asks to be reopened.
    pub fn restore_last_project(&mut self) {
        let Some(last) = self.settings.last_project.clone() else {
            return;
        };
        let root = PathBuf::from(&last);
        if !ProjectManifest::path_in(&root).exists() {
            return;
        }
        if let Ok(manifest) = ProjectManifest::load(&root) {
            if !manifest.startup.auto_open_last {
                return;
            }
        }
        self.open_project_folder(root);
    }

    /// Open the Options dialog (also used by the screenshot mode).
    pub fn open_options(&mut self) {
        self.show_settings = true;
    }

    /// The welcome screen: shown instead of the modelling UI until a project is
    /// open, because every component needs the project's definitions.
    fn draw_project_gate(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading("rcad");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("open a project to start modelling")
                        .size(12.0)
                        .color(egui::Color32::GRAY),
                );
                ui.add_space(20.0);
                ui.label(
                    "A project is the folder where your parts, assemblies and skeletons\n\
                     are saved, together with the definitions they all share:\n\
                     tolerances, global variables, project functions, and the colour\n\
                     values used across the project.",
                );
                ui.add_space(22.0);
                ui.horizontal(|ui| {
                    if ui.button("  Create a new project…  ").clicked() {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_title("Choose the folder for the new project")
                            .pick_folder()
                        {
                            self.open_project_folder(folder);
                        }
                    }
                    if ui.button("  Open an existing project…  ").clicked() {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_title("Choose the project folder")
                            .pick_folder()
                        {
                            self.open_project_folder(folder);
                        }
                    }
                });

                if let Some(error) = self.project_error.clone() {
                    ui.add_space(14.0);
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }

                if !self.settings.recent_projects.is_empty() {
                    ui.add_space(24.0);
                    ui.separator();
                    ui.label(egui::RichText::new("Recent projects").strong());
                    ui.add_space(4.0);
                    let recents = self.settings.recent_projects.clone();
                    for path in recents {
                        if ui.link(&path).clicked() {
                            self.open_project_folder(PathBuf::from(path));
                        }
                    }
                }
            });
        });
    }

    /// Name a new project after its folder.
    fn project_name_from_folder(root: &Path) -> String {
        root.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled project".to_string())
    }
    /// Options dialog: the settings that survive a restart.
    fn draw_settings_dialog(&mut self, ui: &mut egui::Ui) {
        // The project comes first: it is what every component shares.
        if let Some(root) = self.project_root.clone() {
            ui.strong("Project");
            ui.label(
                egui::RichText::new(root.display().to_string())
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            );
            ui.horizontal(|ui| {
                ui.label("name");
                ui.text_edit_singleline(&mut self.manifest.project.name);
            });
            ui.horizontal(|ui| {
                ui.label("linear tolerance");
                ui.add(
                    egui::DragValue::new(&mut self.manifest.general_tolerance.linear)
                        .speed(0.001)
                        .range(0.0..=1.0),
                );
            });
            ui.label(
                egui::RichText::new(format!(
                    "{} global variable(s), {} function(s), {} colour(s) defined here",
                    self.manifest.global_parameters.len(),
                    self.manifest.functions.len(),
                    self.manifest.surface_colors.len(),
                ))
                .size(10.0)
                .color(egui::Color32::GRAY),
            );
            if ui.button("Save the project settings").clicked() {
                match self.manifest.save(&root) {
                    Ok(()) => self.status = Some("Project settings saved.".to_string()),
                    Err(err) => self.last_error = Some(err),
                }
            }
            ui.separator();
        }

        ui.strong("Tool palette");
        ui.add(
            egui::Slider::new(&mut self.settings.toolbar_width, 120.0..=420.0)
                .text("width"),
        );
        ui.separator();

        ui.strong("Model tree");
        ui.add(
            egui::Slider::new(&mut self.settings.history_width, 180.0..=600.0)
                .text("width"),
        );
        // Push the value into the overlay so the slider takes effect at once.
        self.history_overlay.width = self.settings.history_width;
        ui.separator();

        ui.strong("Grid");
        ui.checkbox(&mut self.settings.show_grid, "Show the ground grid");
        ui.add_enabled(
            self.settings.show_grid,
            egui::Slider::new(&mut self.settings.grid_spacing, 1.0..=100.0).text("spacing"),
        );
        ui.separator();

        ui.strong("Colour vocabulary");
        ui.label(
            egui::RichText::new(
                "Names for logical surfaces. The RGB values for these names live in\n\
                 each project, so two projects can colour the same name differently.",
            )
            .size(10.0)
            .color(egui::Color32::GRAY),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_colour_id)
                    .hint_text("new colour id")
                    .desired_width(140.0),
            );
            if ui.button("Add").clicked() {
                let id = self.new_colour_id.trim().to_string();
                if !id.is_empty() && !self.settings.colour_ids.contains(&id) {
                    self.settings.colour_ids.push(id);
                }
                self.new_colour_id.clear();
            }
        });

        let mut remove = None;
        for (index, id) in self.settings.colour_ids.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("✖").clicked() {
                    remove = Some(index);
                }
                ui.label(id);
                match self.manifest.colour(id) {
                    Some([r, g, b]) => {
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("not set in this project")
                                .size(9.0)
                                .color(egui::Color32::GRAY),
                        );
                    }
                }
            });
        }
        if let Some(index) = remove {
            self.settings.colour_ids.remove(index);
        }

        // Ids the project uses that this installation has never heard of.
        let unknown = self
            .manifest
            .colours_missing_from(&self.settings.colour_ids);
        if !unknown.is_empty() {
            ui.label(
                egui::RichText::new(format!("This project also uses: {}", unknown.join(", ")))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(230, 160, 60)),
            );
            if ui.button("Add them to the vocabulary").clicked() {
                self.settings.colour_ids.extend(unknown);
            }
        }
        ui.separator();

        match AppSettings::path() {
            Some(path) => ui.label(
                egui::RichText::new(format!("Saved to {}", path.display()))
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            ),
            None => ui.label(
                egui::RichText::new("No config directory found; settings last one session.")
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            ),
        };
        if ui.button("Reset to defaults").clicked() {
            self.settings = AppSettings::default();
            self.history_overlay.width = self.settings.history_width;
        }
    }

    /// Capture mode: let a few frames render (so the async kernel rebuild can
    /// land), then ask egui for the framebuffer, write it out and quit.
    fn handle_capture(&mut self, ctx: &egui::Context) {
        let Some(capture) = self.capture.as_mut() else {
            return;
        };

        // Keep frames coming even though there is no user input.
        ctx.request_repaint();

        if let Some(image) = take_screenshot(ctx) {
            match save_screenshot(&capture.path, &image) {
                Ok(()) => tracing::info!("screenshot written to {}", capture.path.display()),
                Err(err) => tracing::error!("could not write screenshot: {err}"),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        capture.rendered += 1;
        if !capture.requested && capture.rendered >= capture.frames {
            capture.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }

    /// Open the feature dialog for the first feature in the model tree.
    ///
    /// Used by `--dialog` to capture the dialog in documentation images.
    pub fn open_feature_dialog_for_first_feature(&mut self) {
        if let Some(feature) = self
            .project
            .parts
            .first()
            .and_then(|part| part.features.first())
        {
            self.show_relations_dialog = Some(feature.id);
        }
    }

    /// Load a small demo part so screenshots and first runs show real geometry.
    pub fn load_demo_part(&mut self) {
        if let Some(part) = self.get_active_part_mut() {
            rcad_features::add_box(part, "Base", [40.0, 30.0, 8.0]);
            let tool = rcad_features::add_cylinder(part, "HoleTool", 5.0, 20.0);
            part.features.push(Feature::new(
                "CutHole",
                FeatureKind::Boolean {
                    tool_feature: tool,
                    op: rcad_geom::BooleanOp::Subtract,
                },
            ));
        }
        self.rebuild_active_part();
    }

    /// `--demo-colours`: tag two visible faces of the demo part with the first
    /// two colour ids the project defines. It is how a screenshot shows the
    /// colour coming from rcad.toml rather than from the component.
    pub fn tag_demo_faces_with_project_colours(&mut self) {
        self.pending_demo_colours = true;
    }

    /// `--select-demo-face`: select the tagged face, for a screenshot that shows
    /// the selection bar and the highlight.
    pub fn select_demo_face_for_screenshot(&mut self) {
        self.pending_demo_select = true;
    }

    fn apply_pending_demo_colours(&mut self) {
        if !self.pending_demo_colours {
            return;
        }
        let ids: Vec<String> = self.manifest.surface_colors.keys().cloned().collect();
        if ids.len() < 2 {
            tracing::warn!("--demo-colours needs two colours in the project rcad.toml");
            self.pending_demo_colours = false;
            return;
        }
        // Keep waiting until a tessellation with stable face names has arrived.
        let Some((top, side)) = demo_face_names(&self.mesh) else {
            return;
        };
        if let Some(part) = self.get_active_part_mut() {
            part.set_face_colour(top.clone(), ids[0].clone());
            part.set_face_colour(side.clone(), ids[1].clone());
        }
        // `--select-demo-face` (screenshots only): show the selection bar and
        // the highlight on the face that was just tagged.
        if self.pending_demo_select {
            if let Some((triangle, label)) = first_triangle_of_face(&self.mesh, &top) {
                self.selection.replace(Selected::Face {
                    reference: top.clone(),
                    label,
                    triangle,
                });
            }
        }
        self.pending_demo_colours = false;
        self.status = Some(format!(
            "Two demo faces coloured from rcad.toml: {}, {}",
            ids[0], ids[1]
        ));
    }

    /// Warn when a colour tag no longer matches any face of the rebuilt model.
    ///
    /// That happens when the face was edited or deleted: its stable name
    /// changed with its geometry. Saying so is better than leaving the user
    /// wondering where the colour went - and much better than painting the
    /// colour on whichever face took its place.
    fn report_unmatched_face_colours(&mut self) {
        let available: Vec<String> = self
            .mesh
            .face_names
            .values()
            .map(|face| face.name.clone())
            .collect();
        if available.is_empty() {
            return;
        }
        let unmatched = match self.get_active_part() {
            Some(part) => part.unmatched_face_colours(&available).len(),
            None => return,
        };
        if unmatched > 0 {
            self.status = Some(format!(
                "{unmatched} colour tag(s) no longer match a face; the geometry changed."
            ));
        }
    }

    /// Tag a face with a logical colour id from the project vocabulary, or
    /// clear its tag. The key is the stable face name, so the tag still points
    /// at the same face after a rebuild, a save and a reload, or a restart.
    pub fn apply_face_colour(
        &mut self,
        face_name: String,
        face_label: String,
        colour_id: Option<String>,
    ) {
        let what = if face_label.is_empty() {
            "Face".to_string()
        } else {
            face_label
        };
        let message = {
            let Some(part) = self.get_active_part_mut() else {
                return;
            };
            match colour_id {
                Some(id) => {
                    part.set_face_colour(face_name.clone(), id.clone());
                    format!("{what} tagged '{id}'.")
                }
                None => {
                    part.clear_face_colour(&face_name);
                    format!("{what} colour cleared.")
                }
            }
        };
        self.status = Some(message);
    }

    /// The face-colour control for the triangle selected in the viewport.
    ///
    /// Only ids the project actually defines are offered: the vocabulary is
    /// installation-wide, but an id without an RGB value has nothing to show.
    fn draw_face_colour_control(&mut self, ui: &mut egui::Ui) {
        let Some(triangle) = self.selection.primary_face_triangle() else {
            return;
        };
        let Some(face_name) = self.mesh.face_name(triangle).map(str::to_string) else {
            return;
        };
        let face_label = self.mesh.face_label(triangle).unwrap_or("Face").to_string();

        ui.separator();
        ui.label(&face_label)
            .on_hover_text(format!("Stable face id:\n{face_name}"));
        let current = self
            .get_active_part()
            .and_then(|part| part.face_colour_id(&face_name))
            .map(str::to_string);
        let ids: Vec<String> = self.manifest.surface_colors.keys().cloned().collect();
        let mut choice = current.clone();
        egui::ComboBox::from_id_salt("face_colour_id")
            .selected_text(current.clone().unwrap_or_else(|| "not set".to_string()))
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "not set").clicked() {
                    choice = None;
                }
                for id in &ids {
                    let selected = current.as_deref() == Some(id.as_str());
                    if ui.selectable_label(selected, id).clicked() {
                        choice = Some(id.clone());
                    }
                }
            });
        if choice != current {
            self.apply_face_colour(face_name, face_label, choice);
        }
    }

    /// The selection strip: what is selected, in the order it was selected.
    ///
    /// The order is not cosmetic: a command reads the first item as its
    /// placement, the rest as orientation. Showing it is how the user checks it
    /// before starting a sketch or a fillet.
    fn draw_selection_bar(&mut self, ctx: &egui::Context) {
        if self.selection.is_empty() {
            return;
        }
        // The menu choices are built before the panel, so the closures only
        // read them. The axis the vertical and horizontal modes need comes from
        // the selection: without it those two are offered disabled.
        let axis = self.selected_axis();
        let mut edge_sets: Vec<(&'static str, EdgeSet)> = Vec::new();
        let edge_references = self.selection.edge_references();
        if !edge_references.is_empty() {
            edge_sets.push((
                "Selected edges",
                EdgeSet::References(edge_references.iter().map(|r| r.to_string()).collect()),
            ));
        }
        edge_sets.push(("Outer edges (up to 180)", EdgeSet::Outer));
        edge_sets.push(("Inner edges (over 180)", EdgeSet::Inner));
        edge_sets.push(("Every edge", EdgeSet::All));
        if let Some(axis) = axis {
            edge_sets.push(("Vertical edges", EdgeSet::Vertical { axis }));
            edge_sets.push(("Horizontal edges", EdgeSet::Horizontal { axis }));
        }
        let mut remove_index: Option<usize> = None;
        let mut clear = false;
        let mut query: Option<SelectionQuery> = None;
        let mut pending_feature: Option<(EdgeFeature, EdgeSet)> = None;
        egui::TopBottomPanel::bottom("selection_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let seed = self
                    .selection
                    .primary()
                    .map(|item| item.label().to_string())
                    .unwrap_or_default();
                ui.label(egui::RichText::new(format!("Selection ({})", self.selection.len())))
                    .on_hover_text(format!(
                        "{}\nSeed (the last picked): {seed}",
                        self.selection.summary()
                    ));
                for (index, item) in self.selection.items().iter().enumerate() {
                    let text = format!("{} {}  ✕", item.kind(), item.label());
                    let response = match item.reference() {
                        Some(reference) => ui.small_button(text).on_hover_text(reference),
                        None => ui.small_button(text),
                    };
                    let clicked = response.clicked();
                    // Creo puts the smart-selection queries on the item itself.
                    let mut chosen: Option<SelectionQuery> = None;
                    response.context_menu(|ui| {
                        ui.label("Select more from the seed edge:");
                        if ui.button("Boundary").clicked() {
                            chosen = Some(SelectionQuery::Boundary);
                            ui.close();
                        }
                        if ui.button("Tangent chain").clicked() {
                            chosen = Some(SelectionQuery::TangentChain);
                            ui.close();
                        }
                        if ui.button("Surface (all its edges)").clicked() {
                            chosen = Some(SelectionQuery::SurfaceRing);
                            ui.close();
                        }
                    });
                    if chosen.is_some() {
                        query = chosen;
                    }
                    if clicked {
                        remove_index = Some(index);
                    }
                }
                // Round and break read the same edge sets, the way the Creo
                // dashboard does.
                for (label, kind) in [
                    ("Round", EdgeFeature::Fillet),
                    ("Break", EdgeFeature::Chamfer),
                ] {
                    ui.menu_button(label, |ui| {
                        for (name, set) in &edge_sets {
                            if ui.button(*name).clicked() {
                                pending_feature = Some((kind, (*set).clone()));
                                ui.close();
                            }
                        }
                        if axis.is_none() {
                            for name in ["Vertical edges", "Horizontal edges"] {
                                ui.add_enabled(false, egui::Button::new(name)).on_hover_text(
                                    "Select a datum axis as the vertical reference first",
                                );
                            }
                        }
                    });
                }
                if ui.small_button("Clear").clicked() {
                    clear = true;
                }
            });
        });
        if let Some(index) = remove_index {
            let item = self.selection.items()[index].clone();
            self.selection.remove(&item);
        }
        if clear {
            self.selection.clear();
        }
        if let Some(query) = query {
            self.apply_selection_query(query);
        }
        if let Some((kind, edges)) = pending_feature {
            self.add_edge_feature(kind, 1.0, edges);
        }
    }

    /// The first selected datum axis: the vertical reference the vertical and
    /// horizontal edge sets need.
    fn selected_axis(&self) -> Option<Uuid> {
        let part = self.get_active_part()?;
        self.selection.items().iter().find_map(|item| {
            let Selected::Datum { feature, .. } = item else {
                return None;
            };
            let datum = part.features.iter().find(|candidate| candidate.id == *feature)?;
            match &datum.kind {
                FeatureKind::Construction {
                    kind: ConstructionKind::Axis,
                    ..
                } => Some(datum.id),
                _ => None,
            }
        })
    }

    /// Add a round or a break driven by an edge set, at the default size.
    pub fn add_edge_feature(&mut self, kind: EdgeFeature, size: f64, edges: EdgeSet) {
        self.undo_stack.push(self.project.clone());
        let Some(part) = self.get_active_part_mut() else {
            return;
        };
        let name = rcad_features::unique_feature_name(part, kind.name());
        match kind {
            EdgeFeature::Fillet => rcad_features::add_fillet(part, &name, size, edges.clone()),
            EdgeFeature::Chamfer => rcad_features::add_chamfer(part, &name, size, edges.clone()),
        };
        self.rebuild_active_part();
        self.status = Some(format!("{name} added."));
    }

    /// Expand the selection with a Creo-style query on the seed edge.
    pub fn apply_selection_query(&mut self, query: SelectionQuery) {
        let seed = self.selection.items().iter().rev().find_map(|item| match item {
            Selected::Edge { reference, .. } => Some(reference.clone()),
            _ => None,
        });
        let Some(seed) = seed else {
            self.status = Some("Pick an edge first, then use a query on it.".to_string());
            return;
        };
        let Some(index) = self.mesh.edges.iter().position(|edge| edge.name == seed) else {
            return;
        };

        let indices: Vec<usize> = match query {
            SelectionQuery::TangentChain => rcad_geom::query::tangent_chain(
                &self.mesh,
                index,
                rcad_geom::query::TANGENT_TOLERANCE,
            ),
            SelectionQuery::SurfaceRing => rcad_geom::query::surface_ring(&self.mesh, index),
            SelectionQuery::Boundary => {
                // The boundary loop of every surface the seed edge bounds.
                let faces = self.mesh.edges[index].faces.clone();
                faces
                    .iter()
                    .filter_map(|face| {
                        rcad_geom::query::boundary_loops(&self.mesh, face)
                            .into_iter()
                            .next()
                    })
                    .flatten()
                    .collect()
            }
        };

        let before = self.selection.len();
        for edge_index in indices {
            let (reference, label) = {
                let edge = &self.mesh.edges[edge_index];
                (edge.name.clone(), edge.label.clone())
            };
            self.selection.extend(Selected::Edge { reference, label });
        }
        let added = self.selection.len() - before;
        let name = match query {
            SelectionQuery::Boundary => "boundary",
            SelectionQuery::TangentChain => "tangent chain",
            SelectionQuery::SurfaceRing => "surface",
        };
        self.status = Some(format!(
            "{name}: {added} edge(s) added, {} selected.",
            self.selection.len()
        ));
    }

    fn draw_shortcuts_dialog(&mut self, ui: &mut egui::Ui) {
        let shortcuts = shortcuts_list();
        egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
            ui.set_min_width(300.0);
            for (key, desc) in shortcuts {
                if desc.is_empty() {
                    // Section header
                    ui.separator();
                    ui.strong(key);
                } else {
                    ui.horizontal(|ui| {
                        ui.monospace(key);
                        ui.separator();
                        ui.label(desc);
                    });
                }
            }
        });
        ui.separator();
        if ui.button("Close").clicked() {
            self.show_shortcuts_dialog = false;
        }
    }
    
    /// Feature dialog: edit the feature parameters, and its part relations.
    ///
    /// Opened from the model tree by clicking a feature type icon. This is the
    /// Creo "Edit Definition" surface: a Parameters tab where parameters can be
    /// edited, added and removed, and a Relations tab for part-level equations.
    fn draw_feature_dialog(&mut self, ui: &mut egui::Ui, feature_id: uuid::Uuid) {
        let mut status: Option<String> = None;
        let mut close = false;
        let mut tab: usize = self.feature_dialog_tab;

        {
            let Some(part) = self.project.parts.first_mut() else {
                ui.label("No active part");
                return;
            };
            let Some(index) = part.features.iter().position(|f| f.id == feature_id) else {
                ui.label("Feature not found");
                if ui.button("Close").clicked() {
                    close = true;
                }
                // Fall through to the shared footer below.
                if close {
                    self.feature_dialog_tab = tab;
                    self.show_relations_dialog = None;
                }
                return;
            };

            let kind_name = feature_kind_label(&part.features[index].kind);
            ui.heading(format!("{} — {kind_name}", part.features[index].name));
            ui.separator();

            ui.horizontal(|ui| {
                ui.selectable_value(&mut tab, 0, "Parameters");
                ui.selectable_value(&mut tab, 1, "Relations");
            });
            ui.separator();

            match tab {
                0 => draw_parameters_tab(ui, &mut part.features[index].parameters, &mut self.new_parameter),
                _ => {
                    if draw_relations_tab(ui, &mut part.relations) {
                        let errors = rcad_param::evaluate_relations(part);
                        status = Some(if errors.is_empty() {
                            format!(
                                "Regenerated {} relation(s).",
                                part.relations.len()
                            )
                        } else {
                            format!("Relation errors: {}", errors.join("; "))
                        });
                    }
                }
            }

            ui.separator();
            if ui.button("Close").clicked() {
                close = true;
            }
        }

        self.feature_dialog_tab = tab;
        if let Some(message) = status {
            self.last_error = Some(message);
        }
        if close {
            self.show_relations_dialog = None;
        }
    }

    fn draw_relations_dialog(&mut self, ui: &mut egui::Ui, feature_id: uuid::Uuid) {
        self.draw_feature_dialog(ui, feature_id);
    }

    
    fn get_active_part_mut(&mut self) -> Option<&mut rcad_doc::Part> {
        // For now, just use the first part
        self.project.parts.get_mut(0)
    }

    fn get_active_part(&self) -> Option<&rcad_doc::Part> {
        self.project.parts.get(0)
    }

    /// Request a rebuild of the active part.
    ///
    /// The rebuild runs on the kernel worker thread, which owns the geometry
    /// backend and the incremental `ShapeCache`. Marking the part dirty keeps
    /// this cheap enough to call from every mutation site.
    fn rebuild_active_part(&mut self) {
        self.needs_rebuild = true;
    }

    
    // ===== Public methods for keyboard shortcuts =====
    
    // The camera stores an orientation, so a standard view is just the
    // direction to look from. Straight up and straight down are exact, which a
    // yaw/pitch camera could not represent.
    // Z is up, so Front is the XZ plane (normal Y), Right is the YZ plane
    // (normal X) and Top looks straight down the Z axis.
    pub fn set_view_front(&mut self) {
        self.camera.look_from(glam::Vec3::Y);
    }

    pub fn set_view_back(&mut self) {
        self.camera.look_from(-glam::Vec3::Y);
    }

    pub fn set_view_left(&mut self) {
        self.camera.look_from(-glam::Vec3::X);
    }

    pub fn set_view_right(&mut self) {
        self.camera.look_from(glam::Vec3::X);
    }

    pub fn set_view_top(&mut self) {
        self.camera.look_from(glam::Vec3::Z);
    }

    pub fn set_view_bottom(&mut self) {
        self.camera.look_from(-glam::Vec3::Z);
    }

    pub fn set_view_iso(&mut self) {
        let (yaw, pitch) = (0.7_f32, 0.5_f32);
        self.camera.look_from(glam::Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.sin(),
            pitch.cos() * yaw.sin(),
        ));
    }
    
    pub fn reset_view(&mut self) {
        self.camera = OrbitCamera::default();
    }

    /// Frame the model in the viewport (View > Zoom Fit, or the toolbar button).
    pub fn zoom_fit(&mut self) {
        match self.mesh.bounds() {
            Some((min, max)) => self.camera.fit_to_bounds(min, max),
            None => self.reset_view(),
        }
    }
    
    pub fn new_document(&mut self) {
        // A new document keeps the project: the project owns the folder,
        // the tolerances and the colours, and outlives any single part.
        let project_root = self.project_root.take();
        let manifest = self.manifest.clone();
        let settings = self.settings.clone();
        *self = Self::default();
        self.project_root = project_root;
        self.manifest = manifest;
        self.settings = settings;
        self.saved_settings = self.settings.clone();
        let _ = self.create_default_datum_geometry();
    }
    
    pub fn open_document(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            match Project::load(&path) {
                Ok(project) => {
                    self.project = project;
                    self.current_path = Some(path);
                    self.last_error = None;
                    self.restore_view();
                }
                Err(e) => {
                    self.last_error = Some(format!("Failed to load project: {e}"));
                }
            }
        }
    }
    
    /// Store the current camera in the project, so saving keeps the view.
    fn capture_view(&mut self) {
        let orientation = self.camera.orientation;
        self.project.view = Some(rcad_doc::SavedView {
            orientation: [orientation.x, orientation.y, orientation.z, orientation.w],
            target: [
                self.camera.target.x,
                self.camera.target.y,
                self.camera.target.z,
            ],
            distance: self.camera.distance,
            orthographic: self.camera.projection == Projection::Orthographic,
        });
    }

    /// Restore the view saved with the document, if there is one.
    fn restore_view(&mut self) {
        let Some(view) = self.project.view else {
            return;
        };
        self.camera.orientation = glam::Quat::from_xyzw(
            view.orientation[0],
            view.orientation[1],
            view.orientation[2],
            view.orientation[3],
        )
        .normalize();
        self.camera.target = glam::Vec3::from(view.target);
        self.camera.distance = view.distance.max(1.0);
        self.camera.projection = if view.orthographic {
            Projection::Orthographic
        } else {
            Projection::Perspective
        };
        // The saved framing was deliberate, so do not auto-fit over it.
        self.needs_fit = false;
    }

    pub fn save_document(&mut self) {
        self.capture_view();
        if let Some(path) = &self.current_path {
            if let Err(e) = self.project.save(path) {
                self.last_error = Some(format!("Failed to save project: {e}"));
            }
        } else {
            self.save_document_as();
        }
    }
    
    pub fn save_document_as(&mut self) {
        self.capture_view();
        if let Some(path) = rfd::FileDialog::new().save_file() {
            if let Err(e) = self.project.save(&path) {
                self.last_error = Some(format!("Failed to save project: {e}"));
            } else {
                self.current_path = Some(path);
                self.last_error = None;
            }
        }
    }

    /// Open a document by path. The menu uses the file dialog; this is the
    /// headless path used by `--open` and by tests.
    pub fn open_document_at(&mut self, path: PathBuf) -> Result<(), String> {
        let project =
            Project::load(&path).map_err(|e| format!("Failed to load {}: {e}", path.display()))?;
        self.project = project;
        self.current_path = Some(path);
        self.last_error = None;
        self.restore_view();
        self.needs_rebuild = true;
        self.mesh_ready = false;
        Ok(())
    }

    /// Save the document to a path. The menu uses the file dialog; this is the
    /// headless path used by `--save` and by tests.
    pub fn save_document_at(&mut self, path: PathBuf) -> Result<(), String> {
        self.capture_view();
        self.project
            .save(&path)
            .map_err(|e| format!("Failed to save {}: {e}", path.display()))?;
        self.current_path = Some(path);
        self.last_error = None;
        Ok(())
    }

    /// `--save`: write the document once the model is built, so a headless run
    /// can save a document that already carries its face colours.
    pub fn request_save(&mut self, path: PathBuf) {
        self.pending_save = Some(path);
    }

    fn flush_pending_save(&mut self) {
        let Some(path) = self.pending_save.clone() else {
            return;
        };
        // Wait for the first rebuild, and for --demo-colours to be applied, so
        // the file on disk is the finished document.
        if self.pending_demo_colours || !self.mesh_ready {
            return;
        }
        self.pending_save = None;
        match self.save_document_at(path) {
            Ok(()) => self.status = Some("Document saved.".to_string()),
            Err(err) => self.last_error = Some(err),
        }
    }
    
    pub fn export_stl(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("stl", &["stl"]).save_file() {
            if let Err(e) = self.export_stl_internal(&path) {
                self.last_error = Some(format!("Failed to export STL: {e}"));
            }
        }
    }
    
    pub fn export_step(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("step", &["step"]).save_file() {
            if let Err(e) = self.export_step_internal(&path) {
                self.last_error = Some(format!("Failed to export STEP: {e}"));
            }
        }
    }
    
    pub fn undo(&mut self) {
        if self.undo_stack.undo(&mut self.project) {
            self.rebuild_active_part();
        }
    }
    
    pub fn redo(&mut self) {
        if self.undo_stack.redo(&mut self.project) {
            self.rebuild_active_part();
        }
    }
    
    pub fn delete_selected(&mut self) {
        self.last_error = Some("Delete not yet implemented".to_string());
    }
    
    /// A new sketch, placed on the first planar reference in the selection.
    ///
    /// The plane comes from the selection, and the reference is kept: this is
    /// the first consumer of the shared selection, and the same shape of code a
    /// feature dashboard will use.
    pub fn new_sketch(&mut self) {
        self.undo_stack.push(self.project.clone());
        let mut sketch = rcad_sketch::Sketch::new("Sketch1");

        match self.selection_plane() {
            Some((origin, normal, x_direction)) => {
                sketch.set_sketch_plane(origin, normal, x_direction);
                self.status = Some("Sketch placed on the selected reference.".to_string());
            }
            None => {
                // No plane in the selection: ask for one and wait for it.
                sketch.in_placement_mode = true;
                self.status = Some(
                    "Select a planar face or datum plane, or start the sketch again on the default plane."
                        .to_string(),
                );
            }
        }

        self.sketch = Some(sketch);
    }

    /// The placement plane of the first planar reference in the selection.
    fn selection_plane(&self) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
        let part = self.get_active_part()?;
        let item = self.selection.items().iter().find(|item| {
            matches!(
                item,
                Selected::Datum { .. } | Selected::Face { .. } | Selected::Feature { .. }
            )
        })?;
        let feature = match item {
            Selected::Datum { feature, .. } | Selected::Feature { feature, .. } => {
                part.features.iter().find(|f| f.id == *feature)?
            }
            // A planar solid face has no stored datum yet; the sketch tools that
            // read the mesh for that come next.
            Selected::Face { .. } => return None,
            Selected::Edge { .. } => return None,
        };
        let FeatureKind::Construction {
            kind: ConstructionKind::Plane,
            origin,
            direction,
        } = &feature.kind
        else {
            return None;
        };
        let normal = normalised(*direction)?;
        // Any direction perpendicular to the normal will do as the sketch X.
        let reference = if normal[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let x_direction = normalised(cross(normal, reference))?;
        Some((*origin, normal, x_direction))
    }

    /// The feature a tree row refers to: a datum when it is construction
    /// geometry, a feature otherwise.
    fn tree_item(&self, feature: uuid::Uuid) -> Option<Selected> {
        let part = self.get_active_part()?;
        let feature = part.features.iter().find(|f| f.id == feature)?;
        Some(match feature.kind {
            FeatureKind::Construction { .. } => Selected::Datum {
                feature: feature.id,
                label: feature.name.clone(),
            },
            _ => Selected::Feature {
                feature: feature.id,
                label: feature.name.clone(),
            },
        })
    }
    
    pub fn solve_sketch(&mut self) {
        if let Some(sketch) = &mut self.sketch {
            if let Err(e) = sketch.solve() {
                self.last_error = Some(format!("Constraints: {e}"));
            }
        }
    }
    
    pub fn add_sketch_point(&mut self) {
        if let Some(sketch) = &mut self.sketch {
            self.undo_stack.push(self.project.clone());
            sketch.add_point(0.0, 0.0);
        }
    }
    
    pub fn add_sketch_line(&mut self) {
        if let Some(sketch) = &mut self.sketch {
            if sketch.points.len() >= 2 {
                self.undo_stack.push(self.project.clone());
                let n = sketch.points.len();
                let (a, b) = (sketch.points[n - 2].id, sketch.points[n - 1].id);
                sketch.add_line(a, b);
            }
        }
    }
    
    pub fn add_sketch_rectangle(&mut self) {
        if let Some(sketch) = &mut self.sketch {
            self.undo_stack.push(self.project.clone());
            sketch.add_rectangle(-25.0, -25.0, 25.0, 25.0);
        }
    }
    
    pub fn extrude_sketch(&mut self) {
        if let Some(sketch) = &self.sketch {
            if let Some(profile) = sketch.to_profile() {
                if let Err(e) = self.extrude_sketch_internal(profile, 10.0) {
                    self.last_error = Some(format!("Extrude failed: {e}"));
                }
            }
        }
    }
    
    pub fn revolve_sketch(&mut self) {
        if let Some(sketch) = &self.sketch {
            if let Some(profile) = sketch.to_profile() {
                if let Err(e) = self.revolve_sketch_internal(profile, 360.0) {
                    self.last_error = Some(format!("Revolve failed: {e}"));
                }
            }
        }
    }
    
    pub fn add_box_feature(&mut self) {
        if let Err(e) = self.add_box_feature_internal() {
            self.last_error = Some(format!("Add box failed: {e}"));
        }
    }
    
    pub fn suppress_feature(&mut self) {
        self.last_error = Some("Feature suppress not yet implemented".to_string());
    }
    
    pub fn select_all(&mut self) {
        // TODO: Select all
    }
    
    pub fn deselect(&mut self) {
        self.selection.clear();
    }
    
    pub fn show_shortcuts_help(&mut self) {
        self.show_shortcuts_dialog = true;
    }
    
    // Internal methods (renamed from original private methods)
    fn extrude_sketch_internal(&mut self, profile: rcad_geom::Profile2D, height: f64) -> Result<(), Box<dyn std::error::Error>> {
        if profile.outer.len() < 3 {
            return Err("extrude needs a closed profile with at least 3 points".into());
        }
        self.undo_stack.push(self.project.clone());
        if let Some(part) = self.get_active_part_mut() {
            // Extrude the profile that was passed in. The previous version
            // discarded it and appended a hard-coded 10 x 10 x height box, so
            // every sketch produced the same prism.
            let name = rcad_features::unique_feature_name(part, "Extrude");
            rcad_features::add_extrude(part, &name, profile, height);
            self.rebuild_active_part();
        }
        Ok(())
    }
    
    fn revolve_sketch_internal(&mut self, profile: rcad_geom::Profile2D, angle_deg: f64) -> Result<(), Box<dyn std::error::Error>> {
        // Save state before modifying
        self.undo_stack.push(self.project.clone());
        if let Some(part) = self.get_active_part_mut() {
            let name = rcad_features::unique_feature_name(part, "Revolve");
            rcad_features::add_revolve(part, &name, profile, angle_deg);
            self.rebuild_active_part();
        }
        Ok(())
    }
    
    fn add_box_feature_internal(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Save state before modifying
        self.undo_stack.push(self.project.clone());
        if let Some(part) = self.get_active_part_mut() {
            let name = rcad_features::unique_feature_name(part, "Box");
            rcad_features::add_box(part, &name, [10.0, 10.0, 10.0]);
            self.rebuild_active_part();
        }
        Ok(())
    }
    
    // ===== Toolbar Action Handlers =====
    
    fn handle_toolbar_action(&mut self, action: ToolbarAction) {
        match action {
            ToolbarAction::NewDocument => self.new_document(),
            ToolbarAction::OpenDocument => self.open_document(),
            ToolbarAction::SaveDocument => self.save_document(),
            ToolbarAction::SaveDocumentAs => self.save_document_as(),
            ToolbarAction::ExportStl => self.export_stl(),
            ToolbarAction::ExportStep => self.export_step(),
            ToolbarAction::Undo => self.undo(),
            ToolbarAction::Redo => self.redo(),
            
            ToolbarAction::ViewFront => self.set_view_front(),
            ToolbarAction::ViewBack => self.set_view_back(),
            ToolbarAction::ViewLeft => self.set_view_left(),
            ToolbarAction::ViewRight => self.set_view_right(),
            ToolbarAction::ViewTop => self.set_view_top(),
            ToolbarAction::ViewBottom => self.set_view_bottom(),
            ToolbarAction::ViewIso => self.set_view_iso(),
            ToolbarAction::ViewReset => self.reset_view(),
            ToolbarAction::ViewPerspective => {
                self.camera.projection = Projection::Perspective;
            }
            ToolbarAction::ViewOrthographic => {
                self.camera.projection = Projection::Orthographic;
            }
            ToolbarAction::ViewShaded => {
                // TODO: Set shaded mode
            }
            ToolbarAction::ViewWireframe => {
                // TODO: Set wireframe mode
            }
            ToolbarAction::ViewZoomFit => self.zoom_fit(),
            ToolbarAction::ViewPrevious => {
                // TODO: Previous view
            }
            
            ToolbarAction::ModelBox => self.add_box_feature(),
            ToolbarAction::ModelExtrude => self.extrude_sketch(),
            ToolbarAction::ModelRevolve => self.revolve_sketch(),
            ToolbarAction::ModelSweep => {
                // TODO: Sweep
            }
            ToolbarAction::ModelLoft => {
                // TODO: Loft
            }
            ToolbarAction::ModelBoundary => {
                // TODO: Boundary
            }
            ToolbarAction::ModelFillet => {
                // TODO: Fillet
            }
            ToolbarAction::ModelChamfer => {
                // TODO: Chamfer
            }
            ToolbarAction::ModelShell => {
                // TODO: Shell
            }
            ToolbarAction::ModelDraft => {
                // TODO: Draft
            }
            ToolbarAction::ModelHole => {
                // TODO: Hole
            }
            ToolbarAction::ModelPattern => {
                // TODO: Pattern
            }
            ToolbarAction::ModelMirror => {
                // TODO: Mirror
            }
            ToolbarAction::ModelBoolean => {
                // TODO: Boolean
            }
            ToolbarAction::ModelCosmeticThread => {
                // TODO: Cosmetic thread
            }
            
            ToolbarAction::SketchNew => self.new_sketch(),
            ToolbarAction::SketchLine => self.add_sketch_line(),
            ToolbarAction::SketchRectangle => self.add_sketch_rectangle(),
            ToolbarAction::SketchCircle => {
                // TODO: Add circle
            }
            ToolbarAction::SketchArc => {
                // TODO: Add arc
            }
            ToolbarAction::SketchSpline => {
                // TODO: Add spline
            }
            ToolbarAction::SketchPoint => self.add_sketch_point(),
            ToolbarAction::SketchConstraintHorizontal => {
                // TODO: Add horizontal constraint
            }
            ToolbarAction::SketchConstraintVertical => {
                // TODO: Add vertical constraint
            }
            ToolbarAction::SketchConstraintParallel => {
                // TODO: Add parallel constraint
            }
            ToolbarAction::SketchConstraintPerpendicular => {
                // TODO: Add perpendicular constraint
            }
            ToolbarAction::SketchConstraintTangent => {
                // TODO: Add tangent constraint
            }
            ToolbarAction::SketchConstraintEqual => {
                // TODO: Add equal constraint
            }
            ToolbarAction::SketchConstraintSymmetric => {
                // TODO: Add symmetric constraint
            }
            ToolbarAction::SketchConstraintConcentric => {
                // TODO: Add concentric constraint
            }
            ToolbarAction::SketchConstraintMidpoint => {
                // TODO: Add midpoint constraint
            }
            ToolbarAction::SketchConstraintCoincident => {
                // TODO: Add coincident constraint
            }
            ToolbarAction::SketchDimension => {
                // TODO: Add dimension
            }
            ToolbarAction::SketchTrim => {
                // TODO: Trim
            }
            ToolbarAction::SketchExtend => {
                // TODO: Extend
            }
            ToolbarAction::SketchOffset => {
                // TODO: Offset
            }
            ToolbarAction::SketchMirror => {
                // TODO: Mirror
            }
            ToolbarAction::SketchPattern => {
                // TODO: Pattern
            }
            ToolbarAction::SketchProjectReferences => {
                // TODO: Project references
            }
            
            ToolbarAction::AssemblyInsertComponent => {
                // Placement mode: the floating bar shows the assembly commands
                // only while this is on.
                self.placing_component = !self.placing_component;
                self.status = Some(if self.placing_component {
                    "Placing a component: assembly commands are on the floating bar."
                        .to_string()
                } else {
                    "Component placement cancelled.".to_string()
                });
            }
            ToolbarAction::AssemblyMateCoincident => {
                // TODO: Coincident mate
            }
            ToolbarAction::AssemblyMateConcentric => {
                // TODO: Concentric mate
            }
            ToolbarAction::AssemblyMateDistance => {
                // TODO: Distance mate
            }
            ToolbarAction::AssemblyMateAngle => {
                // TODO: Angle mate
            }
            ToolbarAction::AssemblyMateParallel => {
                // TODO: Parallel mate
            }
            ToolbarAction::AssemblyMatePerpendicular => {
                // TODO: Perpendicular mate
            }
            ToolbarAction::AssemblyMateTangent => {
                // TODO: Tangent mate
            }
            ToolbarAction::AssemblyMateSymmetric => {
                // TODO: Symmetric mate
            }
            ToolbarAction::AssemblyExplodedView => {
                // TODO: Exploded view
            }
            ToolbarAction::AssemblySubAssembly => {
                // TODO: Sub-assembly
            }
            
            ToolbarAction::AnalysisMeasure => self.measure_model(),
            ToolbarAction::AnalysisMassProperties => {
                // TODO: Mass properties
            }
            ToolbarAction::AnalysisCurvature => {
                // TODO: Curvature analysis
            }
            ToolbarAction::AnalysisDraft => {
                // TODO: Draft analysis
            }
            ToolbarAction::AnalysisThickness => {
                // TODO: Thickness analysis
            }
            ToolbarAction::AnalysisInterference => {
                // TODO: Interference check
            }
            ToolbarAction::AnalysisClearance => {
                // TODO: Clearance check
            }
            
            ToolbarAction::ToolsParameters => {
                // TODO: Parameters dialog
            }
            ToolbarAction::ToolsRelations => {
                // TODO: Relations dialog
            }
            ToolbarAction::ToolsFamilyTable => {
                // TODO: Family table
            }
            ToolbarAction::ToolsDesignExploration => {
                // TODO: Design exploration
            }
            ToolbarAction::ToolsOptions => self.show_settings = true,
            ToolbarAction::ToolsCustomize => {
                // TODO: Customize dialog
            }
            
            ToolbarAction::HelpAbout => {
                // TODO: About dialog
            }
            ToolbarAction::HelpShortcuts => self.show_shortcuts_help(),
        }
    }
    
    // ===== History Overlay Action Handlers =====
    
    /// Handle a floating quick-bar action.
    fn handle_floating_action(&mut self, action: FloatingAction) {
        match action {
            FloatingAction::ToggleDatums(kind) => self.toggle_datums(kind),
            FloatingAction::Toolbar(action) => self.handle_toolbar_action(action),
            FloatingAction::CancelPlacement => {
                self.placing_component = false;
                self.status = Some("Component placement cancelled.".to_string());
            }
        }
    }

    /// Show or hide the datum features of one kind, or every kind.
    pub fn set_datums_visible(&mut self, kind: Option<DatumKind>, visible: bool) {
        if let Some(part) = self.get_active_part_mut() {
            for feature in &mut part.features {
                if let FeatureKind::Construction {
                    kind: construction,
                    ..
                } = &feature.kind
                {
                    let selected = match kind {
                        Some(wanted) => wanted.matches_construction(*construction),
                        None => true,
                    };
                    if selected {
                        feature.visible = visible;
                    }
                }
            }
        }
        self.needs_rebuild = true;
    }

    /// Toggle a datum family: hide it when anything of that family is showing.
    pub fn toggle_datums(&mut self, kind: Option<DatumKind>) {
        let showing = match kind {
            Some(kind) => self.datum_kind_shown(kind),
            None => DatumKind::ALL.iter().any(|k| self.datum_kind_shown(*k)),
        };
        self.set_datums_visible(kind, !showing);
    }

    fn datum_kind_shown(&self, kind: DatumKind) -> bool {
        let Some(part) = self.get_active_part() else {
            return false;
        };
        part.features.iter().any(|feature| {
            feature.visible
                && matches!(&feature.kind, FeatureKind::Construction { kind: c, .. } if kind.matches_construction(*c))
        })
    }

    /// Measure the model and report it in the status bar.
    pub fn measure_model(&mut self) {
        self.status = match self.mesh.bounds() {
            Some((min, max)) => {
                let size = max - min;
                Some(format!(
                    "Measure: bounding box {:.2} x {:.2} x {:.2}, {} triangles",
                    size.x,
                    size.y,
                    size.z,
                    self.mesh.triangle_count()
                ))
            }
            None => Some("Measure: the part has no geometry yet".to_string()),
        };
    }

    fn handle_history_action(&mut self, action: HistoryOverlayAction) {
        use crate::ui::HistoryOverlayAction;
        
        match action {
            HistoryOverlayAction::SelectFeature { feature, toggle } => {
                if let Some(item) = self.tree_item(feature) {
                    if toggle {
                        self.selection.toggle(item);
                    } else {
                        self.selection.replace(item);
                    }
                }
            }
            HistoryOverlayAction::OpenContextMenu(feature_id, pos) => {
                self.history_overlay.open_context_menu(feature_id, pos);
            }
            HistoryOverlayAction::EditDefinition(feature_id) => {
                // TODO: Open Edit Definition dashboard
                self.last_error = Some(format!("Edit Definition for feature {:?} - TODO", feature_id));
            }
            HistoryOverlayAction::SuppressFeature(feature_id) => {
                if let Some(part) = self.get_active_part_mut() {
                    if let Some(idx) = part.features.iter().position(|f| f.id == feature_id) {
                        if !part.features[idx].suppressed {
                            part.features[idx].suppressed = true;
                            part.suppressed.push(feature_id);
                            self.rebuild_active_part();
                        }
                    }
                }
            }
            HistoryOverlayAction::ResumeFeature(feature_id) => {
                if let Some(part) = self.get_active_part_mut() {
                    if let Some(idx) = part.features.iter().position(|f| f.id == feature_id) {
                        if part.features[idx].suppressed {
                            part.features[idx].suppressed = false;
                            part.suppressed.retain(|id| *id != feature_id);
                            self.rebuild_active_part();
                        }
                    }
                }
            }
            HistoryOverlayAction::ToggleSuppression(feature_id) => {
                if let Some(part) = self.get_active_part_mut() {
                    if let Some(idx) = part.features.iter().position(|f| f.id == feature_id) {
                        let currently_suppressed = part.features[idx].suppressed;
                        part.features[idx].suppressed = !currently_suppressed;
                        if currently_suppressed {
                            part.suppressed.retain(|id| *id != feature_id);
                        } else {
                            part.suppressed.push(feature_id);
                        }
                        self.rebuild_active_part();
                    }
                }
            }
            HistoryOverlayAction::ToggleVisibility(feature_id) => {
                if let Some(part) = self.get_active_part_mut() {
                    if let Some(idx) = part.features.iter().position(|f| f.id == feature_id) {
                        part.features[idx].visible = !part.features[idx].visible;
                        self.rebuild_active_part();
                    }
                }
            }
            HistoryOverlayAction::OpenRelationsDialog(feature_id) => {
                // Open the relations/parameters dialog
                self.show_relations_dialog = Some(feature_id);
            }
            HistoryOverlayAction::SetInsertHere(feature_id) => {
                if let Some(part) = self.get_active_part_mut() {
                    if let Some(idx) = part.features.iter().position(|f| f.id == feature_id) {
                        part.insert_here_index = Some(idx);
                        self.history_overlay.set_insert_here(Some(idx));
                    }
                }
            }
            HistoryOverlayAction::SetRollback(feature_id) => {
                if let Some(part) = self.get_active_part_mut() {
                    if let Some(idx) = part.features.iter().position(|f| f.id == feature_id) {
                        part.rollback_index = Some(idx);
                        self.history_overlay.set_rollback(Some(idx));
                    }
                }
            }
            HistoryOverlayAction::RenameFeature(feature_id) => {
                // TODO: Inline rename
                self.last_error = Some(format!("Rename feature {:?} - TODO", feature_id));
            }
            HistoryOverlayAction::DeleteFeature(feature_id) => {
                if let Some(part) = self.get_active_part_mut() {
                    if let Some(idx) = part.features.iter().position(|f| f.id == feature_id) {
                        // Check for children
                        let has_children = part.features.iter().any(|f| f.parent_features.contains(&feature_id));
                        if has_children {
                            self.last_error = Some(format!("Feature has children. Delete anyway? (Not implemented)"));
                        } else {
                            part.features.remove(idx);
                            self.rebuild_active_part();
                        }
                    }
                }
            }
            HistoryOverlayAction::CopyFeature(feature_id) => {
                // TODO: Copy feature
                self.last_error = Some(format!("Copy feature {:?} - TODO", feature_id));
            }
            HistoryOverlayAction::PasteFeature(feature_id) => {
                // TODO: Paste feature
                self.last_error = Some(format!("Paste feature {:?} - TODO", feature_id));
            }
            HistoryOverlayAction::PasteSpecialFeature(_feature_id) => {
                // TODO: Paste special
            }
            HistoryOverlayAction::ShowParents(_feature_id) => {
                self.history_overlay.show_parents = true;
                self.history_overlay.show_children = false;
                // TODO: Highlight parents
            }
            HistoryOverlayAction::ShowChildren(_feature_id) => {
                self.history_overlay.show_children = true;
                self.history_overlay.show_parents = false;
                // TODO: Highlight children
            }
            HistoryOverlayAction::ShowInfo(feature_id) => {
                // TODO: Show info dialog
                self.last_error = Some(format!("Info for feature {:?} - TODO", feature_id));
            }
            HistoryOverlayAction::DragFeature(feature_id) => {
                self.dragged_feature = Some(feature_id);
            }
            HistoryOverlayAction::DropFeature(feature_id) => {
                self.drop_target = Some(feature_id);
                // TODO: Handle reorder
                if let (Some(dragged), Some(target)) = (self.dragged_feature, self.drop_target) {
                    if let Some(part) = self.get_active_part_mut() {
                        if let (Some(from_idx), Some(to_idx)) = (
                            part.features.iter().position(|f| f.id == dragged),
                            part.features.iter().position(|f| f.id == target),
                        ) {
                            if from_idx != to_idx {
                                // Use move_feature from rcad-features
                                rcad_features::move_feature(part, dragged, to_idx);
                                self.rebuild_active_part();
                            }
                        }
                    }
                }
                self.dragged_feature = None;
                self.drop_target = None;
            }
        }
    }
}

impl Default for RcadApp {
    fn default() -> Self {
        let settings = AppSettings::load();
        let (job_tx, job_rx) = bounded::<RebuildJob>(2);
        let (res_tx, res_rx) = bounded::<RebuildResponse>(2);
        let version_counter = Arc::new(AtomicU64::new(0));
        let version_worker = version_counter.clone();
        
        let kernel_worker_handle = thread::Builder::new()
            .name("rcad-kernel-worker".into())
            .spawn(move || {
                let mut backend = rcad_occt::OcctBackend::default();
                let mut cache = ShapeCache::new();
                while let Ok(job) = job_rx.recv() {
                    // Versions are 1-based: a queued job is stale only when a
                    // strictly newer one has been requested. The old check used
                    // the pre-increment value, so it discarded every job.
                    if job.version < version_worker.load(Ordering::Relaxed) {
                        continue;
                    }
                    let res = cache.rebuild_part_incremental(&job.part, &mut backend);
                    let _ = res_tx.send(RebuildResponse {
                        version: job.version,
                        mesh: res.display_mesh,
                        errors: res.errors,
                    });
                }
            })
            .expect("Failed to spawn kernel worker thread");
        
        Self {
            camera: OrbitCamera::default(),
            mesh: TriangleMesh::default(),
            project: Project::default(),
            current_path: None,
            occt_backend: OcctBackend::default(),
            sketch: None,
            last_error: None,
            undo_stack: UndoStack::default(),
            shortcuts: Shortcuts::default(),
            show_shortcuts_dialog: false,
            
            // New UI components
            toolbar: VerticalToolbar::default(),
            history_overlay: HistoryOverlay::default(),
            
            // Relations dialog
            show_relations_dialog: None,
            
            dragged_feature: None,
            drop_target: None,
            
            // Selection state
            selection: Selection::default(),
            hover: None,
            hover_candidates: Vec::new(),
            hover_cycle: 0,
            selection_mode: SelectionMode::Object,
            
            // Async kernel worker
            kernel_worker_tx: Some(job_tx),
            kernel_worker_rx: Some(res_rx),
            kernel_version: version_counter,
            kernel_worker_handle: Some(kernel_worker_handle),
            needs_rebuild: true,
            capture: None,
            needs_fit: true,
            feature_dialog_tab: 0,
            new_parameter: NewParameterDraft::default(),
            floating_bar: FloatingBar::default(),
            placing_component: false,
            settings: settings.clone(),
            saved_settings: settings,
            show_settings: false,
            status: None,
            project_root: None,
            manifest: ProjectManifest::default(),
            project_error: None,
            new_colour_id: String::new(),
            pending_demo_colours: false,
            pending_demo_select: false,
            pending_save: None,
            mesh_ready: false,
        }
    }
}

impl Drop for RcadApp {
    /// Stop the kernel worker before the app goes away.
    ///
    /// The worker owns the OpenCascade backend. Left running detached, it could
    /// still be dropping kernel objects while the process tears down its
    /// libraries, which showed up as an intermittent SIGSEGV at the end of the
    /// test run. Dropping the response receiver first means the worker can
    /// never block on a full channel, and joining waits for the backend to be
    /// released on this thread instead.
    fn drop(&mut self) {
        self.kernel_worker_rx.take();
        self.kernel_worker_tx.take();
        if let Some(handle) = self.kernel_worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl eframe::App for RcadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

/// Unit vector, or `None` when the vector is degenerate.
fn normalised(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (length > 1e-9).then(|| [v[0] / length, v[1] / length, v[2] / length])
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The first triangle of the face with this stable name, and its label.
fn first_triangle_of_face(mesh: &TriangleMesh, name: &str) -> Option<(usize, String)> {
    (0..mesh.triangle_count())
        .find(|triangle| mesh.face_name(*triangle) == Some(name))
        .map(|triangle| {
            (
                triangle,
                mesh.face_label(triangle).unwrap_or(name).to_string(),
            )
        })
}

/// The stable names of the top and of the right-hand face of a mesh.
///
/// Used by `--demo-colours` to make the colour scheme visible in a screenshot.
/// Picked from the summed normal of each face's triangles, so the choice does
/// not depend on tessellation order or on knowing OpenCascade ids in advance.
fn demo_face_names(mesh: &TriangleMesh) -> Option<(String, String)> {
    let mut summed: std::collections::HashMap<u64, glam::Vec3> = std::collections::HashMap::new();
    for (triangle, indices) in mesh.indices.chunks_exact(3).enumerate() {
        let Some(face_id) = mesh.face_id(triangle) else {
            continue;
        };
        if face_id == rcad_geom::NO_FACE_ID {
            continue;
        }
        let normal = mesh
            .normals
            .get(indices[0] as usize)
            .copied()
            .unwrap_or_default();
        *summed.entry(face_id).or_insert(glam::Vec3::ZERO) += glam::Vec3::from(normal);
    }
    let top = *summed.iter().max_by(|a, b| a.1.z.total_cmp(&b.1.z))?.0;
    let side = *summed
        .iter()
        .filter(|(id, _)| **id != top)
        .max_by(|a, b| a.1.x.total_cmp(&b.1.x))?
        .0;
    let name_of = |face_id: u64| mesh.face_names.get(&face_id).map(|face| face.name.clone());
    Some((name_of(top)?, name_of(side)?))
}

/// Pull a screenshot out of this frame's input events, if egui delivered one.
fn take_screenshot(ctx: &egui::Context) -> Option<egui::ColorImage> {
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Screenshot { image, .. } => Some((**image).clone()),
            _ => None,
        })
    })
}

/// Write an egui framebuffer capture to a PNG file.
fn save_screenshot(path: &Path, image: &egui::ColorImage) -> Result<(), String> {
    let [width, height] = image.size;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in &image.pixels {
        rgba.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
    }
    let buffer = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
        .ok_or_else(|| "screenshot size does not match its pixel buffer".to_string())?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    buffer.save(path).map_err(|e| e.to_string())
}
#[cfg(test)]
mod interaction_tests {
    use super::*;

    /// A fresh throwaway project folder for a test.
    fn temp_project_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "rcad-test-project-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        root
    }
    use crate::ui::ToolbarAction;

    /// A headless egui session driving the real application UI.
    ///
    /// Clicks are delivered as genuine pointer events, so these tests go
    /// through the same path a user does: layout, hit-testing, action dispatch
    /// and state mutation. That catches wiring bugs which unit tests on the
    /// underlying functions cannot see.
    struct Session {
        ctx: egui::Context,
        app: RcadApp,
        screen: egui::Rect,
    }

    impl Session {
        /// A session with a throwaway project open: the modelling UI is
        /// unreachable without one.
        fn new(app: RcadApp) -> Self {
            let mut app = app;
            if app.project_root.is_none() {
                app.open_project_folder(temp_project_dir());
            }
            Self::without_project(app)
        }

        /// A session with no project at all, to test the gate itself.
        fn without_project(app: RcadApp) -> Self {
            let _ = &app;
            let ctx = egui::Context::default();
            let screen =
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));
            let mut session = Self { ctx, app, screen };
            // Two frames: widgets only report their rect once laid out.
            session.frame();
            session.frame();
            session
        }

        fn input(&self, events: Vec<egui::Event>) -> egui::RawInput {
            egui::RawInput {
                screen_rect: Some(self.screen),
                events,
                ..Default::default()
            }
        }

        /// Run one frame with no input.
        fn frame(&mut self) {
            let raw = self.input(Vec::new());
            let Self { ctx, app, .. } = self;
            let _ = ctx.run(raw, |ctx| app.ui(ctx));
        }

        /// Press and release the primary button at `pos`, then settle.
        fn click(&mut self, pos: egui::Pos2) {
            let press = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let raw = self.input(press);
            {
                let Self { ctx, app, .. } = self;
                let _ = ctx.run(raw, |ctx| app.ui(ctx));
            }

            let release = vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }];
            let raw = self.input(release);
            {
                let Self { ctx, app, .. } = self;
                let _ = ctx.run(raw, |ctx| app.ui(ctx));
            }

            // One more frame so the dispatched action is fully applied.
            self.frame();
        }

        /// Send a key press with modifiers (for shortcut tests).
        fn press_key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            let events = vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }];
            let mut raw = self.input(events);
            raw.modifiers = modifiers;
            {
                let Self { ctx, app, .. } = self;
                let _ = ctx.run(raw, |ctx| app.ui(ctx));
            }
            // Release: the next frame has no modifiers and no key events.
            self.frame();
        }

        /// Centre of a toolbar button drawn in the last frame.
        fn button_center(&self, action: ToolbarAction) -> egui::Pos2 {
            self.app
                .toolbar
                .button_rect(&action)
                .unwrap_or_else(|| panic!("{action:?} was not drawn on the active tab"))
                .center()
        }

        fn click_toolbar(&mut self, action: ToolbarAction) {
            let center = self.button_center(action);
            self.click(center);
        }
    }

    fn features(app: &RcadApp) -> &Vec<Feature> {
        &app.project.parts.first().expect("project has a part").features
    }

    #[test]
    fn clicking_the_box_button_adds_a_feature() {
        let mut session = Session::new(RcadApp::default());
        let before = features(&session.app).len();

        session.click_toolbar(ToolbarAction::ModelBox);

        assert_eq!(
            features(&session.app).len(),
            before + 1,
            "clicking Box in the toolbar did not add a feature"
        );
    }

    #[test]
    fn clicking_box_twice_creates_two_uniquely_named_features() {
        let mut session = Session::new(RcadApp::default());
        session.click_toolbar(ToolbarAction::ModelBox);
        session.click_toolbar(ToolbarAction::ModelBox);

        let names: Vec<String> = features(&session.app)
            .iter()
            .map(|feature| feature.name.clone())
            .collect();
        assert_eq!(names, vec!["Box1".to_string(), "Box2".to_string()]);
    }

    #[test]
    fn ctrl_z_reverts_the_box_added_by_the_toolbar() {
        let mut session = Session::new(RcadApp::default());
        let before = features(&session.app).len();

        session.click_toolbar(ToolbarAction::ModelBox);
        assert_eq!(features(&session.app).len(), before + 1);

        session.press_key(egui::Key::Z, egui::Modifiers::CTRL);

        assert_eq!(
            features(&session.app).len(),
            before,
            "Ctrl+Z did not undo the box that the toolbar added"
        );
    }

    /// Clicking the feature type icon in the model tree must open the feature
    /// dialog for that feature.
    #[test]
    fn clicking_a_feature_type_icon_opens_the_feature_dialog() {
        let mut session = Session::new(RcadApp::default());
        session.click_toolbar(ToolbarAction::ModelBox);
        let feature_id = features(&session.app)[0].id;
        assert!(session.app.show_relations_dialog.is_none());

        let rect = session
            .app
            .history_overlay
            .type_button_rect(feature_id)
            .expect("the feature type icon was not drawn in the model tree");
        session.click(rect.center());

        assert_eq!(
            session.app.show_relations_dialog,
            Some(feature_id),
            "clicking the feature type icon did not open the feature dialog"
        );
    }

    /// The floating bar toggles datum families one at a time or all together.
    #[test]
    fn toggling_datums_hides_and_shows_construction_geometry() {
        let mut app = RcadApp::default();
        let part = &mut app.project.parts[0];
        part.features.push(Feature::new(
            "FRONT_PLANE",
            FeatureKind::Construction {
                kind: ConstructionKind::Plane,
                origin: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
            },
        ));
        part.features.push(Feature::new(
            "X_AXIS",
            FeatureKind::Construction {
                kind: ConstructionKind::Axis,
                origin: [0.0; 3],
                direction: [1.0, 0.0, 0.0],
            },
        ));

        assert!(app.datum_kind_shown(DatumKind::Plane));
        assert!(app.datum_kind_shown(DatumKind::Axis));

        // One family at a time.
        app.toggle_datums(Some(DatumKind::Plane));
        assert!(!app.datum_kind_shown(DatumKind::Plane));
        assert!(app.datum_kind_shown(DatumKind::Axis), "axes should be untouched");

        // Every family at once: something was showing, so everything hides.
        app.toggle_datums(None);
        assert!(!app.datum_kind_shown(DatumKind::Plane));
        assert!(!app.datum_kind_shown(DatumKind::Axis));

        // And back on again.
        app.toggle_datums(None);
        assert!(app.datum_kind_shown(DatumKind::Plane));
        assert!(app.datum_kind_shown(DatumKind::Axis));
    }

    /// View shortcuts now live on the floating bar, so click the real button.
    #[test]
    fn clicking_the_floating_bar_top_button_moves_the_camera() {
        let mut session = Session::new(RcadApp::default());
        session.app.set_view_front();

        let rect = session
            .app
            .floating_bar
            .button_rect("view-view-top")
            .expect("the Top button was drawn on the floating bar");
        session.click(rect.center());

        // Z is up, so the Top view looks straight down the Z axis.
        let eye_direction = (session.app.camera.eye() - session.app.camera.target).normalize();
        assert!(
            (eye_direction - glam::Vec3::Z).length() < 1e-3,
            "the floating bar Top button did not move the camera: {eye_direction:?}"
        );
    }

    const TOP_FACE: &str = "F:plane:a1.0000:c0.0000,0.0000,0.0000:n0.000,0.000,1.000:e3:p3.0000";
    const SIDE_FACE: &str = "F:plane:a1.0000:c0.0000,0.0000,0.0000:n1.000,0.000,0.000:e3:p3.0000";

    /// Two triangles with distinct faces: one facing +Z (kernel id 5) and one
    /// facing +X (kernel id 9), each with its stable name.
    fn two_face_mesh() -> TriangleMesh {
        let mut mesh = TriangleMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            face_ids: vec![5, 9],
            ..Default::default()
        };
        for (id, name) in [(5, TOP_FACE), (9, SIDE_FACE)] {
            mesh.face_names.insert(
                id,
                rcad_geom::NamedFace {
                    name: name.to_string(),
                    signature: name.to_string(),
                    label: format!("face {id}"),
                },
            );
        }
        mesh
    }

    /// Step F: tagging the selected face stores the logical id on the part, and
    /// clearing it removes the tag.
    #[test]
    fn tagging_a_face_stores_and_clears_its_colour() {
        let mut app = RcadApp::default();
        app.open_project_folder(temp_project_dir());

        app.apply_face_colour(TOP_FACE.to_string(), "top face".to_string(), Some("fit zone".to_string()));
        assert_eq!(
            app.get_active_part().unwrap().face_colour_id(TOP_FACE),
            Some("fit zone")
        );

        app.apply_face_colour(TOP_FACE.to_string(), "top face".to_string(), None);
        assert!(app.get_active_part().unwrap().face_colour_id(TOP_FACE).is_none());
    }

    /// The demo helper picks the top and the right-hand face from the geometry,
    /// not from the tessellation order, and returns their stable names.
    #[test]
    fn demo_face_names_pick_the_top_and_the_right_face() {
        assert_eq!(
            demo_face_names(&two_face_mesh()),
            Some((TOP_FACE.to_string(), SIDE_FACE.to_string()))
        );
    }

    /// `--demo-colours`: the first two ids the project defines are applied once
    /// a mesh with stable names has arrived, and the part keeps the ids so the
    /// project still owns the RGB values.
    #[test]
    fn demo_colours_tag_two_faces_from_the_project() {
        let mut app = RcadApp::default();
        app.open_project_folder(temp_project_dir());
        app.manifest.surface_colors.insert("fit zone".to_string(), [0, 255, 0]);
        app.manifest.surface_colors.insert("threads".to_string(), [255, 165, 0]);
        app.mesh = two_face_mesh();

        app.tag_demo_faces_with_project_colours();
        app.apply_pending_demo_colours();

        let part = app.get_active_part().unwrap();
        assert_eq!(part.face_colour_id(TOP_FACE), Some("fit zone"));
        assert_eq!(part.face_colour_id(SIDE_FACE), Some("threads"));
        assert_eq!(
            part.face_colour(TOP_FACE, &app.manifest),
            Some([0, 255, 0]),
            "the RGB value must come from the project manifest"
        );
    }

    /// With a face selected the status bar draws the colour control; the frame
    /// must still run (and offer only ids the project defines).
    #[test]
    fn a_selected_face_draws_the_colour_control() {
        let mut session = Session::new(RcadApp::default());
        session.app.mesh = two_face_mesh();
        session.app.selection.replace(Selected::Face {
            reference: TOP_FACE.to_string(),
            label: "face 5".to_string(),
            triangle: 0,
        });
        session
            .app
            .manifest
            .surface_colors
            .insert("fit zone".to_string(), [0, 255, 0]);

        session.frame();
        session.frame();

        assert_eq!(session.app.mesh.face_name(0), Some(TOP_FACE));
    }

    /// `--save` and `--open` go through the same methods as the tests.
    #[test]
    fn a_document_saves_and_reopens_through_the_headless_path() {
        let root = temp_project_dir();
        let mut app = RcadApp::default();
        app.open_project_folder(root.clone());
        app.project.parts[0].name = "Reopened".to_string();
        let path = root.join("doc.rcad");

        app.save_document_at(path.clone()).expect("save");

        let mut opened = RcadApp::default();
        opened.open_document_at(path).expect("open");
        assert_eq!(opened.project.parts[0].name, "Reopened");
        assert!(opened.needs_rebuild, "opening a document must rebuild it");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Creo behaviour in the tree: a plain click replaces, Ctrl adds, and Ctrl
    /// on a selected row removes it, keeping the order of the rest.
    #[test]
    fn tree_clicks_collect_an_ordered_selection() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        let part = app.project.parts.first().unwrap();
        let datums: Vec<uuid::Uuid> = part
            .features
            .iter()
            .filter(|feature| matches!(feature.kind, FeatureKind::Construction { .. }))
            .map(|feature| feature.id)
            .collect();
        let (first, second) = (datums[0], datums[1]);

        app.handle_history_action(HistoryOverlayAction::SelectFeature {
            feature: first,
            toggle: false,
        });
        app.handle_history_action(HistoryOverlayAction::SelectFeature {
            feature: second,
            toggle: true,
        });

        assert_eq!(app.selection.len(), 2);
        assert_eq!(app.selection.items()[0].feature(), Some(first));
        assert_eq!(app.selection.items()[1].feature(), Some(second));

        // Ctrl on an already selected row removes it and keeps the rest.
        app.handle_history_action(HistoryOverlayAction::SelectFeature {
            feature: first,
            toggle: true,
        });
        assert_eq!(app.selection.len(), 1);
        assert_eq!(app.selection.items()[0].feature(), Some(second));
    }

    /// The first consumer of the shared selection: a sketch placed on the
    /// selected datum plane, instead of waiting in placement mode.
    #[test]
    fn a_sketch_is_placed_on_the_selected_datum_plane() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        let plane = app
            .project
            .parts
            .first()
            .unwrap()
            .features
            .iter()
            .find(|feature| {
                matches!(
                    &feature.kind,
                    FeatureKind::Construction {
                        kind: ConstructionKind::Plane,
                        ..
                    }
                )
            })
            .unwrap()
            .id;
        app.selection.replace(Selected::Datum {
            feature: plane,
            label: "FRONT_PLANE".to_string(),
        });

        app.new_sketch();

        let sketch = app.sketch.as_ref().expect("a sketch should be open");
        assert!(
            !sketch.in_placement_mode,
            "the sketch should be placed on the selected plane"
        );
    }

    fn test_edge(
        name: &str,
        start: [f64; 3],
        end: [f64; 3],
        tangent: [f64; 3],
    ) -> rcad_geom::NamedEdge {
        rcad_geom::NamedEdge {
            name: name.to_string(),
            signature: name.to_string(),
            label: name.to_string(),
            start,
            end,
            start_tangent: tangent,
            end_tangent: tangent,
            length: ((end[0] - start[0]).powi(2)
                + (end[1] - start[1]).powi(2)
                + (end[2] - start[2]).powi(2))
                .sqrt(),
            ..Default::default()
        }
    }

    /// A tangent chain extends the selection from the seed edge, in order, and
    /// the result is references, ready for a fillet to store.
    #[test]
    fn a_tangent_chain_query_extends_the_selection() {
        let mut app = RcadApp::default();
        app.mesh.edges = vec![
            test_edge("a", [0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            test_edge("b", [5.0, 0.0, 0.0], [10.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            test_edge("c", [10.0, 0.0, 0.0], [10.0, 5.0, 0.0], [0.0, 1.0, 0.0]),
        ];
        app.selection.replace(Selected::Edge {
            reference: "a".to_string(),
            label: "a".to_string(),
        });

        app.apply_selection_query(SelectionQuery::TangentChain);

        assert_eq!(app.selection.len(), 2, "the collinear neighbour joins");
        assert_eq!(app.selection.edge_references(), vec!["a", "b"]);
    }

    /// Without an edge in the selection the query says so instead of guessing.
    #[test]
    fn a_query_without_an_edge_seed_explains_itself() {
        let mut app = RcadApp::default();
        app.apply_selection_query(SelectionQuery::TangentChain);

        let status = app.status.clone().unwrap_or_default();
        assert!(status.contains("Pick an edge"), "{status}");
    }

    /// A round created from the selection stores the edge references, and the
    /// rule modes are stored as rules: the rebuild re-resolves them.
    #[test]
    fn a_round_from_the_selection_stores_its_edge_set() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        app.add_edge_feature(
            EdgeFeature::Fillet,
            1.0,
            EdgeSet::References(vec!["edge(a|b)".to_string()]),
        );
        app.add_edge_feature(EdgeFeature::Chamfer, 1.0, EdgeSet::Inner);

        let part = app.project.parts.first().unwrap();
        let rounds: Vec<&Feature> = part
            .features
            .iter()
            .filter(|feature| matches!(feature.kind, FeatureKind::Fillet { .. }))
            .collect();
        assert_eq!(rounds.len(), 1, "one round");
        match &rounds[0].kind {
            FeatureKind::Fillet { radius, edges } => {
                assert_eq!(*radius, 1.0);
                assert_eq!(*edges, EdgeSet::References(vec!["edge(a|b)".to_string()]));
            }
            _ => unreachable!(),
        }
        assert!(part.features.iter().any(|feature| matches!(
            &feature.kind,
            FeatureKind::Chamfer { edges: EdgeSet::Inner, .. }
        )));
    }

    /// A tag whose face is gone (edited or deleted) is reported, not silently
    /// dropped and not painted on whichever face took its place.
    #[test]
    fn a_lost_face_tag_is_reported_in_the_status_bar() {
        let mut app = RcadApp::default();
        app.open_project_folder(temp_project_dir());
        app.mesh = two_face_mesh();
        app.apply_face_colour(
            TOP_FACE.to_string(),
            "top face".to_string(),
            Some("fit zone".to_string()),
        );
        app.apply_face_colour(
            "F:deleted-face".to_string(),
            "deleted face".to_string(),
            Some("fit zone".to_string()),
        );

        app.report_unmatched_face_colours();

        let status = app.status.clone().unwrap_or_default();
        assert!(
            status.contains("1 colour tag"),
            "the lost tag was not reported: {status}"
        );
    }


    /// The whole story: tag a face, change a parameter, save the document,
    /// reopen it in a fresh kernel - the colour is still on the same face.
    #[test]
    fn a_face_colour_survives_a_parameter_change_and_a_reload() {
        let root = temp_project_dir();
        std::fs::create_dir_all(&root).unwrap();

        let mut part = rcad_doc::Part::new_solid("P");
        let tool = Feature::new("HoleTool", FeatureKind::Cylinder { radius: 5.0, height: 20.0 });
        let tool_id = tool.id;
        part.features.push(Feature::new("Base", FeatureKind::Box { size: [40.0, 30.0, 8.0] }));
        part.features.push(tool);
        part.features.push(Feature::new(
            "CutHole",
            FeatureKind::Boolean {
                tool_feature: tool_id,
                op: rcad_geom::BooleanOp::Subtract,
            },
        ));

        // Rebuild once, pick the top face of the block and tag it.
        let rebuilt = {
            let mut backend = rcad_occt::OcctBackend::default();
            let mut cache = ShapeCache::new();
            cache.rebuild_part_incremental(&part, &mut backend)
        };
        let top_face = rebuilt
            .display_mesh
            .face_names
            .values()
            .find(|face| face.name.contains("/box/z+"))
            .expect("the block should have a top face")
            .name
            .clone();
        part.set_face_colour(&top_face, "fit zone");

        // The user changes the height of the block, then saves.
        part.features[0].kind = FeatureKind::Box { size: [40.0, 30.0, 24.0] };
        let mut project = Project::default();
        project.parts = vec![part];
        let path = root.join("doc.rcad");
        project.save(&path).expect("save");

        // Another process: new kernel, new addresses, rebuild from the file.
        let loaded = Project::load(&path).expect("load");
        let mut backend = rcad_occt::OcctBackend::default();
        let mut cache = ShapeCache::new();
        let rebuilt = cache.rebuild_part_incremental(&loaded.parts[0], &mut backend);

        let names: Vec<String> = rebuilt
            .display_mesh
            .face_names
            .values()
            .map(|face| face.name.clone())
            .collect();
        assert!(
            names.contains(&top_face),
            "the tagged face did not survive the height change: {names:?}"
        );
        let mut manifest = ProjectManifest::default();
        manifest.surface_colors.insert("fit zone".to_string(), [0, 255, 0]);
        assert_eq!(
            loaded.parts[0].face_colour(&top_face, &manifest),
            Some([0, 255, 0])
        );
        assert!(
            loaded.parts[0].unmatched_face_colours(&names).is_empty(),
            "the colour tag is reported as unmatched"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The requirement that started this: tag a face, save the document, load
    /// it in a *fresh kernel* (new shapes, new addresses) and the colour must
    /// still land on the same face.
    #[test]
    fn a_face_colour_survives_a_save_and_a_reload_in_a_fresh_kernel() {
        let project_root = temp_project_dir();
        let mut app = RcadApp::default();
        app.open_project_folder(project_root.clone());
        app.manifest.surface_colors.insert("fit zone".to_string(), [0, 255, 0]);

        // First kernel: build a real box and take the stable name of one face.
        let mut first_backend = rcad_occt::OcctBackend::default();
        let first_shape = first_backend.box_solid(glam::DVec3::new(10.0, 20.0, 30.0)).unwrap();
        let first_mesh = first_backend.tessellate(first_shape).unwrap();
        let face_name = first_mesh.face_names.values().next().unwrap().name.clone();
        let first_ids: std::collections::BTreeSet<u64> =
            first_mesh.face_names.keys().copied().collect();
        app.apply_face_colour(face_name.clone(), "face".to_string(), Some("fit zone".to_string()));

        // Save and load the document, then rebuild in a second kernel.
        let path = project_root.join("part.rcad");
        app.project.save(&path).expect("save");
        let reloaded = rcad_doc::Project::load(&path).expect("load");
        let mut second_backend = rcad_occt::OcctBackend::default();
        let second_shape = second_backend.box_solid(glam::DVec3::new(10.0, 20.0, 30.0)).unwrap();
        let second_mesh = second_backend.tessellate(second_shape).unwrap();
        let second_ids: std::collections::BTreeSet<u64> =
            second_mesh.face_names.keys().copied().collect();

        assert_ne!(first_ids, second_ids, "a fresh kernel has fresh addresses");
        assert_eq!(
            reloaded.parts[0].face_colour_id(&face_name),
            Some("fit zone"),
            "the tag must survive the round trip"
        );
        let resolved = reloaded.parts[0].face_colour(&face_name, &app.manifest);
        assert_eq!(resolved, Some([0, 255, 0]));
        assert_eq!(
            reloaded.parts[0].unmatched_face_colours(
                &second_mesh.face_names.values().map(|face| face.name.clone()).collect::<Vec<_>>()
            ),
            Vec::<&str>::new(),
            "the tag still matches a face in the rebuilt model"
        );
        let _ = std::fs::remove_dir_all(&project_root);
    }

    /// 0.1: a folder without a manifest becomes a project.
    #[test]
    fn opening_a_folder_without_a_manifest_creates_the_project() {
        let root = temp_project_dir();
        let mut app = RcadApp::default();
        app.open_project_folder(root.clone());

        assert_eq!(app.project_root.as_deref(), Some(root.as_path()));
        assert!(rcad_doc::ProjectManifest::path_in(&root).exists());
        assert_eq!(
            app.settings.last_project.as_deref(),
            Some(root.to_string_lossy().as_ref())
        );
        assert!(app.project_error.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 0.1: with no project, the modelling UI must be unreachable.
    #[test]
    fn without_a_project_the_toolbar_is_not_drawn() {
        let session = Session::without_project(RcadApp::default());
        assert!(session.app.project_root.is_none());
        assert!(
            session
                .app
                .toolbar
                .button_rect(&ToolbarAction::ModelBox)
                .is_none(),
            "the modelling toolbar must not exist before a project is open"
        );
    }

    /// ...and it appears as soon as a project is open.
    #[test]
    fn opening_a_project_reveals_the_modelling_ui() {
        let mut session = Session::new(RcadApp::default());
        session.frame();
        assert!(session.app.project_root.is_some());
        assert!(
            session
                .app
                .toolbar
                .button_rect(&ToolbarAction::ModelBox)
                .is_some()
        );
    }

    /// File > New must not throw the project away.
    #[test]
    fn a_new_document_keeps_the_project() {
        let mut session = Session::new(RcadApp::default());
        let root = session.app.project_root.clone();
        assert!(root.is_some(), "the session opens a project");
        session.app.new_document();
        assert_eq!(session.app.project_root, root);
    }

    /// 0.2: the manifest is read when the project opens.
    #[test]
    fn opening_a_project_reads_its_manifest() {
        let root = temp_project_dir();
        let mut manifest = rcad_doc::ProjectManifest::create_default("Cliente A");
        manifest.general_tolerance.linear = 0.002;
        manifest
            .surface_colors
            .insert("fit zone".to_string(), [10, 20, 30]);
        manifest.save(&root).expect("write the manifest");

        let mut app = RcadApp::default();
        app.open_project_folder(root.clone());

        assert_eq!(app.manifest.project.name, "Cliente A");
        assert_eq!(app.manifest.general_tolerance.linear, 0.002);
        assert_eq!(app.manifest.colour("fit zone"), Some([10, 20, 30]));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Opening projects remembers them, most recent first and without repeats.
    #[test]
    fn opening_projects_remembers_them_without_repeats() {
        let first = temp_project_dir();
        let second = temp_project_dir();
        let mut app = RcadApp::default();
        app.open_project_folder(first.clone());
        app.open_project_folder(second.clone());
        app.open_project_folder(first.clone());

        assert_eq!(
            app.settings.recent_projects,
            vec![
                first.to_string_lossy().to_string(),
                second.to_string_lossy().to_string()
            ]
        );
        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
    }
}

/// Human-readable feature type for the dialog heading.
fn feature_kind_label(kind: &FeatureKind) -> &'static str {
    match kind {
        FeatureKind::Box { .. } => "Box",
        FeatureKind::Cylinder { .. } => "Cylinder",
        FeatureKind::Extrude { .. } => "Extrude",
        FeatureKind::ExtrudeSurface { .. } => "Extrude surface",
        FeatureKind::Revolve { .. } => "Revolve",
        FeatureKind::Boolean { .. } => "Boolean",
        FeatureKind::Translate { .. } => "Translate",
        FeatureKind::Fillet { .. } => "Round",
        FeatureKind::Chamfer { .. } => "Chamfer",
        FeatureKind::Sew { .. } => "Sew",
        FeatureKind::Loft { .. } => "Loft",
        FeatureKind::Boundary { .. } => "Boundary surface",
        FeatureKind::Thicken { .. } => "Thicken",
        FeatureKind::Construction { kind, .. } => match kind {
            ConstructionKind::Plane => "Datum plane",
            ConstructionKind::Axis => "Datum axis",
            ConstructionKind::Point => "Datum point",
            ConstructionKind::Curve => "Datum curve",
            ConstructionKind::CoordSystem => "Coordinate system",
            ConstructionKind::Solid => "Datum solid",
        },
    }
}

/// Parameters tab: edit, add and remove the feature's parameters.
///
/// Creo calls this the feature dashboard; here the parameters are the
/// structured values stored on the feature (numeric, string, boolean).
fn draw_parameters_tab(
    ui: &mut egui::Ui,
    parameters: &mut rcad_doc::FeatureParameters,
    draft: &mut NewParameterDraft,
) {
    egui::ScrollArea::vertical()
        .max_height(240.0)
        .show(ui, |ui| {
            egui::Grid::new("feature_parameters")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Value");
                    ui.strong("Type");
                    ui.strong("");
                    ui.end_row();

                    let numeric_keys: Vec<String> = parameters.numeric.keys().cloned().collect();
                    let mut remove_numeric: Option<String> = None;
                    for name in numeric_keys {
                        ui.label(&name);
                        if let Some(value) = parameters.numeric.get_mut(&name) {
                            ui.add(egui::DragValue::new(value).speed(0.1));
                        }
                        ui.label("Real");
                        if ui.small_button("✕").on_hover_text("Remove").clicked() {
                            remove_numeric = Some(name.clone());
                        }
                        ui.end_row();
                    }
                    if let Some(name) = remove_numeric {
                        parameters.numeric.remove(&name);
                    }

                    let string_keys: Vec<String> = parameters.string.keys().cloned().collect();
                    let mut remove_string: Option<String> = None;
                    for name in string_keys {
                        ui.label(&name);
                        if let Some(value) = parameters.string.get_mut(&name) {
                            ui.add(egui::TextEdit::singleline(value).desired_width(120.0));
                        }
                        ui.label("String");
                        if ui.small_button("✕").on_hover_text("Remove").clicked() {
                            remove_string = Some(name.clone());
                        }
                        ui.end_row();
                    }
                    if let Some(name) = remove_string {
                        parameters.string.remove(&name);
                    }

                    let boolean_keys: Vec<String> = parameters.boolean.keys().cloned().collect();
                    let mut remove_boolean: Option<String> = None;
                    for name in boolean_keys {
                        ui.label(&name);
                        if let Some(value) = parameters.boolean.get_mut(&name) {
                            ui.checkbox(value, "");
                        }
                        ui.label("Boolean");
                        if ui.small_button("✕").on_hover_text("Remove").clicked() {
                            remove_boolean = Some(name.clone());
                        }
                        ui.end_row();
                    }
                    if let Some(name) = remove_boolean {
                        parameters.boolean.remove(&name);
                    }
                });

            ui.separator();
            ui.label("Add a parameter:");

            let name_ok = !draft.name.trim().is_empty();
            // Only complain once something has been typed: an empty box is not
            // an error yet.
            let value_ok = draft.value.trim().is_empty()
                || match draft.kind {
                    0 => draft.value.trim().parse::<f64>().is_ok(),
                    _ => true,
                };

            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .hint_text("name")
                        .desired_width(90.0),
                );
                egui::ComboBox::from_id_salt("new_parameter_kind")
                    .selected_text(match draft.kind {
                        1 => "String",
                        2 => "Boolean",
                        _ => "Real",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut draft.kind, 0, "Real");
                        ui.selectable_value(&mut draft.kind, 1, "String");
                        ui.selectable_value(&mut draft.kind, 2, "Boolean");
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut draft.value)
                        .hint_text("value")
                        .desired_width(70.0),
                );

                let add = ui.add_enabled(
                    name_ok && value_ok,
                    egui::Button::new("Add"),
                );
                if add.clicked() {
                    let name = draft.name.trim().to_string();
                    match draft.kind {
                        1 => {
                            parameters.string.insert(name, draft.value.clone());
                        }
                        2 => {
                            let value = matches!(draft.value.trim(), "true" | "1" | "yes");
                            parameters.boolean.insert(name, value);
                        }
                        _ => {
                            if let Ok(value) = draft.value.trim().parse::<f64>() {
                                parameters.numeric.insert(name, value);
                            }
                        }
                    }
                    draft.name.clear();
                    draft.value.clear();
                }

                if !value_ok {
                    ui.colored_label(egui::Color32::LIGHT_RED, "value must be a number");
                } else if !name_ok {
                    ui.label(egui::RichText::new("give the parameter a name").size(10.0));
                }
            });
        });
}

/// Relations tab: Creo-style part equations.
///
/// Returns true when the user asks to regenerate, so the caller can evaluate
/// the relation list and update the parameters.
fn draw_relations_tab(ui: &mut egui::Ui, relations: &mut Vec<rcad_doc::Relation>) -> bool {
    let mut regenerate = false;
    let mut remove: Option<usize> = None;

    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            if relations.is_empty() {
                ui.label("No relations yet.");
                ui.label(
                    egui::RichText::new("For example:  height = width * 2")
                        .size(10.0)
                        .color(egui::Color32::GRAY),
                );
            }

            for (index, relation) in relations.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut relation.lhs)
                            .desired_width(90.0)
                            .hint_text("name"),
                    );
                    ui.label("=");
                    ui.add(
                        egui::TextEdit::singleline(&mut relation.rhs)
                            .desired_width(200.0)
                            .hint_text("expression"),
                    );
                    if ui.small_button("✕").on_hover_text("Remove").clicked() {
                        remove = Some(index);
                    }
                });
            }
        });

    if let Some(index) = remove {
        relations.remove(index);
    }

    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Add relation").clicked() {
            relations.push(rcad_doc::Relation {
                lhs: "new_param".to_string(),
                rhs: "1".to_string(),
                comment: None,
            });
        }
        if ui
            .button("Regenerate")
            .on_hover_text("Evaluate every relation and update the parameters")
            .clicked()
        {
            regenerate = true;
        }
    });
    ui.label(
        egui::RichText::new(
            "Names resolve to part parameters, or to values defined earlier in this list.",
        )
        .size(10.0)
        .color(egui::Color32::GRAY),
    );

    regenerate
}
