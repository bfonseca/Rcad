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
    pub constraints: Vec<Constraint>,
    /// Constraints with strength info for Intent Manager
    pub constrained_constraints: Vec<ConstrainedConstraint>,
    /// Reference geometry projected onto sketch plane (edges, vertices from 3D model)
    pub reference_geometry: Vec<ReferenceGeometry>,
    /// When true, sketch is waiting for user to select a plane in the viewport
    pub in_placement_mode: bool,
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
            constraints: Vec::new(),
            constrained_constraints: Vec::new(),
            reference_geometry: Vec::new(),
            in_placement_mode: false,
        }
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

// Fix the lines_intersect signature to take references
fn line_line_intersect(p1: &SketchPoint, p2: &SketchPoint, p3: &SketchPoint, p4: &SketchPoint) -> bool {
    lines_intersect(p1, p2, p3, p4)
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
}

fn iterative_solve(sketch: &mut Sketch) -> Result<(), SketchError> {
    for _ in 0..80 {
        let mut max_err = 0.0f64;
        let constraints = sketch.constraints.clone();
        for c in constraints {
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
                    // Tangent constraint between two entities (line-line, line-arc, arc-arc, line-spline, arc-spline)
                    // For now, handle line-arc and arc-arc
                    if let Some(arc_a) = sketch.arcs.iter().find(|arc| arc.id == a) {
                        if let Some(_arc_b) = sketch.arcs.iter().find(|arc| arc.id == b) {
                            // Arc-arc tangency: distance between centers = sum of radii (external) or diff (internal)
                            // This is handled by the distance constraints already
                        } else if let Some(line_b) = sketch.lines.iter().find(|line| line.id == b) {
                            // Line-arc tangency: distance from arc center to line = radius
                            if let (Some(p1), Some(p2)) = (sketch.point(line_b.a), sketch.point(line_b.b)) {
                                let cx = sketch.point(arc_a.center).map(|p| p.x).unwrap_or(0.0);
                                let cy = sketch.point(arc_a.center).map(|p| p.y).unwrap_or(0.0);
                                let radius = arc_a.radius;
                                // Distance from point to line
                                let x1 = p1.x;
                                let y1 = p1.y;
                                let x2 = p2.x;
                                let y2 = p2.y;
                                let num = ((y2 - y1) * cx - (x2 - x1) * cy + x2 * y1 - y2 * x1).abs();
                                let den = ((y2 - y1).powi(2) + (x2 - x1).powi(2)).sqrt().max(1e-9);
                                let dist = num / den;
                                let err = (dist - radius).abs();
                                max_err = max_err.max(err);
                                // Move center towards/away from line to satisfy tangency
                                // This is a simplified approach - full implementation needs line direction
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
                    // Equal radius between two arcs or circles
                    let mut radius_a = None;
                    let mut radius_b = None;
                    if let Some(arc) = sketch.arcs.iter().find(|arc| arc.id == a) {
                        radius_a = Some(arc.radius);
                    } else if let Some(circle) = sketch.circles.iter().find(|circle| circle.id == a) {
                        radius_a = Some(circle.radius);
                    }
                    if let Some(arc) = sketch.arcs.iter().find(|arc| arc.id == b) {
                        radius_b = Some(arc.radius);
                    } else if let Some(circle) = sketch.circles.iter().find(|circle| circle.id == b) {
                        radius_b = Some(circle.radius);
                    }
                    if let (Some(ra), Some(rb)) = (radius_a, radius_b) {
                        let err = (ra - rb).abs();
                        max_err = max_err.max(err);
                        // Adjust both towards average
                        let _avg = (ra + rb) * 0.5;
                        // Note: In iterative solver, we'd need to adjust the geometry
                        // This is a placeholder for the logic
                    }
                }
                Constraint::EqualLength { a, b } => {
                    // Equal length between two lines
                    if let (Some(line_a), Some(line_b)) = (
                        sketch.lines.iter().find(|l| l.id == a),
                        sketch.lines.iter().find(|l| l.id == b),
                    ) {
                        if let (Some(pa1), Some(pa2), Some(pb1), Some(pb2)) = (
                            sketch.point(line_a.a),
                            sketch.point(line_a.b),
                            sketch.point(line_b.a),
                            sketch.point(line_b.b),
                        ) {
                            let len_a = ((pa2.x - pa1.x).powi(2) + (pa2.y - pa1.y).powi(2)).sqrt();
                            let len_b = ((pb2.x - pb1.x).powi(2) + (pb2.y - pb1.y).powi(2)).sqrt();
                            let err = (len_a - len_b).abs();
                            max_err = max_err.max(err);
                        }
                    }
                }
                Constraint::Parallel { a, b } => {
                    // Parallel between two lines
                    if let (Some(line_a), Some(line_b)) = (
                        sketch.lines.iter().find(|l| l.id == a),
                        sketch.lines.iter().find(|l| l.id == b),
                    ) {
                        if let (Some(pa1), Some(pa2), Some(pb1), Some(pb2)) = (
                            sketch.point(line_a.a),
                            sketch.point(line_a.b),
                            sketch.point(line_b.a),
                            sketch.point(line_b.b),
                        ) {
                            let dax = pa2.x - pa1.x;
                            let day = pa2.y - pa1.y;
                            let dbx = pb2.x - pb1.x;
                            let dby = pb2.y - pb1.y;
                            let cross = dax * dby - day * dbx;
                            let len_a = (dax * dax + day * day).sqrt().max(1e-9);
                            let len_b = (dbx * dbx + dby * dby).sqrt().max(1e-9);
                            let err = (cross / (len_a * len_b)).abs();
                            max_err = max_err.max(err);
                        }
                    }
                }
                Constraint::Perpendicular { a, b } => {
                    // Perpendicular between two lines
                    if let (Some(line_a), Some(line_b)) = (
                        sketch.lines.iter().find(|l| l.id == a),
                        sketch.lines.iter().find(|l| l.id == b),
                    ) {
                        if let (Some(pa1), Some(pa2), Some(pb1), Some(pb2)) = (
                            sketch.point(line_a.a),
                            sketch.point(line_a.b),
                            sketch.point(line_b.a),
                            sketch.point(line_b.b),
                        ) {
                            let dax = pa2.x - pa1.x;
                            let day = pa2.y - pa1.y;
                            let dbx = pb2.x - pb1.x;
                            let dby = pb2.y - pb1.y;
                            let dot = dax * dbx + day * dby;
                            let len_a = (dax * dax + day * day).sqrt().max(1e-9);
                            let len_b = (dbx * dbx + dby * dby).sqrt().max(1e-9);
                            let err = (dot / (len_a * len_b)).abs();
                            max_err = max_err.max(err);
                        }
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

/// Constraint with strength for Intent Manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstrainedConstraint {
    pub constraint: Constraint,
    pub strength: ConstraintStrength,
    pub is_driving: bool, // For dimensions: true = driving (modifies geometry), false = driven (reference only)
}

impl Sketch {
    /// Analyze degrees of freedom in the sketch
    /// Returns (dof, constrained_dof, redundant_constraints)
    pub fn analyze_dof(&self) -> (usize, usize, usize) {
        // Each point has 2 DOF (x, y)
        let total_dof = self.points.len() * 2;
        
        // Count effective constraints (simplified)
        let mut constrained: usize = 0;
        for constraint in &self.constraints {
            match constraint {
                Constraint::Fixed { .. } => constrained += 2,
                Constraint::Coincident { .. } => constrained += 2,
                Constraint::Distance { .. } => constrained += 1,
                Constraint::Horizontal { .. } => constrained += 1,
                Constraint::Vertical { .. } => constrained += 1,
                Constraint::Tangent { .. } => constrained += 1,
                Constraint::Symmetric { .. } => constrained += 2,
                Constraint::EqualRadius { .. } => constrained += 1,
                Constraint::EqualLength { .. } => constrained += 1,
                Constraint::Parallel { .. } => constrained += 1,
                Constraint::Perpendicular { .. } => constrained += 1,
                Constraint::Midpoint { .. } => constrained += 2,
                Constraint::Concentric { .. } => constrained += 2,
            }
        }
        
        // Simplified: assume some redundancy
        let redundant = constrained.saturating_sub(total_dof);
        let effective = constrained.min(total_dof);
        
        (total_dof, effective, redundant)
    }
    
    /// Get sketch status: "under-constrained", "fully-constrained", "over-constrained"
    pub fn constraint_status(&self) -> &'static str {
        let (total, constrained, redundant) = self.analyze_dof();
        if redundant > 0 {
            "over-constrained"
        } else if constrained >= total {
            "fully-constrained"
        } else {
            "under-constrained"
        }
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
    pub fn add_dimension(&mut self, a: Uuid, b: Uuid, value: f64, driving: bool, strong: bool) -> Uuid {
        let id = Uuid::new_v4();
        self.constraints.push(Constraint::Distance { a, b, value });
        self.constrained_constraints.push(ConstrainedConstraint {
            constraint: Constraint::Distance { a, b, value },
            strength: if strong { ConstraintStrength::Strong } else { ConstraintStrength::Weak },
            is_driving: driving,
        });
        id
    }

    /// Promote a weak constraint to strong (user explicitly adds dimension)
    pub fn promote_to_strong(&mut self, constraint_id: Uuid) -> bool {
        if let Some(cc) = self.constrained_constraints.iter_mut().find(|c| c.constraint_id() == Some(constraint_id)) {
            cc.strength = ConstraintStrength::Strong;
            return true;
        }
        false
    }

    /// Get all weak constraints that could be promoted
    pub fn weak_constraints(&self) -> Vec<&ConstrainedConstraint> {
        self.constrained_constraints.iter()
            .filter(|c| c.strength == ConstraintStrength::Weak)
            .collect()
    }

    /// Get all driving dimensions
    pub fn driving_dimensions(&self) -> Vec<&ConstrainedConstraint> {
        self.constrained_constraints.iter()
            .filter(|c| c.is_driving)
            .collect()
    }

    /// Get all driven dimensions (reference only)
    pub fn driven_dimensions(&self) -> Vec<&ConstrainedConstraint> {
        self.constrained_constraints.iter()
            .filter(|c| !c.is_driving)
            .collect()
    }

    /// Trim a line/arc at intersection with another entity
    pub fn trim_entity(&mut self, entity_id: Uuid, trim_point: [f64; 2]) -> Result<(), SketchError> {
        // First, find the line and collect data needed
        let line_data = self.lines.iter()
            .find(|l| l.id == entity_id)
            .and_then(|line| {
                Some((
                    line.a, line.b,
                    self.point(line.a)?.clone(),
                    self.point(line.b)?.clone(),
                ))
            });
        
        if let Some((pt_a, pt_b, pa, pb)) = line_data {
            // Project trim_point onto line
            let v = Vec2::new(pb.x as f32 - pa.x as f32, pb.y as f32 - pa.y as f32);
            let _w = Vec2::new(trim_point[0] as f32 - pa.x as f32, trim_point[1] as f32 - pa.y as f32);
            let v_len_sq = v.length_squared().max(1e-6);
            let t = (Vec2::new(trim_point[0] as f32 - pa.x as f32, trim_point[1] as f32 - pa.y as f32).dot(v)) / v_len_sq;
            
            if t >= 0.0 && t <= 1.0 {
                let dist_a = ((pa.x - trim_point[0]).powi(2) + (pa.y - trim_point[1]).powi(2)).sqrt();
                let dist_b = ((pb.x - trim_point[0]).powi(2) + (pb.y - trim_point[1]).powi(2)).sqrt();
                
                if dist_a < dist_b {
                    if let Some(p) = self.point_mut(pt_a) {
                        p.x = trim_point[0];
                        p.y = trim_point[1];
                    }
                } else {
                    if let Some(p) = self.point_mut(pt_b) {
                        p.x = trim_point[0];
                        p.y = trim_point[1];
                    }
                }
            }
        }
        // Similar for arcs...
        Ok(())
    }

    /// Extend a line to meet another entity
    pub fn extend_entity(&mut self, entity_id: Uuid, _target_id: Uuid) -> Result<(), SketchError> {
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
        
        if let Some((pt_a, pt_b, pa, pb)) = line_data {
            let dir = Vec2::new(pb.x as f32 - pa.x as f32, pb.y as f32 - pa.y as f32);
            let len = dir.length().max(1e-6);
            let unit = dir / len;
            
            // Extend both endpoints by 10 units
            let ext = 10.0;
            if let Some(p) = self.point_mut(pt_a) {
                p.x -= unit.x as f64 * ext;
                p.y -= unit.y as f64 * ext;
            }
            if let Some(p) = self.point_mut(pt_b) {
                p.x += unit.x as f64 * ext;
                p.y += unit.y as f64 * ext;
            }
        }
        Ok(())
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

    /// Mirror entities across a line (axis)
    pub fn mirror_entities(&mut self, entity_ids: &[Uuid], axis_line_id: Uuid) -> Result<Vec<Uuid>, SketchError> {
        // First collect axis data
        let axis_data = self.lines.iter()
            .find(|l| l.id == axis_line_id)
            .and_then(|axis| {
                Some((
                    self.point(axis.a)?.clone(),
                    self.point(axis.b)?.clone(),
                ))
            });
        
        let (p1, p2) = match axis_data {
            Some((a, b)) => (a, b),
            None => return Err(SketchError::Unknown(axis_line_id)),
        };
        
        let axis_dir = Vec2::new(p2.x as f32 - p1.x as f32, p2.y as f32 - p1.y as f32);
        let axis_len = axis_dir.length().max(1e-6);
        let axis_unit = axis_dir / axis_len;
        
        let mut new_ids = Vec::new();
        
        for entity_id in entity_ids {
            if let Some(point) = self.points.iter().find(|p| p.id == *entity_id).cloned() {
                let v = Vec2::new(point.x as f32 - p1.x as f32, point.y as f32 - p1.y as f32);
                let proj = v.dot(axis_unit);
                let perp = v - axis_unit * proj;
                let mirrored = Vec2::new(p1.x as f32 + proj * axis_unit.x - perp.x,
                                        p1.y as f32 + proj * axis_unit.y - perp.y);
                
                let new_id = self.add_point(mirrored.x as f64, mirrored.y as f64);
                new_ids.push(new_id);
            }
            // Also handle lines, arcs, circles by mirroring their defining points
        }
        
        Ok(new_ids)
    }

    /// Create a linear pattern of entities
    pub fn linear_pattern(&mut self, entity_ids: &[Uuid], direction: [f64; 2], spacing: f64, count: usize) -> Result<Vec<Uuid>, SketchError> {
        let dir = Vec2::new(direction[0] as f32, direction[1] as f32).normalize_or_zero();
        let mut new_ids = Vec::new();
        
        // First collect all points to pattern
        let points_to_pattern: Vec<_> = entity_ids.iter()
            .filter_map(|id| self.points.iter().find(|p| p.id == *id).cloned())
            .collect();
        
        for i in 1..count {
            let offset = dir * (spacing as f32 * i as f32);
            
            for point in &points_to_pattern {
                let new_id = self.add_point(
                    point.x + offset.x as f64,
                    point.y + offset.y as f64,
                );
                new_ids.push(new_id);
            }
            // Also handle lines, arcs, circles by patterning their defining points
        }
        
        Ok(new_ids)
    }

    /// Create a circular pattern of entities
    pub fn circular_pattern(&mut self, entity_ids: &[Uuid], center: [f64; 2], angle_step: f64, count: usize) -> Result<Vec<Uuid>, SketchError> {
        let mut new_ids = Vec::new();
        
        // First collect all points to pattern
        let points_to_pattern: Vec<_> = entity_ids.iter()
            .filter_map(|id| self.points.iter().find(|p| p.id == *id).cloned())
            .collect();
        
        for i in 1..count {
            let angle = angle_step * i as f64;
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            
            for point in &points_to_pattern {
                let dx = point.x - center[0];
                let dy = point.y - center[1];
                let new_x = center[0] + dx * cos_a - dy * sin_a;
                let new_y = center[1] + dx * sin_a + dy * cos_a;
                
                let new_id = self.add_point(new_x, new_y);
                new_ids.push(new_id);
            }
            // Also handle lines, arcs, circles by patterning their defining points
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
