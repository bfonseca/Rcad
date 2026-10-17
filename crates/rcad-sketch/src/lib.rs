//! 2D sketches with constraints.
//!
//! Default solver is a lightweight iterative pass (distance / horizontal /
//! vertical / coincident). Enable feature `solvespace` to use the `slvs` crate
//! (SolveSpace) when available in the build environment.

use glam::Vec2;
use rcad_geom::Profile2D;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub type SketchId = Uuid;

#[derive(Debug, Error)]
pub enum SketchError {
    #[error("solver failed: {0}")]
    Solver(String),
    #[error("unknown entity {0}")]
    Unknown(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sketch {
    pub id: SketchId,
    pub name: String,
    pub plane_origin: [f64; 3],
    pub plane_normal: [f64; 3],
    pub plane_x_axis: [f64; 3],
    pub plane_y_axis: [f64; 3],
    pub points: Vec<SketchPoint>,
    pub lines: Vec<SketchLine>,
    pub circles: Vec<SketchCircle>,
    pub arcs: Vec<SketchArc>,
    pub splines: Vec<SketchSpline>,
    /// Conics: rational quadratic curves from three control points and a rho.
    #[serde(default)]
    pub conics: Vec<SketchConic>,
    /// Text notes placed in the sketch. They are drawn, but are not part of the
    /// profile the kernel builds from the sketch.
    #[serde(default)]
    pub texts: Vec<SketchText>,
    pub constraints: Vec<Constraint>,
    /// Constraints with strength info for Intent Manager
    pub constrained_constraints: Vec<ConstrainedConstraint>,
    /// Reference geometry projected onto sketch plane (edges, vertices from 3D model)
    pub reference_geometry: Vec<ReferenceGeometry>,
    /// When true, sketch is waiting for user to select a plane in the viewport
    pub in_placement_mode: bool,
    /// The two references that position the sketch in the model: the
    /// orientation reference it was placed on, and the automatic second one
    /// (the coordinate system at the origin, or the geometry it came from).
    #[serde(default)]
    pub references: Vec<SketchReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceGeometry {
    pub id: Uuid,
    pub name: String,
    pub kind: ReferenceGeometryKind,
    pub projected_points: Vec<[f64; 2]>, // 2D coordinates in sketch plane
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferenceGeometryKind {
    Edge,
    Vertex,
    Face,
    Axis,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StandardPlane {
    XY,
    YZ,
    ZX,
}


/// Which of the two placing references a [`SketchReference`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SketchReferenceRole {
    /// The reference the sketch was placed on: it gives the plane and one axis.
    Orientation,
    /// The automatic second reference - the coordinate system at the origin, a
    /// plane, or the geometry the sketch was drawn from - that fixes the other
    /// direction.
    Automatic,
}

/// A reference that positions a sketch in the model.
///
/// A sketch always needs two: one orientation reference (the plane or face it
/// was placed on) and one that fixes the remaining direction. The second one is
/// automatic: the coordinate system at the origin, or the plane/geometry used
/// as reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchReference {
    /// A stable reference (a datum feature id, a face name), when it has one.
    pub reference: Option<String>,
    /// What the sketch strip shows for it.
    pub label: String,
    pub role: SketchReferenceRole,
    /// The reference's origin and direction, in model space.
    pub origin: [f64; 3],
    pub direction: [f64; 3],
}

impl Default for Sketch {
    fn default() -> Self {
        Self::new("Sketch1")
    }
}

impl Sketch {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: [1.0, 0.0, 0.0],
            plane_y_axis: [0.0, 1.0, 0.0],
            points: Vec::new(),
            lines: Vec::new(),
            circles: Vec::new(),
            arcs: Vec::new(),
            splines: Vec::new(),
            conics: Vec::new(),
            texts: Vec::new(),
            constraints: Vec::new(),
            constrained_constraints: Vec::new(),
            reference_geometry: Vec::new(),
            in_placement_mode: false,
            references: Vec::new(),
        }
    }

    /// Set the two references that position the sketch: the orientation one it
    /// was placed on, and the automatic one that fixes the other direction.
    pub fn set_references(&mut self, orientation: SketchReference, automatic: SketchReference) {
        self.references = vec![orientation, automatic];
    }

    /// The positioning reference with a role, when the sketch has it.
    pub fn reference(&self, role: SketchReferenceRole) -> Option<&SketchReference> {
        self.references
            .iter()
            .find(|reference| reference.role == role)
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> Uuid {
        let id = Uuid::new_v4();
        self.points.push(SketchPoint { id, x, y, fixed: false });
        id
    }

    pub fn add_line(&mut self, a: Uuid, b: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        self.lines.push(SketchLine { id, a, b });
        id
    }

    /// Add a circle from a centre point and radius.
    pub fn add_circle(&mut self, center_x: f64, center_y: f64, radius: f64) -> Uuid {
        let center = self.add_point(center_x, center_y);
        let id = Uuid::new_v4();
        self.circles.push(SketchCircle { id, center, radius });
        id
    }

    pub fn add_rectangle(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> [Uuid; 4] {
        let p0 = self.add_point(x0, y0);
        let p1 = self.add_point(x1, y0);
        let p2 = self.add_point(x1, y1);
        let p3 = self.add_point(x0, y1);
        self.add_line(p0, p1);
        self.add_line(p1, p2);
        self.add_line(p2, p3);
        self.add_line(p3, p0);
        self.constraints.push(Constraint::Horizontal {
            line: self.lines[self.lines.len() - 4].id,
        });
        self.constraints.push(Constraint::Horizontal {
            line: self.lines[self.lines.len() - 2].id,
        });
        self.constraints.push(Constraint::Vertical {
            line: self.lines[self.lines.len() - 3].id,
        });
        self.constraints.push(Constraint::Vertical {
            line: self.lines[self.lines.len() - 1].id,
        });
        [p0, p1, p2, p3]
    }

    /// Add an arc defined by center, radius, start/end angles.
    /// Creates start and end points automatically with coincident constraints.
    pub fn add_arc(
        &mut self,
        center_x: f64,
        center_y: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Uuid {
        let center_id = self.add_point(center_x, center_y);
        let start_x = center_x + radius * start_angle.cos();
        let start_y = center_y + radius * start_angle.sin();
        let end_x = center_x + radius * end_angle.cos();
        let end_y = center_y + radius * end_angle.sin();
        let start_id = self.add_point(start_x, start_y);
        let end_id = self.add_point(end_x, end_y);

        let arc_id = Uuid::new_v4();
        self.arcs.push(SketchArc {
            id: arc_id,
            center: center_id,
            radius,
            start_angle,
            end_angle,
            start_point: start_id,
            end_point: end_id,
        });

        // Coincident constraints: start/end points lie on the arc
        self.constraints.push(Constraint::Coincident {
            a: start_id,
            b: start_id, // Self-reference indicates point-on-arc; handled specially
        });
        self.constraints.push(Constraint::Coincident {
            a: end_id,
            b: end_id,
        });

        // Radius constraint (distance from center to start/end)
        self.constraints.push(Constraint::Distance {
            a: center_id,
            b: start_id,
            value: radius,
        });
        self.constraints.push(Constraint::Distance {
            a: center_id,
            b: end_id,
            value: radius,
        });

        arc_id
    }

    /// Add a spline through a series of points (control points).
    pub fn add_spline(&mut self, control_points: &[Uuid], degree: u32) -> Uuid {
        if control_points.len() < 2 {
            panic!("Spline needs at least 2 control points");
        }
        if degree < 1 || degree as usize >= control_points.len() {
            panic!("Invalid degree for spline");
        }

        let id = Uuid::new_v4();
        let n = control_points.len();
        // Uniform clamped knot vector
        let mut knots = Vec::with_capacity(n + degree as usize + 1);
        for _ in 0..=degree {
            knots.push(0.0);
        }
        for i in 1..n - degree as usize {
            knots.push(i as f64 / (n - degree as usize) as f64);
        }
        for _ in 0..=degree {
            knots.push(1.0);
        }

        self.splines.push(SketchSpline {
            id,
            control_points: control_points.to_vec(),
            degree,
            knots,
            is_rational: false,
            weights: None,
        });

        id
    }

    /// Add a conic from its three control points and its rho.
    pub fn add_conic(&mut self, start: Uuid, control: Uuid, end: Uuid, rho: f64) -> Uuid {
        let id = Uuid::new_v4();
        self.conics.push(SketchConic {
            id,
            start,
            control,
            end,
            rho: rho.clamp(0.05, 0.95),
        });
        id
    }

    /// Points along a conic, `samples` segments, as a rational quadratic Bézier.
    pub fn conic_points(&self, id: Uuid, samples: usize) -> Vec<[f64; 2]> {
        let Some(conic) = self.conics.iter().find(|conic| conic.id == id) else {
            return Vec::new();
        };
        let (Some(p0), Some(p1), Some(p2)) = (
            self.point(conic.start),
            self.point(conic.control),
            self.point(conic.end),
        ) else {
            return Vec::new();
        };
        let segments = samples.max(1);
        (0..=segments)
            .map(|step| {
                let t = step as f64 / segments as f64;
                let b0 = (1.0 - t) * (1.0 - t);
                let b1 = 2.0 * (1.0 - t) * t * conic.rho;
                let b2 = t * t;
                let denominator = (b0 + b1 + b2).max(1e-12);
                [
                    (b0 * p0.x + b1 * p1.x + b2 * p2.x) / denominator,
                    (b0 * p0.y + b1 * p1.y + b2 * p2.y) / denominator,
                ]
            })
            .collect()
    }

    /// Add a text note at a point in the sketch.
    pub fn add_text(&mut self, position: [f64; 2], height: f64, content: impl Into<String>) -> Uuid {
        let id = Uuid::new_v4();
        self.texts.push(SketchText {
            id,
            position,
            height: height.max(0.5),
            angle_deg: 0.0,
            content: content.into(),
        });
        id
    }

    /// Set the sketch plane from a 3D origin, normal, and X direction.
    /// Y direction is computed as normal × x_direction.
    pub fn set_sketch_plane(&mut self, origin: [f64; 3], normal: [f64; 3], x_direction: [f64; 3]) {
        use glam::DVec3;
        let n = DVec3::from(normal).normalize();
        let x = DVec3::from(x_direction);
        let y = n.cross(x).normalize();
        self.plane_origin = origin;
        self.plane_normal = n.into();
        self.plane_x_axis = x.into();
        self.plane_y_axis = y.into();
    }

    /// Set sketch plane to one of the standard planes.
    pub fn set_standard_plane(&mut self, plane: StandardPlane) {
        match plane {
            StandardPlane::XY => {
                self.plane_origin = [0.0, 0.0, 0.0];
                self.plane_normal = [0.0, 0.0, 1.0];
                self.plane_x_axis = [1.0, 0.0, 0.0];
                self.plane_y_axis = [0.0, 1.0, 0.0];
            }
            StandardPlane::YZ => {
                self.plane_origin = [0.0, 0.0, 0.0];
                self.plane_normal = [1.0, 0.0, 0.0];
                self.plane_x_axis = [0.0, 1.0, 0.0];
                self.plane_y_axis = [0.0, 0.0, 1.0];
            }
            StandardPlane::ZX => {
                self.plane_origin = [0.0, 0.0, 0.0];
                self.plane_normal = [0.0, 1.0, 0.0];
                self.plane_x_axis = [0.0, 0.0, 1.0];
                self.plane_y_axis = [1.0, 0.0, 0.0];
            }
        }
    }

    /// Convert a 3D point to 2D sketch coordinates.
    pub fn project_point_3d_to_2d(&self, point_3d: [f64; 3]) -> [f64; 2] {
        use glam::DVec3;
        let p = DVec3::from(point_3d) - DVec3::from(self.plane_origin);
        let x_axis = DVec3::from(self.plane_x_axis);
        let y_axis = DVec3::from(self.plane_y_axis);
        [
            p.dot(x_axis),
            p.dot(y_axis),
        ]
    }

    /// Convert 2D sketch coordinates to 3D point.
    pub fn unproject_point_2d_to_3d(&self, point_2d: [f64; 2]) -> [f64; 3] {
        use glam::DVec3;
        let origin = DVec3::from(self.plane_origin);
        let x_axis = DVec3::from(self.plane_x_axis);
        let y_axis = DVec3::from(self.plane_y_axis);
        (origin + x_axis * point_2d[0] + y_axis * point_2d[1]).into()
    }

    /// Add reference geometry projected onto sketch plane.
    pub fn add_reference_geometry(&mut self, name: String, kind: ReferenceGeometryKind, points_3d: &[[f64; 3]]) {
        let projected_points: Vec<[f64; 2]> = points_3d.iter()
            .map(|p| self.project_point_3d_to_2d(*p))
            .collect();
        self.reference_geometry.push(ReferenceGeometry {
            id: Uuid::new_v4(),
            name,
            kind,
            projected_points,
        });
    }

    pub fn point_mut(&mut self, id: Uuid) -> Option<&mut SketchPoint> {
        self.points.iter_mut().find(|p| p.id == id)
    }

    pub fn point(&self, id: Uuid) -> Option<&SketchPoint> {
        self.points.iter().find(|p| p.id == id)
    }

    pub fn arc_mut(&mut self, id: Uuid) -> Option<&mut SketchArc> {
        self.arcs.iter_mut().find(|a| a.id == id)
    }

    pub fn arc(&self, id: Uuid) -> Option<&SketchArc> {
        self.arcs.iter().find(|a| a.id == id)
    }

    pub fn spline_mut(&mut self, id: Uuid) -> Option<&mut SketchSpline> {
        self.splines.iter_mut().find(|s| s.id == id)
    }

    pub fn spline(&self, id: Uuid) -> Option<&SketchSpline> {
        self.splines.iter().find(|s| s.id == id)
    }

    /// Iterative constraint relaxation (default backend).
    pub fn solve(&mut self) -> Result<(), SketchError> {
        #[cfg(feature = "solvespace")]
        {
            if let Err(e) = solvespace::solve(self) {
                // Fall back to iterative if slvs fails to init.
                tracing_fallback();
                let _ = e;
            } else {
                return Ok(());
            }
        }
        iterative_solve(self)
    }

    /// Closed outer profile from line connectivity (simple ordered points).
    pub fn to_profile(&self) -> Option<Profile2D> {
        if self.points.len() < 3 {
            return None;
        }
        // Prefer rectangle-like ordering from lines if present.
        if self.lines.len() >= 4 {
            let mut ordered = Vec::new();
            let mut current = self.lines[0].a;
            ordered.push(current);
            let mut used = vec![false; self.lines.len()];
            for _ in 0..self.lines.len() {
                if let Some((idx, line)) = self
                    .lines
                    .iter()
                    .enumerate()
                    .find(|(i, l)| !used[*i] && (l.a == current || l.b == current))
                {
                    used[idx] = true;
                    current = if line.a == current { line.b } else { line.a };
                    if ordered.contains(&current) {
                        break;
                    }
                    ordered.push(current);
                } else {
                    break;
                }
            }
            let outer: Vec<[f64; 2]> = ordered
                .iter()
                .filter_map(|id| self.point(*id).map(|p| [p.x, p.y]))
                .collect();
            if outer.len() >= 3 {
                return Some(Profile2D {
                    outer,
                    holes: Vec::new(),
                });
            }
        }
        Some(Profile2D {
            outer: self.points.iter().map(|p| [p.x, p.y]).collect(),
            holes: Vec::new(),
        })
    }

    /// Check if the sketch forms a closed profile (valid for solid creation).
    /// Returns (is_closed, issues) where issues describes any problems.
    pub fn check_profile(&self) -> (bool, Vec<String>) {
        let mut issues = Vec::new();
        
        // Count connections for each point
        let mut connections: std::collections::HashMap<Uuid, usize> = std::collections::HashMap::new();
        
        for line in &self.lines {
            *connections.entry(line.a).or_insert(0) += 1;
            *connections.entry(line.b).or_insert(0) += 1;
        }
        
        for arc in &self.arcs {
            *connections.entry(arc.start_point).or_insert(0) += 1;
            *connections.entry(arc.end_point).or_insert(0) += 1;
        }
        
        for spline in &self.splines {
            if let (Some(first), Some(last)) = (spline.control_points.first(), spline.control_points.last()) {
                *connections.entry(*first).or_insert(0) += 1;
                *connections.entry(*last).or_insert(0) += 1;
            }
        }
        
        // Check for dangling endpoints (degree 1)
        let dangling: Vec<_> = connections.iter()
            .filter(|(_, count)| **count == 1)
            .map(|(id, _)| *id)
            .collect();
        
        if !dangling.is_empty() {
            issues.push(format!("Open profile: {} dangling endpoint(s)", dangling.len()));
        }
        
        // Check for self-intersections (simplified - check line-line intersections)
        for i in 0..self.lines.len() {
            for j in (i + 1)..self.lines.len() {
                let li = &self.lines[i];
                let lj = &self.lines[j];
                if li.a != lj.a && li.a != lj.b && li.b != lj.a && li.b != lj.b {
                    if let (Some(pi1), Some(pi2), Some(pj1), Some(pj2)) = (
                        self.point(li.a), self.point(li.b),
                        self.point(lj.a), self.point(lj.b),
                    ) {
                        if lines_intersect(pi1, pi2, pj1, pj2) {
                            issues.push("Self-intersection detected between lines".to_string());
                        }
                    }
                }
            }
        }
        
        let is_closed = dangling.is_empty() && self.points.len() >= 3;
        (is_closed, issues)
    }
}

/// Parameter `t` along segment `a->b` where it crosses segment `c->d` (both strict interior).
fn segment_segment_t(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> Option<f64> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < 1e-12 {
        return None; // parallel or degenerate
    }
    let qp = [c[0] - a[0], c[1] - a[1]];
    let t = (qp[0] * s[1] - qp[1] * s[0]) / denom;
    let u = (qp[0] * r[1] - qp[1] * r[0]) / denom;
    if t > 1e-9 && t < 1.0 - 1e-9 && u >= -1e-9 && u <= 1.0 + 1e-9 {
        Some(t)
    } else {
        None
    }
}

/// Parameter `t` along the *infinite* line `a->b` where it crosses the line `c->d`.
fn line_line_t(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> Option<f64> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [d[0] - c[0], d[1] - c[1]];
    let denom = r[0] * s[1] - r[1] * s[0];
    if denom.abs() < 1e-12 {
        return None;
    }
    let qp = [c[0] - a[0], c[1] - a[1]];
    Some((qp[0] * s[1] - qp[1] * s[0]) / denom)
}

/// Parameters along the segment `a->b` where it crosses a circle (strict interior).
fn segment_circle_ts(a: [f64; 2], b: [f64; 2], center: [f64; 2], radius: f64) -> Vec<f64> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let f = [a[0] - center[0], a[1] - center[1]];
    let aa = d[0] * d[0] + d[1] * d[1];
    if aa < 1e-12 {
        return Vec::new();
    }
    let bb = 2.0 * (f[0] * d[0] + f[1] * d[1]);
    let cc = f[0] * f[0] + f[1] * f[1] - radius * radius;
    let disc = bb * bb - 4.0 * aa * cc;
    if disc < 0.0 {
        return Vec::new();
    }
    let sq = disc.sqrt();
    [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)]
        .into_iter()
        .filter(|t| *t > 1e-9 && *t < 1.0 - 1e-9)
        .collect()
}

/// Parameters along the *infinite* line `a->b` where it crosses a circle.
fn line_circle_ts(a: [f64; 2], b: [f64; 2], center: [f64; 2], radius: f64) -> Vec<f64> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let f = [a[0] - center[0], a[1] - center[1]];
    let aa = d[0] * d[0] + d[1] * d[1];
    if aa < 1e-12 {
        return Vec::new();
    }
    let bb = 2.0 * (f[0] * d[0] + f[1] * d[1]);
    let cc = f[0] * f[0] + f[1] * f[1] - radius * radius;
    let disc = bb * bb - 4.0 * aa * cc;
    if disc < 0.0 {
        return Vec::new();
    }
    let sq = disc.sqrt();
    vec![(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)]
}

fn lerp2(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn dist_sq(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn lines_intersect(p1: &SketchPoint, p2: &SketchPoint, p3: &SketchPoint, p4: &SketchPoint) -> bool {
    let x1 = p1.x; let y1 = p1.y;
    let x2 = p2.x; let y2 = p2.y;
    let x3 = p3.x; let y3 = p3.y;
    let x4 = p4.x; let y4 = p4.y;
    
    let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if denom.abs() < 1e-9 {
        return false; // Parallel or collinear
    }
    
    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
    let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;
    
    t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0
}


#[allow(dead_code)]
fn tracing_fallback() {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchPoint {
    pub id: Uuid,
    pub x: f64,
    pub y: f64,
    pub fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchLine {
    pub id: Uuid,
    pub a: Uuid,
    pub b: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchCircle {
    pub id: Uuid,
    pub center: Uuid,
    pub radius: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchArc {
    pub id: Uuid,
    pub center: Uuid,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
    pub start_point: Uuid,
    pub end_point: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchSpline {
    pub id: Uuid,
    pub control_points: Vec<Uuid>,
    pub degree: u32,
    pub knots: Vec<f64>,
    pub is_rational: bool,
    pub weights: Option<Vec<f64>>,
}

/// A conic: a rational quadratic curve from `start` to `end`, pulled towards
/// `control`, with `rho` as the middle weight.
///
/// `rho` 0.5 is a parabola, smaller values flatten the curve and larger values
/// round it. The curve always passes through the start and the end, and through
/// the shoulder point the user picked to place it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchConic {
    pub id: Uuid,
    pub start: Uuid,
    pub control: Uuid,
    pub end: Uuid,
    pub rho: f64,
}

/// A text note in the sketch: a position, a height and the words.
///
/// The viewport draws it with the interface font. It is not yet converted to
/// curves for the kernel, so it does not take part in the profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchText {
    pub id: Uuid,
    pub position: [f64; 2],
    pub height: f64,
    pub angle_deg: f64,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Constraint {
    Coincident { a: Uuid, b: Uuid },
    Distance { a: Uuid, b: Uuid, value: f64 },
    Horizontal { line: Uuid },
    Vertical { line: Uuid },
    Fixed { point: Uuid },
    Tangent { a: Uuid, b: Uuid },
    Symmetric { a: Uuid, b: Uuid, axis_line: Uuid },
    EqualRadius { a: Uuid, b: Uuid },
    EqualLength { a: Uuid, b: Uuid },
    Parallel { a: Uuid, b: Uuid },
    Perpendicular { a: Uuid, b: Uuid },
    Midpoint { point: Uuid, line: Uuid },
    Concentric { a: Uuid, b: Uuid },
    /// Angle between two lines, in degrees.
    Angle { a: Uuid, b: Uuid, value: f64 },
    /// Perpendicular distance between two parallel lines.
    LineDistance { a: Uuid, b: Uuid, value: f64 },
    /// The distance between two points measured along X.
    HorizontalDistance { a: Uuid, b: Uuid, value: f64 },
    /// The distance between two points measured along Y.
    VerticalDistance { a: Uuid, b: Uuid, value: f64 },
    /// The radius of a circle or an arc.
    Radius { curve: Uuid, value: f64 },
    /// The diameter of a circle or an arc.
    Diameter { curve: Uuid, value: f64 },
    /// The perpendicular distance from a point to a line.
    PointLineDistance { point: Uuid, line: Uuid, value: f64 },
    /// The distance from a line to the tangent of a circle or an arc: the
    /// perpendicular distance from the line to the centre, minus the radius.
    LineCurveDistance { line: Uuid, curve: Uuid, value: f64 },
}

/// Which relationship [`align_lines`] should impose between two segments.
#[derive(Clone, Copy)]
enum AlignTarget {
    Parallel,
    Perpendicular,
}

/// Rotate segment `b` around a pivot until it is parallel or perpendicular to
/// segment `a`, respecting fixed endpoints.
///
/// Previously the iterative solver only *measured* the parallel/perpendicular
/// error and never moved any geometry, so those constraints were no-ops.
fn align_lines(sketch: &mut Sketch, a: Uuid, b: Uuid, max_err: &mut f64, target: AlignTarget) {
    let segment = |sketch: &Sketch, id: Uuid| -> Option<(Uuid, Uuid, [f64; 2], [f64; 2], bool, bool)> {
        let line = sketch.lines.iter().find(|l| l.id == id)?;
        let p1 = sketch.point(line.a)?;
        let p2 = sketch.point(line.b)?;
        Some((line.a, line.b, [p1.x, p1.y], [p2.x, p2.y], p1.fixed, p2.fixed))
    };

    let (Some((_, _, pa1, pa2, _, _)), Some((b1, b2, pb1, pb2, f1, f2))) =
        (segment(sketch, a), segment(sketch, b))
    else {
        return;
    };

    let (dax, day) = (pa2[0] - pa1[0], pa2[1] - pa1[1]);
    let (dbx, dby) = (pb2[0] - pb1[0], pb2[1] - pb1[1]);
    let len_a = (dax * dax + day * day).sqrt();
    let len_b = (dbx * dbx + dby * dby).sqrt();
    if len_a < 1e-12 || len_b < 1e-12 {
        return;
    }
    let (uax, uay) = (dax / len_a, day / len_a);
    let (ubx, uby) = (dbx / len_b, dby / len_b);
    let dot = uax * ubx + uay * uby;
    let cross = uax * uby - uay * ubx;
    let err = match target {
        AlignTarget::Parallel => cross.abs(),
        AlignTarget::Perpendicular => dot.abs(),
    };
    *max_err = max_err.max(err);
    if err < 1e-9 || (f1 && f2) {
        return;
    }

    // Target direction for b, picking whichever orientation is already closest.
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let (tx, ty) = match target {
        AlignTarget::Parallel => (uax * sign, uay * sign),
        AlignTarget::Perpendicular => (-uay * sign, uax * sign),
    };

    // Rotation taking u_b onto t.
    let cos_t = tx * ubx + ty * uby;
    let sin_t = ubx * ty - uby * tx;

    let pivot = if f1 {
        pb1
    } else if f2 {
        pb2
    } else {
        [(pb1[0] + pb2[0]) * 0.5, (pb1[1] + pb2[1]) * 0.5]
    };

    for (id, p, is_fixed) in [(b1, pb1, f1), (b2, pb2, f2)] {
        if is_fixed {
            continue;
        }
        let vx = p[0] - pivot[0];
        let vy = p[1] - pivot[1];
        let nx = pivot[0] + cos_t * vx - sin_t * vy;
        let ny = pivot[1] + sin_t * vx + cos_t * vy;
        if let Some(pt) = sketch.point_mut(id) {
            pt.x = nx;
            pt.y = ny;
        }
    }
}

/// Scale segment `e1-e2` towards `target` length around a pivot (its midpoint,
/// or a fixed endpoint). Fixed endpoints never move.
#[allow(clippy::too_many_arguments)]
fn resize_segment(
    sketch: &mut Sketch,
    e1: Uuid,
    e2: Uuid,
    p1: [f64; 2],
    p2: [f64; 2],
    f1: bool,
    f2: bool,
    target: f64,
) {
    let len = dist_sq(p1, p2).sqrt();
    if len < 1e-12 || target <= 0.0 || (f1 && f2) {
        return;
    }
    let scale = target / len;
    let pivot = if f1 {
        p1
    } else if f2 {
        p2
    } else {
        [(p1[0] + p2[0]) * 0.5, (p1[1] + p2[1]) * 0.5]
    };
    for (id, p, is_fixed) in [(e1, p1, f1), (e2, p2, f2)] {
        if is_fixed {
            continue;
        }
        let x = pivot[0] + (p[0] - pivot[0]) * scale;
        let y = pivot[1] + (p[1] - pivot[1]) * scale;
        if let Some(pt) = sketch.point_mut(id) {
            pt.x = x;
            pt.y = y;
        }
    }
}

/// The centre point and radius of a circle or an arc.
fn curve_of(sketch: &Sketch, id: Uuid) -> Option<(Uuid, f64)> {
    sketch
        .circles
        .iter()
        .find(|circle| circle.id == id)
        .map(|circle| (circle.center, circle.radius))
        .or_else(|| {
            sketch
                .arcs
                .iter()
                .find(|arc| arc.id == id)
                .map(|arc| (arc.center, arc.radius))
        })
}

/// Set the radius of a circle or an arc, keeping an arc's endpoints on it.
fn set_curve_radius(sketch: &mut Sketch, id: Uuid, radius: f64) {
    if radius <= 1e-9 {
        return;
    }
    if let Some(circle) = sketch.circles.iter_mut().find(|circle| circle.id == id) {
        circle.radius = radius;
        return;
    }
    let Some(arc) = sketch.arcs.iter().find(|arc| arc.id == id).cloned() else {
        return;
    };
    let Some(centre) = sketch.point(arc.center).map(|point| [point.x, point.y]) else {
        return;
    };
    if let Some(stored) = sketch.arcs.iter_mut().find(|candidate| candidate.id == id) {
        stored.radius = radius;
    }
    let start = [
        centre[0] + radius * arc.start_angle.cos(),
        centre[1] + radius * arc.start_angle.sin(),
    ];
    let end = [
        centre[0] + radius * arc.end_angle.cos(),
        centre[1] + radius * arc.end_angle.sin(),
    ];
    if let Some(point) = sketch.point_mut(arc.start_point) {
        point.x = start[0];
        point.y = start[1];
    }
    if let Some(point) = sketch.point_mut(arc.end_point) {
        point.x = end[0];
        point.y = end[1];
    }
}

fn iterative_solve(sketch: &mut Sketch) -> Result<(), SketchError> {
    for _ in 0..80 {
        let mut max_err = 0.0f64;
        let constraints = sketch.constraints.clone();
        // A driven (reference) dimension measures; it does not drive.
        let driven: Vec<Constraint> = sketch
            .constrained_constraints
            .iter()
            .filter(|record| !record.is_driving)
            .map(|record| record.constraint.clone())
            .collect();
        for c in constraints {
            // A driven (reference) dimension measures; it does not drive.
            if is_dimension(&c) && driven.iter().any(|subject| same_subject(subject, &c)) {
                continue;
            }
            match c {
                Constraint::Fixed { point } => {
                    if let Some(p) = sketch.point_mut(point) {
                        p.fixed = true;
                    }
                }
                Constraint::Coincident { a, b } => {
                    let (ax, ay, af) = {
                        let p = sketch.point(a).ok_or(SketchError::Unknown(a))?;
                        (p.x, p.y, p.fixed)
                    };
                    let (bx, by, bf) = {
                        let p = sketch.point(b).ok_or(SketchError::Unknown(b))?;
                        (p.x, p.y, p.fixed)
                    };
                    let mx = (ax + bx) * 0.5;
                    let my = (ay + by) * 0.5;
                    max_err = max_err.max((ax - bx).abs() + (ay - by).abs());
                    if !af {
                        if let Some(p) = sketch.point_mut(a) {
                            p.x = mx;
                            p.y = my;
                        }
                    }
                    if !bf {
                        if let Some(p) = sketch.point_mut(b) {
                            p.x = mx;
                            p.y = my;
                        }
                    }
                }
                Constraint::Distance { a, b, value } => {
                    let (ax, ay, af) = {
                        let p = sketch.point(a).ok_or(SketchError::Unknown(a))?;
                        (p.x, p.y, p.fixed)
                    };
                    let (bx, by, bf) = {
                        let p = sketch.point(b).ok_or(SketchError::Unknown(b))?;
                        (p.x, p.y, p.fixed)
                    };
                    let pa = Vec2::new(ax as f32, ay as f32);
                    let pb = Vec2::new(bx as f32, by as f32);
                    let mut d = pb - pa;
                    let len = d.length().max(1e-6);
                    let err = (len as f64 - value).abs();
                    max_err = max_err.max(err);
                    d = d.normalize() * value as f32;
                    let mid = (pa + pb) * 0.5;
                    let na = mid - d * 0.5;
                    let nb = mid + d * 0.5;
                    if !af {
                        if let Some(p) = sketch.point_mut(a) {
                            p.x = na.x as f64;
                            p.y = na.y as f64;
                        }
                    }
                    if !bf {
                        if let Some(p) = sketch.point_mut(b) {
                            p.x = nb.x as f64;
                            p.y = nb.y as f64;
                        }
                    }
                }
                Constraint::Horizontal { line } => {
                    let (a, b) = {
                        let l = sketch
                            .lines
                            .iter()
                            .find(|l| l.id == line)
                            .ok_or(SketchError::Unknown(line))?;
                        (l.a, l.b)
                    };
                    let (ay, af) = {
                        let p = sketch.point(a).ok_or(SketchError::Unknown(a))?;
                        (p.y, p.fixed)
                    };
                    let (by, bf) = {
                        let p = sketch.point(b).ok_or(SketchError::Unknown(b))?;
                        (p.y, p.fixed)
                    };
                    let my = (ay + by) * 0.5;
                    max_err = max_err.max((ay - by).abs());
                    if !af {
                        if let Some(p) = sketch.point_mut(a) {
                            p.y = my;
                        }
                    }
                    if !bf {
                        if let Some(p) = sketch.point_mut(b) {
                            p.y = my;
                        }
                    }
                }
                Constraint::Vertical { line } => {
                    let (a, b) = {
                        let l = sketch
                            .lines
                            .iter()
                            .find(|l| l.id == line)
                            .ok_or(SketchError::Unknown(line))?;
                        (l.a, l.b)
                    };
                    let (ax, af) = {
                        let p = sketch.point(a).ok_or(SketchError::Unknown(a))?;
                        (p.x, p.fixed)
                    };
                    let (bx, bf) = {
                        let p = sketch.point(b).ok_or(SketchError::Unknown(b))?;
                        (p.x, p.fixed)
                    };
                    let mx = (ax + bx) * 0.5;
                    max_err = max_err.max((ax - bx).abs());
                    if !af {
                        if let Some(p) = sketch.point_mut(a) {
                            p.x = mx;
                        }
                    }
                    if !bf {
                        if let Some(p) = sketch.point_mut(b) {
                            p.x = mx;
                        }
                    }
                }
                Constraint::Tangent { a, b } => {
                    // Line/arc tangency: move the arc centre along the line
                    // normal until its distance to the line equals the radius.
                    // The previous version measured the error and then changed
                    // nothing at all. Arc/arc tangency is still not handled.
                    let arc = sketch
                        .arcs
                        .iter()
                        .find(|arc| arc.id == a || arc.id == b)
                        .map(|arc| (arc.center, arc.radius));
                    let line = sketch
                        .lines
                        .iter()
                        .find(|l| l.id == a || l.id == b)
                        .map(|l| (l.a, l.b));
                    if let (Some((center_id, radius)), Some((l1, l2))) = (arc, line) {
                        let data = match (
                            sketch.point(center_id),
                            sketch.point(l1),
                            sketch.point(l2),
                        ) {
                            (Some(c), Some(p1), Some(p2)) => {
                                Some((c.x, c.y, c.fixed, p1.x, p1.y, p2.x, p2.y))
                            }
                            _ => None,
                        };
                        if let Some((cx, cy, fixed, x1, y1, x2, y2)) = data {
                            let dx = x2 - x1;
                            let dy = y2 - y1;
                            let len = (dx * dx + dy * dy).sqrt();
                            if len > 1e-12 {
                                let nx = -dy / len;
                                let ny = dx / len;
                                let signed = (cx - x1) * nx + (cy - y1) * ny;
                                max_err = max_err.max((signed.abs() - radius).abs());
                                if !fixed {
                                    let target = if signed < 0.0 { -radius } else { radius };
                                    let delta = target - signed;
                                    if let Some(p) = sketch.point_mut(center_id) {
                                        p.x = cx + nx * delta;
                                        p.y = cy + ny * delta;
                                    }
                                }
                            }
                        }
                    }
                }
                Constraint::Symmetric { a, b, axis_line } => {
                    // Symmetry: points a and b are symmetric about axis_line
                    // First get axis line endpoints to avoid borrow conflicts
                    let axis_endpoints = sketch.lines.iter()
                        .find(|l| l.id == axis_line)
                        .and_then(|l| {
                            Some((sketch.point(l.a)?.clone(), sketch.point(l.b)?.clone()))
                        });
                    
                    if let Some((p1, p2)) = axis_endpoints {
                        // Get points sequentially to avoid double mutable borrow
                        // First compute the mirrored position
                        let result = if let Some(pa) = sketch.point_mut(a) {
                            // Mirror point a across axis line to get target for b
                            let dx = p2.x - p1.x;
                            let dy = p2.y - p1.y;
                            let len = (dx * dx + dy * dy).sqrt().max(1e-9);
                            let ux = dx / len;
                            let uy = dy / len;
                            // Project vector from p1 to pa onto axis
                            let vx = pa.x - p1.x;
                            let vy = pa.y - p1.y;
                            let proj = vx * ux + vy * uy;
                            let perp_x = vx - proj * ux;
                            let perp_y = vy - proj * uy;
                            // Mirrored point
                            let mx = p1.x + proj * ux - perp_x;
                            let my = p1.y + proj * uy - perp_y;
                            let pa_fixed = pa.fixed;
                            if !pa_fixed {
                                pa.x = mx;
                                pa.y = my;
                            }
                            Some((mx, my, pa_fixed))
                        } else {
                            None
                        };
                        
                        if let Some((mx, my, _pa_fixed)) = result {
                            if let Some(pb) = sketch.point_mut(b) {
                                let err = ((pb.x - mx).powi(2) + (pb.y - my).powi(2)).sqrt();
                                max_err = max_err.max(err);
                                if !pb.fixed {
                                    pb.x = mx;
                                    pb.y = my;
                                }
                            }
                        }
                    }
                }
                Constraint::EqualRadius { a, b } => {
                    // Equal radius between two arcs or circles. The radii are
                    // moved to their average; previously the error was measured
                    // but never applied, so the constraint did nothing.
                    let radius_of = |sketch: &Sketch, id: Uuid| -> Option<f64> {
                        sketch
                            .arcs
                            .iter()
                            .find(|x| x.id == id)
                            .map(|x| x.radius)
                            .or_else(|| {
                                sketch
                                    .circles
                                    .iter()
                                    .find(|x| x.id == id)
                                    .map(|x| x.radius)
                            })
                    };
                    if let (Some(ra), Some(rb)) = (radius_of(sketch, a), radius_of(sketch, b)) {
                        max_err = max_err.max((ra - rb).abs());
                        if (ra - rb).abs() > 1e-12 {
                            let avg = (ra + rb) * 0.5;
                            for id in [a, b] {
                                if let Some(arc) = sketch.arcs.iter_mut().find(|x| x.id == id) {
                                    arc.radius = avg;
                                } else if let Some(circle) =
                                    sketch.circles.iter_mut().find(|x| x.id == id)
                                {
                                    circle.radius = avg;
                                }
                            }
                            // Keep arc endpoints on the new radius.
                            let rescaled: Vec<(Uuid, f64, f64)> = sketch
                                .arcs
                                .iter()
                                .filter(|arc| arc.id == a || arc.id == b)
                                .filter_map(|arc| {
                                    let c = sketch.point(arc.center)?;
                                    let s = sketch.point(arc.start_point)?;
                                    let e = sketch.point(arc.end_point)?;
                                    let sa = (s.y - c.y).atan2(s.x - c.x);
                                    let ea = (e.y - c.y).atan2(e.x - c.x);
                                    Some(vec![
                                        (
                                            arc.start_point,
                                            c.x + avg * sa.cos(),
                                            c.y + avg * sa.sin(),
                                        ),
                                        (
                                            arc.end_point,
                                            c.x + avg * ea.cos(),
                                            c.y + avg * ea.sin(),
                                        ),
                                    ])
                                })
                                .flatten()
                                .collect();
                            for (id, x, y) in rescaled {
                                if let Some(p) = sketch.point_mut(id) {
                                    p.x = x;
                                    p.y = y;
                                }
                            }
                        }
                    }
                }
                Constraint::EqualLength { a, b } => {
                    // Equal length between two lines: scale each towards the
                    // average length around its midpoint. Previously the error
                    // was measured but never applied.
                    let segment = |sketch: &Sketch,
                                   id: Uuid|
     -> Option<(Uuid, Uuid, [f64; 2], [f64; 2], bool, bool)> {
                        let line = sketch.lines.iter().find(|l| l.id == id)?;
                        let p1 = sketch.point(line.a)?;
                        let p2 = sketch.point(line.b)?;
                        Some((
                            line.a,
                            line.b,
                            [p1.x, p1.y],
                            [p2.x, p2.y],
                            p1.fixed,
                            p2.fixed,
                        ))
                    };
                    if let (Some(sa), Some(sb)) = (segment(sketch, a), segment(sketch, b)) {
                        let len_a = dist_sq(sa.2, sa.3).sqrt();
                        let len_b = dist_sq(sb.2, sb.3).sqrt();
                        max_err = max_err.max((len_a - len_b).abs());
                        let target = (len_a + len_b) * 0.5;
                        resize_segment(sketch, sa.0, sa.1, sa.2, sa.3, sa.4, sa.5, target);
                        resize_segment(sketch, sb.0, sb.1, sb.2, sb.3, sb.4, sb.5, target);
                    }
                }
                Constraint::Parallel { a, b } => {
                    // Rotate line b until it is parallel to line a.
                    align_lines(sketch, a, b, &mut max_err, AlignTarget::Parallel);
                }
                Constraint::Perpendicular { a, b } => {
                    // Rotate line b until it is perpendicular to line a.
                    align_lines(sketch, a, b, &mut max_err, AlignTarget::Perpendicular);
                }
                Constraint::Angle { a, b, value } => {
                    // Rotate line b about its midpoint (or its fixed end) until
                    // it sits at the wanted angle to line a.
                    let segment = |sketch: &Sketch,
                                   id: Uuid|
                     -> Option<(Uuid, Uuid, [f64; 2], [f64; 2], bool, bool)> {
                        let line = sketch.lines.iter().find(|line| line.id == id)?;
                        let p1 = sketch.point(line.a)?;
                        let p2 = sketch.point(line.b)?;
                        Some((line.a, line.b, [p1.x, p1.y], [p2.x, p2.y], p1.fixed, p2.fixed))
                    };
                    let Some(first) = segment(sketch, a) else {
                        continue;
                    };
                    let Some(second) = segment(sketch, b) else {
                        continue;
                    };
                    let direction =
                        |segment: &(Uuid, Uuid, [f64; 2], [f64; 2], bool, bool)| {
                            [
                                segment.3[0] - segment.2[0],
                                segment.3[1] - segment.2[1],
                            ]
                        };
                    let (d1, d2) = (direction(&first), direction(&second));
                    if d1[0].hypot(d1[1]) < 1e-12 || d2[0].hypot(d2[1]) < 1e-12 {
                        continue;
                    }
                    let current =
                        (d1[0] * d2[1] - d1[1] * d2[0]).atan2(d1[0] * d2[0] + d1[1] * d2[1]);
                    let mut delta = value.to_radians() - current;
                    while delta > std::f64::consts::PI {
                        delta -= 2.0 * std::f64::consts::PI;
                    }
                    while delta < -std::f64::consts::PI {
                        delta += 2.0 * std::f64::consts::PI;
                    }
                    max_err = max_err.max(delta.abs());
                    if delta.abs() < 1e-12 {
                        continue;
                    }
                    let pivot = if second.4 {
                        second.2
                    } else if second.5 {
                        second.3
                    } else {
                        [
                            (second.2[0] + second.3[0]) * 0.5,
                            (second.2[1] + second.3[1]) * 0.5,
                        ]
                    };
                    let (sin, cos) = delta.sin_cos();
                    for (id, point, is_fixed) in
                        [(second.0, second.2, second.4), (second.1, second.3, second.5)]
                    {
                        if is_fixed {
                            continue;
                        }
                        let x = pivot[0]
                            + (point[0] - pivot[0]) * cos
                            - (point[1] - pivot[1]) * sin;
                        let y = pivot[1]
                            + (point[0] - pivot[0]) * sin
                            + (point[1] - pivot[1]) * cos;
                        if let Some(p) = sketch.point_mut(id) {
                            p.x = x;
                            p.y = y;
                        }
                    }
                }
                Constraint::HorizontalDistance { a, b, value } => {
                    // Move the two points along X until the horizontal distance
                    // is the wanted one, keeping the midpoint.
                    let (ax, af) = {
                        let p = sketch.point(a).ok_or(SketchError::Unknown(a))?;
                        (p.x, p.fixed)
                    };
                    let (bx, bf) = {
                        let p = sketch.point(b).ok_or(SketchError::Unknown(b))?;
                        (p.x, p.fixed)
                    };
                    let mut d = bx - ax;
                    max_err = max_err.max((d.abs() - value).abs());
                    let sign = if d < 0.0 { -1.0 } else { 1.0 };
                    d = value * sign;
                    let mid = (ax + bx) * 0.5;
                    if !af {
                        if let Some(p) = sketch.point_mut(a) {
                            p.x = mid - d * 0.5;
                        }
                    }
                    if !bf {
                        if let Some(p) = sketch.point_mut(b) {
                            p.x = mid + d * 0.5;
                        }
                    }
                }
                Constraint::VerticalDistance { a, b, value } => {
                    let (ay, af) = {
                        let p = sketch.point(a).ok_or(SketchError::Unknown(a))?;
                        (p.y, p.fixed)
                    };
                    let (by, bf) = {
                        let p = sketch.point(b).ok_or(SketchError::Unknown(b))?;
                        (p.y, p.fixed)
                    };
                    let mut d = by - ay;
                    max_err = max_err.max((d.abs() - value).abs());
                    let sign = if d < 0.0 { -1.0 } else { 1.0 };
                    d = value * sign;
                    let mid = (ay + by) * 0.5;
                    if !af {
                        if let Some(p) = sketch.point_mut(a) {
                            p.y = mid - d * 0.5;
                        }
                    }
                    if !bf {
                        if let Some(p) = sketch.point_mut(b) {
                            p.y = mid + d * 0.5;
                        }
                    }
                }
                Constraint::LineDistance { a, b, value } => {
                    // Slide line b along the normal of line a until the
                    // perpendicular distance is the wanted one.
                    let segment = |sketch: &Sketch,
                                   id: Uuid|
                     -> Option<(Uuid, Uuid, [f64; 2], [f64; 2], bool, bool)> {
                        let line = sketch.lines.iter().find(|line| line.id == id)?;
                        let p1 = sketch.point(line.a)?;
                        let p2 = sketch.point(line.b)?;
                        Some((line.a, line.b, [p1.x, p1.y], [p2.x, p2.y], p1.fixed, p2.fixed))
                    };
                    let Some(first) = segment(sketch, a) else {
                        continue;
                    };
                    let Some(second) = segment(sketch, b) else {
                        continue;
                    };
                    let direction = [
                        first.3[0] - first.2[0],
                        first.3[1] - first.2[1],
                    ];
                    let length = direction[0].hypot(direction[1]);
                    if length < 1e-12 {
                        continue;
                    }
                    let normal = [-direction[1] / length, direction[0] / length];
                    let signed = (second.2[0] - first.2[0]) * normal[0]
                        + (second.2[1] - first.2[1]) * normal[1];
                    let target = if signed < 0.0 { -value } else { value };
                    let delta = target - signed;
                    max_err = max_err.max(delta.abs());
                    if delta.abs() < 1e-12 {
                        continue;
                    }
                    for (id, point, is_fixed) in
                        [(second.0, second.2, second.4), (second.1, second.3, second.5)]
                    {
                        if is_fixed {
                            continue;
                        }
                        if let Some(p) = sketch.point_mut(id) {
                            p.x = point[0] + normal[0] * delta;
                            p.y = point[1] + normal[1] * delta;
                        }
                    }
                }

                Constraint::Radius { curve, value } => {
                    let Some((_, current)) = curve_of(sketch, curve) else {
                        continue;
                    };
                    max_err = max_err.max((current - value).abs());
                    set_curve_radius(sketch, curve, value);
                }
                Constraint::Diameter { curve, value } => {
                    let Some((_, current)) = curve_of(sketch, curve) else {
                        continue;
                    };
                    max_err = max_err.max((current * 2.0 - value).abs());
                    set_curve_radius(sketch, curve, value * 0.5);
                }
                Constraint::PointLineDistance { point, line, value } => {
                    // The perpendicular distance from the point to the line.
                    let endpoints = sketch
                        .lines
                        .iter()
                        .find(|candidate| candidate.id == line)
                        .and_then(|candidate| {
                            Some((
                                sketch.point(candidate.a)?.clone(),
                                sketch.point(candidate.b)?.clone(),
                            ))
                        });
                    let Some((start, end)) = endpoints else {
                        continue;
                    };
                    let Some((px, py, fixed)) = sketch.point(point).map(|p| (p.x, p.y, p.fixed)) else {
                        continue;
                    };
                    let (dx, dy) = (end.x - start.x, end.y - start.y);
                    let length = dx.hypot(dy);
                    if length < 1e-12 {
                        continue;
                    }
                    let (nx, ny) = (-dy / length, dx / length);
                    let signed = (px - start.x) * nx + (py - start.y) * ny;
                    max_err = max_err.max((signed.abs() - value).abs());
                    if fixed {
                        continue;
                    }
                    let target = if signed < 0.0 { -value } else { value };
                    let delta = target - signed;
                    if let Some(point) = sketch.point_mut(point) {
                        point.x = px + nx * delta;
                        point.y = py + ny * delta;
                    }
                }
                Constraint::LineCurveDistance { line, curve, value } => {
                    // Slide the curve away from (or towards) the line until the
                    // gap between them is the wanted one, measured on the
                    // tangent: centre distance minus radius.
                    let endpoints = sketch
                        .lines
                        .iter()
                        .find(|candidate| candidate.id == line)
                        .and_then(|candidate| {
                            Some((
                                sketch.point(candidate.a)?.clone(),
                                sketch.point(candidate.b)?.clone(),
                            ))
                        });
                    let Some((start, end)) = endpoints else {
                        continue;
                    };
                    let Some((centre, radius)) = curve_of(sketch, curve) else {
                        continue;
                    };
                    let Some((cx, cy, fixed)) = sketch.point(centre).map(|p| (p.x, p.y, p.fixed)) else {
                        continue;
                    };
                    let (dx, dy) = (end.x - start.x, end.y - start.y);
                    let length = dx.hypot(dy);
                    if length < 1e-12 {
                        continue;
                    }
                    let (nx, ny) = (-dy / length, dx / length);
                    let signed = (cx - start.x) * nx + (cy - start.y) * ny;
                    let wanted = value + radius;
                    max_err = max_err.max((signed.abs() - wanted).abs());
                    if fixed {
                        continue;
                    }
                    let target = if signed < 0.0 { -wanted } else { wanted };
                    let delta = target - signed;
                    if let Some(point) = sketch.point_mut(centre) {
                        point.x = cx + nx * delta;
                        point.y = cy + ny * delta;
                    }
                }
                Constraint::Midpoint { point, line } => {
                    // Point is midpoint of line
                    // First get line endpoints to avoid borrow conflicts
                    let line_endpoints = sketch.lines.iter()
                        .find(|l| l.id == line)
                        .and_then(|l| {
                            Some((sketch.point(l.a)?.clone(), sketch.point(l.b)?.clone()))
                        });
                    
                    if let Some((pa, pb)) = line_endpoints {
                        if let Some(p) = sketch.point_mut(point) {
                            let mx = (pa.x + pb.x) * 0.5;
                            let my = (pa.y + pb.y) * 0.5;
                            let err = ((p.x - mx).powi(2) + (p.y - my).powi(2)).sqrt();
                            max_err = max_err.max(err);
                            if !p.fixed {
                                p.x = mx;
                                p.y = my;
                            }
                        }
                    }
                }
                Constraint::Concentric { a, b } => {
                    // Concentric: centers of two arcs/circles coincide
                    let mut center_a = None;
                    let mut center_b = None;
                    if let Some(arc) = sketch.arcs.iter().find(|arc| arc.id == a) {
                        center_a = Some(arc.center);
                    } else if let Some(circle) = sketch.circles.iter().find(|circle| circle.id == a) {
                        center_a = Some(circle.center);
                    }
                    if let Some(arc) = sketch.arcs.iter().find(|arc| arc.id == b) {
                        center_b = Some(arc.center);
                    } else if let Some(circle) = sketch.circles.iter().find(|circle| circle.id == b) {
                        center_b = Some(circle.center);
                    }
                    if let (Some(ca), Some(cb)) = (center_a, center_b) {
                        // Reuse coincident constraint logic
                        let (ax, ay, af) = {
                            let p = sketch.point(ca).ok_or(SketchError::Unknown(ca))?;
                            (p.x, p.y, p.fixed)
                        };
                        let (bx, by, bf) = {
                            let p = sketch.point(cb).ok_or(SketchError::Unknown(cb))?;
                            (p.x, p.y, p.fixed)
                        };
                        let mx = (ax + bx) * 0.5;
                        let my = (ay + by) * 0.5;
                        max_err = max_err.max((ax - bx).abs() + (ay - by).abs());
                        if !af {
                            if let Some(p) = sketch.point_mut(ca) {
                                p.x = mx;
                                p.y = my;
                            }
                        }
                        if !bf {
                            if let Some(p) = sketch.point_mut(cb) {
                                p.x = mx;
                                p.y = my;
                            }
                        }
                    }
                }
            }
        }
        if max_err < 1e-6 {
            break;
        }
    }
    Ok(())
}

/// Intent Manager: Weak/Strong constraint management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintStrength {
    Weak,   // Auto-generated by Intent Manager
    Strong, // User-defined
}

impl Default for ConstraintStrength {
    fn default() -> Self {
        ConstraintStrength::Strong
    }
}

/// Which direction a linear dimension measures along.
///
/// An inclined line can be measured along itself, or projected on the sketch
/// axes; the user picks one when placing the dimension.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DimensionOrientation {
    /// Along the line itself.
    #[default]
    Aligned,
    /// Along the sketch X axis.
    Horizontal,
    /// Along the sketch Y axis.
    Vertical,
}

/// Constraint with strength for Intent Manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstrainedConstraint {
    pub constraint: Constraint,
    pub strength: ConstraintStrength,
    pub is_driving: bool, // For dimensions: true = driving (modifies geometry), false = driven (reference only)
    /// Where the user put the dimension's text, in sketch coordinates. None
    /// means the viewport picks the default offset.
    #[serde(default)]
    pub dimension_text_position: Option<[f64; 2]>,
    /// Which direction an inclined line's length is measured along.
    #[serde(default)]
    pub dimension_orientation: DimensionOrientation,
}

/// True when two constraints are about the same things, ignoring the value.
///
/// The Intent Manager record of a dimension is matched to the constraint by its
/// entities, never by its current number, so editing the number keeps them
/// together.
pub fn same_subject(left: &Constraint, right: &Constraint) -> bool {
    match (left, right) {
        (
            Constraint::Distance { a: a1, b: b1, .. }
            | Constraint::Angle { a: a1, b: b1, .. }
            | Constraint::LineDistance { a: a1, b: b1, .. }
            | Constraint::HorizontalDistance { a: a1, b: b1, .. }
            | Constraint::VerticalDistance { a: a1, b: b1, .. },
            Constraint::Distance { a: a2, b: b2, .. }
            | Constraint::Angle { a: a2, b: b2, .. }
            | Constraint::LineDistance { a: a2, b: b2, .. }
            | Constraint::HorizontalDistance { a: a2, b: b2, .. }
            | Constraint::VerticalDistance { a: a2, b: b2, .. },
        ) => (a1 == a2 && b1 == b2) || (a1 == b2 && b1 == a2),
        (
            Constraint::Radius { curve: c1, .. } | Constraint::Diameter { curve: c1, .. },
            Constraint::Radius { curve: c2, .. } | Constraint::Diameter { curve: c2, .. },
        ) => c1 == c2,
        (
            Constraint::PointLineDistance {
                point: p1,
                line: l1,
                ..
            },
            Constraint::PointLineDistance {
                point: p2,
                line: l2,
                ..
            },
        ) => p1 == p2 && l1 == l2,
        (
            Constraint::LineCurveDistance {
                line: l1,
                curve: c1,
                ..
            },
            Constraint::LineCurveDistance {
                line: l2,
                curve: c2,
                ..
            },
        ) => l1 == l2 && c1 == c2,
        _ => false,
    }
}

/// True when the constraint is a dimension: it has a value the user can edit,
/// a position for its text and a driving/driven flag.
pub fn is_dimension(constraint: &Constraint) -> bool {
    matches!(
        constraint,
        Constraint::Distance { .. }
            | Constraint::Angle { .. }
            | Constraint::LineDistance { .. }
            | Constraint::HorizontalDistance { .. }
            | Constraint::VerticalDistance { .. }
            | Constraint::Radius { .. }
            | Constraint::Diameter { .. }
            | Constraint::PointLineDistance { .. }
            | Constraint::LineCurveDistance { .. }
    )
}

/// Whether a sketch still has freedom left, has none, or asks for more than it
/// has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintStatus {
    UnderConstrained,
    FullyConstrained,
    OverConstrained,
}

impl ConstraintStatus {
    /// The words the interface shows for this status.
    pub fn as_str(self) -> &'static str {
        match self {
            ConstraintStatus::UnderConstrained => "under-constrained",
            ConstraintStatus::FullyConstrained => "fully-constrained",
            ConstraintStatus::OverConstrained => "over-constrained",
        }
    }
}

/// A rank-based analysis of a sketch's degrees of freedom.
///
/// The previous version counted constraints: two horizontals on the same line
/// counted twice, a self-reference on an arc endpoint counted two, and a
/// dimension that repeated another one counted as if it added freedom. Here the
/// constraint equations are differentiated with respect to everything the
/// solver can move (two coordinates per point, one radius per circle or arc)
/// and the rank of that Jacobian is what "constrained" means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DOFAnalysis {
    /// Variables the solver can move: two per point, one per radius.
    pub total_dof: usize,
    /// Independent equations: the rank of the constraint Jacobian.
    pub constrained_dof: usize,
    /// Freedom left: the variables the rank does not cover.
    pub free_dof: usize,
    /// Equations that add nothing to the rank: duplicates, or a constraint
    /// that conflicts with the ones already there.
    pub redundant_constraints: usize,
    pub status: ConstraintStatus,
}

/// The constraint equations of a sketch, read through a flat variable vector.
///
/// Variables are laid out as: `x, y` for each point in `points` order, then one
/// radius per circle, then one radius per arc. Everything the residuals need is
/// looked up by id, so the layout can be rearranged without changing them.
struct DofModel<'a> {
    sketch: &'a Sketch,
    point_index: std::collections::HashMap<Uuid, usize>,
    circle_index: std::collections::HashMap<Uuid, usize>,
    arc_index: std::collections::HashMap<Uuid, usize>,
}

