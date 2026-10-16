//! Selection queries over the topology the rebuild produced.
//!
//! These are the Creo smart-selection queries: the boundary loop of a face, the
//! tangent-continuous chain through an edge, every edge of a surface. They work
//! only on the edge geometry and the face references carried by the mesh, never
//! on kernel ids, so the result goes straight into a feature reference.

use glam::Vec3;

use crate::TriangleMesh;

/// Distance below which two edge endpoints are the same vertex, in mm.
pub const VERTEX_TOLERANCE: f64 = 1e-6;

/// Cosine gap allowed between two tangents that count as continuous: 1 minus
/// the cosine of one degree.
pub const TANGENT_TOLERANCE: f32 = 0.00015;

/// Indices into the mesh edges of the edges bounding a face, in mesh order.
pub fn edges_of_face(mesh: &TriangleMesh, face_reference: &str) -> Vec<usize> {
    mesh.edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.faces.iter().any(|face| face == face_reference))
        .map(|(index, _)| index)
        .collect()
}

/// The loops of edges bounding a face, longest first.
///
/// The first loop is the outer boundary and the rest are inner wires, which is
/// how a face with a hole reads.
pub fn boundary_loops(mesh: &TriangleMesh, face_reference: &str) -> Vec<Vec<usize>> {
    chain_loops(mesh, &edges_of_face(mesh, face_reference))
}

/// Chains edge indices into closed loops by shared endpoints.
pub fn chain_loops(mesh: &TriangleMesh, edges: &[usize]) -> Vec<Vec<usize>> {
    let mut remaining: Vec<usize> = edges.to_vec();
    let mut loops: Vec<Vec<usize>> = Vec::new();
    while let Some(seed) = remaining.pop() {
        let start = mesh.edges[seed].start;
        let mut tip = mesh.edges[seed].end;
        let mut chain = vec![seed];
        loop {
            let next = remaining.iter().position(|index| {
                let edge = &mesh.edges[*index];
                touches(edge.start, tip) || touches(edge.end, tip)
            });
            let Some(position) = next else { break };
            let index = remaining.remove(position);
            let edge = &mesh.edges[index];
            tip = if touches(edge.start, tip) { edge.end } else { edge.start };
            chain.push(index);
            if touches(tip, start) {
                break;
            }
        }
        if chain.len() > 1 {
            loops.push(chain);
        }
    }
    loops.sort_by(|a, b| loop_length(mesh, b).total_cmp(&loop_length(mesh, a)));
    loops
}

/// Every edge whose direction continues the seed edge through a shared vertex,
/// walking both ways from the seed.
///
/// The walk stops where the next edge meets at an angle, which is what makes
/// the tangent chain of a boss different from the whole outline of the part.
pub fn tangent_chain(mesh: &TriangleMesh, seed: usize, tolerance: f32) -> Vec<usize> {
    let mut chain = vec![seed];
    for forward in [true, false] {
        let seed_edge = &mesh.edges[seed];
        let mut tip = if forward { seed_edge.end } else { seed_edge.start };
        let mut direction = if forward {
            to_vec3(seed_edge.end_tangent)
        } else {
            -to_vec3(seed_edge.start_tangent)
        };
        loop {
            let mut found: Option<(usize, [f64; 3], Vec3)> = None;
            for (index, edge) in mesh.edges.iter().enumerate() {
                if chain.contains(&index) {
                    continue;
                }
                if touches(edge.start, tip) && continues(direction, edge.start_tangent, tolerance) {
                    found = Some((index, edge.end, to_vec3(edge.end_tangent)));
                    break;
                }
                if touches(edge.end, tip) && continues(direction, negate(edge.end_tangent), tolerance) {
                    found = Some((index, edge.start, -to_vec3(edge.start_tangent)));
                    break;
                }
            }
            let Some((index, next_tip, next_direction)) = found else { break };
            chain.push(index);
            tip = next_tip;
            direction = next_direction;
        }
    }
    chain
}

/// Every edge of the faces the seed edge belongs to.
pub fn surface_ring(mesh: &TriangleMesh, seed: usize) -> Vec<usize> {
    let faces = &mesh.edges[seed].faces;
    mesh.edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.faces.iter().any(|face| faces.contains(face)))
        .map(|(index, _)| index)
        .collect()
}

/// The stable references of a set of edge indices, in the given order.
pub fn references(mesh: &TriangleMesh, edges: &[usize]) -> Vec<String> {
    edges
        .iter()
        .map(|index| mesh.edges[*index].name.clone())
        .collect()
}

fn touches(a: [f64; 3], b: [f64; 3]) -> bool {
    let distance = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    distance <= VERTEX_TOLERANCE
}

fn negate(v: [f64; 3]) -> [f64; 3] {
    [-v[0], -v[1], -v[2]]
}

