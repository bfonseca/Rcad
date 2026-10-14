//! Vertical toolbar implementation for rcad - Creo-style ribbon replacement

use eframe::egui;
// use std::collections::HashMap; // Currently unused

/// Toolbar action types that map to app methods
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolbarAction {
    // File actions
    NewDocument,
    OpenDocument,
    SaveDocument,
    SaveDocumentAs,
    ExportStl,
    ExportStep,
    Undo,
    Redo,
    
    // View actions
    ViewFront,
    ViewBack,
    ViewLeft,
    ViewRight,
    ViewTop,
    ViewBottom,
    ViewIso,
    ViewReset,
    ViewPerspective,
    ViewOrthographic,
    ViewShaded,
    ViewWireframe,
    ViewZoomFit,
    ViewPrevious,
    
    // Model actions
    ModelBox,
    ModelExtrude,
    ModelRevolve,
    ModelSweep,
    ModelLoft,
    ModelBoundary,
    ModelFillet,
    ModelChamfer,
    ModelShell,
    ModelDraft,
    ModelHole,
    ModelPattern,
    ModelMirror,
    ModelBoolean,
    ModelCosmeticThread,
    
    // Sketch actions
    SketchNew,
    SketchLine,
    SketchRectangle,
    SketchCircle,
    SketchArc,
    SketchSpline,
    SketchPoint,
    SketchConstraintHorizontal,
    SketchConstraintVertical,
    SketchConstraintParallel,
    SketchConstraintPerpendicular,
    SketchConstraintTangent,
    SketchConstraintEqual,
    SketchConstraintSymmetric,
    SketchConstraintConcentric,
    SketchConstraintMidpoint,
    SketchConstraintCoincident,
    SketchDimension,
    SketchTrim,
    SketchExtend,
    SketchOffset,
    SketchMirror,
    SketchPattern,
    SketchProjectReferences,
    
    // Assembly actions
    AssemblyInsertComponent,
    AssemblyMateCoincident,
    AssemblyMateConcentric,
    AssemblyMateDistance,
    AssemblyMateAngle,
    AssemblyMateParallel,
    AssemblyMatePerpendicular,
    AssemblyMateTangent,
    AssemblyMateSymmetric,
    AssemblyExplodedView,
    AssemblySubAssembly,
    
    // Analysis actions
    AnalysisMeasure,
    AnalysisMassProperties,
    AnalysisCurvature,
    AnalysisDraft,
    AnalysisThickness,
    AnalysisInterference,
    AnalysisClearance,
    
    // Tools actions
    ToolsParameters,
    ToolsRelations,
    ToolsFamilyTable,
    ToolsDesignExploration,
    ToolsOptions,
    ToolsCustomize,
    
    // Help
    HelpAbout,
    HelpShortcuts,
}

/// Toolbar button definition
#[derive(Debug, Clone)]
pub struct ToolbarButton {
    pub id: String,
    pub icon: &'static str,
    pub label: String,
    pub action: ToolbarAction,
    pub shortcut: Option<String>,
    pub tooltip: Option<String>,
}

impl ToolbarButton {
    pub fn new(id: impl Into<String>, icon: &'static str, label: impl Into<String>, action: ToolbarAction) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            action,
            shortcut: None,
            tooltip: None,
        }
    }
    
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
    
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// Toolbar tab definition
#[derive(Debug, Clone)]
pub struct ToolbarTab {
    pub name: String,
    pub icon: &'static str,
    pub buttons: Vec<ToolbarButton>,
}

impl ToolbarTab {
    pub fn new(name: impl Into<String>, icon: &'static str) -> Self {
        Self {
            name: name.into(),
            icon,
            buttons: Vec::new(),
        }
    }
    
    pub fn add_button(mut self, button: ToolbarButton) -> Self {
        self.buttons.push(button);
        self
    }
    
    pub fn add_buttons(mut self, buttons: Vec<ToolbarButton>) -> Self {
        self.buttons.extend(buttons);
        self
    }
}

