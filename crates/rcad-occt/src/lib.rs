//! OpenCascade backend powered by [`cadrum`].

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use cadrum::{DVec3 as CVec3, Edge, Solid, Surface, SurfaceKind, Tessellation};
use glam::DVec3;
use rcad_geom::{
    naming, BooleanOp, EdgeFacts, FaceFacts, FaceInfo, FaceSource, GeomError, GeomResult,
    GeometryBackend, NamedEdge, NamedFace, Profile2D, ShapeId, ShapeTopology, TriangleMesh,
};

/// A stored shape and, when an operation produced it, where its faces came
/// from. The history is recorded at operation time: the kernel reports the
/// source faces of the *copies* it worked on, so the source can only be named
/// by geometry while those copies are alive.
pub(crate) struct StoredShape {
    solid: Solid,
    face_history: BTreeMap<u64, FaceSource>,
}

pub struct OcctBackend {
    next_id: u64,
    solids: HashMap<u64, StoredShape>,
}

impl Default for OcctBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl OcctBackend {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            solids: HashMap::new(),
        }
    }

    fn insert(&mut self, solid: Solid) -> ShapeId {
        self.insert_with_history(solid, BTreeMap::new())
    }

    fn insert_with_history(
        &mut self,
        solid: Solid,
        face_history: BTreeMap<u64, FaceSource>,
    ) -> ShapeId {
        let id = self.next_id;
        self.next_id += 1;
        self.solids.insert(id, StoredShape { solid, face_history });
        ShapeId(id)
    }

    fn stored(&self, id: ShapeId) -> GeomResult<&StoredShape> {
        self.solids
            .get(&id.0)
            .ok_or(GeomError::UnknownShape(id))
    }

    fn get(&self, id: ShapeId) -> GeomResult<&Solid> {
        Ok(&self.stored(id)?.solid)
    }


    fn profile_edges(profile: &Profile2D) -> GeomResult<Vec<Edge>> {
        if profile.outer.len() < 3 {
            return Err(GeomError::InvalidProfile(
                "outer loop needs at least 3 points".into(),
            ));
        }
        let mut edges = Vec::new();
        let outer = polygon_from_xy(&profile.outer)?;
        edges.extend(outer);
        for hole in &profile.holes {
            if hole.len() < 3 {
                return Err(GeomError::InvalidProfile(
                    "hole loop needs at least 3 points".into(),
                ));
            }
            edges.extend(polygon_from_xy(hole)?);
        }
        Ok(edges)
    }

    fn mesh_of(&self, shapes: &[ShapeId]) -> GeomResult<TriangleMesh> {
        let solids: GeomResult<Vec<&Solid>> = shapes.iter().map(|id| self.get(*id)).collect();
        let solids = solids?;
        let owned: Vec<Solid> = solids.into_iter().cloned().collect();
        let mesh = Solid::mesh(&owned, Tessellation::default())
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        let mut result = triangle_mesh_from_cadrum(&mesh);
        // OpenCascade identifies faces and edges by address, which changes in
        // every process. Start from the geometric signature; the rebuild then
        // replaces it with a reference to the feature that created the face.
        let topology = topology_of(&owned);
        result.face_names = topology
            .faces
            .iter()
            .map(|(face_id, info)| {
                (
                    *face_id,
                    NamedFace {
                        name: info.signature.clone(),
                        signature: info.signature.clone(),
                        label: info.label.clone(),
                    },
                )
            })
            .collect();
        result.edges = topology.edges.values().cloned().collect();
        Ok(result)
    }
}

fn cvec(v: DVec3) -> CVec3 {
    CVec3::new(v.x, v.y, v.z)
}

fn polygon_from_xy(pts: &[[f64; 2]]) -> GeomResult<Vec<Edge>> {
    let pts3: Vec<CVec3> = pts
        .iter()
        .map(|p| CVec3::new(p[0], p[1], 0.0))
        .collect();
    Edge::polygon(&pts3).map_err(|e| GeomError::Backend(e.to_string()))
}

