use eframe::egui;
use crate::selection::{ClickMode, Selected, Selection};
use crate::viewport::{draw_sketch_plane, draw_viewport, pick_candidates, sketch_basis};
use crate::camera::{OrbitCamera, Projection};
use crate::settings::AppSettings;
use crate::undo::UndoStack;
use crate::shortcuts::{Shortcuts, handle_shortcuts, shortcuts_list};
use crate::sketch_ui::{
    self, ConstraintAction, OverlayState, SketchDraft, SketchEdit, SketchTool, GRID_EXTENT,
    GRID_SPACING, HIT_RADIUS,
};
use crate::ui::{
    DatumKind, FloatingAction, FloatingBar, HistoryOverlay, HistoryOverlayAction, ToolbarAction,
    VerticalToolbar,
};
use rcad_geom::{GeometryBackend, TriangleMesh};
use rcad_doc::{
    ConstructionKind, EdgeSet, Feature, FeatureKind, Part, Project, ProjectManifest,
    SketchPlacement,
};
use rcad_features;
use rcad_occt::OcctBackend;
use rcad_param::{ShapeCache};
use rcad_sketch::{
    Constraint, DimensionOrientation, ReferenceGeometryKind, Sketch, SketchReference,
    SketchReferenceRole,
};
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
    /// References the rebuild could not resolve: a plane or a face a feature
    /// was placed on, or a sketch's own reference.
    pub lost_references: Vec<rcad_doc::LostReference>,
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
    /// Size of the next round or break, in mm: the radius or the distance the
    /// toolbar and the selection menu use.
    edge_feature_size: f64,
    /// The plane the open sketch is drawn on: origin, normal and sketch x. None
    /// until a sketch is started.
    sketch_plane: Option<([f64; 3], [f64; 3], [f64; 3])>,
    /// Where every reference was the last time a build still had it, so a repair
    /// list can say where a lost one used to be. Merged, never replaced: a
    /// reference that disappears keeps its last known entry.
    reference_memory: std::collections::BTreeMap<String, rcad_geom::ReferenceMemory>,
    /// The references the part asks for that the body no longer has.
    lost_references: Vec<rcad_doc::LostReference>,
    /// The stable reference of the plane the open sketch was placed on, when the
    /// selection named one: a datum plane feature or a model face. With it, a
    /// feature the sketch feeds follows the plane instead of a snapshot of it.
    sketch_plane_reference: Option<String>,
    /// The sketch mouse tool and the first click of a two-click shape.
    sketch_draft: SketchDraft,
    /// Sketch entities picked in the viewport: points, lines, circles, arcs.
    sketch_selection: Vec<Uuid>,
    /// The sketch entity the cursor is over.
    sketch_hover: Option<Uuid>,
    /// Sketch undo history. The sketch is not part of the project, so it keeps
    /// its own snapshots: Ctrl+Z in a sketch steps back through the drawing.
    sketch_undo: Vec<Sketch>,
    sketch_redo: Vec<Sketch>,
    /// The dimension being edited in place, by its index in the sketch
    /// constraints, and the place the editor opens.
    dimension_editor: Option<DimensionEditor>,
    /// The dimension selected in the sketch, by its index in `constraints`.
    selected_dimension: Option<usize>,
    /// The right-click dimension menu: which dimension, and where.
    dimension_menu: Option<(usize, egui::Pos2)>,
    /// True once the dimension menu has been drawn a frame, so the click that
    /// opened it does not close it straight away.
    dimension_menu_was_shown: bool,
    /// The sketch feature being edited, when the session opened an existing one.
    /// None means OK will add a new Sketch feature to the history.
    sketch_feature: Option<Uuid>,
    /// What the dynamic dimension tool has picked so far.
    dimension_picks: Vec<(Uuid, sketch_ui::SketchEntityKind)>,
    /// Which direction the next linear dimension measures along.
    dimension_orientation: DimensionOrientation,
    /// Whether a radius or a diameter dimension is made from one curve.
    dimension_size: sketch_ui::DimensionSize,
    /// The sketch point the mouse is dragging.
    dragging_point: Option<Uuid>,
    /// Draw the sketch entities that still have a degree of freedom in another
    /// colour.
    show_free_dof: bool,
    /// True while the constraint table window is open.
    show_constraint_table: bool,
    /// The import dialog: whether it is open, and the file being read.
    show_import_dialog: bool,
    import_path: String,
    /// The text note being typed, if any.
    sketch_text_editor: Option<SketchTextEditor>,
    /// The pattern tool's mode and sweep, and the other command parameters the
    /// strip shows.
    tool_params: sketch_ui::ToolParams,
    /// The viewport rectangle of the last frame, so the floating sketch
    /// controls and the tests can speak in screen coordinates.
    viewport_rect: Option<egui::Rect>,
    
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

/// The in-place editor of a sketch text note: where it goes and what is being
/// typed.
#[derive(Debug, Clone)]
pub struct SketchTextEditor {
    /// The place in the sketch the text will sit at.
    pub position: [f64; 2],
    /// What the user has typed so far.
    pub content: String,
    /// Where the editor opens, in screen points.
    pub at: egui::Pos2,
    /// True until the field has taken the focus.
    pub fresh: bool,
}

