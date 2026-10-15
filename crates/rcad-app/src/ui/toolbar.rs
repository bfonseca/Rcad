//! Vertical toolbar implementation for rcad - Creo-style ribbon replacement

use eframe::egui;
use std::collections::HashMap;
use std::sync::OnceLock;

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

/// Toolbar action to rcad-icon ID mapping
fn action_to_icon_id(action: &ToolbarAction) -> Option<&'static str> {
    match action {
        // Model actions
        ToolbarAction::ModelExtrude => Some("features-extrude"),
        ToolbarAction::ModelRevolve => Some("features-revolve"),
        ToolbarAction::ModelSweep => Some("features-sweep"),
        ToolbarAction::ModelLoft => Some("features-loft"),
        ToolbarAction::ModelBoundary => Some("features-boundary"),
        ToolbarAction::ModelFillet => Some("features-round"),
        ToolbarAction::ModelChamfer => Some("features-chamfer"),
        ToolbarAction::ModelShell => Some("features-shell"),
        ToolbarAction::ModelDraft => Some("features-draft"),
        ToolbarAction::ModelHole => Some("features-hole"),
        ToolbarAction::ModelPattern => Some("edit-pattern-direction"),
        ToolbarAction::ModelMirror => Some("edit-mirror"),
        ToolbarAction::ModelBoolean => Some("edit-boolean-union"),
        ToolbarAction::ModelCosmeticThread => Some("features-hole-threaded"),
        ToolbarAction::ModelBox => Some("features-extrude"),
        
        // Sketch actions
        ToolbarAction::SketchNew => Some("sketch-sketch"),
        ToolbarAction::SketchLine => Some("sketch-line"),
        ToolbarAction::SketchRectangle => Some("sketch-rectangle"),
        ToolbarAction::SketchCircle => Some("sketch-circle-center"),
        ToolbarAction::SketchArc => Some("sketch-arc-3point"),
        ToolbarAction::SketchSpline => Some("sketch-spline"),
        ToolbarAction::SketchPoint => Some("sketch-point"),
        ToolbarAction::SketchConstraintHorizontal => Some("constraints-horizontal"),
        ToolbarAction::SketchConstraintVertical => Some("constraints-vertical"),
        ToolbarAction::SketchConstraintParallel => Some("constraints-parallel"),
        ToolbarAction::SketchConstraintPerpendicular => Some("constraints-perpendicular"),
        ToolbarAction::SketchConstraintTangent => Some("constraints-tangent"),
        ToolbarAction::SketchConstraintEqual => Some("constraints-equal"),
        ToolbarAction::SketchConstraintSymmetric => Some("constraints-symmetric"),
        ToolbarAction::SketchConstraintConcentric => Some("constraints-concentric"),
        ToolbarAction::SketchConstraintMidpoint => Some("constraints-midpoint"),
        ToolbarAction::SketchConstraintCoincident => Some("constraints-coincident"),
        ToolbarAction::SketchDimension => Some("sketch-dimension"),
        ToolbarAction::SketchTrim => Some("sketch-trim"),
        ToolbarAction::SketchExtend => Some("sketch-extend-line"),
        ToolbarAction::SketchOffset => Some("sketch-offset-edge"),
        ToolbarAction::SketchMirror => Some("sketch-mirror-sketch"),
        ToolbarAction::SketchPattern => Some("edit-pattern-direction"),
        ToolbarAction::SketchProjectReferences => Some("sketch-project-edge"),
        
        // Assembly actions
        ToolbarAction::AssemblyInsertComponent => Some("assembly-asm-insert"),
        ToolbarAction::AssemblyMateCoincident => Some("assembly-asm-mate"),
        ToolbarAction::AssemblyMateConcentric => Some("assembly-asm-concentric"),
        ToolbarAction::AssemblyMateDistance => Some("assembly-asm-distance"),
        ToolbarAction::AssemblyMateAngle => Some("assembly-asm-angle"),
        ToolbarAction::AssemblyMateParallel => Some("assembly-asm-parallel"),
        ToolbarAction::AssemblyMatePerpendicular => Some("assembly-asm-normal"),
        ToolbarAction::AssemblyMateTangent => Some("assembly-asm-tangent"),
        ToolbarAction::AssemblyMateSymmetric => Some("assembly-asm-symmetric"),
        ToolbarAction::AssemblyExplodedView => Some("assembly-asm-explode"),
        ToolbarAction::AssemblySubAssembly => Some("assembly-asm-sub-assembly"),
        
        // Analysis actions
        ToolbarAction::AnalysisMeasure => Some("general-measure"),
        ToolbarAction::AnalysisMassProperties => Some("general-info"),
        ToolbarAction::AnalysisCurvature => Some("analysis-curvature"),
        ToolbarAction::AnalysisDraft => Some("analysis-draft"),
        ToolbarAction::AnalysisThickness => Some("analysis-thickness"),
        ToolbarAction::AnalysisInterference => Some("assembly-asm-interference"),
        ToolbarAction::AnalysisClearance => Some("analysis-clearance"),
        
        // View actions
        ToolbarAction::ViewFront => Some("view-view-front"),
        ToolbarAction::ViewBack => Some("view-view-back"),
        ToolbarAction::ViewLeft => Some("view-view-left"),
        ToolbarAction::ViewRight => Some("view-view-right"),
        ToolbarAction::ViewTop => Some("view-view-top"),
        ToolbarAction::ViewBottom => Some("view-view-bottom"),
        ToolbarAction::ViewIso => Some("view-view-isometric"),
        ToolbarAction::ViewReset => Some("view-view-saved"),
        ToolbarAction::ViewPerspective => Some("view-view-perspective"),
        ToolbarAction::ViewOrthographic => Some("view-view-perspective"),
        ToolbarAction::ViewShaded => Some("view-display-shaded"),
        ToolbarAction::ViewWireframe => Some("view-display-wireframe"),
        ToolbarAction::ViewZoomFit => Some("view-zoom-fit"),
        ToolbarAction::ViewPrevious => Some("general-undo"),
        
        // Tools actions
        ToolbarAction::ToolsParameters => Some("general-settings"),
        ToolbarAction::ToolsRelations => Some("general-link"),
        ToolbarAction::ToolsFamilyTable => Some("general-model-tree"),
        ToolbarAction::ToolsDesignExploration => Some("general-search"),
        ToolbarAction::ToolsOptions => Some("general-settings"),
        ToolbarAction::ToolsCustomize => Some("tools-customize"),
        
        // File actions
        ToolbarAction::NewDocument => Some("general-new"),
        ToolbarAction::OpenDocument => Some("general-open"),
        ToolbarAction::SaveDocument => Some("general-save"),
        ToolbarAction::SaveDocumentAs => Some("general-save-as"),
        ToolbarAction::ExportStl => Some("general-export"),
        ToolbarAction::ExportStep => Some("general-export"),
        ToolbarAction::Undo => Some("general-undo"),
        ToolbarAction::Redo => Some("general-redo"),
        
        // Help
        ToolbarAction::HelpAbout => Some("general-info"),
        ToolbarAction::HelpShortcuts => Some("general-help"),
        
        // Box/ModelBox
        ToolbarAction::ModelBox => Some("features-extrude"),
        ToolbarAction::ModelSweep => Some("features-sweep"),
        ToolbarAction::ModelLoft => Some("features-loft"),
        ToolbarAction::ModelBoundary => Some("features-boundary"),
        
        _ => None,
    }
}