impl<'a> DofModel<'a> {
    fn new(sketch: &'a Sketch) -> Self {
        let point_index = sketch
            .points
            .iter()
            .enumerate()
            .map(|(index, point)| (point.id, index))
            .collect();
        let circle_index = sketch
            .circles
            .iter()
            .enumerate()
            .map(|(index, circle)| (circle.id, index))
            .collect();
        let arc_index = sketch
            .arcs
            .iter()
            .enumerate()
            .map(|(index, arc)| (arc.id, index))
            .collect();
        Self {
            sketch,
            point_index,
            circle_index,
            arc_index,
        }
    }

    fn variable_count(&self) -> usize {
        self.sketch.points.len() * 2 + self.sketch.circles.len() + self.sketch.arcs.len()
    }

    fn variables(&self) -> Vec<f64> {
        let mut values = Vec::with_capacity(self.variable_count());
        for point in &self.sketch.points {
            values.push(point.x);
            values.push(point.y);
        }
        for circle in &self.sketch.circles {
            values.push(circle.radius);
        }
        for arc in &self.sketch.arcs {
            values.push(arc.radius);
        }
        values
    }

    fn point(&self, variables: &[f64], id: Uuid) -> Option<[f64; 2]> {
        let index = *self.point_index.get(&id)?;
        Some([variables[index * 2], variables[index * 2 + 1]])
    }

