use eframe::egui;
use crate::viewport::{draw_viewport, handle_picking, SelectionState};
use crate::camera::OrbitCamera;
use crate::undo::UndoStack;
use crate::shortcuts::{Shortcuts, handle_shortcuts, shortcuts_list};
use crate::ui::{VerticalToolbar, ToolbarAction, HistoryOverlay, HistoryOverlayAction};
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
}

impl RcadApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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
            // Front plane (XY)
            let front_plane_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "FRONT_PLANE",
                FeatureKind::Construction {
                    kind: ConstructionKind::Plane,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 0.0, 1.0], // Normal = Z
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
            
            // Top plane (XZ)
            let top_plane_id = Uuid::new_v4();
            part.features.push(Feature::new(
                "TOP_PLANE",
                FeatureKind::Construction {
                    kind: ConstructionKind::Plane,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 1.0, 0.0], // Normal = Y
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

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
                    if ui.button("Shaded").clicked() {
                        // TODO: Set shaded mode
                        ui.close();
                    }
                    if ui.button("Wireframe").clicked() {
                        // TODO: Set wireframe mode
                        ui.close();
                    }
                    if ui.button("Zoom Fit").clicked() {
                        // TODO: Zoom fit
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
        
        // 1) Vertical toolbar (left or right)
        let toolbar_position = self.toolbar.position;
        let toolbar_width = self.toolbar.width();
        
        if toolbar_position == crate::ui::ToolbarPosition::Left {
            egui::SidePanel::left("vertical_toolbar")
                .exact_width(toolbar_width)
                .resizable(false)
                .show(ctx, |ui| {
                    self.toolbar.show(ui, &mut |action| toolbar_action = Some(action));
                });
        } else {
            egui::SidePanel::right("vertical_toolbar")
                .exact_width(toolbar_width)
                .resizable(false)
                .show(ctx, |ui| {
                    self.toolbar.show(ui, &mut |action| toolbar_action = Some(action));
                });
        }
        
        // 2) Bottom panel: status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let eye = self.camera.eye();
                ui.label(format!("Camera: [{:.1}, {:.1}, {:.1}]", eye.x, eye.y, eye.z));
                ui.separator();
                match &self.last_error {
                    Some(e) => ui.colored_label(egui::Color32::LIGHT_RED, e),
                    None => ui.label("rcad - Open Source CAD in Rust"),
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
            
            let response = draw_viewport(ui, &self.mesh, &mut self.camera, None, self.selected_triangle, self.selected_construction_feature, part_clone.as_ref());
            
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
            
            // Handle async kernel worker responses
            if let Some(rx) = &self.kernel_worker_rx {
                while let Ok(response) = rx.try_recv() {
                    // Only apply if this is the latest version
                    if response.version >= self.kernel_version.load(Ordering::Relaxed) {
                        self.mesh = response.mesh;
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
        
        // Show relations dialog if requested
        if let Some(feature_id) = self.show_relations_dialog {
            let mut open = true;
            egui::Window::new("Relations & Parameters")
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
    
    fn draw_relations_dialog(&mut self, ui: &mut egui::Ui, feature_id: uuid::Uuid) {
        // Find the feature - clone data to avoid borrow issues
        let feature_name = {
            let part = self.get_active_part();
            part.and_then(|p| p.features.iter().find(|f| f.id == feature_id))
                .map(|f| (f.name.clone(), f.parameters.clone()))
        };
        
        if let Some((name, parameters)) = feature_name {
            ui.heading(format!("Relations & Parameters - {}", name));
            ui.separator();
            
            // Tab selector
            let mut tab = 0;
            ui.horizontal(|ui| {
                if ui.selectable_label(tab == 0, "Relations").clicked() { tab = 0; }
                if ui.selectable_label(tab == 1, "Parameters").clicked() { tab = 1; }
                if ui.selectable_label(tab == 2, "Programmatic").clicked() { tab = 2; }
            });
            ui.separator();
            
            match tab {
                0 => {
                    // Relations tab
                    ui.label("Equations and relations:");
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if parameters.string.is_empty() && parameters.numeric.is_empty() {
                            ui.label("No relations defined.");
                            ui.label("Click 'Add Relation' to create equations like: length = width * 2");
                        } else {
                            for (key, value) in &parameters.string {
                                ui.horizontal(|ui| {
                                    ui.monospace(key);
                                    ui.label("=");
                                    ui.monospace(value);
                                });
                            }
                            for (key, value) in &parameters.numeric {
                                ui.horizontal(|ui| {
                                    ui.monospace(key);
                                    ui.label("=");
                                    ui.monospace(format!("{:.3}", value));
                                });
                            }
                        }
                        
                        ui.separator();
                        if ui.button("Add Relation").clicked() {
                            self.last_error = Some("Add Relation - TODO: Implement relation editor".to_string());
                        }
                        if ui.button("Edit Relation").clicked() {
                            self.last_error = Some("Edit Relation - TODO".to_string());
                        }
                        if ui.button("Delete Relation").clicked() {
                            self.last_error = Some("Delete Relation - TODO".to_string());
                        }
                    });
                },
                1 => {
                    // Parameters tab
                    ui.label("Feature parameters:");
                    
                    // Use cloned parameters to avoid borrow issues
                    let mut params = parameters.clone();
                    let mut changed = false;
                    
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if params.numeric.is_empty() && params.string.is_empty() && params.boolean.is_empty() {
                            ui.label("No parameters defined.");
                        } else {
                            egui::Grid::new("params_grid").num_columns(3).show(ui, |ui| {
                                ui.strong("Name");
                                ui.strong("Value");
                                ui.strong("Type");
                                ui.end_row();
                                
                                // Collect keys first to avoid borrow issues
                                let numeric_keys: Vec<String> = params.numeric.keys().cloned().collect();
                                for name in numeric_keys {
                                    if let Some(value) = params.numeric.get_mut(&name) {
                                        ui.label(&name);
                                        let mut v = *value;
                                        if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                                            *value = v;
                                            changed = true;
                                        }
                                        ui.label("Real");
                                        ui.end_row();
                                    }
                                }
                                let string_keys: Vec<String> = params.string.keys().cloned().collect();
                                for name in string_keys {
                                    if let Some(value) = params.string.get(&name) {
                                        ui.label(&name);
                                        ui.label(value);
                                        ui.label("String");
                                        ui.end_row();
                                    }
                                }
                                let boolean_keys: Vec<String> = params.boolean.keys().cloned().collect();
                                for name in boolean_keys {
                                    if let Some(value) = params.boolean.get_mut(&name) {
                                        ui.label(&name);
                                        let mut v = *value;
                                        if ui.checkbox(&mut v, "").changed() {
                                            *value = v;
                                            changed = true;
                                        }
                                        ui.label("Boolean");
                                        ui.end_row();
                                    }
                                }
                            });
                        }
                    });
                    
                    // Apply changes back to feature if modified
                    if changed {
                        if let Some(part) = self.get_active_part_mut() {
                            if let Some(f) = part.features.iter_mut().find(|f| f.id == feature_id) {
                                f.parameters = params;
                            }
                        }
                    }
                    
                    ui.separator();
                    if ui.button("Add Parameter").clicked() {
                        self.last_error = Some("Add Parameter - TODO: Implement parameter editor".to_string());
                    }
                },
                2 => {
                    // Programmatic tab
                    ui.label("Programmatic behavior (Creo-style):");
                    ui.separator();
                    ui.label("This tab allows defining programmatic logic for the feature.");
                    ui.label("Examples: conditional suppression, pattern logic, family table driven values.");
                    ui.separator();
                    ui.label("Programmatic logic:");
                    
                    // Get programmatic logic value from cloned parameters
                    let mut programmatic = parameters.string.get("programmatic")
                        .cloned()
                        .unwrap_or_default();
                    let mut changed = false;
                    
                    if ui.text_edit_multiline(&mut programmatic).changed() {
                        changed = true;
                    }
                    
                    // Apply changes
                    if changed {
                        if let Some(part) = self.get_active_part_mut() {
                            if let Some(f) = part.features.iter_mut().find(|f| f.id == feature_id) {
                                f.parameters.string.insert("programmatic".to_string(), programmatic);
                            }
                        }
                    }
                    
                    ui.separator();
                    if ui.button("Edit Programmatic Logic").clicked() {
                        self.last_error = Some("Edit Programmatic - TODO: Implement code editor".to_string());
                    }
                },
                _ => {}
            }
            
            ui.separator();
            if ui.button("Close").clicked() {
                self.show_relations_dialog = None;
            }
        } else {
            ui.label("Feature not found");
            if ui.button("Close").clicked() {
                self.show_relations_dialog = None;
            }
        }
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
    
    pub fn set_view_front(&mut self) {
        self.camera.yaw = 0.0;
        self.camera.pitch = 0.0;
    }
    
    pub fn set_view_back(&mut self) {
        self.camera.yaw = std::f32::consts::PI;
        self.camera.pitch = 0.0;
    }
    
    pub fn set_view_left(&mut self) {
        self.camera.yaw = -std::f32::consts::PI / 2.0;
        self.camera.pitch = 0.0;
    }
    
    pub fn set_view_right(&mut self) {
        self.camera.yaw = std::f32::consts::PI / 2.0;
        self.camera.pitch = 0.0;
    }
    
    pub fn set_view_top(&mut self) {
        self.camera.yaw = 0.0;
        self.camera.pitch = std::f32::consts::PI / 2.0 - 0.01;
    }
    
    pub fn set_view_bottom(&mut self) {
        self.camera.yaw = 0.0;
        self.camera.pitch = -std::f32::consts::PI / 2.0 + 0.01;
    }
    
    pub fn set_view_iso(&mut self) {
        self.camera.yaw = 0.7;
        self.camera.pitch = 0.5;
    }
    
    pub fn reset_view(&mut self) {
        self.camera = OrbitCamera::default();
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
                }
                Err(e) => {
                    self.last_error = Some(format!("Failed to load project: {e}"));
                }
            }
        }
    }
    
    pub fn save_document(&mut self) {
        if let Some(path) = &self.current_path {
            if let Err(e) = self.project.save(path) {
                self.last_error = Some(format!("Failed to save project: {e}"));
            }
        } else {
            self.save_document_as();
        }
    }
    
    pub fn save_document_as(&mut self) {
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
                // TODO: Toggle perspective
            }
            ToolbarAction::ViewOrthographic => {
                // TODO: Toggle orthographic
            }
            ToolbarAction::ViewShaded => {
                // TODO: Set shaded mode
            }
            ToolbarAction::ViewWireframe => {
                // TODO: Set wireframe mode
            }
            ToolbarAction::ViewZoomFit => {
                // TODO: Zoom to fit
            }
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
                // TODO: Insert component
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
            
            ToolbarAction::AnalysisMeasure => {
                // TODO: Measure
            }
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
            ToolbarAction::ToolsOptions => {
                // TODO: Options dialog
            }
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
        }
    }
}

impl eframe::App for RcadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update(ctx, _frame);
    }
}