/// Theme mode for icons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconTheme {
    Light,
    Dark,
}

/// Icon cache for rcad-icons with theme support
struct IconCache {
    light_sprites: HashMap<String, String>,
    dark_sprites: HashMap<String, String>,
}

impl IconCache {
    fn new() -> Self {
        let mut light_sprites = HashMap::new();
        let mut dark_sprites = HashMap::new();
        
        // Load light theme sprites
        include_str!("../../../../rcad-icons/sprite-light.svg")
            .lines()
            .for_each(|line| {
                if let Some(start) = line.find("id=\"") {
                    let start = start + 4;
                    if let Some(end) = line[start..].find("\"") {
                        let id = &line[start..start+end];
                        let content_start = line.find(">").unwrap_or(0) + 1;
                        let content_end = line.rfind("</symbol>").unwrap_or(line.len());
                        let symbol = line[content_start..content_end].to_string();
                        light_sprites.insert(id.to_string(), symbol);
                    }
                }
            });
        
        // Load dark theme sprites
        include_str!("../../../../rcad-icons/sprite-dark.svg")
            .lines()
            .for_each(|line| {
                if let Some(start) = line.find("id=\"") {
                    let start = start + 4;
                    if let Some(end) = line[start..].find("\"") {
                        let id = &line[start..start+end];
                        let content_start = line.find(">").unwrap_or(0) + 1;
                        let content_end = line.rfind("</symbol>").unwrap_or(line.len());
                        let symbol = line[content_start..content_end].to_string();
                        dark_sprites.insert(id.to_string(), symbol);
                    }
                }
            });
        
        Self { light_sprites, dark_sprites }
    }
    