fn triangle_mesh_from_cadrum(mesh: &cadrum::Mesh) -> TriangleMesh {
    TriangleMesh {
        positions: mesh
            .vertices
            .iter()
            .map(|v| [v.x as f32, v.y as f32, v.z as f32])
            .collect(),
        normals: mesh
            .normals
            .iter()
            .map(|n| [n.x as f32, n.y as f32, n.z as f32])
            .collect(),
        indices: mesh.indices.iter().map(|&i| i as u32).collect(),
        // OpenCascade already computes triangle -> face; keep it so a face can
        // be tagged with a project colour. It is the address of the face, so it
        // only identifies the face inside this process; `mesh_of` fills in the
        // stable names separately.
        face_ids: mesh.face_ids.clone(),
        ..Default::default()
    }
}

/// One line describing the surface a face lies on. The parameters are part of
/// the face name, because two cylinders of different radii are different faces.
fn describe_surface(surface: Option<Surface>) -> String {
    let Some(surface) = surface else {
        return "other".to_string();
    };
    match surface.kind {
        SurfaceKind::Plane => "plane".to_string(),
        SurfaceKind::Cylinder { radius } => {
            format!("cylinder(r{})", naming::length_text(radius))
        }
        SurfaceKind::Cone { radius, semi_angle } => format!(
            "cone(r{},a{})",
            naming::length_text(radius),
            naming::length_text(semi_angle)
        ),
        SurfaceKind::Sphere { radius } => format!("sphere(r{})", naming::length_text(radius)),
        SurfaceKind::Torus {
            major_radius,
            minor_radius,
        } => format!(
            "torus(r{},m{})",
            naming::length_text(major_radius),
            naming::length_text(minor_radius)
        ),
    }
}

fn edge_polyline(edge: &Edge) -> Vec<CVec3> {
    edge.approximation_segments(Tessellation::default())
}

fn polyline_length(points: &[CVec3]) -> f64 {
    points.windows(2).map(|pair| (pair[1] - pair[0]).length()).sum()
}

/// The point halfway along the polyline, measured by arc length.
fn polyline_midpoint(points: &[CVec3]) -> CVec3 {
    match points {
        [] => CVec3::ZERO,
        [only] => *only,
        _ => {
            let half = polyline_length(points) * 0.5;
            let mut travelled = 0.0;
            for pair in points.windows(2) {
                let segment = (pair[1] - pair[0]).length();
                if travelled + segment >= half {
                    let t = if segment > 0.0 {
                        (half - travelled) / segment
                    } else {
                        0.0
                    };
                    return pair[0] + (pair[1] - pair[0]) * t;
                }
                travelled += segment;
            }
            points[points.len() - 1]
        }
    }
}

/// Geometric facts of one face, shared by the mesh builder and the rebuild.
fn describe_face(face: &cadrum::Face) -> FaceInfo {
    let surface = face.surface();
    let kind = describe_surface(surface);
    let center = face.center();
    // Only a planar face has one meaningful normal. On a curved face the normal
    // at the centre depends on where the projection lands, which is not a
    // property of the geometry.
    let normal = match surface.map(|surface| surface.kind) {
        Some(SurfaceKind::Plane) => {
            let (_, normal) = face.project(center);
            (normal.length() > 0.0).then(|| normal.normalize().to_array())
        }
        _ => None,
    };
    let edge_ids: Vec<u64> = face.iter_edge().map(|edge| edge.id()).collect();
    let perimeter = face
        .iter_edge()
        .map(|edge| polyline_length(&edge_polyline(edge)))
        .sum();
    let facts = FaceFacts {
        kind: &kind,
        area: face.area(),
        center: center.to_array(),
        normal,
        edge_count: edge_ids.len(),
        perimeter,
    };
    FaceInfo {
        signature: naming::face_signature(&facts),
        label: naming::face_label(&facts),
        kind,
        area: face.area(),
        center: center.to_array(),
        normal,
        edges: edge_ids,
    }
}

