use eframe::egui;
use eframe::egui::{CentralPanel, Color32, ComboBox, MenuBar, SidePanel, TopBottomPanel, Ui, WidgetText};
use crate::viewport::draw_viewport;
use crate::camera::OrbitCamera;
use rcad_geom::{TriangleMesh, GeometryBackend, Profile2D};
use rcad_doc::{self, Project};
use rcad_features;
use rcad_occt::OcctBackend;
use rcad_sketch::{Sketch, SketchPoint, SketchLine, SketchCircle, Constraint};
use glam::DVec3;
use std::path::{Path, PathBuf};
use rfd::FileDialog;
use uuid::Uuid;

pub struct RcadApp {
    camera: OrbitCamera,
    mesh: TriangleMesh,
    selected_triangle: Option<usize>,
    project: Project,
    current_path: Option<PathBuf>,
    occt_backend: OcctBackend,
    sketch: Option<Sketch>,
    last_error: Option<String>,
    active_tab: String,
}

impl RcadApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut this = Self {
            camera: OrbitCamera::default(),
            mesh: TriangleMesh::default(),
            selected_triangle: None,
            project: Project::default(),
            current_path: None,
            occt_backend: OcctBackend::default(),
            sketch: None,
            last_error: None,
            active_tab: "sketch".to_string(),
        };
        // Create a simple box to demonstrate OCCT integration
        let _ = this.create_box();
        this
    }

    fn create_box(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let solid = self.occt_backend.box_solid(DVec3::new(10.0, 10.0, 10.0))?;
        let mesh = self.occt_backend.tessellate(solid)?;
        self.mesh = mesh;
        Ok(())
    }

    fn export_stl(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // For now, we'll just export a placeholder - in a real implementation,
        // we would use OCCT to write STL from the current solid
        std::fs::write(path, "solid rcad_export\n  facet normal 0.0 0.0 0.0\n    outer loop\n      vertex 0.0 0.0 0.0\n      vertex 0.0 0.0 0.0\n      vertex 0.0 0.0 0.0\n    endloop\n  endfacet\nendsolid rcad_export\n")?;
        Ok(())
    }

    fn export_step(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // For now, we'll just export a placeholder - in a real implementation,
        // we would use OCCT to write STEP from the current solid
        std::fs::write(path, "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('rcad STEP export'),'1');\nFILE_NAME('rcad_export.step','2023-01-01T00:00:00',(''),(''),'rcad','rcad');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN')); \nENDSEC;\nDATA;\nENDSEC;\nENDSEC;\nEND-ISO-10303-21;")?;
        Ok(())
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1) Top panel: menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                self.file_menu(ui, ctx);
                self.help_menu(ui);
            });
        });

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

        // 3) Left panel: tabs
        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Sketch").on_hover_text("Create and edit 2D sketches").clicked() {
                    self.active_tab = "sketch".to_string();
                }
                if ui.button("Features").on_hover_text("Manage feature tree").clicked() {
                    self.active_tab = "features".to_string();
                }
            });
            if self.active_tab == "sketch" {
                self.sketch_panel(ui);
            } else {
                self.feature_tree_panel(ui);
            }
        });

        // 4) Central panel: viewport
        egui::CentralPanel::default().show(ctx, |ui| {
            let response = draw_viewport(ui, &self.mesh, &mut self.camera, None, self.selected_triangle);
            if response.clicked() {
                self.selected_triangle = (!self.mesh.is_empty()).then_some(0); // TODO: picking real
            }
        });
    }

    fn sketch_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sketch");
        if ui.button("New Sketch").clicked() {
            self.sketch = Some(Sketch::new("Sketch1"));
            self.last_error = None;
        }

        // Handle adding points, lines, rectangle (these mutate sketch)
        if let Some(sketch) = &mut self.sketch {
            if ui.button("Add Point").clicked() {
                sketch.add_point(0.0, 0.0);
            }
            if ui.button("Add Line").clicked() && sketch.points.len() >= 2 {
                let n = sketch.points.len();
                let (a, b) = (sketch.points[n - 2].id, sketch.points[n - 1].id);
                sketch.add_line(a, b);
            }
            if ui.button("Add Rectangle").clicked() {
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
            if let Err(e) = self.extrude_sketch(profile, 10.0) {
                self.last_error = Some(format!("Extrude failed: {e}"));
            }
        }
        if let Some(profile) = profile_to_revolve {
            if let Err(e) = self.revolve_sketch(profile, 360.0) {
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
        if let Some(part) = self.get_active_part_mut() {
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
        } else {
            ui.label("No active part");
        }
    }

    fn get_active_part_mut(&mut self) -> Option<&mut rcad_doc::Part> {
        // For now, just use the first part
        self.project.parts.get_mut(0)
    }

    fn get_active_part(&self) -> Option<&rcad_doc::Part> {
        self.project.parts.get(0)
    }

    fn rebuild_active_part(&mut self) {
        if let Some(part) = self.get_active_part() {
            let mut backend = rcad_occt::OcctBackend::default();
            let result = rcad_param::rebuild_part(part, &mut backend);
            self.mesh = result.display_mesh;
            if !result.errors.is_empty() {
                self.last_error = Some(result.errors.join("\n"));
            }
        }
    }

    fn extrude_sketch(&mut self, profile: rcad_geom::Profile2D, height: f64) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(part) = self.get_active_part_mut() {
            rcad_features::add_extrude_from_sketch(part, "Extrude", &Sketch::new(""), height);
            // TODO: Actually use the profile from sketch - this is a placeholder
            // For now, we'll just add a box to demonstrate
            rcad_features::add_extrude_rect(part, "Extrude", 10.0, 10.0, height);
            self.rebuild_active_part();
        }
        Ok(())
    }

    fn revolve_sketch(&mut self, profile: rcad_geom::Profile2D, angle_deg: f64) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(part) = self.get_active_part_mut() {
            rcad_features::add_revolve(part, "Revolve", profile, angle_deg);
            self.rebuild_active_part();
        }
        Ok(())
    }

    fn add_box_feature(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(part) = self.get_active_part_mut() {
            rcad_features::add_box(part, "Box", [10.0, 10.0, 10.0]);
            self.rebuild_active_part();
        }
        Ok(())
    }

    fn file_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button("File", |ui| {
            if ui.button("New").clicked() {
                *self = Self::default();
                // Create a simple box to demonstrate OCCT integration
                let _ = self.create_box();
                ui.close_menu();
            }
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
                if let Some(path) = FileDialog::new().add_filter("stl", &["stl"]).save_file() {
                    if let Err(e) = self.export_stl(&path) {
                        self.last_error = Some(format!("Failed to export STL: {e}"));
                    }
                }
                ui.close_menu();
            }
            if ui.button("Export STEP").clicked() {
                if let Some(path) = FileDialog::new().add_filter("step", &["step"]).save_file() {
                    if let Err(e) = self.export_step(&path) {
                        self.last_error = Some(format!("Failed to export STEP: {e}"));
                    }
                }
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
        });
    }
}

impl Default for RcadApp {
    fn default() -> Self {
        Self {
            camera: OrbitCamera::default(),
            mesh: TriangleMesh::default(),
            selected_triangle: None,
            project: Project::default(),
            current_path: None,
            occt_backend: OcctBackend::default(),
            sketch: None,
            last_error: None,
            active_tab: "sketch".to_string(),
        }
    }
}

impl eframe::App for RcadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update(ctx, _frame);
    }
}