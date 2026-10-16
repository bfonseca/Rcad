//! Stable, geometry-derived names for topology faces and edges.
//!
//! cadrum identifies a face or an edge by the address of its TopoDS_TShape.
//! That id is process-local: the same face gets a different id in every run,
//! so nothing can be saved against it. The names built here come from the
//! geometry itself, rounded to a fixed grid, so the same shape produces the
//! same name in every process, on every machine, and after a save/reload.
//!
//! A name is stable while the element geometry is unchanged. Move or resize
//! that face and its name changes, which is the correct behaviour: a stored
//! reference no longer points at the same geometry. Callers detect it by
//! looking the name up in a freshly built mesh and finding nothing - the
//! colour is then reported as unmatched instead of being drawn on the wrong
//! face.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::TriangleMesh;

/// Grid the face and edge signatures are rounded to, in millimetres. Also the
/// smallest difference in a length, area or position the names can express.
pub const NAME_QUANTUM: f64 = 1e-4;

/// Directions are rounded more coarsely than positions. They come from surface
/// and curve evaluation, where a 1e-4 threshold would be noise.
pub const DIRECTION_QUANTUM: f64 = 1e-3;

/// A topology face with its stable key and a label for the interface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedFace {
    /// Key used to look the face up. The rebuild replaces it with the
    /// feature-anchored reference; on its own it is the geometric signature.
    pub name: String,
    /// Geometry-derived signature. Always present, and the fallback the
    /// rebuild falls back to when no feature can claim the face.
    #[serde(default)]
    pub signature: String,
    /// Human-readable description, for menus and the status bar.
    #[serde(default)]
    pub label: String,
}

/// A topology edge with its stable key, geometry and a polyline.
///
/// The polyline is the display/picking approximation; start, end, midpoint and
/// length are what the name is built from.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NamedEdge {
    /// Key used to look the edge up. The rebuild replaces it with the
    /// feature-anchored reference; on its own it is the geometric signature.
    pub name: String,
    /// Geometry-derived signature, kept as the fallback name.
    #[serde(default)]
    pub signature: String,
    /// Human-readable description, for menus and the status bar.
    #[serde(default)]
    pub label: String,
    /// Backend-local edge id. Only meaningful inside one process; kept for
    /// matching against the live kernel shape.
    #[serde(default)]
    pub kernel_id: u64,
    /// Exact curve endpoints, in the curve's native direction. The name
    /// canonicalises them itself, so the order here only matters for pairing
    /// with the tangents below.
    pub start: [f64; 3],
    pub end: [f64; 3],
    /// Tangent at `start`, pointing from start towards end.
    #[serde(default)]
    pub start_tangent: [f64; 3],
    /// Tangent at `end`, pointing from start towards end.
    #[serde(default)]
    pub end_tangent: [f64; 3],
    pub midpoint: [f64; 3],
    pub length: f64,
    pub closed: bool,
    /// Stable references of the faces this edge bounds: two for a normal edge,
    /// one for a seam. Filled by the rebuild; empty on construction geometry.
    #[serde(default)]
    pub faces: Vec<String>,
    /// Angle through the material at this edge, in degrees. Below 180 is a
    /// convex (outer) edge, above 180 a concave (inner) one, and 0 means the
    /// backend did not say - which keeps such an edge out of either filter.
    #[serde(default)]
    pub dihedral_degrees: f64,
    /// Polyline approximation, for drawing and picking.
    #[serde(default)]
    pub points: Vec<[f32; 3]>,
}

impl NamedEdge {
    /// Whether this edge carries the given stable name.
    pub fn matches(&self, name: &str) -> bool {
        self.name == name
    }

    /// A convex edge: the material angle is below 180 degrees. Creo calls these
    /// the outer edges.
    pub fn is_outer(&self) -> bool {
        self.dihedral_degrees > 0.0 && self.dihedral_degrees < 180.0
    }

    /// A concave edge: the material angle is above 180 degrees.
    pub fn is_inner(&self) -> bool {
        self.dihedral_degrees > 180.0
    }

    /// Unit direction from start to end, falling back to the start tangent on a
    /// degenerate edge.
    pub fn direction(&self) -> [f64; 3] {
        let d = [
            self.end[0] - self.start[0],
            self.end[1] - self.start[1],
            self.end[2] - self.start[2],
        ];
        let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if length > 1e-12 {
            [d[0] / length, d[1] / length, d[2] / length]
        } else {
            self.start_tangent
        }
    }
}

