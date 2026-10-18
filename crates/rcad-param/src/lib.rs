use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Rebuild a [`rcad_doc::Part`] feature tree against a [`GeometryBackend`].

use glam::{dvec3, DVec3};
use rcad_doc::{dvec3_from, ConstructionKind, FeatureKind, Part, RebuildResult};
use rcad_geom::{BooleanOp, GeometryBackend, Profile2D, ShapeId};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;
use uuid::Uuid;

pub mod naming;

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

/// One cached feature: its shape, the hash it was built from, and the
/// feature-anchored reference of every face of that shape.
struct CachedShape {
    hash: u64,
    shape: ShapeId,
    refs: naming::RefMap,
    /// Kernel edge id -> stable edge reference, so a fillet created later can
    /// resolve the edges the user picked on this shape.
    edge_refs: BTreeMap<u64, String>,
}

/// Shape cache for incremental rebuilds
pub struct ShapeCache {
    /// Cached shapes per feature UUID + parameter hash
    entries: HashMap<Uuid, CachedShape>,
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
        // Face references of every feature built so far, so a later feature can
        // inherit the names of the body and of its tool.
        let mut named_refs: HashMap<Uuid, naming::RefMap> = HashMap::new();
        let mut named_edge_refs: HashMap<Uuid, BTreeMap<u64, String>> = HashMap::new();
        // The feature that produced the current body, so a fillet can resolve the
        // edge references the user picked on it.
        let mut current_feature: Option<Uuid> = None;
        let mut history: Vec<(Uuid, ShapeId)> = Vec::new();
        let mut current: Option<ShapeId> = None;
        let mut current_refs: Option<naming::RefMap> = None;
        let mut current_visible = true;
        let mut first_dirty_idx = None;
        let mut visible_shapes: Vec<ShapeId> = Vec::new();
        // The plane each sketch's orientation reference resolves to, so an
        // extrude built from that sketch follows the plane the sketch is on and
        // not a snapshot of it.
        let mut resolved_sketch_planes: HashMap<Uuid, rcad_doc::SketchPlacement> =
            HashMap::new();

