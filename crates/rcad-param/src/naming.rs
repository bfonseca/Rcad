//! Feature-anchored references for faces and edges.
//!
//! A geometric signature is stable only while the geometry is unchanged: change
//! the height of a box and every signature on it changes with the geometry.
//! Creo solves this by naming topology from the *feature* that created it and
//! following that name through regeneration. This module does the same with
//! what the kernel gives us:
//!
//! - a primitive names its own faces by role (`<feature>/box/z+`), a role taken
//!   from the feature definition rather than from the computed geometry, so it
//!   survives a parameter change;
//! - an operation that records face history (boolean, fillet, chamfer) makes
//!   each result face inherit the reference of the source face it came from;
//! - an operation that preserves face ids (translate, rotate) inherits by id;
//! - anything else falls back to the geometric signature;
//! - edges are named from the two faces they bound, so an edge keeps its
//!   reference exactly as long as its faces do. That is what lets a fillet or a
//!   chamfer reference survive a parameter change.
//!
//! A reference is `<feature uuid>/<role>`, `<source feature uuid>/<source role>`
//! or a geometric signature. The uuid, not the feature name: renaming a feature
//! must not break the references stored in a document.

use std::collections::{BTreeMap, HashMap};

use rcad_doc::{EdgeSet, Feature, FeatureKind};
use rcad_geom::{FaceInfo, ShapeTopology, TriangleMesh};

/// The reference of one face of `feature`.
pub fn face_reference(feature: &Feature, face: &FaceInfo) -> String {
    role_of(feature, face).unwrap_or_else(|| face.signature.clone())
}

/// The references of one shape, looked up by kernel id and by geometry.
///
/// An operation records its history in terms of the *signature* of the source
/// face, because the ids belong to copies the caller never sees. The id map is
/// still needed for operations that preserve face ids (translate, rotate).
#[derive(Debug, Clone, Default)]
pub struct RefMap {
    pub by_id: BTreeMap<u64, String>,
    pub by_signature: BTreeMap<String, String>,
}

impl RefMap {
    /// Pair a topology with the references derived for it.
    pub fn new(topology: &ShapeTopology, refs: &BTreeMap<u64, String>) -> Self {
        let by_signature = topology
            .faces
            .iter()
            .filter_map(|(face_id, face)| {
                refs.get(face_id)
                    .map(|reference| (face.signature.clone(), reference.clone()))
            })
            .collect();
        Self {
            by_id: refs.clone(),
            by_signature,
        }
    }
}

/// References for every face of a freshly built shape.
///
/// `input` is the reference map of the body the feature consumed and `tool`
/// that of its tool feature, for a boolean. A face that came from the tool is
/// namespaced under this feature: two holes cut with the same tool must not
/// share a name.
pub fn derive_face_refs(
    feature: &Feature,
    topology: &ShapeTopology,
    input: Option<&RefMap>,
    tool: Option<&RefMap>,
) -> BTreeMap<u64, String> {
    let roles = role_refs(feature, topology);
    let mut refs: BTreeMap<u64, String> = BTreeMap::new();
    for (face_id, face) in &topology.faces {
        // A face the operation derived from an input face inherits that face's
        // reference, so it survives a change in the parameters that moved it.
        let inherited = topology.face_history.get(face_id).and_then(|source| {
            let (map, from_tool) = if source.from_tool {
                (tool, true)
            } else {
                (input, false)
            };
            map.and_then(|map| map.by_signature.get(&source.source_signature))
                .map(|reference| {
                    if from_tool {
                        format!("{}/{}", feature.id, reference)
                    } else {
                        reference.clone()
                    }
                })
        });
        let reference = inherited
            // An operation that keeps face ids (translate, rotate) has no
            // history, but the faces are literally the same ones.
            .or_else(|| input.and_then(|map| map.by_id.get(face_id).cloned()))
            .or_else(|| roles.get(face_id).cloned())
            .unwrap_or_else(|| face.signature.clone());
        refs.insert(*face_id, reference);
    }
    disambiguate_faces(&mut refs, topology);
    refs
}

/// References for every edge, from the two faces it bounds.
pub fn derive_edge_refs(
    topology: &ShapeTopology,
    face_refs: &BTreeMap<u64, String>,
) -> BTreeMap<u64, String> {
    let mut incident: HashMap<u64, Vec<u64>> = HashMap::new();
    for (face_id, face) in &topology.faces {
        for edge_id in &face.edges {
            incident.entry(*edge_id).or_default().push(*face_id);
        }
    }

    let mut refs: BTreeMap<u64, String> = BTreeMap::new();
    for (edge_id, edge) in &topology.edges {
        let mut faces: Vec<&String> = incident
            .get(edge_id)
            .map(|faces| {
                faces
                    .iter()
                    .filter_map(|face_id| face_refs.get(face_id))
                    .collect()
            })
            .unwrap_or_default();
        faces.sort();
        faces.dedup();
        let reference = match faces.as_slice() {
            [] => edge.signature.clone(),
            [only] => format!("edge({only}|seam)"),
            [a, b] => format!("edge({a}|{b})"),
            many => format!(
                "edge({})",
                many.iter().map(|name| name.as_str()).collect::<Vec<_>>().join("|")
            ),
        };
        refs.insert(*edge_id, reference);
    }
    disambiguate_edges(&mut refs, topology);
    refs
}