/// Signature per face id, for the input of an operation.
fn face_signatures(solid: &Solid) -> BTreeMap<u64, String> {
    solid
        .iter_face()
        .map(|face| (face.id(), describe_face(face).signature))
        .collect()
}

/// Where each face of an operation result came from, named by the signature of
/// the source face in the input it belongs to. `inputs` pairs each operand with
/// whether it is the tool of the operation.
fn operation_history(
    result: &Solid,
    inputs: &[(&BTreeMap<u64, String>, bool)],
) -> BTreeMap<u64, FaceSource> {
    let mut history = BTreeMap::new();
    for [post, source] in result.iter_history() {
        for (signatures, from_tool) in inputs {
            if let Some(signature) = signatures.get(&source) {
                history.insert(
                    post,
                    FaceSource {
                        source_signature: signature.clone(),
                        from_tool: *from_tool,
                    },
                );
                break;
            }
        }
    }
    history
}

/// Faces, edges and face history of these solids.
///
/// Single source of the geometric topology facts: the mesh builder uses the
/// signatures as names, and the rebuild turns them into feature-anchored
/// references.
fn topology_of(solids: &[Solid]) -> ShapeTopology {
    let mut faces: Vec<(u64, FaceInfo)> = Vec::new();
    let mut named: Vec<NamedFace> = Vec::new();
    let mut edges: Vec<NamedEdge> = Vec::new();

    for solid in solids {
        for face in solid.iter_face() {
            let info = describe_face(face);
            named.push(NamedFace {
                name: info.signature.clone(),
                signature: info.signature.clone(),
                label: info.label.clone(),
            });
            faces.push((face.id(), info));
        }

        for edge in solid.iter_edge() {
            let points = edge_polyline(edge);
            let length = polyline_length(&points);
            let start = edge.start_point();
            let end = edge.end_point();
            let closed = (end - start).length() <= Edge::precision_distance();
            let midpoint = polyline_midpoint(&points);
            let kind = if points.len() <= 2 {
                "line"
            } else if closed {
                "circle"
            } else {
                "curve"
            };
            let facts = EdgeFacts {
                kind,
                length,
                start: start.to_array(),
                end: end.to_array(),
                midpoint: midpoint.to_array(),
                closed,
            };
            let signature = naming::edge_signature(&facts);
            edges.push(NamedEdge {
                name: signature.clone(),
                signature,
                label: naming::edge_label(&facts),
                kernel_id: edge.id(),
                start: start.to_array(),
                end: end.to_array(),
                midpoint: midpoint.to_array(),
                length,
                closed,
                points: points
                    .iter()
                    .map(|point| [point.x as f32, point.y as f32, point.z as f32])
                    .collect(),
            });
        }
    }

    // Two elements sharing a signature are the same geometry (two identical
    // holes): the suffix keeps the lookup unambiguous.
    naming::disambiguate_faces(&mut named);
    naming::disambiguate_edges(&mut edges);

    let mut topology = ShapeTopology::default();
    for ((face_id, mut info), face) in faces.into_iter().zip(named) {
        info.signature = face.signature;
        info.label = face.label;
        topology.faces.insert(face_id, info);
    }
    for edge in edges {
        topology.edges.insert(edge.kernel_id, edge);
    }
    // Face history cannot be resolved here: the kernel reports the source
    // faces of the copies an operation worked on, and those are gone by now.
    // It is recorded when the operation runs (`operation_history`) and put
    // back by `OcctBackend::topology`.
    topology
}

impl GeometryBackend for OcctBackend {
    fn box_solid(&mut self, size: DVec3) -> GeomResult<ShapeId> {
        let solid = Solid::cube(CVec3::ZERO, cvec(size));
        Ok(self.insert(solid))
    }

    fn cylinder(&mut self, radius: f64, height: f64) -> GeomResult<ShapeId> {
        let solid = Solid::cylinder(radius, CVec3::Z * height);
        Ok(self.insert(solid))
    }