/// The facts a face name is built from. The kernel supplies them; the naming
/// itself is pure, so it is tested without OpenCascade.
#[derive(Debug, Clone)]
pub struct FaceFacts<'a> {
    /// Surface kind and parameters, e.g. 'plane' or 'cylinder(r5.0000)'.
    pub kind: &'a str,
    /// Trimmed area, in square millimetres.
    pub area: f64,
    /// Area-weighted centre.
    pub center: [f64; 3],
    /// Outward normal, when the surface defines one (planar faces).
    pub normal: Option<[f64; 3]>,
    /// Number of boundary edges, including inner wires.
    pub edge_count: usize,
    /// Sum of the boundary edge lengths.
    pub perimeter: f64,
}

/// The facts an edge name is built from.
#[derive(Debug, Clone)]
pub struct EdgeFacts<'a> {
    /// 'line', 'circle' or 'curve'.
    pub kind: &'a str,
    pub length: f64,
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub midpoint: [f64; 3],
    pub closed: bool,
}

/// Round a length, area or coordinate to the naming grid.
///
/// Negative zero is folded into zero, so the same point always formats the
/// same way.
pub fn quantised(value: f64) -> f64 {
    quantise(value, NAME_QUANTUM)
}

fn quantise(value: f64, quantum: f64) -> f64 {
    let scaled = (value / quantum).round();
    if scaled == 0.0 {
        0.0
    } else {
        scaled * quantum
    }
}

/// Format a length on the naming grid, e.g. '40.0000'.
pub fn length_text(value: f64) -> String {
    format!("{:.4}", quantised(value))
}

fn point_text(point: [f64; 3]) -> String {
    format!(
        "{:.4},{:.4},{:.4}",
        quantised(point[0]),
        quantised(point[1]),
        quantised(point[2])
    )
}

fn direction_text(direction: [f64; 3]) -> String {
    let length = (direction[0] * direction[0]
        + direction[1] * direction[1]
        + direction[2] * direction[2])
        .sqrt();
    let unit = if length > 0.0 {
        [direction[0] / length, direction[1] / length, direction[2] / length]
    } else {
        [0.0; 3]
    };
    let component = |value: f64| quantise(value, DIRECTION_QUANTUM);
    format!(
        "{:.3},{:.3},{:.3}",
        component(unit[0]),
        component(unit[1]),
        component(unit[2])
    )
}

fn point_key(point: [f64; 3]) -> (f64, f64, f64) {
    (quantised(point[0]), quantised(point[1]), quantised(point[2]))
}

fn lexicographic(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// Endpoints in a fixed order, so an edge name does not depend on the curve
/// native parameter direction.
fn ordered_endpoints(start: [f64; 3], end: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let (a, b) = (point_key(start), point_key(end));
    let order = lexicographic(a.0, b.0)
        .then(lexicographic(a.1, b.1))
        .then(lexicographic(a.2, b.2));
    match order {
        std::cmp::Ordering::Greater => (end, start),
        _ => (start, end),
    }
}

/// The stable name of a face.
pub fn face_signature(facts: &FaceFacts) -> String {
    let mut name = format!(
        "F:{}:a{}:c{}",
        facts.kind,
        length_text(facts.area),
        point_text(facts.center)
    );
    if let Some(normal) = facts.normal {
        name.push_str(":n");
        name.push_str(&direction_text(normal));
    }
    name.push_str(&format!(
        ":e{}:p{}",
        facts.edge_count,
        length_text(facts.perimeter)
    ));
    name
}

/// A short human description of a face.
pub fn face_label(facts: &FaceFacts) -> String {
    format!(
        "{}, {:.1} mm2 at ({:.1}, {:.1}, {:.1})",
        facts.kind, facts.area, facts.center[0], facts.center[1], facts.center[2]
    )
}

/// The stable name of an edge.
pub fn edge_signature(facts: &EdgeFacts) -> String {
    let (start, end) = ordered_endpoints(facts.start, facts.end);
    format!(
        "E:{}:l{}:s{}:t{}:m{}",
        facts.kind,
        length_text(facts.length),
        point_text(start),
        point_text(end),
        point_text(facts.midpoint)
    )
}

/// A short human description of an edge.
pub fn edge_label(facts: &EdgeFacts) -> String {
    let (start, end) = ordered_endpoints(facts.start, facts.end);
    if facts.closed {
        format!("{} edge, {:.1} mm, closed loop", facts.kind, facts.length)
    } else {
        format!(
            "{} edge, {:.1} mm from ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1})",
            facts.kind,
            facts.length,
            start[0], start[1], start[2],
            end[0], end[1], end[2]
        )
    }
}

/// Give every duplicated face name a unique suffix.
///
/// Names are geometric signatures, so two faces sharing a name are the same
/// geometry (two identical holes, for instance). The suffix keeps the map keys
/// unique, and the label is suffixed too so the interface can tell them apart.
/// Assigning the suffixes between two *identical* faces is arbitrary, on
/// purpose: they are interchangeable, and the set of names does not change.
pub fn disambiguate_faces(faces: &mut [NamedFace]) {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, face) in faces.iter().enumerate() {
        groups.entry(face.name.clone()).or_default().push(index);
    }
    for (name, indices) in groups {
        if indices.len() < 2 {
            continue;
        }
        for (suffix, index) in indices.into_iter().enumerate() {
            let label = faces[index].label.clone();
            faces[index].name = format!("{name}#{}", suffix + 1);
            faces[index].label = format!("{label} #{}", suffix + 1);
        }
    }
}