/// Write the derived references into a tessellated shape.
///
/// The mesh arrives carrying geometric signatures (from the kernel backend);
/// this replaces them with the references the rebuild derived, and names the
/// edges from the two faces they bound.
pub fn apply_references(mesh: &mut TriangleMesh, topology: &ShapeTopology, refs: &RefMap) {
    // Match by geometry-derived signature, not by kernel id: the solid is
    // cloned to be tessellated and a clone can hand a face a fresh TShape, so
    // ids only agree by accident. Signatures are disambiguated and unique, and
    // the kernel id is still there as a fallback.
    for (face_id, face) in mesh.face_names.iter_mut() {
        let reference = refs
            .by_signature
            .get(&face.signature)
            .or_else(|| refs.by_id.get(face_id))
            .cloned();
        if let Some(reference) = reference {
            face.name = reference;
        }
    }

    let edge_refs = derive_edge_refs(topology, &refs.by_id);
    let edge_by_signature: BTreeMap<&str, &str> = topology
        .edges
        .iter()
        .filter_map(|(edge_id, edge)| {
            edge_refs
                .get(edge_id)
                .map(|reference| (edge.signature.as_str(), reference.as_str()))
        })
        .collect();
    // The faces each edge bounds, by signature for the same reason: the
    // selection queries (boundary, tangent chain, surface ring) work from these
    // references, never from kernel ids.
    let mut incident: BTreeMap<u64, Vec<&str>> = BTreeMap::new();
    for (face_id, face) in &topology.faces {
        if let Some(reference) = refs.by_id.get(face_id) {
            for edge_id in &face.edges {
                incident.entry(*edge_id).or_default().push(reference.as_str());
            }
        }
    }
    let mut faces_by_signature: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (edge_id, edge) in &topology.edges {
        let mut faces: Vec<String> = incident
            .get(edge_id)
            .map(|references| references.iter().map(|r| r.to_string()).collect())
            .unwrap_or_default();
        faces.sort();
        faces.dedup();
        faces_by_signature.insert(edge.signature.as_str(), faces);
    }

    for edge in mesh.edges.iter_mut() {
        let reference = edge_by_signature
            .get(edge.signature.as_str())
            .copied()
            .or_else(|| edge_refs.get(&edge.kernel_id).map(String::as_str));
        if let Some(reference) = reference {
            edge.name = reference.to_string();
        }
        if let Some(faces) = faces_by_signature.get(edge.signature.as_str()) {
            edge.faces = faces.clone();
        }
    }
}

/// What an edge set resolved to on a shape: the edges it found, and the
/// references it asked for that are no longer there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdgeSetResolution {
    pub found: Vec<String>,
    pub lost: Vec<String>,
}

/// Cosine of one degree between an edge and the reference axis, and its sine:
/// the tolerances of the vertical and horizontal edge sets.
const AXIS_PARALLEL: f64 = 0.99985;
const AXIS_PERPENDICULAR: f64 = 0.01745;

/// The edges an edge set asks for, on a shape with this topology, as edge
/// signatures.
///
/// Signatures, not kernel ids: the kernel hands out fresh ids when a shape is
/// copied for an operation, so an id resolved here would not match there. The
/// signature is the same geometry either way. `references` maps kernel edge id
/// to stable reference, the same map the mesh carries, and `axis` is the
/// direction the vertical or horizontal set refers to. A mode whose reference is
/// missing resolves to nothing rather than to everything: silently rounding
/// every edge is the failure that hurts.
pub fn resolve_edge_set(
    set: &EdgeSet,
    topology: &ShapeTopology,
    references: Option<&BTreeMap<u64, String>>,
    axis: Option<[f64; 3]>,
) -> EdgeSetResolution {
    // References can go missing; the rule modes cannot, they are re-evaluated.
    if let EdgeSet::References(names) = set {
        let mut found = Vec::new();
        let mut lost = Vec::new();
        for name in names {
            let edge_id = references.and_then(|map| {
                map.iter()
                    .find(|(_, reference)| *reference == name)
                    .map(|(edge_id, _)| *edge_id)
            });
            match edge_id.and_then(|edge_id| topology.edges.get(&edge_id)) {
                Some(edge) => found.push(edge.signature.clone()),
                None => lost.push(name.clone()),
            }
        }
        return EdgeSetResolution { found, lost };
    }

    let wanted: Vec<u64> = match set {
        EdgeSet::All => topology.edges.keys().copied().collect(),
        // Handled above, because it is the only mode that can lose one.
        EdgeSet::References(_) => Vec::new(),
        EdgeSet::Outer => topology
            .edges
            .iter()
            .filter(|(_, edge)| edge.is_outer())
            .map(|(edge_id, _)| *edge_id)
            .collect(),
        EdgeSet::Inner => topology
            .edges
            .iter()
            .filter(|(_, edge)| edge.is_inner())
            .map(|(edge_id, _)| *edge_id)
            .collect(),
        EdgeSet::Vertical { .. } => along_axis(topology, axis, true),
        EdgeSet::Horizontal { .. } => along_axis(topology, axis, false),
    };
    EdgeSetResolution {
        found: wanted
            .into_iter()
            .filter_map(|edge_id| topology.edges.get(&edge_id).map(|edge| edge.signature.clone()))
            .collect(),
        lost: Vec::new(),
    }
}

