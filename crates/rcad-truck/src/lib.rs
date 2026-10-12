//! Optional pure-Rust geometry experiments via the Truck kernel.
//!
//! The default build is a stub so the workspace stays light. Enable the `truck`
//! feature and implement [`rcad_geom::GeometryBackend`] when ready.

use rcad_geom::{
    BooleanOp, GeomError, GeomResult, GeometryBackend, Profile2D, ShapeId, TriangleMesh,
};
use glam::DVec3;
use std::path::Path;

/// Placeholder backend — returns errors until the `truck` feature is implemented.
pub struct TruckBackend;

impl Default for TruckBackend {
    fn default() -> Self {
        Self
    }
}

impl TruckBackend {
    pub fn new() -> Self {
        Self
    }
}

fn unsupported() -> GeomError {
    GeomError::Backend(
        "rcad-truck backend is a stub; use rcad-occt or enable the truck feature".into(),
    )
}

impl GeometryBackend for TruckBackend {
    fn box_solid(&mut self, _size: DVec3) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn cylinder(&mut self, _radius: f64, _height: f64) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn sphere(&mut self, _radius: f64) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn extrude_profile(&mut self, _profile: &Profile2D, _height: f64) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn revolve_profile(
        &mut self,
        _profile: &Profile2D,
        _axis_origin: DVec3,
        _axis_dir: DVec3,
        _angle_rad: f64,
    ) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn boolean(&mut self, _a: ShapeId, _b: ShapeId, _op: BooleanOp) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn translate(&mut self, _shape: ShapeId, _delta: DVec3) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn fillet_all(&mut self, _shape: ShapeId, _radius: f64) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn chamfer_all(&mut self, _shape: ShapeId, _distance: f64) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn sew(&mut self, _shapes: &[ShapeId], _tolerance: f64) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn tessellate(&mut self, _shape: ShapeId) -> GeomResult<TriangleMesh> {
        Err(unsupported())
    }
    fn tessellate_many(&mut self, _shapes: &[ShapeId]) -> GeomResult<TriangleMesh> {
        Err(unsupported())
    }
    fn write_step(&mut self, _shapes: &[ShapeId], _path: &Path) -> GeomResult<()> {
        Err(unsupported())
    }
    fn write_stl(&mut self, _shapes: &[ShapeId], _path: &Path) -> GeomResult<()> {
        Err(unsupported())
    }
    fn read_step(&mut self, _path: &Path) -> GeomResult<Vec<ShapeId>> {
        Err(unsupported())
    }
    fn loft_profiles(&mut self, _profiles: &[Profile2D], _solid: bool) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn boundary_surface(&mut self, _u_curves: &[Profile2D], _v_curves: &[Profile2D]) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn thicken(&mut self, _shape: ShapeId, _distance: f64, _both_sides: bool) -> GeomResult<ShapeId> {
        Err(unsupported())
    }
    fn drop_shape(&mut self, _shape: ShapeId) {}
}
