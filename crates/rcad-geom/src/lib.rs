//! Kernel-agnostic geometry types and the [`GeometryBackend`] trait.
//!
//! Application code talks only to this crate. Concrete kernels live in
//! `rcad-occt` (OpenCascade via cadrum) and optionally `rcad-truck`.

use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Opaque handle to a BREP shape owned by a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShapeId(pub u64);

/// Triangle mesh for viewport display and physics proxies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriangleMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl TriangleMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        if self.positions.is_empty() {
            return None;
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for p in &self.positions {
            let v = Vec3::from(*p);
            min = min.min(v);
            max = max.max(v);
        }
        Some((min, max))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile2D {
    /// Closed outer loop in the XY plane (Z = 0), CCW preferred.
    pub outer: Vec<[f64; 2]>,
    /// Optional hole loops.
    pub holes: Vec<Vec<[f64; 2]>>,
}

#[derive(Debug, Error)]
pub enum GeomError {
    #[error("geometry backend error: {0}")]
    Backend(String),
    #[error("unknown shape id {0:?}")]
    UnknownShape(ShapeId),
    #[error("invalid profile: {0}")]
    InvalidProfile(String),
    #[error("io error: {0}")]
    Io(String),
}

pub type GeomResult<T> = Result<T, GeomError>;

/// Abstraction over a BREP CAD kernel.
pub trait GeometryBackend: Send {
    fn box_solid(&mut self, size: DVec3) -> GeomResult<ShapeId>;
    fn cylinder(&mut self, radius: f64, height: f64) -> GeomResult<ShapeId>;
    fn sphere(&mut self, radius: f64) -> GeomResult<ShapeId>;

    fn extrude_profile(&mut self, profile: &Profile2D, height: f64) -> GeomResult<ShapeId>;
    fn revolve_profile(
        &mut self,
        profile: &Profile2D,
        axis_origin: DVec3,
        axis_dir: DVec3,
        angle_rad: f64,
    ) -> GeomResult<ShapeId>;

    fn boolean(&mut self, a: ShapeId, b: ShapeId, op: BooleanOp) -> GeomResult<ShapeId>;
    fn translate(&mut self, shape: ShapeId, delta: DVec3) -> GeomResult<ShapeId>;
    fn fillet_all(&mut self, shape: ShapeId, radius: f64) -> GeomResult<ShapeId>;
    fn chamfer_all(&mut self, shape: ShapeId, distance: f64) -> GeomResult<ShapeId>;

    /// Sew open shells / faces into a solid (surface → solid path).
    fn sew(&mut self, shapes: &[ShapeId], tolerance: f64) -> GeomResult<ShapeId>;

    /// Loft through multiple profiles (solid or surface).
    fn loft_profiles(&mut self, profiles: &[Profile2D], solid: bool) -> GeomResult<ShapeId>;

    /// Create boundary surface from U/V curves.
    fn boundary_surface(&mut self, u_curves: &[Profile2D], v_curves: &[Profile2D]) -> GeomResult<ShapeId>;

    /// Thicken a surface to solid.
    fn thicken(&mut self, surface: ShapeId, distance: f64, both_sides: bool) -> GeomResult<ShapeId>;

    fn tessellate(&mut self, shape: ShapeId) -> GeomResult<TriangleMesh>;
    fn tessellate_many(&mut self, shapes: &[ShapeId]) -> GeomResult<TriangleMesh>;

    fn write_step(&mut self, shapes: &[ShapeId], path: &std::path::Path) -> GeomResult<()>;
    fn write_stl(&mut self, shapes: &[ShapeId], path: &std::path::Path) -> GeomResult<()>;
    fn read_step(&mut self, path: &std::path::Path) -> GeomResult<Vec<ShapeId>>;

    fn drop_shape(&mut self, shape: ShapeId);

    // Datum geometry creation
    fn plane(&mut self, origin: DVec3, normal: DVec3, size: f64) -> GeomResult<ShapeId>;
    fn axis(&mut self, origin: DVec3, direction: DVec3, length: f64) -> GeomResult<ShapeId>;
    fn point(&mut self, position: DVec3, size: f64) -> GeomResult<ShapeId>;
    fn coordinate_system(&mut self, origin: DVec3, x_dir: DVec3, y_dir: DVec3, z_dir: DVec3, size: f64) -> GeomResult<ShapeId>;
}
