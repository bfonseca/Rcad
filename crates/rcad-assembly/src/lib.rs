//! Assemblies: component instances, hierarchy, and mate solving.
//!
//! This module provides:
//! - Component instances with full transform hierarchy
//! - Mate constraints (coincident, concentric, distance, angle, parallel, perpendicular)
//! - Assembly solver using rust_slvs for constraint satisfaction
//! - Exploded view generation
//! - Sub-assembly support

use std::collections::HashMap;

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

/// Extended mate kinds for full assembly constraints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MateKindExtended {
    Coincident,
    Concentric,
    Distance,
    Angle,
    Parallel,
    Perpendicular,
    Tangent,
    Symmetric,
}

impl From<MateKind> for MateKindExtended {
    fn from(mk: MateKind) -> Self {
        match mk {
            MateKind::Coincident => MateKindExtended::Coincident,
            MateKind::Concentric => MateKindExtended::Concentric,
            MateKind::Distance => MateKindExtended::Distance,
        }
    }
}

/// Extended mate with full constraint parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MateExtended {
    pub id: Uuid,
    pub kind: MateKindExtended,
    pub instance_a: Uuid,
    pub instance_b: Uuid,
    /// For distance/angle mates
    pub value: Option<f64>,
    /// For angle/parallel/perpendicular: reference entities (faces, edges, axes)
    pub entity_a: Option<Uuid>,
    pub entity_b: Option<Uuid>,
    /// Mate reference frames (for complex mates)
    pub frame_a: Option<[f64; 16]>, // 4x4 column-major matrix
    pub frame_b: Option<[f64; 16]>,
    /// Whether this mate is suppressed
    pub suppressed: bool,
    /// Mate strength for solver priority
    pub strength: f64,
}

impl Default for MateExtended {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: MateKindExtended::Coincident,
            instance_a: Uuid::nil(),
            instance_b: Uuid::nil(),
            value: None,
            entity_a: None,
            entity_b: None,
            frame_a: None,
            frame_b: None,
            suppressed: false,
            strength: 1.0,
        }
    }
}

/// Assembly with extended capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyExtended {
    pub base: Assembly,
    /// Extended mates
    pub mates_ext: Vec<MateExtended>,
    /// Sub-assemblies (nested assemblies)
    pub sub_assemblies: Vec<AssemblyExtended>,
    /// Instance hierarchy (parent-child relationships)
    pub instance_hierarchy: HashMap<Uuid, Uuid>, // child -> parent
    /// Grounded instances (fixed in space)
    pub grounded: Vec<Uuid>,
    /// Exploded view state
    pub exploded_state: Option<ExplodedState>,
}

/// Exploded view state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplodedState {
    pub steps: Vec<ExplodedStep>,
    pub current_step: usize,
    pub total_distance: f64,
    pub direction: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplodedStep {
    pub instance_id: Uuid,
    pub offset: [f64; 3],
    pub step_number: usize,
}

impl Default for ExplodedState {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            current_step: 0,
            total_distance: 50.0,
            direction: [1.0, 1.0, 1.0],
        }
    }
}

/// Assembly solver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverConfig {
    pub max_iterations: usize,
    pub tolerance: f64,
    pub use_slvs: bool,
    pub step_size: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-6,
            use_slvs: true,
            step_size: 0.1,
        }
    }
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

