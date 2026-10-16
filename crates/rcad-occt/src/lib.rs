//! OpenCascade backend powered by [`cadrum`].

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use cadrum::{DVec3 as CVec3, Edge, Solid, Surface, SurfaceKind, Tessellation};
use glam::DVec3;
use rcad_geom::{
    naming, BooleanOp, EdgeFacts, FaceFacts, GeomError, GeomResult, GeometryBackend, NamedEdge,
    NamedFace, Profile2D, ShapeId, TriangleMesh,
};

pub struct OcctBackend {
    next_id: u64,
    solids: HashMap<u64, Solid>,
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
        let id = self.next_id;
        self.next_id += 1;
        self.solids.insert(id, solid);
        ShapeId(id)
    }

    fn get(&self, id: ShapeId) -> GeomResult<&Solid> {
        self.solids
            .get(&id.0)
            .ok_or(GeomError::UnknownShape(id))
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
        // every process. The document needs names that do not.
        result.face_names = stable_face_names(&owned);
        result.edges = stable_edges(&owned);
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

/// Stable name for every face of these solids, keyed by the process-local face
/// id that also appears in the tessellation.
fn stable_face_names(solids: &[Solid]) -> BTreeMap<u64, NamedFace> {
    let mut named: Vec<(u64, NamedFace)> = Vec::new();
    for solid in solids {
        for face in solid.iter_face() {
            let surface = face.surface();
            let kind = describe_surface(surface);
            let center = face.center();
            // Only a planar face has one meaningful normal. On a curved face
            // the normal at the centre depends on where the projection lands,
            // which is not a property of the geometry.
            let normal = match surface.map(|surface| surface.kind) {
                Some(SurfaceKind::Plane) => {
                    let (_, normal) = face.project(center);
                    (normal.length() > 0.0).then(|| normal.normalize().to_array())
                }
                _ => None,
            };
            let edges: Vec<&Edge> = face.iter_edge().collect();
            let perimeter = edges.iter().map(|edge| polyline_length(&edge_polyline(edge))).sum();
            let facts = FaceFacts {
                kind: &kind,
                area: face.area(),
                center: center.to_array(),
                normal,
                edge_count: edges.len(),
                perimeter,
            };
            named.push((
                face.id(),
                NamedFace {
                    name: naming::face_signature(&facts),
                    label: naming::face_label(&facts),
                },
            ));
        }
    }
    let mut faces: Vec<NamedFace> = named.iter().map(|(_, face)| face.clone()).collect();
    naming::disambiguate_faces(&mut faces);
    named
        .into_iter()
        .zip(faces)
        .map(|((id, _), face)| (id, face))
        .collect()
}

/// Stable name for every edge of these solids, with the geometry and polyline
/// the interface needs to draw and pick them.
fn stable_edges(solids: &[Solid]) -> Vec<NamedEdge> {
    let mut edges = Vec::new();
    for solid in solids {
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
            edges.push(NamedEdge {
                name: naming::edge_signature(&facts),
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
    naming::disambiguate_edges(&mut edges);
    edges
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
        Ok(self.insert(solid))
    }

    fn translate(&mut self, shape: ShapeId, delta: DVec3) -> GeomResult<ShapeId> {
        let solid = self.get(shape)?.clone().translate(cvec(delta));
        Ok(self.insert(solid))
    }

    fn fillet_all(&mut self, shape: ShapeId, radius: f64) -> GeomResult<ShapeId> {
        let solid = self.get(shape)?.clone();
        let edges: Vec<_> = solid.iter_edge().collect();
        let filleted = solid
            .fillet_edges(radius, edges)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(self.insert(filleted))
    }

    fn chamfer_all(&mut self, shape: ShapeId, distance: f64) -> GeomResult<ShapeId> {
        let solid = self.get(shape)?.clone();
        let edges: Vec<_> = solid.iter_edge().collect();
        let chamfered = solid
            .chamfer_edges(distance, edges)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(self.insert(chamfered))
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
