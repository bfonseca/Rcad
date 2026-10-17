//! Kernel-agnostic geometry types and the [`GeometryBackend`] trait.
//!
//! Application code talks only to this crate. Concrete kernels live in
//! `rcad-occt` (OpenCascade via cadrum) and optionally `rcad-truck`.

use std::collections::BTreeMap;

use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod naming;
pub mod profile;
pub mod query;
pub mod topology;

pub use naming::{
    EdgeFacts, FaceFacts, NamedEdge, NamedFace, ReferenceMemory, plane_basis, remember,
};
pub use profile::{Curve2D, Profile2D};
pub use topology::{FaceInfo, FaceSource, ShapeTopology};

/// Opaque handle to a BREP shape owned by a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShapeId(pub u64);

/// Face id for triangles that belong to no solid face (construction
/// geometry, physics proxies). Never a real kernel face id, so a face tagged
/// in the document can never match one by accident.
pub const NO_FACE_ID: u64 = u64::MAX;

/// Triangle mesh for viewport display and physics proxies, plus the topology
/// names that let a document refer to its faces and edges.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriangleMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Kernel face id for every triangle, one entry per triangle
    /// (indices.len() / 3). Empty when the backend does not report face ids,
    /// in which case no triangle can be colour-matched. Process-local: use
    /// [`TriangleMesh::face_name`] for anything that is stored.
    #[serde(default)]
    pub face_ids: Vec<u64>,
    /// Stable, geometry-derived name for every distinct kernel face id in this
    /// mesh. This is what a document stores, and it is the same in every
    /// process for the same geometry.
    #[serde(default)]
    pub face_names: BTreeMap<u64, NamedFace>,
    /// Topology edges of the shape, with their stable names and polylines.
    #[serde(default)]
    pub edges: Vec<NamedEdge>,
}