        for (idx, feature) in part.features.iter().enumerate() {
            // Rollback Here: features after the marker are history, not part of
            // the current model (foundation C1).
            if let Some(rollback) = part.rollback_index {
                if idx > rollback {
                    break;
                }
            }
            if feature.suppressed || part.suppressed.contains(&feature.id) {
                continue;
            }

            let is_construction = matches!(feature.kind, FeatureKind::Construction { .. });
            let hash = compute_feature_hash(feature);

            // A sketch is history, not geometry, but its two positioning
            // references are resolved here: a reference that is gone belongs in
            // the repair list, and one that resolves says which plane the
            // features built from the sketch should follow.
            if let FeatureKind::Sketch { sketch } = &feature.kind {
                if let Some(reference) =
                    sketch.reference(rcad_sketch::SketchReferenceRole::Orientation)
                {
                    let resolved = reference.reference.as_deref().and_then(|reference| {
                        resolve_placement(
                            reference,
                            part,
                            current,
                            backend,
                            current_feature.and_then(|id| named_refs.get(&id)),
                        )
                    });
                    match resolved {
                        Some(placement) => {
                            resolved_sketch_planes.insert(feature.id, placement);
                        }
                        None => {
                            if let Some(text) = &reference.reference {
                                result.lost_references.push(rcad_doc::LostReference {
                                    feature: feature.id,
                                    feature_name: feature.name.clone(),
                                    reference: text.clone(),
                                    last_location: Some(reference.origin),
                                    last_description: Some(format!(
                                        "sketch plane at ({}, {}, {})",
                                        reference.origin[0],
                                        reference.origin[1],
                                        reference.origin[2]
                                    )),
                                });
                            }
                        }
                    }
                }
            }

            // Reuse cached shapes only while nothing upstream has changed.
            // Once one feature is dirty, every later feature must be
            // re-evaluated because the body it consumes may have changed.
            // NOTE: hidden features are still evaluated; `visible` is a display
            // flag, and skipping hidden features used to break every feature
            // downstream of a hidden body.
            if first_dirty_idx.is_none() {
                let cached = self
                    .entries
                    .get(&feature.id)
                    .map(|cached| {
                        (
                            cached.hash,
                            cached.shape,
                            cached.refs.clone(),
                            cached.edge_refs.clone(),
                        )
                    });
                if let Some((cached_hash, shape_id, refs, edge_refs)) = cached {
                    if cached_hash == hash {
                        named.insert(feature.id, shape_id);
                        named_refs.insert(feature.id, refs.clone());
                        named_edge_refs.insert(feature.id, edge_refs);
                        history.push((feature.id, shape_id));
                        if is_construction {
                            if feature.visible {
                                visible_shapes.push(shape_id);
                            }
                        } else {
                            current = Some(shape_id);
                            current_refs = Some(refs);
                            current_feature = Some(feature.id);
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
            // A fillet or chamfer is told which edges to work on, resolved from
            // its edge set against the body it consumes. `references_only` is
            // what tells a set whose picks vanished from the rule modes.
            // Whether this feature is built from a stored sketch, and the
            // sketch's index, so the profile and the plane can be re-read from
            // it instead of using the copy the feature was created with.
            let sketch_source = match &feature.kind {
                FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } => *sketch,
                _ => None,
            };
            // The plane a profile lies on, resolved from its reference when it
            // has one, so the feature follows the face or datum it was drawn on.
            // A feature built from a sketch takes its plane from the sketch
            // instead, further down.
            let mut plane = if sketch_source.is_some() {
                None
            } else {
                match &feature.kind {
                    FeatureKind::Extrude { placement, .. }
                    | FeatureKind::Revolve { placement, .. } => {
                        placement.as_ref().map(|planes| {
                            let resolved = planes.reference.as_deref().and_then(|reference| {
                                resolve_placement(
                                    reference,
                                    part,
                                    current,
                                    backend,
                                    current_feature.and_then(|id| named_refs.get(&id)),
                                )
                            });
                            if resolved.is_none() {
                                if let Some(reference) = &planes.reference {
                                    result.lost_references.push(rcad_doc::LostReference {
                                        feature: feature.id,
                                        feature_name: feature.name.clone(),
                                        reference: reference.clone(),
                                        last_location: Some(planes.origin),
                                        last_description: Some(format!(
                                            "sketch plane at ({}, {}, {})",
                                            planes.origin[0], planes.origin[1], planes.origin[2]
                                        )),
                                    });
                                }
                            }
                            resolved.unwrap_or_else(|| planes.clone())
                        })
                    }
                    _ => None,
                }
            };

            // A feature built from a sketch re-reads it: the profile comes from
            // the sketch and the plane from the sketch's resolved reference, so
            // editing the sketch (or moving the plane it sits on) updates the
            // solid. The stored profile stays as the last known copy.
            let mut kind = feature.kind.clone();
            if let Some(source) = sketch_source {
                match part
                    .features
                    .iter()
                    .find(|candidate| candidate.id == source)
                    .map(|candidate| &candidate.kind)
                {
                    Some(FeatureKind::Sketch { sketch }) => {
                        let fresh = sketch.to_profile();
                        let fallback = rcad_doc::SketchPlacement {
                            origin: sketch.plane_origin,
                            normal: sketch.plane_normal,
                            x_direction: sketch.plane_x_axis,
                            reference: sketch
                                .reference(rcad_sketch::SketchReferenceRole::Orientation)
                                .and_then(|reference| reference.reference.clone()),
                        };
                        match fresh {
                            Some(fresh) => {
                                match &mut kind {
                                    FeatureKind::Extrude { profile, .. }
                                    | FeatureKind::Revolve { profile, .. } => *profile = fresh,
                                    _ => {}
                                }
                                plane = Some(
                                    resolved_sketch_planes
                                        .get(&source)
                                        .cloned()
                                        .unwrap_or(fallback),
                                );
                            }
                            None => {
                                result.errors.push(format!(
                                    "{}: the sketch it is built from has no closed profile",
                                    feature.name
                                ));
                                continue;
                            }
                        }
                    }
                    _ => {
                        result.errors.push(format!(
                            "{}: the sketch it is built from is gone",
                            feature.name
                        ));
                        continue;
                    }
                }
            }

            let references_only = matches!(
                &feature.kind,
                FeatureKind::Fillet { edges, .. } | FeatureKind::Chamfer { edges, .. }
                    if edges.is_references()
            );
            let resolution = match &feature.kind {
                FeatureKind::Fillet { edges, .. } | FeatureKind::Chamfer { edges, .. } => {
                    let topology = match current {
                        Some(shape) => backend.topology(shape).unwrap_or_default(),
                        None => Default::default(),
                    };
                    let references = current_feature.and_then(|id| named_edge_refs.get(&id));
                    let axis = edges.axis().and_then(|axis_id| {
                        part.features
                            .iter()
                            .find(|candidate| candidate.id == axis_id)
                            .and_then(|candidate| match &candidate.kind {
                                FeatureKind::Construction { direction, .. } => unit(*direction),
                                _ => None,
                            })
                    });
                    naming::resolve_edge_set(edges, &topology, references, axis)
                }
                _ => naming::EdgeSetResolution::default(),
            };
            let edge_signatures = resolution.found;

            for reference in &resolution.lost {
                result.lost_references.push(rcad_doc::LostReference {
                    feature: feature.id,
                    feature_name: feature.name.clone(),
                    reference: reference.clone(),
                    last_location: None,
                    last_description: None,
                });
            }
            // A round whose references are all gone must not quietly become a
            // round of every edge: skip it and let the user repair it.
            if references_only
                && edge_signatures.is_empty()
                && !resolution.lost.is_empty()
            {
                result.errors.push(format!(
                    "{}: none of its edge references exist any more ({} lost)",
                    feature.name,
                    resolution.lost.len()
                ));
                continue;
            }
            match eval_feature(
                &kind,
                current,
                &named,
                &history,
                backend,
                &edge_signatures,
                plane,
            ) {
                Ok(Some(new_shape)) => {
                    // Name the new topology before any later feature can consume
                    // it: its faces inherit the reference of the source face
                    // they came from, or take a role from this feature.
                    let topology = backend.topology(new_shape).unwrap_or_default();
                    let tool_refs = match &feature.kind {
                        FeatureKind::Boolean { tool_feature, .. } => {
                            named_refs.get(tool_feature).cloned()
                        }
                        _ => None,
                    };
                    // A boolean does not consume the last body: it uses the last
                    // shape in the history that is not its tool, exactly as
                    // `eval_feature` does. Everything else consumes `current`.
                    let input_refs = match &feature.kind {
                        FeatureKind::Boolean { tool_feature, .. } => history
                            .iter()
                            .rev()
                            .find(|(id, _)| id != tool_feature)
                            .and_then(|(id, _)| named_refs.get(id).cloned()),
                        _ => current_refs.clone(),
                    };
                    let face_refs = naming::derive_face_refs(
                        feature,
                        &topology,
                        input_refs.as_ref(),
                        tool_refs.as_ref(),
                    );
                    let refs = naming::RefMap::new(&topology, &face_refs);
                    let edge_refs = naming::derive_edge_refs(&topology, &face_refs);

                    let previous = self.entries.insert(
                        feature.id,
                        CachedShape {
                            hash,
                            shape: new_shape,
                            refs: refs.clone(),
                            edge_refs: edge_refs.clone(),
                        },
                    );
                    // Release the superseded shape; otherwise the backend
                    // accumulates one dead solid per rebuild forever.
                    if let Some(previous) = previous {
                        if previous.shape != new_shape {
                            backend.drop_shape(previous.shape);
                        }
                    }
                    named.insert(feature.id, new_shape);
                    named_refs.insert(feature.id, refs.clone());
                    named_edge_refs.insert(feature.id, edge_refs);
                    history.push((feature.id, new_shape));
                    if is_construction {
                        if feature.visible {
                            visible_shapes.push(new_shape);
                        }
                    } else {
                        // Boolean / fillet / transform update the active body.
                        current = Some(new_shape);
                        current_refs = Some(refs);
                        current_feature = Some(feature.id);
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
            if let Some(cached) = self.entries.remove(&id) {
                backend.drop_shape(cached.shape);
            }
        }

        // Tessellate the current body (last solid/surface), unless the
        // feature that produced it is hidden.
        if let Some(shape) = current {
            if current_visible {
                match backend.tessellate(shape) {
                    Ok(mut mesh) => {
                        // The mesh carries geometric signatures. Replace them
                        // with the references this rebuild derived, and name
                        // the edges from the two faces they bound.
                        let topology = backend.topology(shape).unwrap_or_default();
                        let empty = naming::RefMap::default();
                        naming::apply_references(
                            &mut mesh,
                            &topology,
                            current_refs.as_ref().unwrap_or(&empty),
                        );
                        result.display_mesh = mesh;
                    }
                    Err(e) => result.errors.push(e.to_string()),
                }
            }
        }
        
        // Also tessellate all visible construction geometry and add to display mesh
        if !visible_shapes.is_empty() {
            match backend.tessellate_many(&visible_shapes) {
                Ok(construction_mesh) => {
                    // Construction geometry belongs to no solid face, so its
                    // triangles carry the sentinel face id and can never be
                    // matched by a face colour set on the model.
                    result
                        .display_mesh
                        .append(&construction_mesh, rcad_geom::NO_FACE_ID);
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

/// The plane a feature profile lies on, resolved from the reference it came
/// from so the feature follows the face or datum it was drawn on.
///
/// A datum plane is named by its feature id and never moves; a model face is
/// named by its stable reference and is looked up on the body this feature
/// consumes. `None` means the reference is gone: the caller keeps the last known
/// numbers and reports it, which is what the repair list shows.
fn resolve_placement(
    reference: &str,
    part: &Part,
    current: Option<ShapeId>,
    backend: &mut dyn GeometryBackend,
    refs: Option<&naming::RefMap>,
) -> Option<rcad_doc::SketchPlacement> {
    if let Ok(id) = Uuid::parse_str(reference) {
        if let Some(feature) = part.features.iter().find(|candidate| candidate.id == id) {
            if let FeatureKind::Construction {
                kind: ConstructionKind::Plane,
                origin,
                direction,
            } = &feature.kind
            {
                let normal = unit(*direction)?;
                let (x_direction, _) = rcad_geom::plane_basis(normal, [1.0, 0.0, 0.0]);
                return Some(rcad_doc::SketchPlacement {
                    origin: *origin,
                    normal,
                    x_direction,
                    reference: Some(reference.to_string()),
                });
            }
        }
    }

    let shape = current?;
    let refs = refs?;
    let face_id = refs
        .by_id
        .iter()
        .find(|(_, name)| name.as_str() == reference)
        .map(|(id, _)| *id)?;
    let topology = backend.topology(shape).ok()?;
    let face = topology.faces.get(&face_id)?;
    let normal = unit(face.normal?)?;
    let (x_direction, _) = rcad_geom::plane_basis(normal, [1.0, 0.0, 0.0]);
    Some(rcad_doc::SketchPlacement {
        origin: face.center,
        normal,
        x_direction,
        reference: Some(reference.to_string()),
    })
}

/// Unit vector, or `None` when the vector is degenerate.
fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (length > 1e-9).then(|| [v[0] / length, v[1] / length, v[2] / length])
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
    edge_signatures: &[String],
    plane: Option<rcad_doc::SketchPlacement>,
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
        FeatureKind::Extrude {
            profile, height, ..
        } => {
            // A profile that knows where it lies is swept along its own normal;
            // one that does not is taken to be on the XY plane, as before.
            let shape = match plane.as_ref() {
                Some(plane) => backend.extrude_profile_placed(
                    profile,
                    *height,
                    dvec3(plane.origin[0], plane.origin[1], plane.origin[2]),
                    dvec3(
                        plane.x_direction[0],
                        plane.x_direction[1],
                        plane.x_direction[2],
                    ),
                    dvec3(plane.normal[0], plane.normal[1], plane.normal[2]),
                ),
                None => backend.extrude_profile(profile, *height),
            };
            shape.map(Some).map_err(|e| e.to_string())
        }
        FeatureKind::ExtrudeSurface { profile, height } => {
            // Until a dedicated surface type exists, extrude a thin solid as proxy.
            backend
                .extrude_profile(profile, (*height).max(0.01))
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Revolve {
            profile, angle_deg, ..
        } => {
            let angle = angle_deg.to_radians();
            // A profile that knows its plane is revolved about the in-plane axis
            // of that plane; one that does not keeps the old world-Z axis.
            let shape = match plane.as_ref() {
                Some(plane) => backend.revolve_profile_placed(
                    profile,
                    angle,
                    dvec3(plane.origin[0], plane.origin[1], plane.origin[2]),
                    dvec3(
                        plane.x_direction[0],
                        plane.x_direction[1],
                        plane.x_direction[2],
                    ),
                    dvec3(plane.normal[0], plane.normal[1], plane.normal[2]),
                ),
                None => backend.revolve_profile(profile, DVec3::ZERO, DVec3::Z, angle),
            };
            shape.map(Some).map_err(|e| e.to_string())
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
        FeatureKind::Fillet { radius, .. } => {
            let base = current.ok_or_else(|| "fillet needs a body".to_string())?;
            backend
                .fillet_edges(base, *radius, edge_signatures)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        FeatureKind::Chamfer { distance, .. } => {
            let base = current.ok_or_else(|| "chamfer needs a body".to_string())?;
            backend
                .chamfer_edges(base, *distance, edge_signatures)
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
        // A sketch is history, not geometry: it keeps the body it found and the
        // next feature (an extrude or a revolve) reads its profile.
        FeatureKind::Sketch { .. } => Ok(None),
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
    Profile2D::from_polygon(vec![[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]])
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
        /// Mesh the mock kernel returns for the solid body.
        solid_mesh: TriangleMesh,
        /// Mesh the mock kernel returns for construction geometry.
        construction_mesh: TriangleMesh,
        /// The last profile the mock was asked to extrude, and the plane it was
        /// swept from: enough to see whether a feature re-read its sketch.
        last_extrude_profile: Option<Profile2D>,
        last_extrude_origin: Option<DVec3>,
        last_extrude_normal: Option<DVec3>,
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

        fn extrude_profile(&mut self, profile: &Profile2D, _height: f64) -> GeomResult<ShapeId> {
            self.last_extrude_profile = Some(profile.clone());
            Ok(self.insert())
        }

        fn extrude_profile_placed(
            &mut self,
            profile: &Profile2D,
            _height: f64,
            origin: DVec3,
            _x_direction: DVec3,
            normal: DVec3,
        ) -> GeomResult<ShapeId> {
            self.last_extrude_profile = Some(profile.clone());
            self.last_extrude_origin = Some(origin);
            self.last_extrude_normal = Some(normal);
            Ok(self.insert())
        }

        fn fillet_all(&mut self, _shape: ShapeId, _radius: f64) -> GeomResult<ShapeId> {
            self.fillet_calls += 1;
            Ok(self.insert())
        }

        fn tessellate(&mut self, _shape: ShapeId) -> GeomResult<TriangleMesh> {
            Ok(self.solid_mesh.clone())
        }

        fn tessellate_many(&mut self, _shapes: &[ShapeId]) -> GeomResult<TriangleMesh> {
            Ok(self.construction_mesh.clone())
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
            Ok(self.insert())
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
            Feature::new("Round", FeatureKind::Fillet { radius, edges: Default::default() }),
        ]
    }

    /// The hash must cover every field of every variant. The original
    /// implementation hashed only the discriminant for Fillet, Chamfer,
    /// Boolean, Sew, Loft, Boundary, Thicken and Translate.
    #[test]
    fn feature_hash_covers_all_parameters() {
        let base = Feature::new("F", FeatureKind::Fillet { radius: 1.0, edges: Default::default() });

        let mut other_radius = base.clone();
        other_radius.kind = FeatureKind::Fillet { radius: 2.0, edges: Default::default() };
        assert_ne!(
            compute_feature_hash(&base),
            compute_feature_hash(&other_radius),
            "fillet radius is not part of the feature hash"
        );

        let profile = Profile2D::from_polygon(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        let mut with_hole = base.clone();
        let hole_profile = Profile2D::from_loops(
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            vec![vec![[0.1, 0.1], [0.2, 0.1], [0.1, 0.2]]],
        );
        with_hole.kind = FeatureKind::Extrude {
            placement: None,
            profile: hole_profile,
            height: 5.0,
            sketch: None,
        };
        let mut without_hole = base.clone();
        without_hole.kind = FeatureKind::Extrude {
            placement: None,
            profile,
            height: 5.0,
            sketch: None,
        };
        assert_ne!(
            compute_feature_hash(&with_hole),
            compute_feature_hash(&without_hole),
            "profile holes are not part of the feature hash"
        );
    }


    /// A sketch placed on a datum plane, and the extrude built from it: the
    /// rebuild re-reads the sketch, so an edited sketch updates the solid.
    #[test]
    fn an_extrude_rereads_the_sketch_it_came_from() {
        use rcad_sketch::{Sketch, SketchReference, SketchReferenceRole};

        let mut sketch = Sketch::new("Sketch1");
        sketch.add_rectangle(0.0, 0.0, 10.0, 5.0);
        sketch.set_references(
            SketchReference {
                reference: None,
                label: "XY".to_string(),
                role: SketchReferenceRole::Orientation,
                origin: [0.0; 3],
                direction: [1.0, 0.0, 0.0],
            },
            SketchReference {
                reference: None,
                label: "CYS (origin)".to_string(),
                role: SketchReferenceRole::Automatic,
                origin: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
            },
        );
        let sketch_feature = Feature::new("Sketch1", FeatureKind::Sketch { sketch });
        let source = sketch_feature.id;
        let stale =
            rcad_geom::Profile2D::from_polygon(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        let extrude = Feature::new(
            "Extrude1",
            FeatureKind::Extrude {
                profile: stale,
                height: 5.0,
                placement: None,
                sketch: Some(source),
            },
        );

        let mut backend = MockBackend::default();
        let result = rebuild_part(&part_with(vec![sketch_feature, extrude]), &mut backend);

        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let profile = backend
            .last_extrude_profile
            .expect("the sketch profile was extruded");
        assert_eq!(profile.outer.len(), 4, "the stale profile was used");
        let highest = profile
            .sample_outer(1)
            .iter()
            .map(|point| point[0])
            .fold(f64::MIN, f64::max);
        assert!((highest - 10.0).abs() < 1e-9, "{highest}");
    }

    /// The sketch's reference is re-resolved on every rebuild: a sketch on a
    /// datum plane follows the plane when it moves.
    #[test]
    fn a_sketch_follows_the_plane_it_is_placed_on() {
        use rcad_sketch::{Sketch, SketchReference, SketchReferenceRole};

        let plane = Feature::new(
            "Front",
            FeatureKind::Construction {
                kind: ConstructionKind::Plane,
                origin: [0.0, 0.0, 10.0],
                direction: [0.0, 0.0, 1.0],
            },
        );
        let plane_id = plane.id;
        let mut sketch = Sketch::new("Sketch1");
        sketch.add_rectangle(0.0, 0.0, 10.0, 5.0);
        sketch.set_references(
            SketchReference {
                reference: Some(plane_id.to_string()),
                label: "Front".to_string(),
                role: SketchReferenceRole::Orientation,
                origin: [0.0, 0.0, 10.0],
                direction: [1.0, 0.0, 0.0],
            },
            SketchReference {
                reference: None,
                label: "CYS (origin)".to_string(),
                role: SketchReferenceRole::Automatic,
                origin: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
            },
        );
        let sketch_feature = Feature::new("Sketch1", FeatureKind::Sketch { sketch });
        let source = sketch_feature.id;
        let extrude = Feature::new(
            "Extrude1",
            FeatureKind::Extrude {
                profile: rcad_geom::Profile2D::from_polygon(vec![
                    [0.0, 0.0],
                    [1.0, 0.0],
                    [0.0, 1.0],
                ]),
                height: 5.0,
                placement: None,
                sketch: Some(source),
            },
        );

        let mut first = MockBackend::default();
        let part = part_with(vec![plane, sketch_feature, extrude]);
        let result = rebuild_part(&part, &mut first);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let origin = first.last_extrude_origin.expect("a placed extrude");
        assert!((origin.z - 10.0).abs() < 1e-9, "{origin:?}");

        // Move the plane: the sketch and the extrude follow it.
        let mut moved = part.clone();
        moved.features[0].kind = FeatureKind::Construction {
            kind: ConstructionKind::Plane,
            origin: [0.0, 0.0, 20.0],
            direction: [0.0, 0.0, 1.0],
        };
        let mut second = MockBackend::default();
        let result = rebuild_part(&moved, &mut second);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let origin = second.last_extrude_origin.expect("a placed extrude");
        assert!((origin.z - 20.0).abs() < 1e-9, "{origin:?}");
    }

    /// A sketch whose plane reference is gone is listed for repair.
    #[test]
    fn a_sketch_reports_a_reference_that_is_gone() {
        use rcad_sketch::{Sketch, SketchReference, SketchReferenceRole};

        let plane = Feature::new(
            "Front",
            FeatureKind::Construction {
                kind: ConstructionKind::Plane,
                origin: [0.0, 0.0, 10.0],
                direction: [0.0, 0.0, 1.0],
            },
        );
        let plane_id = plane.id;
        let mut sketch = Sketch::new("Sketch1");
        sketch.add_rectangle(0.0, 0.0, 10.0, 5.0);
        sketch.set_references(
            SketchReference {
                reference: Some(plane_id.to_string()),
                label: "Front".to_string(),
                role: SketchReferenceRole::Orientation,
                origin: [0.0, 0.0, 10.0],
                direction: [1.0, 0.0, 0.0],
            },
            SketchReference {
                reference: None,
                label: "CYS (origin)".to_string(),
                role: SketchReferenceRole::Automatic,
                origin: [0.0; 3],
                direction: [0.0, 0.0, 1.0],
            },
        );
        let sketch_feature = Feature::new("Sketch1", FeatureKind::Sketch { sketch });
        let sketch_id = sketch_feature.id;
        // The plane is deleted from the tree; the sketch stays.
        let _ = plane;
        let part = part_with(vec![sketch_feature]);

        let mut backend = MockBackend::default();
        let result = rebuild_part(&part, &mut backend);

        let entry = result
            .lost_references
            .iter()
            .find(|entry| entry.feature == sketch_id)
            .expect("the sketch reference is reported");
        assert_eq!(entry.reference, plane_id.to_string());
        assert!(entry.last_description.as_deref().unwrap_or("").contains("sketch plane"));
    }

    /// A feature built from a sketch that is no longer there fails with a
    /// sentence naming it, instead of building from a stale profile.
    #[test]
    fn an_extrude_from_a_missing_sketch_says_so() {
        let extrude = Feature::new(
            "Extrude1",
            FeatureKind::Extrude {
                profile: rcad_geom::Profile2D::from_polygon(vec![
                    [0.0, 0.0],
                    [1.0, 0.0],
                    [0.0, 1.0],
                ]),
                height: 5.0,
                placement: None,
                sketch: Some(Uuid::new_v4()),
            },
        );
        let mut backend = MockBackend::default();
        let result = rebuild_part(&part_with(vec![extrude]), &mut backend);
        assert!(
            result.errors.iter().any(|error| error.contains("is gone")),
            "{:?}",
            result.errors
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

    /// A one-triangle mesh tagged with a face id, to stand in for a kernel
    /// tessellation.
    fn triangle_mesh_at(x: f32, face_id: u64) -> TriangleMesh {
        TriangleMesh {
            positions: vec![[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            face_ids: vec![face_id],
            ..Default::default()
        }
    }

    /// The display mesh is the solid plus construction geometry. The
    /// construction triangles must carry the sentinel face id, otherwise a
    /// colour set on a model face could light up a datum plane.
    #[test]
    fn construction_triangles_are_sentinel_tagged() {
        let mut backend = MockBackend {
            solid_mesh: triangle_mesh_at(0.0, 42),
            construction_mesh: triangle_mesh_at(10.0, 7),
            ..Default::default()
        };
        let part = part_with(vec![
            Feature::new("Base", FeatureKind::Box { size: [10.0, 10.0, 10.0] }),
            Feature::new(
                "Plane",
                FeatureKind::Construction {
                    kind: ConstructionKind::Plane,
                    origin: [0.0, 0.0, 0.0],
                    direction: [0.0, 0.0, 1.0],
                },
            ),
        ]);

        let result = rebuild_part(&part, &mut backend);

        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.display_mesh.triangle_count(), 2);
        assert_eq!(
            result.display_mesh.face_ids,
            vec![42, rcad_geom::NO_FACE_ID],
            "construction geometry must not inherit a solid face id"
        );
        assert_eq!(result.display_mesh.indices, vec![0, 1, 2, 3, 4, 5]);
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

        part.features[1].kind = FeatureKind::Fillet { radius: 4.0, edges: Default::default() };
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
        let round = Feature::new("Round", FeatureKind::Fillet { radius: 1.0, edges: Default::default() });
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

    #[test]
    fn a_rollback_marker_stops_later_features_from_rebuilding() {
        let mut part = Part::new_solid("P");
        part.features
            .push(Feature::new("Box1", FeatureKind::Box { size: [1.0; 3] }));
        part.features
            .push(Feature::new("Box2", FeatureKind::Box { size: [2.0; 3] }));
        part.rollback_index = Some(0);

        let mut backend = MockBackend::default();
        let result = rebuild_part(&part, &mut backend);

        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(backend.box_calls, 1, "only the feature before the marker built");
    }
}

// ---------------------------------------------------------------------------
// Relations
// ---------------------------------------------------------------------------

/// Evaluate the part relations in order, writing each left-hand side back into
/// `part.parameters` as a real value.
///
/// Supported syntax: `+ - * /`, parentheses, unary minus, numbers and names.
/// A name resolves to an existing parameter, or to a value defined earlier in
/// the relation list. Returns one message per relation that failed.
pub fn evaluate_relations(part: &mut Part) -> Vec<String> {
    let mut scope: HashMap<String, f64> = HashMap::new();
    for (name, parameter) in &part.parameters {
        match &parameter.value {
            rcad_doc::ParameterValue::Real(value) => {
                scope.insert(name.clone(), *value);
            }
            rcad_doc::ParameterValue::Integer(value) => {
                scope.insert(name.clone(), *value as f64);
            }
            _ => {}
        }
    }

    let mut errors = Vec::new();
    for relation in part.relations.clone() {
        // Tolerate "name = expression" in the right-hand side, which is how the
        // relation field was documented.
        let expression = match relation.rhs.split_once('=') {
            Some((_, tail)) => tail.trim().to_string(),
            None => relation.rhs.clone(),
        };

        match eval_expression(&expression, &scope) {
            Ok(value) => {
                scope.insert(relation.lhs.clone(), value);
                let entry = part
                    .parameters
                    .entry(relation.lhs.clone())
                    .or_insert_with(|| rcad_doc::Parameter {
                        name: relation.lhs.clone(),
                        value: rcad_doc::ParameterValue::Real(value),
                        description: Some("computed by a relation".to_string()),
                        is_global: false,
                    });
                entry.value = rcad_doc::ParameterValue::Real(value);
            }
            Err(err) => errors.push(format!("{}: {err}", relation.lhs)),
        }
    }
    errors
}

/// Evaluate a single arithmetic expression against `scope`.
pub fn eval_expression(input: &str, scope: &HashMap<String, f64>) -> Result<f64, String> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err("empty expression".to_string());
    }
    let mut parser = ExprParser {
        tokens: &tokens,
        pos: 0,
        scope,
    };
    let value = parser.parse_sum()?;
    if parser.pos != tokens.len() {
        return Err("unexpected trailing input".to_string());
    }
    if !value.is_finite() {
        return Err("expression is not a finite number".to_string());
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let value: f64 = text
                    .parse()
                    .map_err(|_| format!("invalid number '{text}'"))?;
                tokens.push(Token::Number(value));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Name(chars[start..i].iter().collect()));
            }
            other => return Err(format!("unexpected character '{other}'")),
        }
    }
    Ok(tokens)
}

struct ExprParser<'a> {
    tokens: &'a [Token],
    pos: usize,
    scope: &'a HashMap<String, f64>,
}

impl<'a> ExprParser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn parse_sum(&mut self) -> Result<f64, String> {
        let mut value = self.parse_product()?;
        loop {
            if matches!(self.peek(), Some(Token::Plus)) {
                self.pos += 1;
                value += self.parse_product()?;
            } else if matches!(self.peek(), Some(Token::Minus)) {
                self.pos += 1;
                value -= self.parse_product()?;
            } else {
                break;
            }
        }
        Ok(value)
    }

    fn parse_product(&mut self) -> Result<f64, String> {
        let mut value = self.parse_unary()?;
        loop {
            if matches!(self.peek(), Some(Token::Star)) {
                self.pos += 1;
                value *= self.parse_unary()?;
            } else if matches!(self.peek(), Some(Token::Slash)) {
                self.pos += 1;
                let divisor = self.parse_unary()?;
                if divisor == 0.0 {
                    return Err("division by zero".to_string());
                }
                value /= divisor;
            } else {
                break;
            }
        }
        Ok(value)
    }

    fn parse_unary(&mut self) -> Result<f64, String> {
        if matches!(self.peek(), Some(Token::Minus)) {
            self.pos += 1;
            return Ok(-self.parse_unary()?);
        }
        if matches!(self.peek(), Some(Token::Plus)) {
            self.pos += 1;
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<f64, String> {
        match self.peek().cloned() {
            Some(Token::Number(value)) => {
                self.pos += 1;
                Ok(value)
            }
            Some(Token::Name(name)) => {
                self.pos += 1;
                self.scope
                    .get(&name)
                    .copied()
                    .ok_or_else(|| format!("unknown parameter '{name}'"))
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let value = self.parse_sum()?;
                if matches!(self.peek(), Some(Token::RParen)) {
                    self.pos += 1;
                    Ok(value)
                } else {
                    Err("missing ')'".to_string())
                }
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

#[cfg(test)]
mod relation_tests {
    use super::*;
    use rcad_doc::{Parameter, ParameterValue, Part, Relation};

    fn part_with(parameters: Vec<(&str, f64)>) -> Part {
        let mut part = Part::new_solid("P");
        for (name, value) in parameters {
            part.parameters.insert(
                name.to_string(),
                Parameter {
                    name: name.to_string(),
                    value: ParameterValue::Real(value),
                    description: None,
                    is_global: false,
                },
            );
        }
        part
    }

    fn relation(lhs: &str, rhs: &str) -> Relation {
        Relation {
            lhs: lhs.to_string(),
            rhs: rhs.to_string(),
            comment: None,
        }
    }

    fn real(part: &Part, name: &str) -> f64 {
        match &part.parameters.get(name).expect("parameter exists").value {
            ParameterValue::Real(value) => *value,
            other => panic!("expected a real parameter, got {other:?}"),
        }
    }

    #[test]
    fn evaluates_arithmetic_with_correct_precedence() {
        let scope = HashMap::new();
        assert_eq!(eval_expression("2 + 3 * 4", &scope).unwrap(), 14.0);
        assert_eq!(eval_expression("(2 + 3) * 4", &scope).unwrap(), 20.0);
        assert_eq!(eval_expression("-4 + 10 / 2", &scope).unwrap(), 1.0);
    }

    #[test]
    fn resolves_names_from_the_scope() {
        let mut scope = HashMap::new();
        scope.insert("width".to_string(), 10.0);
        assert_eq!(eval_expression("width * 2 + 1", &scope).unwrap(), 21.0);
    }

    #[test]
    fn reports_unknown_names_and_bad_syntax() {
        let scope = HashMap::new();
        assert!(eval_expression("missing + 1", &scope).is_err());
        assert!(eval_expression("1 +", &scope).is_err());
        assert!(eval_expression("(1 + 2", &scope).is_err());
        assert!(eval_expression("1 / 0", &scope).is_err());
        assert!(eval_expression("", &scope).is_err());
    }

    #[test]
    fn relations_write_results_back_into_the_part() {
        let mut part = part_with(vec![("width", 10.0)]);
        part.relations.push(relation("height", "width * 2"));
        part.relations.push(relation("volume", "width * height * 5"));

        let errors = evaluate_relations(&mut part);

        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(real(&part, "height"), 20.0);
        // Later relations can use values defined by earlier ones.
        assert_eq!(real(&part, "volume"), 1000.0);
    }

    #[test]
    fn accepts_an_assignment_in_the_right_hand_side() {
        let mut part = part_with(vec![("shaft_dia", 20.0)]);
        part.relations
            .push(relation("hole_dia", "hole_dia = shaft_dia + 0.5"));

        let errors = evaluate_relations(&mut part);

        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(real(&part, "hole_dia"), 20.5);
    }

    #[test]
    fn a_failing_relation_is_reported_and_does_not_stop_the_rest() {
        let mut part = part_with(vec![("a", 2.0)]);
        part.relations.push(relation("bad", "nope * 2"));
        part.relations.push(relation("good", "a * 3"));

        let errors = evaluate_relations(&mut part);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("bad:"), "{errors:?}");
        assert_eq!(real(&part, "good"), 6.0);
    }
}
