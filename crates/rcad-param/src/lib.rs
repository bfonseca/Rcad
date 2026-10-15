use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Rebuild a [`rcad_doc::Part`] feature tree against a [`GeometryBackend`].

use glam::{dvec3, DVec3};
use rcad_doc::{dvec3_from, ConstructionKind, FeatureKind, Part, RebuildResult};
use rcad_geom::{BooleanOp, GeometryBackend, Profile2D, ShapeId};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RebuildError {
    #[error("{0}")]
    Message(String),
}

pub type RebuildOutcome = Result<RebuildResult, RebuildError>;

/// Compute a hash for a feature based on its parameters
fn compute_feature_hash(feature: &rcad_doc::Feature) -> u64 {
    let mut hasher = DefaultHasher::new();
    feature.id.hash(&mut hasher);
    feature.name.hash(&mut hasher);
    feature.suppressed.hash(&mut hasher);
    feature.visible.hash(&mut hasher);
    match &feature.kind {
        FeatureKind::Box { size } => {
            size[0].to_bits().hash(&mut hasher);
            size[1].to_bits().hash(&mut hasher);
            size[2].to_bits().hash(&mut hasher);
        }
        FeatureKind::Cylinder { radius, height } => {
            radius.to_bits().hash(&mut hasher);
            height.to_bits().hash(&mut hasher);
        }
        FeatureKind::Extrude { profile, height } => {
            height.to_bits().hash(&mut hasher);
            // Hash profile points
            for p in &profile.outer {
                p[0].to_bits().hash(&mut hasher);
                p[1].to_bits().hash(&mut hasher);
            }
        }
        FeatureKind::Revolve { profile, angle_deg } => {
            angle_deg.to_bits().hash(&mut hasher);
            for p in &profile.outer {
                p[0].to_bits().hash(&mut hasher);
                p[1].to_bits().hash(&mut hasher);
            }
        }
        FeatureKind::Construction { kind, origin, direction } => {
            (*kind as u8).hash(&mut hasher);
            origin[0].to_bits().hash(&mut hasher);
            origin[1].to_bits().hash(&mut hasher);
            origin[2].to_bits().hash(&mut hasher);
            direction[0].to_bits().hash(&mut hasher);
            direction[1].to_bits().hash(&mut hasher);
            direction[2].to_bits().hash(&mut hasher);
        }
        _ => {
            // For other feature types, hash the entire kind discriminant
            std::mem::discriminant(&feature.kind).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Shape cache for incremental rebuilds
pub struct ShapeCache {
    /// Cached shape IDs per feature UUID + parameter hash
    entries: std::collections::HashMap<Uuid, (u64, ShapeId)>,
}

impl ShapeCache {
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    pub fn rebuild_part_incremental(
        &mut self,
        part: &Part,
        backend: &mut dyn GeometryBackend,
    ) -> RebuildResult {
        let mut result = RebuildResult::default();
        let mut named: HashMap<Uuid, ShapeId> = HashMap::new();
        let mut history: Vec<(Uuid, ShapeId)> = Vec::new();
        let mut current: Option<ShapeId> = None;
        let mut first_dirty_idx = None;
        let mut visible_shapes: Vec<ShapeId> = Vec::new();

        for (idx, feature) in part.features.iter().enumerate() {
            if feature.suppressed || part.suppressed.contains(&feature.id) {
                continue;
            }
            if !feature.visible && !matches!(feature.kind, FeatureKind::Construction { .. }) {
                continue;
            }

            let hash = compute_feature_hash(feature);
            // let mut feature_changed = false;

            if let Some(&(cached_hash, shape_id)) = self.entries.get(&feature.id) {
                if cached_hash == hash && first_dirty_idx.is_none() {
                    // Feature unchanged: reuse cached shape
                    named.insert(feature.id, shape_id);
                    history.push((feature.id, shape_id));
                    current = Some(shape_id);
                    if feature.visible {
                        visible_shapes.push(shape_id);
                    }
                    continue;
                }
            }

            if first_dirty_idx.is_none() {
                first_dirty_idx = Some(idx);
            }
            // feature_changed = true;

            // Invalidate this feature and all subsequent
            self.entries.remove(&feature.id);
            if let Ok(Some(new_shape)) = eval_feature(&feature.kind, current, &named, &history, backend) {
                self.entries.insert(feature.id, (hash, new_shape));
                named.insert(feature.id, new_shape);
                history.push((feature.id, new_shape));
                if feature.visible {
                    visible_shapes.push(new_shape);
                }
                // Boolean / fillet / transform update the active body
                let updates_body = !matches!(feature.kind, FeatureKind::Construction { .. });
                if updates_body {
                    current = Some(new_shape);
                }
            } else if let Err(e) = eval_feature(&feature.kind, current, &named, &history, backend) {
                result.errors.push(format!("{}: {e}", feature.name));
            }
        }

        // Tessellate the current body (last solid/surface)
        if let Some(shape) = current {
            match backend.tessellate(shape) {
                Ok(mesh) => result.display_mesh = mesh,
                Err(e) => result.errors.push(e.to_string()),
            }
        }
        
        // Also tessellate all visible construction geometry and add to display mesh
        if !visible_shapes.is_empty() {
            match backend.tessellate_many(&visible_shapes) {
                Ok(construction_mesh) => {
                    let offset = result.display_mesh.positions.len() as u32;
                    result.display_mesh.positions.extend(construction_mesh.positions);
                    result.display_mesh.normals.extend(construction_mesh.normals);
                    for idx in construction_mesh.indices {
                        result.display_mesh.indices.push(idx + offset);
                    }
                }
                Err(e) => result.errors.push(e.to_string()),
            }
        }

        result
    }
}

impl Default for ShapeCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate features in order; each feature consumes the previous body unless noted.
pub fn rebuild_part(part: &Part, backend: &mut dyn GeometryBackend) -> RebuildResult {
    let mut cache = ShapeCache::new();
    cache.rebuild_part_incremental(part, backend)
}

fn eval_feature(
    kind: &FeatureKind,
    current: Option<ShapeId>,
    named: &HashMap<Uuid, ShapeId>,
    history: &[(Uuid, ShapeId)],
    backend: &mut dyn GeometryBackend,
) -> Result<Option<ShapeId>, String> {
    match kind {
        FeatureKind::Box { size } => backend
            .box_solid(dvec3(size[0], size[1], size[2]))
            .map(Some)
            .map_err(|e| e.to_string()),
        FeatureKind::Cylinder { radius, height } => backend
            .cylinder(*radius, *height)
            .map(Some)
            .map_err(|e| e.to_string()),
        FeatureKind::Extrude { profile, height } => backend
            .extrude_profile(profile, *height)
            .map(Some)
            .map_err(|e| e.to_string()),
        FeatureKind::ExtrudeSurface { profile, height } => {
            // Until a dedicated surface type exists, extrude a thin solid as proxy.
            backend
                .extrude_profile(profile, (*height).max(0.01))
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Revolve { profile, angle_deg } => {
            let angle = angle_deg.to_radians();
            backend
                .revolve_profile(profile, DVec3::ZERO, DVec3::Z, angle)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Boolean { tool_feature, op } => {
            let tool = named
                .get(tool_feature)
                .copied()
                .ok_or_else(|| "boolean tool feature missing".to_string())?;
            let base = history
                .iter()
                .rev()
                .find(|(id, _)| id != tool_feature)
                .map(|(_, s)| *s)
                .or(current)
                .ok_or_else(|| "boolean needs a base body".to_string())?;
            backend
                .boolean(base, tool, *op)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Translate { delta } => {
            let base = current.ok_or_else(|| "translate needs a body".to_string())?;
            backend
                .translate(base, dvec3_from(*delta))
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Fillet { radius } => {
            let base = current.ok_or_else(|| "fillet needs a body".to_string())?;
            backend
                .fillet_all(base, *radius)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Chamfer { distance } => {
            let base = current.ok_or_else(|| "chamfer needs a body".to_string())?;
            backend
                .chamfer_all(base, *distance)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Sew { tolerance } => {
            let base = current.ok_or_else(|| "sew needs a body".to_string())?;
            backend
                .sew(&[base], *tolerance)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Loft { profiles, solid } => {
            if profiles.is_empty() {
                return Err("loft needs at least one profile".to_string());
            }
            if *solid {
                backend
                    .loft_profiles(profiles, true)
                    .map(Some)
                    .map_err(|e| e.to_string())
            } else {
                backend
                    .loft_profiles(profiles, false)
                    .map(Some)
                    .map_err(|e| e.to_string())
            }
        }
        FeatureKind::Boundary { u_curves, v_curves } => {
            if u_curves.is_empty() || v_curves.is_empty() {
                return Err("boundary needs U and V curves".to_string());
            }
            backend
                .boundary_surface(u_curves, v_curves)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Thicken { surface_feature, distance, both_sides } => {
            let surface = named
                .get(surface_feature)
                .copied()
                .ok_or_else(|| "thicken surface feature missing".to_string())?;
            backend
                .thicken(surface, *distance, *both_sides)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Construction { kind, origin, direction } => {
            let origin_vec = DVec3::from(*origin);
            let dir_vec = DVec3::from(*direction);
            match kind {
                ConstructionKind::Plane => backend
                    .plane(origin_vec, dir_vec, 100.0)
                    .map(Some)
                    .map_err(|e| e.to_string()),
                ConstructionKind::Axis => backend
                    .axis(origin_vec, dir_vec, 100.0)
                    .map(Some)
                    .map_err(|e| e.to_string()),
                ConstructionKind::Point => backend
                    .point(origin_vec, 5.0)
                    .map(Some)
                    .map_err(|e| e.to_string()),
                ConstructionKind::CoordSystem => backend
                    .coordinate_system(
                        origin_vec,
                        DVec3::X,  // X direction
                        DVec3::Y,  // Y direction
                        DVec3::Z,  // Z direction
                        100.0
                    )
                    .map(Some)
                    .map_err(|e| e.to_string()),
                ConstructionKind::Curve => Ok(None), // Curves need special handling
                ConstructionKind::Solid => Ok(None), // Solids handled elsewhere
            }
        }
    }
}

/// Convenience rectangle profile in XY.
pub fn rect_profile(width: f64, height: f64) -> Profile2D {
    let hx = width * 0.5;
    let hy = height * 0.5;
    Profile2D {
        outer: vec![
            [-hx, -hy],
            [hx, -hy],
            [hx, hy],
            [-hx, hy],
        ],
        holes: Vec::new(),
    }
}

/// Demo part: box with a cylindrical cut.
pub fn demo_bracket_features() -> Vec<rcad_doc::Feature> {
    use rcad_doc::Feature;
    vec![
        Feature::new(
            "Base",
            FeatureKind::Box {
                size: [40.0, 30.0, 8.0],
            },
        ),
        Feature::new(
            "HoleTool",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
            },
        ),
    ]
}

pub fn apply_demo_cut(part: &mut Part) {
    if part.features.len() < 2 {
        return;
    }
    let tool = part.features[1].id;
    part.features.push(rcad_doc::Feature::new(
        "CutHole",
        FeatureKind::Boolean {
            tool_feature: tool,
            op: BooleanOp::Subtract,
        },
    ));
}
