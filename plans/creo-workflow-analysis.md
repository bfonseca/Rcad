# PTC Creo Parametric Working Method Analysis & Implementation Plan for rcad

## Executive Summary

This document provides a comprehensive analysis of the PTC Creo Parametric working method and maps it to the current rcad codebase. It identifies gaps, required improvements, and creates a detailed implementation plan to clone Creo's working method while respecting rcad's architecture (modular crates, rcad.toml config, no top ribbon, vertical toolbar + adaptive history overlay).

---

## 1. PTC Creo Parametric Working Method - Core Concepts

### 1.1 Feature-Based Parametric Modeling
- **Feature Tree (Model Tree)**: Ordered, hierarchical list of features with parent/child relationships
- **Feature Types**: Sketched (extrude, revolve, sweep, blend), Placed (fillet, chamfer, hole, shell), Pattern, Mirror, Boolean
- **Insert Here Mode**: Move insertion point in feature tree (Insert Here / End of Part)
- **Suppress/Resume**: Temporarily disable features without deleting
- **Edit Definition**: Modify feature parameters and references
- **Rollback**: Temporarily revert to earlier state in feature tree

### 1.2 Sketcher & Intent Manager
- **Intent Manager**: Automatic constraint/dimension management
  - **Weak Dimensions**: Auto-generated, gray/dashed, ensure fully-constrained sketch
  - **Strong Dimensions**: User-defined, replace corresponding weak dimensions
  - **Driving vs Driven Dimensions**: Driving modifies geometry; driven is reference-only
- **Reference Geometry**: Project 3D edges/vertices onto sketch plane (One-Side, Both-Sides)
- **Constraints**: Horizontal, Vertical, Parallel, Perpendicular, Tangent, Equal, Symmetric, Concentric, Midpoint, Coincident
- **DOF Analysis**: Real-time display of degrees of freedom
- **Constraint Status**: Under-constrained / Fully-constrained / Over-constrained

### 1.3 Top-Down Design (Skeletons)
- **Skeleton Parts**: Special parts containing only construction geometry (planes, axes, points, curves, surfaces)
- **Publish Geometry**: Select and name reference entities in skeleton for downstream use
- **Copy Geometry**: Consume published geometry in child parts with associativity
- **External References**: Visual status indicators (Up-to-date / Out-of-date / Broken)
- **External Reference Management**: Update, replace, break references

### 1.4 Assembly Design
- **Component Instances**: Parts/sub-assemblies with placement constraints
- **Mate Constraints**: Coincident, Concentric, Distance, Angle, Parallel, Perpendicular, Tangent, Symmetric
- **Assembly Features**: Cuts, holes, patterns at assembly level
- **Exploded Views**: Step-by-step disassembly visualization
- **Sub-assemblies**: Hierarchical assembly structure
- **Flexible/Rigid**: Allow motion in sub-assemblies

### 1.5 User Interface (Creo-style)
- **Ribbon Interface**: Contextual tabs at top (File, Model, Sketch, Assembly, Analysis, View, Tools)
- **Model Tree**: Left sidebar with feature tree, layers, design items
- **Graphics Area**: Central 3D viewport
- **Dashboard/Dialogs**: Feature creation via dashboard (right side) or dialogs
- **Mini Toolbar**: Context-sensitive on selection
- **Quick Access Toolbar**: Customizable shortcuts
- **Status Bar**: Coordinates, selection info, constraints status

---

## 1.6 PTC Creo Parametric Philosophy - Deep Dive (Design Intent & Robustness)

### Design Intent: The Core Philosophy
Creo's parametric philosophy centers on **capturing and preserving design intent** - the engineering logic behind design decisions. This is not just about parameters; it's about relationships, constraints, and the "why" behind every feature.

#### Key Principles:
1. **Design Intent = Constraints + Parameters + Feature Order**
   - How a model reacts to changes is determined by how it was built
   - Same geometry, different feature trees = different behavior on modification

2. **Parent/Child Relationships Are Sacred**
   - **Parent**: Feature that provides references (sketch plane, edge for fillet, face for draft)
   - **Child**: Feature that depends on parent's geometry
   - **Rule**: Children must regenerate after parents; deleting parent breaks children
   - **Reference Stability**: Creo uses internal IDs (not topology) to track references across regenerations

3. **Regeneration Discipline**
   - **Sequential Regeneration**: Features rebuild in tree order (top-down)
   - **Dirty Flag Propagation**: Change to feature → mark it and all children dirty → regenerate in order
   - **Failure Modes**:
     - **Warning**: Regenerates with issues (e.g., tangent edge fillet)
     - **Error**: Stops regeneration; model rolls back to last good state
   - **Regeneration Manager**: User can control regeneration (automatic, manual, suppressed)

4. **Robust Modeling Best Practices (Creo Methodology)**
   - **Reference Stable Geometry**: Use datum planes/axes/points over model edges/faces
   - **Minimize Parent/Child Chains**: Shallow trees are more robust
   - **Use Skeletons for Top-Down**: Centralize design intent in skeleton; children reference skeleton
   - **Design for Change**: Anticipate modifications; use parameters/relations
   - **Standard Features Over Complex Ones**: Simple extrude + cut vs. single complex sweep
   - **Naming Conventions**: Semantic names (MOUNT_HOLE vs. Extrude_3)

5. **Parameter Management**
   - **Parameters**: Numeric, string, boolean values attached to features/parts
   - **Relations**: Equations between parameters (e.g., `hole_dia = shaft_dia + 0.1`)
   - **Family Tables**: Multiple configurations driven by parameter tables
   - **Global Parameters**: Part-level parameters accessible to all features

6. **Reference Management (External References)**
   - **Associativity**: External refs maintain link to source geometry
   - **Status Tracking**: Up-to-date / Out-of-date / Broken
   - **Update Control**: User decides when to update (not automatic)
   - **Reference Viewer**: Graph of all external dependencies

7. **Feature Operations That Preserve Intent**
   - **Reorder**: Drag features; Creo validates no circular refs
   - **Insert Mode**: Insert features at specific point; downstream features regenerate
   - **Rollback**: Temporary view at earlier state; edit early features
   - **Suppress/Resume**: Toggle feature without deleting; children suppress automatically
   - **Edit References**: Re-attach feature to different geometry (with validation)
   - **Replace**: Swap feature type (e.g., Extrude → Revolve) preserving children where possible

### 1.7 Sketcher Philosophy: Intent Manager Deep Dive

#### The Intent Manager's Role
The Intent Manager is not just a constraint solver - it's a **design intent capture system** that:
- **Anticipates** what constraints user likely needs
- **Manages** the transition from under-constrained → fully-constrained
- **Preserves** user's explicit decisions (strong dimensions) over auto-generated ones (weak)

#### Weak vs Strong Dimensions - The Complete Picture
| Aspect | Weak Dimensions | Strong Dimensions |
|--------|-----------------|-------------------|
| **Origin** | Auto-generated by Intent Manager | User explicitly placed |
| **Visual** | Gray, dashed line, gray text | Black, solid line, black text |
| **Behavior** | Deleted/replaced when strong added on same entities | Permanent until user deletes |
| **Promotion** | Auto-promote to strong if user dimensions same entities | N/A |
| **Driving/Driven** | Always driving (solver uses them) | Can be driving or driven |

#### Constraint Solving Strategy
1. **Virtual Constraints**: Temporary constraints during drag operations (not saved)
2. **Relaxation Solver**: Iterative constraint satisfaction (SolveSpace backend)
3. **Solution Selection**: When multiple solutions exist, pick closest to current geometry
4. **Conflict Detection**: Identify over-constrained sets; highlight conflicting constraints

#### Reference Geometry in Sketches
- **Project**: Project 3D edges/vertices onto sketch plane (creates reference entities)
- **Intersect**: Intersect 3D geometry with sketch plane (creates curves)
- **Offset**: Offset projected references
- **One-Side / Both-Sides**: Control projection direction relative to sketch plane
- **Associativity**: References update when 3D model changes (with status tracking)

### 1.8 Assembly Philosophy: Constraint-Based Positioning

#### Mate Constraints as Equations
Each mate = constraint equation reducing DOF between components
- **6 DOF** per free component (3 translation + 3 rotation)
- **Mate** removes DOF: Coincident (3 trans), Concentric (2 trans + 1 rot), etc.
- **Fully Constrained Assembly**: 0 DOF (all components grounded or fully mated)
- **Mechanisms**: Intentional DOF for motion studies