impl TriangleMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Kernel face id of triangle `triangle`, when the backend reported one.
    pub fn face_id(&self, triangle: usize) -> Option<u64> {
        self.face_ids.get(triangle).copied()
    }

    /// Stable name of the face triangle `triangle` belongs to.
    ///
    /// `None` when the backend did not attribute the triangle (construction
    /// geometry carries [`NO_FACE_ID`]) or did not report face ids at all.
    pub fn face_name(&self, triangle: usize) -> Option<&str> {
        self.face(triangle).map(|face| face.name.as_str())
    }

    /// Human label of the face triangle `triangle` belongs to.
    pub fn face_label(&self, triangle: usize) -> Option<&str> {
        self.face(triangle).map(|face| face.label.as_str())
    }

    fn face(&self, triangle: usize) -> Option<&NamedFace> {
        let face_id = self.face_id(triangle)?;
        if face_id == NO_FACE_ID {
            return None;
        }
        self.face_names.get(&face_id)
    }

    /// Whether the mesh contains a face with this stable name.
    pub fn has_face_name(&self, name: &str) -> bool {
        self.face_names
            .values()
            .any(|face| face.name == name)
    }

    /// The edge with this stable name, if the shape has it.
    pub fn edge_by_name(&self, name: &str) -> Option<&NamedEdge> {
        self.edges.iter().find(|edge| edge.matches(name))
    }

    /// Make `face_ids` as long as the triangle list, tagging any triangle the
    /// backend did not attribute with [`NO_FACE_ID`]. Keeping the two lists the
    /// same length means the viewport can index one by the other.
    pub fn normalize_face_ids(&mut self) {
        if self.face_ids.len() < self.triangle_count() {
            self.face_ids.resize(self.triangle_count(), NO_FACE_ID);
        }
    }

    /// Append another mesh, offsetting its indices, and tag every appended
    /// triangle with `face_id`.
    ///
    /// Construction geometry is appended to the solid display mesh, and its
    /// face ids come from different shapes: tagging it with [`NO_FACE_ID`]
    /// keeps it out of the face-colour lookup. Its names and edges are
    /// deliberately not merged either, so a construction plane can never be
    /// mistaken for a face of the model.
    pub fn append(&mut self, other: &TriangleMesh, face_id: u64) {
        self.normalize_face_ids();
        let offset = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.indices
            .extend(other.indices.iter().map(|index| index + offset));
        self.face_ids
            .extend(std::iter::repeat_n(face_id, other.triangle_count()));
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

    /// Sweep a profile that lies on the plane given by an origin, an in-plane x
    /// and a normal, along that normal. A backend that cannot place a profile
    /// sweeps it along Z from the XY plane instead, which is right for a sketch
    /// that was drawn there.
    fn extrude_profile_placed(
        &mut self,
        profile: &Profile2D,
        height: f64,
        origin: DVec3,
        x_direction: DVec3,
        normal: DVec3,
    ) -> GeomResult<ShapeId> {
        let _ = (origin, x_direction, normal);
        self.extrude_profile(profile, height)
    }
    /// Revolve a profile that lies on a plane, about the in-plane y axis through
    /// the plane origin. The default revolves about the world Z axis through the
    /// origin, which is only meaningful for a profile drawn on the XY plane.
    fn revolve_profile_placed(
        &mut self,
        profile: &Profile2D,
        angle_rad: f64,
        origin: DVec3,
        x_direction: DVec3,
        normal: DVec3,
    ) -> GeomResult<ShapeId> {
        let _ = (origin, x_direction, normal);
        self.revolve_profile(profile, DVec3::ZERO, DVec3::Z, angle_rad)
    }

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

    /// Round only the edges with these signatures. An empty list means every
    /// edge, which is what a backend that cannot pick edges still supports.
    ///
    /// Signatures rather than kernel ids: an operation works on a copy of the
    /// shape, whose ids are fresh.
    fn fillet_edges(&mut self, shape: ShapeId, radius: f64, edges: &[String]) -> GeomResult<ShapeId> {
        if edges.is_empty() {
            self.fillet_all(shape, radius)
        } else {
            Err(GeomError::Backend(
                "this backend cannot round specific edges".into(),
            ))
        }
    }
    fn chamfer_all(&mut self, shape: ShapeId, distance: f64) -> GeomResult<ShapeId>;

    /// Break only the edges with these signatures. An empty list means every
    /// edge.
    fn chamfer_edges(
        &mut self,
        shape: ShapeId,
        distance: f64,
        edges: &[String],
    ) -> GeomResult<ShapeId> {
        if edges.is_empty() {
            self.chamfer_all(shape, distance)
        } else {
            Err(GeomError::Backend(
                "this backend cannot break specific edges".into(),
            ))
        }
    }

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

    /// Set the display tessellation quality (foundation A5).
    ///
    /// `linear` is a fraction of the feature size when `relative`, otherwise a
    /// distance in millimetres; `angular_rad` is the maximum angle between
    /// adjacent facets. Display tessellation must not change topology names:
    /// backends derive names from a fixed canonical tessellation. Backends that
    /// cannot tune display quality ignore this call.
    fn set_display_tessellation(&mut self, _linear: f64, _angular_rad: f64, _relative: bool) {}

    /// Faces, edges and face history of `shape`, so the rebuild can give its
    /// topology names that survive a rebuild.
    ///
    /// Backends that cannot report topology return an empty one; every name
    /// then falls back to its geometric signature.
    fn topology(&mut self, _shape: ShapeId) -> GeomResult<ShapeTopology> {
        Ok(ShapeTopology::default())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-triangle mesh, optionally carrying a face id.
    fn tri_mesh(offset: f32, face: Option<u64>) -> TriangleMesh {
        TriangleMesh {
            positions: vec![
                [offset, 0.0, 0.0],
                [offset + 1.0, 0.0, 0.0],
                [offset, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            face_ids: face.into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_mesh_without_face_ids_reports_none() {
        let mesh = TriangleMesh::default();
        assert!(mesh.face_id(0).is_none());
    }

    /// The display mesh is the solid plus construction geometry: the appended
    /// triangles are offset and sentinel-tagged so they cannot claim a solid
    /// face colour.
    #[test]
    fn appending_tags_the_new_triangles_and_offsets_their_indices() {
        let mut solid = tri_mesh(0.0, Some(7));
        let construction = tri_mesh(10.0, Some(3));

        solid.append(&construction, NO_FACE_ID);

        assert_eq!(solid.triangle_count(), 2);
        assert_eq!(solid.face_ids, vec![7, NO_FACE_ID]);
        assert_eq!(solid.indices, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(solid.face_id(1), Some(NO_FACE_ID));
    }

    #[test]
    fn normalizing_keeps_face_ids_as_long_as_the_triangle_list() {
        let mut mesh = tri_mesh(0.0, None);
        assert!(mesh.face_ids.is_empty());
        mesh.normalize_face_ids();
        assert_eq!(mesh.face_ids, vec![NO_FACE_ID]);
    }
}