    fn get(&self, id: &str, theme: IconTheme) -> Option<&str> {
        match theme {
            IconTheme::Light => self.light_sprites.get(id).map(|s| s.as_str()),
            IconTheme::Dark => self.dark_sprites.get(id).map(|s| s.as_str()),
        }
    }
}

fn icon_cache() -> &'static IconCache {
    static CACHE: OnceLock<IconCache> = OnceLock::new();
    CACHE.get_or_init(IconCache::new)
}

/// Get icon SVG from rcad-icons sprite for current theme
fn get_icon(icon_id: &str, theme: IconTheme) -> Option<String> {
    icon_cache().get(icon_id, theme).map(|s| s.to_string())
}

/// Toolbar button definition
#[derive(Debug, Clone)]
pub struct ToolbarButton {
    pub id: String,
    pub icon: &'static str,           // Fallback emoji/text icon
    pub icon_id: Option<String>,      // rcad-icons sprite ID
    pub label: String,
    pub action: ToolbarAction,
    pub shortcut: Option<String>,
    pub tooltip: Option<String>,
}

impl ToolbarButton {
    pub fn new(id: impl Into<String>, icon: &'static str, label: impl Into<String>, action: ToolbarAction) -> Self {
        let icon_id = action_to_icon_id(&action).map(|s| s.to_string());
        Self {
            id: id.into(),
            icon,
            icon_id,
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

/// Simple toolbar tab definition (without rcad-icons complexity)
#[derive(Debug, Clone)]
pub struct SimpleToolbarTab {
    pub name: String,
    pub icon: &'static str,
    pub icon_id: Option<String>,
    pub buttons: Vec<ToolbarButton>,
}

impl SimpleToolbarTab {
    pub fn new(name: impl Into<String>, icon: &'static str) -> Self {
        Self {
            name: name.into(),
            icon,
            icon_id: None,
            buttons: Vec::new(),
        }
    }
    
    pub fn with_icon_id(mut self, icon_id: impl Into<String>) -> Self {
        self.icon_id = Some(icon_id.into());
        self
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
    pub tabs: Vec<SimpleToolbarTab>,
    pub active_tab: usize,
    pub hover_tab: Option<usize>,
    pub theme: IconTheme,  // Current theme for icons
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
            theme: IconTheme::Dark, // Default to dark theme
        }
    }
}

impl VerticalToolbar {
    /// Create the default Creo-style tabs with buttons
    fn create_default_tabs() -> Vec<SimpleToolbarTab> {
        vec![
            // Model Tab
            SimpleToolbarTab::new("Model", "📦")
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
            SimpleToolbarTab::new("Sketch", "✏️")
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
            SimpleToolbarTab::new("Assembly", "🔧")
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
            SimpleToolbarTab::new("Analysis", "📊")
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
            SimpleToolbarTab::new("View", "👁")
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
            SimpleToolbarTab::new("Tools", "🔧")
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
    pub fn active_tab(&self) -> &SimpleToolbarTab {
        &self.tabs[self.active_tab]
    }
    
    /// Get mutable reference to active tab
    pub fn active_tab_mut(&mut self) -> &mut SimpleToolbarTab {
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
    
    /// Detect theme from egui context
    fn detect_theme(&self, ctx: &egui::Context) -> IconTheme {
        if ctx.style().visuals.dark_mode {
            IconTheme::Dark
        } else {
            IconTheme::Light
        }
    }
    
    /// Render the toolbar in egui
    pub fn show(&mut self, ui: &mut egui::Ui, on_action: &mut dyn FnMut(ToolbarAction)) {
        // Update theme based on egui context
        self.theme = self.detect_theme(ui.ctx());
        
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
    
    /// Render a single toolbar button with rcad-icons SVG support
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
                // Try to render SVG icon from rcad-icons
                let icon_svg = button.icon_id.as_ref()
                    .and_then(|id| get_icon(id, self.theme));
                
                if let Some(_svg) = icon_svg {
                    // For now, use fallback icon - SVG rendering would need egui_extras or custom implementation
                    // For now, use fallback to emoji/icon
                    let btn = egui::Button::new(
                        egui::RichText::new(button.icon).size(18.0)
                    )
                    .min_size(egui::vec2(36.0, 36.0))
                    .corner_radius(4.0);
                    
                    let response = ui.add(btn);
                    if response.clicked() {
                        on_action(button.action.clone());
                    }
                } else {
                    // Fallback to emoji/text icon
                    let btn = egui::Button::new(
                        egui::RichText::new(button.icon).size(18.0)
                    )
                    .min_size(egui::vec2(36.0, 36.0))
                    .corner_radius(4.0);
                    
                    let response = ui.add(btn);
                    if response.clicked() {
                        on_action(button.action.clone());
                    }
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