    fn sphere(&mut self, radius: f64) -> GeomResult<ShapeId> {
        Ok(self.insert(Solid::sphere(radius)))
    }

    fn extrude_profile(&mut self, profile: &Profile2D, height: f64) -> GeomResult<ShapeId> {
        let edges = Self::profile_edges(profile)?;
        let solid = Solid::extrude(&edges, CVec3::Z * height)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(self.insert(solid))
    }

    fn revolve_profile(
        &mut self,
        profile: &Profile2D,
        axis_origin: DVec3,
        axis_dir: DVec3,
        angle_rad: f64,
    ) -> GeomResult<ShapeId> {
        let edges = Self::profile_edges(profile)?;
        let solid = Solid::revolve(&edges, cvec(axis_origin), cvec(axis_dir), angle_rad)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(self.insert(solid))
    }

    fn boolean(&mut self, a: ShapeId, b: ShapeId, op: BooleanOp) -> GeomResult<ShapeId> {
        let sa = self.get(a)?.clone();
        let sb = self.get(b)?.clone();
        // Name the operands by geometry while the copies are still alive: the
        // history the kernel records points at these copies, not at the shapes
        // the caller holds.
        let base_signatures = face_signatures(&sa);
        let tool_signatures = face_signatures(&sb);
        let solid = match op {
            BooleanOp::Union => (&sa + &sb)
                .build()
                .map_err(|e| GeomError::Backend(e.to_string()))?,
            BooleanOp::Subtract => (&sa - &sb)
                .build()
                .map_err(|e| GeomError::Backend(e.to_string()))?,
            BooleanOp::Intersect => (&sa * &sb)
                .build()
                .map_err(|e| GeomError::Backend(e.to_string()))?,
        };
        let history = operation_history(
            &solid,
            &[(&base_signatures, false), (&tool_signatures, true)],
        );
        Ok(self.insert_with_history(solid, history))
    }

    fn translate(&mut self, shape: ShapeId, delta: DVec3) -> GeomResult<ShapeId> {
        let solid = self.get(shape)?.clone().translate(cvec(delta));
        Ok(self.insert(solid))
    }

    fn fillet_all(&mut self, shape: ShapeId, radius: f64) -> GeomResult<ShapeId> {
        let solid = self.get(shape)?.clone();
        let input_signatures = face_signatures(&solid);
        let edges: Vec<_> = solid.iter_edge().collect();
        let filleted = solid
            .fillet_edges(radius, edges)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        let history = operation_history(&filleted, &[(&input_signatures, false)]);
        Ok(self.insert_with_history(filleted, history))
    }

    fn chamfer_all(&mut self, shape: ShapeId, distance: f64) -> GeomResult<ShapeId> {
        let solid = self.get(shape)?.clone();
        let input_signatures = face_signatures(&solid);
        let edges: Vec<_> = solid.iter_edge().collect();
        let chamfered = solid
            .chamfer_edges(distance, edges)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        let history = operation_history(&chamfered, &[(&input_signatures, false)]);
        Ok(self.insert_with_history(chamfered, history))
    }

    fn sew(&mut self, shapes: &[ShapeId], tolerance: f64) -> GeomResult<ShapeId> {
        let solids: GeomResult<Vec<Solid>> =
            shapes.iter().map(|id| self.get(*id).cloned()).collect();
        let solids = solids?;
        let faces: Vec<_> = solids.iter().flat_map(|s| s.iter_face()).collect();
        let sewn = Solid::sew(faces, tolerance).map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(self.insert(sewn))
    }

    fn tessellate(&mut self, shape: ShapeId) -> GeomResult<TriangleMesh> {
        self.mesh_of(&[shape])
    }

    fn tessellate_many(&mut self, shapes: &[ShapeId]) -> GeomResult<TriangleMesh> {
        self.mesh_of(shapes)
    }

