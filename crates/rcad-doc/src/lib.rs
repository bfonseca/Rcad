//! Project document model: parts, assemblies, skeletons, feature trees.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use chrono;
use glam::DVec3;
use rcad_geom::{BooleanOp, Profile2D, ShapeId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub mod manifest;

pub use manifest::{MANIFEST_FILE, ProjectManifest};

pub type EntityId = Uuid;

#[derive(Debug, Error)]
pub enum DocError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialize: {0}")]
    Serialize(String),
    #[error("unknown entity {0}")]
    Unknown(EntityId),
}

pub type DocResult<T> = Result<T, DocError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub parts: Vec<Part>,
    pub assemblies: Vec<Assembly>,
    /// Camera state, so reopening a document restores the last view.
    /// Absent in files written before this existed.
    #[serde(default)]
    pub view: Option<SavedView>,
}

/// Camera state persisted with a document.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SavedView {
    /// Camera orientation as a quaternion (x, y, z, w).
    pub orientation: [f32; 4],
    pub target: [f32; 3],
    pub distance: f32,
    pub orthographic: bool,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: 1,
            name: "Untitled".into(),
            parts: vec![Part::new_solid("Part1")],
            assemblies: Vec::new(),
            view: None,
        }
    }
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn save(&self, path: &Path) -> DocResult<()> {
        let bytes = rmp_serde::to_vec_named(self).map_err(|e| DocError::Serialize(e.to_string()))?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn load(path: &Path) -> DocResult<Self> {
        let bytes = std::fs::read(path)?;
        rmp_serde::from_slice(&bytes).map_err(|e| DocError::Serialize(e.to_string()))
    }

    pub fn part_mut(&mut self, id: EntityId) -> Option<&mut Part> {
        self.parts.iter_mut().find(|p| p.id == id)
    }

    pub fn part(&self, id: EntityId) -> Option<&Part> {
        self.parts.iter().find(|p| p.id == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartKind {
    Solid,
    Skeleton,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body {
    pub id: EntityId,
    pub name: String,
    pub feature_id: EntityId, // Feature that produced this body
    pub is_solid: bool,       // true = solid, false = surface
    pub color: Option<[u8; 4]>, // RGBA for display
    pub visible: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RegenerationMode {
    #[default]
    Automatic,  // Rebuild on any change (default)
    Manual,     // User must trigger rebuild
    Suppressed, // Part is suppressed in assembly
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub id: EntityId,
    pub name: String,
    pub kind: PartKind,
    pub features: Vec<Feature>,
    pub suppressed: Vec<EntityId>,
    pub underlay_ids: Vec<EntityId>,
    /// Multi-body support: each feature produces a body
    pub bodies: Vec<Body>,
    /// Active body for new features (Insert Here mode)
    pub active_body_index: Option<usize>,
    /// Stable face name -> logical colour id from the project vocabulary.
    ///
    /// The key is the geometry-derived name from [`rcad_geom::naming`], not a
    /// kernel handle: it is the same in every process and after a save/reload,
    /// so the tag keeps pointing at the same face. The RGB value is never
    /// stored here either: it lives in the project manifest, so editing the
    /// project restyles every component that uses an id. Construction geometry
    /// is sentinel-tagged and can never match a face name.
    ///
    /// If a face is edited until its geometry no longer matches, the tag finds
    /// no face and is reported as unmatched instead of being drawn elsewhere;
    /// see [`Part::unmatched_face_colours`].
    #[serde(default)]
    pub face_colours: BTreeMap<String, String>,
    /// Skeleton reference published entities consumed by child parts.
    pub published: Vec<PublishedRef>,
    /// Optional skeleton this part is driven by.
    pub skeleton_of: Option<EntityId>,
    // NEW: Feature tree enhancements
    #[serde(default)]
    pub insert_here_index: Option<usize>,       // Insert Here position in feature list
    #[serde(default)]
    pub rollback_index: Option<usize>,          // Rollback position (rebuild stops here)
    #[serde(default)]
    pub layers: Vec<Layer>,                     // Layer management
    #[serde(default)]
    pub parameters: HashMap<String, Parameter>, // Part-level parameters (global)
    #[serde(default)]
    pub relations: Vec<Relation>,               // Equations: param = expression
    #[serde(default)]
    pub family_table: Option<FamilyTable>,      // For configurations
    #[serde(default)]
    pub regeneration_mode: RegenerationMode,    // Automatic, Manual, Suppressed
}

impl Part {
    pub fn new_solid(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind: PartKind::Solid,
            features: Vec::new(),
            suppressed: Vec::new(),
            underlay_ids: Vec::new(),
            bodies: Vec::new(),
            active_body_index: None,
            face_colours: BTreeMap::new(),
            published: Vec::new(),
            skeleton_of: None,
            // NEW fields with defaults
            insert_here_index: None,
            rollback_index: None,
            layers: Vec::new(),
            parameters: HashMap::new(),
            relations: Vec::new(),
            family_table: None,
            regeneration_mode: RegenerationMode::default(),
        }
    }

    pub fn new_skeleton(name: impl Into<String>) -> Self {
        let mut p = Self::new_solid(name);
        p.kind = PartKind::Skeleton;
        p
    }

    /// Tag a face with a logical colour id from the project vocabulary.
    ///
    /// `face_name` is the stable name reported by the kernel for that face.
    pub fn set_face_colour(&mut self, face_name: impl Into<String>, colour_id: impl Into<String>) {
        self.face_colours.insert(face_name.into(), colour_id.into());
    }

    /// Remove the colour of a face. Returns whether it had one.
    pub fn clear_face_colour(&mut self, face_name: &str) -> bool {
        self.face_colours.remove(face_name).is_some()
    }

    /// The logical colour id of a face, if it is tagged.
    pub fn face_colour_id(&self, face_name: &str) -> Option<&str> {
        self.face_colours.get(face_name).map(String::as_str)
    }

    /// The RGB value a face resolves to, given the project manifest. `None`
    /// when the face is untagged or the project does not define its id, in
    /// which case the viewport keeps its default shading.
    pub fn face_colour(&self, face_name: &str, manifest: &ProjectManifest) -> Option<[u8; 3]> {
        manifest.colour(self.face_colour_id(face_name)?)
    }

    /// Colour tags that no face of the rebuilt model matches any more, because
    /// that face was edited or deleted. The interface reports them instead of
    /// drawing the colour on whatever face happens to be there now.
    pub fn unmatched_face_colours(&self, available: &[String]) -> Vec<&str> {
        self.face_colours
            .keys()
            .filter(|name| !available.iter().any(|candidate| candidate == *name))
            .map(String::as_str)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalRefStatus {
    /// Reference is up to date with skeleton
    UpToDate,
    /// Skeleton has changed, reference needs update
    OutOfDate,
    /// Referenced geometry no longer exists in skeleton
    Broken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedRef {
    pub id: EntityId,
    pub name: String,
    pub kind: ConstructionKind,
    // NEW: Creo-style publish metadata
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub feature_id: EntityId,           // Source construction feature
    #[serde(default)]
    pub body_id: Option<EntityId>,      // Source body (for solid/surface publish)
    #[serde(default)]
    pub selection_query: String,        // Stable selection query for regeneration
    #[serde(default)]
    pub publish_type: PublishType,      // Geometry, Shrinkwrap, etc.
    #[serde(default)]
    pub include_children: bool,         // For assemblies: include sub-components
    #[serde(default)]
    pub status: ExternalRefStatus,      // External reference status
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublishType {
    Geometry,       // Standard publish (construction + solid geometry)
    Shrinkwrap,     // Simplified envelope (for large assemblies)
    CopyGeometry,   // Published specifically for Copy Geometry
    Interface,      // For mechanism connections
}

impl Default for ExternalRefStatus {
    fn default() -> Self {
        ExternalRefStatus::UpToDate
    }
}

impl Default for PublishType {
    fn default() -> Self {
        PublishType::Geometry
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstructionKind {
    Plane,
    Axis,
    Point,
    Curve,
    Solid,
    CoordSystem,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: EntityId,
    pub name: String,
    pub suppressed: bool,
    #[serde(default)]
    pub visible: bool,                       // Visibility in viewport
    pub kind: FeatureKind,
    // NEW: Parent/Child tracking for design intent
    #[serde(default)]
    pub parent_features: Vec<EntityId>,      // Features this feature references
    #[serde(default)]
    pub child_features: Vec<EntityId>,       // Features that reference this (computed)
    #[serde(default)]
    pub references: FeatureReferences,       // Detailed reference info for Edit References
    #[serde(default)]
    pub parameters: FeatureParameters,       // Structured parameters for Edit Definition
    #[serde(default)]
    pub insert_here_marker: bool,            // True if this is Insert Here position
    #[serde(default)]
    pub rollback_marker: bool,               // True if this is Rollback position
    #[serde(default)]
    pub created_timestamp: String,           // RFC3339 for trail log
    #[serde(default)]
    pub modified_timestamp: String,          // RFC3339 for trail log
    /// Logical colour id, e.g. "fit zone". The RGB value for that id lives in
    /// the project manifest, so a component stores a name and never a colour:
    /// editing the project updates every component that uses it.
    #[serde(default)]
    pub colour_id: Option<String>,
}

impl Feature {
    pub fn new(name: impl Into<String>, kind: FeatureKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            suppressed: false,
            kind,
            // NEW fields with defaults
            parent_features: Vec::new(),
            child_features: Vec::new(),
            references: FeatureReferences::default(),
            parameters: FeatureParameters::default(),
            insert_here_marker: false,
            rollback_marker: false,
            visible: true,  // Visible by default
            created_timestamp: chrono::Utc::now().to_rfc3339(),
            modified_timestamp: chrono::Utc::now().to_rfc3339(),
            colour_id: None,
        }
    }

    /// Tag this feature with a logical colour id from the project vocabulary.
    pub fn with_colour(mut self, colour_id: impl Into<String>) -> Self {
        self.colour_id = Some(colour_id.into());
        self
    }

    /// The RGB value for this feature, given the project manifest.
    ///
    /// Returns `None` when the feature has no colour id or the project does not
    /// define one for it, in which case the viewport uses its default shading.
    pub fn colour(&self, manifest: &ProjectManifest) -> Option<[u8; 3]> {
        manifest.colour(self.colour_id.as_deref()?)
    }
}

/// Where a sketch profile lies in space: the sketch origin, its normal and the
/// x direction inside the plane. The profile itself is 2D, so without this it
/// would be taken to be on the XY plane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SketchPlacement {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub x_direction: [f64; 3],
}

/// Which edges a fillet or a chamfer applies to.
///
/// Creo calls this an edge set: a list of picked edges, or a rule that is
/// re-evaluated on every regeneration, so the feature follows the model instead
/// of a snapshot of it. The rule modes are the Creo ones: every edge, the outer
/// edges (material angle below 180 degrees), the inner edges (above 180), and the
/// edges parallel or perpendicular to a reference axis - which is why the
/// vertical and horizontal modes need that reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EdgeSet {
    /// Edges the user picked, by stable reference.
    References(Vec<String>),
    /// Every edge of the body.
    All,
    /// Convex edges: the angle through the material is below 180 degrees.
    Outer,
    /// Concave edges: the angle through the material is above 180 degrees.
    Inner,
    /// Edges parallel to this datum axis.
    Vertical { axis: EntityId },
    /// Edges perpendicular to this datum axis.
    Horizontal { axis: EntityId },
}

impl Default for EdgeSet {
    fn default() -> Self {
        EdgeSet::All
    }
}

impl EdgeSet {
    /// True when the set names specific edges rather than a rule.
    pub fn is_references(&self) -> bool {
        matches!(self, EdgeSet::References(_))
    }

    /// The reference axis, for the modes that need one.
    pub fn axis(&self) -> Option<EntityId> {
        match self {
            EdgeSet::Vertical { axis } | EdgeSet::Horizontal { axis } => Some(*axis),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureKind {
    Box {
        size: [f64; 3],
    },
    Cylinder {
        radius: f64,
        height: f64,
    },
    Extrude {
        profile: Profile2D,
        height: f64,
        /// Where the profile lies. Without it the profile is on the XY plane and
        /// is swept along +Z, which is what every extrude did before placements.
        #[serde(default)]
        placement: Option<SketchPlacement>,
    },
    Revolve {
        profile: Profile2D,
        angle_deg: f64,
    },
    Boolean {
        tool_feature: EntityId,
        op: BooleanOp,
    },
    Translate {
        delta: [f64; 3],
    },
    Fillet {
        radius: f64,
        /// Which edges to round. Empty of references and `All` mean every edge,
        /// which is what the feature did before edge sets existed.
        #[serde(default)]
        edges: EdgeSet,
    },
    Chamfer {
        distance: f64,
        #[serde(default)]
        edges: EdgeSet,
    },
    Construction {
        kind: ConstructionKind,
        origin: [f64; 3],
        direction: [f64; 3],
    },
    /// Surface extruded then intended for sew/thicken workflows.
    ExtrudeSurface {
        profile: Profile2D,
        height: f64,
    },
    Sew {
        tolerance: f64,
    },
    /// Loft between multiple profiles
    Loft {
        profiles: Vec<Profile2D>,
        solid: bool,
    },
    /// Boundary surface from U/V curves
    Boundary {
        u_curves: Vec<Profile2D>,
        v_curves: Vec<Profile2D>,
    },
    /// Thicken a surface to solid
    Thicken {
        surface_feature: EntityId,
        distance: f64,
        both_sides: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assembly {
    pub id: EntityId,
    pub name: String,
    pub instances: Vec<ComponentInstance>,
    pub mates: Vec<Mate>,
}

impl Assembly {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            instances: Vec::new(),
            mates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub id: EntityId,
    pub part_id: EntityId,
    pub name: String,
    pub translation: [f64; 3],
    pub rotation_deg: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mate {
    pub id: EntityId,
    pub kind: MateKind,
    pub a: EntityId,
    pub b: EntityId,
    pub distance: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MateKind {
    Coincident,
    Concentric,
    Distance,
}

/// Runtime rebuild output (not persisted).
#[derive(Debug, Clone, Default)]
pub struct RebuildResult {
    /// Shapes produced by the body features, in evaluation order. The last
    /// entry is the active body, which is what STEP/STL export should use.
    /// Construction geometry (planes, axes, points) is excluded.
    pub shapes: Vec<(EntityId, ShapeId)>,
    /// Tessellated mesh of the active body plus any visible construction
    /// geometry, ready for the viewport.
    pub display_mesh: rcad_geom::TriangleMesh,
    /// Human-readable errors, one per feature that failed to evaluate.
    pub errors: Vec<String>,
}

pub fn dvec3_from(a: [f64; 3]) -> DVec3 {
    DVec3::new(a[0], a[1], a[2])
}

#[cfg(test)]
mod project_io_tests {
    use super::*;

    #[test]
    fn project_round_trips_through_the_native_format() {
        let mut project = Project::new("RoundTrip");
        let mut part = Part::new_solid("Bracket");
        part.features.push(Feature::new(
            "Base",
            FeatureKind::Box {
                size: [10.0, 20.0, 30.0],
            },
        ));
        project.parts = vec![part];

        let path = std::env::temp_dir().join("rcad_project_roundtrip.rcad");
        project.save(&path).expect("save");
        let loaded = Project::load(&path).expect("load");

        assert_eq!(loaded.name, "RoundTrip");
        assert_eq!(loaded.parts.len(), 1);
        assert_eq!(loaded.parts[0].name, "Bracket");
        assert_eq!(loaded.parts[0].features.len(), 1);
        assert_eq!(loaded.parts[0].features[0].name, "Base");
    }
}

#[cfg(test)]
mod colour_id_tests {
    use super::*;

    fn a_box() -> Feature {
        Feature::new("Base", FeatureKind::Box { size: [10.0, 10.0, 10.0] })
    }

    #[test]
    fn a_feature_carries_the_logical_id_not_a_value() {
        let feature = a_box().with_colour("fit zone");
        assert_eq!(feature.colour_id.as_deref(), Some("fit zone"));
    }

    #[test]
    fn the_colour_id_survives_a_round_trip() {
        let feature = a_box().with_colour("metric thread");
        let bytes = rmp_serde::to_vec(&feature).expect("serialise");
        let back: Feature = rmp_serde::from_slice(&bytes).expect("deserialise");
        assert_eq!(back.colour_id.as_deref(), Some("metric thread"));
    }

    /// An untagged feature resolves to no colour, so the viewport keeps its
    /// default shading instead of guessing.
    #[test]
    fn an_untagged_feature_has_no_colour() {
        let mut manifest = ProjectManifest::default();
        manifest.surface_colors.insert("fit zone".to_string(), [0, 255, 0]);

        assert_eq!(a_box().colour(&manifest), None);
        assert_eq!(a_box().with_colour("fit zone").colour(&manifest), Some([0, 255, 0]));
        assert_eq!(a_box().with_colour("unknown").colour(&manifest), None);
    }
}

#[cfg(test)]
mod face_colour_tests {
    use super::*;

    fn manifest_with(entries: &[(&str, [u8; 3])]) -> ProjectManifest {
        let mut manifest = ProjectManifest::default();
        for (id, rgb) in entries {
            manifest.surface_colors.insert((*id).to_string(), *rgb);
        }
        manifest
    }

    /// A stable face name, in the same shape the kernel produces.
    const FACE: &str = "F:plane:a1200.0000:c20.0000,15.0000,8.0000:n0.000,0.000,1.000:e4:p140.0000";

    #[test]
    fn a_face_is_tagged_with_the_id_not_a_value() {
        let mut part = Part::new_solid("P");
        part.set_face_colour(FACE, "fit zone");

        assert_eq!(part.face_colour_id(FACE), Some("fit zone"));
        assert!(part.face_colour_id("F:another").is_none());
    }

    #[test]
    fn a_face_colour_resolves_through_the_project() {
        let mut part = Part::new_solid("P");
        part.set_face_colour(FACE, "fit zone");
        let manifest = manifest_with(&[("fit zone", [0, 255, 0])]);

        assert_eq!(part.face_colour(FACE, &manifest), Some([0, 255, 0]));
    }

    /// An untagged face, or one whose id the project does not define, resolves
    /// to no colour so the viewport keeps the default shading.
    #[test]
    fn an_untagged_or_unknown_face_has_no_colour() {
        let manifest = manifest_with(&[("fit zone", [0, 255, 0])]);
        let mut part = Part::new_solid("P");

        assert_eq!(part.face_colour("F:never-tagged", &manifest), None);
        part.set_face_colour("F:never-tagged", "not in this project");
        assert_eq!(part.face_colour("F:never-tagged", &manifest), None);
    }

    #[test]
    fn clearing_a_face_colour_is_reversible() {
        let mut part = Part::new_solid("P");
        part.set_face_colour(FACE, "thread");

        assert!(part.clear_face_colour(FACE));
        assert!(!part.clear_face_colour(FACE));
        assert!(part.face_colour_id(FACE).is_none());
    }

    /// The vocabulary is global, the RGB value is per project: the part stores
    /// the id, so the same component renders differently for client A and
    /// client B without being touched.
    #[test]
    fn the_project_owns_the_rgb_value() {
        let mut part = Part::new_solid("P");
        part.set_face_colour(FACE, "fit zone");
        let before = part.clone();

        let client_a = manifest_with(&[("fit zone", [0, 255, 0])]);
        let client_b = manifest_with(&[("fit zone", [255, 0, 0])]);

        assert_eq!(part.face_colour(FACE, &client_a), Some([0, 255, 0]));
        assert_eq!(part.face_colour(FACE, &client_b), Some([255, 0, 0]));
        assert_eq!(part.face_colours, before.face_colours);
    }

    #[test]
    fn face_colours_survive_a_round_trip() {
        let mut part = Part::new_solid("P");
        part.set_face_colour(FACE, "sealing zone");

        let bytes = rmp_serde::to_vec_named(&part).expect("serialise");
        let back: Part = rmp_serde::from_slice(&bytes).expect("deserialise");

        assert_eq!(back.face_colour_id(FACE), Some("sealing zone"));
    }

    /// A tag whose face is gone (edited or deleted) must be reported, not
    /// silently dropped and not drawn on whatever face took its place.
    #[test]
    fn a_tag_whose_face_is_gone_is_reported_as_unmatched() {
        let mut part = Part::new_solid("P");
        part.set_face_colour(FACE, "fit zone");
        part.set_face_colour("F:deleted-face", "thread");

        let available = vec![FACE.to_string(), "F:new-face".to_string()];
        assert_eq!(part.unmatched_face_colours(&available), vec!["F:deleted-face"]);

        let all = vec![FACE.to_string(), "F:deleted-face".to_string()];
        assert!(part.unmatched_face_colours(&all).is_empty());
    }
}