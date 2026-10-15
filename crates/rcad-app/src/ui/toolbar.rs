//! Vertical toolbar implementation for rcad - Creo-style ribbon replacement

use eframe::egui;
use rcad_doc::FeatureKind;
use std::collections::HashMap;

/// Toolbar action types that map to app methods
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)] // file and help variants are driven from the menu bar
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
        ToolbarAction::ModelLoft => Some("features-swept-blend"),
        ToolbarAction::ModelBoundary => Some("surfaces-surface-boundary"),
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
        ToolbarAction::AssemblyMateConcentric => Some("constraints-concentric"),
        ToolbarAction::AssemblyMateDistance => Some("assembly-asm-distance"),
        ToolbarAction::AssemblyMateAngle => Some("assembly-asm-angle"),
        ToolbarAction::AssemblyMateParallel => Some("assembly-asm-parallel"),
        ToolbarAction::AssemblyMatePerpendicular => Some("assembly-asm-normal"),
        ToolbarAction::AssemblyMateTangent => Some("assembly-asm-tangent"),
        ToolbarAction::AssemblyMateSymmetric => Some("constraints-symmetric"),
        ToolbarAction::AssemblyExplodedView => Some("assembly-asm-explode"),
        ToolbarAction::AssemblySubAssembly => Some("assembly-asm-new-component"),
        
        // Analysis actions
        ToolbarAction::AnalysisMeasure => Some("general-measure"),
        ToolbarAction::AnalysisMassProperties => Some("general-info"),
        ToolbarAction::AnalysisCurvature => Some("general-measure-angle"),
        ToolbarAction::AnalysisDraft => Some("features-draft"),
        ToolbarAction::AnalysisThickness => Some("surfaces-surface-thicken"),
        ToolbarAction::AnalysisInterference => Some("assembly-asm-interference"),
        ToolbarAction::AnalysisClearance => Some("assembly-asm-distance"),
        
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
        ToolbarAction::ToolsCustomize => Some("general-settings"),
        
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
        
    }
}

/// rcad-icons id for a feature kind, used by the model tree to show what each
/// feature is at a glance.
pub fn feature_kind_icon_id(kind: &FeatureKind) -> Option<&'static str> {
    use rcad_doc::ConstructionKind;
    Some(match kind {
        FeatureKind::Box { .. } => "features-extrude",
        FeatureKind::Cylinder { .. } => "features-hole",
        FeatureKind::Extrude { .. } => "features-extrude",
        FeatureKind::ExtrudeSurface { .. } => "features-extrude-surface",
        FeatureKind::Revolve { .. } => "features-revolve",
        FeatureKind::Boolean { op, .. } => match op {
            rcad_geom::BooleanOp::Union => "edit-boolean-union",
            rcad_geom::BooleanOp::Subtract => "edit-boolean-subtract",
            rcad_geom::BooleanOp::Intersect => "edit-boolean-intersect",
        },
        FeatureKind::Translate { .. } => "edit-move",
        FeatureKind::Fillet { .. } => "features-round",
        FeatureKind::Chamfer { .. } => "features-chamfer",
        FeatureKind::Sew { .. } => "surfaces-surface-merge",
        FeatureKind::Loft { .. } => "features-swept-blend",
        FeatureKind::Boundary { .. } => "surfaces-surface-boundary",
        FeatureKind::Thicken { .. } => "surfaces-surface-thicken",
        // The rcad-icons files are datum/datum-plane.png etc, so the id is
        // "datum-datum-plane" - the category is part of the file name too.
        FeatureKind::Construction { kind, .. } => match kind {
            ConstructionKind::Plane => "datum-datum-plane",
            ConstructionKind::Axis => "datum-datum-axis",
            ConstructionKind::Point => "datum-datum-point",
            ConstructionKind::CoordSystem => "datum-datum-csys",
            ConstructionKind::Curve => "curves-curve-composite",
            ConstructionKind::Solid => "features-extrude",
        },
    })
}

/// Theme mode for icons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconTheme {
    Light,
    Dark,
}

// One embedded icon in both themes; generated by build.rs.
include!(concat!(env!("OUT_DIR"), "/icon_pngs.rs"));

/// Raw PNG bytes for `icon_id` in the given theme, if rcad-icons ships it.
pub fn icon_png(icon_id: &str, theme: IconTheme) -> Option<&'static [u8]> {
    EMBEDDED_ICONS
        .iter()
        .find(|icon| icon.id == icon_id)
        .map(|icon| match theme {
            IconTheme::Light => icon.light,
            IconTheme::Dark => icon.dark,
        })
}

