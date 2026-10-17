//! Analytic 2D profiles (foundation B).
//!
//! A profile is a set of closed loops, each made of analytic segments. The
//! kernel builds real lines, arcs and circles from them, so an extruded circle
//! is a cylinder rather than a polygon. Documents written before B stored a
//! list of points; the custom `Deserialize` below migrates those to line
//! segments on load, without a separate migration step.

use serde::{Deserialize, Deserializer, Serialize};

/// One analytic segment of a closed 2D loop, in sketch coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Curve2D {
    /// Straight segment.
    Line { start: [f64; 2], end: [f64; 2] },
    /// Circular arc. `sweep_angle` is signed: positive runs counter-clockwise
    /// from `start_angle`, negative clockwise.
    Arc {
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    /// Full circle, used as a complete loop (outer or hole).
    Circle { center: [f64; 2], radius: f64 },
}

impl Curve2D {
    pub fn start(&self) -> [f64; 2] {
        match self {
            Curve2D::Line { start, .. } => *start,
            Curve2D::Arc {
                center,
                radius,
                start_angle,
                ..
            } => [
                center[0] + radius * start_angle.cos(),
                center[1] + radius * start_angle.sin(),
            ],
            Curve2D::Circle { center, radius } => [center[0] + radius, center[1]],
        }
    }

    pub fn end(&self) -> [f64; 2] {
        match self {
            Curve2D::Line { end, .. } => *end,
            Curve2D::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                let angle = start_angle + sweep_angle;
                [
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ]
            }
            Curve2D::Circle { .. } => self.start(),
        }
    }

    /// Reverse the direction of travel. A circle has no direction.
    pub fn reverse(&mut self) {
        match self {
            Curve2D::Line { start, end } => std::mem::swap(start, end),
            Curve2D::Arc {
                start_angle,
                sweep_angle,
                ..
            } => {
                *start_angle += *sweep_angle;
                *sweep_angle = -*sweep_angle;
            }
            Curve2D::Circle { .. } => {}
        }
    }

    /// Append the points after the start of this curve to `out`.
    ///
    /// Used for area/containment and for the fallback tessellation of profiles.
    pub fn sample_into(&self, samples: usize, out: &mut Vec<[f64; 2]>) {
        let samples = samples.max(1);
        match self {
            Curve2D::Line { end, .. } => out.push(*end),
            Curve2D::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
            } => {
                for i in 1..=samples {
                    let angle = start_angle + sweep_angle * i as f64 / samples as f64;
                    out.push([
                        center[0] + radius * angle.cos(),
                        center[1] + radius * angle.sin(),
                    ]);
                }
            }
            Curve2D::Circle { center, radius } => {
                for i in 1..=samples {
                    let angle = std::f64::consts::TAU * i as f64 / samples as f64;
                    out.push([
                        center[0] + radius * angle.cos(),
                        center[1] + radius * angle.sin(),
                    ]);
                }
            }
        }
    }

    /// Approximate this curve with a polygon, including its start point.
    pub fn sample(&self, samples: usize) -> Vec<[f64; 2]> {
        let mut points = vec![self.start()];
        self.sample_into(samples, &mut points);
        points
    }
}

/// A closed profile: one outer loop and any number of hole loops.
///
/// Loops are stored as analytic segments. The old point-list representation is
/// still accepted when reading a document (see the custom `Deserialize`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Profile2D {
    /// Closed outer loop in the XY plane (Z = 0), counter-clockwise preferred.
    pub outer: Vec<Curve2D>,
    /// Optional hole loops.
    pub holes: Vec<Vec<Curve2D>>,
}

impl Profile2D {
    /// A polygon loop: consecutive points become lines, the last closes the loop.
    pub fn polygon_loop(points: &[[f64; 2]]) -> Vec<Curve2D> {
        if points.len() < 2 {
            return Vec::new();
        }
        (0..points.len())
            .map(|index| {
                let next = (index + 1) % points.len();
                Curve2D::Line {
                    start: points[index],
                    end: points[next],
                }
            })
            .collect()
    }

    /// A profile from a closed polygon, optionally with hole polygons.
    pub fn from_polygon(points: Vec<[f64; 2]>) -> Self {
        Self {
            outer: Self::polygon_loop(&points),
            holes: Vec::new(),
        }
    }

    pub fn from_loops(outer: Vec<[f64; 2]>, holes: Vec<Vec<[f64; 2]>>) -> Self {
        Self {
            outer: Self::polygon_loop(&outer),
            holes: holes
                .into_iter()
                .map(|hole| Self::polygon_loop(&hole))
                .filter(|hole| hole.len() >= 3)
                .collect(),
        }
    }