/// Build instance world transforms from hierarchy
pub fn build_instance_transforms(
    assembly: &AssemblyExtended,
) -> Result<HashMap<Uuid, (DVec3, DQuat)>, AssemblyError> {
    let mut transforms = HashMap::new();
    
    // First, add grounded instances
    for &id in &assembly.grounded {
        if let Some(inst) = assembly.base.instances.iter().find(|i| i.id == id) {
            let (t, q) = instance_transform(inst);
            transforms.insert(id, (t, q));
        }
    }
    
    // Process hierarchy in topological order
    let mut processed = assembly.grounded.clone();
    let remaining: Vec<_> = assembly.instance_hierarchy.keys().cloned().collect();
    
    while !remaining.is_empty() {
        let mut progress = false;
        for &child_id in &remaining {
            if let Some(parent_id) = assembly.instance_hierarchy.get(&child_id) {
                if processed.contains(parent_id) {
                    // Compute child transform
                    if let (Some(_parent_inst), Some(child_inst)) = (
                        assembly.base.instances.iter().find(|i| i.id == *parent_id),
                        assembly.base.instances.iter().find(|i| i.id == child_id),
                    ) {
                        let (parent_t, parent_q) = transforms[parent_id];
                        let (child_t_local, child_q_local) = instance_transform(child_inst);
                        let child_t = parent_t + parent_q * child_t_local;
                        let child_q = parent_q * child_q_local;
                        transforms.insert(child_id, (child_t, child_q));
                        processed.push(child_id);
                        progress = true;
                    }
                }
            }
        }
        if !progress {
            // Cycle or missing parent
            break;
        }
    }
    
    Ok(transforms)
}

/// Solve assembly mates using iterative constraint satisfaction
pub fn solve_assembly(
    assembly: &mut AssemblyExtended,
    config: &SolverConfig,
) -> Result<Vec<SolvedPose>, AssemblyError> {
    // If slvs feature enabled and available, use it
    if config.use_slvs {
        #[cfg(feature = "solvespace")]
        {
            return solve_with_slvs(assembly, config);
        }
        #[cfg(not(feature = "solvespace"))]
        {
            eprintln!("slvs feature not enabled, using iterative solver");
        }
    }
    
    // Otherwise use iterative solver
    solve_iterative(assembly, config)
}

/// Iterative constraint solver (fallback)
fn solve_iterative(
    assembly: &mut AssemblyExtended,
    config: &SolverConfig,
) -> Result<Vec<SolvedPose>, AssemblyError> {
    let mut transforms = build_instance_transforms(assembly)?;
    
    for iteration in 0..config.max_iterations {
        let mut max_error: f64 = 0.0;
        
        // Clone mates to avoid borrow conflicts
        let mates: Vec<_> = assembly.mates_ext.iter().filter(|m| !m.suppressed).cloned().collect();
        
        for mut mate in mates {
            let error = solve_mate_constraint(&*assembly, &mut mate, &mut transforms, config.step_size, config.tolerance)?;
            // Update the original mate
            if let Some(orig) = assembly.mates_ext.iter_mut().find(|m| m.id == mate.id) {
                *orig = mate;
            }
            max_error = max_error.max(error);
        }
        
        if max_error < config.tolerance {
            break;
        }
        
        if iteration == config.max_iterations - 1 {
            eprintln!("Assembly solver did not converge after {} iterations (max error: {})",
                     config.max_iterations, max_error);
        }
    }
    
    // Update instances with solved transforms
    for (id, (t, q)) in transforms {
        if let Some(inst) = assembly.base.instances.iter_mut().find(|i| i.id == id) {
            inst.translation = t.into();
            let euler = q.to_euler(EulerRot::XYZ);
            inst.rotation_deg = [
                euler.0.to_degrees(),
                euler.1.to_degrees(),
                euler.2.to_degrees(),
            ];
        }
    }
    
    Ok(assembly.base.instances.iter().map(|i| SolvedPose {
        instance_id: i.id,
        translation: i.translation,
        rotation_deg: i.rotation_deg,
    }).collect())
}