    fn radius(&self, variables: &[f64], id: Uuid) -> Option<f64> {
        let offset = self.sketch.points.len() * 2;
        if let Some(index) = self.circle_index.get(&id) {
            return Some(variables[offset + index]);
        }
        let arc_offset = offset + self.sketch.circles.len();
        self.arc_index
            .get(&id)
            .map(|index| variables[arc_offset + index])
    }

    fn line(&self, variables: &[f64], id: Uuid) -> Option<([f64; 2], [f64; 2])> {
        let line = self.sketch.lines.iter().find(|line| line.id == id)?;
        Some((self.point(variables, line.a)?, self.point(variables, line.b)?))
    }

    /// The centre point of a circle or an arc.
    fn curve_centre(&self, variables: &[f64], id: Uuid) -> Option<[f64; 2]> {
        let centre = self
            .sketch
            .circles
            .iter()
            .find(|circle| circle.id == id)
            .map(|circle| circle.center)
            .or_else(|| {
                self.sketch
                    .arcs
                    .iter()
                    .find(|arc| arc.id == id)
                    .map(|arc| arc.center)
            })?;
        self.point(variables, centre)
    }

    /// Every constraint equation, in a layout that depends only on which
    /// entities exist - never on their values - so the numeric Jacobian is
    /// consistent from row to row and column to column.
    ///
    /// Residuals are squared where a square root would otherwise make the
    /// derivative infinite at coincident geometry: the solution set is the same
    /// and the Jacobian stays finite.
    fn residuals(&self, variables: &[f64]) -> Vec<f64> {
        let mut rows = Vec::new();

        // A driven (reference) dimension measures without constraining: it is
        // not an equation at all, and counting it would make a sketch look
        // over-constrained because of a measurement the user asked for.
        let driven: Vec<Constraint> = self
            .sketch
            .constrained_constraints
            .iter()
            .filter(|record| !record.is_driving)
            .map(|record| record.constraint.clone())
            .collect();

        // A point the solver treats as fixed is pinned even when the constraint
        // list does not say so (the solver sets the flag when it meets Fixed).
        let fixed_by_constraint: std::collections::HashSet<Uuid> = self
            .sketch
            .constraints
            .iter()
            .filter_map(|constraint| match constraint {
                Constraint::Fixed { point } => Some(*point),
                _ => None,
            })
            .collect();
        for point in &self.sketch.points {
            if point.fixed && !fixed_by_constraint.contains(&point.id) {
                if let Some([x, y]) = self.point(variables, point.id) {
                    rows.push(x);
                    rows.push(y);
                }
            }
        }

        for constraint in &self.sketch.constraints {
            // A driven (reference) dimension measures without constraining.
            if is_dimension(constraint)
                && driven.iter().any(|subject| same_subject(subject, constraint))
            {
                continue;
            }
            match constraint {
                Constraint::Fixed { point } => {
                    if let Some([x, y]) = self.point(variables, *point) {
                        rows.push(x);
                        rows.push(y);
                    }
                }
                Constraint::Coincident { a, b } => {
                    // An arc endpoint is stored coincident with itself as a
                    // marker; it says nothing about the sketch.
                    if a == b {
                        continue;
                    }
                    if let (Some(pa), Some(pb)) =
                        (self.point(variables, *a), self.point(variables, *b))
                    {
                        rows.push(pa[0] - pb[0]);
                        rows.push(pa[1] - pb[1]);
                    }
                }
                Constraint::Distance { a, b, value } => {
                    if a == b {
                        continue;
                    }
                    if let (Some(pa), Some(pb)) =
                        (self.point(variables, *a), self.point(variables, *b))
                    {
                        let dx = pb[0] - pa[0];
                        let dy = pb[1] - pa[1];
                        rows.push(dx * dx + dy * dy - value * value);
                    }
                }
                Constraint::Horizontal { line } => {
                    if let Some((a, b)) = self.line(variables, *line) {
                        rows.push(a[1] - b[1]);
                    }
                }
                Constraint::Vertical { line } => {
                    if let Some((a, b)) = self.line(variables, *line) {
                        rows.push(a[0] - b[0]);
                    }
                }
                Constraint::Parallel { a, b } => {
                    if let (Some((a1, a2)), Some((b1, b2))) =
                        (self.line(variables, *a), self.line(variables, *b))
                    {
                        rows.push(
                            (a2[0] - a1[0]) * (b2[1] - b1[1])
                                - (a2[1] - a1[1]) * (b2[0] - b1[0]),
                        );
                    }
                }
                Constraint::Perpendicular { a, b } => {
                    if let (Some((a1, a2)), Some((b1, b2))) =
                        (self.line(variables, *a), self.line(variables, *b))
                    {
                        rows.push(
                            (a2[0] - a1[0]) * (b2[0] - b1[0])
                                + (a2[1] - a1[1]) * (b2[1] - b1[1]),
                        );
                    }
                }
                Constraint::Angle { a, b, value } => {
                    if let (Some((a1, a2)), Some((b1, b2))) =
                        (self.line(variables, *a), self.line(variables, *b))
                    {
                        let da = [a2[0] - a1[0], a2[1] - a1[1]];
                        let db = [b2[0] - b1[0], b2[1] - b1[1]];
                        let target = value.to_radians();
                        // sin(current - target) == 0, without the square root.
                        rows.push(
                            (da[0] * db[1] - da[1] * db[0]) * target.cos()
                                - (da[0] * db[0] + da[1] * db[1]) * target.sin(),
                        );
                    }
                }
                Constraint::HorizontalDistance { a, b, value } => {
                    if let (Some(pa), Some(pb)) =
                        (self.point(variables, *a), self.point(variables, *b))
                    {
                        let dx = pb[0] - pa[0];
                        rows.push(dx * dx - value * value);
                    }
                }
                Constraint::VerticalDistance { a, b, value } => {
                    if let (Some(pa), Some(pb)) =
                        (self.point(variables, *a), self.point(variables, *b))
                    {
                        let dy = pb[1] - pa[1];
                        rows.push(dy * dy - value * value);
                    }
                }
                Constraint::LineDistance { a, b, value } => {
                    if let (Some((a1, a2)), Some((b1, _))) =
                        (self.line(variables, *a), self.line(variables, *b))
                    {
                        let da = [a2[0] - a1[0], a2[1] - a1[1]];
                        let length_squared = da[0] * da[0] + da[1] * da[1];
                        if length_squared > 1e-18 {
                            let cross =
                                da[0] * (b1[1] - a1[1]) - da[1] * (b1[0] - a1[0]);
                            rows.push(cross * cross / length_squared - value * value);
                        }
                    }
                }

                Constraint::Radius { curve, value } => {
                    if let Some(radius) = self.radius(variables, *curve) {
                        rows.push(radius - value);
                    }
                }
                Constraint::Diameter { curve, value } => {
                    if let Some(radius) = self.radius(variables, *curve) {
                        rows.push(2.0 * radius - value);
                    }
                }
                Constraint::PointLineDistance { point, line, value } => {
                    if let (Some(p), Some((a, b))) = (
                        self.point(variables, *point),
                        self.line(variables, *line),
                    ) {
                        let da = [b[0] - a[0], b[1] - a[1]];
                        let length_squared = da[0] * da[0] + da[1] * da[1];
                        if length_squared > 1e-18 {
                            let cross = da[0] * (p[1] - a[1]) - da[1] * (p[0] - a[0]);
                            rows.push(cross * cross / length_squared - value * value);
                        }
                    }
                }
                Constraint::LineCurveDistance { line, curve, value } => {
                    if let (Some((a, b)), Some(centre), Some(radius)) = (
                        self.line(variables, *line),
                        self.curve_centre(variables, *curve),
                        self.radius(variables, *curve),
                    ) {
                        let da = [b[0] - a[0], b[1] - a[1]];
                        let length_squared = da[0] * da[0] + da[1] * da[1];
                        if length_squared > 1e-18 {
                            let cross = da[0] * (centre[1] - a[1]) - da[1] * (centre[0] - a[0]);
                            let wanted = value + radius;
                            rows.push(cross * cross / length_squared - wanted * wanted);
                        }
                    }
                }
                Constraint::EqualLength { a, b } => {
                    if let (Some((a1, a2)), Some((b1, b2))) =
                        (self.line(variables, *a), self.line(variables, *b))
                    {
                        let length_a = (a2[0] - a1[0]).hypot(a2[1] - a1[1]);
                        let length_b = (b2[0] - b1[0]).hypot(b2[1] - b1[1]);
                        rows.push(length_a - length_b);
                    }
                }
                Constraint::EqualRadius { a, b } => {
                    if let (Some(ra), Some(rb)) =
                        (self.radius(variables, *a), self.radius(variables, *b))
                    {
                        rows.push(ra - rb);
                    }
                }
                Constraint::Tangent { a, b } => {
                    // Line/curve tangency: the squared distance from the centre
                    // to the line equals the squared radius.
                    let curve = self
                        .radius(variables, *a)
                        .map(|radius| (*a, radius))
                        .or_else(|| self.radius(variables, *b).map(|radius| (*b, radius)));
                    let line = self
                        .line(variables, *a)
                        .or_else(|| self.line(variables, *b));
                    if let (Some((curve_id, radius)), Some((p1, p2))) = (curve, line) {
                        if let Some(centre) = self.curve_centre(variables, curve_id) {
                            let dx = p2[0] - p1[0];
                            let dy = p2[1] - p1[1];
                            let length_squared = dx * dx + dy * dy;
                            if length_squared > 1e-18 {
                                let cross = (centre[0] - p1[0]) * -dy + (centre[1] - p1[1]) * dx;
                                rows.push(cross * cross / length_squared - radius * radius);
                            }
                        }
                    }
                }
                Constraint::Symmetric { a, b, axis_line } => {
                    if let (Some(pa), Some(pb), Some((p1, p2))) = (
                        self.point(variables, *a),
                        self.point(variables, *b),
                        self.line(variables, *axis_line),
                    ) {
                        let dx = p2[0] - p1[0];
                        let dy = p2[1] - p1[1];
                        let midpoint = [
                            (pa[0] + pb[0]) * 0.5 - p1[0],
                            (pa[1] + pb[1]) * 0.5 - p1[1],
                        ];
                        // The midpoint lies on the axis...
                        rows.push(dx * midpoint[1] - dy * midpoint[0]);
                        // ...and the pair is perpendicular to it.
                        rows.push(dx * (pb[0] - pa[0]) + dy * (pb[1] - pa[1]));
                    }
                }
                Constraint::Midpoint { point, line } => {
                    if let (Some(point), Some((p1, p2))) =
                        (self.point(variables, *point), self.line(variables, *line))
                    {
                        rows.push(2.0 * point[0] - p1[0] - p2[0]);
                        rows.push(2.0 * point[1] - p1[1] - p2[1]);
                    }
                }
                Constraint::Concentric { a, b } => {
                    if let (Some(ca), Some(cb)) =
                        (self.curve_centre(variables, *a), self.curve_centre(variables, *b))
                    {
                        rows.push(ca[0] - cb[0]);
                        rows.push(ca[1] - cb[1]);
                    }
                }
            }
        }
        rows
    }
}

