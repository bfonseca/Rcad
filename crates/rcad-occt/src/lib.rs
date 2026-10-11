//! OpenCascade backend powered by [`cadrum`].

use std::collections::HashMap;
use std::path::Path;

use cadrum::{DVec3 as CVec3, Edge, Solid, Tessellation};
use glam::DVec3;
use rcad_geom::{
    BooleanOp, GeomError, GeomResult, GeometryBackend, Profile2D, ShapeId, TriangleMesh,
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

    fn take(&mut self, id: ShapeId) -> GeomResult<Solid> {
        self.solids
            .remove(&id.0)
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
        Ok(triangle_mesh_from_cadrum(&mesh))
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
    }
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
        let mesh = self.mesh_of(shapes)?;
        // Rebuild cadrum mesh path via tessellation of solids for proper STL writer
        let solids: GeomResult<Vec<Solid>> =
            shapes.iter().map(|id| self.get(*id).cloned()).collect();
        let solids = solids?;
        let cmesh = Solid::mesh(&solids, Tessellation::default())
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        let mut file = std::fs::File::create(path).map_err(|e| GeomError::Io(e.to_string()))?;
        cmesh
            .write_stl(&mut file)
            .map_err(|e| GeomError::Backend(e.to_string()))?;
        let _ = mesh;
        Ok(())
    }

    fn read_step(&mut self, path: &Path) -> GeomResult<Vec<ShapeId>> {
        let mut file = std::fs::File::open(path).map_err(|e| GeomError::Io(e.to_string()))?;
        let solids =
            Solid::read_step(&mut file).map_err(|e| GeomError::Backend(e.to_string()))?;
        Ok(solids.into_iter().map(|s| self.insert(s)).collect())
    }

    fn drop_shape(&mut self, shape: ShapeId) {
        self.solids.remove(&shape.0);
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
}
