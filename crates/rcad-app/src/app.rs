use eframe::egui;
use crate::viewport::{draw_viewport, handle_picking, SelectionState};
use crate::camera::{OrbitCamera, Projection};
use crate::settings::AppSettings;
use crate::undo::UndoStack;
use crate::shortcuts::{Shortcuts, handle_shortcuts, shortcuts_list};
use crate::ui::{
    DatumKind, FloatingAction, FloatingBar, HistoryOverlay, HistoryOverlayAction, ToolbarAction,
    VerticalToolbar,
};
use rcad_geom::{GeometryBackend, TriangleMesh};
use rcad_doc::{Project, Feature, FeatureKind, ConstructionKind, Part};
use rcad_features;
use rcad_occt::OcctBackend;
use rcad_param::{ShapeCache};
use rcad_sketch::Sketch;
use std::path::{Path, PathBuf};
use rfd::FileDialog;
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
    selected_triangle: Option<usize>,
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
    
    // Selection state
    selected_feature: Option<uuid::Uuid>,
    selected_construction_feature: Option<usize>, // Feature index for construction geometry
    selection_mode: SelectionMode,
    
    // Async kernel worker
    kernel_worker_tx: Option<Sender<RebuildJob>>,
    kernel_worker_rx: Option<Receiver<RebuildResponse>>,
    kernel_version: Arc<AtomicU64>,
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
    show_settings: bool,    /// Informational message for the status bar (measurement results etc).
    status: Option<String>,
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
            selected_triangle: None,
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
            selected_feature: None,
            selected_construction_feature: None,
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
            show_settings: false,            status: None,
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
            egui::menu::bar(ui, |ui| {
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
            });
        });
        
        // 3) Central panel: viewport + history overlay
        egui::CentralPanel::default().show(ctx, |ui| {
            // Viewport with picking
            let part_clone = self.get_active_part().cloned();
            
            let mut selection_state = SelectionState::default();
            // Initialize selection state with current selection to preserve history tree selection
            selection_state.selected_construction = self.selected_construction_feature;
            selection_state.selected_triangle = self.selected_triangle;
            
            let response = draw_viewport(
                ui,
                &self.mesh,
                &mut self.camera,
                None,
                self.selected_triangle,
                self.selected_construction_feature,
                part_clone.as_ref(),
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
            
            // Handle viewport picking
            if let Some(part) = part_clone.as_ref() {
                handle_picking(ui, response.rect, &self.mesh, &self.camera, &mut selection_state, part);
            }
            
            // Update selected triangle from selection state
            // Only update if viewport actually picked something (not None from empty click)
            if selection_state.selected_triangle != self.selected_triangle {
                self.selected_triangle = selection_state.selected_triangle;
            }
            
            // Update selected construction feature from selection state
            // Only update if viewport picked a new construction feature, otherwise preserve history tree selection
            if selection_state.selected_construction != self.selected_construction_feature {
                self.selected_construction_feature = selection_state.selected_construction;
            }
            
            // Sync viewport selection to history overlay (only when viewport initiates selection)
            if let Some(feat_idx) = selection_state.selected_construction {
                if self.selected_construction_feature != Some(feat_idx) || self.selected_feature.is_none() {
                    if let Some(part) = part_clone.as_ref() {
                        if let Some(feature) = part.features.get(feat_idx) {
                            self.history_overlay.select_feature(Some(feature.id));
                            self.selected_feature = Some(feature.id);
                        }
                    }
                }
            } else if selection_state.selected_triangle.is_some() && self.selected_triangle != selection_state.selected_triangle {
                // Triangle selected via viewport - could map to feature, but for now just clear construction selection
                self.history_overlay.select_feature(None);
                self.selected_feature = None;
            }
            // Don't clear selection when clicking empty space if history tree has a construction feature selected
            // The history tree selection should persist until explicitly changed
            
            // History overlay (right side, over viewport)
            if self.history_overlay.visible {
                if let Some(part) = part_clone {
                    self.history_overlay.show(ui, &part, &mut |action| history_action = Some(action));
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

    /// Options dialog: the settings that survive a restart.
    fn draw_settings_dialog(&mut self, ui: &mut egui::Ui) {
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

    
    fn sketch_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sketch");
        if ui.button("New Sketch").clicked() {
            self.undo_stack.push(self.project.clone());
            self.sketch = Some(Sketch::new("Sketch1"));
            self.last_error = None;
        }

        // Handle adding points, lines, rectangle (these mutate sketch)
        if let Some(sketch) = &mut self.sketch {
            if ui.button("Add Point").clicked() {
                self.undo_stack.push(self.project.clone());
                sketch.add_point(0.0, 0.0);
            }
            if ui.button("Add Line").clicked() && sketch.points.len() >= 2 {
                self.undo_stack.push(self.project.clone());
                let n = sketch.points.len();
                let (a, b) = (sketch.points[n - 2].id, sketch.points[n - 1].id);
                sketch.add_line(a, b);
            }
            if ui.button("Add Rectangle").clicked() {
                self.undo_stack.push(self.project.clone());
                sketch.add_rectangle(-25.0, -25.0, 25.0, 25.0);
            }
            if ui.button("Solve Constraints").clicked() {
                if let Err(e) = sketch.solve() {
                    self.last_error = Some(format!("Constraints: {e}"));
                }
            }
        }

        // Handle extrude and revolve (need profile from sketch, but call self methods)
        let mut profile_to_extrude = None;
        let mut profile_to_revolve = None;
        if let Some(sketch) = &self.sketch {
            if ui.button("Extrude Sketch").clicked() {
                profile_to_extrude = sketch.to_profile();
            }
            if ui.button("Revolve Sketch").clicked() {
                profile_to_revolve = sketch.to_profile();
            }
        }

        if let Some(profile) = profile_to_extrude {
            if let Err(e) = self.extrude_sketch_internal(profile, 10.0) {
                self.last_error = Some(format!("Extrude failed: {e}"));
            }
        }
        if let Some(profile) = profile_to_revolve {
            if let Err(e) = self.revolve_sketch_internal(profile, 360.0) {
                self.last_error = Some(format!("Revolve failed: {e}"));
            }
        }

        ui.separator();
        if let Some(sketch) = &self.sketch {
            ui.label(format!("Current sketch: {}", sketch.name));
            ui.label(format!("Points: {}", sketch.points.len()));
            ui.label(format!("Lines: {}", sketch.lines.len()));
            ui.label(format!("Circles: {}", sketch.circles.len()));
            ui.label(format!("Constraints: {}", sketch.constraints.len()));
        }
    }

    fn feature_tree_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Feature Tree");
        
        // First, collect what needs to be changed without mutating
        let (suppress_these, unsuppress_these) = if let Some(part) = self.get_active_part_mut() {
            let mut suppress_these = Vec::new();
            let mut unsuppress_these = Vec::new();

            for i in 0..part.features.len() {
                let feature = &part.features[i];
                ui.horizontal(|ui| {
                    let label = if feature.suppressed {
                        format!("☐ {} (suppressed)", feature.name)
                    } else {
                        format!("☑ {}", feature.name)
                    };
                    if ui.selectable_label(false, label).clicked() {
                        // TODO: feature editing
                    }
                    if ui.button("Suppress").clicked() {
                        if !feature.suppressed {
                            suppress_these.push(i);
                        }
                    }
                    if feature.suppressed && ui.button("Unsuppress").clicked() {
                        unsuppress_these.push(i);
                    }
                });
            }
            
            (suppress_these, unsuppress_these)
        } else {
            ui.label("No active part");
            return;
        };
        
        // Now we can safely clone and modify
        if !suppress_these.is_empty() || !unsuppress_these.is_empty() {
            self.undo_stack.push(self.project.clone());
        }

        if let Some(part) = self.get_active_part_mut() {
            for i in &suppress_these {
                let feature = &mut part.features[*i];
                if !feature.suppressed {
                    feature.suppressed = true;
                    part.suppressed.push(feature.id);
                }
            }
            for i in &unsuppress_these {
                let feature = &mut part.features[*i];
                if feature.suppressed {
                    feature.suppressed = false;
                    part.suppressed.retain(|id| *id != feature.id);
                }
            }

            if !suppress_these.is_empty() || !unsuppress_these.is_empty() {
                self.rebuild_active_part();
            }
        }
    }

    fn feature_tree_with_sketch_panel(&mut self, ui: &mut egui::Ui) {
        // Sketch section (if active sketch exists)
        // First collect sketch info without holding borrow across closure
        let sketch_info = if let Some(sketch) = &self.sketch {
            Some((
                sketch.name.clone(),
                sketch.points.len(),
                sketch.lines.len(),
                sketch.circles.len(),
                sketch.constraints.len(),
                sketch.points.len() >= 2, // can add line
            ))
        } else {
            None
        };
        
        if let Some((name, points_len, lines_len, circles_len, constraints_len, can_add_line)) = sketch_info {
            ui.collapsing("Sketch", |ui| {
                ui.label(format!("Name: {}", name));
                ui.label(format!("Points: {}", points_len));
                ui.label(format!("Lines: {}", lines_len));
                ui.label(format!("Circles: {}", circles_len));
                ui.label(format!("Constraints: {}", constraints_len));
                ui.separator();
                
                // Sketch editing buttons with icons
                ui.horizontal(|ui| {
                    if ui.button("➕ Point").clicked() {
                        self.undo_stack.push(self.project.clone());
                        if let Some(s) = &mut self.sketch {
                            s.add_point(0.0, 0.0);
                        }
                    }
                    if ui.button("📏 Line").clicked() && can_add_line {
                        self.undo_stack.push(self.project.clone());
                        if let Some(s) = &mut self.sketch {
                            let n = s.points.len();
                            let (a, b) = (s.points[n - 2].id, s.points[n - 1].id);
                            s.add_line(a, b);
                        }
                    }
                    if ui.button("▭ Rect").clicked() {
                        self.undo_stack.push(self.project.clone());
                        if let Some(s) = &mut self.sketch {
                            s.add_rectangle(-25.0, -25.0, 25.0, 25.0);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("✅ Solve").clicked() {
                        if let Some(s) = &mut self.sketch {
                            if let Err(e) = s.solve() {
                                self.last_error = Some(format!("Constraints: {e}"));
                            }
                        }
                    }
                    if ui.button("⬆ Extrude").clicked() {
                        if let Some(s) = &self.sketch {
                            if let Some(profile) = s.to_profile() {
                                if let Err(e) = self.extrude_sketch_internal(profile, 10.0) {
                                    self.last_error = Some(format!("Extrude failed: {e}"));
                                }
                            }
                        }
                    }
                    if ui.button("🔄 Revolve").clicked() {
                        if let Some(s) = &self.sketch {
                            if let Some(profile) = s.to_profile() {
                                if let Err(e) = self.revolve_sketch_internal(profile, 360.0) {
                                    self.last_error = Some(format!("Revolve failed: {e}"));
                                }
                            }
                        }
                    }
                });
                ui.separator();
            });
        } else {
            // No active sketch - show "New Sketch" button
            if ui.button("📝 New Sketch").clicked() {
                self.new_sketch();
            }
            ui.separator();
        }
        
        // Feature tree section - collect feature data first to avoid borrow issues
        let features_data = if let Some(part) = self.get_active_part_mut() {
            part.features.iter().enumerate().map(|(i, f)| {
                (i, f.id, f.name.clone(), f.suppressed)
            }).collect::<Vec<_>>()
        } else {
            vec![]
        };
        
        ui.strong("Feature Tree");
        if features_data.is_empty() && self.get_active_part().is_none() {
            ui.label("No active part");
        } else {
            for (i, feature_id, name, suppressed) in features_data {
                let mut suppress_action = false;
                let mut unsuppress_action = false;
                
                ui.horizontal(|ui| {
                    let icon = if suppressed { "🚫" } else { "✅" };
                    let label = format!("{} {}", icon, name);
                    if ui.selectable_label(false, label).clicked() {
                        // TODO: feature editing
                    }
                    if ui.small_button(if suppressed { "⬆" } else { "🚫" })
                        .on_hover_text(if suppressed { "Unsuppress" } else { "Suppress" })
                        .clicked()
                    {
                        if suppressed {
                            unsuppress_action = true;
                        } else {
                            suppress_action = true;
                        }
                    }
                });
                
                // Apply actions after UI to avoid borrow issues
                if suppress_action || unsuppress_action {
                    self.undo_stack.push(self.project.clone());
                    if let Some(part) = self.get_active_part_mut() {
                        if let Some(feature) = part.features.get_mut(i) {
                            if suppress_action {
                                feature.suppressed = true;
                                part.suppressed.push(feature.id);
                            } else if unsuppress_action {
                                feature.suppressed = false;
                                part.suppressed.retain(|id| *id != feature.id);
                            }
                        }
                        self.rebuild_active_part();
                    }
                }
            }
            
            // Add feature buttons
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("📦 Box").clicked() {
                    let _ = self.add_box_feature_internal();
                }
                if ui.button("📝 Sketch").clicked() {
                    self.new_sketch();
                }
            });
        }
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

    fn file_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button("File", |ui| {
            if ui.button("New").clicked() {
                self.undo_stack.push(self.project.clone());
                *self = Self::default();
                // Create default datum geometry (coordinate system + datum planes)
                let _ = self.create_default_datum_geometry();
                ui.close_menu();
            }
            if ui.button("Undo").clicked() {
                if self.undo_stack.undo(&mut self.project) {
                    self.rebuild_active_part();
                }
                ui.close_menu();
            }
            if ui.button("Redo").clicked() {
                if self.undo_stack.redo(&mut self.project) {
                    self.rebuild_active_part();
                }
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Open").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    match Project::load(&path) {
                        Ok(project) => {
                            self.project = project;
                            self.current_path = Some(path);
                            // TODO: Load mesh from project
                            self.last_error = None;
                        }
                        Err(e) => {
                            self.last_error = Some(format!("Failed to load project: {e}"));
                        }
                    }
                }
                ui.close_menu();
            }
            if ui.button("Save").clicked() {
                if let Some(path) = &self.current_path {
                    if let Err(e) = self.project.save(path) {
                        self.last_error = Some(format!("Failed to save project: {e}"));
                    }
                } else {
                    // TODO: Implement Save As
                    if let Some(path) = FileDialog::new().save_file() {
                        if let Err(e) = self.project.save(&path) {
                            self.last_error = Some(format!("Failed to save project: {e}"));
                        } else {
                            self.current_path = Some(path);
                            self.last_error = None;
                        }
                    }
                }
                ui.close_menu();
            }
            if ui.button("Export STL").clicked() {
                self.export_stl();
                ui.close_menu();
            }
            if ui.button("Export STEP").clicked() {
                self.export_step();
                ui.close_menu();
            }
            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn help_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Help", |ui| {
            if ui.button("About").clicked() {
                // TODO: Show about dialog
                ui.close_menu();
            }
            if ui.button("Shortcuts").clicked() {
                self.show_shortcuts_dialog = true;
                ui.close_menu();
            }
        });
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
        self.undo_stack.push(self.project.clone());
        *self = Self::default();
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
    
    pub fn new_sketch(&mut self) {
        self.undo_stack.push(self.project.clone());
        
        // Check if there's a selected feature (plane, face, etc.) for sketch placement
        // If not, prompt user to select a planar face, datum plane, or create on default plane
        if self.selected_feature.is_none() {
            self.last_error = Some("Select a planar face or datum plane for sketch placement (left-click to select), then click 'New Sketch' again. Or click 'New Sketch' again to create on default XY plane.".to_string());
            // Create sketch in placement mode - waiting for plane selection
            self.sketch = Some(rcad_sketch::Sketch::new("Sketch1"));
            self.sketch.as_mut().unwrap().in_placement_mode = true;
        } else {
            // Use the selected feature as sketch plane
            self.sketch = Some(rcad_sketch::Sketch::new("Sketch1"));
            self.last_error = Some("Sketch created on selected plane. Choose vertical orientation (edge/axis) for sketch coordinate system.".to_string());
        }
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
        self.selected_triangle = None;
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
            HistoryOverlayAction::SelectFeature(feature_id) => {
                self.history_overlay.select_feature(Some(feature_id));
                self.selected_feature = Some(feature_id);
                
                // Sync to viewport: find feature index and update selected_construction_feature
                if let Some(part) = self.get_active_part() {
                    if let Some(feat_idx) = part.features.iter().position(|f| f.id == feature_id) {
                        if matches!(part.features[feat_idx].kind, FeatureKind::Construction { .. }) {
                            self.selected_construction_feature = Some(feat_idx);
                        }
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
            HistoryOverlayAction::PasteSpecialFeature(feature_id) => {
                // TODO: Paste special
            }
            HistoryOverlayAction::ShowParents(feature_id) => {
                self.history_overlay.show_parents = true;
                self.history_overlay.show_children = false;
                // TODO: Highlight parents
            }
            HistoryOverlayAction::ShowChildren(feature_id) => {
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
            selected_triangle: None,
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
            selected_feature: None,
            selected_construction_feature: None,
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
            show_settings: false,            status: None,
        }
    }
}

impl eframe::App for RcadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
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
        fn new(app: RcadApp) -> Self {
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