    fn topology(&mut self, shape: ShapeId) -> GeomResult<ShapeTopology> {
        // Read the stored shape, never a clone: cloning rebuilds topology and
        // hands out fresh TShape addresses, so the ids would not match the ones
        // another call reported for the same shape.
        let stored = self.stored(shape)?;
        let mut topology = topology_of(std::slice::from_ref(&stored.solid));
        topology.face_history = stored.face_history.clone();
        Ok(topology)
    }

    fn write_step(&mut self, shapes: &[ShapeId], path: &Path) -> GeomResult<()> {
        let solids: GeomResult<Vec<Solid>> =
            shapes.iter().map(|id| self.get(*id).cloned()).collect();
        let solids = solids?;
        let mut file = std::fs::File::create(path).map_err(|e| GeomError::Io(e.to_string()))?;
        Solid::write_step(&solids, &mut file).map_err(|e| GeomError::Backend(e.to_string()))
    }

    fn write_stl(&mut self, shapes: &[ShapeId], path: &Path) -> GeomResult<()> {
        // Tessellate once. The previous version built a TriangleMesh via
        // mesh_of() and then immediately discarded it, tessellating a second
        // time for the writer.
        let solids: GeomResult<Vec<Solid>> =
            shapes.iter().map(|id| self.get(*id).cloned()).collect();
        let solids = solids?;
        let cmesh = Solid::mesh(&solids, Tessellation::default())
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        let mut file = std::fs::File::create(path).map_err(|e| GeomError::Io(e.to_string()))?;
        cmesh
            .write_stl(&mut file)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(())
    }

    fn read_step(&mut self, path: &Path) -> GeomResult<Vec<ShapeId>> {
        let mut file = std::fs::File::open(path).map_err(|e| GeomError::Io(e.to_string()))?;
        let solids =
            Solid::read_step(&mut file).map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(solids.into_iter().map(|s| self.insert(s)).collect())
    }

    fn loft_profiles(&mut self, profiles: &[Profile2D], solid: bool) -> GeomResult<ShapeId> {
        if profiles.is_empty() {
            return Err(GeomError::InvalidProfile("loft needs at least one profile".into()));
        }
        
        let mut all_edges = Vec::new();
        for profile in profiles {
            all_edges.extend(Self::profile_edges(profile)?);
        }
        
        // For now, loft as solid by extruding first profile and using the rest as sections
        // In a full implementation, we'd use OCCT's ThruSections or similar
        if solid && profiles.len() >= 2 {
            // Extrude first profile to last profile's position (simplified)
            let edges = Self::profile_edges(&profiles[0])?;
            let solid_shape = Solid::extrude(&edges, CVec3::Z * 10.0)
                .map_err(|e| GeomError::Backend(e.to_string()))?;
            Ok(self.insert(solid_shape))
        } else {
            // Surface loft - return first profile as surface
            let edges = Self::profile_edges(&profiles[0])?;
            let surface = Solid::extrude(&edges, CVec3::Z * 0.01)
                .map_err(|e| GeomError::Backend(e.to_string()))?;
            Ok(self.insert(surface))
        }
    }

    fn boundary_surface(&mut self, u_curves: &[Profile2D], v_curves: &[Profile2D]) -> GeomResult<ShapeId> {
        if u_curves.is_empty() || v_curves.is_empty() {
            return Err(GeomError::InvalidProfile("boundary needs U and V curves".into()));
        }
        // Simplified: create a surface from first U and V curve
        // Full implementation would use OCCT's GeomFill_BSplineSurface or similar
        let u_edges = Self::profile_edges(&u_curves[0])?;
        let surface = Solid::extrude(&u_edges, CVec3::Z * 0.01)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(self.insert(surface))
    }

