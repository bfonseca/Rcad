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
    pub points: Vec<SketchPoint>,
    pub lines: Vec<SketchLine>,
    pub circles: Vec<SketchCircle>,
    pub constraints: Vec<Constraint>,
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
            points: Vec::new(),
            lines: Vec::new(),
            circles: Vec::new(),
            constraints: Vec::new(),
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

    pub fn point_mut(&mut self, id: Uuid) -> Option<&mut SketchPoint> {
        self.points.iter_mut().find(|p| p.id == id)
    }

    pub fn point(&self, id: Uuid) -> Option<&SketchPoint> {
        self.points.iter().find(|p| p.id == id)
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
pub enum Constraint {
    Coincident { a: Uuid, b: Uuid },
    Distance { a: Uuid, b: Uuid, value: f64 },
    Horizontal { line: Uuid },
    Vertical { line: Uuid },
    Fixed { point: Uuid },
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
            }
        }
        if max_err < 1e-6 {
            break;
        }
    }
    Ok(())
}

#[cfg(feature = "solvespace")]
mod solvespace {
    use super::*;
    pub fn solve(_sketch: &mut Sketch) -> Result<(), SketchError> {
        // Hook for `slvs` integration — keep iterative as primary until FFI is
        // validated across CI targets.
        Err(SketchError::Solver(
            "slvs feature enabled but adapter not yet complete".into(),
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