/// Edges parallel to the reference axis, or perpendicular to it, within one
/// degree. Perpendicular is what horizontal means, which is why that mode needs
/// the vertical reference as well.
fn along_axis(topology: &ShapeTopology, axis: Option<[f64; 3]>, parallel: bool) -> Vec<u64> {
    let Some(axis) = axis else {
        return Vec::new();
    };
    topology
        .edges
        .iter()
        .filter(|(_, edge)| {
            let direction = edge.direction();
            let dot = (axis[0] * direction[0] + axis[1] * direction[1] + axis[2] * direction[2])
                .abs();
            if parallel {
                dot >= AXIS_PARALLEL
            } else {
                dot <= AXIS_PERPENDICULAR
            }
        })
        .map(|(edge_id, _)| *edge_id)
        .collect()
}

/// Role of a face, when the feature definition says what its faces are.
fn role_of(feature: &Feature, face: &FaceInfo) -> Option<String> {
    let topology = ShapeTopology {
        faces: BTreeMap::from([(0, face.clone())]),
        ..Default::default()
    };
    role_refs(feature, &topology).remove(&0)
}

/// Role names for every face the feature can name from its own definition.
fn role_refs(feature: &Feature, topology: &ShapeTopology) -> BTreeMap<u64, String> {
    match &feature.kind {
        FeatureKind::Box { .. } => axis_roles(feature, topology, "box"),
        FeatureKind::Cylinder { .. } => cylinder_roles(feature, topology),
        FeatureKind::Extrude { .. } | FeatureKind::ExtrudeSurface { .. } => {
            extrusion_roles(feature, topology)
        }
        _ => BTreeMap::new(),
    }
}

fn axis_roles(
    feature: &Feature,
    topology: &ShapeTopology,
    family: &str,
) -> BTreeMap<u64, String> {
    let mut roles = BTreeMap::new();
    for (face_id, face) in &topology.faces {
        let Some(normal) = face.normal else {
            continue;
        };
        let mut best: Option<(&str, f64)> = None;
        for (axis, name) in [
            ([1.0, 0.0, 0.0], "x"),
            ([0.0, 1.0, 0.0], "y"),
            ([0.0, 0.0, 1.0], "z"),
        ] {
            let dot = normal[0] * axis[0] + normal[1] * axis[1] + normal[2] * axis[2];
            if dot.abs() > 0.99 && best.map_or(true, |(_, best)| dot.abs() > best.abs()) {
                best = Some((name, dot));
            }
        }
        if let Some((name, dot)) = best {
            let sign = if dot > 0.0 { "+" } else { "-" };
            roles.insert(*face_id, format!("{}/{family}/{name}{sign}", feature.id));
        }
    }
    roles
}

fn cylinder_roles(feature: &Feature, topology: &ShapeTopology) -> BTreeMap<u64, String> {
    let mut roles = BTreeMap::new();
    for (face_id, face) in &topology.faces {
        let role = match face.normal {
            // A cap lies in a plane perpendicular to the cylinder axis.
            Some(normal) if normal[2].abs() > 0.99 => {
                if normal[2] > 0.0 { "top" } else { "bottom" }
            }
            // The wall is the only curved face a cylinder produces.
            _ if face.kind.starts_with("cylinder") => "side",
            _ => continue,
        };
        roles.insert(*face_id, format!("{}/cylinder/{role}", feature.id));
    }
    roles
}