/// Solve a single mate constraint
fn solve_mate_constraint(
    _assembly: &AssemblyExtended,
    mate: &mut MateExtended,
    transforms: &mut HashMap<Uuid, (DVec3, DQuat)>,
    step_size: f64,
    tolerance: f64,
) -> Result<f64, AssemblyError> {
    // Clone the values we need to avoid borrow checker issues
    let (t_a, q_a) = transforms.get(&mate.instance_a).cloned().ok_or(AssemblyError::Message("instance_a not found".into()))?;
    let (t_b, q_b) = transforms.get(&mate.instance_b).cloned().ok_or(AssemblyError::Message("instance_b not found".into()))?;
    
    let mut error = 0.0;
    
    match mate.kind {
        MateKindExtended::Coincident => {
            // Translate B to match A
            let delta = t_a - t_b;
            error = delta.length();
            if let Some(t_b_mut) = transforms.get_mut(&mate.instance_b) {
                t_b_mut.0 = t_a;
            }
        }
        MateKindExtended::Concentric => {
            // Align XY of B to A (keep Z)
            let delta_xy = DVec3::new(t_a.x - t_b.x, t_a.y - t_b.y, 0.0);
            error = delta_xy.length();
            if let Some(t_b_mut) = transforms.get_mut(&mate.instance_b) {
                t_b_mut.0.x = t_a.x;
                t_b_mut.0.y = t_a.y;
            }
        }
        MateKindExtended::Distance => {
            if let Some(target) = mate.value {
                let delta = t_a - t_b;
                let current_dist = delta.length();
                error = (current_dist - target).abs();
                if current_dist > 1e-6 {
                    let dir = delta / current_dist;
                    let new_pos = t_a - dir * target;
                    if let Some(t_b_mut) = transforms.get_mut(&mate.instance_b) {
                        t_b_mut.0 = new_pos;
                    }
                }
            }
        }
        MateKindExtended::Angle => {
            if let Some(target) = mate.value {
                // Angle between two instance frames
                // Simplified: angle between Z axes
                let axis_a = q_a * DVec3::Z;
                let axis_b = q_b * DVec3::Z;
                let current_angle = axis_a.dot(axis_b).clamp(-1.0, 1.0).acos();
                error = (current_angle - target).abs();
                
                // Simple correction: rotate B around axis perpendicular to both
                if error > tolerance {
                    let axis = axis_a.cross(axis_b).normalize_or_zero();
                    if axis.length() > 1e-6 {
                        let delta_q = DQuat::from_axis_angle(axis, (target - current_angle) * step_size);
                        if let Some(t_b_mut) = transforms.get_mut(&mate.instance_b) {
                            t_b_mut.1 = delta_q * t_b_mut.1;
                        }
                    }
                }
            }
        }
        MateKindExtended::Parallel => {
            // Align Z axes
            let axis_a = q_a * DVec3::Z;
            let axis_b = q_b * DVec3::Z;
            let cross = axis_a.cross(axis_b);
            error = cross.length();
            
            if error > tolerance {
                let axis = cross.normalize_or_zero();
                let angle = axis_a.dot(axis_b).clamp(-1.0, 1.0).acos();
                let delta_q = DQuat::from_axis_angle(axis, -angle * step_size);
                if let Some(t_b_mut) = transforms.get_mut(&mate.instance_b) {
                    t_b_mut.1 = delta_q * t_b_mut.1;
                }
            }
        }
        MateKindExtended::Perpendicular => {
            // Align axes to be perpendicular
            let axis_a = q_a * DVec3::Z;
            let axis_b = q_b * DVec3::Z;
            let dot = axis_a.dot(axis_b);
            error = dot.abs();
            
            if error > tolerance {
                let cross = axis_a.cross(axis_b).normalize_or_zero();
                let angle = dot.clamp(-1.0, 1.0).acos() - std::f64::consts::FRAC_PI_2;
                let delta_q = DQuat::from_axis_angle(cross, -angle * step_size);
                if let Some(t_b_mut) = transforms.get_mut(&mate.instance_b) {
                    t_b_mut.1 = delta_q * t_b_mut.1;
                }
            }
        }
        _ => {}
    }
    
    Ok(error)
}