    /// A full circle profile: an extruded circle becomes a real cylinder.
    pub fn circle(center: [f64; 2], radius: f64) -> Self {
        Self {
            outer: vec![Curve2D::Circle { center, radius }],
            holes: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.outer.is_empty()
    }

    /// Number of analytic segments in the outer loop.
    pub fn outer_segment_count(&self) -> usize {
        self.outer.len()
    }

    /// A polygon approximation of a loop, used for area and containment.
    pub fn sample_loop(loop_curves: &[Curve2D], samples_per_curve: usize) -> Vec<[f64; 2]> {
        let Some(first) = loop_curves.first() else {
            return Vec::new();
        };
        let mut points = vec![first.start()];
        for curve in loop_curves {
            curve.sample_into(samples_per_curve, &mut points);
        }
        points
    }

    pub fn sample_outer(&self, samples_per_curve: usize) -> Vec<[f64; 2]> {
        Self::sample_loop(&self.outer, samples_per_curve)
    }

    pub fn sample_holes(&self, samples_per_curve: usize) -> Vec<Vec<[f64; 2]>> {
        self.holes
            .iter()
            .map(|hole| Self::sample_loop(hole, samples_per_curve))
            .collect()
    }
}

/// Backwards-compatible representation used only while deserialising.
#[derive(Deserialize)]
#[serde(untagged)]
enum ProfileRepr {
    Curves {
        outer: Vec<Curve2D>,
        holes: Vec<Vec<Curve2D>>,
    },
    /// Pre-B documents stored only points.
    Points {
        outer: Vec<[f64; 2]>,
        holes: Vec<Vec<[f64; 2]>>,
    },
}

impl<'de> Deserialize<'de> for Profile2D {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match ProfileRepr::deserialize(deserializer)? {
            ProfileRepr::Curves { outer, holes } => Ok(Self { outer, holes }),
            ProfileRepr::Points { outer, holes } => Ok(Self::from_loops(outer, holes)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_polygon_closes_its_last_segment() {
        let profile = Profile2D::from_polygon(vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 5.0],
            [0.0, 5.0],
        ]);
        assert_eq!(profile.outer.len(), 4);
        assert_eq!(profile.outer[3].end(), [0.0, 0.0]);
        let points = profile.sample_outer(1);
        assert_eq!(points.first(), Some(&[0.0, 0.0]));
        assert_eq!(points.last(), Some(&[0.0, 0.0]));
    }

    #[test]
    fn an_arc_samples_between_its_endpoints() {
        let curve = Curve2D::Arc {
            center: [0.0, 0.0],
            radius: 10.0,
            start_angle: 0.0,
            sweep_angle: std::f64::consts::FRAC_PI_2,
        };
        assert!((curve.start()[0] - 10.0).abs() < 1e-12);
        assert!((curve.end()[1] - 10.0).abs() < 1e-12);
        let points = curve.sample(4);
        assert_eq!(points.len(), 5);
        assert!((points[2][0] - 10.0 * std::f64::consts::FRAC_PI_4.cos()).abs() < 1e-12);
    }

    #[test]
    fn reversing_an_arc_swaps_its_endpoints() {
        let mut curve = Curve2D::Arc {
            center: [1.0, 2.0],
            radius: 3.0,
            start_angle: 0.3,
            sweep_angle: 1.1,
        };
        let (start, end) = (curve.start(), curve.end());
        curve.reverse();
        assert!((curve.start()[0] - end[0]).abs() < 1e-12);
        assert!((curve.end()[1] - start[1]).abs() < 1e-12);
    }

    #[test]
    fn legacy_point_profiles_migrate_to_lines() {
        let legacy = r#"{"outer":[[0,0],[10,0],[10,5]],"holes":[]}"#;
        let profile: Profile2D = serde_json::from_str(legacy).expect("legacy profile");
        assert_eq!(profile.outer.len(), 3);
        assert!(matches!(profile.outer[0], Curve2D::Line { .. }));
    }

    #[test]
    fn analytic_profiles_round_trip() {
        let profile = Profile2D {
            outer: vec![Curve2D::Circle {
                center: [1.0, 2.0],
                radius: 4.0,
            }],
            holes: vec![vec![Curve2D::Arc {
                center: [0.0, 0.0],
                radius: 1.0,
                start_angle: 0.0,
                sweep_angle: std::f64::consts::PI,
            }]],
        };
        let text = serde_json::to_string(&profile).expect("serialise");
        let parsed: Profile2D = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed, profile);
    }
}