/// The in-place value editor of a sketch dimension.
#[derive(Debug, Clone)]
pub struct DimensionEditor {
    /// Index of the dimension constraint in the sketch.
    pub index: usize,
    /// The value as the user is typing it.
    pub text: String,
    /// Where the editor opens when the dimension is not on screen.
    pub at: egui::Pos2,
    /// True until the field has taken the focus and selected its text, so
    /// typing replaces the value instead of appending to it.
    pub fresh: bool,
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
                        lost_references: res.lost_references,
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
            edge_feature_size: 1.0,
            sketch_plane: None,
            reference_memory: std::collections::BTreeMap::new(),
            lost_references: Vec::new(),
            sketch_plane_reference: None,
            sketch_draft: SketchDraft::default(),
            sketch_selection: Vec::new(),
            sketch_hover: None,
            sketch_undo: Vec::new(),
            sketch_redo: Vec::new(),
            dimension_editor: None,
            selected_dimension: None,
            dimension_menu: None,
            dimension_menu_was_shown: false,
            sketch_feature: None,
            show_free_dof: false,
            dimension_picks: Vec::new(),
            dimension_orientation: DimensionOrientation::default(),
            dimension_size: sketch_ui::DimensionSize::default(),
            show_constraint_table: false,
            show_import_dialog: false,
            import_path: String::new(),
            sketch_text_editor: None,
            dragging_point: None,
            tool_params: sketch_ui::ToolParams::default(),
            viewport_rect: None,
            
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
                    if ui.button("Sketch file (DXF/SVG)…").clicked() {
                        self.show_import_dialog = true;
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
        
        // 2a) Repair list, above the selection bar, when a feature lost a
        // reference and is waiting for another one.
        self.draw_lost_references(ctx);

        // 2a-bis) The sketch strip: status, degrees of freedom and the active
        // tool, shown only while a sketch is open.
        self.draw_sketch_panel(ctx);

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

            self.viewport_rect = Some(response.rect);

            // The open sketch takes the mouse before the model picker: its
            // tools turn clicks into geometry and read the cursor in sketch
            // coordinates, so the grid snap is visible while drawing.
            let (sketch_handled, sketch_cursor) = self.sketch_interaction(ui, &response);

            // The plane the open sketch is being drawn on, and the sketch on
            // it: seeing both is the difference between drawing and guessing.
            if let (Some((origin, normal, x_direction)), Some(sketch)) =
                (self.sketch_plane, self.sketch.as_ref())
            {
                if !sketch.in_placement_mode {
                    let (u, v) = sketch_basis(normal, x_direction);
                    draw_sketch_plane(
                        ui,
                        response.rect,
                        &self.camera,
                        origin,
                        u,
                        v,
                        GRID_EXTENT,
                        GRID_SPACING as f32,
                    );
                    // Geometry, constraints, dimensions and the points that
                    // are still free, all in sketch coordinates.
                    sketch_ui::draw_sketch_overlay(
                        ui,
                        response.rect,
                        &self.camera,
                        &OverlayState {
                            sketch,
                            plane: (origin, normal, x_direction),
                            selection: &self.sketch_selection,
                            hovered: self.sketch_hover,
                            draft: &self.sketch_draft,
                            cursor: sketch_cursor,
                            selected_dimension: self.selected_dimension,
                            editing_dimension: self
                                .dimension_editor
                                .as_ref()
                                .map(|editor| editor.index),
                            show_free_dof: self.show_free_dof,
                        },
                    );
                }
            }

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
            if sketch_handled {
                // The sketch consumed the click: no model selection this frame.
                self.hover = None;
                self.hover_candidates.clear();
                self.hover_cycle = 0;
            } else if response.hovered() {
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
                        // Remember where the references are now, keeping the last
                        // known entry of any that has already gone.
                        self.reference_memory.extend(rcad_geom::remember(&self.mesh));
                        if let Some(part) = self.get_active_part() {
                            self.lost_references = lost_references(part, &self.mesh);
                        }
                        // The kernel also reports the placements and sketch
                        // references it could not resolve; the two lists are
                        // merged, without repeating an entry.
                        self.lost_references.extend(response.lost_references);
                        let mut seen = std::collections::HashSet::new();
                        self.lost_references
                            .retain(|entry| seen.insert((entry.feature, entry.reference.clone())));
                        describe_lost(&mut self.lost_references, &self.reference_memory);
                        // Frame the model the first time it appears: the app
                        // previously opened at a fixed camera distance.
                        if self.needs_fit {
                            if let Some((min, max)) = self.mesh.bounds() {
                                // A box with no size - a lone construction
                                // point - would frame the camera at nothing
                                // (distance 1.7) and hide the whole document.
                                if (max - min).length() > 1e-3 {
                                    self.camera.fit_to_bounds(min, max);
                                }
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
        
        // Dimension editing floats over the viewport: the right-click menu and
        // the in-place value editor.
        self.draw_dimension_menu(ctx);
        self.draw_dimension_editor(ctx);
        self.draw_sketch_text_editor(ctx);
        self.draw_import_dialog(ctx);
        self.draw_constraint_table(ctx);

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

    /// The repair list: every reference a feature lost, where it used to be, and
    /// a way to put another one in its place.
    ///
    /// This is the Creo behaviour the user asked for: show the lost reference,
    /// say where it was, and let a substitution fix the feature.
    fn draw_lost_references(&mut self, ctx: &egui::Context) {
        if self.lost_references.is_empty() {
            return;
        }
        // The replacement is the selected edge, or the selected face or datum
        // plane: a sketch placed on a plane is repaired with another plane, not
        // with an edge.
        let replacement = self
            .selection
            .edge_references()
            .first()
            .map(|reference| reference.to_string())
            .or_else(|| self.selection_plane_reference());
        let mut substitute: Option<(Uuid, String, String)> = None;
        let mut dismiss = false;
        egui::TopBottomPanel::bottom("lost_references").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} lost reference(s) - pick an edge, a face or a datum plane and substitute",
                        self.lost_references.len()
                    ))
                    .strong()
                    .color(egui::Color32::from_rgb(235, 150, 90)),
                );
                for entry in &self.lost_references {
                    let where_it_was = entry
                        .last_description
                        .clone()
                        .unwrap_or_else(|| "no location was ever recorded for it".to_string());
                    let hover = format!("{}: {where_it_was}", entry.feature_name);
                    let clicked = match &replacement {
                        Some(reference) => ui
                            .button(format!("Substitute {} with {reference}", entry.reference))
                            .on_hover_text(hover)
                            .clicked(),
                        None => ui
                            .add_enabled(
                                false,
                                egui::Button::new(format!("Substitute {}", entry.reference)),
                            )
                            .on_hover_text(format!(
                                "{hover}\nSelect an edge, a face or a datum plane in the viewport first"
                            ))
                            .clicked(),
                    };
                    if clicked {
                        if let Some(reference) = &replacement {
                            substitute =
                                Some((entry.feature, entry.reference.clone(), reference.clone()));
                        }
                    }
                }
                if ui.small_button("Dismiss").clicked() {
                    dismiss = true;
                }
            });
        });
        if let Some((feature, lost, replacement)) = substitute {
            self.substitute_reference(feature, &lost, &replacement);
        }
        if dismiss {
            self.lost_references.clear();
        }
    }

    /// Replace a lost reference with another one, in the feature that used it,
    /// and rebuild. Returns false when there is nothing to replace.
    pub fn substitute_reference(&mut self, feature_id: Uuid, lost: &str, replacement: &str) -> bool {
        let snapshot = self.project.clone();
        {
            let Some(part) = self.get_active_part_mut() else {
                return false;
            };
            let Some(target) = part
                .features
                .iter_mut()
                .find(|candidate| candidate.id == feature_id)
            else {
                return false;
            };
            match &mut target.kind {
                FeatureKind::Fillet { edges, .. } | FeatureKind::Chamfer { edges, .. } => {
                    let EdgeSet::References(references) = edges else {
                        return false;
                    };
                    let Some(slot) = references.iter_mut().find(|reference| *reference == lost) else {
                        return false;
                    };
                    *slot = replacement.to_string();
                }
                FeatureKind::Sketch { sketch } => {
                    let Some(reference) = sketch.references.iter_mut().find(|reference| {
                        reference.reference.as_deref() == Some(lost)
                    }) else {
                        return false;
                    };
                    reference.reference = Some(replacement.to_string());
                    reference.label = replacement.to_string();
                }
                FeatureKind::Extrude {
                    placement: Some(placement),
                    ..
                }
                | FeatureKind::Revolve {
                    placement: Some(placement),
                    ..
                } if placement.reference.as_deref() == Some(lost) => {
                    placement.reference = Some(replacement.to_string());
                }
                _ => return false,
            }
        }
        self.undo_stack.push(snapshot);
        self.lost_references
            .retain(|entry| !(entry.feature == feature_id && entry.reference == lost));
        self.status = Some(format!("{lost} substituted with {replacement}."));
        self.rebuild_active_part();
        true
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
                // The size of the next round or break, then the sets.
                ui.label("Size");
                ui.add(
                    egui::DragValue::new(&mut self.edge_feature_size)
                        .speed(0.1)
                        .range(0.05..=100.0)
                        .suffix(" mm"),
                );
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
            let size = self.edge_feature_size;
            self.add_edge_feature(kind, size, edges);
        }
    }

    /// The stable reference of the plane the sketch is placed on, when the
    /// selection names one: a model face, or a datum plane feature.
    fn selection_plane_reference(&self) -> Option<String> {
        self.selection.items().iter().find_map(|item| match item {
            Selected::Face { reference, .. } => Some(reference.clone()),
            Selected::Datum { feature, .. } | Selected::Feature { feature, .. } => {
                Some(feature.to_string())
            }
            Selected::Edge { .. } => None,
        })
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

    /// The edge set the toolbar buttons use: the edges that are selected, or
    /// every edge when none is.
    fn selection_edge_set(&self) -> EdgeSet {
        let references = self.selection.edge_references();
        if references.is_empty() {
            EdgeSet::All
        } else {
            EdgeSet::References(references.iter().map(|r| r.to_string()).collect())
        }
    }

    /// Add a round or a break driven by an edge set.
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
        // The sketch keeps its own history: a Ctrl+Z while sketching steps
        // back through the drawing, and only an empty sketch history falls
        // through to the model.
        if self.sketch.is_some() {
            if let Some(previous) = self.sketch_undo.pop() {
                if let Some(current) = self.sketch.replace(previous) {
                    self.sketch_redo.push(current);
                }
                self.dimension_editor = None;
                self.selected_dimension = None;
                self.last_error = None;
                self.status = Some("Sketch edit undone.".to_string());
                return;
            }
        }
        if self.undo_stack.undo(&mut self.project) {
            self.rebuild_active_part();
            self.last_error = None;
            self.status = Some("Undone.".to_string());
        }
    }
    
    pub fn redo(&mut self) {
        if self.sketch.is_some() {
            if let Some(next) = self.sketch_redo.pop() {
                if let Some(current) = self.sketch.replace(next) {
                    self.sketch_undo.push(current);
                }
                self.dimension_editor = None;
                self.selected_dimension = None;
                self.last_error = None;
                self.status = Some("Sketch edit redone.".to_string());
                return;
            }
        }
        if self.undo_stack.redo(&mut self.project) {
            self.rebuild_active_part();
            self.last_error = None;
            self.status = Some("Redone.".to_string());
        }
    }
    
    pub fn delete_selected(&mut self) {
        if self.selected_dimension.is_some() {
            self.delete_selected_dimension();
            return;
        }
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

        // The orientation reference is what the plane came from; the second one
        // is automatic: the coordinate system at the origin (or the plane's own
        // normal) fixes the remaining direction.
        let (orientation_label, orientation_reference) = self
            .selection
            .items()
            .iter()
            .find_map(|item| match item {
                Selected::Datum { feature, label } => Some((label.clone(), Some(feature.to_string()))),
                Selected::Face {
                    reference, label, ..
                } => Some((label.clone(), Some(reference.clone()))),
                _ => None,
            })
            .unwrap_or_else(|| ("Default plane".to_string(), None));

        match self.selection_plane() {
            Some((origin, normal, x_direction)) => {
                sketch.set_sketch_plane(origin, normal, x_direction);
                sketch.set_references(
                    SketchReference {
                        reference: orientation_reference,
                        label: orientation_label,
                        role: SketchReferenceRole::Orientation,
                        origin,
                        direction: x_direction,
                    },
                    SketchReference {
                        reference: None,
                        label: "CYS (origin)".to_string(),
                        role: SketchReferenceRole::Automatic,
                        origin: [0.0; 3],
                        direction: normal,
                    },
                );
                self.sketch_plane = Some((origin, normal, x_direction));
                self.sketch_plane_reference = self.selection_plane_reference();
                // A sketch is drawn face-on. From the default orbit direction
                // most planes are edge-on, a screen ray never meets the sketch
                // and drawing would silently do nothing.
                self.camera.look_from(glam::Vec3::new(
                    normal[0] as f32,
                    normal[1] as f32,
                    normal[2] as f32,
                ));
                // The sketch view is deliberate: the automatic frame of the
                // first rebuild must not take it away.
                self.needs_fit = false;
                self.status = Some("Sketch placed on the selected reference.".to_string());
            }
            None => {
                // No plane in the selection: ask for one and wait for it. The
                // grid is not drawn while the sketch has no plane of its own.
                sketch.in_placement_mode = true;
                sketch.set_references(
                    SketchReference {
                        reference: None,
                        label: orientation_label,
                        role: SketchReferenceRole::Orientation,
                        origin: [0.0; 3],
                        direction: [1.0, 0.0, 0.0],
                    },
                    SketchReference {
                        reference: None,
                        label: "CYS (origin)".to_string(),
                        role: SketchReferenceRole::Automatic,
                        origin: [0.0; 3],
                        direction: [0.0, 0.0, 1.0],
                    },
                );
                self.sketch_plane = Some(([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
                self.sketch_plane_reference = None;
                self.status = Some(
                    "Select a planar face or datum plane, or start the sketch again on the default plane."
                        .to_string(),
                );
            }
        }

        // A fresh sketch starts with no tool, no selection, no history and no
        // open editor. It also starts a new session: OK will add a feature.
        self.sketch_draft.begin(SketchTool::Select);
        self.sketch_selection.clear();
        self.sketch_hover = None;
        self.sketch_undo.clear();
        self.sketch_redo.clear();
        self.dimension_editor = None;
        self.selected_dimension = None;
        self.dimension_menu = None;
        self.dragging_point = None;
        self.sketch_feature = None;
        self.dimension_picks.clear();
        self.dimension_orientation = DimensionOrientation::default();
        self.sketch = Some(sketch);
    }

    /// Open an existing Sketch feature for editing.
    pub fn edit_sketch_feature(&mut self, feature: Uuid) -> Result<(), String> {
        if self.sketch.is_some() {
            return Err("Finish (OK) or cancel the sketch that is open first.".to_string());
        }
        let part = self
            .get_active_part()
            .ok_or_else(|| "No active part.".to_string())?;
        let candidate = part
            .features
            .iter()
            .find(|candidate| candidate.id == feature)
            .ok_or_else(|| "That feature no longer exists.".to_string())?;
        let FeatureKind::Sketch { sketch } = &candidate.kind else {
            return Err("That feature is not a sketch.".to_string());
        };
        let sketch = sketch.clone();
        let name = candidate.name.clone();
        let (origin, normal, x_direction) = (
            sketch.plane_origin,
            sketch.plane_normal,
            sketch.plane_x_axis,
        );
        let mut camera = self.camera.clone();
        camera.look_from(glam::Vec3::new(
            normal[0] as f32,
            normal[1] as f32,
            normal[2] as f32,
        ));
        self.camera = camera;
        self.needs_fit = false;
        self.sketch_plane = Some((origin, normal, x_direction));
        self.sketch_plane_reference = sketch
            .reference(SketchReferenceRole::Orientation)
            .and_then(|reference| reference.reference.clone());
        self.sketch_draft.begin(SketchTool::Select);
        self.sketch_selection.clear();
        self.sketch_hover = None;
        self.sketch_undo.clear();
        self.sketch_redo.clear();
        self.dimension_editor = None;
        self.selected_dimension = None;
        self.dimension_menu = None;
        self.dragging_point = None;
        self.dimension_picks.clear();
        self.dimension_orientation = DimensionOrientation::default();
        self.sketch_feature = Some(feature);
        self.sketch = Some(sketch);
        self.last_error = None;
        self.status = Some(format!("Editing {name}. OK keeps the changes, Cancel drops them."));
        Ok(())
    }

    /// OK: keep the sketch. An empty sketch is not added to the history.
    pub fn confirm_sketch(&mut self) {
        let Some(sketch) = self.sketch.clone() else {
            return;
        };
        if !sketch_has_geometry(&sketch) {
            self.status =
                Some("The sketch is empty: nothing was added to the history.".to_string());
            self.close_sketch_session();
            return;
        }
        self.undo_stack.push(self.project.clone());
        let editing = self.sketch_feature;
        let mut feature_name = None;
        if let Some(part) = self.get_active_part_mut() {
            let updated = editing.and_then(|id| {
                part.features
                    .iter_mut()
                    .find(|feature| feature.id == id)
                    .map(|feature| {
                        feature.kind = FeatureKind::Sketch {
                            sketch: sketch.clone(),
                        };
                        feature.name.clone()
                    })
            });
            feature_name = match updated {
                Some(name) => Some(name),
                None => {
                    let name = rcad_features::unique_feature_name(part, "Sketch");
                    part.features.push(Feature::new(
                        name.clone(),
                        FeatureKind::Sketch {
                            sketch: sketch.clone(),
                        },
                    ));
                    Some(name)
                }
            };
            // The extrude and revolve built from this sketch keep a copy of its
            // profile and plane: bring them up to date now, so the document says
            // what the rebuild will compute.
            if let Some(source) = editing {
                let fresh = sketch.to_profile();
                let placement = SketchPlacement {
                    origin: sketch.plane_origin,
                    normal: sketch.plane_normal,
                    x_direction: sketch.plane_x_axis,
                    reference: sketch
                        .reference(SketchReferenceRole::Orientation)
                        .and_then(|reference| reference.reference.clone()),
                };
                for feature in &mut part.features {
                    let (sketch_source, profile, feature_placement) = match &mut feature.kind {
                        FeatureKind::Extrude {
                            sketch,
                            profile,
                            placement,
                            ..
                        }
                        | FeatureKind::Revolve {
                            sketch,
                            profile,
                            placement,
                            ..
                        } => (sketch, profile, placement),
                        _ => continue,
                    };
                    if *sketch_source != Some(source) {
                        continue;
                    }
                    if let Some(fresh) = &fresh {
                        *profile = fresh.clone();
                    }
                    *feature_placement = Some(placement.clone());
                }
            }
        }
        self.rebuild_active_part();
        self.last_error = None;
        self.status = Some(match feature_name {
            Some(name) => format!("{name} saved in the history."),
            None => "Sketch saved.".to_string(),
        });
        self.close_sketch_session();
    }

    /// Cancel: leave the sketch without keeping anything.
    pub fn cancel_sketch(&mut self) {
        self.status = Some("Sketch cancelled; nothing was saved.".to_string());
        self.last_error = None;
        self.close_sketch_session();
    }

    /// Leave the sketch edit session and drop its transient state.
    fn close_sketch_session(&mut self) {
        self.sketch = None;
        self.sketch_plane = None;
        self.sketch_plane_reference = None;
        self.sketch_feature = None;
        self.sketch_draft.begin(SketchTool::Select);
        self.sketch_selection.clear();
        self.sketch_hover = None;
        self.sketch_undo.clear();
        self.sketch_redo.clear();
        self.dimension_editor = None;
        self.selected_dimension = None;
        self.dimension_menu = None;
        self.dragging_point = None;
        self.dimension_picks.clear();
        self.sketch_text_editor = None;
    }

    /// The sketch to sweep with an extrude or a revolve: the open one, else the
    /// selected Sketch feature, else the last one in the tree.
    fn sketch_for_sweep(&self) -> Option<(Sketch, Option<SketchPlacement>, Option<Uuid>)> {
        if let Some(sketch) = &self.sketch {
            let placement = self
                .sketch_plane
                .map(|(origin, normal, x_direction)| SketchPlacement {
                    origin,
                    normal,
                    x_direction,
                    reference: self.sketch_plane_reference.clone(),
                });
            return Some((sketch.clone(), placement, self.sketch_feature));
        }
        let part = self.get_active_part()?;
        let selected = self.selection.items().iter().find_map(|item| match item {
            Selected::Feature { feature, .. } => Some(*feature),
            _ => None,
        });
        let is_sketch = |feature: &&Feature| matches!(feature.kind, FeatureKind::Sketch { .. });
        let chosen = selected
            .and_then(|id| part.features.iter().find(|feature| feature.id == id && is_sketch(&feature)))
            .or_else(|| part.features.iter().rev().find(is_sketch))?;
        let FeatureKind::Sketch { sketch } = &chosen.kind else {
            return None;
        };
        let reference = sketch
            .reference(SketchReferenceRole::Orientation)
            .and_then(|reference| reference.reference.clone());
        Some((
            sketch.clone(),
            Some(SketchPlacement {
                origin: sketch.plane_origin,
                normal: sketch.plane_normal,
                x_direction: sketch.plane_x_axis,
                reference,
            }),
            Some(chosen.id),
        ))
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
            // A face of the model is a sketch plane in its own right: its
            // centre and normal are all the sketch needs.
            Selected::Face { triangle, .. } => return face_plane(&self.mesh, *triangle),
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
    
    /// Draw a rectangle straight away, with the corners given. Used by the
    /// tests that need a profile; the interface draws through `sketch_click`.
    #[cfg(test)]
    fn add_sketch_rectangle(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        if let Some(sketch) = &mut self.sketch {
            sketch.add_rectangle(x0, y0, x1, y1);
        }
    }

    /// Choose the tool the next viewport click uses.
    pub fn set_sketch_tool(&mut self, tool: SketchTool) {
        if self.sketch.is_none() {
            self.last_error = Some("Start a sketch (K) before drawing in one.".to_string());
            return;
        }
        self.sketch_draft.begin(tool);
        self.status = Some(format!("{}: {}", tool.label(), tool.hint()));
    }

    /// Escape: drop the half-finished shape, then the tool, then the selection.
    ///
    /// Creo steps back one level at a time; clearing the model selection while
    /// a rectangle was half drawn would lose the shape without saying so.
    pub fn escape(&mut self) {
        // Close what is open first: the editor or the menu is what the user is
        // looking at, and cancelling the sketch tool behind it would surprise.
        if self.sketch_text_editor.is_some() {
            self.cancel_sketch_text();
            return;
        }
        if self.show_import_dialog || self.show_constraint_table {
            self.show_import_dialog = false;
            self.show_constraint_table = false;
            return;
        }
        if self.dimension_editor.is_some()
            || self.dimension_menu.is_some()
            || self.selected_dimension.is_some()
        {
            self.dimension_editor = None;
            self.dimension_menu = None;
            self.selected_dimension = None;
            return;
        }
        if self.dragging_point.take().is_some() {
            return;
        }
        if !self.dimension_picks.is_empty() {
            self.dimension_picks.clear();
            self.status = Some("Dimension pick cancelled.".to_string());
            return;
        }
        if self.sketch_draft.cancel() {
            let tool = self.sketch_draft.tool();
            self.status = Some(format!("{}: {}", tool.label(), tool.hint()));
            return;
        }
        self.deselect();
    }

    /// Apply a constraint action to the current sketch selection, then solve.
    ///
    /// A refused action gives the selection back, so the user can add what was
    /// missing instead of selecting everything again.
    pub fn apply_sketch_constraint(&mut self, action: ConstraintAction) {
        let Some(sketch) = self.sketch.as_mut() else {
            self.last_error = Some("No sketch is open.".to_string());
            return;
        };
        let selection = std::mem::take(&mut self.sketch_selection);
        match sketch_ui::apply_constraint(sketch, &selection, action) {
            Ok(message) => {
                self.last_error = None;
                self.status = Some(message);
                if let Err(error) = sketch.solve() {
                    self.last_error = Some(format!("Constraints: {error}"));
                }
            }
            Err(message) => {
                self.last_error = Some(message);
                self.sketch_selection = selection;
            }
        }
    }

    /// The cursor in sketch coordinates, snapped to the grid when the snap is
    /// on. None without a sketch plane, or when the view is edge-on to it.
    pub fn sketch_cursor(&self, rect: egui::Rect, pos: egui::Pos2) -> Option<[f64; 2]> {
        let plane = self.sketch_plane?;
        let raw = sketch_ui::screen_to_sketch(pos, rect, &self.camera, plane)?;
        // The polyline snaps to the points it already has, so the user can
        // close the loop exactly on the first one.
        if self.sketch_draft.tool() == SketchTool::LineChain {
            for point in self.sketch_draft.pending_points() {
                if let Some(screen) =
                    sketch_ui::sketch_to_screen(rect, &self.camera, plane, *point)
                {
                    if screen.distance(pos) <= sketch_ui::CHAIN_SNAP_RADIUS {
                        return Some(*point);
                    }
                }
            }
        }
        Some(if self.settings.sketch_snap {
            sketch_ui::snap_to_grid(raw, GRID_SPACING)
        } else {
            raw
        })
    }

    /// One click inside the viewport while a sketch is open.
    ///
    /// Returns true when the sketch consumed the click, so the model picker
    /// stays out of the way. `mode` is the Ctrl/Shift combination of the click.
    pub fn sketch_click(&mut self, rect: egui::Rect, pos: egui::Pos2, mode: ClickMode) -> bool {
        let Some(plane) = self.sketch_plane else {
            return false;
        };
        let Some(cursor) = self.sketch_cursor(rect, pos) else {
            return false;
        };
        // Take the sketch out while the handlers run: the click also touches
        // the selection, the status and the solver, and holding a borrow of
        // one field while writing another is needless trouble.
        let Some(mut sketch) = self.sketch.take() else {
            return false;
        };
        if sketch.in_placement_mode {
            self.sketch = Some(sketch);
            return false;
        }

        let tool = self.sketch_draft.tool();
        let mut consumed = false;
        // Set when the text tool wants its editor opened after the borrow ends.
        let mut text_at: Option<[f64; 2]> = None;

        // A click on a dimension belongs to the dimension, whatever tool is
        // active: dimensions are thin and would otherwise be hard to reach.
        if let Some(index) =
            sketch_ui::dimension_hit_test(&sketch, rect, &self.camera, plane, pos, HIT_RADIUS)
        {
            self.selected_dimension = Some(index);
            self.sketch_selection.clear();
            self.dimension_menu = None;
            self.status = Some("Dimension selected: double-click to edit it.".to_string());
            self.sketch = Some(sketch);
            return true;
        }

        match tool {
            // The select tool picks a sketch entity. A click on empty space is
            // left to the model selection.
            SketchTool::Select => {
                if let Some(hit) =
                    sketch_ui::hit_test(&sketch, rect, &self.camera, plane, pos, HIT_RADIUS)
                {
                    if mode.toggle && self.sketch_selection.contains(&hit.id) {
                        self.sketch_selection.retain(|id| *id != hit.id);
                    } else {
                        if !mode.toggle && !mode.extend {
                            self.sketch_selection.clear();
                        }
                        if !self.sketch_selection.contains(&hit.id) {
                            self.sketch_selection.push(hit.id);
                        }
                    }
                    self.sketch_hover = Some(hit.id);
                    self.selected_dimension = None;
                    let label = sketch_ui::entity_label(&sketch, hit.id);
                    self.status = Some(format!(
                        "{label} selected ({} in the sketch selection).",
                        self.sketch_selection.len()
                    ));
                    consumed = true;
                }
            }
            // The dynamic dimension tool: pick one or two things, then click
            // empty space to place the text there.
            SketchTool::Dimension => {
                let hit = sketch_ui::hit_test(&sketch, rect, &self.camera, plane, pos, HIT_RADIUS);
                match hit {
                    Some(hit) => {
                        self.dimension_picks.push((hit.id, hit.kind));
                        self.selected_dimension = None;
                        self.status = Some(if self.dimension_picks.len() == 1 {
                            "Click a second entity for an angle or a distance, or click empty space to place the dimension.".to_string()
                        } else {
                            "Click empty space to place the dimension.".to_string()
                        });
                    }
                    None => {
                        if self.dimension_picks.is_empty() {
                            self.status =
                                Some("Click a line or a point to dimension.".to_string());
                        } else {
                            let picks = std::mem::take(&mut self.dimension_picks);
                            let before = sketch.clone();
                            match place_dimension(
                                &mut sketch,
                                &picks,
                                cursor,
                                self.dimension_orientation,
                                self.dimension_size,
                            ) {
                                Ok(message) => {
                                    self.push_sketch_snapshot(before);
                                    self.last_error = None;
                                    self.status = Some(message);
                                    let _ = sketch.solve();
                                }
                                Err(message) => {
                                    self.dimension_picks = picks;
                                    self.last_error = Some(message);
                                }
                            }
                        }
                    }
                }
                consumed = true;
            }
            // The tangent arc leaves a line end along the line itself: two
            // clicks, a line and then the end of the arc.
            SketchTool::ArcTangent => {
                if let Some(span) = self.sketch_draft.tangent_arc().cloned() {
                    let before = sketch.clone();
                    match sketch_ui::tangent_arc(span.start, span.direction, cursor) {
                        Some((centre, radius, start_angle, end_angle)) => {
                            let arc = sketch.add_arc(
                                centre[0],
                                centre[1],
                                radius,
                                start_angle,
                                end_angle,
                            );
                            // The arc starts on the line end it was built from.
                            if let Some(arc) =
                                sketch.arcs.iter().find(|candidate| candidate.id == arc)
                            {
                                let start_point = arc.start_point;
                                sketch.constraints.push(Constraint::Coincident {
                                    a: start_point,
                                    b: span.point,
                                });
                            }
                            self.push_sketch_snapshot(before);
                            if let Err(error) = sketch.solve() {
                                self.last_error = Some(format!("Constraints: {error}"));
                            }
                            self.status = Some(format!(
                                "Tangent arc drawn from the {} end.",
                                sketch_ui::entity_label(&sketch, span.line)
                            ));
                            self.sketch_draft.begin(SketchTool::ArcTangent);
                        }
                        None => {
                            self.last_error =
                                Some("Click further from the line: the arc has no size.".to_string())
                        }
                    }
                } else {
                    let hit =
                        sketch_ui::hit_test(&sketch, rect, &self.camera, plane, pos, HIT_RADIUS);
                    // A click on the line or on one of its ends both pick it: the
                    // end is what the arc will start from.
                    let picked = match hit {
                        Some(hit) if hit.kind == sketch_ui::SketchEntityKind::Line => sketch
                            .lines
                            .iter()
                            .find(|candidate| candidate.id == hit.id)
                            .cloned(),
                        Some(hit) if hit.kind == sketch_ui::SketchEntityKind::Point => sketch
                            .lines
                            .iter()
                            .find(|candidate| candidate.a == hit.id || candidate.b == hit.id)
                            .cloned(),
                        _ => None,
                    };
                    match picked {
                        Some(line) => {
                            let (Some(a), Some(b)) = (
                                sketch_ui::point_of(&sketch, line.a),
                                sketch_ui::point_of(&sketch, line.b),
                            ) else {
                                self.last_error = Some("That line has no ends.".to_string());
                                self.sketch = Some(sketch);
                                return true;
                            };
                            let near_a = (cursor[0] - a[0]).hypot(cursor[1] - a[1])
                                <= (cursor[0] - b[0]).hypot(cursor[1] - b[1]);
                            let (point_id, start, other) = if near_a {
                                (line.a, a, b)
                            } else {
                                (line.b, b, a)
                            };
                            let direction = [start[0] - other[0], start[1] - other[1]];
                            self.sketch_draft
                                .begin_arc_tangent(line.id, point_id, start, direction);
                            self.status = Some("Now click where the arc ends.".to_string());
                        }
                        None => {
                            self.last_error =
                                Some("Click the line the arc should leave from.".to_string())
                        }
                    }
                }
                consumed = true;
            }
            // The command tools act on the entity under the cursor. Dimension
            // is handled above, so what is left here is trim, extend, offset,
            // mirror and pattern.
            command if command.picks_entities() => {
                let hit = sketch_ui::hit_test(&sketch, rect, &self.camera, plane, pos, HIT_RADIUS);
                let before = sketch.clone();
                let mut picks: Vec<Uuid> = self.sketch_draft.pending_entity().into_iter().collect();
                let had_pick = !picks.is_empty();
                match sketch_ui::command_step(
                    &mut sketch,
                    tool,
                    &mut picks,
                    &self.sketch_selection,
                    cursor,
                    hit.as_ref(),
                    &self.tool_params,
                ) {
                    Ok(message) => {
                        // The first Extend click only remembers a line; it is
                        // not an edit worth an undo step.
                        let remembers_only = tool == SketchTool::Extend && !had_pick;
                        if !remembers_only {
                            self.push_sketch_snapshot(before);
                            let _ = sketch.solve();
                        }
                        self.last_error = None;
                        self.status = Some(message);
                        self.sketch_draft.set_picks(picks);
                        self.selected_dimension = None;
                    }
                    Err(message) => {
                        self.last_error = Some(message);
                        self.sketch_draft.set_picks(picks);
                    }
                }
                consumed = true;
            }
            // The drawing tools: the click is the corner, end, centre or
            // control point the tool was waiting for.
            _ => {
                if tool == SketchTool::Conic {
                    self.sketch_draft.set_conic_rho(self.tool_params.conic_rho);
                }
                let before = sketch.clone();
                let edit = self.sketch_draft.click(&mut sketch, cursor);
                if let SketchEdit::TextAt(position) = edit {
                    text_at = Some(position);
                }
                if edit.added_geometry() {
                    self.push_sketch_snapshot(before);
                }
                self.report_sketch_edit(&edit);
                if edit.added_geometry() {
                    if let Err(error) = sketch.solve() {
                        self.last_error = Some(format!("Constraints: {error}"));
                    }
                }
                self.selected_dimension = None;
                consumed = true;
            }
        }
        self.sketch = Some(sketch);
        if let Some(position) = text_at {
            self.open_sketch_text_editor(position, pos);
        }
        consumed
    }

    /// Keep a snapshot of the sketch as it was before an edit, for Ctrl+Z.
    fn push_sketch_snapshot(&mut self, before: Sketch) {
        self.sketch_undo.push(before);
        if self.sketch_undo.len() > 64 {
            self.sketch_undo.remove(0);
        }
        self.sketch_redo.clear();
    }

    /// A copy of the sketch as it is now, or None when none is open.
    fn sketch_snapshot(&self) -> Option<Sketch> {
        self.sketch.clone()
    }

    /// Say what a drawing click did.
    fn report_sketch_edit(&mut self, edit: &SketchEdit) {
        self.last_error = None;
        match edit {
            SketchEdit::Pending => {
                self.status = Some("First point set: click again to finish.".to_string())
            }
            SketchEdit::Point(_) => self.status = Some("Point placed.".to_string()),
            SketchEdit::Line(_) => self.status = Some("Line drawn.".to_string()),
            SketchEdit::Rectangle(_) => self.status = Some("Rectangle drawn.".to_string()),
            SketchEdit::Circle(_) => self.status = Some("Circle drawn.".to_string()),
            SketchEdit::LineChain(_) => {
                self.status = Some("Segment added: click on or right-click to stop.".to_string())
            }
            SketchEdit::Closed(_) => {
                // A closed loop ends the polyline tool by itself.
                self.sketch_draft.begin(SketchTool::Select);
                self.status = Some("Polyline closed.".to_string());
            }
            SketchEdit::Polygon(_) => self.status = Some("Polygon drawn.".to_string()),
            SketchEdit::Arc(_) => self.status = Some("Arc drawn.".to_string()),
            SketchEdit::Conic(_) => self.status = Some("Conic drawn.".to_string()),
            SketchEdit::Spline(_) => self.status = Some("Spline drawn.".to_string()),
            SketchEdit::Text(_) => self.status = Some("Text added.".to_string()),
            SketchEdit::TextAt(_) => {}
            SketchEdit::TooSmall => {
                self.last_error =
                    Some("That shape has no size: click further from the first point.".to_string())
            }
            SketchEdit::Refused(reason) => self.last_error = Some(reason.to_string()),
            SketchEdit::NotDrawing => {}
        }
    }

    /// Finish the shape being drawn (Enter). Returns true when something was
    /// finished, so the caller knows not to fall back to solving.
    pub fn finish_sketch_shape(&mut self) -> bool {
        let Some(mut sketch) = self.sketch.take() else {
            return false;
        };
        let before = sketch.clone();
        let Some(edit) = self.sketch_draft.finish(&mut sketch) else {
            self.sketch = Some(sketch);
            return false;
        };
        self.push_sketch_snapshot(before);
        self.report_sketch_edit(&edit);
        if let Err(error) = sketch.solve() {
            self.last_error = Some(format!("Constraints: {error}"));
        }
        self.sketch = Some(sketch);
        true
    }

    /// Enter: finish the shape being drawn, or solve when nothing is pending.
    pub fn finish_sketch_shape_or_solve(&mut self) {
        if !self.finish_sketch_shape() {
            self.solve_sketch();
        }
    }

    /// Right-click in a sketch: leave whatever tool or pick sequence is
    /// active, keeping the geometry already created.
    pub fn cancel_sketch_operation(&mut self) {
        let cancelled = self.sketch_draft.cancel();
        let had_picks = !self.dimension_picks.is_empty();
        self.dimension_picks.clear();
        self.dimension_editor = None;
        self.dimension_menu = None;
        self.status = Some(if cancelled || had_picks {
            "Sketch tool cancelled; the geometry drawn so far stays.".to_string()
        } else {
            "Already in Select: click an entity to select it.".to_string()
        });
    }

    /// Move a sketch point to a place in sketch coordinates and re-solve.
    pub fn move_sketch_point(&mut self, point: Uuid, to: [f64; 2]) -> bool {
        let Some(mut sketch) = self.sketch.take() else {
            return false;
        };
        let moved = sketch_ui::move_point(&mut sketch, point, to);
        if moved {
            let _ = sketch.solve();
        }
        self.sketch = Some(sketch);
        moved
    }

    /// Sketch input for this frame: the snapped cursor, the entity under it and
    /// what a click did. Returns (click consumed, cursor in sketch coordinates).
    fn sketch_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
    ) -> (bool, Option<[f64; 2]>) {
        let Some(plane) = self.sketch_plane else {
            self.sketch_hover = None;
            return (false, None);
        };
        let pos = if response.hovered() {
            ui.input(|i| i.pointer.hover_pos())
        } else {
            None
        };
        let cursor = pos.and_then(|pos| self.sketch_cursor(response.rect, pos));
        let placed = self
            .sketch
            .as_ref()
            .map(|sketch| !sketch.in_placement_mode)
            .unwrap_or(false);
        if !placed {
            self.sketch_hover = None;
            return (false, cursor);
        }

        // An edge-on view has no sketch coordinates for the cursor: the ray is
        // parallel to the plane. Drawing clicks would do nothing, so say why.
        if response.hovered() && cursor.is_none() && self.sketch_draft.tool().creates_geometry() {
            self.status = Some(
                "The view is edge-on to the sketch plane: rotate the view to draw.".to_string(),
            );
        }

        // A double-click on a dimension opens its value editor; a right-click
        // opens the dimension menu. Both belong to the dimension under the
        // cursor before any tool sees the click.
        if let Some(position) = pos {
            let dimension = self.sketch.as_ref().and_then(|sketch| {
                sketch_ui::dimension_hit_test(
                    sketch,
                    response.rect,
                    &self.camera,
                    plane,
                    position,
                    HIT_RADIUS,
                )
            });
            if let Some(index) = dimension {
                if ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) {
                    self.open_dimension_editor(index, position);
                    return (true, cursor);
                }
                if ui.input(|i| i.pointer.secondary_clicked()) {
                    self.selected_dimension = Some(index);
                    self.dimension_menu = Some((index, position));
                    self.dimension_editor = None;
                    return (true, cursor);
                }
            }
        }

        // While the value field is open a click only takes the focus away from
        // it (which commits the value); it must not also draw or pick behind it.
        if (self.dimension_editor.is_some() || self.sketch_text_editor.is_some())
            && ui.input(|i| i.pointer.any_click())
        {
            return (true, cursor);
        }

        // The crosshair cursor is only for the tools that build geometry; a
        // picking tool leaves the system cursor alone.
        if response.hovered() && self.sketch_draft.tool().creates_geometry() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }

        // A right-click that was not on a dimension leaves the current tool or
        // the pending picks, keeping the geometry already drawn.
        if ui.input(|i| i.pointer.secondary_clicked()) {
            self.cancel_sketch_operation();
            return (true, cursor);
        }

        // Dragging a point moves it, so the constraints can be felt.
        if self.dragging_point.is_none()
            && response.drag_started_by(egui::PointerButton::Primary)
            && self.sketch_draft.tool() == SketchTool::Select
        {
            // The drag started where the button went down, not where the
            // pointer is now: test the press origin for the point.
            if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
                let point = self
                    .sketch
                    .as_ref()
                    .and_then(|sketch| {
                        sketch_ui::hit_test(
                            sketch,
                            response.rect,
                            &self.camera,
                            plane,
                            origin,
                            HIT_RADIUS,
                        )
                    })
                    .filter(|hit| hit.kind == sketch_ui::SketchEntityKind::Point);
                if let Some(hit) = point {
                    if let Some(before) = self.sketch_snapshot() {
                        self.push_sketch_snapshot(before);
                    }
                    // Move on the frame the drag starts too: the next frame may
                    // be the release, which is no longer a drag.
                    if let Some(cursor) = cursor {
                        self.move_sketch_point(hit.id, cursor);
                    }
                    self.dragging_point = Some(hit.id);
                    self.selected_dimension = None;
                    return (true, cursor);
                }
            }
        }
        if let Some(point) = self.dragging_point {
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(cursor) = cursor {
                    self.move_sketch_point(point, cursor);
                }
                return (true, cursor);
            }
            self.dragging_point = None;
        }

        self.sketch_hover = pos.and_then(|pos| {
            let sketch = self.sketch.as_ref()?;
            sketch_ui::hit_test(sketch, response.rect, &self.camera, plane, pos, HIT_RADIUS)
                .map(|hit| hit.id)
        });

        let mut consumed = false;
        if ui.input(|i| i.pointer.primary_clicked()) {
            if let Some(pos) = pos {
                let mode = ui.input(|i| ClickMode::from_modifiers(&i.modifiers));
                consumed = self.sketch_click(response.rect, pos, mode);
            }
        }
        (consumed, cursor)
    }

    /// Open the in-place editor for a dimension.
    ///
    /// One undo snapshot is taken here, not on every keystroke, so Ctrl+Z after
    /// an edit steps back over the whole edit instead of one digit.
    pub fn open_dimension_editor(&mut self, index: usize, at: egui::Pos2) {
        let Some(value) = self.sketch.as_ref().and_then(|sketch| sketch.dimension_value(index))
        else {
            return;
        };
        if let Some(before) = self.sketch_snapshot() {
            self.push_sketch_snapshot(before);
        }
        self.selected_dimension = Some(index);
        self.dimension_menu = None;
        self.dimension_editor = Some(DimensionEditor {
            index,
            text: sketch_ui::format_length(value),
            at,
            fresh: true,
        });
        self.status = Some("Editing dimension: type a value and press Enter.".to_string());
    }

    /// The dimension the in-place editor is showing.
    pub fn edited_dimension(&self) -> Option<usize> {
        self.dimension_editor.as_ref().map(|editor| editor.index)
    }

    /// The dimension the buttons act on: the edited one, or the selected one.
    fn active_dimension(&self) -> Option<usize> {
        self.dimension_editor
            .as_ref()
            .map(|editor| editor.index)
            .or(self.selected_dimension)
    }

    /// Apply a value to the dimension being edited and solve.
    pub fn apply_dimension_value(&mut self, value: f64) {
        let Some(index) = self.edited_dimension() else {
            return;
        };
        if let Some(editor) = self.dimension_editor.as_mut() {
            editor.text = sketch_ui::format_length(value);
        }
        let Some(sketch) = self.sketch.as_mut() else {
            return;
        };
        if sketch.set_dimension_value(index, value) {
            let _ = sketch.solve();
            self.last_error = None;
            self.status = Some(format!(
                "Dimension set to {}.",
                sketch_ui::format_length(value)
            ));
        }
    }

    /// Toggle a dimension between driving the geometry and reporting it.
    pub fn toggle_selected_dimension_driving(&mut self) {
        let Some(index) = self.active_dimension() else {
            return;
        };
        let Some(driving) =
            self.sketch.as_ref().and_then(|sketch| sketch.dimension_is_driving(index))
        else {
            return;
        };
        let Some(before) = self.sketch_snapshot() else {
            return;
        };
        self.push_sketch_snapshot(before);
        let Some(sketch) = self.sketch.as_mut() else {
            return;
        };
        if sketch.set_dimension_driving(index, !driving) {
            let _ = sketch.solve();
            self.last_error = None;
            self.status = Some(if driving {
                "Dimension is now driven (a reference).".to_string()
            } else {
                "Dimension is now driving.".to_string()
            });
        }
    }

    /// Promote the weak dimension (Intent Manager) to a strong one.
    pub fn promote_selected_dimension(&mut self) {
        let Some(index) = self.active_dimension() else {
            return;
        };
        let weak = self
            .sketch
            .as_ref()
            .map(|sketch| sketch.dimension_is_weak(index))
            .unwrap_or(false);
        if !weak {
            self.status = Some("That dimension is already strong.".to_string());
            return;
        }
        let Some(before) = self.sketch_snapshot() else {
            return;
        };
        self.push_sketch_snapshot(before);
        let Some(sketch) = self.sketch.as_mut() else {
            return;
        };
        if sketch.promote_dimension(index) {
            self.last_error = None;
            self.status = Some("Weak dimension promoted to strong.".to_string());
        }
    }

    /// Delete the selected or edited dimension.
    pub fn delete_selected_dimension(&mut self) {
        let Some(index) = self.active_dimension() else {
            self.last_error = Some("Select a dimension first.".to_string());
            return;
        };
        let Some(before) = self.sketch_snapshot() else {
            return;
        };
        self.push_sketch_snapshot(before);
        let Some(sketch) = self.sketch.as_mut() else {
            return;
        };
        if sketch.remove_constraint(index) {
            let _ = sketch.solve();
            self.dimension_editor = None;
            self.dimension_menu = None;
            self.selected_dimension = None;
            self.last_error = None;
            self.status = Some("Dimension deleted.".to_string());
        }
    }

    /// Project the selected model edges and face boundaries into the open
    /// sketch as reference geometry.
    pub fn project_references_into_sketch(&mut self) -> Result<(), String> {
        if self.sketch.is_none() {
            return Err("Start a sketch before projecting references.".to_string());
        }
        let mut pieces: Vec<(String, Vec<[f64; 3]>)> = Vec::new();
        for item in self.selection.items() {
            match item {
                Selected::Edge { reference, label } => {
                    if let Some(edge) = self.mesh.edges.iter().find(|edge| &edge.name == reference) {
                        if edge.points.len() >= 2 {
                            let points = edge
                                .points
                                .iter()
                                .map(|point| {
                                    [point[0] as f64, point[1] as f64, point[2] as f64]
                                })
                                .collect();
                            pieces.push((label.clone(), points));
                        } else if edge.start != edge.end {
                            // A reference without a sampled polyline still has
                            // its two ends.
                            pieces.push((
                                label.clone(),
                                vec![
                                    [
                                        edge.start[0] as f64,
                                        edge.start[1] as f64,
                                        edge.start[2] as f64,
                                    ],
                                    [
                                        edge.end[0] as f64,
                                        edge.end[1] as f64,
                                        edge.end[2] as f64,
                                    ],
                                ],
                            ));
                        }
                    }
                }
                Selected::Face {
                    reference, label, ..
                } => {
                    for (index, segment) in face_boundary_segments(&self.mesh, reference)
                        .into_iter()
                        .enumerate()
                    {
                        pieces.push((format!("{label} boundary {}", index + 1), segment.to_vec()));
                    }
                }
                _ => {}
            }
        }
        if pieces.is_empty() {
            return Err(
                "Select edges or faces of the model to project into the sketch.".to_string()
            );
        }
        let count = pieces.len();
        let Some(before) = self.sketch_snapshot() else {
            return Err("Start a sketch before projecting references.".to_string());
        };
        self.push_sketch_snapshot(before);
        let Some(sketch) = self.sketch.as_mut() else {
            return Err("Start a sketch before projecting references.".to_string());
        };
        for (name, points) in pieces {
            sketch.add_reference_geometry(name, ReferenceGeometryKind::Edge, &points);
        }
        self.last_error = None;
        self.status = Some(format!(
            "{count} reference piece(s) projected into the sketch."
        ));
        Ok(())
    }

    /// The right-click menu of a dimension.
    fn draw_dimension_menu(&mut self, ctx: &egui::Context) {
        let Some((index, at)) = self.dimension_menu else {
            self.dimension_menu_was_shown = false;
            return;
        };
        let Some(sketch) = self.sketch.as_ref() else {
            self.dimension_menu = None;
            return;
        };
        let driving = sketch.dimension_is_driving(index).unwrap_or(true);
        let weak = sketch.dimension_is_weak(index);

        let mut edit = false;
        let mut toggle = false;
        let mut promote = false;
        let mut delete = false;
        egui::Area::new(egui::Id::new("rcad_dimension_menu"))
            .fixed_pos(at + egui::vec2(6.0, 6.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.label(egui::RichText::new("Dimension").strong());
                    if ui.button("Edit value…").clicked() {
                        edit = true;
                    }
                    if ui
                        .button(if driving {
                            "Make driven (reference)"
                        } else {
                            "Make driving"
                        })
                        .clicked()
                    {
                        toggle = true;
                    }
                    if weak && ui.button("Promote to strong").clicked() {
                        promote = true;
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        delete = true;
                    }
                });
            });

        // The click that opened the menu must not close it in the same frame.
        let shown_before = std::mem::replace(&mut self.dimension_menu_was_shown, true);
        let clicked_outside = shown_before
            && !edit
            && !toggle
            && !promote
            && !delete
            && ctx.input(|input| input.pointer.any_click());
        if edit {
            self.selected_dimension = Some(index);
            self.open_dimension_editor(index, at);
        }
        if toggle {
            self.selected_dimension = Some(index);
            self.toggle_selected_dimension_driving();
        }
        if promote {
            self.selected_dimension = Some(index);
            self.promote_selected_dimension();
        }
        if delete {
            self.selected_dimension = Some(index);
            self.delete_selected_dimension();
        }
        if clicked_outside {
            self.dimension_menu = None;
        }
    }

    /// The in-place value editor of a dimension: a field over the number
    /// itself, with no window around it.
    ///
    /// Typing Enter or clicking anywhere else commits the value; Escape drops
    /// it. The dimension keeps its last value until then.
    fn draw_dimension_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.dimension_editor.as_ref() else {
            return;
        };
        let index = editor.index;
        let fallback = editor.at;
        let at = self
            .sketch
            .as_ref()
            .and_then(|sketch| {
                let rect = self.viewport_rect?;
                let plane = self.sketch_plane?;
                let dimension = sketch_ui::dimensions(sketch)
                    .into_iter()
                    .find(|dimension| dimension.constraint_index == index)?;
                sketch_ui::dimension_screen_geometry(rect, &self.camera, plane, &dimension)
                    .map(|screen| screen.text)
            })
            .unwrap_or(fallback);

        let mut text = self
            .dimension_editor
            .as_ref()
            .map(|editor| editor.text.clone())
            .unwrap_or_default();
        let mut fresh = self
            .dimension_editor
            .as_ref()
            .map(|editor| editor.fresh)
            .unwrap_or(false);
        let mut commit = false;
        let mut cancel = false;
        egui::Area::new(egui::Id::new("rcad_dimension_editor"))
            .fixed_pos(at + egui::vec2(-30.0, -12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(54.0)
                        .horizontal_align(egui::Align::Center)
                        .font(egui::FontId::proportional(12.0)),
                );
                if fresh {
                    // Typing replaces the value, the way Creo edits a
                    // dimension in place.
                    response.request_focus();
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                        let end = egui::text::CCursor::new(text.chars().count());
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            end,
                        )));
                        state.store(ui.ctx(), response.id);
                    }
                    fresh = false;
                }
                if response.lost_focus() {
                    commit = true;
                }
                ui.input(|input| {
                    if input.key_pressed(egui::Key::Escape) {
                        cancel = true;
                    }
                });
            });

        if let Some(editor) = self.dimension_editor.as_mut() {
            editor.text = text.clone();
            editor.fresh = fresh;
        }
        if cancel {
            self.dimension_editor = None;
            self.status = Some("Dimension edit cancelled.".to_string());
            return;
        }
        if commit {
            let parsed = text.trim().replace(',', ".").parse::<f64>();
            match parsed {
                Ok(value) if value > 0.0 => {
                    self.apply_dimension_value(value);
                    self.dimension_editor = None;
                }
                _ => {
                    self.last_error =
                        Some("A dimension needs a positive number.".to_string());
                    self.dimension_editor = None;
                }
            }
        }
    }

    /// The sketch strip, above the status bar: what the sketch is, how
    /// constrained it is, which tool is active and how to leave it.
    ///
    /// Shown only while a sketch is open, so the viewport keeps its room the
    /// rest of the time.
    /// Open the in-place editor of a text note.
    pub fn open_sketch_text_editor(&mut self, position: [f64; 2], at: egui::Pos2) {
        self.sketch_text_editor = Some(SketchTextEditor {
            position,
            content: String::new(),
            at,
            fresh: true,
        });
        self.status = Some("Text: type it, then press Enter.".to_string());
    }

    /// Add the typed text to the sketch and close the editor.
    pub fn commit_sketch_text(&mut self) {
        let Some(editor) = self.sketch_text_editor.take() else {
            return;
        };
        let content = editor.content.trim().to_string();
        if content.is_empty() {
            self.status = Some("Empty text: nothing was added.".to_string());
            return;
        }
        let height = self.tool_params.text_height;
        let Some(mut sketch) = self.sketch.take() else {
            return;
        };
        let before = sketch.clone();
        self.push_sketch_snapshot(before);
        let edit = self
            .sketch_draft
            .finish_text(&mut sketch, editor.position, &content, height);
        self.sketch = Some(sketch);
        self.report_sketch_edit(&edit);
    }

    /// Drop the text being typed.
    pub fn cancel_sketch_text(&mut self) {
        if self.sketch_text_editor.take().is_some() {
            self.status = Some("Text cancelled.".to_string());
        }
    }

    /// The in-place text editor: a field floating where the click landed.
    fn draw_sketch_text_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.sketch_text_editor.as_ref() else {
            return;
        };
        let at = editor.at;
        let mut content = editor.content.clone();
        let mut fresh = editor.fresh;
        let mut commit = false;
        let mut cancel = false;
        egui::Area::new(egui::Id::new("rcad_sketch_text_editor"))
            .fixed_pos(at + egui::vec2(8.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut content)
                        .desired_width(170.0)
                        .hint_text("text")
                        .font(egui::FontId::proportional(13.0)),
                );
                if fresh {
                    response.request_focus();
                    fresh = false;
                }
                if response.lost_focus() {
                    commit = true;
                }
                ui.input(|input| {
                    if input.key_pressed(egui::Key::Escape) {
                        cancel = true;
                    }
                });
            });
        if let Some(editor) = self.sketch_text_editor.as_mut() {
            editor.content = content;
            editor.fresh = fresh;
        }
        if cancel {
            self.cancel_sketch_text();
        } else if commit {
            self.commit_sketch_text();
        }
    }

    /// Import a DXF or an SVG file into the open sketch.
    pub fn import_sketch_file(&mut self, path: &Path) -> Result<String, String> {
        if self.sketch.is_none() {
            return Err("Start a sketch before importing geometry into it.".to_string());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_string();
        let mut sketch = self.sketch.take().expect("checked above");
        let before = sketch.clone();
        let result = sketch.import_sketch(&content, &extension);
        match result {
            Ok(added) => {
                self.push_sketch_snapshot(before);
                let _ = sketch.solve();
                self.sketch = Some(sketch);
                let message = format!(
                    "{added} entities imported from {}.",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("the file")
                );
                self.last_error = None;
                self.status = Some(message.clone());
                Ok(message)
            }
            Err(error) => {
                self.sketch = Some(sketch);
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    /// The import dialog, with a path field.
    fn draw_import_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_import_dialog {
            return;
        }
        let mut open = self.show_import_dialog;
        let mut path = self.import_path.clone();
        let mut import = false;
        egui::Window::new("Import sketch geometry")
            .id(egui::Id::new("rcad_import_dialog"))
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("DXF or SVG file path:");
                ui.add(egui::TextEdit::singleline(&mut path).desired_width(460.0));
                ui.label(
                    "Lines, circles, arcs, polylines and paths are imported. Text and dimensions are skipped.",
                );
                if ui.button("Import").clicked() {
                    import = true;
                }
            });
        self.show_import_dialog = open;
        self.import_path = path;
        if import {
            let file = std::path::PathBuf::from(self.import_path.trim());
            let _ = self.import_sketch_file(&file);
            self.show_import_dialog = false;
        }
    }

    /// One row of the constraint table: what the constraint is, its value when
    /// it is a dimension, and whether that dimension drives.
    pub fn constraint_table_rows(&self) -> Vec<(String, Option<f64>, bool)> {
        let Some(sketch) = self.sketch.as_ref() else {
            return Vec::new();
        };
        sketch
            .constraints
            .iter()
            .enumerate()
            .map(|(index, constraint)| {
                (
                    sketch_ui::constraint_label(constraint),
                    sketch.dimension_value(index),
                    sketch.dimension_is_driving(index).unwrap_or(true),
                )
            })
            .collect()
    }

    /// Delete one constraint by its index in the sketch, for the table.
    pub fn delete_sketch_constraint(&mut self, index: usize) -> bool {
        let Some(mut sketch) = self.sketch.take() else {
            return false;
        };
        let before = sketch.clone();
        if !sketch.remove_constraint(index) {
            self.sketch = Some(sketch);
            return false;
        }
        self.push_sketch_snapshot(before);
        let _ = sketch.solve();
        self.sketch = Some(sketch);
        self.status = Some("Constraint deleted.".to_string());
        true
    }

    /// Give a dimension a new value, for the table.
    pub fn set_sketch_constraint_value(&mut self, index: usize, value: f64) -> bool {
        let Some(mut sketch) = self.sketch.take() else {
            return false;
        };
        let before = sketch.clone();
        if !sketch.set_dimension_value(index, value) {
            self.sketch = Some(sketch);
            return false;
        }
        self.push_sketch_snapshot(before);
        let _ = sketch.solve();
        self.sketch = Some(sketch);
        true
    }

    /// The editable constraint table: every constraint, its value, and a way to
    /// delete it. The panel says how many there are; this says which.
    fn draw_constraint_table(&mut self, ctx: &egui::Context) {
        if !self.show_constraint_table {
            return;
        }
        let rows = self.constraint_table_rows();
        let mut open = self.show_constraint_table;
        let mut delete: Option<usize> = None;
        let mut change: Option<(usize, f64)> = None;
        egui::Window::new("Constraints")
            .id(egui::Id::new("rcad_constraint_table"))
            .resizable(true)
            .default_width(380.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if rows.is_empty() {
                    ui.label("The sketch has no constraints yet.");
                    return;
                }
                egui::Grid::new("rcad_constraint_rows")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        for (index, (label, value, driving)) in rows.iter().enumerate() {
                            ui.label(label);
                            match value {
                                Some(value) => {
                                    let mut edited = *value;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut edited)
                                                .speed(0.1)
                                                .range(0.01..=100000.0),
                                        )
                                        .changed()
                                    {
                                        change = Some((index, edited));
                                    }
                                }
                                None => {
                                    ui.label("");
                                }
                            }
                            if ui.small_button("Delete").clicked() {
                                delete = Some(index);
                            }
                            if !driving {
                                ui.label("driven");
                            }
                            ui.end_row();
                        }
                    });
                ui.label("Dimensions can be given a new value; any row can be deleted.");
            });
        self.show_constraint_table = open;
        if let Some((index, value)) = change {
            self.set_sketch_constraint_value(index, value);
        }
        if let Some(index) = delete {
            self.delete_sketch_constraint(index);
        }
    }

    fn draw_sketch_panel(&mut self, ctx: &egui::Context) {
        let Some(sketch) = self.sketch.as_ref() else {
            return;
        };
        let name = sketch.name.clone();
        let dof = sketch.analyze_dof_detailed();
        let weak = sketch.weak_constraints().len();
        let driven = sketch.driven_dimensions().len();
        let (points, lines, circles, arcs, splines) = (
            sketch.points.len(),
            sketch.lines.len(),
            sketch.circles.len(),
            sketch.arcs.len(),
            sketch.splines.len(),
        );
        let constraints = sketch.constraints.len();
        let dimensions = sketch_ui::dimensions(sketch).len();
        let references = sketch.reference_geometry.len();
        let reference_labels: Vec<String> = sketch
            .references
            .iter()
            .map(|reference| {
                let role = if reference.role == SketchReferenceRole::Orientation {
                    "R1"
                } else {
                    "R2"
                };
                format!("{role}: {}", reference.label)
            })
            .collect();
        let (colour, verdict) = match dof.status {
            rcad_sketch::ConstraintStatus::FullyConstrained => {
                (egui::Color32::from_rgb(120, 220, 120), "fully constrained")
            }
            rcad_sketch::ConstraintStatus::OverConstrained => {
                (egui::Color32::LIGHT_RED, "over-constrained")
            }
            rcad_sketch::ConstraintStatus::UnderConstrained => {
                (egui::Color32::from_rgb(240, 200, 90), "under-constrained")
            }
        };
        let placing = sketch.in_placement_mode;
        let tool = self.sketch_draft.tool();
        let pending =
            self.sketch_draft.pending().is_some() || self.sketch_draft.pending_entity().is_some();
        let selected = self.sketch_selection.len();
        let active = self.active_dimension();
        let dimension_info = active.and_then(|index| {
            self.sketch.as_ref().map(|sketch| {
                (
                    sketch.dimension_value(index),
                    sketch.dimension_is_driving(index).unwrap_or(true),
                    sketch.dimension_is_weak(index),
                )
            })
        });
        let picks = self.dimension_picks.len();
        let orientation = self.dimension_orientation;
        let editing = self.dimension_editor.is_some();

        egui::TopBottomPanel::bottom("sketch_panel").show(ctx, |ui| {
            // Row 1: what the sketch is, how constrained it is, and the two
            // ways out of the session.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Sketch {name}")).strong());
                if placing {
                    ui.label("waiting for a plane: select a planar face or datum plane");
                } else {
                    ui.colored_label(colour, verdict);
                    ui.label(format!(
                        "DOF {}/{} — {} free",
                        dof.constrained_dof, dof.total_dof, dof.free_dof
                    ));
                    if dof.redundant_constraints > 0 {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("{} redundant", dof.redundant_constraints),
                        );
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.settings.sketch_snap, "Snap");
                if ui.button("Solve").clicked() {
                    self.solve_sketch();
                }
                ui.separator();
                let ok = ui
                    .add(egui::Button::new(egui::RichText::new("OK").strong()))
                    .on_hover_text("Keep this sketch in the model history");
                if ok.clicked() {
                    self.confirm_sketch();
                }
                if ui
                    .button("Cancel")
                    .on_hover_text("Leave the sketch without saving it")
                    .clicked()
                {
                    self.cancel_sketch();
                }
            });
            // Row 2: the tool, what the sketch holds and the two references.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Tool: {} — {}", tool.label(), tool.hint()))
                        .size(11.0),
                );
                if pending {
                    ui.colored_label(egui::Color32::from_rgb(255, 210, 90), "click pending");
                }
                if selected > 0 {
                    ui.label(format!("· {selected} selected"));
                }
                ui.separator();
                ui.label(
                    egui::RichText::new(format!(
                        "{points} points · {lines} lines · {circles} circles · {arcs} arcs · {splines} splines · {constraints} constraints · {dimensions} dimensions"
                    ))
                    .size(11.0),
                );
                if weak > 0 {
                    ui.label(egui::RichText::new(format!("· {weak} weak")).size(11.0));
                }
                if driven > 0 {
                    ui.label(egui::RichText::new(format!("· {driven} driven")).size(11.0));
                }
                if references > 0 {
                    ui.label(egui::RichText::new(format!("· {references} refs")).size(11.0));
                }
                if !reference_labels.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new(reference_labels.join("  ·  ")).size(11.0))
                        .on_hover_text("The two references that position the sketch");
                }
                ui.separator();
                ui.checkbox(&mut self.show_free_dof, "Free DOF")
                    .on_hover_text("Draw the entities that still have freedom in orange");
                if ui
                    .button("Constraints…")
                    .on_hover_text("Every constraint, with its value and a delete button")
                    .clicked()
                {
                    self.show_constraint_table = true;
                }
                if ui
                    .button("Import…")
                    .on_hover_text("Import lines and curves from a DXF or SVG file")
                    .clicked()
                {
                    self.show_import_dialog = true;
                }
            });
            // Row 3: the dimension being looked at, how it measures, and the
            // tool parameters.
            ui.horizontal(|ui| {
                if editing {
                    ui.label(
                        egui::RichText::new("Editing the value: Enter commits, Esc cancels")
                            .size(11.0),
                    );
                } else if let Some((value, driving, weak)) = dimension_info {
                    ui.label(format!(
                        "Dimension ({}{}): {}",
                        if driving { "driving" } else { "driven" },
                        if weak { ", weak" } else { "" },
                        sketch_ui::format_length(value.unwrap_or(0.0))
                    ));
                    if weak && ui.button("Promote").clicked() {
                        self.promote_selected_dimension();
                    }
                    if ui
                        .button(if driving { "Make driven" } else { "Make driving" })
                        .clicked()
                    {
                        self.toggle_selected_dimension_driving();
                    }
                    if ui.button("Edit").clicked() {
                        let at = ctx
                            .input(|input| input.pointer.hover_pos())
                            .unwrap_or_else(|| ctx.screen_rect().center());
                        if let Some(index) = active {
                            self.open_dimension_editor(index, at);
                        }
                    }
                    if ui.button("Delete").clicked() {
                        self.delete_selected_dimension();
                    }
                } else {
                    ui.label(
                        egui::RichText::new(
                            "Dimension: click one to select, double-click to edit in place",
                        )
                        .size(11.0),
                    );
                }
                if tool == SketchTool::Dimension {
                    ui.separator();
                    ui.label(egui::RichText::new("Measure along:").size(11.0));
                    for (label, value) in [
                        ("Aligned", DimensionOrientation::Aligned),
                        ("Horizontal", DimensionOrientation::Horizontal),
                        ("Vertical", DimensionOrientation::Vertical),
                    ] {
                        if ui.selectable_label(orientation == value, label).clicked() {
                            self.dimension_orientation = value;
                        }
                    }
                    ui.label(egui::RichText::new("Curve:").size(11.0));
                    for (label, value) in [
                        ("Radius", sketch_ui::DimensionSize::Radius),
                        ("Diameter", sketch_ui::DimensionSize::Diameter),
                    ] {
                        if ui
                            .selectable_label(self.dimension_size == value, label)
                            .clicked()
                        {
                            self.dimension_size = value;
                        }
                    }
                    if picks > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 210, 90),
                            format!("{picks} picked: click empty space"),
                        );
                    }
                }
                if tool == SketchTool::Pattern {
                    ui.separator();
                    ui.label(egui::RichText::new("Pattern:").size(11.0));
                    for mode in [
                        sketch_ui::PatternMode::Linear,
                        sketch_ui::PatternMode::Circular,
                    ] {
                        if ui
                            .selectable_label(
                                self.tool_params.pattern_mode == mode,
                                sketch_ui::ToolParams::pattern_mode_label(mode),
                            )
                            .clicked()
                        {
                            self.tool_params.pattern_mode = mode;
                        }
                    }
                    if self.tool_params.pattern_mode == sketch_ui::PatternMode::Circular {
                        ui.label(egui::RichText::new("Sweep°").size(11.0));
                        ui.add(
                            egui::DragValue::new(&mut self.tool_params.pattern_angle)
                                .speed(1.0)
                                .range(1.0..=360.0),
                        );
                    }
                }
                if tool == SketchTool::Conic {
                    ui.separator();
                    ui.label(egui::RichText::new("Rho").size(11.0));
                    ui.add(
                        egui::DragValue::new(&mut self.tool_params.conic_rho)
                            .speed(0.01)
                            .range(0.05..=0.95),
                    )
                    .on_hover_text("0.5 is a parabola; lower is flatter, higher rounder");
                }
                if tool == SketchTool::Text {
                    ui.separator();
                    ui.label(egui::RichText::new("Text height").size(11.0));
                    ui.add(
                        egui::DragValue::new(&mut self.tool_params.text_height)
                            .speed(0.5)
                            .range(0.5..=100.0),
                    );
                }
                ui.separator();
                ui.label(egui::RichText::new("Offset").size(11.0));
                ui.add(
                    egui::DragValue::new(&mut self.tool_params.offset_distance)
                        .speed(0.5)
                        .range(0.1..=500.0),
                );
                ui.label(egui::RichText::new("Circle R").size(11.0));
                ui.add(
                    egui::DragValue::new(&mut self.tool_params.circle_radius)
                        .speed(0.5)
                        .range(0.1..=500.0),
                );
                ui.label(egui::RichText::new("Pattern").size(11.0));
                ui.add(egui::DragValue::new(&mut self.tool_params.pattern_count).range(2..=50));
                ui.label(egui::RichText::new("×").size(11.0));
                ui.add(
                    egui::DragValue::new(&mut self.tool_params.pattern_spacing)
                        .speed(0.5)
                        .range(0.1..=500.0),
                );
            });
        });
    }

    /// A sketch with geometry, constraints and dimensions, for screenshots.
    pub fn open_sketch_demo(&mut self) {
        self.sketch_plane = Some(([0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
        self.sketch_plane_reference = None;
        let mut sketch = rcad_sketch::Sketch::new("Sketch1");
        sketch.set_standard_plane(rcad_sketch::StandardPlane::XY);
        sketch.set_references(
            SketchReference {
                reference: None,
                label: "TOP_PLANE".to_string(),
                role: SketchReferenceRole::Orientation,
                origin: [0.0; 3],
                direction: [1.0, 0.0, 0.0],
            },
            SketchReference {
                reference: None,
                label: "CYS (origin)".to_string(),
                role: SketchReferenceRole::Automatic,
                origin: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
            },
        );
        self.sketch_feature = None;
        self.dimension_picks.clear();
        self.dimension_editor = None;
        let corners = sketch.add_rectangle(-30.0, -20.0, 30.0, 20.0);
        sketch.add_circle(0.0, 4.0, 7.0);

        // A three-point arc and a spline, so the demo exercises the drawing
        // tools that are not just lines and circles.
        if let Some((centre, radius, start, end)) = sketch_ui::arc_through_points(
            [-20.0, -35.0],
            [20.0, -35.0],
            [0.0, -28.0],
        ) {
            sketch.add_arc(centre[0], centre[1], radius, start, end);
        }
        let control: Vec<Uuid> = [[-30.0, 28.0], [-10.0, 34.0], [10.0, 22.0], [30.0, 28.0]]
            .iter()
            .map(|point| sketch.add_point(point[0], point[1]))
            .collect();
        sketch.add_spline(&control, 3);

        // A conic and a text note: the other two entities a sketch can hold.
        let conic_start = sketch.add_point(-30.0, -28.0);
        let conic_control = sketch.add_point(0.0, -40.0);
        let conic_end = sketch.add_point(30.0, -28.0);
        sketch.add_conic(conic_start, conic_control, conic_end, 0.5);
        sketch.add_text([-32.0, 34.0], 4.0, "RCAD sketch");

        // A radius dimension on the circle, so the radial kind is on screen.
        if let Some(circle) = sketch.circles.first() {
            let circle_id = circle.id;
            let radius = circle.radius;
            sketch.add_dimension_constraint(
                rcad_sketch::Constraint::Radius {
                    curve: circle_id,
                    value: radius,
                },
                true,
                true,
                Some([16.0, 14.0]),
                rcad_sketch::DimensionOrientation::Aligned,
            );
        }

        // A strong driving dimension, a weak one and a driven (reference) one,
        // so all three colours the viewport uses are visible.
        sketch.add_dimension(corners[0], corners[1], 60.0, true, true);
        sketch.add_dimension(corners[1], corners[2], 40.0, true, false);
        sketch.add_dimension(corners[2], corners[3], 60.0, false, true);
        sketch.constraints.push(rcad_sketch::Constraint::Fixed {
            point: corners[0],
        });
        let mut camera = self.camera.clone();
        camera.look_from(glam::Vec3::Z);
        self.camera = camera;
        // The sketch view is what the screenshot is about: keep it.
        self.needs_fit = false;
        self.sketch_draft.begin(SketchTool::Rectangle);
        self.sketch_selection.clear();
        self.sketch_hover = None;
        self.sketch = Some(sketch);
        self.status = Some(
            "Sketch demo: rectangle, arc, spline, dimensions and DOF on screen.".to_string(),
        );
    }
    
    pub fn extrude_sketch(&mut self) {
        if self.sketch.is_some() {
            self.last_error = Some(
                "Finish (OK) or cancel the sketch before extruding it.".to_string(),
            );
            return;
        }
        let Some((sketch, placement, source)) = self.sketch_for_sweep() else {
            self.last_error = Some("There is no sketch to extrude.".to_string());
            return;
        };
        match sketch.to_profile() {
            Some(profile) => {
                if let Err(e) = self.extrude_sketch_internal(profile, 10.0, placement, source) {
                    self.last_error = Some(format!("Extrude failed: {e}"));
                }
            }
            None => {
                self.last_error = Some("The sketch has no profile to extrude.".to_string());
            }
        }
    }
    
    pub fn revolve_sketch(&mut self) {
        if self.sketch.is_some() {
            self.last_error = Some(
                "Finish (OK) or cancel the sketch before revolving it.".to_string(),
            );
            return;
        }
        let Some((sketch, placement, source)) = self.sketch_for_sweep() else {
            self.last_error = Some("There is no sketch to revolve.".to_string());
            return;
        };
        match sketch.to_profile() {
            Some(profile) => {
                if let Err(e) = self.revolve_sketch_internal(profile, 360.0, placement, source) {
                    self.last_error = Some(format!("Revolve failed: {e}"));
                }
            }
            None => {
                self.last_error = Some("The sketch has no profile to revolve.".to_string());
            }
        }
    }
    
    pub fn add_box_feature(&mut self) {
        if self.sketch.is_some() {
            self.last_error = Some(
                "Finish (OK) or cancel the sketch before another feature.".to_string(),
            );
            return;
        }
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
        self.sketch_selection.clear();
        self.selected_dimension = None;
        self.dimension_menu = None;
        self.dimension_editor = None;
    }
    
    pub fn show_shortcuts_help(&mut self) {
        self.show_shortcuts_dialog = true;
    }
    
    // Internal methods (renamed from original private methods)
    fn extrude_sketch_internal(
        &mut self,
        profile: rcad_geom::Profile2D,
        height: f64,
        placement: Option<SketchPlacement>,
        source: Option<Uuid>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if profile.outer.len() < 3 {
            return Err("extrude needs a closed profile with at least 3 points".into());
        }
        self.undo_stack.push(self.project.clone());
        if let Some(part) = self.get_active_part_mut() {
            // Extrude the profile that was passed in. The previous version
            // discarded it and appended a hard-coded 10 x 10 x height box, so
            // every sketch produced the same prism.
            let name = rcad_features::unique_feature_name(part, "Extrude");
            rcad_features::add_extrude(part, &name, profile, height, placement, source);
            self.rebuild_active_part();
        }
        Ok(())
    }
    
    fn revolve_sketch_internal(
        &mut self,
        profile: rcad_geom::Profile2D,
        angle_deg: f64,
        placement: Option<SketchPlacement>,
        source: Option<Uuid>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Save state before modifying
        self.undo_stack.push(self.project.clone());
        if let Some(part) = self.get_active_part_mut() {
            let name = rcad_features::unique_feature_name(part, "Revolve");
            rcad_features::add_revolve(part, &name, profile, angle_deg, placement, source);
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
        // While a sketch is being edited no other feature may start: the body
        // the feature would build on is not in the tree yet. OK or Cancel ends
        // the session first.
        if self.sketch.is_some() && !action_allowed_while_sketching(&action) {
            self.last_error = Some(
                "Finish the sketch with OK, or cancel it, before another feature.".to_string(),
            );
            return;
        }
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
                self.add_edge_feature(
                    EdgeFeature::Fillet,
                    self.edge_feature_size,
                    self.selection_edge_set(),
                );
            }
            ToolbarAction::ModelChamfer => {
                self.add_edge_feature(
                    EdgeFeature::Chamfer,
                    self.edge_feature_size,
                    self.selection_edge_set(),
                );
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
            // The drawing tools are two-click tools now: pressing one arms it,
            // the clicks in the viewport draw. Nothing enters the sketch from a
            // button on its own.
            ToolbarAction::SketchLine => self.set_sketch_tool(SketchTool::Line),
            ToolbarAction::SketchRectangle => self.set_sketch_tool(SketchTool::Rectangle),
            ToolbarAction::SketchCircle => self.set_sketch_tool(SketchTool::Circle),
            ToolbarAction::SketchArc => self.set_sketch_tool(SketchTool::Arc3Point),
            ToolbarAction::SketchSpline => self.set_sketch_tool(SketchTool::Spline),
            ToolbarAction::SketchPoint => self.set_sketch_tool(SketchTool::Point),
            // Constraints apply to the current sketch selection.
            ToolbarAction::SketchConstraintHorizontal => {
                self.apply_sketch_constraint(ConstraintAction::Horizontal)
            }
            ToolbarAction::SketchConstraintVertical => {
                self.apply_sketch_constraint(ConstraintAction::Vertical)
            }
            ToolbarAction::SketchConstraintParallel => {
                self.apply_sketch_constraint(ConstraintAction::Parallel)
            }
            ToolbarAction::SketchConstraintPerpendicular => {
                self.apply_sketch_constraint(ConstraintAction::Perpendicular)
            }
            ToolbarAction::SketchConstraintTangent => {
                self.apply_sketch_constraint(ConstraintAction::Tangent)
            }
            ToolbarAction::SketchConstraintEqual => {
                self.apply_sketch_constraint(ConstraintAction::Equal)
            }
            ToolbarAction::SketchConstraintSymmetric => {
                self.apply_sketch_constraint(ConstraintAction::Symmetric)
            }
            ToolbarAction::SketchConstraintConcentric => {
                self.apply_sketch_constraint(ConstraintAction::Concentric)
            }
            ToolbarAction::SketchConstraintMidpoint => {
                self.apply_sketch_constraint(ConstraintAction::Midpoint)
            }
            ToolbarAction::SketchConstraintCoincident => {
                self.apply_sketch_constraint(ConstraintAction::Coincident)
            }
            ToolbarAction::SketchDimension => self.set_sketch_tool(SketchTool::Dimension),
            ToolbarAction::SketchTrim => self.set_sketch_tool(SketchTool::Trim),
            ToolbarAction::SketchExtend => self.set_sketch_tool(SketchTool::Extend),
            ToolbarAction::SketchOffset => self.set_sketch_tool(SketchTool::Offset),
            ToolbarAction::SketchMirror => self.set_sketch_tool(SketchTool::Mirror),
            ToolbarAction::SketchPattern => self.set_sketch_tool(SketchTool::Pattern),
            ToolbarAction::SketchProjectReferences => {
                if let Err(message) = self.project_references_into_sketch() {
                    self.last_error = Some(message);
                }
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
            FloatingAction::SketchTool(tool) => self.set_sketch_tool(tool),
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
                // A sketch opens back into its editor; every other feature
                // opens the feature dialog.
                let is_sketch = self
                    .get_active_part()
                    .and_then(|part| part.features.iter().find(|feature| feature.id == feature_id))
                    .map(|feature| matches!(feature.kind, FeatureKind::Sketch { .. }))
                    .unwrap_or(false);
                if is_sketch {
                    if let Err(message) = self.edit_sketch_feature(feature_id) {
                        self.last_error = Some(message);
                    }
                } else {
                    self.show_relations_dialog = Some(feature_id);
                }
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
                        lost_references: res.lost_references,
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
            edge_feature_size: 1.0,
            sketch_plane: None,
            reference_memory: std::collections::BTreeMap::new(),
            lost_references: Vec::new(),
            sketch_plane_reference: None,
            sketch_draft: SketchDraft::default(),
            sketch_selection: Vec::new(),
            sketch_hover: None,
            sketch_undo: Vec::new(),
            sketch_redo: Vec::new(),
            dimension_editor: None,
            selected_dimension: None,
            dimension_menu: None,
            dimension_menu_was_shown: false,
            sketch_feature: None,
            show_free_dof: false,
            dimension_picks: Vec::new(),
            dimension_orientation: DimensionOrientation::default(),
            dimension_size: sketch_ui::DimensionSize::default(),
            show_constraint_table: false,
            show_import_dialog: false,
            import_path: String::new(),
            sketch_text_editor: None,
            dragging_point: None,
            tool_params: sketch_ui::ToolParams::default(),
            viewport_rect: None,
            
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

/// The references the part asks for that the body no longer has.
///
/// The kernel does the same test to decide whether to skip a feature; this one is
/// what the repair list shows, and it names the feature the user has to fix.
fn lost_references(part: &Part, mesh: &TriangleMesh) -> Vec<rcad_doc::LostReference> {
    let present: std::collections::BTreeSet<&str> =
        mesh.edges.iter().map(|edge| edge.name.as_str()).collect();
    let mut lost = Vec::new();
    for feature in &part.features {
        let edges = match &feature.kind {
            FeatureKind::Fillet { edges, .. } | FeatureKind::Chamfer { edges, .. } => edges,
            _ => continue,
        };
        let EdgeSet::References(references) = edges else {
            continue;
        };
        for reference in references {
            if !present.contains(reference.as_str()) {
                lost.push(rcad_doc::LostReference {
                    feature: feature.id,
                    feature_name: feature.name.clone(),
                    reference: reference.clone(),
                    last_location: None,
                    last_description: None,
                });
            }
        }
    }
    lost
}

/// Attach what is known about each lost reference from the last build that had
/// it: the location and the sentence the repair list shows.
fn describe_lost(
    lost: &mut [rcad_doc::LostReference],
    memory: &std::collections::BTreeMap<String, rcad_geom::ReferenceMemory>,
) {
    for entry in lost.iter_mut() {
        if let Some(remembered) = memory.get(&entry.reference) {
            entry.last_location = Some(remembered.location);
            entry.last_description = Some(remembered.describe());
        }
    }
}


/// The boundary of the triangles of one face, as segments between vertices
/// that belong to a single triangle.
///
/// The tessellation repeats an interior edge in the two triangles on either
/// side of it, so the edges seen exactly once are the outline of the face. This
/// is what "project the face into the sketch" projects.
fn face_boundary_segments(mesh: &TriangleMesh, face: &str) -> Vec<[[f64; 3]; 2]> {
    let position = |index: u32| {
        mesh.positions
            .get(index as usize)
            .map(|point| [point[0] as f64, point[1] as f64, point[2] as f64])
    };
    let mut seen: std::collections::HashMap<(u32, u32), (usize, [f64; 3], [f64; 3])> =
        std::collections::HashMap::new();
    for (triangle, indices) in mesh.indices.chunks_exact(3).enumerate() {
        if mesh.face_name(triangle) != Some(face) {
            continue;
        }
        let corners = [indices[0], indices[1], indices[2]];
        for step in 0..3 {
            let (i, j) = (corners[step], corners[(step + 1) % 3]);
            let key = if i < j { (i, j) } else { (j, i) };
            if let (Some(a), Some(b)) = (position(key.0), position(key.1)) {
                seen.entry(key).or_insert((0, a, b)).0 += 1;
            }
        }
    }
    seen.into_values()
        .filter(|(count, _, _)| *count == 1)
        .map(|(_, a, b)| [a, b])
        .collect()
}



/// Actions that may run while a sketch is being edited: the sketch tools, the
/// view, undo/redo and help. A model feature would build on a body the sketch
/// has not been added to yet, so it has to wait for OK.
fn action_allowed_while_sketching(action: &ToolbarAction) -> bool {
    matches!(
        action,
        ToolbarAction::Undo
            | ToolbarAction::Redo
            | ToolbarAction::ViewFront
            | ToolbarAction::ViewBack
            | ToolbarAction::ViewLeft
            | ToolbarAction::ViewRight
            | ToolbarAction::ViewTop
            | ToolbarAction::ViewBottom
            | ToolbarAction::ViewIso
            | ToolbarAction::ViewReset
            | ToolbarAction::ViewPerspective
            | ToolbarAction::ViewOrthographic
            | ToolbarAction::ViewShaded
            | ToolbarAction::ViewWireframe
            | ToolbarAction::ViewZoomFit
            | ToolbarAction::ViewPrevious
            | ToolbarAction::SketchLine
            | ToolbarAction::SketchRectangle
            | ToolbarAction::SketchCircle
            | ToolbarAction::SketchArc
            | ToolbarAction::SketchSpline
            | ToolbarAction::SketchPoint
            | ToolbarAction::SketchTrim
            | ToolbarAction::SketchExtend
            | ToolbarAction::SketchOffset
            | ToolbarAction::SketchMirror
            | ToolbarAction::SketchPattern
            | ToolbarAction::SketchProjectReferences
            | ToolbarAction::SketchConstraintHorizontal
            | ToolbarAction::SketchConstraintVertical
            | ToolbarAction::SketchConstraintParallel
            | ToolbarAction::SketchConstraintPerpendicular
            | ToolbarAction::SketchConstraintTangent
            | ToolbarAction::SketchConstraintEqual
            | ToolbarAction::SketchConstraintSymmetric
            | ToolbarAction::SketchConstraintConcentric
            | ToolbarAction::SketchConstraintMidpoint
            | ToolbarAction::SketchConstraintCoincident
            | ToolbarAction::SketchDimension
            | ToolbarAction::ToolsOptions
            | ToolbarAction::HelpShortcuts
            | ToolbarAction::HelpAbout
    )
}

/// True when the sketch has anything worth keeping in the history.
fn sketch_has_geometry(sketch: &Sketch) -> bool {
    !sketch.points.is_empty()
        || !sketch.lines.is_empty()
        || !sketch.circles.is_empty()
        || !sketch.arcs.is_empty()
        || !sketch.splines.is_empty()
}


/// Turn the dynamic dimension tool's picks and the placement click into a
/// dimension on the sketch.
///
/// One line becomes its length - along the line, or projected on the sketch X
/// or Y axis. Two lines become an angle, or the distance between them when they
/// are parallel. Two points (a circle or arc centre counts as a point) become a
/// distance. The click position is where the text goes.
fn place_dimension(
    sketch: &mut Sketch,
    picks: &[(Uuid, sketch_ui::SketchEntityKind)],
    at: [f64; 2],
    orientation: DimensionOrientation,
    size: sketch_ui::DimensionSize,
) -> Result<String, String> {
    use sketch_ui::SketchEntityKind;

    let line = |sketch: &Sketch, id: Uuid| -> Option<(Uuid, Uuid, [f64; 2], [f64; 2])> {
        let line = sketch.lines.iter().find(|line| line.id == id)?;
        let a = sketch.point(line.a)?;
        let b = sketch.point(line.b)?;
        Some((line.a, line.b, [a.x, a.y], [b.x, b.y]))
    };
    let entity_point = |sketch: &Sketch,
                        id: Uuid,
                        kind: SketchEntityKind|
     -> Option<(Uuid, [f64; 2])> {
        let point = match kind {
            SketchEntityKind::Point => id,
            SketchEntityKind::Circle => sketch
                .circles
                .iter()
                .find(|circle| circle.id == id)?
                .center,
            SketchEntityKind::Arc => sketch
                .arcs
                .iter()
                .find(|arc| arc.id == id)?
                .center,
            SketchEntityKind::Line => return None,
        };
        let point = sketch.point(point)?;
        Some((point.id, [point.x, point.y]))
    };

    let (constraint, description) = match picks {
        [(id, SketchEntityKind::Line)] => {
            let (first, second, a, b) =
                line(sketch, *id).ok_or_else(|| "That line is gone.".to_string())?;
            let value = match orientation {
                DimensionOrientation::Aligned => (b[0] - a[0]).hypot(b[1] - a[1]),
                DimensionOrientation::Horizontal => (b[0] - a[0]).abs(),
                DimensionOrientation::Vertical => (b[1] - a[1]).abs(),
            };
            if value < 1e-9 {
                return Err("That dimension would be zero.".to_string());
            }
            let constraint = match orientation {
                DimensionOrientation::Aligned => Constraint::Distance {
                    a: first,
                    b: second,
                    value,
                },
                DimensionOrientation::Horizontal => Constraint::HorizontalDistance {
                    a: first,
                    b: second,
                    value,
                },
                DimensionOrientation::Vertical => Constraint::VerticalDistance {
                    a: first,
                    b: second,
                    value,
                },
            };
            (
                constraint,
                format!("Length dimension: {}.", sketch_ui::format_length(value)),
            )
        }
        [(id, kind @ (SketchEntityKind::Circle | SketchEntityKind::Arc))] => {
            let curve = |sketch: &Sketch, id: Uuid| -> Option<([f64; 2], f64)> {
                if let Some(circle) = sketch.circles.iter().find(|circle| circle.id == id) {
                    let centre = sketch.point(circle.center)?;
                    return Some(([centre.x, centre.y], circle.radius));
                }
                let arc = sketch.arcs.iter().find(|arc| arc.id == id)?;
                let centre = sketch.point(arc.center)?;
                Some(([centre.x, centre.y], arc.radius))
            };
            let (_, radius) = curve(sketch, *id).ok_or_else(|| "That curve is gone.".to_string())?;
            let (constraint, description) = match size {
                sketch_ui::DimensionSize::Radius => (
                    Constraint::Radius {
                        curve: *id,
                        value: radius,
                    },
                    format!("Radius dimension: R{}.", sketch_ui::format_length(radius)),
                ),
                sketch_ui::DimensionSize::Diameter => (
                    Constraint::Diameter {
                        curve: *id,
                        value: radius * 2.0,
                    },
                    format!("Diameter dimension: Ø{}.", sketch_ui::format_length(radius * 2.0)),
                ),
            };
            let _ = kind;
            (constraint, description)
        }
        [(a, kind_a), (b, kind_b)] => {
            let first_line = line(sketch, *a);
            let second_line = line(sketch, *b);
            // A line and a point: the perpendicular distance from the point to
            // the line.
            let point_of_pick = |sketch: &Sketch,
                                 id: Uuid,
                                 kind: SketchEntityKind|
             -> Option<(Uuid, [f64; 2])> {
                match kind {
                    SketchEntityKind::Point => {
                        let point = sketch.point(id)?;
                        Some((point.id, [point.x, point.y]))
                    }
                    SketchEntityKind::Circle | SketchEntityKind::Arc => {
                        entity_point(sketch, id, kind)
                    }
                    SketchEntityKind::Line => None,
                }
            };
            let line_side = if first_line.is_some() && second_line.is_none() {
                Some((*a, *b, *kind_b))
            } else if second_line.is_some() && first_line.is_none() {
                Some((*b, *a, *kind_a))
            } else {
                None
            };
            if let Some((line_id, other_id, other_kind)) = line_side {
                let ends = line(sketch, line_id)
                    .ok_or_else(|| "That line is gone.".to_string())?;
                if other_kind == SketchEntityKind::Point {
                    let (point_id, point) = point_of_pick(sketch, other_id, other_kind)
                        .ok_or_else(|| "That point is gone.".to_string())?;
                    let foot = sketch_ui::foot_on_line(point, ends.2, ends.3)
                        .ok_or_else(|| "That line has no length.".to_string())?;
                    let value = (point[0] - foot[0]).hypot(point[1] - foot[1]);
                    if value < 1e-9 {
                        return Err("The point is on the line: no distance to dimension.".to_string());
                    }
                    (
                        Constraint::PointLineDistance {
                            point: point_id,
                            line: line_id,
                            value,
                        },
                        format!(
                            "Point-line distance: {}.",
                            sketch_ui::format_length(value)
                        ),
                    )
                } else if other_kind == SketchEntityKind::Circle
                    || other_kind == SketchEntityKind::Arc
                {
                    let (centre, radius) = if let Some(circle) = sketch
                        .circles
                        .iter()
                        .find(|circle| circle.id == other_id)
                    {
                        let centre = sketch
                            .point(circle.center)
                            .ok_or_else(|| "That curve is gone.".to_string())?;
                        ([centre.x, centre.y], circle.radius)
                    } else {
                        let arc = sketch
                            .arcs
                            .iter()
                            .find(|arc| arc.id == other_id)
                            .ok_or_else(|| "That curve is gone.".to_string())?;
                        let centre = sketch
                            .point(arc.center)
                            .ok_or_else(|| "That curve is gone.".to_string())?;
                        ([centre.x, centre.y], arc.radius)
                    };
                    let foot = sketch_ui::foot_on_line(centre, ends.2, ends.3)
                        .ok_or_else(|| "That line has no length.".to_string())?;
                    let distance = (centre[0] - foot[0]).hypot(centre[1] - foot[1]);
                    let value = (distance - radius).max(0.0);
                    (
                        Constraint::LineCurveDistance {
                            line: line_id,
                            curve: other_id,
                            value,
                        },
                        format!(
                            "Tangent distance: {}.",
                            sketch_ui::format_length(value)
                        ),
                    )
                } else {
                    return Err(
                        "Pick two points or two lines (or one line for its length).".to_string()
                    );
                }
            } else {
            if let (Some(first), Some(second)) = (first_line, second_line) {
                let d1 = [first.3[0] - first.2[0], first.3[1] - first.2[1]];
                let d2 = [second.3[0] - second.2[0], second.3[1] - second.2[1]];
                let angle = sketch_ui::angle_between(d1, d2);
                if angle < 10.0 || angle > 170.0 {
                    let value =
                        sketch_ui::line_distance((first.2, first.3), (second.2, second.3));
                    if value < 1e-9 {
                        return Err(
                            "Those lines cross: pick a distance between two that do not.".to_string()
                        );
                    }
                    (
                        Constraint::LineDistance {
                            a: *a,
                            b: *b,
                            value,
                        },
                        format!("Distance dimension: {}.", sketch_ui::format_length(value)),
                    )
                } else {
                    (
                        Constraint::Angle {
                            a: *a,
                            b: *b,
                            value: angle,
                        },
                        format!("Angle dimension: {}°.", sketch_ui::format_length(angle)),
                    )
                }
            } else if first_line.is_none() && second_line.is_none() {
                let (first_id, first) = entity_point(sketch, *a, *kind_a)
                    .ok_or_else(|| "Pick two points or two lines.".to_string())?;
                let (second_id, second) = entity_point(sketch, *b, *kind_b)
                    .ok_or_else(|| "Pick two points or two lines.".to_string())?;
                let value = match orientation {
                    DimensionOrientation::Aligned => {
                        (second[0] - first[0]).hypot(second[1] - first[1])
                    }
                    DimensionOrientation::Horizontal => (second[0] - first[0]).abs(),
                    DimensionOrientation::Vertical => (second[1] - first[1]).abs(),
                };
                if value < 1e-9 {
                    return Err("That dimension would be zero.".to_string());
                }
                let constraint = match orientation {
                    DimensionOrientation::Aligned => Constraint::Distance {
                        a: first_id,
                        b: second_id,
                        value,
                    },
                    DimensionOrientation::Horizontal => Constraint::HorizontalDistance {
                        a: first_id,
                        b: second_id,
                        value,
                    },
                    DimensionOrientation::Vertical => Constraint::VerticalDistance {
                        a: first_id,
                        b: second_id,
                        value,
                    },
                };
                (
                    constraint,
                    format!("Distance dimension: {}.", sketch_ui::format_length(value)),
                )
                } else {
                    return Err(
                        "Pick two points or two lines (or one line for its length).".to_string()
                    );
                }
            }
        }
        [] => return Err("Click a line or a point to dimension.".to_string()),
        _ => {
            return Err("A dimension takes one line or two things, not more.".to_string())
        }
    };

    if !sketch.add_dimension_constraint(constraint, true, true, Some(at), orientation) {
        return Err("Could not add that dimension.".to_string());
    }
    Ok(description)
}

/// The plane of a mesh triangle: its centre, its normal and an x direction in
/// the plane. The normal comes from the triangle vertices, so it does not depend
/// on how the tessellation happens to store its normals.
fn face_plane(mesh: &TriangleMesh, triangle: usize) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let indices = mesh.indices.get(triangle * 3..triangle * 3 + 3)?;
    let mut corners = [[0.0f64; 3]; 3];
    for (slot, index) in corners.iter_mut().zip(indices) {
        let position = mesh.positions.get(*index as usize)?;
        *slot = [position[0] as f64, position[1] as f64, position[2] as f64];
    }
    let centre = [
        (corners[0][0] + corners[1][0] + corners[2][0]) / 3.0,
        (corners[0][1] + corners[1][1] + corners[2][1]) / 3.0,
        (corners[0][2] + corners[1][2] + corners[2][2]) / 3.0,
    ];
    let edge_a = [
        corners[1][0] - corners[0][0],
        corners[1][1] - corners[0][1],
        corners[1][2] - corners[0][2],
    ];
    let edge_b = [
        corners[2][0] - corners[0][0],
        corners[2][1] - corners[0][1],
        corners[2][2] - corners[0][2],
    ];
    let normal = normalised(cross(edge_a, edge_b))?;
    let (x_direction, _) = crate::viewport::sketch_basis(normal, [1.0, 0.0, 0.0]);
    Some((centre, normal, [x_direction[0] as f64, x_direction[1] as f64, x_direction[2] as f64]))
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

        /// Run one frame with the given events.
        fn run_step(&mut self, events: Vec<egui::Event>) {
            let raw = self.input(events);
            let Self { ctx, app, .. } = self;
            let _ = ctx.run(raw, |ctx| app.ui(ctx));
        }

        /// Two clicks in quick succession: a double-click.
        fn double_click(&mut self, pos: egui::Pos2) {
            self.click(pos);
            self.click(pos);
            self.frame();
        }

        /// Press at `from`, move to `to` with the button held, then release.
        fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
            let press = vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            self.run_step(press);
            let middle = from + (to - from) * 0.5;
            self.run_step(vec![
                egui::Event::PointerMoved(middle),
                egui::Event::PointerMoved(to),
            ]);
            self.run_step(vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }]);
            self.frame();
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

    /// The toolbar round reads the selection and the size field: the cycle from
    /// picking edges to a feature that stores them.
    #[test]
    fn the_toolbar_round_uses_the_selected_edges_and_the_size() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        app.selection.replace(Selected::Edge {
            reference: "e1".to_string(),
            label: "e1".to_string(),
        });
        app.selection.extend(Selected::Edge {
            reference: "e2".to_string(),
            label: "e2".to_string(),
        });
        app.edge_feature_size = 2.5;

        app.handle_toolbar_action(ToolbarAction::ModelFillet);

        let part = app.project.parts.first().unwrap();
        let round = part
            .features
            .iter()
            .find(|feature| matches!(feature.kind, FeatureKind::Fillet { .. }))
            .expect("a round");
        match &round.kind {
            FeatureKind::Fillet { radius, edges } => {
                assert_eq!(*radius, 2.5);
                assert_eq!(
                    *edges,
                    EdgeSet::References(vec!["e1".to_string(), "e2".to_string()])
                );
            }
            _ => unreachable!(),
        }
    }

    /// With nothing picked, the same button means every edge, as it always did.
    #[test]
    fn the_toolbar_break_without_a_selection_covers_every_edge() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();

        app.handle_toolbar_action(ToolbarAction::ModelChamfer);

        let part = app.project.parts.first().unwrap();
        assert!(part.features.iter().any(|feature| matches!(
            &feature.kind,
            FeatureKind::Chamfer { edges: EdgeSet::All, distance } if (*distance - 1.0).abs() < 1e-9
        )));
    }

    /// A face of the model is a sketch plane: its centre and normal are enough,
    /// with no datum needed.
    #[test]
    fn a_sketch_is_placed_on_a_selected_model_face() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        app.mesh = TriangleMesh {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        app.selection.replace(Selected::Face {
            reference: "face".to_string(),
            label: "face".to_string(),
            triangle: 0,
        });

        app.new_sketch();

        let sketch = app.sketch.as_ref().expect("a sketch");
        assert!(!sketch.in_placement_mode, "the face gives the plane");
        let (origin, normal, _) = app.sketch_plane.expect("a plane");
        assert_eq!(origin, [10.0 / 3.0, 10.0 / 3.0, 0.0]);
        assert!(normal[2].abs() > 0.99, "{normal:?}");
    }

    /// The cycle the sketch exists for: rectangle, solve, profile, extrude.
    #[test]
    fn a_rectangle_sketch_extrudes_into_a_solid_feature() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        app.new_sketch();
        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        app.solve_sketch();
        // OK puts the sketch in the history; only then can a feature use it.
        app.confirm_sketch();

        app.extrude_sketch();

        let part = app.project.parts.first().unwrap();
        assert!(
            part.features
                .iter()
                .any(|feature| matches!(feature.kind, FeatureKind::Sketch { .. })),
            "OK should have added the sketch to the history"
        );
        assert!(
            part.features
                .iter()
                .any(|feature| matches!(feature.kind, FeatureKind::Extrude { .. })),
            "the sketch profile should have become an extrude: {:?}",
            app.last_error
        );
    }

    /// Off the XY plane the profile now carries its plane, so the sweep follows
    /// the sketch normal instead of always running along Z.
    #[test]
    fn a_sketch_off_the_xy_plane_extrudes_along_its_own_normal() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        app.new_sketch();
        app.sketch_plane = Some(([0.0; 3], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]));
        // The sketch itself carries the plane the feature will follow.
        app.sketch
            .as_mut()
            .unwrap()
            .set_sketch_plane([0.0; 3], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        app.solve_sketch();
        app.confirm_sketch();

        app.extrude_sketch();

        let part = app.project.parts.first().unwrap();
        let placement = part.features.iter().find_map(|feature| match &feature.kind {
            FeatureKind::Extrude { placement, .. } => Some(placement.clone()),
            _ => None,
        });
        let placement = placement.expect("the sketch should have extruded").expect("a placement");
        assert_eq!(placement.normal, [0.0, 1.0, 0.0]);
        assert_eq!(placement.x_direction, [1.0, 0.0, 0.0]);
    }

    /// The references a feature asks for that the body no longer has are what the
    /// repair list shows, named by feature and reference.
    #[test]
    fn a_reference_the_body_lost_is_listed_for_repair() {
        let mut part = Part::new_solid("P");
        part.features.push(Feature::new(
            "Round1",
            FeatureKind::Fillet {
                radius: 1.0,
                edges: EdgeSet::References(vec!["gone".to_string(), "here".to_string()]),
            },
        ));
        let mut mesh = TriangleMesh::default();
        mesh.edges.push(test_edge(
            "here",
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ));

        let lost = lost_references(&part, &mesh);

        assert_eq!(lost.len(), 1, "only the missing one is listed");
        assert_eq!(lost[0].reference, "gone");
        assert_eq!(lost[0].feature_name, "Round1");
        assert!(lost[0].last_description.is_none(), "nothing is invented");
    }

    /// The repair list says where the reference was, from the last build that
    /// still had it.
    #[test]
    fn the_repair_list_says_where_the_reference_was() {
        let mut lost = vec![rcad_doc::LostReference {
            reference: "gone".to_string(),
            ..Default::default()
        }];
        let mut memory = std::collections::BTreeMap::new();
        memory.insert(
            "gone".to_string(),
            rcad_geom::ReferenceMemory {
                reference: "gone".to_string(),
                kind: "edge".to_string(),
                location: [20.0, 15.0, 8.0],
                size: 40.0,
                neighbours: vec!["face".to_string()],
                label: "edge".to_string(),
            },
        );

        describe_lost(&mut lost, &memory);

        assert_eq!(lost[0].last_location, Some([20.0, 15.0, 8.0]));
        let description = lost[0].last_description.clone().unwrap_or_default();
        assert!(description.contains("40"), "{description}");
        assert!(description.contains("20"), "{description}");
    }

    /// Substituting a lost reference rewrites the feature edge set in place, and
    /// refuses when there is nothing to replace.
    #[test]
    fn substituting_a_lost_reference_rewrites_the_feature() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        let feature = Feature::new(
            "Round1",
            FeatureKind::Fillet {
                radius: 1.0,
                edges: EdgeSet::References(vec!["gone".to_string()]),
            },
        );
        let id = feature.id;
        app.get_active_part_mut().unwrap().features.push(feature);

        assert!(app.substitute_reference(id, "gone", "edge(now|here)"));
        let part = app.project.parts.first().unwrap();
        match &part.features.last().unwrap().kind {
            FeatureKind::Fillet { edges, .. } => assert_eq!(
                *edges,
                EdgeSet::References(vec!["edge(now|here)".to_string()])
            ),
            _ => unreachable!(),
        }
        assert!(
            !app.substitute_reference(id, "not-there", "x"),
            "a reference the feature never had cannot be substituted"
        );
    }

    /// The revolve carries the sketch plane too, so it spins about the axis of
    /// the plane the sketch was drawn on.
    #[test]
    fn a_sketch_revolves_around_its_plane_axis() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        app.new_sketch();
        app.sketch_plane = Some(([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        app.sketch
            .as_mut()
            .unwrap()
            .set_sketch_plane([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        app.solve_sketch();
        app.confirm_sketch();

        app.revolve_sketch();

        let part = app.project.parts.first().unwrap();
        let placement = part.features.iter().find_map(|feature| match &feature.kind {
            FeatureKind::Revolve { placement, .. } => Some(placement.clone()),
            _ => None,
        });
        let placement = placement.expect("the sketch should have revolved");
        assert_eq!(placement.expect("a placement").normal, [1.0, 0.0, 0.0]);
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

    /// A sketch open on the XY plane. The camera looks straight down at it:
    /// from the default direction the plane is edge-on and a screen ray never
    /// meets it, so drawing would be impossible.
    fn open_xy_sketch(app: &mut RcadApp) {
        app.sketch_plane = Some(([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]));
        app.sketch_plane_reference = None;
        app.camera.look_from(glam::Vec3::Z);
        // The test owns the camera and the snap: the user settings may have
        // either turned off, and a test must not depend on them.
        app.needs_fit = false;
        app.settings.sketch_snap = true;
        let mut sketch = Sketch::new("Sketch1");
        sketch.set_references(
            SketchReference {
                reference: None,
                label: "XY".to_string(),
                role: SketchReferenceRole::Orientation,
                origin: [0.0; 3],
                direction: [1.0, 0.0, 0.0],
            },
            SketchReference {
                reference: None,
                label: "CYS (origin)".to_string(),
                role: SketchReferenceRole::Automatic,
                origin: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
            },
        );
        app.sketch = Some(sketch);
        app.sketch_draft.begin(SketchTool::Select);
        app.sketch_selection.clear();
    }

    /// The viewport rectangle the sketch tests click in.
    fn viewport_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    /// The screen position of a sketch point in that rectangle.
    fn screen_of(app: &RcadApp, point: [f64; 2]) -> egui::Pos2 {
        sketch_ui::sketch_to_screen(
            viewport_rect(),
            &app.camera,
            app.sketch_plane.expect("a sketch plane"),
            point,
        )
        .expect("the point is in view")
    }

    /// The viewport as the last frame laid it out.
    fn viewport_of(app: &RcadApp) -> egui::Rect {
        app.viewport_rect.expect("the viewport was drawn")
    }

    /// The screen position of a sketch point in the real viewport.
    fn viewport_screen_of(app: &RcadApp, point: [f64; 2]) -> egui::Pos2 {
        sketch_ui::sketch_to_screen(
            viewport_of(app),
            &app.camera,
            app.sketch_plane.expect("a sketch plane"),
            point,
        )
        .expect("the point is in view")
    }

    /// Point 1 of the sketch: two clicks in the viewport draw a rectangle, and
    /// its corners land on the grid.
    #[test]
    fn two_viewport_clicks_draw_a_snapped_rectangle() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.set_sketch_tool(SketchTool::Rectangle);

        // 12, 7 is not on the 5 mm grid; the snap moves it to 10, 5.
        let first = screen_of(&app, [12.0, 7.0]);
        assert!(app.sketch_click(viewport_rect(), first, ClickMode::PLAIN));
        assert!(
            app.sketch.as_ref().unwrap().lines.is_empty(),
            "one click is half a rectangle"
        );
        assert_eq!(app.sketch_draft.pending(), Some([10.0, 5.0]));

        let second = screen_of(&app, [38.0, 26.0]);
        assert!(app.sketch_click(viewport_rect(), second, ClickMode::PLAIN));

        let sketch = app.sketch.as_ref().unwrap();
        assert_eq!(sketch.lines.len(), 4, "four sides of the rectangle");
        let corners: Vec<[f64; 2]> = sketch.points.iter().map(|point| [point.x, point.y]).collect();
        for expected in [[10.0, 5.0], [40.0, 5.0], [40.0, 25.0], [10.0, 25.0]] {
            assert!(
                corners.contains(&expected),
                "{expected:?} is missing from {corners:?}"
            );
        }
    }

    /// Turning the snap off keeps the coordinates the user clicked.
    #[test]
    fn turning_the_snap_off_keeps_the_clicked_coordinates() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.settings.sketch_snap = false;
        app.set_sketch_tool(SketchTool::Rectangle);

        let corner = screen_of(&app, [12.0, 7.0]);
        app.sketch_click(viewport_rect(), corner, ClickMode::PLAIN);

        let pending = app.sketch_draft.pending().expect("a first corner");
        assert!(
            (pending[0] - 12.0).abs() < 1e-3 && (pending[1] - 7.0).abs() < 1e-3,
            "{pending:?}"
        );
    }

    /// Escape steps back one level at a time instead of throwing the tool away.
    #[test]
    fn escape_steps_back_through_the_sketch_tool() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.set_sketch_tool(SketchTool::Rectangle);
        let origin = screen_of(&app, [0.0, 0.0]);
        app.sketch_click(viewport_rect(), origin, ClickMode::PLAIN);
        assert!(app.sketch_draft.pending().is_some());

        app.escape();
        assert!(
            app.sketch_draft.pending().is_none(),
            "the pending corner goes first"
        );
        assert_eq!(
            app.sketch_draft.tool(),
            SketchTool::Rectangle,
            "the tool stays"
        );

        app.escape();
        assert_eq!(app.sketch_draft.tool(), SketchTool::Select);
        assert!(
            app.sketch.as_ref().unwrap().lines.is_empty(),
            "nothing was drawn"
        );
    }

    /// Point 2: a constraint applies to the sketch selection, and the sketch is
    /// solved straight away.
    #[test]
    fn a_constraint_applies_to_the_sketch_selection() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        let line = app.sketch.as_ref().unwrap().lines[0].id;
        app.sketch_selection = vec![line];

        let before = app.sketch.as_ref().unwrap().constraints.len();
        app.apply_sketch_constraint(ConstraintAction::Horizontal);

        assert!(app.last_error.is_none(), "{:?}", app.last_error);
        assert_eq!(app.sketch.as_ref().unwrap().constraints.len(), before + 1);
    }

    /// A constraint that does not fit says what to select and gives the
    /// selection back.
    #[test]
    fn a_constraint_that_does_not_fit_explains_itself() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        let line = app.sketch.as_ref().unwrap().lines[0].id;
        app.sketch_selection = vec![line];

        app.apply_sketch_constraint(ConstraintAction::Parallel);

        let error = app.last_error.clone().expect("parallel needs two lines");
        assert!(error.contains("select two lines"), "{error}");
        assert_eq!(app.sketch_selection, vec![line], "the selection comes back");
    }

    /// The dimension tool measures what it was clicked on.
    #[test]
    fn the_dimension_tool_dimensions_a_clicked_line() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        app.set_sketch_tool(SketchTool::Dimension);

        // The bottom side runs from (-25, -25) to (25, -25): 50 mm.
        let midpoint = screen_of(&app, [0.0, -25.0]);
        assert!(app.sketch_click(viewport_rect(), midpoint, ClickMode::PLAIN));
        // The first click only picks the line; the second one places the text.
        let placement = screen_of(&app, [0.0, -40.0]);
        assert!(app.sketch_click(viewport_rect(), placement, ClickMode::PLAIN));

        let sketch = app.sketch.as_ref().unwrap();
        let dimensions = sketch_ui::dimensions(sketch);
        assert_eq!(dimensions.len(), 1);
        assert!(
            (dimensions[0].value - 50.0).abs() < 1e-6,
            "{:?}",
            dimensions[0]
        );
        assert!(dimensions[0].driving);
        // The text went where the user clicked, not to a default offset.
        let placed = dimensions[0].text_position.expect("a placed text");
        assert!(
            (placed[0]).abs() < 1e-6 && (placed[1] + 40.0).abs() < 1e-6,
            "the text was placed at {placed:?}"
        );
    }

    /// Two lines dimensioned together become an angle, or a distance when they
    /// are parallel.
    #[test]
    fn the_dimension_tool_measures_between_two_lines() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        app.set_sketch_tool(SketchTool::Dimension);

        // The bottom side and the right side meet at a right angle.
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [0.0, -25.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [25.0, 0.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [40.0, -40.0]),
            ClickMode::PLAIN
        ));
        let dimensions = sketch_ui::dimensions(app.sketch.as_ref().unwrap());
        assert_eq!(dimensions.len(), 1);
        assert!(
            matches!(dimensions[0].kind, sketch_ui::DimensionKind::Angular { .. }),
            "two perpendicular lines should give an angle: {:?}",
            dimensions[0].kind
        );
        assert!((dimensions[0].value - 90.0).abs() < 1e-6);

        // Two parallel sides (bottom and top) give the distance between them.
        app.dimension_picks.clear();
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [0.0, -25.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [0.0, 25.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [30.0, 0.0]),
            ClickMode::PLAIN
        ));
        let dimensions = sketch_ui::dimensions(app.sketch.as_ref().unwrap());
        assert_eq!(dimensions.len(), 2);
        assert!(
            matches!(dimensions[1].kind, sketch_ui::DimensionKind::LineDistance { .. }),
            "parallel sides should give a distance: {:?}",
            dimensions[1].kind
        );
        assert!((dimensions[1].value - 50.0).abs() < 1e-6);
    }

    /// The select tool only consumes a click that lands on sketch geometry, so
    /// a click on empty space still picks the model.
    #[test]
    fn a_click_on_empty_sketch_space_is_left_to_the_model() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.sketch_draft.begin(SketchTool::Select);

        let empty = screen_of(&app, [0.0, 0.0]);
        assert!(!app.sketch_click(viewport_rect(), empty, ClickMode::PLAIN));

        app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        let line = app.sketch.as_ref().unwrap().lines[1].id;
        let on_the_line = screen_of(&app, [25.0, 0.0]);
        assert!(app.sketch_click(viewport_rect(), on_the_line, ClickMode::PLAIN));
        assert_eq!(app.sketch_selection, vec![line]);
    }

    /// With a sketch open the whole frame runs: plane, geometry, constraints,
    /// dimensions and the sketch strip, with no panic on the way.

    /// The extrude remembers the sketch it came from, and follows it when the
    /// sketch is edited.
    #[test]
    fn editing_a_sketch_updates_the_extrude_built_from_it() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 10.0, 5.0);
        app.confirm_sketch();
        let sketch_id = app.project.parts[0]
            .features
            .iter()
            .find(|feature| matches!(feature.kind, FeatureKind::Sketch { .. }))
            .expect("the sketch is in the tree")
            .id;
        app.extrude_sketch();

        let extrude = app.project.parts[0]
            .features
            .iter()
            .find(|feature| matches!(feature.kind, FeatureKind::Extrude { .. }))
            .expect("an extrude");
        let FeatureKind::Extrude {
            sketch, profile, ..
        } = &extrude.kind
        else {
            unreachable!()
        };
        assert_eq!(*sketch, Some(sketch_id), "the extrude does not name its sketch");
        let highest = profile
            .outer
            .iter()
            .map(|point| point[0])
            .fold(f64::MIN, f64::max);
        assert!((highest - 10.0).abs() < 1e-9, "{highest}");

        // Edit the sketch: replace the rectangle with a bigger one.
        app.edit_sketch_feature(sketch_id).expect("opens the sketch");
        {
            let mut sketch = app.sketch.take().expect("the sketch is open");
            sketch.points.clear();
            sketch.lines.clear();
            sketch.constraints.clear();
            sketch.constrained_constraints.clear();
            sketch.add_rectangle(0.0, 0.0, 40.0, 20.0);
            app.sketch = Some(sketch);
        }
        app.confirm_sketch();

        let extrude = app.project.parts[0]
            .features
            .iter()
            .find(|feature| matches!(feature.kind, FeatureKind::Extrude { .. }))
            .expect("the extrude is still there");
        let FeatureKind::Extrude {
            profile,
            placement,
            ..
        } = &extrude.kind
        else {
            unreachable!()
        };
        let highest = profile
            .outer
            .iter()
            .map(|point| point[0])
            .fold(f64::MIN, f64::max);
        assert!(
            (highest - 40.0).abs() < 1e-9,
            "the extrude kept the old profile: {highest}"
        );
        assert!(placement.is_some(), "the plane was not refreshed");
    }

    /// A sketch's lost plane reference can be substituted like any other.
    #[test]
    fn a_lost_sketch_reference_can_be_substituted() {
        let mut app = RcadApp::default();
        let _ = app.create_default_datum_geometry();
        let plane = app.project.parts[0]
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
            .expect("a datum plane");
        let plane_id = plane.id;
        let plane_name = plane.name.clone();
        app.selection.replace(Selected::Datum {
            feature: plane_id,
            label: plane_name,
        });
        app.new_sketch();
        app.add_sketch_rectangle(0.0, 0.0, 10.0, 5.0);
        app.confirm_sketch();
        let sketch_id = app.project.parts[0]
            .features
            .iter()
            .find(|feature| matches!(feature.kind, FeatureKind::Sketch { .. }))
            .expect("the sketch is in the tree")
            .id;

        assert!(app.substitute_reference(sketch_id, &plane_id.to_string(), "another-plane"));

        let replaced = app.project.parts[0]
            .features
            .iter()
            .find(|feature| feature.id == sketch_id)
            .and_then(|feature| match &feature.kind {
                FeatureKind::Sketch { sketch } => sketch
                    .reference(SketchReferenceRole::Orientation)
                    .and_then(|reference| reference.reference.clone()),
                _ => None,
            });
        assert_eq!(replaced, Some("another-plane".to_string()));
    }

    /// The tangent arc tool leaves a line end along the line.
    #[test]
    fn the_tangent_arc_tool_leaves_a_line_end() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        let endpoint = {
            let sketch = app.sketch.as_mut().expect("a sketch");
            let a = sketch.add_point(0.0, 0.0);
            let b = sketch.add_point(20.0, 0.0);
            sketch.add_line(a, b);
            b
        };
        app.set_sketch_tool(SketchTool::ArcTangent);
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [20.0, 0.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [30.0, 10.0]),
            ClickMode::PLAIN
        ));

        let sketch = app.sketch.as_ref().expect("a sketch");
        assert_eq!(sketch.arcs.len(), 1, "{:?}", app.last_error);
        let arc = &sketch.arcs[0];
        let centre = sketch.point(arc.center).expect("a centre");
        let start = sketch.point(arc.start_point).expect("a start");
        assert!((start.x - 20.0).abs() < 1e-6 && (start.y).abs() < 1e-6, "{start:?}");
        // Tangent along X means the radius at the start is vertical.
        assert!(
            (start.x - centre.x).abs() < 1e-6,
            "the arc is not tangent to the line at its start"
        );
        assert!(sketch.constraints.iter().any(|constraint| matches!(
            constraint,
            rcad_sketch::Constraint::Coincident { a, b }
                if (*a == arc.start_point && *b == endpoint)
                    || (*b == arc.start_point && *a == endpoint)
        )));
    }

    /// The conic tool draws a curve through the third click.
    #[test]
    fn the_conic_tool_draws_a_curve_through_three_clicks() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.set_sketch_tool(SketchTool::Conic);
        for point in [[0.0, 0.0], [20.0, 0.0], [10.0, 10.0]] {
            assert!(app.sketch_click(
                viewport_rect(),
                screen_of(&app, point),
                ClickMode::PLAIN
            ));
        }
        let sketch = app.sketch.as_ref().expect("a sketch");
        assert_eq!(sketch.conics.len(), 1, "{:?}", app.last_error);
        let middle = sketch.conic_points(sketch.conics[0].id, 20)[10];
        assert!((middle[0] - 10.0).abs() < 1e-6, "{middle:?}");
        assert!((middle[1] - 10.0).abs() < 1e-6, "{middle:?}");
    }

    /// The text tool types a note in place.
    #[test]
    fn the_text_tool_types_a_note() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        session.app.set_sketch_tool(SketchTool::Text);
        let at = screen_of(&session.app, [0.0, 0.0]);
        assert!(session.app.sketch_click(viewport_rect(), at, ClickMode::PLAIN));
        assert!(session.app.sketch_text_editor.is_some());
        session.frame();
        session.run_step(vec![egui::Event::Text("RCAD".to_string())]);
        session.run_step(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }]);
        session.frame();

        let sketch = session.app.sketch.as_ref().expect("a sketch");
        assert!(session.app.sketch_text_editor.is_none(), "Enter did not commit");
        assert_eq!(sketch.texts.len(), 1);
        assert_eq!(sketch.texts[0].content, "RCAD");
        assert!((sketch.texts[0].height - session.app.tool_params.text_height).abs() < 1e-9);
    }

    /// A DXF or SVG file imports into the open sketch.
    #[test]
    fn a_sketch_file_is_imported() {
        let root = temp_project_dir();
        std::fs::create_dir_all(&root).expect("a temp folder");
        let path = root.join("shape.svg");
        std::fs::write(
            &path,
            r#"<svg><line x1="0" y1="0" x2="10" y2="0"/><circle cx="5" cy="5" r="2"/></svg>"#,
        )
        .expect("write the file");
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);

        let message = app.import_sketch_file(&path).expect("import");

        assert!(message.contains("entities imported"), "{message}");
        let sketch = app.sketch.as_ref().expect("a sketch");
        assert_eq!(sketch.lines.len(), 1);
        assert_eq!(sketch.circles.len(), 1);
        assert!(app.sketch_undo.len() == 1, "the import is one undo step");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The constraint table lists every constraint, edits a value and deletes.
    #[test]
    fn the_constraint_table_lists_edits_and_deletes() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 50.0, 30.0);
        let (a, b) = {
            let sketch = app.sketch.as_ref().expect("a sketch");
            (sketch.lines[0].a, sketch.lines[0].b)
        };
        app.sketch
            .as_mut()
            .expect("a sketch")
            .add_dimension(a, b, 50.0, true, true);
        assert_eq!(app.constraint_table_rows().len(), 5);
        let index = app
            .sketch
            .as_ref()
            .expect("a sketch")
            .constraints
            .iter()
            .position(|constraint| matches!(constraint, rcad_sketch::Constraint::Distance { .. }))
            .expect("the dimension");
        assert!(
            app.constraint_table_rows()
                .iter()
                .any(|(label, value, _)| label.starts_with("Distance") && value.is_some())
        );

        assert!(app.set_sketch_constraint_value(index, 60.0));
        assert_eq!(
            app.sketch
                .as_ref()
                .expect("a sketch")
                .dimension_value(index),
            Some(60.0)
        );
        assert!(app.delete_sketch_constraint(index));
        assert_eq!(app.constraint_table_rows().len(), 4);
    }

    /// The dimension tool also measures radii, point-line and tangent distances.
    #[test]
    fn the_dimension_tool_measures_curves_points_and_tangents() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        {
            let sketch = app.sketch.as_mut().expect("a sketch");
            let a = sketch.add_point(-30.0, -10.0);
            let b = sketch.add_point(30.0, -10.0);
            sketch.add_line(a, b);
            sketch.add_circle(0.0, 10.0, 5.0);
            sketch.add_point(0.0, 0.0);
        }
        app.set_sketch_tool(SketchTool::Dimension);

        // A circle on its own becomes a radius dimension.
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [5.0, 10.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [20.0, 25.0]),
            ClickMode::PLAIN
        ));
        let dimensions = sketch_ui::dimensions(app.sketch.as_ref().expect("a sketch"));
        assert_eq!(dimensions.len(), 1, "{:?}", app.last_error);
        assert!(matches!(
            dimensions[0].kind,
            sketch_ui::DimensionKind::Radial { .. }
        ));
        assert_eq!(dimensions[0].text, "R5");

        // A line and a point: the perpendicular distance between them.
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [10.0, -10.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [0.0, 0.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [-20.0, 25.0]),
            ClickMode::PLAIN
        ));
        let dimensions = sketch_ui::dimensions(app.sketch.as_ref().expect("a sketch"));
        assert_eq!(dimensions.len(), 2);
        assert!(
            matches!(
                dimensions[1].kind,
                sketch_ui::DimensionKind::PointDistance { .. }
            ),
            "{:?}",
            dimensions[1].kind
        );
        assert!((dimensions[1].value - 10.0).abs() < 1e-6, "{:?}", dimensions[1]);

        // A line and a curve: the gap to the circle's tangent.
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [10.0, -10.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [5.0, 10.0]),
            ClickMode::PLAIN
        ));
        assert!(app.sketch_click(
            viewport_rect(),
            screen_of(&app, [40.0, 25.0]),
            ClickMode::PLAIN
        ));
        let dimensions = sketch_ui::dimensions(app.sketch.as_ref().expect("a sketch"));
        assert_eq!(dimensions.len(), 3);
        assert!(
            matches!(
                dimensions[2].kind,
                sketch_ui::DimensionKind::TangentDistance { .. }
            ),
            "{:?}",
            dimensions[2].kind
        );
        assert!((dimensions[2].value - 15.0).abs() < 1e-6, "{:?}", dimensions[2]);
    }

    /// The free-DOF highlight has entities to colour while the sketch is loose.
    #[test]
    fn the_free_dof_highlight_lists_the_loose_entities() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        session.app.add_sketch_rectangle(0.0, 0.0, 10.0, 10.0);
        session.app.show_free_dof = true;
        session.frame();
        session.frame();

        let sketch = session.app.sketch.as_ref().expect("a sketch");
        // Four points and the four lines that use them are all free.
        assert_eq!(sketch.find_unconstrained_entities().len(), 8);
    }

    #[test]
    fn a_sketch_frame_draws_the_overlay_and_the_panel() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        session.app.add_sketch_rectangle(-25.0, -25.0, 25.0, 25.0);
        session.app.sketch_draft.begin(SketchTool::Rectangle);

        session.frame();
        session.frame();

        let sketch = session.app.sketch.as_ref().expect("the sketch is open");
        assert_eq!(sketch.lines.len(), 4);
        assert_eq!(sketch.constraint_status(), "under-constrained");
    }




    /// The sketch feature, with its references, survives a save and a reload.
    #[test]
    fn a_sketch_feature_survives_a_save_and_reload() {
        let root = temp_project_dir();
        let mut app = RcadApp::default();
        app.open_project_folder(root.clone());
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 20.0, 10.0);
        app.confirm_sketch();

        let path = root.join("sketch.rcad");
        app.project.save(&path).expect("save");
        let loaded = rcad_doc::Project::load(&path).expect("load");
        let sketch = loaded.parts[0]
            .features
            .iter()
            .find_map(|feature| match &feature.kind {
                FeatureKind::Sketch { sketch } => Some(sketch),
                _ => None,
            })
            .expect("the sketch feature is in the document");
        assert_eq!(sketch.lines.len(), 4);
        assert_eq!(sketch.references.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Clicking anywhere else commits the inline value too.
    #[test]
    fn clicking_elsewhere_commits_the_inline_dimension() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        let corners = session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_rectangle(0.0, 0.0, 50.0, 30.0);
        let index = session.app.sketch.as_ref().unwrap().constraints.len();
        session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_dimension(corners[0], corners[1], 50.0, true, true);
        session
            .app
            .open_dimension_editor(index, egui::Pos2::new(200.0, 200.0));
        session.frame();
        session.run_step(vec![egui::Event::Text("80".to_string())]);

        // A click in the viewport, away from the field, commits it.
        let elsewhere = viewport_of(&session.app).center();
        session.click(elsewhere);
        session.frame();

        assert!(
            session.app.dimension_editor.is_none(),
            "clicking elsewhere must close the field"
        );
        assert_eq!(
            session.app.sketch.as_ref().unwrap().dimension_value(index),
            Some(80.0)
        );
    }

    /// A new sketch records the two references that position it: the
    /// orientation one from the selection and the automatic CYS one.
    #[test]
    fn a_new_sketch_records_its_two_positioning_references() {
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
            .unwrap();
        let plane_id = plane.id;
        let plane_name = plane.name.clone();
        app.selection.replace(Selected::Datum {
            feature: plane_id,
            label: plane_name.clone(),
        });

        app.new_sketch();

        let sketch = app.sketch.as_ref().expect("a sketch");
        assert_eq!(sketch.references.len(), 2);
        let orientation = sketch
            .reference(SketchReferenceRole::Orientation)
            .expect("the orientation reference");
        assert_eq!(orientation.label, plane_name);
        assert_eq!(orientation.reference, Some(plane_id.to_string()));
        let automatic = sketch
            .reference(SketchReferenceRole::Automatic)
            .expect("the automatic reference");
        assert_eq!(automatic.label, "CYS (origin)");
    }

    /// OK puts the sketch in the history; an empty sketch is not history.
    #[test]
    fn ok_saves_the_sketch_and_an_empty_sketch_is_not_saved() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.confirm_sketch();
        assert!(app.sketch.is_none(), "OK closes the session");
        assert!(
            !app.project.parts[0]
                .features
                .iter()
                .any(|feature| matches!(feature.kind, FeatureKind::Sketch { .. })),
            "an empty sketch must not enter the history"
        );

        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 10.0, 10.0);
        app.confirm_sketch();
        assert!(app.sketch.is_none());
        assert!(
            app.project.parts[0]
                .features
                .iter()
                .any(|feature| matches!(feature.kind, FeatureKind::Sketch { .. })),
            "the sketch with geometry should be in the history"
        );
    }

    /// Cancel leaves without saving anything.
    #[test]
    fn cancel_discards_the_sketch() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 10.0, 10.0);
        app.cancel_sketch();
        assert!(app.sketch.is_none());
        assert!(!app.project.parts[0]
            .features
            .iter()
            .any(|feature| matches!(feature.kind, FeatureKind::Sketch { .. })));
    }

    /// Editing a stored sketch and cancelling keeps the stored one untouched.
    #[test]
    fn cancelling_an_edit_keeps_the_stored_sketch() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 10.0, 10.0);
        app.confirm_sketch();
        let feature = app.project.parts[0]
            .features
            .iter()
            .find(|feature| matches!(feature.kind, FeatureKind::Sketch { .. }))
            .unwrap()
            .id;

        app.edit_sketch_feature(feature).expect("opens the sketch");
        assert_eq!(app.sketch_feature, Some(feature));
        app.sketch.as_mut().unwrap().add_circle(0.0, 0.0, 5.0);
        app.cancel_sketch();

        let FeatureKind::Sketch { sketch } = &app.project.parts[0]
            .features
            .iter()
            .find(|candidate| candidate.id == feature)
            .unwrap()
            .kind
        else {
            panic!("the sketch feature is gone");
        };
        assert_eq!(sketch.lines.len(), 4, "the stored sketch is untouched");
        assert!(sketch.circles.is_empty(), "the cancelled circle is gone");
    }

    /// A sketch feature double-clicked in the tree opens its editor.
    #[test]
    fn a_sketch_feature_opens_from_the_tree() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 10.0, 10.0);
        app.confirm_sketch();
        assert!(app.sketch.is_none());
        let feature = app.project.parts[0]
            .features
            .iter()
            .find(|feature| matches!(feature.kind, FeatureKind::Sketch { .. }))
            .unwrap()
            .id;

        app.handle_history_action(HistoryOverlayAction::EditDefinition(feature));

        assert!(app.sketch.is_some());
        assert_eq!(app.sketch_feature, Some(feature));
        assert_eq!(app.sketch.as_ref().unwrap().lines.len(), 4);
    }

    /// No other feature may start while a sketch is being edited.
    #[test]
    fn another_feature_is_blocked_while_a_sketch_is_edited() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        let before = app.project.parts[0].features.len();

        app.handle_toolbar_action(ToolbarAction::ModelBox);

        assert_eq!(app.project.parts[0].features.len(), before);
        assert!(app
            .last_error
            .as_deref()
            .unwrap_or("")
            .contains("Finish the sketch"));
        // The sketch tools still work.
        app.set_sketch_tool(SketchTool::Line);
        assert_eq!(app.sketch_draft.tool(), SketchTool::Line);
    }

    /// A polyline closes on its first point and leaves the tool by itself.
    #[test]
    fn a_polyline_in_the_viewport_closes_and_ends_the_tool() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.set_sketch_tool(SketchTool::LineChain);
        for point in [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0]] {
            let at = screen_of(&app, point);
            assert!(app.sketch_click(viewport_rect(), at, ClickMode::PLAIN));
        }
        // The cursor snaps onto the first point, so the loop closes exactly.
        let near_first = screen_of(&app, [0.4, 0.4]);
        assert!(app.sketch_click(viewport_rect(), near_first, ClickMode::PLAIN));

        assert_eq!(app.sketch.as_ref().unwrap().lines.len(), 3);
        assert_eq!(app.sketch_draft.tool(), SketchTool::Select);
    }

    /// The in-place dimension editor commits on Enter, with no window around it.
    #[test]
    fn the_inline_dimension_editor_commits_on_enter() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        let corners = session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_rectangle(0.0, 0.0, 50.0, 30.0);
        let index = session.app.sketch.as_ref().unwrap().constraints.len();
        session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_dimension(corners[0], corners[1], 50.0, true, true);
        session
            .app
            .open_dimension_editor(index, egui::Pos2::new(200.0, 200.0));
        // The field takes the keyboard on the first frame.
        session.frame();
        session.run_step(vec![egui::Event::Text("70".to_string())]);
        session.run_step(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }]);
        session.frame();

        assert_eq!(
            session.app.sketch.as_ref().unwrap().dimension_value(index),
            Some(70.0)
        );
        assert!(
            session.app.dimension_editor.is_none(),
            "Enter commits and closes the field"
        );
    }

    /// While a text field has the keyboard, shortcuts do not fire.
    #[test]
    fn shortcuts_do_not_fire_while_a_dimension_is_being_typed() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        let corners = session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_rectangle(0.0, 0.0, 50.0, 30.0);
        let index = session.app.sketch.as_ref().unwrap().constraints.len();
        session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_dimension(corners[0], corners[1], 50.0, true, true);
        session.app.set_sketch_tool(SketchTool::Select);
        session
            .app
            .open_dimension_editor(index, egui::Pos2::new(200.0, 200.0));
        session.frame();

        // "P" arms the Point tool normally; while typing it must not.
        session.run_step(vec![egui::Event::Key {
            key: egui::Key::P,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }]);

        assert_eq!(
            session.app.sketch_draft.tool(),
            SketchTool::Select,
            "a shortcut fired while the dimension was being typed"
        );
    }

    /// A dimension can be edited in place: the value changes and the geometry
    /// follows, because a driving dimension drives.
    #[test]
    fn a_dimension_can_be_edited_in_place() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 50.0, 30.0);
        app.set_sketch_tool(SketchTool::Dimension);
        let bottom = screen_of(&app, [25.0, 0.0]);
        assert!(app.sketch_click(viewport_rect(), bottom, ClickMode::PLAIN));
        // The second click places the text; the first only picked the line.
        let placement = screen_of(&app, [25.0, -20.0]);
        assert!(app.sketch_click(viewport_rect(), placement, ClickMode::PLAIN));

        let index = app
            .sketch
            .as_ref()
            .unwrap()
            .constraints
            .iter()
            .position(|constraint| matches!(constraint, rcad_sketch::Constraint::Distance { .. }))
            .expect("the click added a dimension");
        assert_eq!(
            app.sketch.as_ref().unwrap().dimension_value(index),
            Some(50.0)
        );

        app.open_dimension_editor(index, egui::Pos2::new(10.0, 10.0));
        assert_eq!(app.edited_dimension(), Some(index));
        app.apply_dimension_value(70.0);

        let sketch = app.sketch.as_ref().unwrap();
        assert_eq!(sketch.dimension_value(index), Some(70.0));
        let (first, second) = (sketch.lines[0].a, sketch.lines[0].b);
        let a = sketch.point(first).unwrap();
        let b = sketch.point(second).unwrap();
        let length = (b.x - a.x).hypot(b.y - a.y);
        assert!(
            (length - 70.0).abs() < 1e-6,
            "the geometry did not follow the edit: {length}"
        );
    }

    /// A dimension toggled to driven reports the geometry without moving it.
    #[test]
    fn a_dimension_can_be_made_a_reference() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 50.0, 30.0);
        let (a, b) = {
            let sketch = app.sketch.as_ref().unwrap();
            (sketch.lines[0].a, sketch.lines[0].b)
        };
        let index = app.sketch.as_ref().unwrap().constraints.len();
        app.sketch
            .as_mut()
            .unwrap()
            .add_dimension(a, b, 50.0, true, true);

        app.open_dimension_editor(index, egui::Pos2::new(10.0, 10.0));
        app.toggle_selected_dimension_driving();
        assert_eq!(
            app.sketch.as_ref().unwrap().dimension_is_driving(index),
            Some(false)
        );

        app.apply_dimension_value(80.0);
        let sketch = app.sketch.as_ref().unwrap();
        let pa = sketch.point(a).unwrap();
        let pb = sketch.point(b).unwrap();
        let length = (pb.x - pa.x).hypot(pb.y - pa.y);
        assert!(
            (length - 50.0).abs() < 1e-6,
            "a driven dimension moved the geometry: {length}"
        );
    }

    /// A weak dimension can be promoted, and a dimension can be deleted and
    /// brought back with Ctrl+Z.
    #[test]
    fn a_weak_dimension_can_be_promoted_and_deleted() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.add_sketch_rectangle(0.0, 0.0, 50.0, 30.0);
        let (a, b) = {
            let sketch = app.sketch.as_ref().unwrap();
            (sketch.lines[0].a, sketch.lines[0].b)
        };
        let index = app.sketch.as_ref().unwrap().constraints.len();
        app.sketch
            .as_mut()
            .unwrap()
            .add_dimension(a, b, 50.0, true, false);
        assert!(app.sketch.as_ref().unwrap().dimension_is_weak(index));

        app.open_dimension_editor(index, egui::Pos2::new(10.0, 10.0));
        app.promote_selected_dimension();
        assert!(!app.sketch.as_ref().unwrap().dimension_is_weak(index));

        app.delete_selected_dimension();
        assert!(
            !app.sketch
                .as_ref()
                .unwrap()
                .constraints
                .iter()
                .any(|constraint| matches!(constraint, rcad_sketch::Constraint::Distance { .. })),
            "the dimension is gone"
        );

        app.undo();
        assert!(
            app.sketch
                .as_ref()
                .unwrap()
                .constraints
                .iter()
                .any(|constraint| matches!(constraint, rcad_sketch::Constraint::Distance { .. })),
            "Ctrl+Z did not bring the dimension back"
        );
    }

    /// Point 5: Ctrl+Z steps back through the sketch, and redo forward again.
    #[test]
    fn ctrl_z_steps_back_through_the_sketch() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.set_sketch_tool(SketchTool::Rectangle);
        let first = screen_of(&app, [0.0, 0.0]);
        let second = screen_of(&app, [20.0, 10.0]);
        assert!(app.sketch_click(viewport_rect(), first, ClickMode::PLAIN));
        assert!(app.sketch_click(viewport_rect(), second, ClickMode::PLAIN));
        assert_eq!(app.sketch.as_ref().unwrap().lines.len(), 4);

        app.undo();
        assert_eq!(
            app.sketch.as_ref().unwrap().lines.len(),
            0,
            "the rectangle is undone"
        );
        assert_eq!(app.sketch.as_ref().unwrap().points.len(), 0);

        app.redo();
        assert_eq!(
            app.sketch.as_ref().unwrap().lines.len(),
            4,
            "and comes back with redo"
        );
    }

    /// Enter finishes the spline being drawn.
    #[test]
    fn enter_finishes_the_spline_being_drawn() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        app.set_sketch_tool(SketchTool::Spline);
        for point in [[0.0, 0.0], [10.0, 5.0], [20.0, 0.0]] {
            let at = screen_of(&app, point);
            assert!(app.sketch_click(viewport_rect(), at, ClickMode::PLAIN));
        }
        assert!(app.sketch.as_ref().unwrap().splines.is_empty(), "not finished yet");

        assert!(app.finish_sketch_shape());
        assert_eq!(app.sketch.as_ref().unwrap().splines.len(), 1);
        assert!(!app.finish_sketch_shape(), "nothing left to finish");
    }

    /// Point 4: selected edges and faces project into the sketch, and the
    /// projection is one more sketch edit Ctrl+Z can step over.
    #[test]
    fn selected_edges_and_faces_project_into_the_sketch() {
        let mut app = RcadApp::default();
        open_xy_sketch(&mut app);
        let mut edge = test_edge(
            "E:line:one",
            [0.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        );
        edge.points = vec![[0.0, 0.0, 0.0], [20.0, 0.0, 0.0]];
        app.mesh.edges = vec![edge];
        app.selection.replace(Selected::Edge {
            reference: "E:line:one".to_string(),
            label: "edge one".to_string(),
        });
        app.project_references_into_sketch()
            .expect("the edge projects");
        assert_eq!(app.sketch.as_ref().unwrap().reference_geometry.len(), 1);
        assert_eq!(
            app.sketch.as_ref().unwrap().reference_geometry[0]
                .projected_points
                .len(),
            2
        );

        // A triangle face projects its three boundary edges.
        open_xy_sketch(&mut app);
        app.mesh = two_face_mesh();
        app.selection.replace(Selected::Face {
            reference: TOP_FACE.to_string(),
            label: "top".to_string(),
            triangle: 0,
        });
        app.project_references_into_sketch()
            .expect("the face projects");
        assert_eq!(
            app.sketch.as_ref().unwrap().reference_geometry.len(),
            3,
            "a triangle has three boundary edges"
        );

        app.undo();
        assert!(
            app.sketch.as_ref().unwrap().reference_geometry.is_empty(),
            "Ctrl+Z did not remove the projection"
        );
    }

    /// Point 3: dragging a point moves it, with the snap, and Ctrl+Z restores
    /// it. The drag goes through real pointer events.
    #[test]
    fn dragging_a_sketch_point_moves_it_and_undo_puts_it_back() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        session.app.set_sketch_tool(SketchTool::Point);
        // The sketch strip claims its space one frame after it appears.
        session.frame();
        session.frame();
        let start = viewport_screen_of(&session.app, [10.0, 5.0]);
        session.click(start);
        assert_eq!(session.app.sketch.as_ref().unwrap().points.len(), 1);
        let point = session.app.sketch.as_ref().unwrap().points[0].id;

        session.app.set_sketch_tool(SketchTool::Select);
        session.frame();
        let from = viewport_screen_of(&session.app, [10.0, 5.0]);
        let to = viewport_screen_of(&session.app, [25.0, 15.0]);
        session.drag(from, to);

        let moved = session
            .app
            .sketch
            .as_ref()
            .unwrap()
            .point(point)
            .map(|point| [point.x, point.y])
            .unwrap();
        assert!(
            (moved[0] - 25.0).abs() < 0.6 && (moved[1] - 15.0).abs() < 0.6,
            "the point did not follow the drag: {moved:?}"
        );

        session.app.undo();
        let back = session
            .app
            .sketch
            .as_ref()
            .unwrap()
            .point(point)
            .map(|point| [point.x, point.y])
            .unwrap();
        assert!(
            (back[0] - 10.0).abs() < 0.6 && (back[1] - 5.0).abs() < 0.6,
            "Ctrl+Z did not put the point back: {back:?}"
        );
    }

    /// Point 1: a double-click on a dimension opens its in-place editor.
    #[test]
    fn double_clicking_a_dimension_opens_its_editor() {
        let mut session = Session::new(RcadApp::default());
        open_xy_sketch(&mut session.app);
        let corners = session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_rectangle(0.0, 0.0, 40.0, 30.0);
        let index = session.app.sketch.as_ref().unwrap().constraints.len();
        session
            .app
            .sketch
            .as_mut()
            .unwrap()
            .add_dimension(corners[0], corners[1], 40.0, true, true);
        // The new bottom strip claims its space on the frame after it first
        // appears, so settle two frames before reading screen positions.
        session.frame();
        session.frame();

        let annotation = sketch_ui::dimensions(session.app.sketch.as_ref().unwrap())
            .pop()
            .expect("a dimension");
        let text = sketch_ui::dimension_screen_geometry(
            viewport_of(&session.app),
            &session.app.camera,
            session.app.sketch_plane.expect("a plane"),
            &annotation,
        )
        .expect("the dimension is on screen")
        .text;
        session.double_click(text);

        assert_eq!(
            session.app.edited_dimension(),
            Some(index),
            "the double-click did not open the dimension editor"
        );
    }

    /// Opening projects remembers them, most recent first and without repeats.
    #[test]
    fn opening_projects_remembers_them_without_repeats() {
        let first = temp_project_dir();
        let second = temp_project_dir();
        let mut app = RcadApp::default();
        // The test is about the order it creates, not about the settings file.
        app.settings.recent_projects.clear();
        app.settings.last_project = None;
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
        FeatureKind::Sketch { .. } => "Sketch",
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