/// Caps by axis, then the sides in a deterministic order. A side keeps its
/// place under a height change; editing the profile can still relabel sides.
fn extrusion_roles(feature: &Feature, topology: &ShapeTopology) -> BTreeMap<u64, String> {
    let mut roles = axis_roles(feature, topology, "extrude");
    let mut sides: Vec<(u64, [f64; 3])> = topology
        .faces
        .iter()
        .filter(|(face_id, _)| !roles.contains_key(face_id))
        .map(|(face_id, face)| (*face_id, face.center))
        .collect();
    sides.sort_by(|a, b| {
        a.1[0]
            .partial_cmp(&b.1[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1[1].partial_cmp(&b.1[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    for (side, (face_id, _)) in sides.into_iter().enumerate() {
        roles.insert(face_id, format!("{}/extrude/side{}", feature.id, side + 1));
    }
    roles
}

/// Give duplicated references a unique suffix, ordered by geometry so the
/// assignment is reproducible. Two elements that still share a reference after
/// that are the same geometry and interchangeable.
fn disambiguate_faces(refs: &mut BTreeMap<u64, String>, topology: &ShapeTopology) {
    let mut groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for (face_id, reference) in refs.iter() {
        groups.entry(reference.clone()).or_default().push(*face_id);
    }
    for (reference, mut faces) in groups {
        if faces.len() < 2 {
            continue;
        }
        faces.sort_by(|a, b| point_order(face_center(topology, *a), face_center(topology, *b)));
        for (suffix, face_id) in faces.into_iter().enumerate() {
            refs.insert(face_id, format!("{reference}#{}", suffix + 1));
        }
    }
}

fn disambiguate_edges(refs: &mut BTreeMap<u64, String>, topology: &ShapeTopology) {
    let mut groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for (edge_id, reference) in refs.iter() {
        groups.entry(reference.clone()).or_default().push(*edge_id);
    }
    for (reference, mut edges) in groups {
        if edges.len() < 2 {
            continue;
        }
        edges.sort_by(|a, b| point_order(edge_midpoint(topology, *a), edge_midpoint(topology, *b)));
        for (suffix, edge_id) in edges.into_iter().enumerate() {
            refs.insert(edge_id, format!("{reference}#{}", suffix + 1));
        }
    }
}

fn face_center(topology: &ShapeTopology, face_id: u64) -> [f64; 3] {
    topology
        .faces
        .get(&face_id)
        .map(|face| face.center)
        .unwrap_or_default()
}

fn edge_midpoint(topology: &ShapeTopology, edge_id: u64) -> [f64; 3] {
    topology
        .edges
        .get(&edge_id)
        .map(|edge| edge.midpoint)
        .unwrap_or_default()
}

fn point_order(a: [f64; 3], b: [f64; 3]) -> std::cmp::Ordering {
    a[0].partial_cmp(&b[0])
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
        .then(a[2].partial_cmp(&b[2]).unwrap_or(std::cmp::Ordering::Equal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShapeCache;
    use rcad_doc::Part;
    use rcad_geom::NamedEdge;

    fn plane_feature(kind: FeatureKind) -> Feature {
        Feature::new("F", kind)
    }

    fn face(
        signature: &str,
        kind: &str,
        normal: Option<[f64; 3]>,
        center: [f64; 3],
        edges: Vec<u64>,
    ) -> FaceInfo {
        FaceInfo {
            signature: signature.to_string(),
            label: signature.to_string(),
            kind: kind.to_string(),
            area: 1.0,
            center,
            normal,
            edges,
        }
    }

    fn edge(kernel_id: u64, midpoint: [f64; 3], signature: &str) -> NamedEdge {
        NamedEdge {
            name: signature.to_string(),
            signature: signature.to_string(),
            kernel_id,
            midpoint,
            ..Default::default()
        }
    }

    /// Rebuild a part from scratch with the real kernel.
    fn rebuild(part: &Part) -> rcad_doc::RebuildResult {
        let mut backend = rcad_occt::OcctBackend::default();
        let mut cache = ShapeCache::new();
        cache.rebuild_part_incremental(part, &mut backend)
    }

    fn face_refs(result: &rcad_doc::RebuildResult) -> std::collections::BTreeSet<String> {
        result
            .display_mesh
            .face_names
            .values()
            .map(|face| face.name.clone())
            .collect()
    }

    fn edge_refs(result: &rcad_doc::RebuildResult) -> std::collections::BTreeSet<String> {
        result
            .display_mesh
            .edges
            .iter()
            .map(|edge| edge.name.clone())
            .collect()
    }

    fn box_feature(size: [f64; 3]) -> Feature {
        plane_feature(FeatureKind::Box { size })
    }

    // ----- pure derivation -------------------------------------------------

    #[test]
    fn a_primitive_names_its_faces_by_role_not_by_geometry() {
        let feature = box_feature([40.0, 30.0, 8.0]);
        let mut topology = ShapeTopology::default();
        topology.faces.insert(1, face("sig-x+", "plane", Some([1.0, 0.0, 0.0]), [40.0, 15.0, 4.0], vec![]));
        topology.faces.insert(2, face("sig-z+", "plane", Some([0.0, 0.0, 1.0]), [20.0, 15.0, 8.0], vec![]));
        topology.faces.insert(3, face("sig-other", "other", None, [0.0; 3], vec![]));

        let refs = derive_face_refs(&feature, &topology, None, None);

        assert_eq!(refs[&1], format!("{}/box/x+", feature.id));
        assert_eq!(refs[&2], format!("{}/box/z+", feature.id));
        assert_eq!(refs[&3], "sig-other", "a face with no role keeps its signature");
    }

    #[test]
    fn a_boolean_inherits_the_body_and_namespaces_the_tool() {
        let tool_id = uuid::Uuid::new_v4();
        let feature = plane_feature(FeatureKind::Boolean {
            tool_feature: tool_id,
            op: rcad_geom::BooleanOp::Subtract,
        });
        let mut topology = ShapeTopology::default();
        topology.faces.insert(1, face("sig-kept", "plane", None, [0.0; 3], vec![]));
        topology.faces.insert(2, face("sig-new", "cylinder(r5.0000)", None, [0.0; 3], vec![]));
        topology.face_history.insert(
            1,
            rcad_geom::FaceSource {
                source_signature: "body-signature".to_string(),
                from_tool: false,
            },
        );
        topology.face_history.insert(
            2,
            rcad_geom::FaceSource {
                source_signature: "tool-signature".to_string(),
                from_tool: true,
            },
        );
        let body = RefMap {
            by_id: BTreeMap::from([(100, "body/box/z+".to_string())]),
            by_signature: BTreeMap::from([(
                "body-signature".to_string(),
                "body/box/z+".to_string(),
            )]),
        };
        let tool = RefMap {
            by_id: BTreeMap::from([(200, "tool/cylinder/side".to_string())]),
            by_signature: BTreeMap::from([(
                "tool-signature".to_string(),
                "tool/cylinder/side".to_string(),
            )]),
        };

        let refs = derive_face_refs(&feature, &topology, Some(&body), Some(&tool));

        assert_eq!(refs[&1], "body/box/z+", "the body face keeps its reference");
        assert_eq!(
            refs[&2],
            format!("{}/tool/cylinder/side", feature.id),
            "a face cut from the tool belongs to this boolean"
        );
    }

    /// A translate keeps the kernel face ids, so the reference carries over by
    /// id even though the kernel records no history.
    #[test]
    fn a_face_keeps_its_reference_by_id_when_history_is_empty() {
        let feature = plane_feature(FeatureKind::Translate { delta: [10.0, 0.0, 0.0] });
        let mut topology = ShapeTopology::default();
        topology.faces.insert(7, face("sig-moved", "plane", None, [10.0, 0.0, 0.0], vec![]));
        let body = RefMap {
            by_id: BTreeMap::from([(7, "body/box/x+".to_string())]),
            by_signature: BTreeMap::new(),
        };

        let refs = derive_face_refs(&feature, &topology, Some(&body), None);

        assert_eq!(refs[&7], "body/box/x+");
    }

    #[test]
    fn an_edge_is_named_from_the_two_faces_it_bounds() {
        let mut topology = ShapeTopology::default();
        topology.faces.insert(1, face("f1", "plane", None, [0.0; 3], vec![10, 11]));
        topology.faces.insert(2, face("f2", "plane", None, [0.0; 3], vec![10, 12]));
        topology.edges.insert(10, edge(10, [0.0; 3], "sig10"));
        topology.edges.insert(11, edge(11, [0.0; 3], "sig11"));
        topology.edges.insert(12, edge(12, [0.0; 3], "sig12"));
        let mut faces = BTreeMap::new();
        faces.insert(1, "A".to_string());
        faces.insert(2, "B".to_string());

        let refs = derive_edge_refs(&topology, &faces);

        assert_eq!(refs[&10], "edge(A|B)");
        assert_eq!(refs[&11], "edge(A|seam)", "an edge on one face is a seam");
        assert_eq!(refs[&12], "edge(B|seam)");
    }

    #[test]
    fn two_edges_on_the_same_face_pair_get_distinct_references() {
        let mut topology = ShapeTopology::default();
        topology.faces.insert(1, face("f1", "plane", None, [0.0; 3], vec![10, 11]));
        topology.faces.insert(2, face("f2", "plane", None, [0.0; 3], vec![10, 11]));
        topology.edges.insert(10, edge(10, [0.0, 0.0, 0.0], "sig10"));
        topology.edges.insert(11, edge(11, [0.0, 0.0, 5.0], "sig11"));
        let mut faces = BTreeMap::new();
        faces.insert(1, "A".to_string());
        faces.insert(2, "B".to_string());

        let refs = derive_edge_refs(&topology, &faces);

        assert_ne!(refs[&10], refs[&11]);
        assert!(refs[&10].starts_with("edge(A|B)#"));
        assert!(refs[&11].starts_with("edge(A|B)#"));
    }


    // ----- through the real kernel -----------------------------------------

    /// The case that started this: change the height of a box and every face
    /// and every edge keeps its reference.
    #[test]
    fn a_box_keeps_its_face_and_edge_references_when_its_height_changes() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        let first = rebuild(&part);
        let first_faces = face_refs(&first);
        let first_edges = edge_refs(&first);

        assert_eq!(first_faces.len(), 6);
        assert_eq!(first_edges.len(), 12);
        assert!(first_faces.iter().all(|reference| reference.contains("/box/")));

        part.features[0].kind = FeatureKind::Box { size: [40.0, 30.0, 20.0] };
        let second = rebuild(&part);

        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert_eq!(first_faces, face_refs(&second), "face references moved");
        assert_eq!(first_edges, edge_refs(&second), "edge references moved");
    }

    /// A hole is a boolean: its wall comes from the cylinder tool, and the
    /// block faces keep their own references. All of them must survive a
    /// height change, because a fillet on one of those edges must too.
    #[test]
    fn a_drilled_block_keeps_its_references_when_the_height_changes() {
        let mut part = Part::new_solid("P");
        let base = box_feature([40.0, 30.0, 8.0]);
        let tool = plane_feature(FeatureKind::Cylinder { radius: 5.0, height: 20.0 });
        let tool_id = tool.id;
        part.features.push(base);
        part.features.push(tool);
        part.features.push(plane_feature(FeatureKind::Boolean {
            tool_feature: tool_id,
            op: rcad_geom::BooleanOp::Subtract,
        }));

        let first = rebuild(&part);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        let first_faces = face_refs(&first);
        let first_edges = edge_refs(&first);
        assert!(first_faces.iter().any(|reference| reference.contains("/box/")));
        assert!(
            first_faces.iter().any(|reference| reference.contains("/cylinder/side")),
            "the bore wall should be named from the tool: {first_faces:?}"
        );

        part.features[0].kind = FeatureKind::Box { size: [40.0, 30.0, 20.0] };
        let second = rebuild(&part);

        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert_eq!(first_faces, face_refs(&second), "face references moved");
        assert_eq!(first_edges, edge_refs(&second), "edge references moved");
    }

    /// A fillet rebuilds the topology but records the face history, so the
    /// original faces keep their references. The fillet surface itself is new
    /// and falls back to its signature.
    #[test]
    fn the_original_faces_keep_their_references_through_a_fillet() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        part.features.push(plane_feature(FeatureKind::Fillet {
            radius: 2.0,
            edges: Default::default(),
        }));

        let first = rebuild(&part);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        let box_refs: std::collections::BTreeSet<String> = face_refs(&first)
            .into_iter()
            .filter(|reference| reference.contains("/box/"))
            .collect();
        assert_eq!(box_refs.len(), 6, "the six original faces lost their names");

        part.features[0].kind = FeatureKind::Box { size: [40.0, 30.0, 20.0] };
        let second = rebuild(&part);
        let box_refs_after: std::collections::BTreeSet<String> = face_refs(&second)
            .into_iter()
            .filter(|reference| reference.contains("/box/"))
            .collect();

        assert_eq!(box_refs, box_refs_after, "a height change lost the box faces");
    }

    /// Every edge of a rebuilt box knows the two faces it bounds, which is what
    /// the selection queries work from.
    #[test]
    fn every_edge_knows_the_faces_it_bounds() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        let result = rebuild(&part);
        let mesh = &result.display_mesh;

        assert_eq!(mesh.edges.len(), 12);
        for edge in &mesh.edges {
            assert_eq!(edge.faces.len(), 2, "a box edge bounds two faces");
            assert!(edge.faces.iter().all(|face| face.contains("/box/")));
            assert!(
                edge.start_tangent.iter().any(|value| value.abs() > 0.5),
                "the tangents must come from the kernel"
            );
        }
    }

    /// The whole way round: kernel, references, query. The boundary of the top
    /// face is its four edges, and they are named as references.
    #[test]
    fn the_boundary_of_the_top_face_is_its_four_edges() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        let result = rebuild(&part);
        let mesh = &result.display_mesh;
        let top = mesh
            .face_names
            .values()
            .find(|face| face.name.contains("/box/z+"))
            .expect("the top face")
            .name
            .clone();

        let loops = rcad_geom::query::boundary_loops(mesh, &top);
        assert_eq!(loops.len(), 1, "a box face has one boundary loop");
        assert_eq!(loops[0].len(), 4);
        let references = rcad_geom::query::references(mesh, &loops[0]);
        assert!(
            references.iter().all(|reference| reference.starts_with("edge(")),
            "{references:?}"
        );
    }

    /// An extrude that carries its plane is swept along that plane normal, so a
    /// sketch drawn off the XY plane lands where it was drawn.
    #[test]
    fn an_extrude_with_a_placement_sweeps_along_its_own_normal() {
        let profile = crate::rect_profile(20.0, 10.0);
        let mut part = Part::new_solid("P");
        part.features.push(Feature::new(
            "E",
            FeatureKind::Extrude {
                profile,
                height: 5.0,
                placement: Some(rcad_doc::SketchPlacement {
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    x_direction: [1.0, 0.0, 0.0],
                    reference: None,
                }),
            },
        ));

        let result = rebuild(&part);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let positions = &result.display_mesh.positions;
        assert!(!positions.is_empty(), "the extrude produced no geometry");
        for position in positions {
            assert!(
                position[1] >= -1e-4 && position[1] <= 5.0 + 1e-4,
                "y {} is outside the sweep along the normal",
                position[1]
            );
            assert!(position[0].abs() <= 10.0 + 1e-4, "x {} outside the profile", position[0]);
            assert!(position[2].abs() <= 5.0 + 1e-4, "z {} outside the profile", position[2]);
        }
    }

    /// A round on the edges a rule names: the outer ones are the twelve of a box.
    #[test]
    fn a_round_on_the_outer_edges_matches_the_whole_box() {
        let mut by_rule = Part::new_solid("P");
        by_rule.features.push(box_feature([40.0, 30.0, 8.0]));
        by_rule.features.push(plane_feature(FeatureKind::Fillet {
            radius: 1.0,
            edges: EdgeSet::Outer,
        }));
        let by_rule = rebuild(&by_rule);
        assert!(by_rule.errors.is_empty(), "{:?}", by_rule.errors);

        let mut explicit = Part::new_solid("P");
        explicit.features.push(box_feature([40.0, 30.0, 8.0]));
        let all = rebuild(&explicit);
        let references: Vec<String> = all
            .display_mesh
            .edges
            .iter()
            .map(|edge| edge.name.clone())
            .collect();
        explicit.features.push(plane_feature(FeatureKind::Fillet {
            radius: 1.0,
            edges: EdgeSet::References(references),
        }));
        let explicit = rebuild(&explicit);

        assert_eq!(
            by_rule.display_mesh.face_names.len(),
            explicit.display_mesh.face_names.len(),
            "the outer set of a box is every edge of it"
        );
    }

    /// A reference that is no longer on the body is reported as lost, the rest of
    /// the picks still apply, and the round never widens to every edge.
    #[test]
    fn a_lost_reference_is_reported_without_widening_the_round() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        let boxed = rebuild(&part);
        let good = boxed.display_mesh.edges[0].name.clone();

        part.features.push(plane_feature(FeatureKind::Fillet {
            radius: 1.0,
            edges: EdgeSet::References(vec![good, "edge(gone|gone)".to_string()]),
        }));
        let partial = rebuild(&part);

        assert_eq!(partial.lost_references.len(), 1, "one reference went missing");
        assert_eq!(partial.lost_references[0].reference, "edge(gone|gone)");
        assert_eq!(partial.lost_references[0].feature_name, "F");
        assert!(partial.errors.is_empty(), "the rest still builds: {:?}", partial.errors);
        assert!(
            partial.display_mesh.face_names.len() > 6,
            "the pick that survived should still have been rounded"
        );
    }

    /// When every pick is gone the feature is skipped, not widened: rounding the
    /// whole body is the failure that hurts.
    #[test]
    fn a_round_with_no_surviving_reference_is_skipped() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        part.features.push(plane_feature(FeatureKind::Fillet {
            radius: 1.0,
            edges: EdgeSet::References(vec!["edge(gone|gone)".to_string()]),
        }));

        let result = rebuild(&part);

        assert_eq!(result.lost_references.len(), 1);
        assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
        assert!(result.errors[0].contains("none of its edge references"), "{:?}", result.errors);
        assert_eq!(
            result.display_mesh.face_names.len(),
            6,
            "the body is still the plain box"
        );
    }

    /// A revolve that carries its plane spins about that plane in-plane axis,
    /// instead of the world Z axis, which for a flat profile is degenerate.
    #[test]
    fn a_revolve_with_a_placement_spins_around_the_plane_axis() {
        // A profile offset from the axis, so the result is a ring and not a
        // profile that crosses its own axis of revolution.
        let profile = rcad_geom::Profile2D {
            outer: vec![[5.0, -5.0], [15.0, -5.0], [15.0, 5.0], [5.0, 5.0]],
            holes: Vec::new(),
        };
        let mut part = Part::new_solid("P");
        part.features.push(Feature::new(
            "R",
            FeatureKind::Revolve {
                profile,
                angle_deg: 360.0,
                placement: Some(rcad_doc::SketchPlacement {
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    x_direction: [1.0, 0.0, 0.0],
                    reference: None,
                }),
            },
        ));

        let result = rebuild(&part);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let mut spun_out = false;
        for position in &result.display_mesh.positions {
            let radius = (position[0] as f64).hypot(position[2] as f64);
            assert!(
                radius >= 5.0 - 1e-2 && radius <= 15.0 + 1e-2,
                "radius {radius} outside the profile"
            );
            assert!(position[1].abs() <= 5.0 + 1e-2, "y {} outside it", position[1]);
            if position[2].abs() > 10.0 {
                spun_out = true;
            }
        }
        assert!(spun_out, "the profile should have been spun around the Y axis");
    }

    /// The sketch plane follows the face it was drawn on: a profile placed by
    /// reference sits on that face and is swept from it, whatever numbers were
    /// stored when the sketch was made.
    #[test]
    fn a_profile_placed_by_reference_follows_the_face() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        let boxed = rebuild(&part);
        let top = boxed
            .display_mesh
            .face_names
            .values()
            .find(|face| face.name.contains("/box/z+"))
            .expect("the top face")
            .name
            .clone();

        part.features.push(Feature::new(
            "E",
            FeatureKind::Extrude {
                profile: rcad_geom::Profile2D {
                    outer: vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]],
                    holes: Vec::new(),
                },
                height: 4.0,
                placement: Some(rcad_doc::SketchPlacement {
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    x_direction: [1.0, 0.0, 0.0],
                    reference: Some(top.clone()),
                }),
            },
        ));

        let result = rebuild(&part);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(result.lost_references.is_empty(), "{:?}", result.lost_references);
        let zs: Vec<f32> = result.display_mesh.positions.iter().map(|p| p[2]).collect();
        let lowest = zs.iter().cloned().fold(f32::INFINITY, f32::min);
        let highest = zs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!((lowest - 8.0).abs() < 1e-2, "sits on the face: {lowest}");
        assert!(highest >= 12.0 - 1e-2, "swept from it: {highest}");
    }

    /// A plane reference that is gone is reported, and the last known numbers are
    /// used instead of the profile flying off to the origin.
    #[test]
    fn a_placed_profile_reports_a_reference_that_is_gone() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        part.features.push(Feature::new(
            "E",
            FeatureKind::Extrude {
                profile: rcad_geom::Profile2D {
                    outer: vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]],
                    holes: Vec::new(),
                },
                height: 4.0,
                placement: Some(rcad_doc::SketchPlacement {
                    origin: [0.0, 0.0, 20.0],
                    normal: [0.0, 0.0, 1.0],
                    x_direction: [1.0, 0.0, 0.0],
                    reference: Some("face(never|seen)".to_string()),
                }),
            },
        ));

        let result = rebuild(&part);
        assert_eq!(result.lost_references.len(), 1);
        assert_eq!(result.lost_references[0].reference, "face(never|seen)");
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let highest = result
            .display_mesh
            .positions
            .iter()
            .map(|p| p[2])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(highest >= 24.0 - 1e-2, "the stored plane is still used: {highest}");
    }

    fn test_edge(
        kernel_id: u64,
        signature: &str,
        start: [f64; 3],
        end: [f64; 3],
        dihedral_degrees: f64,
    ) -> NamedEdge {
        let d = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let unit = [d[0] / length, d[1] / length, d[2] / length];
        NamedEdge {
            name: signature.to_string(),
            signature: signature.to_string(),
            label: signature.to_string(),
            kernel_id,
            start,
            end,
            start_tangent: unit,
            end_tangent: unit,
            midpoint: [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5, 0.0],
            length,
            dihedral_degrees,
            ..Default::default()
        }
    }

    /// The rule modes of an edge set, on a topology with one edge of each kind:
    /// an outer edge along X, an inner edge along X and an outer edge along Z.
    #[test]
    fn an_edge_set_resolves_by_rule() {
        let mut topology = ShapeTopology::default();
        topology.edges.insert(
            1,
            test_edge(1, "outer", [0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 90.0),
        );
        topology.edges.insert(
            2,
            test_edge(2, "inner", [0.0, 5.0, 0.0], [10.0, 5.0, 0.0], 270.0),
        );
        topology.edges.insert(
            3,
            test_edge(3, "along-z", [20.0, 0.0, 0.0], [20.0, 0.0, 10.0], 90.0),
        );
        let references = BTreeMap::from([
            (1, "outer".to_string()),
            (2, "inner".to_string()),
            (3, "along-z".to_string()),
        ]);
        let axis = Some([0.0, 0.0, 1.0]);
        let resolve = |set: &EdgeSet| {
            resolve_edge_set(set, &topology, Some(&references), axis).found
        };
        let pick = |name: &str| name.to_string();

        assert_eq!(resolve(&EdgeSet::All).len(), 3);
        assert_eq!(
            resolve(&EdgeSet::Outer),
            vec![pick("outer"), pick("along-z")]
        );
        assert_eq!(resolve(&EdgeSet::Inner), vec![pick("inner")]);
        assert_eq!(
            resolve(&EdgeSet::Vertical {
                axis: uuid::Uuid::new_v4()
            }),
            vec![pick("along-z")]
        );
        assert_eq!(
            resolve(&EdgeSet::Horizontal {
                axis: uuid::Uuid::new_v4()
            }),
            vec![pick("outer"), pick("inner")]
        );
        assert_eq!(
            resolve(&EdgeSet::References(vec![pick("inner")])),
            vec![pick("inner")]
        );

        // The modes that need a reference axis select nothing without one:
        // quietly rounding every edge is the failure that hurts.
        for set in [
            EdgeSet::Vertical {
                axis: uuid::Uuid::new_v4(),
            },
            EdgeSet::Horizontal {
                axis: uuid::Uuid::new_v4(),
            },
        ] {
            assert!(resolve_edge_set(&set, &topology, Some(&references), None)
                .found
                .is_empty());
        }
    }

    /// A fillet on picked edges rounds those edges and only those, which is the
    /// whole point of an edge set: a couple of faces, not one per edge.
    #[test]
    fn a_fillet_by_reference_rounds_only_the_picked_edges() {
        let mut part = Part::new_solid("P");
        part.features.push(box_feature([40.0, 30.0, 8.0]));
        let first = rebuild(&part);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        let picked: Vec<String> = first
            .display_mesh
            .edges
            .iter()
            .take(2)
            .map(|edge| edge.name.clone())
            .collect();
        assert_eq!(picked.len(), 2);

        part.features.push(plane_feature(FeatureKind::Fillet {
            radius: 1.0,
            edges: EdgeSet::References(picked),
        }));
        let second = rebuild(&part);
        assert!(second.errors.is_empty(), "{:?}", second.errors);
        let by_reference = second.display_mesh.face_names.len();
        assert!(by_reference > 6, "the picked edges should have been rounded");

        part.features[1].kind = FeatureKind::Fillet {
            radius: 1.0,
            edges: EdgeSet::All,
        };
        let all = rebuild(&part);
        assert!(all.errors.is_empty(), "{:?}", all.errors);
        assert!(
            all.display_mesh.face_names.len() > by_reference,
            "every edge should add more faces than two: {} vs {}",
            all.display_mesh.face_names.len(),
            by_reference
        );
    }
}