/// Generate exploded view
pub fn generate_exploded_view(
    assembly: &mut AssemblyExtended,
    direction: [f64; 3],
    total_distance: f64,
    steps: usize,
) -> Result<ExplodedState, AssemblyError> {
    let transforms = build_instance_transforms(assembly)?;
    
    // Compute center of mass
    let center = transforms.values()
        .map(|(t, _)| *t)
        .sum::<DVec3>() / transforms.len().max(1) as f64;
    
    let dir = DVec3::from(direction).normalize_or_zero();
    
    let mut state = ExplodedState {
        steps: Vec::new(),
        current_step: 0,
        total_distance,
        direction,
    };
    
    for i in 0..steps {
        let step_dist = total_distance * (i as f64 + 1.0) / steps as f64;
        let _offset = dir * step_dist;
        
        let mut step = ExplodedStep {
            instance_id: Uuid::nil(),
            offset: [0.0, 0.0, 0.0],
            step_number: i,
        };
        
        // Apply to all instances
        for (id, (t, _)) in &transforms {
            let instance_offset = (*t - center).normalize_or_zero() * step_dist;
            step.offset = instance_offset.into();
            step.instance_id = *id;
            state.steps.push(step.clone());
        }
    }
    
    assembly.exploded_state = Some(state.clone());
    Ok(state)
}

/// Get exploded transform for a step
pub fn get_exploded_transform(
    assembly: &AssemblyExtended,
    step: usize,
) -> Result<HashMap<Uuid, (DVec3, DQuat)>, AssemblyError> {
    let transforms = build_instance_transforms(assembly)?;
    
    if let Some(exploded) = &assembly.exploded_state {
        if step < exploded.steps.len() {
            let mut result = HashMap::new();
            for (id, (t, q)) in &transforms {
                if let Some(exploded_step) = exploded.steps.iter().find(|s| s.instance_id == *id && s.step_number == step) {
                    result.insert(*id, (t + DVec3::from(exploded_step.offset), *q));
                } else {
                    result.insert(*id, (*t, *q));
                }
            }
            return Ok(result);
        }
    }
    
    Ok(transforms)
}

/// Add sub-assembly to parent
pub fn add_sub_assembly(
    parent: &mut AssemblyExtended,
    child: AssemblyExtended,
    mate: Option<MateExtended>,
) -> Result<(), AssemblyError> {
    if let Some(m) = mate {
        parent.mates_ext.push(m);
    }
    parent.sub_assemblies.push(child);
    Ok(())
}

/// Ground an instance (fix in space)
pub fn ground_instance(assembly: &mut AssemblyExtended, instance_id: Uuid) {
    if !assembly.grounded.contains(&instance_id) {
        assembly.grounded.push(instance_id);
    }
}

/// Unground an instance
pub fn unground_instance(assembly: &mut AssemblyExtended, instance_id: Uuid) {
    assembly.grounded.retain(|&id| id != instance_id);
}

/// Set parent-child relationship in hierarchy
pub fn set_instance_parent(
    assembly: &mut AssemblyExtended,
    child_id: Uuid,
    parent_id: Uuid,
) -> Result<(), AssemblyError> {
    if child_id == parent_id {
        return Err(AssemblyError::Message("Cannot parent instance to itself".into()));
    }
    // Check for cycles
    let mut current = Some(parent_id);
    while let Some(p) = current {
        if p == child_id {
            return Err(AssemblyError::Message("Cycle detected in hierarchy".into()));
        }
        current = assembly.instance_hierarchy.get(&p).copied();
    }
    assembly.instance_hierarchy.insert(child_id, parent_id);
    Ok(())
}

/// Get all children of an instance
pub fn get_children(assembly: &AssemblyExtended, parent_id: Uuid) -> Vec<Uuid> {
    assembly.instance_hierarchy
        .iter()
        .filter(|&(_, &p)| p == parent_id)
        .map(|(&k, _)| k)
        .collect()
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

/// Solve using SolveSpace (when slvs feature is enabled)
#[cfg(feature = "solvespace")]
fn solve_with_slvs(
    assembly: &mut AssemblyExtended,
    config: &SolverConfig,
) -> Result<Vec<SolvedPose>, AssemblyError> {
    // Placeholder for SolveSpace integration
    // In a full implementation, this would:
    // 1. Create a SolveSpace system
    // 2. Add all instances as rigid bodies
    // 3. Add mate constraints
    // 4. Solve the system
    // 5. Extract transforms back to instances
    eprintln!("SolveSpace integration not yet implemented, falling back to iterative solver");
    solve_iterative(assembly, config)
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