/// Give every duplicated edge name a unique suffix. See disambiguate_faces.
pub fn disambiguate_edges(edges: &mut [NamedEdge]) {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, edge) in edges.iter().enumerate() {
        groups.entry(edge.name.clone()).or_default().push(index);
    }
    for (name, indices) in groups {
        if indices.len() < 2 {
            continue;
        }
        for (suffix, index) in indices.into_iter().enumerate() {
            let label = edges[index].label.clone();
            edges[index].name = format!("{name}#{}", suffix + 1);
            edges[index].label = format!("{label} #{}", suffix + 1);
        }
    }
}


/// What is known about a reference from the last build that had it: enough for a
/// repair list to tell the user which one went missing and where it used to be.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReferenceMemory {
    pub reference: String,
    /// "edge" or "face".
    pub kind: String,
    /// Midpoint of an edge, in mm.
    pub location: [f64; 3],
    /// Length of the edge, in mm.
    pub size: f64,
    /// The faces it bounded, by reference.
    pub neighbours: Vec<String>,
    pub label: String,
}

impl ReferenceMemory {
    /// One sentence for the repair list.
    pub fn describe(&self) -> String {
        format!(
            "{} {} of {} mm at ({}, {}, {})",
            self.label,
            self.kind,
            quantised(self.size),
            quantised(self.location[0]),
            quantised(self.location[1]),
            quantised(self.location[2]),
        )
    }
}

