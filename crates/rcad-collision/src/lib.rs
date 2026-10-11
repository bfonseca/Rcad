//! Collision detection over tessellated meshes (Rapier-ready).
//!
//! v0.1 uses axis-aligned bounding box overlap as a fast broad-phase report.
//! Narrow-phase Rapier contact manifolds can be layered on the same
//! [`CollisionWorld`] without changing call sites.

use rcad_geom::TriangleMesh;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub body_a: u64,
    pub body_b: u64,
    pub point: [f32; 3],
    pub depth: f32,
}

#[derive(Debug, Default)]
pub struct CollisionWorld {
    meshes: Vec<(u64, TriangleMesh)>,
}

impl CollisionWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.meshes.clear();
    }

    pub fn insert_mesh(&mut self, id: u64, mesh: TriangleMesh) {
        if let Some(slot) = self.meshes.iter_mut().find(|(i, _)| *i == id) {
            slot.1 = mesh;
        } else {
            self.meshes.push((id, mesh));
        }
    }

    pub fn contacts(&self) -> Vec<Contact> {
        let mut contacts = Vec::new();
        for i in 0..self.meshes.len() {
            for j in (i + 1)..self.meshes.len() {
                if let Some(c) = aabb_contact(self.meshes[i].0, &self.meshes[i].1, self.meshes[j].0, &self.meshes[j].1)
                {
                    contacts.push(c);
                }
            }
        }
        contacts
    }

    /// Hook reserved for Rapier narrow-phase (feature `rapier`).
    #[cfg(feature = "rapier")]
    pub fn rapier_available() -> bool {
        true
    }

    #[cfg(not(feature = "rapier"))]
    pub fn rapier_available() -> bool {
        false
    }
}

fn aabb_contact(a_id: u64, a: &TriangleMesh, b_id: u64, b: &TriangleMesh) -> Option<Contact> {
    let (amin, amax) = a.bounds()?;
    let (bmin, bmax) = b.bounds()?;
    let overlap = amin.x <= bmax.x
        && amax.x >= bmin.x
        && amin.y <= bmax.y
        && amax.y >= bmin.y
        && amin.z <= bmax.z
        && amax.z >= bmin.z;
    if !overlap {
        return None;
    }
    let point = ((amin + amax) * 0.5 + (bmin + bmax) * 0.5) * 0.5;
    Some(Contact {
        body_a: a_id,
        body_b: b_id,
        point: point.to_array(),
        depth: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_boxes_report_contact() {
        let mut w = CollisionWorld::new();
        let mut m1 = TriangleMesh::default();
        m1.positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        m1.indices = vec![0, 1, 2];
        let mut m2 = m1.clone();
        m2.positions = vec![[0.5, 0.5, 0.0], [1.5, 0.5, 0.0], [0.5, 1.5, 0.0]];
        w.insert_mesh(1, m1);
        w.insert_mesh(2, m2);
        assert_eq!(w.contacts().len(), 1);
    }
}