/// Decoded egui textures for the icon set, created on first use.
///
/// egui cannot draw SVG, and the sprite loader this replaces never rendered
/// anything: it always fell through to an emoji. The committed 64px PNGs are
/// decoded once per (icon, theme) pair and cached as GPU textures.
#[derive(Default, Clone)]
pub struct IconTextures {
    textures: HashMap<(String, IconTheme), egui::TextureHandle>,
}

// egui::TextureHandle is not Debug, so summarise the cache instead of deriving.
impl std::fmt::Debug for IconTextures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconTextures")
            .field("loaded", &self.textures.len())
            .finish()
    }
}

impl IconTextures {
    /// Decode and cache the texture for `icon_id` in the given theme.
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        icon_id: &str,
        theme: IconTheme,
    ) -> Option<egui::TextureHandle> {
        let key = (icon_id.to_string(), theme);
        if let Some(handle) = self.textures.get(&key) {
            return Some(handle.clone());
        }
        let image = decode_icon(icon_png(icon_id, theme)?)?;
        let handle = ctx.load_texture(
            format!("rcad-icon/{theme:?}/{icon_id}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.textures.insert(key, handle.clone());
        Some(handle)
    }
}

fn decode_icon(bytes: &[u8]) -> Option<egui::ColorImage> {
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
    let rgba = decoded.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

/// Toolbar button definition
#[derive(Debug, Clone)]
#[allow(dead_code)] // id is used by the tests and future customisation
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
#[allow(dead_code)] // tab icons are reserved
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
    /// Decoded rcad-icons textures, filled lazily as buttons are drawn.
    icon_textures: IconTextures,
    /// Screen rect of each button as drawn in the last frame.
    ///
    /// Used by the interaction tests to click a real button through egui's
    /// own hit-testing, instead of poking at application state directly.
    button_rects: HashMap<ToolbarAction, egui::Rect>,
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
            theme: IconTheme::Dark, // Refreshed from the egui visuals each frame
            icon_textures: IconTextures::default(),
            button_rects: HashMap::new(),
        }
    }
}