#### Assembly Structure
- **Bottom-Up**: Create parts independently, assemble later
- **Top-Down**: Create skeleton/layout, design parts in assembly context
- **In-Context Design**: Edit part features while in assembly (references other components)
- **External Refs in Assembly**: Same status tracking as skeleton refs

---

## 2. Current rcad State Analysis

### 2.1 What Exists (Strengths)
| Component | Status | Details |
|-----------|--------|---------|
| **Project Structure** | ✅ Complete | Modular crates, Cargo workspace, MessagePack serialization |
| **rcad.toml Config** | ✅ Defined | Tolerances, surface colors, external tables, startup, trail log, MCP |
| **Geometry Kernel** | ✅ Complete | OCCT via cadrum, trait-based (rcad-geom), box/cylinder/extrude/revolve/boolean/fillet/chamfer/sew/loft/boundary/thicken |
| **Parametric Rebuild** | ✅ Complete | rcad-param rebuild_part(), feature evaluation order, dirty tracking |
| **Feature Tree** | ✅ Basic | Feature struct with id, name, kind, suppressed flag |
| **Sketcher** | ✅ Advanced | Points, lines, circles, arcs, splines, 14 constraint types, Intent Manager (weak/strong), DOF analysis, constraint status, trim/extend/offset/mirror/pattern |
| **Skeletons** | ✅ Complete | Skeleton parts, Publish Geometry, Copy Geometry, External Ref status (UpToDate/OutOfDate/Broken) |
| **Underlay** | ✅ Complete | Image import, scale calibration (2 pts + distance), opacity, lock, rotation, snap-to-feature, world↔pixel |
| **Assembly** | ✅ Foundation | Component instances, basic mates (coincident/concentric/distance), extended mates, sub-assemblies, exploded views, solver (iterative + slvs placeholder) |
| **Undo/Redo** | ✅ Complete | UndoStack with project cloning |
| **Keyboard Shortcuts** | ✅ Complete | Comprehensive shortcut system with F1 dialog |
| **Viewport** | ✅ Basic | egui + wgpu, orbit/pan/zoom, standard views, picking |

### 2.2 What's Missing (Gaps vs Creo)
| Area | Gap | Priority |
|------|-----|----------|
| **UI Layout** | Top menu bar exists; no vertical toolbar; no adaptive history overlay with blur | 🔴 Critical |
| **Feature Tree UX** | Basic list; no Parent/Child visualization; no Insert Here mode; no drag-reorder; no rollback | 🔴 Critical |
| **rcad.toml Integration** | Config file exists but not loaded/used in rcad-app | 🔴 Critical |
| **Surface Color Rendering** | Colors defined in rcad.toml but not applied in viewport | 🟠 High |
| **External Tables** | rcad-table crate exists but not connected to features/parts | 🟠 High |
| **Trail Logging** | Defined in rcad.toml but not implemented | 🟠 High |
| **File Associations** | Extensions defined but not registered | 🟡 Medium |
| **MCP Server** | Crate exists but minimal implementation | 🟡 Medium |
| **Project Initialization** | No recent projects, no auto-create on first run | 🟡 Medium |
| **Reference Geometry in Sketch** | Projection exists but UI for "One-Side/Both-Sides" missing | 🟡 Medium |
| **Dashboard-style Feature Creation** | Features added via sidebar buttons, not contextual dashboard | 🟡 Medium |
| **Assembly Mate Solver** | Basic solver only; needs robust slvs integration | 🟡 Medium |
| **Feature Edit Definition** | No dialog to edit feature parameters after creation | 🟡 Medium |
| **Suppress/Resume UX** | Basic suppress exists but no visual state in tree | 🟢 Low |
| **Rollback** | Not implemented | 🟢 Low |
| **Layers/Design Items** | Not in model tree | 🟢 Low |

---

## 3. Implementation Plan

### Phase 1: Core UI Architecture (Vertical Toolbar + Adaptive History Overlay) 🔴

#### 1.1 Remove Top Menu Bar
- Replace `egui::TopBottomPanel::top("menu_bar")` with vertical toolbar
- Move File/Help actions to toolbar buttons or hamburger menu

#### 1.2 Vertical Dockable Toolbar (Left Side)
```rust
// New structure in app.rs
struct VerticalToolbar {
    collapsed: bool,
    icon_only_mode: bool,
    tabs: Vec<ToolbarTab>,  // Model, Sketch, Assembly, Analysis, View, Tools
    active_tab: usize,
}

struct ToolbarTab {
    name: String,
    icon: &'static str,
    buttons: Vec<ToolbarButton>,
}

struct ToolbarButton {
    id: String,
    icon: &'static str,
    label: String,
    action: ToolbarAction,
    shortcut: Option<String>,
}
```
- Default position: Left, dockable to right
- Toggle: Icon-only vs Icon+Text
- Tabs matching Creo: Model, Sketch, Assembly, Analysis, View, Tools

#### 1.3 Adaptive History Overlay (Right Side, Over Viewport)
```rust
struct HistoryOverlay {
    visible: bool,
    hovered: bool,
    blur_enabled: bool,
    tree_state: HistoryTreeState,  // Expanded/collapsed nodes
    insert_here_index: Option<usize>,  // Insert Here mode
    rollback_index: Option<usize>,  // Rollback mode
}
```
- **Behavior**: 100% transparent when not hovered; blur + darken on hover (wgpu shader)
- **Content**: Feature tree with Parent/Child indentation, suppression state, Insert Here marker
- **Interactions**: Click to select feature, right-click for context menu (Edit Definition, Suppress, Delete, Rollback)

#### 1.4 Viewport Integration
- Central panel remains viewport
- Status bar at bottom (keep existing)
- Mini-toolbar on selection (context-sensitive)

### Phase 2: rcad.toml Integration in rcad-app 🔴

#### 2.1 Config Loading & Structure
```rust
// In crates/rcad-app/src/config.rs (new file)
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub project: ProjectConfig,
    pub general_tolerance: GeneralTolerance,
    pub global_parameters: GlobalParameters,
    pub surface_colors: HashMap<String, SurfaceColor>,
    pub external_tables: ExternalTablesConfig,
    pub startup: StartupConfig,
    pub trail_log: TrailLogConfig,
    pub file_extensions: FileExtensionsConfig,
    pub mcp_server: McpServerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub repository: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneralTolerance {
    pub linear: f64,
    pub angular: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalParameters {
    pub default_extrusion_length: f64,
    // Extensible for more global params
    #[serde(flatten)]
    pub extra: HashMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceColor {
    pub rgb: [u8; 3],
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExternalTablesConfig {
    pub default_path: PathBuf,
    pub default_format: String,  // "csv", "xlsx", "xls"
    pub cache_in_memory: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StartupConfig {
    pub auto_open_last: bool,
    pub recent_projects_file: PathBuf,  // Empty = platform default
    pub max_recent_projects: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrailLogConfig {
    pub enabled: bool,
    pub format: String,  // "jsonl" or "text"
    pub level: String,   // "full", "replay", "errors"
    pub max_size_mb: u64,
    pub max_files: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileExtensionsConfig {
    pub project: String,    // ".rcad"
    pub part: String,       // ".rcadprt"
    pub skeleton: String,   // ".rcadskl"
    pub assembly: String,   // ".rcadasm"
    pub trail: String,      // ".rcadlog"
    pub register_on_first_run: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub enabled: bool,
    pub tcp_port: u16,      // 0 = disabled, stdio only
    pub tcp_host: String,
    pub allow_write: bool,
    pub request_timeout_secs: u64,
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let path = std::env::current_dir()?.join("rcad.toml");
        if !path.exists() {
            return Ok(Self::default());  // Return defaults if no config
        }
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).map_err(ConfigError::Parse)
    }
    
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = std::env::current_dir()?.join("rcad.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
    
    pub fn get_surface_color(&self, surface_type: &str) -> Option<[u8; 3]> {
        self.surface_colors.get(surface_type).map(|c| c.rgb)
    }
    
    pub fn get_global_param(&self, key: &str) -> Option<f64> {
        self.global_parameters.extra.get(key).copied()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        // Defaults matching rcad.toml
        let mut surface_colors = HashMap::new();
        surface_colors.insert("threads".to_string(), SurfaceColor {
            rgb: [255, 165, 0], description: Some("Thread surfaces".to_string())
        });
        surface_colors.insert("fit_surfaces".to_string(), SurfaceColor {
            rgb: [0, 255, 0], description: Some("Fit surfaces".to_string())
        });
        surface_colors.insert("sealing_zones".to_string(), SurfaceColor {
            rgb: [255, 0, 0], description: Some("Sealing zones".to_string())
        });
        
        Self {
            project: ProjectConfig {
                name: "rcad".to_string(),
                version: "0.0.1".to_string(),
                description: "CAD paramétrico open-source em Rust".to_string(),
                authors: vec!["Bruno Fonseca".to_string()],
                license: "MIT".to_string(),
                repository: "https://github.com/bfonseca/Rcad".to_string(),
            },
            general_tolerance: GeneralTolerance { linear: 0.01, angular: 0.1 },
            global_parameters: GlobalParameters {
                default_extrusion_length: 10.0,
                extra: HashMap::new(),
            },
            surface_colors,
            external_tables: ExternalTablesConfig {
                default_path: PathBuf::from("tables"),
                default_format: "xlsx".to_string(),
                cache_in_memory: true,
            },
            startup: StartupConfig {
                auto_open_last: true,
                recent_projects_file: PathBuf::new(),
                max_recent_projects: 10,
            },
            trail_log: TrailLogConfig {
                enabled: true,
                format: "jsonl".to_string(),
                level: "full".to_string(),
                max_size_mb: 10,
                max_files: 5,
            },
            file_extensions: FileExtensionsConfig {
                project: ".rcad".to_string(),
                part: ".rcadprt".to_string(),
                skeleton: ".rcadskl".to_string(),
                assembly: ".rcadasm".to_string(),
                trail: ".rcadlog".to_string(),
                register_on_first_run: true,
            },
            mcp_server: McpServerConfig {
                enabled: true,
                tcp_port: 0,
                tcp_host: "127.0.0.1".to_string(),
                allow_write: true,
                request_timeout_secs: 30,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}
```