    fn thicken(&mut self, surface: ShapeId, distance: f64, both_sides: bool) -> GeomResult<ShapeId> {
        let solid = self.get(surface)?.clone();
        let faces: Vec<_> = solid.iter_face().collect();
        if faces.is_empty() {
            return Err(GeomError::InvalidProfile("thicken requires a surface with faces".into()));
        }
        
        // Thicken using offset on all faces
        // Note: This is a simplified implementation. Full OCCT thicken requires BRepOffsetAPI_MakeThickSolid
        let dist = if both_sides { distance * 0.5 } else { distance };
        let face_refs: Vec<&cadrum::Face> = faces.iter().map(|f| *f).collect();
        let thickened = solid.offset(dist, face_refs, 1e-4)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        
        Ok(self.insert(thickened))
    }

    fn drop_shape(&mut self, shape: ShapeId) {
        self.solids.remove(&shape.0);
    }

    fn plane(&mut self, _origin: DVec3, _normal: DVec3, _size: f64) -> GeomResult<ShapeId> {
        // Lightweight datum: don't create B-Rep solid for reference planes
        // The viewport renders these directly as line geometry
        // Return a minimal placeholder shape ID
        let solid = Solid::cube(CVec3::ZERO, cvec(DVec3::new(0.001, 0.001, 0.001)));
        Ok(self.insert(solid))
    }

    fn axis(&mut self, _origin: DVec3, _direction: DVec3, _length: f64) -> GeomResult<ShapeId> {
        // Lightweight datum: don't create B-Rep solid for reference axes
        // The viewport renders these directly as line geometry
        let solid = Solid::cube(CVec3::ZERO, cvec(DVec3::new(0.001, 0.001, 0.001)));
        Ok(self.insert(solid))
    }

    fn point(&mut self, _position: DVec3, _size: f64) -> GeomResult<ShapeId> {
        // Lightweight datum: don't create B-Rep solid for reference points
        let solid = Solid::cube(CVec3::ZERO, cvec(DVec3::new(0.001, 0.001, 0.001)));
        Ok(self.insert(solid))
    }