/// The rank of a matrix, by Gaussian elimination with partial pivoting.
fn matrix_rank(matrix: &[f64], rows: usize, columns: usize) -> usize {
    let tolerance = 1e-6;
    let mut matrix = matrix.to_vec();
    let mut pivot_row = 0;
    for column in 0..columns {
        if pivot_row >= rows {
            break;
        }
        let mut best = pivot_row;
        let mut best_value = matrix[pivot_row * columns + column].abs();
        for candidate in pivot_row + 1..rows {
            let value = matrix[candidate * columns + column].abs();
            if value > best_value {
                best_value = value;
                best = candidate;
            }
        }
        if best_value <= tolerance {
            continue;
        }
        if best != pivot_row {
            for c in 0..columns {
                matrix.swap(pivot_row * columns + c, best * columns + c);
            }
        }
        let pivot = matrix[pivot_row * columns + column];
        for below in pivot_row + 1..rows {
            let factor = matrix[below * columns + column] / pivot;
            if factor == 0.0 {
                continue;
            }
            for c in column..columns {
                matrix[below * columns + c] -= factor * matrix[pivot_row * columns + c];
            }
        }
        pivot_row += 1;
    }
    pivot_row
}

impl Sketch {
    /// The degrees of freedom the constraints actually leave, by rank.
    pub fn analyze_dof_detailed(&self) -> DOFAnalysis {
        let model = DofModel::new(self);
        let variables = model.variables();
        let variable_count = model.variable_count();
        let base = model.residuals(&variables);
        let equation_count = base.len();

        // A central difference is exact for the quadratic equations most of
        // these constraints are, and accurate enough for the rest.
        const EPSILON: f64 = 1e-6;
        let mut jacobian = vec![0.0; equation_count * variable_count];
        for column in 0..variable_count {
            let mut forward = variables.clone();
            forward[column] += EPSILON;
            let mut backward = variables.clone();
            backward[column] -= EPSILON;
            let plus = model.residuals(&forward);
            let minus = model.residuals(&backward);
            for row in 0..equation_count {
                jacobian[row * variable_count + column] =
                    (plus[row] - minus[row]) / (2.0 * EPSILON);
            }
        }

        // Scale each equation to its own size before ranking it, so a squared
        // distance and a horizontal are weighed the same.
        for row in 0..equation_count {
            let start = row * variable_count;
            let largest = jacobian[start..start + variable_count]
                .iter()
                .fold(0.0f64, |largest, value| largest.max(value.abs()));
            if largest > 0.0 {
                for value in &mut jacobian[start..start + variable_count] {
                    *value /= largest;
                }
            }
        }

        let rank = matrix_rank(&jacobian, equation_count, variable_count);
        let free_dof = variable_count.saturating_sub(rank);
        let redundant_constraints = equation_count.saturating_sub(rank);
        let status = if redundant_constraints > 0 {
            ConstraintStatus::OverConstrained
        } else if free_dof == 0 {
            ConstraintStatus::FullyConstrained
        } else {
            ConstraintStatus::UnderConstrained
        };
        DOFAnalysis {
            total_dof: variable_count,
            constrained_dof: rank,
            free_dof,
            redundant_constraints,
            status,
        }
    }

