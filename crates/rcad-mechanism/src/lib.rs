//! Mechanism definition API (joints, motors, manual actuation).
//!
//! Implementation of real-time solving is reserved for post-MVP work; this crate
//! defines the document types and placeholders the app can already serialize.

use glam::DVec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JointKind {
    Revolute,
    Slider,
    Cylindrical,
    Ball,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Joint {
    pub id: Uuid,
    pub kind: JointKind,
    pub instance_a: Uuid,
    pub instance_b: Uuid,
    pub axis_origin: [f64; 3],
    pub axis_direction: [f64; 3],
    pub limits: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActuatorKind {
    Manual,
    Motor,
    Spring,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actuator {
    pub id: Uuid,
    pub joint_id: Uuid,
    pub kind: ActuatorKind,
    /// Target speed (rad/s or mm/s) for motors; ignored for manual.
    pub rate: f64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mechanism {
    pub joints: Vec<Joint>,
    pub actuators: Vec<Actuator>,
}

impl Mechanism {
    pub fn add_revolute(
        &mut self,
        a: Uuid,
        b: Uuid,
        origin: DVec3,
        axis: DVec3,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.joints.push(Joint {
            id,
            kind: JointKind::Revolute,
            instance_a: a,
            instance_b: b,
            axis_origin: origin.to_array(),
            axis_direction: axis.normalize().to_array(),
            limits: None,
        });
        id
    }

    pub fn add_motor(&mut self, joint_id: Uuid, rate: f64) -> Uuid {
        let id = Uuid::new_v4();
        self.actuators.push(Actuator {
            id,
            joint_id,
            kind: ActuatorKind::Motor,
            rate,
            enabled: true,
        });
        id
    }

    /// Placeholder step — returns joint parameter suggestions.
    pub fn step(&mut self, _dt: f64) -> Vec<(Uuid, f64)> {
        self.actuators
            .iter()
            .filter(|a| a.enabled && a.kind == ActuatorKind::Motor)
            .map(|a| (a.joint_id, a.rate * _dt))
            .collect()
    }
}
