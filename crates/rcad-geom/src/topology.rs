//! Geometric facts the rebuild needs to name a shape topology.
//!
//! A kernel face id is an address, so it cannot be persisted. The rebuild
//! needs more than the tessellation to fix that: for every face it needs the
//! geometric signature (the fallback name), the facts that let a feature
//! recognise its own faces by role, the edges the face bounds, and the face
//! derivation the last kernel operation recorded.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::naming::NamedEdge;

/// Facts about one face, and the edges it bounds.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FaceInfo {
    /// Geometry-derived signature: the fallback name when no feature can claim
    /// the face. Already disambiguated, so it is unique within the shape.
    pub signature: String,
    /// Human description, e.g. "plane, 1200.0 mm2 at (20.0, 15.0, 8.0)".
    pub label: String,
    /// Surface kind and parameters, e.g. "plane" or "cylinder(r5.0000)".
    pub kind: String,
    pub area: f64,
    pub center: [f64; 3],
    /// Outward normal, for planar faces only.
    pub normal: Option<[f64; 3]>,
    /// Kernel ids of the face boundary edges. An edge shared by two faces
    /// appears in both, which is what lets edges be named by their face pair.
    #[serde(default)]
    pub edges: Vec<u64>,
}

/// Where one face of an operation result came from.
///
/// The source is named by its *signature*, not by a kernel id: the operation
/// works on copies of its inputs, so the ids the kernel reports for them belong
/// to shapes the caller never sees. The signature is the same geometry either
/// way.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceSource {
    pub source_signature: String,
    /// True when the source face belongs to the tool operand of a boolean.
    pub from_tool: bool,
}

/// Topology of one shape: its faces, its edges, and the face derivation that
/// the last operation recorded.
#[derive(Debug, Clone, Default)]
pub struct ShapeTopology {
    /// Kernel face id -> facts.
    pub faces: BTreeMap<u64, FaceInfo>,
    /// Kernel edge id -> stable geometric edge.
    pub edges: BTreeMap<u64, NamedEdge>,
    /// Result face id -> where it came from. Empty for a primitive and for
    /// operations that rebuild topology without recording history.
    pub face_history: BTreeMap<u64, FaceSource>,
}

impl ShapeTopology {
    /// Faces incident to an edge, in kernel face-id order.
    pub fn faces_of_edge(&self, edge_id: u64) -> Vec<u64> {
        self.faces
            .iter()
            .filter(|(_, face)| face.edges.contains(&edge_id))
            .map(|(face_id, _)| *face_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_with_edges(edges: Vec<u64>) -> FaceInfo {
        FaceInfo {
            edges,
            ..Default::default()
        }
    }

    #[test]
    fn an_edge_reports_every_face_that_bounds_it() {
        let mut topology = ShapeTopology::default();
        topology.faces.insert(1, face_with_edges(vec![10, 11]));
        topology.faces.insert(2, face_with_edges(vec![11, 12]));

        assert_eq!(topology.faces_of_edge(11), vec![1, 2]);
        assert_eq!(topology.faces_of_edge(10), vec![1]);
        assert!(topology.faces_of_edge(99).is_empty());
    }
}