/// Two perpendicular unit vectors spanning the plane with this normal, the
/// first as close to the wanted x direction as the plane allows.
///
/// The app and the rebuild both need it: the app to draw the sketch grid, the
/// rebuild to place a profile on the plane a reference resolves to.
pub fn plane_basis(normal: [f64; 3], wanted_x: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if length <= 1e-12 {
        return ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    }
    let n = [normal[0] / length, normal[1] / length, normal[2] / length];
    let dot = wanted_x[0] * n[0] + wanted_x[1] * n[1] + wanted_x[2] * n[2];
    let mut u = [
        wanted_x[0] - n[0] * dot,
        wanted_x[1] - n[1] * dot,
        wanted_x[2] - n[2] * dot,
    ];
    let mut u_length = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    if u_length < 1e-6 {
        // The wanted direction is along the normal: any perpendicular will do.
        let fallback = if n[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let dot = fallback[0] * n[0] + fallback[1] * n[1] + fallback[2] * n[2];
        u = [
            fallback[0] - n[0] * dot,
            fallback[1] - n[1] * dot,
            fallback[2] - n[2] * dot,
        ];
        u_length = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    }
    let u = [u[0] / u_length, u[1] / u_length, u[2] / u_length];
    let v = [
        n[1] * u[2] - n[2] * u[1],
        n[2] * u[0] - n[0] * u[2],
        n[0] * u[1] - n[1] * u[0],
    ];
    (u, v)
}

/// Remember every named edge of a mesh, by reference.
///
/// References are stable names, so this survives a rebuild: a reference that
/// disappears keeps its last entry, which is what the repair list needs to say
/// where it was.
pub fn remember(mesh: &TriangleMesh) -> BTreeMap<String, ReferenceMemory> {
    mesh.edges
        .iter()
        .map(|edge| {
            (
                edge.name.clone(),
                ReferenceMemory {
                    reference: edge.name.clone(),
                    kind: "edge".to_string(),
                    location: edge.midpoint,
                    size: edge.length,
                    neighbours: edge.faces.clone(),
                    label: edge.label.clone(),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(area: f64, center: [f64; 3]) -> FaceFacts<'static> {
        FaceFacts {
            kind: "plane",
            area,
            center,
            normal: Some([0.0, 0.0, 1.0]),
            edge_count: 4,
            perimeter: 140.0,
        }
    }

    fn edge(start: [f64; 3], end: [f64; 3], midpoint: [f64; 3]) -> EdgeFacts<'static> {
        EdgeFacts {
            kind: "line",
            length: 40.0,
            start,
            end,
            midpoint,
            closed: false,
        }
    }

    #[test]
    fn quantisation_folds_negative_zero_and_rounds_to_the_grid() {
        assert_eq!(length_text(-0.0), "0.0000");
        assert_eq!(length_text(0.0), "0.0000");
        assert_eq!(length_text(1.00004), "1.0000");
        assert_eq!(length_text(1.00006), "1.0001");
    }

    /// The whole point: the name follows the geometry, not the address, so it
    /// is identical in another process.
    #[test]
    fn the_same_geometry_always_gives_the_same_name() {
        let a = face_signature(&face(1200.0, [20.0, 15.0, 8.0]));
        let b = face_signature(&face(1200.0, [20.0, 15.0, 8.0]));
        assert_eq!(a, b);
        assert!(a.starts_with("F:plane:a1200.0000:c20.0000,15.0000,8.0000"));
    }

    #[test]
    fn different_geometry_gives_a_different_name() {
        let a = face_signature(&face(1200.0, [20.0, 15.0, 8.0]));
        let moved = face_signature(&face(1200.0, [20.0, 15.0, 10.0]));
        let resized = face_signature(&face(1201.0, [20.0, 15.0, 8.0]));
        assert_ne!(a, moved);
        assert_ne!(a, resized);
    }

    /// One run may evaluate a normal as 0.9999996 and the next as 1.0; the
    /// coarser direction grid must absorb that.
    #[test]
    fn direction_noise_is_absorbed() {
        let mut noisy = face(10.0, [0.0, 0.0, 0.0]);
        noisy.normal = Some([1e-7, -2e-7, 1.0]);
        let clean = face(10.0, [0.0, 0.0, 0.0]);
        assert_eq!(face_signature(&noisy), face_signature(&clean));
    }

    /// Edges have no inherent direction; the name must not depend on the
    /// curve parameter direction OCCT happens to choose.
    #[test]
    fn an_edge_name_does_not_depend_on_the_endpoint_order() {
        let forward = edge([0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
        let backward = edge([40.0, 0.0, 0.0], [0.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
        assert_eq!(edge_signature(&forward), edge_signature(&backward));
        assert!(edge_signature(&forward).starts_with("E:line:l40.0000") );
    }

    #[test]
    fn duplicate_names_get_a_unique_suffix() {
        let base = face_signature(&face(50.0, [0.0, 0.0, 0.0]));
        let mut faces = vec![
            NamedFace { name: base.clone(), label: "hole".to_string(), ..Default::default() },
            NamedFace { name: base.clone(), label: "hole".to_string(), ..Default::default() },
            NamedFace { name: "F:unique".to_string(), label: "other".to_string(), ..Default::default() },
        ];
        disambiguate_faces(&mut faces);
        assert_eq!(faces[0].name, format!("{base}#1"));
        assert_eq!(faces[1].name, format!("{base}#2"));
        assert_eq!(faces[2].name, "F:unique");
        assert_eq!(faces[0].label, "hole #1");
        let unique: std::collections::BTreeSet<String> =
            faces.iter().map(|face| face.name.clone()).collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn duplicate_edge_names_get_a_unique_suffix() {
        let base = edge_signature(&edge([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.0, 0.0]));
        let mut edges = vec![
            NamedEdge { name: base.clone(), label: "edge".to_string(), ..Default::default() },
            NamedEdge { name: base.clone(), label: "edge".to_string(), ..Default::default() },
        ];
        disambiguate_edges(&mut edges);
        assert_eq!(edges[0].name, format!("{base}#1"));
        assert_eq!(edges[1].name, format!("{base}#2"));
    }
}