impl VerticalToolbar {
    /// Create the default Creo-style tabs with buttons
    fn create_default_tabs() -> Vec<SimpleToolbarTab> {
        let tabs = vec![
            // Model Tab
            SimpleToolbarTab::new("Model", "📦")
                .add_buttons(vec![
                    // Sketching starts from the main palette; the sketch tools
                    // themselves live in the floating bar while a sketch is open.
                    ToolbarButton::new("model_sketch", "📝", "Sketch", ToolbarAction::SketchNew)
                        .with_shortcut("K")
                        .with_tooltip("Create a new sketch"),
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
        ];

        // The left palette is the feature commands only. Sketch and Analysis
        // tools appear on the floating bar while they are relevant, Assembly
        // commands appear there while placing a component, and View/Tools have
        // their own shortcuts on the floating bar and the menu bar.
        tabs.into_iter().filter(|tab| tab.name == "Model").collect()
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
        self.button_rects.clear();

        // Collapsed: a single expand button. Drawing the header and the tab bar
        // in a 48px column made them wrap into an unreadable stack.
        if self.collapsed {
            ui.vertical_centered(|ui| {
                if ui
                    .small_button("▶")
                    .on_hover_text("Expand the toolbar")
                    .clicked()
                {
                    self.toggle_collapsed();
                }
            });
            return;
        }

        // The panel owns the width now: it is resizable and the value is
        // persisted. Forcing set_min_width here overrode the panel, so dragging
        // the edge did nothing and left a wide empty strip beside the commands.
        
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Toolbar header: view toggles. The File/Edit/Help commands live in
            // the application menu bar, so there is no hamburger menu here.
            ui.horizontal(|ui| {
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
            
            // Tab bar: pointless when only the Model palette exists.
            if self.tabs.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (i, tab) in self.tabs.iter().enumerate() {
                    let is_active = i == self.active_tab;
                    let is_hovered = self.hover_tab == Some(i);
                    
                    // 34px square: six tabs then still fit on one row, so the
                    // tab bar does not wrap onto a second line.
                    let button = egui::Button::new(
                        egui::RichText::new(tab.icon)
                            .size(if self.icon_only_mode || self.collapsed { 18.0 } else { 16.0 })
                            .color(if is_active { egui::Color32::WHITE } else { egui::Color32::GRAY })
                    )
                    .fill(if is_active { egui::Color32::from_gray(60) } else if is_hovered { egui::Color32::from_gray(40) } else { egui::Color32::TRANSPARENT })
                    .corner_radius(4.0)
                    .min_size(egui::vec2(34.0, 34.0));
                    
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
            }

            // Active tab content
            if !self.collapsed {
                // Clone so render_button can take &mut self for the texture cache.
                let buttons = self.tabs[self.active_tab].buttons.clone();

                for button in &buttons {
                    self.render_button(ui, button, on_action);
                }
            }
        });
    }
    
    /// Screen rect of a toolbar button as drawn in the last frame.
#[cfg(test)]
    pub fn button_rect(&self, action: &ToolbarAction) -> Option<egui::Rect> {
        self.button_rects.get(action).copied()
    }

    /// Render one toolbar button: icon and label on a single line, with the
    /// keyboard shortcut in brackets, e.g. "Box (B)".
    ///
    /// In compact mode only the icon is drawn.
    fn render_button(&mut self, ui: &mut egui::Ui, button: &ToolbarButton, on_action: &mut dyn FnMut(ToolbarAction)) {
        let is_compact = self.icon_only_mode || self.collapsed;
        let texture = button
            .icon_id
            .as_deref()
            .and_then(|id| self.icon_textures.get(ui.ctx(), id, self.theme));

        let icon_size = if is_compact { 24.0 } else { 16.0 };
        let label = match &button.shortcut {
            Some(shortcut) => format!("{} ({shortcut})", button.label),
            None => button.label.clone(),
        };

        // Real rcad-icons artwork when available, emoji otherwise. In full mode
        // the image and the label live inside one button, so the text sits
        // directly beside the icon on the same line.
        let widget = match &texture {
            Some(texture) => {
                let image = egui::Image::from_texture(texture)
                    .fit_to_exact_size(egui::vec2(icon_size, icon_size));
                if is_compact {
                    egui::Button::image(image)
                } else {
                    egui::Button::image_and_text(image, egui::RichText::new(label).size(11.0))
                }
            }
            None => {
                let text = if is_compact {
                    egui::RichText::new(button.icon).size(icon_size)
                } else {
                    egui::RichText::new(format!("{} {label}", button.icon)).size(11.0)
                };
                egui::Button::new(text)
            }
        };

        // frame(false): the commands sit directly on the panel instead of in a
        // grey box, matching the model tree.
        let widget = if is_compact {
            widget
                .frame(false)
                .min_size(egui::vec2(40.0, 40.0))
                .corner_radius(4.0)
        } else {
            widget
                .frame(false)
                .min_size(egui::vec2(ui.available_width(), 22.0))
                .corner_radius(4.0)
        };

        let tooltip = match (&button.tooltip, &button.shortcut) {
            (Some(tip), Some(shortcut)) => format!("{tip} ({shortcut})"),
            (Some(tip), None) => tip.clone(),
            (None, Some(shortcut)) => format!("{} ({shortcut})", button.label),
            (None, None) => button.label.clone(),
        };

        let response = ui.add(widget).on_hover_text(tooltip);
        self.button_rects.insert(button.action.clone(), response.rect);
        if response.clicked() {
            on_action(button.action.clone());
        }
    }
}
#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn icon_set_is_embedded_for_both_themes() {
        assert!(
            EMBEDDED_ICONS.len() > 150,
            "only {} icons were embedded from rcad-icons",
            EMBEDDED_ICONS.len()
        );
        let sample = EMBEDDED_ICONS
            .iter()
            .find(|icon| icon.id == "features-extrude")
            .expect("features-extrude is missing from rcad-icons");
        assert!(!sample.light.is_empty() && !sample.dark.is_empty());
        // PNG magic bytes, so a broken asset fails loudly here.
        assert_eq!(&sample.light[..4], b"\x89PNG");
        assert_eq!(&sample.dark[..4], b"\x89PNG");
    }

    #[test]
    fn embedded_pngs_decode_to_square_rgba_images() {
        let bytes = icon_png("features-extrude", IconTheme::Dark).expect("icon present");
        let image = decode_icon(bytes).expect("icon decodes");
        assert_eq!(image.size, [64, 64]);
        assert!(!image.pixels.is_empty());
    }

