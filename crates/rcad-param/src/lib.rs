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

/// Compute a hash for a feature based on every parameter that affects its shape.
///
/// The kind is serialized rather than matched variant-by-variant: an explicit
/// match silently goes stale whenever a new `FeatureKind` is added (the old
/// code hashed *only* the discriminant for Fillet, Chamfer, Boolean, Sew,
/// Loft, Boundary, Thicken and Translate, so changing e.g. a fillet radius
/// reused the stale cached shape).
fn compute_feature_hash(feature: &rcad_doc::Feature) -> u64 {
    let mut hasher = DefaultHasher::new();
    feature.id.hash(&mut hasher);
    feature.name.hash(&mut hasher);
    feature.suppressed.hash(&mut hasher);
    feature.visible.hash(&mut hasher);
    match rmp_serde::to_vec(&feature.kind) {
        Ok(bytes) => bytes.hash(&mut hasher),
        // Serialization is infallible for these plain data types, but never
        // skip hashing entirely if it does fail.
        Err(_) => std::mem::discriminant(&feature.kind).hash(&mut hasher),
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
        let mut current_visible = true;
        let mut first_dirty_idx = None;
        let mut visible_shapes: Vec<ShapeId> = Vec::new();

        for (idx, feature) in part.features.iter().enumerate() {
            if feature.suppressed || part.suppressed.contains(&feature.id) {
                continue;
            }

            let is_construction = matches!(feature.kind, FeatureKind::Construction { .. });
            let hash = compute_feature_hash(feature);

            // Reuse cached shapes only while nothing upstream has changed.
            // Once one feature is dirty, every later feature must be
            // re-evaluated because the body it consumes may have changed.
            // NOTE: hidden features are still evaluated; `visible` is a display
            // flag, and skipping hidden features used to break every feature
            // downstream of a hidden body.
            if first_dirty_idx.is_none() {
                if let Some(&(cached_hash, shape_id)) = self.entries.get(&feature.id) {
                    if cached_hash == hash {
                        named.insert(feature.id, shape_id);
                        history.push((feature.id, shape_id));
                        if is_construction {
                            if feature.visible {
                                visible_shapes.push(shape_id);
                            }
                        } else {
                            current = Some(shape_id);
                            current_visible = feature.visible;
                            result.shapes.push((feature.id, shape_id));
                        }
                        continue;
                    }
                }
            }

            if first_dirty_idx.is_none() {
                first_dirty_idx = Some(idx);
            }

            // Invalidate this feature; later features are re-evaluated below.
            self.entries.remove(&feature.id);

            // Evaluate exactly once. The previous version fell through to a
            // second eval_feature() call in the error branch, which created
            // (and leaked) backend shapes twice for every failing feature.
            match eval_feature(&feature.kind, current, &named, &history, backend) {
                Ok(Some(new_shape)) => {
                    if let Some((_, old_shape)) = self.entries.insert(feature.id, (hash, new_shape))
                    {
                        // Release the superseded shape; otherwise the backend
                        // accumulates one dead solid per rebuild forever.
                        if old_shape != new_shape {
                            backend.drop_shape(old_shape);
                        }
                    }
                    named.insert(feature.id, new_shape);
                    history.push((feature.id, new_shape));
                    if is_construction {
                        if feature.visible {
                            visible_shapes.push(new_shape);
                        }
                    } else {
                        // Boolean / fillet / transform update the active body.
                        current = Some(new_shape);
                        current_visible = feature.visible;
                        result.shapes.push((feature.id, new_shape));
                    }
                }
                // Construction kinds without a B-Rep representation (curve /
                // solid placeholders) legitimately produce nothing.
                Ok(None) => {}
                Err(e) => result.errors.push(format!("{}: {e}", feature.name)),
            }
        }

        // Forget cached shapes for features that no longer exist so a long
        // editing session does not grow the kernel backend without bound.
        let live_ids: std::collections::HashSet<Uuid> =
            part.features.iter().map(|f| f.id).collect();
        let stale: Vec<Uuid> = self
            .entries
            .keys()
            .filter(|id| !live_ids.contains(id))
            .copied()
            .collect();
        for id in stale {
            if let Some((_, shape)) = self.entries.remove(&id) {
                backend.drop_shape(shape);
            }
        }

        // Tessellate the current body (last solid/surface), unless the
        // feature that produced it is hidden.
        if let Some(shape) = current {
            if current_visible {
                match backend.tessellate(shape) {
                    Ok(mesh) => result.display_mesh = mesh,
                    Err(e) => result.errors.push(e.to_string()),
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rcad_doc::Feature;
    use rcad_geom::{GeomError, GeomResult, TriangleMesh};

    /// Minimal in-memory kernel so the rebuild engine can be tested without OCCT.
    #[derive(Default)]
    struct MockBackend {
        next: u64,
        live: HashMap<u64, ()>,
        box_calls: usize,
        fillet_calls: usize,
        fail_everything: bool,
    }

    impl MockBackend {
        fn insert(&mut self) -> ShapeId {
            self.next += 1;
            self.live.insert(self.next, ());
            ShapeId(self.next)
        }

        fn live_shapes(&self) -> usize {
            self.live.len()
        }
    }

    impl GeometryBackend for MockBackend {
        fn box_solid(&mut self, _size: DVec3) -> GeomResult<ShapeId> {
            self.box_calls += 1;
            if self.fail_everything {
                return Err(GeomError::Backend("box failed".into()));
            }
            Ok(self.insert())
        }

        fn cylinder(&mut self, _radius: f64, _height: f64) -> GeomResult<ShapeId> {
            Ok(self.insert())
        }

        fn extrude_profile(&mut self, _profile: &Profile2D, _height: f64) -> GeomResult<ShapeId> {
            Ok(self.insert())
        }

        fn fillet_all(&mut self, _shape: ShapeId, _radius: f64) -> GeomResult<ShapeId> {
            self.fillet_calls += 1;
            Ok(self.insert())
        }

        fn tessellate(&mut self, _shape: ShapeId) -> GeomResult<TriangleMesh> {
            Ok(TriangleMesh::default())
        }

        fn tessellate_many(&mut self, _shapes: &[ShapeId]) -> GeomResult<TriangleMesh> {
            Ok(TriangleMesh::default())
        }

        fn drop_shape(&mut self, shape: ShapeId) {
            self.live.remove(&shape.0);
        }

        fn sphere(&mut self, _radius: f64) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn revolve_profile(
            &mut self,
            _profile: &Profile2D,
            _axis_origin: DVec3,
            _axis_dir: DVec3,
            _angle_rad: f64,
        ) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn boolean(&mut self, _a: ShapeId, _b: ShapeId, _op: BooleanOp) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn translate(&mut self, _shape: ShapeId, _delta: DVec3) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn chamfer_all(&mut self, _shape: ShapeId, _distance: f64) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn sew(&mut self, _shapes: &[ShapeId], _tolerance: f64) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn loft_profiles(&mut self, _profiles: &[Profile2D], _solid: bool) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn boundary_surface(
            &mut self,
            _u_curves: &[Profile2D],
            _v_curves: &[Profile2D],
        ) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn thicken(&mut self, _surface: ShapeId, _distance: f64, _both_sides: bool) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn write_step(&mut self, _shapes: &[ShapeId], _path: &std::path::Path) -> GeomResult<()> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn write_stl(&mut self, _shapes: &[ShapeId], _path: &std::path::Path) -> GeomResult<()> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn read_step(&mut self, _path: &std::path::Path) -> GeomResult<Vec<ShapeId>> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn plane(&mut self, _origin: DVec3, _normal: DVec3, _size: f64) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn axis(&mut self, _origin: DVec3, _direction: DVec3, _length: f64) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn point(&mut self, _position: DVec3, _size: f64) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
        fn coordinate_system(
            &mut self,
            _origin: DVec3,
            _x_dir: DVec3,
            _y_dir: DVec3,
            _z_dir: DVec3,
            _size: f64,
        ) -> GeomResult<ShapeId> {
            unimplemented!("not needed by the rebuild tests")
        }
    }

    fn part_with(features: Vec<Feature>) -> Part {
        let mut part = Part::new_solid("Test");
        part.features = features;
        part
    }

    fn box_and_fillet(radius: f64) -> Vec<Feature> {
        vec![
            Feature::new("Base", FeatureKind::Box { size: [10.0, 10.0, 10.0] }),
            Feature::new("Round", FeatureKind::Fillet { radius }),
        ]
    }

    /// The hash must cover every field of every variant. The original
    /// implementation hashed only the discriminant for Fillet, Chamfer,
    /// Boolean, Sew, Loft, Boundary, Thicken and Translate.
    #[test]
    fn feature_hash_covers_all_parameters() {
        let base = Feature::new("F", FeatureKind::Fillet { radius: 1.0 });

        let mut other_radius = base.clone();
        other_radius.kind = FeatureKind::Fillet { radius: 2.0 };
        assert_ne!(
            compute_feature_hash(&base),
            compute_feature_hash(&other_radius),
            "fillet radius is not part of the feature hash"
        );

        let profile = Profile2D {
            outer: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            holes: Vec::new(),
        };
        let mut with_hole = base.clone();
        let mut hole_profile = profile.clone();
        hole_profile.holes.push(vec![[0.1, 0.1], [0.2, 0.1], [0.1, 0.2]]);
        with_hole.kind = FeatureKind::Extrude {
            profile: hole_profile,
            height: 5.0,
        };
        let mut without_hole = base.clone();
        without_hole.kind = FeatureKind::Extrude {
            profile,
            height: 5.0,
        };
        assert_ne!(
            compute_feature_hash(&with_hole),
            compute_feature_hash(&without_hole),
            "profile holes are not part of the feature hash"
        );
    }

    #[test]
    fn body_chain_shapes_are_recorded() {
        let mut backend = MockBackend::default();
        let result = rebuild_part(&part_with(box_and_fillet(1.0)), &mut backend);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.shapes.len(), 2);
        assert_eq!(backend.fillet_calls, 1);
    }

    #[test]
    fn failing_feature_is_evaluated_exactly_once() {
        let mut backend = MockBackend {
            fail_everything: true,
            ..Default::default()
        };
        let part = part_with(vec![Feature::new(
            "Base",
            FeatureKind::Box { size: [1.0, 1.0, 1.0] },
        )]);
        let result = rebuild_part(&part, &mut backend);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(
            backend.box_calls, 1,
            "a failing feature was evaluated twice, creating shapes twice"
        );
    }

    #[test]
    fn hiding_a_feature_does_not_break_downstream_features() {
        let mut backend = MockBackend::default();
        let mut features = box_and_fillet(2.0);
        features[0].visible = false;
        let result = rebuild_part(&part_with(features), &mut backend);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.shapes.len(), 2);
    }

    #[test]
    fn changing_a_parameter_invalidates_the_cached_shape() {
        let mut backend = MockBackend::default();
        let mut cache = ShapeCache::new();
        let mut part = part_with(box_and_fillet(1.0));

        let first = cache.rebuild_part_incremental(&part, &mut backend);
        let first_body = *first.shapes.last().unwrap();
        let calls_after_first = backend.fillet_calls;

        part.features[1].kind = FeatureKind::Fillet { radius: 4.0 };
        let second = cache.rebuild_part_incremental(&part, &mut backend);
        let second_body = *second.shapes.last().unwrap();

        assert_eq!(
            backend.fillet_calls,
            calls_after_first + 1,
            "stale cache: the fillet was not re-evaluated after its radius changed"
        );
        assert_ne!(first_body, second_body);
    }

    #[test]
    fn unchanged_features_reuse_cached_shapes() {
        let mut backend = MockBackend::default();
        let mut cache = ShapeCache::new();
        let part = part_with(box_and_fillet(1.0));

        let first = cache.rebuild_part_incremental(&part, &mut backend);
        let box_calls = backend.box_calls;
        let fillet_calls = backend.fillet_calls;
        let second = cache.rebuild_part_incremental(&part, &mut backend);

        assert_eq!(backend.box_calls, box_calls, "unchanged box was rebuilt");
        assert_eq!(backend.fillet_calls, fillet_calls, "unchanged fillet was rebuilt");
        assert_eq!(
            first.shapes.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
            second.shapes.iter().map(|(_, s)| *s).collect::<Vec<_>>()
        );
    }

    #[test]
    fn removed_features_release_their_kernel_shapes() {
        let mut backend = MockBackend::default();
        let mut cache = ShapeCache::new();
        let round = Feature::new("Round", FeatureKind::Fillet { radius: 1.0 });
        let round_id = round.id;
        let mut part = part_with(vec![
            Feature::new("Base", FeatureKind::Box { size: [10.0, 10.0, 10.0] }),
            round,
        ]);

        cache.rebuild_part_incremental(&part, &mut backend);
        let before = backend.live_shapes();

        part.features.retain(|f| f.id != round_id);
        let result = cache.rebuild_part_incremental(&part, &mut backend);

        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(
            backend.live_shapes() < before,
            "stale kernel shapes were never released ({} -> {})",
            before,
            backend.live_shapes()
        );
    }
}
