//! Editable animation timeline with analysis markers.

use rcad_cae::AnalysisKind;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe {
    pub id: Uuid,
    pub frame: u32,
    pub joint_id: Option<Uuid>,
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisMarker {
    pub id: Uuid,
    pub frame: u32,
    pub analyses: Vec<AnalysisKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub fps: u32,
    pub length_frames: u32,
    pub current_frame: u32,
    pub keyframes: Vec<Keyframe>,
    pub markers: Vec<AnalysisMarker>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            fps: 30,
            length_frames: 120,
            current_frame: 0,
            keyframes: Vec::new(),
            markers: Vec::new(),
        }
    }
}

impl Timeline {
    pub fn add_keyframe(&mut self, frame: u32, joint_id: Option<Uuid>, value: f64, label: impl Into<String>) -> Uuid {
        let id = Uuid::new_v4();
        self.keyframes.push(Keyframe {
            id,
            frame: frame.min(self.length_frames),
            joint_id,
            value,
            label: label.into(),
        });
        self.keyframes.sort_by_key(|k| k.frame);
        id
    }

    pub fn request_analysis_at(&mut self, frame: u32, analyses: Vec<AnalysisKind>) -> Uuid {
        let id = Uuid::new_v4();
        self.markers.push(AnalysisMarker {
            id,
            frame: frame.min(self.length_frames),
            analyses,
        });
        id
    }

    pub fn markers_at(&self, frame: u32) -> Vec<&AnalysisMarker> {
        self.markers.iter().filter(|m| m.frame == frame).collect()
    }

    pub fn interpolate_joint(&self, joint_id: Uuid, frame: u32) -> Option<f64> {
        let mut relevant: Vec<_> = self
            .keyframes
            .iter()
            .filter(|k| k.joint_id == Some(joint_id))
            .collect();
        if relevant.is_empty() {
            return None;
        }
        relevant.sort_by_key(|k| k.frame);
        if frame <= relevant[0].frame {
            return Some(relevant[0].value);
        }
        if frame >= relevant.last().unwrap().frame {
            return Some(relevant.last().unwrap().value);
        }
        for w in relevant.windows(2) {
            if frame >= w[0].frame && frame <= w[1].frame {
                let t = (frame - w[0].frame) as f64 / (w[1].frame - w[0].frame).max(1) as f64;
                return Some(w[0].value * (1.0 - t) + w[1].value * t);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_keyframes() {
        let mut tl = Timeline::default();
        let j = Uuid::new_v4();
        tl.add_keyframe(0, Some(j), 0.0, "a");
        tl.add_keyframe(10, Some(j), 10.0, "b");
        assert!((tl.interpolate_joint(j, 5).unwrap() - 5.0).abs() < 1e-9);
    }
}