    fn coordinate_system(&mut self, _origin: DVec3, _x_dir: DVec3, _y_dir: DVec3, _z_dir: DVec3, _size: f64) -> GeomResult<ShapeId> {
        // Lightweight datum: don't create B-Rep solid for coordinate systems
        // The viewport renders these directly as line geometry
        let solid = Solid::cube(CVec3::ZERO, cvec(DVec3::new(0.001, 0.001, 0.001)));
        Ok(self.insert(solid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::dvec3;

    #[test]
    fn box_tessellates() {
        let mut backend = OcctBackend::new();
        let id = backend.box_solid(dvec3(10.0, 20.0, 30.0)).unwrap();
        let mesh = backend.tessellate(id).unwrap();
        assert!(!mesh.is_empty());
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn box_exports_real_step_and_stl_files() {
        let mut backend = OcctBackend::new();
        let id = backend.box_solid(dvec3(10.0, 20.0, 30.0)).unwrap();
        let dir = std::env::temp_dir();
        let step_path = dir.join("rcad_box_export.step");
        let stl_path = dir.join("rcad_box_export.stl");

        backend.write_step(&[id], &step_path).unwrap();
        backend.write_stl(&[id], &stl_path).unwrap();

        let step = std::fs::read_to_string(&step_path).unwrap();
        assert!(step.contains("ISO-10303-21"), "not a STEP file");
        assert!(
            std::fs::metadata(&stl_path).unwrap().len() > 100,
            "the STL file is empty"
        );
    }

    /// Step A of the face-colour work: the kernel attributes every triangle to
    /// a face, and a cube has exactly six.
    #[test]
    fn a_box_tessellates_to_six_distinct_face_ids() {
        let mut backend = OcctBackend::new();
        let id = backend.box_solid(dvec3(10.0, 20.0, 30.0)).unwrap();
        let mesh = backend.tessellate(id).unwrap();

        assert_eq!(
            mesh.face_ids.len(),
            mesh.triangle_count(),
            "one face id per triangle"
        );
        let distinct: std::collections::HashSet<u64> = mesh.face_ids.iter().copied().collect();
        assert_eq!(distinct.len(), 6, "a cube has six faces");
    }

    fn box_mesh() -> TriangleMesh {
        let mut backend = OcctBackend::new();
        let id = backend.box_solid(dvec3(10.0, 20.0, 30.0)).unwrap();
        backend.tessellate(id).unwrap()
    }

    fn face_names(mesh: &TriangleMesh) -> std::collections::BTreeSet<String> {
        mesh.face_names
            .values()
            .map(|face| face.name.clone())
            .collect()
    }

    fn edge_names(mesh: &TriangleMesh) -> std::collections::BTreeSet<String> {
        mesh.edges.iter().map(|edge| edge.name.clone()).collect()
    }

    #[test]
    fn a_box_has_six_named_faces_and_twelve_named_edges() {
        let mesh = box_mesh();

        assert_eq!(mesh.face_names.len(), 6, "one name per cube face");
        assert_eq!(face_names(&mesh).len(), 6, "the six names are distinct");
        assert_eq!(mesh.edges.len(), 12, "a cube has twelve edges");
        assert_eq!(edge_names(&mesh).len(), 12, "the twelve names are distinct");

        for face in mesh.face_names.values() {
            assert!(
                face.label.contains("plane"),
                "a cube face is a plane: {}",
                face.label
            );
        }
        for edge in &mesh.edges {
            assert!(edge.name.starts_with("E:"), "edge name: {}", edge.name);
            assert_eq!(edge.points.len(), 2, "a cube edge is a single line");
            assert!((edge.length - 10.0).abs() < 1e-9
                || (edge.length - 20.0).abs() < 1e-9
                || (edge.length - 30.0).abs() < 1e-9);
        }
    }

    /// The whole point of the change: what identifies a face must be the
    /// geometry, not the kernel address. Two independent builds of the same box
    /// produce the same names, and the names carry the face areas (200, 300 and
    /// 600 mm2 on this box) rather than anything address-like.
    #[test]
    fn names_are_derived_from_the_geometry_and_repeat_across_builds() {
        let first = box_mesh();
        let second = box_mesh();

        assert_eq!(face_names(&first), face_names(&second));
        assert_eq!(edge_names(&first), edge_names(&second));

        let names = face_names(&first);
        for area in ["a200.0000", "a300.0000", "a600.0000"] {
            assert!(
                names.iter().any(|name| name.contains(area)),
                "no face carries {area}: {names:?}"
            );
        }
        for name in &names {
            assert!(
                !name.contains("1398"),
                "a raw TShape address leaked into a face name: {name}"
            );
        }
    }

    /// Every triangle must point at a face that has a stable name, otherwise
    /// the viewport would silently fall back to grey.
    #[test]
    fn every_triangle_face_id_has_a_stable_name() {
        let mesh = box_mesh();
        let distinct: std::collections::HashSet<u64> = mesh.face_ids.iter().copied().collect();
        for face_id in distinct {
            assert!(
                mesh.face_names.contains_key(&face_id),
                "triangle face id {face_id} has no stable name"
            );
        }
        for triangle in 0..mesh.triangle_count() {
            assert!(mesh.face_name(triangle).is_some());
        }
    }

    /// A drilled hole adds a cylindrical face, and its name says so.
    #[test]
    fn a_cylindrical_face_is_named_as_one() {
        let mut backend = OcctBackend::new();
        let block = backend.box_solid(dvec3(40.0, 30.0, 8.0)).unwrap();
        let tool = backend.cylinder(5.0, 20.0).unwrap();
        let drilled = backend.boolean(block, tool, BooleanOp::Subtract).unwrap();
        let mesh = backend.tessellate(drilled).unwrap();

        assert!(
            mesh.face_names
                .values()
                .any(|face| face.name.starts_with("F:cylinder(r5.0000)")),
            "the bore face should be named after its cylinder: {:?}",
            face_names(&mesh)
        );
    }
}
