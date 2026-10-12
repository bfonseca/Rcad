//! Project document model: parts, assemblies, skeletons, feature trees.

use std::path::Path;

use glam::DVec3;
use rcad_geom::{BooleanOp, Profile2D, ShapeId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

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
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: 1,
            name: "Untitled".into(),
            parts: vec![Part::new_solid("Part1")],
            assemblies: Vec::new(),
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
    /// Skeleton reference published entities consumed by child parts.
    pub published: Vec<PublishedRef>,
    /// Optional skeleton this part is driven by.
    pub skeleton_of: Option<EntityId>,
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
            published: Vec::new(),
            skeleton_of: None,
        }
    }

    pub fn new_skeleton(name: impl Into<String>) -> Self {
        let mut p = Self::new_solid(name);
        p.kind = PartKind::Skeleton;
        p
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedRef {
    pub id: EntityId,
    pub name: String,
    pub kind: ConstructionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstructionKind {
    Plane,
    Axis,
    Point,
    Curve,
    Solid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: EntityId,
    pub name: String,
    pub suppressed: bool,
    pub kind: FeatureKind,
}

impl Feature {
    pub fn new(name: impl Into<String>, kind: FeatureKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            suppressed: false,
            kind,
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
    },
    Chamfer {
        distance: f64,
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
    pub shapes: Vec<(EntityId, ShapeId)>,
    pub display_mesh: rcad_geom::TriangleMesh,
    pub errors: Vec<String>,
}

pub fn dvec3_from(a: [f64; 3]) -> DVec3 {
    DVec3::new(a[0], a[1], a[2])
}