/// Vertical toolbar state
#[derive(Debug, Clone)]
pub struct VerticalToolbar {
    pub collapsed: bool,
    pub icon_only_mode: bool,
    pub position: ToolbarPosition,
    pub tabs: Vec<ToolbarTab>,
    pub active_tab: usize,
    pub hover_tab: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarPosition {
    Left,
    Right,
}

impl Default for VerticalToolbar {
    fn default() -> Self {
        Self {
            collapsed: false,
            icon_only_mode: false,
            position: ToolbarPosition::Left,
            tabs: Self::create_default_tabs(),
            active_tab: 0,
            hover_tab: None,
        }
    }
}

impl VerticalToolbar {
    /// Create the default Creo-style tabs with buttons
    fn create_default_tabs() -> Vec<ToolbarTab> {
        vec![
            // Model Tab
            ToolbarTab::new("Model", "📦")
                .add_buttons(vec![
                    ToolbarButton::new("model_box", "📦", "Box", ToolbarAction::ModelBox)
                        .with_shortcut("B")
                        .with_tooltip("Create a box primitive"),
                    ToolbarButton::new("model_extrude", "⬆", "Extrude", ToolbarAction::ModelExtrude)
                        .with_shortcut("E")
                        .with_tooltip("Extrude a sketch profile"),
                    ToolbarButton::new("model_revolve", "🔄", "Revolve", ToolbarAction::ModelRevolve)
                        .with_shortcut("V")
                        .with_tooltip("Revolve a sketch profile"),
                    ToolbarButton::new("model_sweep", "🌊", "Sweep", ToolbarAction::ModelSweep)
                        .with_tooltip("Sweep a profile along a path"),
                    ToolbarButton::new("model_loft", "📐", "Loft", ToolbarAction::ModelLoft)
                        .with_tooltip("Loft between multiple profiles"),
                    ToolbarButton::new("model_boundary", "🔲", "Boundary", ToolbarAction::ModelBoundary)
                        .with_tooltip("Create boundary surface"),
                    ToolbarButton::new("model_fillet", "✨", "Fillet", ToolbarAction::ModelFillet)
                        .with_tooltip("Add fillet to edges"),
                    ToolbarButton::new("model_chamfer", "✂", "Chamfer", ToolbarAction::ModelChamfer)
                        .with_tooltip("Add chamfer to edges"),
                    ToolbarButton::new("model_shell", "🐚", "Shell", ToolbarAction::ModelShell)
                        .with_tooltip("Create hollow shell"),
                    ToolbarButton::new("model_draft", "📐", "Draft", ToolbarAction::ModelDraft)
                        .with_tooltip("Add draft angle"),
                    ToolbarButton::new("model_hole", "🕳", "Hole", ToolbarAction::ModelHole)
                        .with_tooltip("Create hole feature"),
                    ToolbarButton::new("model_pattern", "📋", "Pattern", ToolbarAction::ModelPattern)
                        .with_tooltip("Create pattern of features"),
                    ToolbarButton::new("model_mirror", "🪞", "Mirror", ToolbarAction::ModelMirror)
                        .with_tooltip("Mirror features"),
                    ToolbarButton::new("model_boolean", "⚙", "Boolean", ToolbarAction::ModelBoolean)
                        .with_tooltip("Boolean operations"),
                    ToolbarButton::new("model_thread", "🧵", "Thread", ToolbarAction::ModelCosmeticThread)
                        .with_tooltip("Add cosmetic thread"),
                ]),
            
            // Sketch Tab
            ToolbarTab::new("Sketch", "✏️")
                .add_buttons(vec![
                    ToolbarButton::new("sketch_new", "📝", "New Sketch", ToolbarAction::SketchNew)
                        .with_shortcut("K")
                        .with_tooltip("Create new sketch"),
                    ToolbarButton::new("sketch_line", "📏", "Line", ToolbarAction::SketchLine)
                        .with_shortcut("L")
                        .with_tooltip("Draw line"),
                    ToolbarButton::new("sketch_rectangle", "▭", "Rectangle", ToolbarAction::SketchRectangle)
                        .with_shortcut("R")
                        .with_tooltip("Draw rectangle"),
                    ToolbarButton::new("sketch_circle", "⭘", "Circle", ToolbarAction::SketchCircle)
                        .with_tooltip("Draw circle"),
                    ToolbarButton::new("sketch_arc", "🌒", "Arc", ToolbarAction::SketchArc)
                        .with_tooltip("Draw arc"),
                    ToolbarButton::new("sketch_spline", "〰", "Spline", ToolbarAction::SketchSpline)
                        .with_tooltip("Draw spline"),
                    ToolbarButton::new("sketch_point", "⬤", "Point", ToolbarAction::SketchPoint)
                        .with_shortcut("P")
                        .with_tooltip("Add point"),
                    // Constraints group
                    ToolbarButton::new("sketch_h", "➖", "Horizontal", ToolbarAction::SketchConstraintHorizontal)
                        .with_tooltip("Horizontal constraint"),
                    ToolbarButton::new("sketch_v", "➕", "Vertical", ToolbarAction::SketchConstraintVertical)
                        .with_tooltip("Vertical constraint"),
                    ToolbarButton::new("sketch_parallel", "⏸", "Parallel", ToolbarAction::SketchConstraintParallel)
                        .with_tooltip("Parallel constraint"),
                    ToolbarButton::new("sketch_perpendicular", "⟂", "Perpendicular", ToolbarAction::SketchConstraintPerpendicular)
                        .with_tooltip("Perpendicular constraint"),
                    ToolbarButton::new("sketch_tangent", "⌝", "Tangent", ToolbarAction::SketchConstraintTangent)
                        .with_tooltip("Tangent constraint"),
                    ToolbarButton::new("sketch_equal", "=", "Equal", ToolbarAction::SketchConstraintEqual)
                        .with_tooltip("Equal constraint"),
                    ToolbarButton::new("sketch_symmetric", "⇄", "Symmetric", ToolbarAction::SketchConstraintSymmetric)
                        .with_tooltip("Symmetric constraint"),
                    ToolbarButton::new("sketch_concentric", "🎯", "Concentric", ToolbarAction::SketchConstraintConcentric)
                        .with_tooltip("Concentric constraint"),
                    ToolbarButton::new("sketch_midpoint", "⬤", "Midpoint", ToolbarAction::SketchConstraintMidpoint)
                        .with_tooltip("Midpoint constraint"),
                    ToolbarButton::new("sketch_coincident", "📍", "Coincident", ToolbarAction::SketchConstraintCoincident)
                        .with_tooltip("Coincident constraint"),
                    ToolbarButton::new("sketch_dimension", "📐", "Dimension", ToolbarAction::SketchDimension)
                        .with_tooltip("Add dimension"),
                    ToolbarButton::new("sketch_trim", "✂", "Trim", ToolbarAction::SketchTrim)
                        .with_tooltip("Trim entity"),
                    ToolbarButton::new("sketch_extend", "➡", "Extend", ToolbarAction::SketchExtend)
                        .with_tooltip("Extend entity"),
                    ToolbarButton::new("sketch_offset", "↔", "Offset", ToolbarAction::SketchOffset)
                        .with_tooltip("Offset entity"),
                    ToolbarButton::new("sketch_mirror", "🪞", "Mirror", ToolbarAction::SketchMirror)
                        .with_tooltip("Mirror entities"),
                    ToolbarButton::new("sketch_pattern", "📋", "Pattern", ToolbarAction::SketchPattern)
                        .with_tooltip("Pattern entities"),
                    ToolbarButton::new("sketch_project", "📥", "Project Refs", ToolbarAction::SketchProjectReferences)
                        .with_tooltip("Project 3D references"),
                ]),
            
            // Assembly Tab
            ToolbarTab::new("Assembly", "🔧")
                .add_buttons(vec![
                    ToolbarButton::new("asm_insert", "➕", "Insert Component", ToolbarAction::AssemblyInsertComponent)
                        .with_tooltip("Insert component into assembly"),
                    ToolbarButton::new("asm_coincident", "📍", "Coincident", ToolbarAction::AssemblyMateCoincident)
                        .with_tooltip("Coincident mate"),
                    ToolbarButton::new("asm_concentric", "🎯", "Concentric", ToolbarAction::AssemblyMateConcentric)
                        .with_tooltip("Concentric mate"),
                    ToolbarButton::new("asm_distance", "↔", "Distance", ToolbarAction::AssemblyMateDistance)
                        .with_tooltip("Distance mate"),
                    ToolbarButton::new("asm_angle", "📐", "Angle", ToolbarAction::AssemblyMateAngle)
                        .with_tooltip("Angle mate"),
                    ToolbarButton::new("asm_parallel", "⏸", "Parallel", ToolbarAction::AssemblyMateParallel)
                        .with_tooltip("Parallel mate"),
                    ToolbarButton::new("asm_perpendicular", "⟂", "Perpendicular", ToolbarAction::AssemblyMatePerpendicular)
                        .with_tooltip("Perpendicular mate"),
                    ToolbarButton::new("asm_tangent", "⌝", "Tangent", ToolbarAction::AssemblyMateTangent)
                        .with_tooltip("Tangent mate"),
                    ToolbarButton::new("asm_symmetric", "⇄", "Symmetric", ToolbarAction::AssemblyMateSymmetric)
                        .with_tooltip("Symmetric mate"),
                    ToolbarButton::new("asm_exploded", "💥", "Exploded View", ToolbarAction::AssemblyExplodedView)
                        .with_tooltip("Create exploded view"),
                    ToolbarButton::new("asm_sub", "📦", "Sub-Assembly", ToolbarAction::AssemblySubAssembly)
                        .with_tooltip("Create sub-assembly"),
                ]),
            
            // Analysis Tab
            ToolbarTab::new("Analysis", "📊")
                .add_buttons(vec![
                    ToolbarButton::new("analysis_measure", "📏", "Measure", ToolbarAction::AnalysisMeasure)
                        .with_tooltip("Measure distance, angle, area"),
                    ToolbarButton::new("analysis_mass", "⚖", "Mass Props", ToolbarAction::AnalysisMassProperties)
                        .with_tooltip("Mass properties"),
                    ToolbarButton::new("analysis_curvature", "🌈", "Curvature", ToolbarAction::AnalysisCurvature)
                        .with_tooltip("Curvature analysis"),
                    ToolbarButton::new("analysis_draft", "📐", "Draft", ToolbarAction::AnalysisDraft)
                        .with_tooltip("Draft analysis"),
                    ToolbarButton::new("analysis_thickness", "📏", "Thickness", ToolbarAction::AnalysisThickness)
                        .with_tooltip("Thickness analysis"),
                    ToolbarButton::new("analysis_interference", "💥", "Interference", ToolbarAction::AnalysisInterference)
                        .with_tooltip("Interference check"),
                    ToolbarButton::new("analysis_clearance", "↔", "Clearance", ToolbarAction::AnalysisClearance)
                        .with_tooltip("Clearance check"),
                ]),
            
            // View Tab
            ToolbarTab::new("View", "👁")
                .add_buttons(vec![
                    ToolbarButton::new("view_front", "👁", "Front", ToolbarAction::ViewFront)
                        .with_shortcut("1")
                        .with_tooltip("Front view (1)"),
                    ToolbarButton::new("view_back", "👁", "Back", ToolbarAction::ViewBack)
                        .with_shortcut("2")
                        .with_tooltip("Back view (2)"),
                    ToolbarButton::new("view_left", "👁", "Left", ToolbarAction::ViewLeft)
                        .with_shortcut("3")
                        .with_tooltip("Left view (3)"),
                    ToolbarButton::new("view_right", "👁", "Right", ToolbarAction::ViewRight)
                        .with_shortcut("4")
                        .with_tooltip("Right view (4)"),
                    ToolbarButton::new("view_top", "👁", "Top", ToolbarAction::ViewTop)
                        .with_shortcut("5")
                        .with_tooltip("Top view (5)"),
                    ToolbarButton::new("view_bottom", "👁", "Bottom", ToolbarAction::ViewBottom)
                        .with_shortcut("6")
                        .with_tooltip("Bottom view (6)"),
                    ToolbarButton::new("view_iso", "👁", "Isometric", ToolbarAction::ViewIso)
                        .with_shortcut("7")
                        .with_tooltip("Isometric view (7)"),
                    ToolbarButton::new("view_reset", "🏠", "Reset", ToolbarAction::ViewReset)
                        .with_shortcut("Home")
                        .with_tooltip("Reset view (Home)"),
                    ToolbarButton::new("view_perspective", "🔭", "Perspective", ToolbarAction::ViewPerspective)
                        .with_tooltip("Toggle perspective"),
                    ToolbarButton::new("view_ortho", "📐", "Orthographic", ToolbarAction::ViewOrthographic)
                        .with_tooltip("Toggle orthographic"),
                    ToolbarButton::new("view_shaded", "🎨", "Shaded", ToolbarAction::ViewShaded)
                        .with_tooltip("Shaded display"),
                    ToolbarButton::new("view_wireframe", "🕸", "Wireframe", ToolbarAction::ViewWireframe)
                        .with_tooltip("Wireframe display"),
                    ToolbarButton::new("view_zoom_fit", "🔍", "Zoom Fit", ToolbarAction::ViewZoomFit)
                        .with_tooltip("Zoom to fit"),
                    ToolbarButton::new("view_previous", "↩", "Previous", ToolbarAction::ViewPrevious)
                        .with_tooltip("Previous view"),
                ]),
            
            // Tools Tab
            ToolbarTab::new("Tools", "🔧")
                .add_buttons(vec![
                    ToolbarButton::new("tools_params", "📋", "Parameters", ToolbarAction::ToolsParameters)
                        .with_tooltip("Manage parameters"),
                    ToolbarButton::new("tools_relations", "🔗", "Relations", ToolbarAction::ToolsRelations)
                        .with_tooltip("Manage relations"),
                    ToolbarButton::new("tools_family", "👨‍👩‍👧‍👦", "Family Table", ToolbarAction::ToolsFamilyTable)
                        .with_tooltip("Family table"),
                    ToolbarButton::new("tools_explore", "🔬", "Design Exploration", ToolbarAction::ToolsDesignExploration)
                        .with_tooltip("Design exploration"),
                    ToolbarButton::new("tools_options", "⚙", "Options", ToolbarAction::ToolsOptions)
                        .with_tooltip("Application options"),
                    ToolbarButton::new("tools_customize", "🎨", "Customize", ToolbarAction::ToolsCustomize)
                        .with_tooltip("Customize toolbar"),
                ]),
        ]
    }
    