#### 2.2 Integration in RcadApp
```rust
// In app.rs - add to RcadApp struct
pub struct RcadApp {
    // ... existing fields
    config: AppConfig,
    recent_projects: Vec<RecentProject>,
    trail_logger: Option<TrailLogger>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    pub last_opened: String,  // RFC3339 timestamp
}

// In RcadApp::new()
impl RcadApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load().unwrap_or_default();
        let recent_projects = Self::load_recent_projects(&config);
        
        let mut this = Self {
            // ... existing fields
            config,
            recent_projects,
            trail_logger: None,
        };
        
        // Initialize trail logger if enabled
        if this.config.trail_log.enabled {
            this.trail_logger = TrailLogger::new(&this.config.trail_log).ok();
        }
        
        // Auto-open last project if configured
        if this.config.startup.auto_open_last {
            if let Some(last) = this.recent_projects.first().cloned() {
                if last.path.exists() {
                    // Load project async or defer to first frame
                    this.pending_project_load = Some(last.path);
                }
            }
        }
        
        // Create default geometry for empty state
        let _ = this.create_box();
        this
    }
    
    fn load_recent_projects(config: &AppConfig) -> Vec<RecentProject> {
        let path = if config.startup.recent_projects_file.as_os_str().is_empty() {
            // Platform default
            dirs::config_dir()
                .unwrap_or_default()
                .join("rcad")
                .join("recent.json")
        } else {
            config.startup.recent_projects_file.clone()
        };
        
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }
    
    fn save_recent_projects(&self) {
        // ... save to recent.json
    }
    
    fn add_recent_project(&mut self, path: PathBuf) {
        // ... add to front, dedup, truncate to max_recent_projects
    }
}
```

#### 2.3 Application at Runtime
- **Tolerances**: Pass to sketch solver (`rcad-sketch`), feature rebuild (`rcad-param`)
- **Surface Colors**: Apply in viewport rendering via `Body.surface_type` → `config.get_surface_color()`
- **External Tables**: Load on startup via `rcad-table`, cache per `cache_in_memory`, watch for changes
- **Startup**: Auto-open last project, manage recent.json in platform config dir
- **Trail Log**: Initialize `TrailLogger` with rotation; log all user actions
- **MCP Server**: Spawn separate process if `enabled && tcp_port > 0`, else stdio
- **Config Watch**: Watch `rcad.toml` for changes (notify crate), hot-reload non-structural settings

#### 2.4 Config UI (Tools Tab → Options)
- Dialog with tabs: General, Tolerances, Colors, Tables, Startup, Logging, Extensions, MCP
- Live preview of color changes in viewport
- "Reset to Defaults" button
- Auto-save on dialog close

### Phase 3: Feature Tree / Model Tree Enhancements 🔴

#### 3.1 Data Model Extensions (rcad-doc)
```rust
// In rcad-doc/src/lib.rs - Enhanced Feature struct
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: EntityId,
    pub name: String,
    pub suppressed: bool,
    pub kind: FeatureKind,
    // NEW: Parent/Child tracking for design intent
    pub parent_features: Vec<EntityId>,      // Features this feature references
    pub child_features: Vec<EntityId>,       // Features that reference this (computed)
    pub references: FeatureReferences,       // Detailed reference info for Edit References
    pub parameters: FeatureParameters,       // Structured parameters for Edit Definition
    pub insert_here_marker: bool,            // True if this is Insert Here position
    pub rollback_marker: bool,               // True if this is Rollback position
    pub created_timestamp: String,           // RFC3339 for trail log
    pub modified_timestamp: String,          // RFC3339 for trail log
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureReferences {
    pub sketch_plane: Option<Reference>,     // Plane/face sketch was created on
    pub sketch_orientation: Option<Reference>, // Edge/axis for vertical reference
    pub profiles: Vec<Reference>,            // Sketch profiles used
    pub depth_reference: Option<Reference>,  // For "To Next", "To Selected", etc.
    pub draft_hinges: Vec<Reference>,        // Edges for draft hinge
    pub fillet_edges: Vec<Reference>,        // Edges for fillet/chamfer
    pub boolean_tool: Option<EntityId>,      // Tool feature for boolean
    pub pattern_axis: Option<Reference>,     // Axis for circular pattern
    pub pattern_direction: Option<Reference>, // Direction for linear pattern
    pub mirror_plane: Option<Reference>,     // Plane for mirror
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub entity_id: EntityId,           // Feature/body/geometry ID
    pub entity_type: ReferenceType,    // Plane, Face, Edge, Vertex, Axis, Sketch, Curve
    pub stable_name: String,           // Creo-style stable name (e.g., "FRONT_PLANE", "EXTRUDE_1.FACE_3")
    pub selection_query: String,       // Query to re-find after regeneration (e.g., "largest_face", "face_at_point[0,0,10]")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceType {
    Plane, Face, Edge, Vertex, Axis, Sketch, Curve, Surface, Solid, CoordSystem,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureParameters {
    // Common parameters - feature-specific ones in kind-specific structs
    pub numeric: HashMap<String, f64>,     // depth, angle, radius, distance, etc.
    pub string: HashMap<String, String>,   // names, types, options
    pub boolean: HashMap<String, bool>,    // flags: symmetric, both_sides, outward, etc.
    pub vector: HashMap<String, [f64; 3]>, // directions, offsets
    pub option: HashMap<String, String>,   // enum selections: "blind", "through_all", "to_next"
}

// Enhanced Part struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    // ... existing fields
    pub insert_here_index: Option<usize>,       // Insert Here position in feature list
    pub rollback_index: Option<usize>,          // Rollback position (rebuild stops here)
    pub layers: Vec<Layer>,                     // Layer management
    pub parameters: HashMap<String, Parameter>, // Part-level parameters (global)
    pub relations: Vec<Relation>,               // Equations: param = expression
    pub family_table: Option<FamilyTable>,      // For configurations
    pub regeneration_mode: RegenerationMode,    // Automatic, Manual, Suppressed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: EntityId,
    pub name: String,
    pub visible: bool,
    pub items: Vec<LayerItem>,  // Features, bodies, datum geometry
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayerItem {
    Feature(EntityId),
    Body(EntityId),
    Datum(EntityId),  // Construction features
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub value: ParameterValue,
    pub description: Option<String>,
    pub is_global: bool,  // Visible in assembly BOM
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterValue {
    Real(f64),
    Integer(i64),
    String(String),
    Boolean(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub lhs: String,      // Parameter name
    pub rhs: String,      // Expression (e.g., "hole_dia = shaft_dia + 0.1")
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyTable {
    pub columns: Vec<FamilyColumn>,
    pub rows: Vec<FamilyRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyColumn {
    pub parameter_name: String,
    pub column_type: ColumnType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    Dimension,    // Driving dimension
    Parameter,    // General parameter
    Feature,      // Feature suppression state
    Material,     // Material assignment
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyRow {
    pub instance_name: String,
    pub values: HashMap<String, String>,  // Column name -> value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegenerationMode {
    Automatic,  // Rebuild on any change (default)
    Manual,     // User must trigger rebuild
    Suppressed, // Part is suppressed in assembly
}
```