    /// Analyze degrees of freedom in the sketch
    /// Returns (total_dof, constrained_dof, redundant_constraints)
    pub fn analyze_dof(&self) -> (usize, usize, usize) {
        let analysis = self.analyze_dof_detailed();
        (
            analysis.total_dof,
            analysis.constrained_dof,
            analysis.redundant_constraints,
        )
    }

    /// Get sketch status: "under-constrained", "fully-constrained", "over-constrained"
    /// The points nothing anchors: the ones the user can still drag around.
    ///
    /// A point is anchored when it is Fixed, when it coincides with one, or when
    /// it is the midpoint of a line whose two ends are anchored. Horizontal,
    /// Vertical, Distance, Parallel and the rest take freedom away without
    /// saying where the point goes, so they do not anchor it.
    pub fn anchored_points(&self) -> std::collections::HashSet<Uuid> {
        let mut held: std::collections::HashSet<Uuid> = self
            .constraints
            .iter()
            .filter_map(|constraint| match constraint {
                Constraint::Fixed { point } => Some(*point),
                _ => None,
            })
            .collect();
        let mut changed = true;
        while changed {
            changed = false;
            for constraint in &self.constraints {
                match constraint {
                    Constraint::Coincident { a, b } => {
                        if held.contains(a) && held.insert(*b) {
                            changed = true;
                        }
                        if held.contains(b) && held.insert(*a) {
                            changed = true;
                        }
                    }
                    Constraint::Midpoint { point, line } => {
                        if let Some(line) = self.lines.iter().find(|candidate| candidate.id == *line)
                        {
                            if held.contains(&line.a)
                                && held.contains(&line.b)
                                && held.insert(*point)
                            {
                                changed = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        held
    }

    /// The entities that still have a degree of freedom, for the viewport
    /// highlight: a point nothing anchors, a line with a free end, a curve whose
    /// centre is free or whose radius no dimension fixes, a spline or a conic
    /// with a free control point.
    pub fn find_unconstrained_entities(&self) -> Vec<Uuid> {
        let held = self.anchored_points();
        let free = |id: Uuid| !held.contains(&id);
        let mut entities = Vec::new();
        for point in &self.points {
            if free(point.id) {
                entities.push(point.id);
            }
        }
        for line in &self.lines {
            if free(line.a) || free(line.b) {
                entities.push(line.id);
            }
        }
        for circle in &self.circles {
            if free(circle.center) || !self.radius_is_dimensioned(circle.id) {
                entities.push(circle.id);
            }
        }
        for arc in &self.arcs {
            if free(arc.center) || !self.radius_is_dimensioned(arc.id) {
                entities.push(arc.id);
            }
        }
        for spline in &self.splines {
            if spline.control_points.iter().copied().any(free) {
                entities.push(spline.id);
            }
        }
        for conic in &self.conics {
            if [conic.start, conic.control, conic.end]
                .into_iter()
                .any(free)
            {
                entities.push(conic.id);
            }
        }
        entities
    }

    /// Whether a dimension fixes this curve's radius.
    fn radius_is_dimensioned(&self, curve: Uuid) -> bool {
        self.constraints.iter().any(|constraint| match constraint {
            Constraint::Radius { curve: other, .. } | Constraint::Diameter { curve: other, .. } => {
                *other == curve
            }
            Constraint::EqualRadius { a, b } => *a == curve || *b == curve,
            _ => false,
        })
    }

    pub fn constraint_status(&self) -> &'static str {
        self.analyze_dof_detailed().status.as_str()
    }
    
    /// Auto-apply weak constraints to make sketch fully constrained (Intent Manager)
    pub fn apply_weak_constraints(&mut self) {
        let (total, constrained, _) = self.analyze_dof();
        if constrained >= total {
            return; // Already fully constrained
        }
        
        // Find unconstrained points and add weak fixed constraints
        let mut constrained_points = std::collections::HashSet::new();
        for constraint in &self.constraints {
            match constraint {
                Constraint::Fixed { point } => {
                    constrained_points.insert(*point);
                }
                Constraint::Coincident { a, .. } => {
                    constrained_points.insert(*a);
                }
                Constraint::Distance { a, b, .. } => {
                    constrained_points.insert(*a);
                    constrained_points.insert(*b);
                }
                Constraint::Horizontal { line } => {
                    if let Some(l) = self.lines.iter().find(|l| l.id == *line) {
                        constrained_points.insert(l.a);
                        constrained_points.insert(l.b);
                    }
                }
                Constraint::Vertical { line } => {
                    if let Some(l) = self.lines.iter().find(|l| l.id == *line) {
                        constrained_points.insert(l.a);
                        constrained_points.insert(l.b);
                    }
                }
                _ => {}
            }
        }
        
        // Add weak fixed constraints to unconstrained points
        for point in &self.points {
            if !constrained_points.contains(&point.id) && !point.fixed {
                self.constraints.push(Constraint::Fixed { point: point.id });
                // In a full implementation, we'd track strength separately
            }
        }
    }

    /// Add a dimension constraint with Intent Manager support (driving vs driven)
    pub fn add_dimension(
        &mut self,
        a: Uuid,
        b: Uuid,
        value: f64,
        driving: bool,
        strong: bool,
    ) -> Uuid {
        self.add_dimension_constraint(
            Constraint::Distance { a, b, value },
            driving,
            strong,
            None,
            DimensionOrientation::default(),
        );
        Uuid::new_v4()
    }

    /// Add any dimension constraint (distance, angle or line distance) with its
    /// Intent Manager record and its on-screen placement.
    pub fn add_dimension_constraint(
        &mut self,
        constraint: Constraint,
        driving: bool,
        strong: bool,
        text_position: Option<[f64; 2]>,
        orientation: DimensionOrientation,
    ) -> bool {
        if !is_dimension(&constraint) {
            return false;
        }
        self.constraints.push(constraint.clone());
        self.constrained_constraints.push(ConstrainedConstraint {
            constraint,
            strength: if strong {
                ConstraintStrength::Strong
            } else {
                ConstraintStrength::Weak
            },
            is_driving: driving,
            dimension_text_position: text_position,
            dimension_orientation: orientation,
        });
        true
    }

    /// Promote a weak constraint to strong (user explicitly adds dimension)
    pub fn promote_to_strong(&mut self, constraint_id: Uuid) -> bool {
        if let Some(cc) = self
            .constrained_constraints
            .iter_mut()
            .find(|c| c.constraint_id() == Some(constraint_id))
        {
            cc.strength = ConstraintStrength::Strong;
            return true;
        }
        false
    }

    /// Get all weak constraints that could be promoted
    pub fn weak_constraints(&self) -> Vec<&ConstrainedConstraint> {
        self.constrained_constraints
            .iter()
            .filter(|c| c.strength == ConstraintStrength::Weak)
            .collect()
    }

    /// Get all driving dimensions
    pub fn driving_dimensions(&self) -> Vec<&ConstrainedConstraint> {
        self.constrained_constraints
            .iter()
            .filter(|c| c.is_driving)
            .collect()
    }

    /// Get all driven dimensions (reference only)
    pub fn driven_dimensions(&self) -> Vec<&ConstrainedConstraint> {
        self.constrained_constraints
            .iter()
            .filter(|c| !c.is_driving)
            .collect()
    }

    /// The constraint behind a dimension index, when it is a dimension.
    pub fn dimension_constraint(&self, index: usize) -> Option<&Constraint> {
        let constraint = self.constraints.get(index)?;
        is_dimension(constraint).then_some(constraint)
    }

    /// The Intent Manager record of the dimension at `index`.
    pub fn dimension_record(&self, index: usize) -> Option<&ConstrainedConstraint> {
        let constraint = self.dimension_constraint(index)?;
        let position = self.record_index(constraint)?;
        self.constrained_constraints.get(position)
    }

    /// Where the record of this constraint is, matched by its subjects.
    fn record_index(&self, constraint: &Constraint) -> Option<usize> {
        self.constrained_constraints
            .iter()
            .position(|record| same_subject(&record.constraint, constraint))
    }

    /// The value the dimension at `index` holds.
    pub fn dimension_value(&self, index: usize) -> Option<f64> {
        match self.dimension_constraint(index)? {
            Constraint::Distance { value, .. }
            | Constraint::Angle { value, .. }
            | Constraint::LineDistance { value, .. }
            | Constraint::HorizontalDistance { value, .. }
            | Constraint::VerticalDistance { value, .. }
            | Constraint::Radius { value, .. }
            | Constraint::Diameter { value, .. }
            | Constraint::PointLineDistance { value, .. }
            | Constraint::LineCurveDistance { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// Whether the dimension at `index` drives the geometry or only reports it.
    /// A dimension with no Intent Manager record is a driving one.
    pub fn dimension_is_driving(&self, index: usize) -> Option<bool> {
        self.dimension_constraint(index)?;
        Some(
            self.dimension_record(index)
                .map(|record| record.is_driving)
                .unwrap_or(true),
        )
    }

    /// Is the dimension at `index` weak (put there by the Intent Manager)?
    pub fn dimension_is_weak(&self, index: usize) -> bool {
        self.dimension_record(index)
            .map(|record| record.strength == ConstraintStrength::Weak)
            .unwrap_or(false)
    }

    /// Where the dimension's text was placed, when the user placed it.
    pub fn dimension_text_position(&self, index: usize) -> Option<[f64; 2]> {
        self.dimension_record(index)?.dimension_text_position
    }

    /// Which direction the dimension measures along.
    pub fn dimension_orientation(&self, index: usize) -> DimensionOrientation {
        self.dimension_record(index)
            .map(|record| record.dimension_orientation)
            .unwrap_or_default()
    }

    /// Move a dimension's text and change the direction it measures along.
    pub fn set_dimension_layout(
        &mut self,
        index: usize,
        text_position: Option<[f64; 2]>,
        orientation: DimensionOrientation,
    ) -> bool {
        let Some(constraint) = self.dimension_constraint(index).cloned() else {
            return false;
        };
        match self.record_index(&constraint) {
            Some(position) => {
                self.constrained_constraints[position].dimension_text_position = text_position;
                self.constrained_constraints[position].dimension_orientation = orientation;
            }
            None => self.constrained_constraints.push(ConstrainedConstraint {
                constraint,
                strength: ConstraintStrength::Strong,
                is_driving: true,
                dimension_text_position: text_position,
                dimension_orientation: orientation,
            }),
        }
        true
    }

    /// Change a dimension's value, keeping its Intent Manager record in step.
    pub fn set_dimension_value(&mut self, index: usize, value: f64) -> bool {
        let Some(constraint) = self.dimension_constraint(index).cloned() else {
            return false;
        };
        match self.constraints.get_mut(index) {
            Some(
                Constraint::Distance { value: current, .. }
                | Constraint::Angle { value: current, .. }
                | Constraint::LineDistance { value: current, .. }
                | Constraint::HorizontalDistance { value: current, .. }
                | Constraint::VerticalDistance { value: current, .. }
                | Constraint::Radius { value: current, .. }
                | Constraint::Diameter { value: current, .. }
                | Constraint::PointLineDistance { value: current, .. }
                | Constraint::LineCurveDistance { value: current, .. },
            ) => *current = value,
            _ => return false,
        }
        if let Some(position) = self.record_index(&constraint) {
            match &mut self.constrained_constraints[position].constraint {
                Constraint::Distance { value: recorded, .. }
                | Constraint::Angle { value: recorded, .. }
                | Constraint::LineDistance { value: recorded, .. }
                | Constraint::HorizontalDistance { value: recorded, .. }
                | Constraint::VerticalDistance { value: recorded, .. }
                | Constraint::Radius { value: recorded, .. }
                | Constraint::Diameter { value: recorded, .. }
                | Constraint::PointLineDistance { value: recorded, .. }
                | Constraint::LineCurveDistance { value: recorded, .. } => *recorded = value,
                _ => {}
            }
        }
        true
    }

    /// Toggle a dimension between driving the geometry and merely reporting it.
    ///
    /// A driven dimension is skipped by the solver, so its value stays a
    /// reference measurement instead of moving the geometry.
    pub fn set_dimension_driving(&mut self, index: usize, driving: bool) -> bool {
        let Some(constraint) = self.dimension_constraint(index).cloned() else {
            return false;
        };
        match self.record_index(&constraint) {
            Some(position) => self.constrained_constraints[position].is_driving = driving,
            None => self.constrained_constraints.push(ConstrainedConstraint {
                constraint,
                strength: ConstraintStrength::Strong,
                is_driving: driving,
                dimension_text_position: None,
                dimension_orientation: DimensionOrientation::default(),
            }),
        }
        true
    }

    /// Promote a weak dimension to a strong one. Returns true when it changed.
    pub fn promote_dimension(&mut self, index: usize) -> bool {
        let Some(constraint) = self.dimension_constraint(index).cloned() else {
            return false;
        };
        match self.record_index(&constraint) {
            Some(position) => {
                if self.constrained_constraints[position].strength == ConstraintStrength::Strong {
                    return false;
                }
                self.constrained_constraints[position].strength = ConstraintStrength::Strong;
                true
            }
            None => {
                self.constrained_constraints.push(ConstrainedConstraint {
                    constraint,
                    strength: ConstraintStrength::Strong,
                    is_driving: true,
                    dimension_text_position: None,
                    dimension_orientation: DimensionOrientation::default(),
                });
                true
            }
        }
    }

    /// Remove the constraint at `index` and the record that belongs to it.
    pub fn remove_constraint(&mut self, index: usize) -> bool {
        if index >= self.constraints.len() {
            return false;
        }
        let removed = self.constraints.remove(index);
        if is_dimension(&removed) {
            if let Some(position) = self.record_index(&removed) {
                self.constrained_constraints.remove(position);
            }
        }
        true
    }

    /// Trim a line back to its nearest intersection with another entity.
    ///
    /// Returns `Err(SketchError::Unknown)` when `entity_id` is not a line, and
    /// an explicit error when nothing crosses the segment. The previous version
    /// silently returned `Ok(())` in both cases, so a failed trim looked like a
    /// successful one.
    pub fn trim_entity(&mut self, entity_id: Uuid, trim_point: [f64; 2]) -> Result<(), SketchError> {
        let line = self
            .lines
            .iter()
            .find(|l| l.id == entity_id)
            .cloned()
            .ok_or(SketchError::Unknown(entity_id))?;
        let (pa, pb) = match (self.point(line.a), self.point(line.b)) {
            (Some(a), Some(b)) => ([a.x, a.y], [b.x, b.y]),
            _ => return Err(SketchError::Unknown(entity_id)),
        };

        // Every point where this segment crosses another line, circle or arc.
        let mut hits: Vec<[f64; 2]> = Vec::new();
        for other in &self.lines {
            if other.id == entity_id {
                continue;
            }
            if let (Some(c), Some(d)) = (self.point(other.a), self.point(other.b)) {
                if let Some(t) = segment_segment_t(pa, pb, [c.x, c.y], [d.x, d.y]) {
                    hits.push(lerp2(pa, pb, t));
                }
            }
        }
        let round_entities: Vec<([f64; 2], f64)> = self
            .circles
            .iter()
            .filter_map(|c| self.point(c.center).map(|p| ([p.x, p.y], c.radius)))
            .chain(
                self.arcs
                    .iter()
                    .filter_map(|a| self.point(a.center).map(|p| ([p.x, p.y], a.radius))),
            )
            .collect();
        for (center, radius) in round_entities {
            for t in segment_circle_ts(pa, pb, center, radius) {
                hits.push(lerp2(pa, pb, t));
            }
        }

        let target = match hits.into_iter().min_by(|p, q| {
            dist_sq(*p, trim_point)
                .partial_cmp(&dist_sq(*q, trim_point))
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Some(p) => p,
            None => {
                return Err(SketchError::Solver(
                    "trim: the segment does not cross any other entity".into(),
                ))
            }
        };

        // Shorten from the end the user clicked on.
        let endpoint = if dist_sq(pa, trim_point) <= dist_sq(pb, trim_point) {
            line.a
        } else {
            line.b
        };
        if let Some(p) = self.point_mut(endpoint) {
            p.x = target[0];
            p.y = target[1];
        }
        Ok(())
    }

    /// Extend a line until it meets `target_id`.
    ///
    /// Only intersections *outside* the current segment are used, so this always
    /// grows the line. The previous implementation ignored `target_id` entirely
    /// and moved both endpoints outwards by a fixed 10 units.
    pub fn extend_entity(&mut self, entity_id: Uuid, target_id: Uuid) -> Result<(), SketchError> {
        let line = self
            .lines
            .iter()
            .find(|l| l.id == entity_id)
            .cloned()
            .ok_or(SketchError::Unknown(entity_id))?;
        let (pa, pb) = match (self.point(line.a), self.point(line.b)) {
            (Some(a), Some(b)) => ([a.x, a.y], [b.x, b.y]),
            _ => return Err(SketchError::Unknown(entity_id)),
        };

        // (which endpoint to move, where to move it)
        let mut candidates: Vec<(usize, [f64; 2])> = Vec::new();
        let push_ts = |line_ts: Vec<f64>, candidates: &mut Vec<(usize, [f64; 2])>| {
            for t in line_ts {
                if t > 1.0 {
                    candidates.push((1, lerp2(pa, pb, t)));
                } else if t < 0.0 {
                    candidates.push((0, lerp2(pa, pb, t)));
                }
            }
        };

        if let Some(target) = self.lines.iter().find(|l| l.id == target_id) {
            if let (Some(c), Some(d)) = (self.point(target.a), self.point(target.b)) {
                let ts: Vec<f64> = line_line_t(pa, pb, [c.x, c.y], [d.x, d.y])
                    .into_iter()
                    .collect();
                push_ts(ts, &mut candidates);
            }
        } else if let Some(circle) = self.circles.iter().find(|c| c.id == target_id) {
            if let Some(c) = self.point(circle.center) {
                let ts = line_circle_ts(pa, pb, [c.x, c.y], circle.radius);
                push_ts(ts, &mut candidates);
            }
        } else if let Some(arc) = self.arcs.iter().find(|a| a.id == target_id) {
            if let Some(c) = self.point(arc.center) {
                let ts = line_circle_ts(pa, pb, [c.x, c.y], arc.radius);
                push_ts(ts, &mut candidates);
            }
        } else {
            return Err(SketchError::Unknown(target_id));
        }

        // Grow by the smallest amount needed.
        let best = candidates.into_iter().min_by(|(_, p), (_, q)| {
            dist_sq(pa, *p)
                .min(dist_sq(pb, *p))
                .partial_cmp(&dist_sq(pa, *q).min(dist_sq(pb, *q)))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        match best {
            Some((which, p)) => {
                let id = if which == 0 { line.a } else { line.b };
                if let Some(pt) = self.point_mut(id) {
                    pt.x = p[0];
                    pt.y = p[1];
                }
                Ok(())
            }
            None => Err(SketchError::Solver(
                "extend: the line does not reach the target entity".into(),
            )),
        }
    }

    /// Offset an entity by a distance
    pub fn offset_entity(&mut self, entity_id: Uuid, distance: f64) -> Result<Uuid, SketchError> {
        // First collect line data
        let line_data = self.lines.iter()
            .find(|l| l.id == entity_id)
            .and_then(|line| {
                Some((
                    line.a, line.b,
                    self.point(line.a)?.clone(),
                    self.point(line.b)?.clone(),
                ))
            });
        
        if let Some((_pt_a, _pt_b, pa, pb)) = line_data {
            let dir = Vec2::new(pb.x as f32 - pa.x as f32, pb.y as f32 - pa.y as f32);
            let len = dir.length().max(1e-6);
            // Normal perpendicular to line
            let normal = Vec2::new(-dir.y / len, dir.x / len);
            
            let new_a = self.add_point(
                pa.x + normal.x as f64 * distance,
                pa.y + normal.y as f64 * distance,
            );
            let new_b = self.add_point(
                pb.x + normal.x as f64 * distance,
                pb.y + normal.y as f64 * distance,
            );
            
            let new_line = self.add_line(new_a, new_b);
            return Ok(new_line);
        }
        Err(SketchError::Unknown(entity_id))
    }

    /// Duplicate entities through a 2D transform, returning the new entity ids.
    ///
    /// Handles points, lines, circles and arcs. The previous mirror/pattern
    /// helpers only looked at point ids and silently returned an empty vector
    /// for lines, circles and arcs.
    pub fn duplicate_entities<F>(&mut self, entity_ids: &[Uuid], transform: F) -> Vec<Uuid>
    where
        F: Fn([f64; 2]) -> [f64; 2],
    {
        enum Source {
            Point([f64; 2]),
            Line([f64; 2], [f64; 2]),
            Circle([f64; 2], f64),
            /// centre, radius, start point, end point
            Arc([f64; 2], f64, [f64; 2], [f64; 2]),
        }

        // Snapshot the sources before mutating the sketch.
        let mut sources: Vec<Source> = Vec::new();
        for id in entity_ids {
            if let Some(p) = self.point(*id) {
                sources.push(Source::Point([p.x, p.y]));
            } else if let Some(l) = self.lines.iter().find(|l| l.id == *id) {
                if let (Some(a), Some(b)) = (self.point(l.a), self.point(l.b)) {
                    sources.push(Source::Line([a.x, a.y], [b.x, b.y]));
                }
            } else if let Some(c) = self.circles.iter().find(|c| c.id == *id) {
                if let Some(center) = self.point(c.center) {
                    sources.push(Source::Circle([center.x, center.y], c.radius));
                }
            } else if let Some(a) = self.arcs.iter().find(|a| a.id == *id) {
                if let (Some(center), Some(start), Some(end)) =
                    (self.point(a.center), self.point(a.start_point), self.point(a.end_point))
                {
                    sources.push(Source::Arc(
                        [center.x, center.y],
                        a.radius,
                        [start.x, start.y],
                        [end.x, end.y],
                    ));
                }
            }
        }

        let mut new_ids = Vec::new();
        for source in sources {
            match source {
                Source::Point(p) => {
                    let q = transform(p);
                    new_ids.push(self.add_point(q[0], q[1]));
                }
                Source::Line(a, b) => {
                    let qa = transform(a);
                    let qb = transform(b);
                    let na = self.add_point(qa[0], qa[1]);
                    let nb = self.add_point(qb[0], qb[1]);
                    new_ids.push(self.add_line(na, nb));
                }
                Source::Circle(center, radius) => {
                    let q = transform(center);
                    new_ids.push(self.add_circle(q[0], q[1], radius));
                }
                Source::Arc(center, radius, start, end) => {
                    let qc = transform(center);
                    let qs = transform(start);
                    let qe = transform(end);
                    let a0 = (qs[1] - qc[1]).atan2(qs[0] - qc[0]);
                    let a1 = (qe[1] - qc[1]).atan2(qe[0] - qc[0]);
                    new_ids.push(self.add_arc(qc[0], qc[1], radius, a0, a1));
                }
            }
        }
        new_ids
    }

    /// Mirror entities across a line (axis).
    pub fn mirror_entities(&mut self, entity_ids: &[Uuid], axis_line_id: Uuid) -> Result<Vec<Uuid>, SketchError> {
        let axis = self
            .lines
            .iter()
            .find(|l| l.id == axis_line_id)
            .cloned()
            .ok_or(SketchError::Unknown(axis_line_id))?;
        let (p1, p2) = match (self.point(axis.a), self.point(axis.b)) {
            (Some(a), Some(b)) => ([a.x, a.y], [b.x, b.y]),
            _ => return Err(SketchError::Unknown(axis_line_id)),
        };
        let dx = p2[0] - p1[0];
        let dy = p2[1] - p1[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-12 {
            return Err(SketchError::Solver("mirror axis has zero length".into()));
        }
        let (ux, uy) = (dx / len, dy / len);
        let origin = p1;
        Ok(self.duplicate_entities(entity_ids, move |p| {
            let vx = p[0] - origin[0];
            let vy = p[1] - origin[1];
            let proj = vx * ux + vy * uy;
            let perp_x = vx - proj * ux;
            let perp_y = vy - proj * uy;
            [
                origin[0] + proj * ux - perp_x,
                origin[1] + proj * uy - perp_y,
            ]
        }))
    }

    /// Create a linear pattern of entities (points, lines, circles and arcs).
    pub fn linear_pattern(
        &mut self,
        entity_ids: &[Uuid],
        direction: [f64; 2],
        spacing: f64,
        count: usize,
    ) -> Result<Vec<Uuid>, SketchError> {
        let len = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
        if len < 1e-12 {
            return Err(SketchError::Solver("linear pattern direction is zero".into()));
        }
        let (ux, uy) = (direction[0] / len, direction[1] / len);
        let mut new_ids = Vec::new();
        for i in 1..count {
            let d = spacing * i as f64;
            let ids = self.duplicate_entities(entity_ids, move |p| [p[0] + ux * d, p[1] + uy * d]);
            new_ids.extend(ids);
        }
        Ok(new_ids)
    }

    /// Create a circular pattern of entities (points, lines, circles and arcs).
    pub fn circular_pattern(
        &mut self,
        entity_ids: &[Uuid],
        center: [f64; 2],
        angle_step: f64,
        count: usize,
    ) -> Result<Vec<Uuid>, SketchError> {
        let mut new_ids = Vec::new();
        for i in 1..count {
            let angle = angle_step * i as f64;
            let (sin_a, cos_a) = angle.sin_cos();
            let ids = self.duplicate_entities(entity_ids, move |p| {
                let dx = p[0] - center[0];
                let dy = p[1] - center[1];
                [
                    center[0] + dx * cos_a - dy * sin_a,
                    center[1] + dx * sin_a + dy * cos_a,
                ]
            });
            new_ids.extend(ids);
        }
        Ok(new_ids)
    }
}

impl ConstrainedConstraint {
    /// Try to extract constraint ID (for distance constraints)
    fn constraint_id(&self) -> Option<Uuid> {
        match &self.constraint {
            Constraint::Distance { .. } => Some(Uuid::nil()), // Placeholder - would need proper ID tracking
            _ => None,
        }
    }
}


mod import;

#[cfg(feature = "solvespace")]
mod solvespace {
    use super::*;
    
    /// Placeholder for SolveSpace integration.
    /// The `slvs` crate currently has build issues with mimalloc dependency.
    /// This will be implemented once the dependency is resolved.
    pub fn solve(_sketch: &mut Sketch) -> Result<(), SketchError> {
        Err(SketchError::Solver(
            "slvs feature enabled but adapter not yet complete (slvs crate has build issues)".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_solves_axis_aligned() {
        let mut s = Sketch::new("r");
        s.add_rectangle(0.0, 0.0, 10.0, 5.0);
        // Nudge a point off-axis
        s.points[1].y += 0.5;
        s.solve().unwrap();
        assert!((s.points[0].y - s.points[1].y).abs() < 1e-3);
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn linear_pattern_copies_lines_and_circles() {
        let mut sketch = Sketch::new("p");
        let p0 = sketch.add_point(0.0, 0.0);
        let p1 = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(p0, p1);
        let circle = sketch.add_circle(5.0, 5.0, 2.0);

        let created = sketch
            .linear_pattern(&[line, circle], [1.0, 0.0], 20.0, 3)
            .unwrap();

        // Two extra copies of one line and one circle each.
        assert_eq!(created.len(), 4);
        assert_eq!(sketch.lines.len(), 3);
        assert_eq!(sketch.circles.len(), 3);

        // The last copy of the line is offset by 40 in x.
        let last = sketch.lines.last().unwrap();
        let a = sketch.point(last.a).unwrap();
        assert!((a.x - 40.0).abs() < 1e-9, "unexpected x: {}", a.x);
    }

    #[test]
    fn circular_pattern_rotates_geometry() {
        let mut sketch = Sketch::new("c");
        let p0 = sketch.add_point(10.0, 0.0);
        let p1 = sketch.add_point(20.0, 0.0);
        let line = sketch.add_line(p0, p1);

        let created = sketch
            .circular_pattern(&[line], [0.0, 0.0], std::f64::consts::FRAC_PI_2, 4)
            .unwrap();
        assert_eq!(created.len(), 3);

        // The first copy is rotated 90 degrees onto the +Y axis.
        let first = sketch.lines.iter().find(|l| l.id == created[0]).unwrap();
        let a = sketch.point(first.a).unwrap();
        assert!(
            a.x.abs() < 1e-9 && (a.y - 10.0).abs() < 1e-9,
            "unexpected rotated point: ({}, {})",
            a.x,
            a.y
        );
    }

    #[test]
    fn mirror_copies_circles_and_arcs() {
        let mut sketch = Sketch::new("m");
        let a0 = sketch.add_point(0.0, -10.0);
        let a1 = sketch.add_point(0.0, 10.0);
        let axis = sketch.add_line(a0, a1);
        let circle = sketch.add_circle(5.0, 0.0, 2.0);
        let arc = sketch.add_arc(3.0, 0.0, 1.0, 0.0, std::f64::consts::PI);

        let created = sketch.mirror_entities(&[circle, arc], axis).unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(sketch.circles.len(), 2);
        assert_eq!(sketch.arcs.len(), 2);

        let mirrored = sketch.circles.last().unwrap();
        let center = sketch.point(mirrored.center).unwrap();
        assert!(
            (center.x + 5.0).abs() < 1e-9,
            "mirrored centre x = {}",
            center.x
        );
    }

    #[test]
    fn extend_moves_the_line_to_the_target() {
        let mut sketch = Sketch::new("e");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(5.0, 0.0);
        let line = sketch.add_line(a, b);
        let c = sketch.add_point(12.0, -5.0);
        let d = sketch.add_point(12.0, 5.0);
        let target = sketch.add_line(c, d);

        sketch.extend_entity(line, target).unwrap();

        assert!((sketch.point(b).unwrap().x - 12.0).abs() < 1e-9);
        assert!(sketch.point(a).unwrap().x.abs() < 1e-9, "wrong endpoint moved");
    }

    #[test]
    fn extend_reports_entities_that_never_meet() {
        let mut sketch = Sketch::new("e2");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(5.0, 0.0);
        let line = sketch.add_line(a, b);
        let c = sketch.add_point(0.0, 5.0);
        let d = sketch.add_point(5.0, 5.0);
        let parallel = sketch.add_line(c, d);
        assert!(sketch.extend_entity(line, parallel).is_err());
    }

    #[test]
    fn trim_shortens_to_the_nearest_intersection() {
        let mut sketch = Sketch::new("t");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(a, b);
        let c = sketch.add_point(4.0, -5.0);
        let d = sketch.add_point(4.0, 5.0);
        sketch.add_line(c, d);

        // Click near the right end to remove that portion.
        sketch.trim_entity(line, [9.0, 0.0]).unwrap();

        assert!((sketch.point(b).unwrap().x - 4.0).abs() < 1e-9);
        assert!(sketch.point(a).unwrap().x.abs() < 1e-9, "wrong endpoint moved");
    }

    #[test]
    fn trim_reports_unknown_entities() {
        let mut sketch = Sketch::new("t2");
        assert!(sketch.trim_entity(Uuid::new_v4(), [0.0, 0.0]).is_err());
    }

    #[test]
    fn parallel_constraint_is_enforced() {
        let mut sketch = Sketch::new("par");
        let a0 = sketch.add_point(0.0, 0.0);
        let a1 = sketch.add_point(10.0, 0.0);
        let line_a = sketch.add_line(a0, a1);
        let b0 = sketch.add_point(0.0, 10.0);
        let b1 = sketch.add_point(10.0, 15.0);
        let line_b = sketch.add_line(b0, b1);
        sketch.constraints.push(Constraint::Parallel {
            a: line_a,
            b: line_b,
        });

        sketch.solve().unwrap();

        let dy = (sketch.point(b1).unwrap().y - sketch.point(b0).unwrap().y).abs();
        assert!(dy < 1e-3, "line b is not parallel to line a: dy = {}", dy);
    }

    #[test]
    fn perpendicular_constraint_is_enforced() {
        let mut sketch = Sketch::new("perp");
        let a0 = sketch.add_point(0.0, 0.0);
        let a1 = sketch.add_point(10.0, 0.0);
        let line_a = sketch.add_line(a0, a1);
        let b0 = sketch.add_point(0.0, 10.0);
        let b1 = sketch.add_point(2.0, 20.0);
        let line_b = sketch.add_line(b0, b1);
        sketch.constraints.push(Constraint::Perpendicular {
            a: line_a,
            b: line_b,
        });

        sketch.solve().unwrap();

        let dx = (sketch.point(b1).unwrap().x - sketch.point(b0).unwrap().x).abs();
        assert!(dx < 1e-3, "line b is not perpendicular to line a: dx = {}", dx);
    }

    #[test]
    fn equal_length_constraint_is_enforced() {
        let mut sketch = Sketch::new("eq");
        let a0 = sketch.add_point(0.0, 0.0);
        let a1 = sketch.add_point(10.0, 0.0);
        let line_a = sketch.add_line(a0, a1);
        let b0 = sketch.add_point(0.0, 10.0);
        let b1 = sketch.add_point(4.0, 10.0);
        let line_b = sketch.add_line(b0, b1);
        sketch.constraints.push(Constraint::EqualLength {
            a: line_a,
            b: line_b,
        });

        sketch.solve().unwrap();

        let length = |sketch: &Sketch, line: Uuid| {
            let l = sketch.lines.iter().find(|l| l.id == line).unwrap();
            let p0 = sketch.point(l.a).unwrap();
            let p1 = sketch.point(l.b).unwrap();
            ((p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2)).sqrt()
        };
        let len_a = length(&sketch, line_a);
        let len_b = length(&sketch, line_b);
        assert!(
            (len_a - len_b).abs() < 1e-6,
            "lengths converged to {} and {}",
            len_a,
            len_b
        );
    }

    #[test]
    fn equal_radius_constraint_updates_circles() {
        let mut sketch = Sketch::new("eqr");
        let c1 = sketch.add_circle(0.0, 0.0, 2.0);
        let c2 = sketch.add_circle(10.0, 0.0, 6.0);
        sketch
            .constraints
            .push(Constraint::EqualRadius { a: c1, b: c2 });

        sketch.solve().unwrap();

        let radii: Vec<f64> = sketch.circles.iter().map(|c| c.radius).collect();
        assert!(
            (radii[0] - radii[1]).abs() < 1e-9,
            "radii did not converge: {:?}",
            radii
        );
        assert!(
            (radii[0] - 4.0).abs() < 1e-9,
            "expected the average radius, got {:?}",
            radii
        );
    }
}

#[cfg(test)]
mod dof_and_dimension_tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// A sketch with one unconstrained point has its two coordinates free.
    #[test]
    fn a_bare_point_has_two_degrees_of_freedom() {
        let mut sketch = Sketch::new("p");
        sketch.add_point(3.0, 4.0);
        let analysis = sketch.analyze_dof_detailed();
        assert_eq!(analysis.total_dof, 2);
        assert_eq!(analysis.constrained_dof, 0);
        assert_eq!(analysis.free_dof, 2);
        assert_eq!(analysis.redundant_constraints, 0);
        assert_eq!(analysis.status, ConstraintStatus::UnderConstrained);
    }

    /// A circle's radius is something the solver can move, so it counts.
    #[test]
    fn a_circle_radius_is_a_variable() {
        let mut sketch = Sketch::new("c");
        sketch.add_circle(0.0, 0.0, 5.0);
        let analysis = sketch.analyze_dof_detailed();
        assert_eq!(analysis.total_dof, 3, "two coordinates for the centre and one radius");
        assert_eq!(analysis.free_dof, 3);
    }

    /// The four horizontals and verticals of a rectangle remove four freedoms,
    /// not eight: the rectangle can still slide and change size.
    #[test]
    fn a_rectangle_is_under_constrained_by_four() {
        let mut sketch = Sketch::new("r");
        sketch.add_rectangle(0.0, 0.0, 10.0, 5.0);
        let analysis = sketch.analyze_dof_detailed();
        assert_eq!(analysis.total_dof, 8);
        assert_eq!(analysis.constrained_dof, 4);
        assert_eq!(analysis.free_dof, 4);
        assert_eq!(analysis.redundant_constraints, 0);
        assert_eq!(analysis.status, ConstraintStatus::UnderConstrained);
    }

    /// The old counting version celebrated a duplicate constraint as extra
    /// coverage; the rank sees it adds nothing, and calls the sketch over
    /// constrained.
    #[test]
    fn a_duplicate_constraint_is_redundant() {
        let mut sketch = Sketch::new("dup");
        sketch.add_rectangle(0.0, 0.0, 10.0, 5.0);
        let line = sketch.lines[0].id;
        sketch.constraints.push(Constraint::Horizontal { line });

        let analysis = sketch.analyze_dof_detailed();
        assert_eq!(analysis.constrained_dof, 4, "a repeated horizontal adds no rank");
        assert_eq!(analysis.redundant_constraints, 1);
        assert_eq!(analysis.status, ConstraintStatus::OverConstrained);
        assert_eq!(sketch.constraint_status(), "over-constrained");
    }

    /// A rectangle with its width, its height and one fixed corner is fully
    /// constrained: eight independent equations for eight variables.
    #[test]
    fn a_dimensioned_rectangle_with_a_fixed_corner_is_fully_constrained() {
        let mut sketch = Sketch::new("full");
        let corners = sketch.add_rectangle(0.0, 0.0, 60.0, 40.0);
        sketch.add_dimension(corners[0], corners[1], 60.0, true, true);
        sketch.add_dimension(corners[1], corners[2], 40.0, true, true);
        sketch.constraints.push(Constraint::Fixed { point: corners[0] });

        let analysis = sketch.analyze_dof_detailed();
        assert_eq!(analysis.total_dof, 8);
        assert_eq!(analysis.constrained_dof, 8);
        assert_eq!(analysis.free_dof, 0);
        assert_eq!(analysis.redundant_constraints, 0);
        assert_eq!(analysis.status, ConstraintStatus::FullyConstrained);
        assert_eq!(sketch.constraint_status(), "fully-constrained");
    }

    /// An arc stores a self-coincidence on each endpoint as a marker; it must
    /// not be mistaken for a constraint.
    #[test]
    fn an_arc_self_reference_is_not_an_equation() {
        let mut sketch = Sketch::new("arc");
        sketch.add_arc(0.0, 0.0, 5.0, 0.0, std::f64::consts::FRAC_PI_2);
        let analysis = sketch.analyze_dof_detailed();
        assert_eq!(analysis.redundant_constraints, 0);
        assert_eq!(analysis.status, ConstraintStatus::UnderConstrained);
    }

    /// A driven dimension still shows its value but the solver leaves the
    /// geometry alone.
    #[test]
    fn a_driven_dimension_does_not_move_the_geometry() {
        let mut sketch = Sketch::new("driven");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        sketch.add_dimension(a, b, 20.0, false, true);

        sketch.solve().unwrap();

        let pa = sketch.point(a).unwrap();
        let pb = sketch.point(b).unwrap();
        assert!(close((pb.x - pa.x).abs(), 10.0), "a driven dimension must not drive");
    }

    /// A driving dimension moves the geometry to its value, and editing the
    /// value moves it again.
    #[test]
    fn editing_a_dimension_value_reaches_the_solver() {
        let mut sketch = Sketch::new("edit");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let index = sketch.constraints.len();
        sketch.add_dimension(a, b, 20.0, true, true);
        sketch.solve().unwrap();

        let distance = |sketch: &Sketch| {
            let pa = sketch.point(a).unwrap();
            let pb = sketch.point(b).unwrap();
            (pb.x - pa.x).hypot(pb.y - pa.y)
        };
        assert!(close(distance(&sketch), 20.0), "the dimension did not drive");

        assert!(sketch.set_dimension_value(index, 35.0));
        assert_eq!(sketch.dimension_value(index), Some(35.0));
        assert_eq!(
            sketch.constrained_constraints[0].constraint_value_for_test(),
            35.0,
            "the Intent Manager record must follow the edit"
        );
        sketch.solve().unwrap();
        assert!(close(distance(&sketch), 35.0));
    }


    /// A driven dimension measures without constraining, so it must not add an
    /// equation - otherwise asking for a reference measurement would make the
    /// sketch look over-constrained.

    #[test]
    fn a_radius_dimension_sets_and_fixes_the_radius() {
        let mut sketch = Sketch::new("test");
        let circle = sketch.add_circle(0.0, 0.0, 10.0);
        sketch.add_dimension_constraint(
            Constraint::Radius {
                curve: circle,
                value: 25.0,
            },
            true,
            true,
            Some([5.0, 5.0]),
            DimensionOrientation::Aligned,
        );
        sketch.solve().expect("solve");
        assert!(close(sketch.circles[0].radius, 25.0), "{}", sketch.circles[0].radius);
        // The radius is now one equation of the model.
        let analysis = sketch.analyze_dof_detailed();
        assert!(analysis.total_dof > analysis.free_dof);
    }

    #[test]
    fn a_diameter_dimension_halves_itself_into_a_radius() {
        let mut sketch = Sketch::new("test");
        let circle = sketch.add_circle(0.0, 0.0, 10.0);
        sketch.add_dimension_constraint(
            Constraint::Diameter {
                curve: circle,
                value: 30.0,
            },
            true,
            true,
            None,
            DimensionOrientation::Aligned,
        );
        sketch.solve().expect("solve");
        assert!(close(sketch.circles[0].radius, 15.0));
        // The record is reachable as a dimension, with its value.
        let index = sketch.constraints.len() - 1;
        assert_eq!(sketch.dimension_value(index), Some(30.0));
    }

    #[test]
    fn a_point_line_dimension_measures_the_perpendicular() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(20.0, 0.0);
        let line = sketch.add_line(a, b);
        let point = sketch.add_point(5.0, 3.0);
        sketch.add_dimension_constraint(
            Constraint::PointLineDistance {
                point,
                line,
                value: 7.0,
            },
            true,
            true,
            Some([10.0, 8.0]),
            DimensionOrientation::Aligned,
        );
        sketch.solve().expect("solve");
        let moved = sketch.point(point).expect("the point");
        assert!(close(moved.y, 7.0), "{}", moved.y);
        // ... and it keeps its place along the line.
        assert!(close(moved.x, 5.0), "{}", moved.x);
    }

    #[test]
    fn a_tangent_distance_leaves_the_radius_out_of_the_gap() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(-30.0, 0.0);
        let b = sketch.add_point(30.0, 0.0);
        let line = sketch.add_line(a, b);
        let circle = sketch.add_circle(0.0, 12.0, 5.0);
        sketch.add_dimension_constraint(
            Constraint::LineCurveDistance {
                line,
                curve: circle,
                value: 4.0,
            },
            true,
            true,
            Some([0.0, 6.0]),
            DimensionOrientation::Aligned,
        );
        sketch.solve().expect("solve");
        let centre = sketch.point(sketch.circles[0].center).expect("centre");
        // The centre sits radius + gap above the line.
        assert!(close(centre.y, 9.0), "{}", centre.y);
        assert!(close(sketch.circles[0].radius, 5.0), "radius must not move");
    }

    #[test]
    fn a_driven_radius_dimension_does_not_move_the_radius() {
        let mut sketch = Sketch::new("test");
        let circle = sketch.add_circle(0.0, 0.0, 10.0);
        sketch.add_dimension_constraint(
            Constraint::Radius {
                curve: circle,
                value: 40.0,
            },
            false,
            true,
            None,
            DimensionOrientation::Aligned,
        );
        sketch.solve().expect("solve");
        assert!(close(sketch.circles[0].radius, 10.0));
        // A driven dimension is not an equation either: the radius keeps one.
        let index = sketch.constraints.len() - 1;
        assert_eq!(sketch.dimension_is_driving(index), Some(false));
    }

    #[test]
    fn the_entities_with_freedom_are_listed() {
        let mut sketch = Sketch::new("test");
        let corners = sketch.add_rectangle(0.0, 0.0, 10.0, 10.0);
        let circle = sketch.add_circle(20.0, 0.0, 4.0);
        // Nothing is anchored yet: every point, the four lines and the circle.
        let free = sketch.find_unconstrained_entities();
        assert!(free.contains(&corners[0]));
        assert!(free.contains(&circle));

        // Pin a corner: the two lines that use it still have a free end.
        sketch.constraints.push(Constraint::Fixed { point: corners[0] });
        let free = sketch.find_unconstrained_entities();
        assert!(!free.contains(&corners[0]));
        assert!(free.contains(&sketch.lines[0].id));

        // A radius dimension takes the radius out of the circle's freedom, but
        // its centre is still free.
        sketch.constraints.push(Constraint::Radius {
            curve: circle,
            value: 4.0,
        });
        let free = sketch.find_unconstrained_entities();
        assert!(free.contains(&circle), "the centre is still free");
        sketch.constraints.push(Constraint::Fixed {
            point: sketch.circles[0].center,
        });
        let free = sketch.find_unconstrained_entities();
        assert!(!free.contains(&circle), "radius and centre are fixed");
    }

    #[test]
    fn a_conic_passes_through_its_shoulder() {
        let mut sketch = Sketch::new("test");
        let start = sketch.add_point(-10.0, 0.0);
        let control = sketch.add_point(0.0, 20.0);
        let end = sketch.add_point(10.0, 0.0);
        let conic = sketch.add_conic(start, control, end, 0.5);
        let points = sketch.conic_points(conic, 20);
        assert_eq!(points.len(), 21);
        // At rho 0.5 the curve passes through the quadratic Bezier midpoint.
        let middle = points[10];
        let expected = [
            (-10.0 + 2.0 * 0.5 * 0.0 + 10.0) / 3.0,
            (0.0 + 2.0 * 0.5 * 20.0 + 0.0) / 3.0,
        ];
        assert!(close(middle[0], expected[0]), "{middle:?}");
        assert!(close(middle[1], expected[1]), "{middle:?}");
        assert!(sketch.find_unconstrained_entities().contains(&conic));
    }

    #[test]
    fn a_text_keeps_its_words_and_height() {
        let mut sketch = Sketch::new("test");
        let id = sketch.add_text([5.0, 5.0], 3.0, "RCAD");
        let text = sketch.texts.iter().find(|text| text.id == id).expect("text");
        assert_eq!(text.content, "RCAD");
        assert!(close(text.height, 3.0));
        assert!(close(text.position[0], 5.0));
    }

    #[test]
    fn a_driven_dimension_is_not_an_equation() {
        let mut sketch = Sketch::new("driven dof");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        sketch.add_dimension(a, b, 20.0, false, true);

        let analysis = sketch.analyze_dof_detailed();
        assert_eq!(analysis.total_dof, 4);
        assert_eq!(
            analysis.constrained_dof, 0,
            "a reference measures, it does not constrain"
        );
        assert_eq!(analysis.free_dof, 4);
        assert_eq!(analysis.redundant_constraints, 0);
        assert_eq!(analysis.status, ConstraintStatus::UnderConstrained);
    }

    /// Toggling driving on and off is visible to the caller and to the solver.
    #[test]
    fn toggling_driving_and_promoting_a_dimension() {
        let mut sketch = Sketch::new("toggle");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let index = sketch.constraints.len();
        sketch.add_dimension(a, b, 20.0, true, false);

        assert_eq!(sketch.dimension_is_driving(index), Some(true));
        assert!(sketch.dimension_is_weak(index));

        assert!(sketch.set_dimension_driving(index, false));
        assert_eq!(sketch.dimension_is_driving(index), Some(false));

        assert!(sketch.promote_dimension(index), "weak to strong");
        assert!(!sketch.dimension_is_weak(index));
        assert!(!sketch.promote_dimension(index), "already strong");
    }

    /// Removing a dimension drops its Intent Manager record too.
    #[test]
    fn removing_a_dimension_cleans_its_record() {
        let mut sketch = Sketch::new("remove");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let index = sketch.constraints.len();
        sketch.add_dimension(a, b, 20.0, true, true);
        assert_eq!(sketch.constrained_constraints.len(), 1);

        assert!(sketch.remove_constraint(index));
        assert!(sketch.constraints.is_empty());
        assert!(sketch.constrained_constraints.is_empty());
        assert_eq!(sketch.dimension_value(index), None);
    }

    /// A sketch records the two references that position it: the orientation
    /// one it was placed on and the automatic one.
    #[test]
    fn a_sketch_records_its_two_positioning_references() {
        let mut sketch = Sketch::new("refs");
        sketch.set_references(
            SketchReference {
                reference: Some("plane-id".to_string()),
                label: "FRONT_PLANE".to_string(),
                role: SketchReferenceRole::Orientation,
                origin: [0.0, 0.0, 0.0],
                direction: [1.0, 0.0, 0.0],
            },
            SketchReference {
                reference: None,
                label: "CYS (origin)".to_string(),
                role: SketchReferenceRole::Automatic,
                origin: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, 1.0],
            },
        );
        assert_eq!(sketch.references.len(), 2);
        assert_eq!(
            sketch
                .reference(SketchReferenceRole::Orientation)
                .map(|reference| reference.label.as_str()),
            Some("FRONT_PLANE")
        );
        assert_eq!(
            sketch
                .reference(SketchReferenceRole::Automatic)
                .map(|reference| reference.label.as_str()),
            Some("CYS (origin)")
        );
    }

    /// The angle constraint rotates the second line to the wanted angle.
    #[test]
    fn an_angle_constraint_turns_the_second_line() {
        let mut sketch = Sketch::new("angle");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let first = sketch.add_line(a, b);
        let c = sketch.add_point(0.0, 10.0);
        let d = sketch.add_point(10.0, 10.0);
        let second = sketch.add_line(c, d);
        sketch.constraints.push(Constraint::Angle {
            a: first,
            b: second,
            value: 45.0,
        });

        sketch.solve().unwrap();

        let first_direction = {
            let line = sketch.lines[0].clone();
            let pa = sketch.point(line.a).unwrap();
            let pb = sketch.point(line.b).unwrap();
            [pb.x - pa.x, pb.y - pa.y]
        };
        let second_direction = {
            let line = sketch.lines[1].clone();
            let pa = sketch.point(line.a).unwrap();
            let pb = sketch.point(line.b).unwrap();
            [pb.x - pa.x, pb.y - pa.y]
        };
        let angle = (second_direction[0] * first_direction[1]
            - second_direction[1] * first_direction[0])
            .atan2(
                second_direction[0] * first_direction[0]
                    + second_direction[1] * first_direction[1],
            )
            .to_degrees()
            .abs();
        assert!((angle - 45.0).abs() < 1e-3, "angle is {angle}");
    }

    /// The line-distance constraint slides the second line's distance.
    #[test]
    fn a_line_distance_constraint_moves_the_second_line() {
        let mut sketch = Sketch::new("lines");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(20.0, 0.0);
        let first = sketch.add_line(a, b);
        let c = sketch.add_point(0.0, 3.0);
        let d = sketch.add_point(20.0, 3.0);
        let second = sketch.add_line(c, d);
        sketch.constraints.push(Constraint::LineDistance {
            a: first,
            b: second,
            value: 8.0,
        });

        sketch.solve().unwrap();

        let moved = sketch.point(c).unwrap();
        assert!((moved.y - 8.0).abs() < 1e-3, "the line did not move: {moved:?}");
    }

    /// A dimension stores its text position and the direction it measures.
    #[test]
    fn a_dimension_keeps_its_placement_and_orientation() {
        let mut sketch = Sketch::new("layout");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(30.0, 40.0);
        let index = sketch.constraints.len();
        assert!(sketch.add_dimension_constraint(
            Constraint::Distance { a, b, value: 50.0 },
            true,
            true,
            Some([10.0, 60.0]),
            DimensionOrientation::Horizontal,
        ));
        assert_eq!(sketch.dimension_text_position(index), Some([10.0, 60.0]));
        assert_eq!(
            sketch.dimension_orientation(index),
            DimensionOrientation::Horizontal
        );

        assert!(sketch.set_dimension_layout(
            index,
            Some([20.0, 70.0]),
            DimensionOrientation::Vertical
        ));
        assert_eq!(sketch.dimension_text_position(index), Some([20.0, 70.0]));
        assert_eq!(
            sketch.dimension_orientation(index),
            DimensionOrientation::Vertical
        );
    }

    /// An angle is a dimension too: it can be driven, promoted and removed.
    #[test]
    fn an_angle_dimension_is_a_dimension() {
        let mut sketch = Sketch::new("angle dimension");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let first = sketch.add_line(a, b);
        let c = sketch.add_point(0.0, 10.0);
        let d = sketch.add_point(10.0, 10.0);
        let second = sketch.add_line(c, d);
        let index = sketch.constraints.len();
        assert!(sketch.add_dimension_constraint(
            Constraint::Angle {
                a: first,
                b: second,
                value: 90.0,
            },
            true,
            false,
            Some([5.0, 5.0]),
            DimensionOrientation::Aligned,
        ));
        assert_eq!(sketch.dimension_value(index), Some(90.0));
        assert_eq!(sketch.dimension_is_driving(index), Some(true));
        assert!(sketch.dimension_is_weak(index));
        assert!(sketch.promote_dimension(index));
        assert!(!sketch.dimension_is_weak(index));
        assert!(sketch.set_dimension_value(index, 60.0));
        assert_eq!(sketch.dimension_value(index), Some(60.0));
        assert!(sketch.remove_constraint(index));
        assert!(sketch.constraints.is_empty());
    }
}

/// Small test-only read helpers.
#[cfg(test)]
impl ConstrainedConstraint {
    fn constraint_value_for_test(&self) -> f64 {
        match &self.constraint {
            Constraint::Distance { value, .. } => *value,
            _ => f64::NAN,
        }
    }
}