    /// Get the currently active tab
    pub fn active_tab(&self) -> &ToolbarTab {
        &self.tabs[self.active_tab]
    }
    
    /// Get mutable reference to active tab
    pub fn active_tab_mut(&mut self) -> &mut ToolbarTab {
        &mut self.tabs[self.active_tab]
    }
    
    /// Set active tab by index
    pub fn set_active_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }
    
    /// Toggle collapsed state
    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }
    
    /// Toggle icon-only mode
    pub fn toggle_icon_only(&mut self) {
        self.icon_only_mode = !self.icon_only_mode;
    }
    
    /// Toggle position (left/right)
    pub fn toggle_position(&mut self) {
        self.position = match self.position {
            ToolbarPosition::Left => ToolbarPosition::Right,
            ToolbarPosition::Right => ToolbarPosition::Left,
        };
    }
    
    /// Get toolbar width based on state
    pub fn width(&self) -> f32 {
        if self.collapsed {
            48.0
        } else if self.icon_only_mode {
            64.0
        } else {
            280.0
        }
    }
    
    /// Render the toolbar in egui
    pub fn show(&mut self, ui: &mut egui::Ui, on_action: &mut dyn FnMut(ToolbarAction)) {
        let width = self.width();
        ui.set_min_width(width);
        ui.set_max_width(width);
        
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Toolbar header with hamburger menu and controls
            ui.horizontal(|ui| {
                // Hamburger menu button with simple menu
                egui::menu::menu_button(ui, "☰ Menu", |ui| {
                    ui.set_min_width(180.0);
                    ui.style_mut().spacing.item_spacing = egui::vec2(8.0, 4.0);
                    
                    // File section
                    ui.strong("File");
                    ui.separator();
                    if ui.button("New Document  Ctrl+N").clicked() {
                        on_action(ToolbarAction::NewDocument);
                        ui.close_menu();
                    }
                    if ui.button("Open Document  Ctrl+O").clicked() {
                        on_action(ToolbarAction::OpenDocument);
                        ui.close_menu();
                    }
                    if ui.button("Save  Ctrl+S").clicked() {
                        on_action(ToolbarAction::SaveDocument);
                        ui.close_menu();
                    }
                    if ui.button("Save As  Ctrl+Shift+S").clicked() {
                        on_action(ToolbarAction::SaveDocumentAs);
                        ui.close_menu();
                    }
                    if ui.button("Export STL").clicked() {
                        on_action(ToolbarAction::ExportStl);
                        ui.close_menu();
                    }
                    if ui.button("Export STEP").clicked() {
                        on_action(ToolbarAction::ExportStep);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Undo  Ctrl+Z").clicked() {
                        on_action(ToolbarAction::Undo);
                        ui.close_menu();
                    }
                    if ui.button("Redo  Ctrl+Y").clicked() {
                        on_action(ToolbarAction::Redo);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        on_action(ToolbarAction::NewDocument); // Will need proper quit action
                        ui.close_menu();
                    }
                    ui.separator();
                    
                    // Help section
                    ui.strong("Help");
                    ui.separator();
                    if ui.button("About").clicked() {
                        on_action(ToolbarAction::HelpAbout);
                        ui.close_menu();
                    }
                    if ui.button("Shortcuts  F1").clicked() {
                        on_action(ToolbarAction::HelpShortcuts);
                        ui.close_menu();
                    }
                });
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Position toggle
                    if ui.small_button(if self.position == ToolbarPosition::Left { "◀" } else { "▶" })
                        .on_hover_text("Toggle position").clicked() {
                        self.toggle_position();
                    }
                    
                    // Icon-only mode toggle
                    if ui.small_button(if self.icon_only_mode { "🔤" } else { "🔣" })
                        .on_hover_text("Toggle icon/text mode").clicked() {
                        self.toggle_icon_only();
                    }
                    
                    // Collapse/expand
                    if ui.small_button(if self.collapsed { "▶" } else { "◀" })
                        .on_hover_text(if self.collapsed { "Expand" } else { "Collapse" }).clicked() {
                        self.toggle_collapsed();
                    }
                });
            });
            
            ui.separator();
            
            // Tab bar
            ui.horizontal_wrapped(|ui| {
                for (i, tab) in self.tabs.iter().enumerate() {
                    let is_active = i == self.active_tab;
                    let is_hovered = self.hover_tab == Some(i);
                    
                    let button = egui::Button::new(
                        egui::RichText::new(tab.icon)
                            .size(if self.icon_only_mode || self.collapsed { 20.0 } else { 18.0 })
                            .color(if is_active { egui::Color32::WHITE } else { egui::Color32::GRAY })
                    )
                    .fill(if is_active { egui::Color32::from_gray(60) } else if is_hovered { egui::Color32::from_gray(40) } else { egui::Color32::TRANSPARENT })
                    .corner_radius(4.0)
                    .min_size(egui::vec2(40.0, 40.0));
                    
                    let response = ui.add(button);
                    
                    if response.clicked() {
                        self.active_tab = i;
                    }
                    
                    if response.hovered() {
                        self.hover_tab = Some(i);
                    } else if self.hover_tab == Some(i) {
                        self.hover_tab = None;
                    }
                    
                    // Show tab name on hover or when not icon-only
                    if response.hovered() || (!self.icon_only_mode && !self.collapsed) {
                        response.on_hover_text(&tab.name);
                    }
                }
            });
            
            ui.separator();
            
            // Active tab content
            if !self.collapsed {
                let active_tab = &self.tabs[self.active_tab];
                
                for button in &active_tab.buttons {
                    self.render_button(ui, button, on_action);
                }
            }
        });
    }
    
    /// Render a single toolbar button
    fn render_button(&self, ui: &mut egui::Ui, button: &ToolbarButton, on_action: &mut dyn FnMut(ToolbarAction)) {
        let is_compact = self.icon_only_mode || self.collapsed;
        
        if is_compact {
            // Compact mode: just icon
            let btn = egui::Button::new(
                egui::RichText::new(button.icon).size(20.0)
            )
            .min_size(egui::vec2(40.0, 40.0))
            .corner_radius(4.0);
            
            let response = ui.add(btn);
            if response.clicked() {
                on_action(button.action.clone());
            }
            if response.hovered() {
                let tooltip_text = if let Some(shortcut) = &button.shortcut {
                    format!("{} ({})", button.label, shortcut)
                } else {
                    button.label.clone()
                };
                response.on_hover_text(tooltip_text);
            }
        } else {
            // Full mode: icon + label + shortcut
            ui.horizontal(|ui| {
                let btn = egui::Button::new(
                    egui::RichText::new(button.icon).size(18.0)
                )
                .min_size(egui::vec2(36.0, 36.0))
                .corner_radius(4.0);
                
                let response = ui.add(btn);
                if response.clicked() {
                    on_action(button.action.clone());
                }
                
                // Label and shortcut
                ui.vertical_centered_justified(|ui| {
                    ui.label(
                        egui::RichText::new(&button.label)
                            .size(13.0)
                            .strong()
                    );
                    if let Some(shortcut) = &button.shortcut {
                        ui.label(
                            egui::RichText::new(shortcut)
                                .size(10.0)
                                .color(egui::Color32::GRAY)
                                .monospace()
                        );
                    }
                });
            });
        }
    }
}