#### 3.2 Feature Tree Operations - Detailed Implementation

| Operation | Implementation Details |
|-----------|------------------------|
| **Drag-Reorder** | `move_feature(part, feature_id, new_index)` in rcad-features; validate: no circular refs (child before parent), no moving suppressed features past children, update `insert_here_index`/`rollback_index` if affected |
| **Insert Here** | Right-click feature → "Insert Here" → sets `part.insert_here_index = index`; visual marker (▶) in tree; new features insert at this index; "End of Part" resets to `None` |
| **Rollback** | Right-click feature → "Rollback Here" → sets `part.rollback_index = index`; rebuild stops at this feature; features after show as "rolled back" (grayed); "Rollback End" resets to `None` |
| **Suppress/Resume** | Existing `suppress/unsuppress` in rcad-features; auto-suppress children; visual: strikethrough name, 🚫 icon; resume validates references still valid |
| **Edit Definition** | Double-click or right-click → "Edit Definition" → opens feature-specific dashboard (see Phase 4 Dashboard); pre-fills with current parameters & references; on OK: update feature, mark dirty, rebuild |
| **Edit References** | Right-click → "Edit References" → shows current references with status (✓ valid, ✗ missing); user can re-select geometry in viewport; validates new reference compatible |
| **Parent/Child Graph** | Computed on demand: traverse `FeatureReferences`; display in tree with indentation or separate "Parents"/"Children" tabs; "Find Parents/Children" in context menu |
| **Delete with Dependencies** | Right-click → "Delete" → shows dialog: "This feature has X children. Delete anyway? (breaks children)" / "Delete & Children" / "Cancel" |
| **Rename** | F2 or right-click → "Rename" → inline edit in tree; updates feature.name |
| **Copy/Paste** | Copy feature (with references); paste at Insert Here position; auto-rename (name + "_copy"); references preserved if valid in new context |

#### 3.3 Context Menu Actions (Complete)
```
┌─────────────────────────────────┐
│ Edit Definition...         Ctrl+E │
│ Edit References...              │
│ ───────────────────────────────  │
│ Suppress                   Ctrl+H │
│ Resume                               │
│ ───────────────────────────────  │
│ Insert Here                        │
│ Rollback Here                      │
│ ───────────────────────────────  │
│ Rename                         F2 │
│ Delete                          Del │
│ ───────────────────────────────  │
│ Copy                          Ctrl+C │
│ Paste                         Ctrl+V │
│ Paste Special...                   │
│ ───────────────────────────────  │
│ Parents                            │
│ Children                           │
│ Information...                     │
└─────────────────────────────────┘
```

#### 3.4 Visual Indicators in Tree
| State | Icon/Style | Description |
|-------|------------|-------------|
| Active | ✅ Solid icon | Normal feature |
| Suppressed | 🚫 Strikethrough + gray | Suppressed feature |
| Insert Here | ▶ Right arrow before feature | Insertion point |
| Rollback | ⏪ Left arrow before feature | Rollback point |
| Error | ❌ Red icon + red text | Regeneration failed |
| Warning | ⚠️ Yellow icon | Regenerated with warnings |
| External Ref | 🔗 Link icon | Feature has external references |
| Out-of-date | 🔗⚠️ Yellow link | External ref needs update |
| Broken Ref | 🔗❌ Red link | External ref broken |

#### 3.5 History Overlay Integration
```rust
// In app.rs - HistoryOverlay state
struct HistoryOverlay {
    visible: bool,
    hovered: bool,
    tree_state: TreeState,  // Expanded/collapsed nodes
    insert_here_index: Option<usize>,
    rollback_index: Option<usize>,
    selected_feature: Option<EntityId>,
    show_parents: bool,  // Toggle parent highlighting
    show_children: bool, // Toggle child highlighting
}

struct TreeState {
    expanded_nodes: HashSet<EntityId>,
    // Persist across sessions
}

// Rendering in egui:
// - Use egui::ScrollArea for virtual scrolling
// - Indent based on tree depth (parent/child)
// - Right-click → context menu (above)
// - Hover → show tooltip with feature info
// - Click → select feature (highlight in viewport)
// - Double-click → Edit Definition
// - Drag → reorder (with validation)
```

### Phase 4: Intent Manager & Sketch Enhancements 🔴

#### 4.1 Weak/Strong Dimension UX - Complete Implementation

**Data Model (rcad-sketch):**
```rust
// Enhanced ConstrainedConstraint in rcad-sketch/src/lib.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstrainedConstraint {
    pub constraint: Constraint,
    pub strength: ConstraintStrength,
    pub is_driving: bool,
    // NEW: Intent Manager metadata
    pub auto_generated: bool,           // True for weak dimensions
    pub promoted_from: Option<Uuid>,    // Weak constraint ID this replaced
    pub dimension_text_position: [f64; 2], // User-adjusted text position
    pub dimension_orientation: DimensionOrientation, // Horizontal, Vertical, Parallel, Radial, Diameter
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DimensionOrientation {
    Horizontal,    // Dimension text horizontal
    Vertical,      // Dimension text vertical
    Parallel,      // Parallel to dimension line
    Radial,        // Radial dimension (for circles/arcs)
    Diameter,      // Diameter dimension (for circles)
}

impl Sketch {
    // ... existing methods ...
    
    /// Add a dimension with full Intent Manager support
    pub fn add_dimension_with_intent(
        &mut self,
        a: Uuid,
        b: Uuid,
        value: f64,
        driving: bool,
        strong: bool,
        orientation: DimensionOrientation,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let constraint = Constraint::Distance { a, b, value };
        
        // If strong, find and remove corresponding weak dimension
        if strong {
            self.promote_weak_to_strong(a, b);
        }
        
        self.constraints.push(constraint);
        self.constrained_constraints.push(ConstrainedConstraint {
            constraint,
            strength: if strong { ConstraintStrength::Strong } else { ConstraintStrength::Weak },
            is_driving: driving,
            auto_generated: !strong,
            promoted_from: None,
            dimension_text_position: self.compute_default_text_pos(a, b),
            dimension_orientation: orientation,
        });
        id
    }
    
    /// Auto-promote weak to strong when user adds dimension on same entities
    fn promote_weak_to_strong(&mut self, a: Uuid, b: Uuid) {
        // Find weak distance constraint on same entities
        if let Some(idx) = self.constrained_constraints.iter().position(|cc| {
            matches!(&cc.constraint, Constraint::Distance { a: ca, b: cb, .. }
                if (*ca == a && *cb == b) || (*ca == b && *cb == a))
            && cc.strength == ConstraintStrength::Weak
        }) {
            if let Some(weak_cc) = self.constrained_constraints.get(idx).cloned() {
                // Remove weak constraint
                self.constrained_constraints.remove(idx);
                self.constraints.retain(|c| !matches!(c, Constraint::Distance { a: ca, b: cb, .. }
                    if (*ca == a && *cb == b) || (*ca == b && *cb == a)));
                // Note: promoted_from could track this for undo
            }
        }
    }
    
    /// Get DOF analysis with color coding
    pub fn analyze_dof_detailed(&self) -> DOFAnalysis {
        let (total, constrained, redundant) = self.analyze_dof();
        let free = total.saturating_sub(constrained);
        let status = if redundant > 0 { ConstraintStatus::OverConstrained }
                     else if free == 0 { ConstraintStatus::FullyConstrained }
                     else { ConstraintStatus::UnderConstrained };
        
        DOFAnalysis {
            total_dof: total,
            constrained_dof: constrained,
            free_dof: free,
            redundant_constraints: redundant,
            status,
            unconstrained_entities: self.find_unconstrained_entities(),
        }
    }
    
    fn find_unconstrained_entities(&self) -> Vec<UnconstrainedEntity> {
        // Analyze which points/lines/circles have DOF
        // Return list for UI highlighting
        vec![]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DOFAnalysis {
    pub total_dof: usize,
    pub constrained_dof: usize,
    pub free_dof: usize,
    pub redundant_constraints: usize,
    pub status: ConstraintStatus,
    pub unconstrained_entities: Vec<UnconstrainedEntity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintStatus {
    UnderConstrained,
    FullyConstrained,
    OverConstrained,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnconstrainedEntity {
    pub entity_type: EntityType,
    pub entity_id: Uuid,
    pub free_dof: usize,  // 1 or 2 for points, 1 for lines (angle), etc.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    Point, Line, Circle, Arc, Spline,
}
```