    /// Every icon the toolbar asks for must exist in rcad-icons, otherwise the
    /// button silently falls back to an emoji.
    #[test]
    fn every_toolbar_icon_exists_in_the_icon_set() {
        let toolbar = VerticalToolbar::default();
        let mut missing: Vec<String> = Vec::new();
        let mut checked = 0usize;

        let mut check = |id: &Option<String>, missing: &mut Vec<String>| {
            let Some(id) = id else { return };
            checked += 1;
            if icon_png(id, IconTheme::Light).is_none() || icon_png(id, IconTheme::Dark).is_none() {
                missing.push(id.clone());
            }
        };

        for tab in &toolbar.tabs {
            check(&tab.icon_id, &mut missing);
            for button in &tab.buttons {
                check(&button.icon_id, &mut missing);
            }
        }

        // The left palette is the Model tab only, so this is a smaller set now.
        assert!(checked > 10, "expected to check many icons, checked {checked}");
        assert!(
            missing.is_empty(),
            "toolbar icons missing from rcad-icons: {missing:?}"
        );
    }

    /// Sketching starts from the main palette, and the Sketch and Analysis tabs
    /// were replaced by the floating quick bar.
    #[test]
    fn the_model_tab_starts_with_sketch_then_box() {
        let toolbar = VerticalToolbar::default();
        assert!(
            !toolbar.tabs.iter().any(|tab| tab.name == "Sketch"),
            "the Sketch tab should have moved to the floating bar"
        );
        assert!(
            !toolbar.tabs.iter().any(|tab| tab.name == "Analysis"),
            "the Analysis tab should have moved to the floating bar"
        );
        let first_two: Vec<&str> = toolbar.tabs[0]
            .buttons
            .iter()
            .take(2)
            .map(|button| button.label.as_str())
            .collect();
        assert_eq!(first_two, vec!["Sketch", "Box"]);
    }

    /// The model tree draws a type icon per feature; every kind must resolve.
    #[test]
    fn every_feature_kind_has_an_icon() {
        use rcad_doc::{ConstructionKind, FeatureKind};
        use rcad_geom::{BooleanOp, Profile2D};

        let profile = Profile2D {
            outer: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            holes: Vec::new(),
        };
        let kinds = vec![
            FeatureKind::Box { size: [1.0, 1.0, 1.0] },
            FeatureKind::Cylinder { radius: 1.0, height: 1.0 },
            FeatureKind::Extrude { profile: profile.clone(), height: 1.0 },
            FeatureKind::ExtrudeSurface { profile: profile.clone(), height: 1.0 },
            FeatureKind::Revolve { profile: profile.clone(), angle_deg: 90.0 },
            FeatureKind::Boolean { tool_feature: uuid::Uuid::new_v4(), op: BooleanOp::Union },
            FeatureKind::Boolean { tool_feature: uuid::Uuid::new_v4(), op: BooleanOp::Subtract },
            FeatureKind::Boolean { tool_feature: uuid::Uuid::new_v4(), op: BooleanOp::Intersect },
            FeatureKind::Translate { delta: [1.0, 0.0, 0.0] },
            FeatureKind::Fillet { radius: 1.0 },
            FeatureKind::Chamfer { distance: 1.0 },
            FeatureKind::Sew { tolerance: 0.1 },
            FeatureKind::Loft { profiles: vec![profile.clone()], solid: true },
            FeatureKind::Boundary { u_curves: vec![profile.clone()], v_curves: vec![profile] },
            FeatureKind::Thicken { surface_feature: uuid::Uuid::new_v4(), distance: 1.0, both_sides: false },
            FeatureKind::Construction { kind: ConstructionKind::Plane, origin: [0.0; 3], direction: [0.0, 0.0, 1.0] },
            FeatureKind::Construction { kind: ConstructionKind::Axis, origin: [0.0; 3], direction: [1.0, 0.0, 0.0] },
            FeatureKind::Construction { kind: ConstructionKind::Point, origin: [0.0; 3], direction: [0.0, 1.0, 0.0] },
            FeatureKind::Construction { kind: ConstructionKind::Curve, origin: [0.0; 3], direction: [0.0, 0.0, 1.0] },
            FeatureKind::Construction { kind: ConstructionKind::Solid, origin: [0.0; 3], direction: [0.0, 0.0, 1.0] },
            FeatureKind::Construction { kind: ConstructionKind::CoordSystem, origin: [0.0; 3], direction: [0.0, 0.0, 1.0] },
        ];

        for kind in kinds {
            let id = feature_kind_icon_id(&kind).expect("every kind maps to an icon");
            assert!(
                icon_png(id, IconTheme::Light).is_some() && icon_png(id, IconTheme::Dark).is_some(),
                "{kind:?} maps to missing icon {id}"
            );
        }
    }
}