/// Edge geometry is stored as f64; the viewport math is f32.
fn to_vec3(v: [f64; 3]) -> Vec3 {
    Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)
}

/// Whether travelling in the given direction continues into the tangent.
fn continues(direction: Vec3, tangent: [f64; 3], tolerance: f32) -> bool {
    let a = direction.normalize_or_zero();
    let b = to_vec3(tangent).normalize_or_zero();
    a.dot(b) >= 1.0 - tolerance
}

fn loop_length(mesh: &TriangleMesh, chain: &[usize]) -> f64 {
    chain.iter().map(|index| mesh.edges[*index].length).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NamedEdge;

    /// A closed square in the XY plane: four edges, one loop.
    fn square(side: f64, face: &str, offset: [f64; 2]) -> Vec<NamedEdge> {
        let (ox, oy) = (offset[0], offset[1]);
        let corners = [
            [ox, oy, 0.0],
            [ox + side, oy, 0.0],
            [ox + side, oy + side, 0.0],
            [ox, oy + side, 0.0],
        ];
        (0..4)
            .map(|index| {
                let start = corners[index];
                let end = corners[(index + 1) % 4];
                let tangent = [(end[0] - start[0]) / side, (end[1] - start[1]) / side, 0.0];
                NamedEdge {
                    name: format!("edge{index}@{face}"),
                    signature: format!("sig{index}@{face}"),
                    label: format!("edge {index}"),
                    start,
                    end,
                    start_tangent: tangent,
                    end_tangent: tangent,
                    midpoint: [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5, 0.0],
                    closed: true,
                    length: side,
                    faces: vec![face.to_string()],
                    ..Default::default()
                }
            })
            .collect()
    }

    fn mesh_with(edges: Vec<NamedEdge>) -> TriangleMesh {
        TriangleMesh {
            edges,
            ..Default::default()
        }
    }

    fn segment(name: &str, start: [f64; 3], end: [f64; 3], face: &str) -> NamedEdge {
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let dz = end[2] - start[2];
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        let tangent = [dx / length, dy / length, dz / length];
        NamedEdge {
            name: name.to_string(),
            signature: name.to_string(),
            start,
            end,
            start_tangent: tangent,
            end_tangent: tangent,
            length,
            faces: vec![face.to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn a_face_boundary_is_one_closed_loop() {
        let mesh = mesh_with(square(10.0, "A", [0.0, 0.0]));
        let loops = boundary_loops(&mesh, "A");

        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 4, "the four edges chain into one loop");
        assert!(boundary_loops(&mesh, "B").is_empty());
    }

    /// A face with a hole has two loops, and the outer one comes first.
    #[test]
    fn an_inner_wire_is_a_second_loop() {
        let mut edges = square(10.0, "A", [0.0, 0.0]);
        edges.extend(square(2.0, "A", [4.0, 4.0]));
        let mesh = mesh_with(edges);

        let loops = boundary_loops(&mesh, "A");
        assert_eq!(loops.len(), 2);
        assert_eq!(loops[0].len(), 4);
        assert_eq!(loops[1].len(), 4);
        assert!(
            loop_length(&mesh, &loops[0]) > loop_length(&mesh, &loops[1]),
            "the outer loop is the longer one"
        );
    }

    #[test]
    fn a_tangent_chain_follows_straight_edges_and_stops_at_a_corner() {
        let mesh = mesh_with(vec![
            segment("a", [0.0, 0.0, 0.0], [5.0, 0.0, 0.0], "A"),
            segment("b", [5.0, 0.0, 0.0], [10.0, 0.0, 0.0], "A"),
            segment("c", [10.0, 0.0, 0.0], [10.0, 5.0, 0.0], "A"),
        ]);

        let chain = tangent_chain(&mesh, 0, TANGENT_TOLERANCE);
        assert_eq!(chain.len(), 2, "the collinear neighbour joins: {chain:?}");
        assert!(chain.contains(&0) && chain.contains(&1));
        assert!(!chain.contains(&2), "the right angle must stop the chain");
    }

    #[test]
    fn a_surface_ring_covers_the_faces_of_the_seed_only() {
        let mut edges = square(10.0, "A", [0.0, 0.0]);
        edges[0].faces = vec!["A".to_string(), "B".to_string()];
        edges.extend(square(2.0, "B", [4.0, 4.0]));
        edges.push(segment("far", [50.0, 0.0, 0.0], [55.0, 0.0, 0.0], "C"));
        let mesh = mesh_with(edges);

        let ring = surface_ring(&mesh, 0);
        assert_eq!(ring.len(), 8, "both faces of the seed edge: {ring:?}");
        assert!(!ring.contains(&8), "an unrelated face is not in the ring");
        assert!(references(&mesh, &ring).iter().all(|name| !name.starts_with("far")));
    }
}