**Visual Distinction in Viewport/Sketch Panel:**
| Dimension Type | Line Style | Text Color | Text Style | Behavior |
|----------------|------------|------------|------------|----------|
| Weak Driving | Dashed (2px dash, 2px gap) | Gray (#888) | Italic | Auto-deleted on promotion |
| Strong Driving | Solid (1.5px) | Black (#000) | Normal | Permanent |
| Weak Driven | Dashed + gray | Gray (#888) | Italic + parentheses | Reference only |
| Strong Driven | Solid + gray | Gray (#666) | Normal + parentheses | Reference only |

**Interaction:**
- **Hover dimension**: Show tooltip with value, driving/driven, weak/strong
- **Right-click dimension**: Context menu → Toggle Driving/Driven, Promote to Strong, Delete, Edit Value, Flip Arrows
- **Drag dimension text**: Updates `dimension_text_position` (persists)
- **Double-click dimension**: In-place edit value (driving only)

#### 4.2 Reference Geometry Improvements - Complete

**Projection Modes (Creo Parity):**
```rust
// In rcad-sketch - add to Sketch struct
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectionMode {
    OneSide,     // Project in direction of sketch plane normal
    BothSides,   // Project both directions (through)
    AlongVector, // Project along custom vector
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceGeometry {
    // ... existing fields ...
    pub projection_mode: ProjectionMode,
    pub source_feature: Option<EntityId>,  // Feature that generated this reference
    pub source_body: Option<EntityId>,     // Body containing source geometry
    pub is_associative: bool,              // True = updates with model changes
    pub status: ExternalRefStatus,         // UpToDate, OutOfDate, Broken
}

impl Sketch {
    /// Project 3D edges/vertices onto sketch plane with mode
    pub fn project_references(
        &mut self,
        entities: Vec<ReferenceEntity>,
        mode: ProjectionMode,
        source_feature: Option<EntityId>,
    ) {
        for entity in entities {
            let projected = self.project_entity_3d(&entity, mode);
            self.reference_geometry.push(ReferenceGeometry {
                id: Uuid::new_v4(),
                name: entity.name,
                kind: entity.kind,
                projected_points: projected,
                projection_mode: mode,
                source_feature,
                source_body: entity.body_id,
                is_associative: true,
                status: ExternalRefStatus::UpToDate,
            });
        }
    }
    
    /// Update associative references (called on model rebuild)
    pub fn update_references(&mut self, project: &Project) -> Vec<ReferenceUpdateResult> {
        // For each associative reference, re-project from source
        // Compare with current; if changed → mark OutOfDate
        // Return list of changes for UI notification
        vec![]
    }
}
```

**Reference Geometry Panel (in Sketch Mode):**
```
┌─────────────────────────────────────┐
│ Reference Geometry          [⚙] [×] │
├─────────────────────────────────────┤
│ ☐ Auto-project on sketch create     │
│ ─────────────────────────────────   │
│ ✅  Edge: EXTRUDE_1.FACE_3.EDGE_2   │
│     Mode: One-Side  Status: ✅      │
│     [👁] [🔄] [🗑]                  │
│ ⚠️  Vertex: HOLE_1.FACE_1.VERT_4   │
│     Mode: Both-Sides  Status: ⚠️    │
│     [👁] [🔄] [🗑]                  │
│ ─────────────────────────────────   │
│ [Project Selected Edges]            │
│ [Project Selected Vertices]         │
│ [Intersect with Face]               │
└─────────────────────────────────────┘
```

**Auto-Projection on Sketch Creation:**
1. User activates "New Sketch" tool
2. If entities pre-selected (faces, edges, vertices) → auto-project with One-Side
3. If face selected for plane → offer to project its silhouette edges
4. Show "Reference Geometry" panel automatically when refs exist

#### 4.3 Sketch Plane Selection Workflow - Creo Style

**Complete Workflow:**
```rust
// In app.rs - Sketch creation state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SketchCreationStep {
    SelectPlane,      // Step 1: Select planar face, datum plane, or default
    SelectOrientation, // Step 2: Select edge/axis for vertical (if face selected)
    Confirm,          // Step 3: Confirm or adjust
}

struct SketchCreator {
    step: SketchCreationStep,
    selected_face: Option<PickedEntity>,
    selected_plane: Option<DatumPlane>,
    selected_orientation: Option<PickedEntity>,  // Edge or axis
    sketch_plane: Option<SketchPlaneDefinition>,
    preview_triad: Option<Triad>,
}

impl RcadApp {
    pub fn start_new_sketch(&mut self) {
        self.sketch_creator = Some(SketchCreator {
            step: SketchCreationStep::SelectPlane,
            selected_face: None,
            selected_plane: None,
            selected_orientation: None,
            sketch_plane: None,
            preview_triad: None,
        });
        self.status_message = "Select planar face, datum plane, or click empty space for default plane".to_string();
    }
    
    fn handle_sketch_creation(&mut self, ctx: &egui::Context) {
        if let Some(creator) = &mut self.sketch_creator {
            match creator.step {
                SketchCreationStep::SelectPlane => {
                    // Highlight planar faces on hover
                    // On click: if face → store, advance to orientation
                    //           if datum plane → store, skip to confirm
                    //           if empty → use default XY, skip to confirm
                }
                SketchCreationStep::SelectOrientation => {
                    // Show triad on selected plane
                    // Highlight linear edges/axes on hover
                    // On click: if edge/axis → store, advance to confirm
                    //           if empty → use default (longest edge), advance
                }
                SketchCreationStep::Confirm => {
                    // Show preview triad
                    // [OK] creates sketch, [Back] returns to orientation
                }
            }
        }
    }
}
```

**Sketch Coordinate System Triad:**
- Always visible while sketch is active
- Origin at sketch plane origin
- X (red), Y (green), Z (blue) axes
- Labels: "H" (horizontal), "V" (vertical)
- Click triad axes to flip sketch orientation

#### 4.4 Additional Sketch Enhancements for Creo Parity

**Constraint Diagnostic Tools:**
- **Show DOF**: Toggle arrows on unconstrained entities
- **Highlight Conflicts**: Red highlight on over-constrained sets
- **Constraint Table**: Spreadsheet view of all constraints (editable)
- **Drag Test**: Hold Shift+drag to test DOF visually

**Sketch Tools (Sketch Tab):**
| Tool | Implementation |
|------|----------------|
| Line Chain | Continuous line creation (click-click-click, Esc to end) |
| Tangent Arc | Arc tangent to previous line |
| 3-Point Arc | Click start, end, pass-through |
| Conic | Rho parameter conic section |
| Text | TrueType text as sketch geometry |
| Import DXF/SVG | Convert to sketch entities |
| Sketch Region | Auto-detect closed regions for selection |

**Sketch State Management:**
- **Active Sketch**: Only one sketch active at a time
- **Exit Sketch**: Auto-solve, validate profile, create feature or save sketch
- **Edit Sketch**: Double-click sketch in tree → re-activate sketch mode
- **Sketch Visibility**: Toggle in tree; ghost mode when editing other features

### Phase 5: Skeleton Workflow Enhancements 🟠

#### 5.1 Publish Geometry - Complete Implementation

**Data Model (rcad-skeleton / rcad-doc):**
```rust
// Enhanced PublishedRef in rcad-doc/src/lib.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedRef {
    pub id: EntityId,
    pub name: String,
    pub kind: ConstructionKind,
    // NEW: Creo-style publish metadata
    pub description: Option<String>,
    pub feature_id: EntityId,           // Source construction feature
    pub body_id: Option<EntityId>,      // Source body (for solid/surface publish)
    pub selection_query: String,        // Stable selection query for regeneration
    pub publish_type: PublishType,      // Geometry, Shrinkwrap, etc.
    pub include_children: bool,         // For assemblies: include sub-components
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublishType {
    Geometry,       // Standard publish (construction + solid geometry)
    Shrinkwrap,     // Simplified envelope (for large assemblies)
    CopyGeometry,   // Published specifically for Copy Geometry
    Interface,      // For mechanism connections
}

impl Part {
    /// Publish Geometry dialog data
    pub fn get_publishable_items(&self) -> Vec<PublishableItem> {
        let mut items = Vec::new();
        for feature in &self.features {
            if let FeatureKind::Construction { kind, .. } = &feature.kind {
                items.push(PublishableItem {
                    feature_id: feature.id,
                    name: feature.name.clone(),
                    kind: *kind,
                    preview_geometry: self.get_construction_preview(feature.id),
                });
            }
        }
        // Also include solid bodies for shrinkwrap
        for body in &self.bodies {
            if body.is_solid {
                items.push(PublishableItem {
                    feature_id: body.feature_id,
                    name: format!("BODY:{}", body.name),
                    kind: ConstructionKind::Solid,
                    preview_geometry: Some(body.id),
                });
            }
        }
        items
    }
}

#[derive(Debug, Clone)]
pub struct PublishableItem {
    pub feature_id: EntityId,
    pub name: String,
    pub kind: ConstructionKind,
    pub preview_geometry: Option<EntityId>,  // For viewport preview
}
```

**Publish Geometry Dialog (Model Tab → Publish Geometry):**
```
┌─────────────────────────────────────────────┐
│ Publish Geometry                    [×]     │
├─────────────────────────────────────────────┤
│ Skeleton: SKELETON_1              [Refresh] │
│ Publish Type: [Geometry ▼]                  │
│ Name Prefix: [PUB_]                         │
│ ─────────────────────────────────────────   │
│ ☐ Show only unpublished                     │
│ ─────────────────────────────────────────   │
│ ☑  PUB_FRONT_PLANE      Plane      [👁]     │
│ ☑  PUB_RIGHT_PLANE      Plane      [👁]     │
│ ☑  PUB_TOP_PLANE        Plane      [👁]     │
│ ☑  PUB_CENTER_AXIS      Axis       [👁]     │
│ ☐  PUB_MOUNT_HOLES      Curve      [👁]     │
│ ☐  BODY:ENVELOPE        Solid      [👁]     │
│ ─────────────────────────────────────────   │
│ [Select All] [Clear All]                    │
│ ─────────────────────────────────────────   │
│ Description: [________________________]     │
│ ─────────────────────────────────────────   │
│ [Publish] [Cancel]                          │
└─────────────────────────────────────────────┘
```

**Implementation:**
```rust
// In rcad-skeleton/src/lib.rs
pub fn publish_geometry_dialog(part: &mut Part, selections: &[PublishSelection]) -> Result<(), SkeletonError> {
    for sel in selections {
        if !sel.publish { continue; }
        let feature = part.features.iter().find(|f| f.id == sel.feature_id)
            .ok_or(SkeletonError::FeatureNotFound)?;
        
        let pub_ref = PublishedRef {
            id: feature.id,  // Use feature ID as published ID
            name: sel.name.clone(),
            kind: sel.kind,
            description: sel.description.clone(),
            feature_id: feature.id,
            body_id: sel.body_id,
            selection_query: generate_selection_query(feature, part),
            publish_type: sel.publish_type,
            include_children: false,
        };
        
        // Avoid duplicates
        if !part.published.iter().any(|p| p.id == pub_ref.id) {
            part.published.push(pub_ref);
        }
    }
    Ok(())
}
```

#### 5.2 Copy Geometry - Complete Implementation

**Copy Geometry Dialog (Model Tab → Copy Geometry):**
```
┌─────────────────────────────────────────────┐
│ Copy Geometry                       [×]     │
├─────────────────────────────────────────────┤
│ Source Skeleton: [SKELETON_1 ▼]  [Browse]   │
│ ─────────────────────────────────────────   │
│ Filter: [All ▼]  Search: [__________]       │
│ ─────────────────────────────────────────   │
│ ☑  PUB_FRONT_PLANE      Plane      ✅       │
│ ☑  PUB_RIGHT_PLANE      Plane      ✅       │
│ ☐  PUB_TOP_PLANE        Plane      ✅       │
│ ☑  PUB_CENTER_AXIS      Axis       ✅       │
│ ☐  PUB_MOUNT_HOLES      Curve      ⚠️       │
│ ─────────────────────────────────────────   │
│ Options:                                    │
│ ☑  Copy as Reference (associative)          │
│ ☐  Independent Copy (break associativity)   │
│ ☑  Create Datum Geometry                    │
│ ☐  Flatten (project to single plane)        │
│ ─────────────────────────────────────────   │
│ [Select All] [Clear All] [Invert]           │
│ ─────────────────────────────────────────   │
│ [Copy] [Cancel]                             │
└─────────────────────────────────────────────┘
```

**Implementation:**
```rust
// In rcad-skeleton/src/lib.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyGeometryOptions {
    pub copy_as_reference: bool,      // True = associative, False = independent
    pub create_datum_geometry: bool,  // Create construction features in child
    pub flatten: bool,                // Project all to sketch plane
    pub target_plane: Option<EntityId>, // For flatten: target plane in child
}

pub fn copy_geometry(
    project: &mut Project,
    child_part_id: EntityId,
    skeleton_id: EntityId,
    selections: &[CopySelection],
    options: CopyGeometryOptions,
) -> Result<Vec<EntityId>, SkeletonError> {
    let skeleton = project.part(skeleton_id).ok_or(SkeletonError::SkeletonNotFound)?;
    let child = project.part_mut(child_part_id).ok_or(SkeletonError::PartNotFound)?;
    
    if child.kind == PartKind::Skeleton {
        return Err(SkeletonError::CannotCopyToSkeleton);
    }
    
    // Set skeleton relationship
    child.skeleton_of = Some(skeleton_id);
    
    let mut created_features = Vec::new();
    
    for sel in selections {
        if !sel.copy { continue; }
        
        let published = skeleton.published.iter()
            .find(|p| p.id == sel.published_id)
            .ok_or(SkeletonError::PublishedNotFound)?;
        
        if options.copy_as_reference {
            // Create associative reference feature
            let ref_feature = Feature::new(
                format!("REF_{}", published.name),
                FeatureKind::Construction {
                    kind: published.kind,
                    origin: [0.0, 0.0, 0.0],  // Will be resolved from skeleton
                    direction: [0.0, 0.0, 1.0],
                },
            );
            
            // Store external reference metadata
            ref_feature.references.sketch_plane = Some(Reference {
                entity_id: published.feature_id,
                entity_type: ReferenceType::Plane,
                stable_name: published.name.clone(),
                selection_query: published.selection_query.clone(),
            });
            ref_feature.parameters.string.insert(
                "external_ref_source".to_string(),
                format!("{}:{}", skeleton_id, published.id)
            );
            ref_feature.parameters.boolean.insert("is_external_ref".to_string(), true);
            
            child.features.push(ref_feature);
            child.published.push(PublishedRef {
                id: ref_feature.id,
                name: format!("REF_{}", published.name),
                kind: published.kind,
                ..published.clone()
            });
            created_features.push(ref_feature.id);
        } else {
            // Independent copy: resolve geometry now, create static feature
            let geometry = resolve_published_geometry(project, skeleton, published)?;
            let static_feature = create_static_construction_feature(&geometry, &published.name);
            child.features.push(static_feature);
            created_features.push(static_feature.id);
        }
    }
    
    // Mark all as UpToDate initially
    for pub_ref in &mut child.published {
        pub_ref.status = ExternalRefStatus::UpToDate;  // Add status field
    }
    
    Ok(created_features)
}
```

#### 5.3 External Reference Manager - Complete

**External Reference Manager Panel (Tools Tab → External Refs):**
```
┌─────────────────────────────────────────────────────────┐
│ External Reference Manager                     [×]      │
├─────────────────────────────────────────────────────────┤
│ Part: BRACKET_1 (child of SKELETON_1)                   │
│ ─────────────────────────────────────────────────────── │
│ Status  | Reference Name      | Source      | Actions   │
│ ─────────────────────────────────────────────────────── │
│ ✅      | REF_FRONT_PLANE     | SKELETON_1  | [🔄][🔗][✂] │
│ ✅      | REF_RIGHT_PLANE     | SKELETON_1  | [🔄][🔗][✂] │
│ ⚠️      | REF_MOUNT_HOLES     | SKELETON_1  | [🔄][🔗][✂] │
│ ❌      | REF_OLD_AXIS        | SKELETON_1  | [🔄][🔗][✂] │
│ ─────────────────────────────────────────────────────── │
│ [Update All] [Replace...] [Break All] [Close]           │
└─────────────────────────────────────────────────────────┘
```

**Status Definitions:**
| Status | Icon | Meaning | Action |
|--------|------|---------|--------|
| UpToDate | ✅ | Source geometry matches | None needed |
| OutOfDate | ⚠️ | Source geometry changed | Update available |
| Broken | ❌ | Source deleted/renamed | Replace or Break |

**Actions:**
- **Update (🔄)**: Re-resolve reference from source; regenerate child part
- **Replace (🔗)**: Select new source geometry in skeleton or other part
- **Break (✂)**: Convert to independent geometry; remove associativity

**Implementation:**
```rust
// In rcad-skeleton/src/lib.rs
pub fn update_external_references(
    project: &mut Project,
    part_id: EntityId,
) -> Result<Vec<ExternalRefUpdateResult>, SkeletonError> {
    let part = project.part_mut(part_id).ok_or(SkeletonError::PartNotFound)?;
    let skeleton_id = part.skeleton_of.ok_or(SkeletonError::NoSkeleton)?;
    let skeleton = project.part(skeleton_id).ok_or(SkeletonError::SkeletonNotFound)?;
    
    let mut results = Vec::new();
    
    for pub_ref in &mut part.published {
        // Find corresponding published in skeleton
        let source = skeleton.published.iter()
            .find(|p| p.name == pub_ref.name.strip_prefix("REF_").unwrap_or(&pub_ref.name));
        
        match source {
            Some(src) => {
                // Re-resolve geometry
                let new_geometry = resolve_published_geometry(project, skeleton, src)?;
                
                // Compare with current (simplified: check feature hash/timestamp)
                let changed = geometry_changed(&pub_ref, &new_geometry);
                
                if changed {
                    pub_ref.status = ExternalRefStatus::OutOfDate;
                    results.push(ExternalRefUpdateResult {
                        ref_id: pub_ref.id,
                        name: pub_ref.name.clone(),
                        status: ExternalRefStatus::OutOfDate,
                        action_needed: "Update",
                    });
                } else {
                    pub_ref.status = ExternalRefStatus::UpToDate;
                }
            }
            None => {
                pub_ref.status = ExternalRefStatus::Broken;
                results.push(ExternalRefUpdateResult {
                    ref_id: pub_ref.id,
                    name: pub_ref.name.clone(),
                    status: ExternalRefStatus::Broken,
                    action_needed: "Replace or Break",
                });
            }
        }
    }
    
    Ok(results)
}

pub fn replace_external_reference(
    project: &mut Project,
    part_id: EntityId,
    ref_id: EntityId,
    new_source: PublishedRef,  // From same or different skeleton
) -> Result<(), SkeletonError> {
    // Update the reference feature to point to new source
    // Regenerate part
    Ok(())
}

pub fn break_external_reference(
    project: &mut Project,
    part_id: EntityId,
    ref_id: EntityId,
) -> Result<(), SkeletonError> {
    // Convert reference feature to static geometry
    // Remove from published list
    // Remove external reference metadata
    Ok(())
}

// External Reference Status in Part struct (add to rcad-doc)
impl PublishedRef {
    pub status: ExternalRefStatus,  // Add this field
}
```

#### 5.4 Skeleton-Specific Enhancements

**Skeleton Creation Wizard (Model Tab → New Skeleton):**
```
┌─────────────────────────────────────────────┐
│ New Skeleton                          [×]   │
├─────────────────────────────────────────────┤
│ Name: [SKELETON_1_______________]           │
│ ─────────────────────────────────────────   │
│ Template: [Standard ▼]                      │
│ ─────────────────────────────────────────   │
│ ☑  Create Datum Planes (Front, Right, Top)  │
│ ☑  Create Datum Axes (X, Y, Z)              │
│ ☑  Create Center Point                       │
│ ☑  Create Coordinate System                 │
│ ☐  Create Parameter Table                    │
│ ─────────────────────────────────────────   │
│ Auto-Publish: [All Construction ▼]          │
│ ─────────────────────────────────────────   │
│ [Create] [Cancel]                           │
└─────────────────────────────────────────────┘
```

**Skeleton Editing in Assembly Context (Top-Down Design):**
- Activate skeleton in assembly → edit skeleton features
- Changes propagate to child parts on next rebuild
- "Update External Refs" button in assembly toolbar
- Visual indicators in assembly tree for out-of-date parts

### Phase 6: Surface Color Rendering 🟠

#### 6.1 Color Application
```rust
// In rcad-occt tessellation or viewport rendering
fn apply_surface_colors(mesh: &mut TriangleMesh, bodies: &[Body], config: &AppConfig) {
    for (body_id, body) in bodies.iter().enumerate() {
        if let Some(color_name) = &body.surface_type {  // e.g., "threads", "fit_surfaces"
            if let Some(rgb) = config.surface_colors.get(color_name) {
                // Apply color to mesh triangles belonging to this body
            }
        }
    }
}
```

#### 6.2 Surface Type Assignment
- Add `surface_type: Option<String>` to `Body` struct
- UI in feature creation: "Surface Type" dropdown (from rcad.toml keys)
- Default types: threads, fit_surfaces, sealing_zones, custom

### Phase 7: External Tables Integration 🟠

#### 7.1 rcad-table Connection
- Load tables on project open (from `external_tables.default_path`)
- Map table columns to feature parameters
- UI: "Link Parameter to Table" in Edit Definition dialog

#### 7.2 Parameter Driving
```rust
// In FeatureKind or separate Parameter struct
pub struct TableDrivenParameter {
    pub table_name: String,
    pub column_name: String,
    pub row_key: String,  // e.g., part name or configuration
    pub current_value: f64,
}
```

### Phase 8: Trail Logging 🟠

#### 8.1 JSONL Logger
```rust
struct TrailLogger {
    file: std::fs::File,
    max_size_mb: u64,
    max_files: usize,
    current_size: u64,
}

impl TrailLogger {
    fn log(&mut self, action: &str, params: serde_json::Value, result: &str, duration_ms: u64) {
        let entry = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "action": action,
            "params": params,
            "result": result,
            "duration_ms": duration_ms,
        });
        // Write line, handle rotation
    }
}
```

#### 8.2 Integration Points
- Feature creation/edit/delete
- Sketch operations
- File operations (open/save/export)
- Rebuild operations
- Assembly mate operations

### Phase 9: Project Initialization & File Associations 🟡

#### 9.1 First Run Experience
1. Check `~/.config/rcad/recent.json`
2. If empty → show "Create New Project" dialog
3. Auto-save to recent list
4. Next startup → auto-open last project

#### 9.2 OS File Association
- Linux: `.desktop` file + `xdg-mime`
- macOS: `Info.plist` + `LSHandlers`
- Windows: Registry keys for `.rcad`, `.rcadprt`, etc.

### Phase 10: MCP Server Implementation 🟡

#### 10.1 Resources
- `rcad://project/{id}` - Project metadata
- `rcad://part/{id}` - Part with feature tree
- `rcad://sketch/{id}` - Sketch entities & constraints
- `rcad://assembly/{id}` - Assembly instances & mates

#### 10.2 Tools
- `create_part`, `add_sketch`, `add_extrude`, `add_revolve`, `add_boolean`
- `create_assembly`, `add_mate`, `export_step`, `export_stl`
- `get_project_state`, `rebuild_part`

#### 10.3 Prompts
- `new_part_from_sketch`, `create_assembly_from_parts`
- `create_skeleton_driven_part`, `reverse_engineer_from_image`

---

## 4. Detailed UI Layout Specification

### 4.1 Window Layout (16:9 Optimized)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  ┌──────┐                                                                   │
│  │ ☰    │  VERTICAL TOOLBAR (collapsible)                                   │
│  ├──────┤  ┌─────────────────────────────────────────────────────────────┐  │
│  │ Model│  │                                                             │  │
│  ├──────┤  │                                                             │  │
│  │Sketch│  │                    VIEWPORT                                   │  │
│  ├──────┤  │                    (wgpu canvas)                              │  │
│  │Asm   │  │                                                             │  │
│  ├──────┤  │                    ┌─────────────────────────────┐          │  │
│  │Analy │  │                    │  ADAPTIVE HISTORY OVERLAY    │          │  │
│  ├──────┤  │                    │  (blur on hover, transparent)│          │  │
│  │View  │  │                    │  ┌─────────────────────────┐ │          │  │
│  ├──────┤  │                    │  │ Feature Tree             │ │          │  │
│  │Tools │  │                    │  │  ├─ Base (Extrude)       │ │          │  │
│  ├──────┤  │                    │  │  │  ├─ Sketch1            │ │          │  │
│  │  ✦   │  │                    │  │  ├─ Hole (Cut)           │ │◀InsertHere│  │
│  └──────┘  │                    │  │  │  ├─ Sketch2            │ │          │  │
│            │                    │  │  ├─ Fillet               │ │          │  │
│            │                    │  │  ├─ Chamfer 🚫           │ │          │  │
│            │                    │  │  └─ Shell                │ │          │  │
│            │                    │  └─────────────────────────┘ │          │  │
│            └────────────────────┴─────────────────────────────┘          │  │
├─────────────────────────────────────────────────────────────────────────────┤
│  STATUS BAR:  Camera: [x,y,z]  |  Selection: Face 3  |  rcad - Open Source CAD  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Toolbar Tabs & Buttons (Creo Parity)

| Tab | Buttons (Icon + Text) |
|-----|----------------------|
| **Model** | Box, Extrude, Revolve, Sweep, Loft, Boundary, Fillet, Chamfer, Shell, Draft, Hole, Pattern, Mirror, Boolean, Cosmetic Thread |
| **Sketch** | New Sketch, Line, Rectangle, Circle, Arc, Spline, Point, Constraint (Horiz, Vert, Par, Perp, Tang, Equal, Symm, Conc, Mid, Coin), Dimension, Trim, Extend, Offset, Mirror, Pattern, Project References |
| **Assembly** | Insert Component, Coincident, Concentric, Distance, Angle, Parallel, Perpendicular, Tangent, Symmetric, Exploded View, Sub-Assembly |
| **Analysis** | Measure, Mass Properties, Curvature, Draft, Thickness, Interference, Clearance |
| **View** | Front, Back, Left, Right, Top, Bottom, Iso, Perspective/Ortho, Shaded/Wireframe, Zoom Fit, Previous View |
| **Tools** | Parameters, Relations, Family Table, Design Exploration, Options, Customize |

### 4.3 Feature Tree Context Menu (Right-Click)

```
┌─────────────────────────┐
│ Edit Definition...      │
│ ─────────────────────   │
│ Suppress                │
│ Resume                  │
│ ─────────────────────   │
│ Insert Here             │
│ Rollback Here           │
│ ─────────────────────   │
│ Rename                  │
│ Delete                  │
│ ─────────────────────   │
│ Copy                    │
│ Paste                   │
│ Paste Special...        │
│ ─────────────────────   │
│ Parents                 │
│ Children                │
│ Info                    │
└─────────────────────────┘
```

---

## 5. Data Flow & Architecture

### 5.1 Configuration Flow
```
rcad.toml (file)
    ↓
AppConfig::load() (on startup)
    ↓
RcadApp.config: AppConfig
    ├─→ Sketch solver (tolerances)
    ├─→ Viewport renderer (surface colors)
    ├─→ Feature creation (default parameters)
    ├─→ Trail logger (config)
    └─→ MCP server (config)
```

### 5.2 Feature Creation Flow (Dashboard-style)
```
User clicks "Extrude" in Model tab
    ↓
Show Extrude Dashboard (right overlay or panel)
    ├─ Profile: [Select sketch / face] ▼
    ├─ Depth: [10.0] [Blind | Symmetric | Through All | To Next | To Selected]
    ├─ Draft: [0°] [□ Draft Outward]
    ├─ Direction: [↑] [Flip]
    ├─ [OK] [Cancel] [Preview]
    ↓
On OK: Create FeatureKind::Extrude, add to Part at insert position
    ↓
Trigger rebuild_part()
    ↓
Update viewport mesh
    ↓
Log to trail
```

### 5.3 Skeleton Publish/Copy Flow
```
SKELETON PART                          CHILD PART
─────────────────                      ─────────────
User selects construction features    User: "Copy Geometry"
    ↓                                      ↓
Publish Geometry dialog                Select skeleton part
    ↓                                      ↓
Name each published entity              Copy Geometry dialog
    ↓                                      ↓
Store in Part.published[]               Select published entities
    ↓                                      ↓
                                       Create Construction features
                                       with REF_ prefix
                                       Store in child Part.published[]
                                       Mark ExternalRefStatus::UpToDate
```

---

## 6. Implementation Priority & Dependencies

### Priority 1 (Must Have - Core UX)
1. **Vertical Toolbar + Adaptive History Overlay** (UI-1)
2. **rcad.toml Integration** (UI-2)
3. **Feature Tree: Insert Here, Rollback, Parent/Child, Edit Definition**
4. **Sketch Plane Selection Workflow**

### Priority 2 (Should Have - Parity Features)
5. **Surface Color Rendering** (UI-3)
6. **External Tables Integration** (UI-4)
7. **Trail Logging** (PRJ-2)
8. **Project Initialization** (PRJ-1)

### Priority 3 (Nice to Have - Polish)
9. **File Associations** (PRJ-3)
10. **MCP Server** (PRJ-4)
11. **Reference Geometry Projection Modes**
12. **Dashboard-style Feature Creation**

### Dependencies
```
UI-1 (Vertical Toolbar) ──┐
                          ├──→ Feature Tree Enhancements need UI-1
UI-2 (rcad.toml) ─────────┤
                          ├──→ Surface Colors need UI-2
Feature Tree ─────────────┘
```

---

## 7. Technical Considerations

### 7.1 egui/wgpu Integration for Blur Effect
```rust
// Custom shader for history overlay blur
// In viewport render pass, render history overlay to texture
// Apply Gaussian blur when hovered
// Composite over viewport
```

### 7.2 Undo/Redo with Config
- Config changes (rcad.toml edits) should be undoable
- Store config snapshot in UndoStack

### 7.3 Performance
- Feature tree: Virtual scrolling for large trees
- Rebuild: Incremental (only dirty features)
- Viewport: Frustum culling, LOD

### 7.4 Serialization
- Add new fields to `Feature`, `Part`, `Project` with `#[serde(default)]`
- Version migration in `Project::load()`

---

## 8. Acceptance Criteria

### 8.1 MVP Usable (6 months)
- [ ] Create parametric part with 5+ features
- [ ] Use skeleton with Publish/Copy Geometry
- [ ] Import underlay, calibrate scale, trace sketch
- [ ] Assemble 3 parts with basic mates
- [ ] Save/load project, export STEP/STL
- [ ] rcad.toml controls tolerances, colors, tables
- [ ] Vertical toolbar + adaptive history overlay working

### 8.2 Creo Parity (This Plan)
- [ ] Full Intent Manager (weak/strong, driving/driven, DOF)
- [ ] Complete feature tree (Insert Here, Rollback, Suppress, Edit Definition, Parent/Child)
- [ ] Dashboard feature creation
- [ ] Skeleton External Reference Manager
- [ ] Surface colors by type in viewport
- [ ] Table-driven parameters
- [ ] Trail log with replay capability
- [ ] Project management (recent, associations)
- [ ] MCP Server with resources/tools/prompts

---

## 9. Questions for Approval

Before implementation, please confirm:

1. **Vertical Toolbar Position**: Left side by default, dockable to right? (Yes/No)
2. **History Overlay**: Right side over viewport with blur-on-hover? (Yes/No)
3. **Menu Bar**: Completely remove top menu bar? File/Help move to toolbar hamburger? (Yes/No)
4. **rcad.toml**: Load on startup, watch for changes, auto-reload? (Yes/No)
5. **Surface Colors**: Apply to bodies in viewport based on `surface_type` field? (Yes/No)
6. **Trail Log**: JSONL in project directory, rotate at 10MB, keep 5 files? (Matches rcad.toml)
7. **First Run**: Create project dialog if no recent projects? (Yes/No)
8. **MCP Server**: Separate binary, stdio transport default, TCP optional? (Yes/No)

---

## 10. Next Steps

Upon approval:
1. Create detailed task breakdown for each phase
2. Set up GitHub issues with `good first issue` labels
3. Begin Phase 1: Vertical Toolbar + Adaptive History Overlay
4. Parallel: rcad.toml integration in rcad-app
5. Weekly sync on progress

---

*Document Version: 1.0*
*Date: 2026-10-12*
*Author: Architect Mode Analysis*