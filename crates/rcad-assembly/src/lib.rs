//! Assemblies: component instances and basic mates.

use glam::{DQuat, DVec3, EulerRot};
use rcad_doc::{Assembly, ComponentInstance, Mate, MateKind, Project};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AssemblyError {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolvedPose {
    pub instance_id: Uuid,
    pub translation: [f64; 3],
    pub rotation_deg: [f64; 3],
}

/// Insert a part instance into an assembly.
pub fn insert_part(
    assembly: &mut Assembly,
    part_id: Uuid,
    name: impl Into<String>,
) -> Uuid {
    let id = Uuid::new_v4();
    assembly.instances.push(ComponentInstance {
        id,
        part_id,
        name: name.into(),
        translation: [0.0, 0.0, 0.0],
        rotation_deg: [0.0, 0.0, 0.0],
    });
    id
}

pub fn add_mate(
    assembly: &mut Assembly,
    kind: MateKind,
    a: Uuid,
    b: Uuid,
    distance: Option<f64>,
) -> Uuid {
    let id = Uuid::new_v4();
    assembly.mates.push(Mate {
        id,
        kind,
        a,
        b,
        distance,
    });
    id
}

/// Lightweight mate solver: adjusts translations of free instances.
///
/// - Coincident: pull B origin onto A origin
/// - Concentric: align B XY to A XY (keep Z)
/// - Distance: place B along +X from A at the given distance
pub fn solve_mates(assembly: &mut Assembly) -> Result<Vec<SolvedPose>, AssemblyError> {
    let mates = assembly.mates.clone();
    for mate in mates {
        let (a_idx, b_idx) = {
            let a = assembly
                .instances
                .iter()
                .position(|i| i.id == mate.a)
                .ok_or_else(|| AssemblyError::Message("mate A missing".into()))?;
            let b = assembly
                .instances
                .iter()
                .position(|i| i.id == mate.b)
                .ok_or_else(|| AssemblyError::Message("mate B missing".into()))?;
            (a, b)
        };
        let a_t = assembly.instances[a_idx].translation;
        match mate.kind {
            MateKind::Coincident => {
                assembly.instances[b_idx].translation = a_t;
            }
            MateKind::Concentric => {
                assembly.instances[b_idx].translation[0] = a_t[0];
                assembly.instances[b_idx].translation[1] = a_t[1];
            }
            MateKind::Distance => {
                let d = mate.distance.unwrap_or(0.0);
                assembly.instances[b_idx].translation =
                    [a_t[0] + d, a_t[1], a_t[2]];
            }
        }
    }
    Ok(assembly
        .instances
        .iter()
        .map(|i| SolvedPose {
            instance_id: i.id,
            translation: i.translation,
            rotation_deg: i.rotation_deg,
        })
        .collect())
}

pub fn instance_transform(instance: &ComponentInstance) -> (DVec3, DQuat) {
    let t = DVec3::from_array(instance.translation);
    let r = instance.rotation_deg;
    let q = DQuat::from_euler(
        EulerRot::XYZ,
        r[0].to_radians(),
        r[1].to_radians(),
        r[2].to_radians(),
    );
    (t, q)
}

pub fn ensure_assembly(project: &mut Project, name: &str) -> Uuid {
    if let Some(a) = project.assemblies.first() {
        return a.id;
    }
    let a = Assembly::new(name);
    let id = a.id;
    project.assemblies.push(a);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_mate_places_component() {
        let mut asm = Assembly::new("A");
        let a = insert_part(&mut asm, Uuid::new_v4(), "p1");
        let b = insert_part(&mut asm, Uuid::new_v4(), "p2");
        add_mate(&mut asm, MateKind::Distance, a, b, Some(25.0));
        solve_mates(&mut asm).unwrap();
        assert!((asm.instances[1].translation[0] - 25.0).abs() < 1e-9);
    }
}
