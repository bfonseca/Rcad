//! Drawing in a sketch: the mouse tools, the grid snap, and what a sketch
//! shows on screen.
//!
//! The geometry itself lives in `rcad-sketch`. This module is the part that
//! turns screen input into sketch coordinates, keeps the two-click state of
//! each tool, and turns the sketch back into something the viewport can draw:
//! its geometry, its constraints, its dimensions and the points that are still
//! free. Everything except the two drawing functions is a pure function, so the
//! interesting behaviour is testable without a running UI.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use glam::Vec3;
use rcad_sketch::{Constraint, ConstraintStrength, Sketch};
use uuid::Uuid;

use crate::camera::OrbitCamera;
use crate::viewport::{Ray3D, sketch_basis};

/// Origin, normal and sketch x direction of the plane a sketch lies on.
pub type SketchPlane = ([f64; 3], [f64; 3], [f64; 3]);

/// The spacing of a sketch's grid, in model units. The snap rounds to the same
/// value, so geometry lands exactly on the lines the user sees.
pub const GRID_SPACING: f64 = 5.0;

/// How far the sketch grid reaches from its origin.
pub const GRID_EXTENT: f32 = 50.0;

/// How close, in pixels, a click must be to a sketch entity to hit it.
pub const HIT_RADIUS: f32 = 6.0;

/// How far a dimension line is drawn from the geometry it measures.
const DIMENSION_OFFSET: f32 = 16.0;

const GEOMETRY: Color32 = Color32::from_rgb(225, 228, 235);
const SELECTED: Color32 = Color32::from_rgb(255, 170, 60);
const HOVERED: Color32 = Color32::from_rgb(255, 220, 140);
const CONSTRAINT: Color32 = Color32::from_rgb(120, 205, 255);
const DIMENSION: Color32 = Color32::from_rgb(130, 230, 150);
const WEAK: Color32 = Color32::from_rgb(160, 160, 160);
const DRIVEN: Color32 = Color32::from_rgb(120, 190, 210);
const FREE_POINT: Color32 = Color32::from_rgb(255, 130, 130);
const PREVIEW: Color32 = Color32::from_rgb(255, 210, 90);
const TEXT: Color32 = Color32::from_rgb(235, 235, 235);

/// What a click in the viewport does while a sketch is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SketchTool {
    /// Nothing is drawn: clicks pick sketch entities and the model.
    #[default]
    Select,
    Point,
    Line,
    Rectangle,
    Circle,
    /// Clicks a line and dimensions its length.
    Dimension,
}

impl SketchTool {
    /// The name the sketch panel and the status bar show.
    pub fn label(self) -> &'static str {
        match self {
            SketchTool::Select => "Select",
            SketchTool::Point => "Point",
            SketchTool::Line => "Line",
            SketchTool::Rectangle => "Rectangle",
            SketchTool::Circle => "Circle",
            SketchTool::Dimension => "Dimension",
        }
    }

    /// True when the tool creates geometry with clicks instead of picking it.
    pub fn creates_geometry(self) -> bool {
        matches!(
            self,
            SketchTool::Point | SketchTool::Line | SketchTool::Rectangle | SketchTool::Circle
        )
    }

    /// The next step, in the words a Creo user expects.
    pub fn hint(self) -> &'static str {
        match self {
            SketchTool::Select => "Click a sketch entity to select it; Ctrl adds.",
            SketchTool::Point => "Click to place a point.",
            SketchTool::Line => "Click the two ends of the line.",
            SketchTool::Rectangle => "Click two opposite corners of the rectangle.",
            SketchTool::Circle => "Click the centre, then a point on the circle.",
            SketchTool::Dimension => "Click a line to dimension its length.",
        }
    }
}

/// What one click did, so the caller can report it and rebuild afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SketchEdit {
    /// The first click of a two-click tool: the geometry is not complete yet.
    Pending,
    Point(Uuid),
    Line(Uuid),
    Rectangle([Uuid; 4]),
    Circle(Uuid),
    /// The two clicks were on top of each other; the first one is kept.
    TooSmall,
    /// The tool does not draw.
    NotDrawing,
}

impl SketchEdit {
    /// True when this click added geometry to the sketch, so the caller knows
    /// a solve is worth running.
    pub fn added_geometry(&self) -> bool {
        matches!(
            self,
            SketchEdit::Point(_)
                | SketchEdit::Line(_)
                | SketchEdit::Rectangle(_)
                | SketchEdit::Circle(_)
        )
    }
}

/// The active sketch tool and, for the two-click tools, the first click.
///
/// This is the whole mouse state: a tool name and one pending point. Keeping it
/// out of `RcadApp` means the interaction rules can be tested directly.
#[derive(Debug, Clone, Default)]
pub struct SketchDraft {
    tool: SketchTool,
    first: Option<[f64; 2]>,
}

impl SketchDraft {
    pub fn tool(&self) -> SketchTool {
        self.tool
    }

    /// The first click of a two-click tool, in sketch coordinates.
    pub fn pending(&self) -> Option<[f64; 2]> {
        self.first
    }

    /// Start a tool: any half-finished shape is dropped.
    pub fn begin(&mut self, tool: SketchTool) {
        self.tool = tool;
        self.first = None;
    }

    /// Escape: drop the pending first click. A second Escape leaves the tool.
    ///
    /// Returns true when something was actually cancelled, so the caller can
    /// fall back to clearing the selection only when the sketch had nothing to
    /// cancel.
    pub fn cancel(&mut self) -> bool {
        if self.first.take().is_some() {
            return true;
        }
        if self.tool != SketchTool::Select {
            self.tool = SketchTool::Select;
            return true;
        }
        false
    }

    /// Feed one click, in sketch coordinates, already snapped to the grid.
    pub fn click(&mut self, sketch: &mut Sketch, point: [f64; 2]) -> SketchEdit {
        match self.tool {
            SketchTool::Select | SketchTool::Dimension => SketchEdit::NotDrawing,
            SketchTool::Point => SketchEdit::Point(sketch.add_point(point[0], point[1])),
            SketchTool::Line | SketchTool::Rectangle | SketchTool::Circle => {
                let Some(first) = self.first else {
                    self.first = Some(point);
                    return SketchEdit::Pending;
                };
                let dx = point[0] - first[0];
                let dy = point[1] - first[1];
                match self.tool {
                    SketchTool::Line => {
                        if dx.hypot(dy) < 1e-9 {
                            return SketchEdit::TooSmall;
                        }
                        self.first = None;
                        let a = sketch.add_point(first[0], first[1]);
                        let b = sketch.add_point(point[0], point[1]);
                        SketchEdit::Line(sketch.add_line(a, b))
                    }
                    SketchTool::Rectangle => {
                        if dx.abs() < 1e-9 || dy.abs() < 1e-9 {
                            return SketchEdit::TooSmall;
                        }
                        self.first = None;
                        SketchEdit::Rectangle(
                            sketch.add_rectangle(first[0], first[1], point[0], point[1]),
                        )
                    }
                    SketchTool::Circle => {
                        let radius = dx.hypot(dy);
                        if radius < 1e-9 {
                            return SketchEdit::TooSmall;
                        }
                        self.first = None;
                        SketchEdit::Circle(sketch.add_circle(first[0], first[1], radius))
                    }
                    _ => SketchEdit::NotDrawing,
                }
            }
        }
    }
}

/// Round a sketch coordinate to the nearest grid line.
///
/// A spacing of zero or less turns the snap off, which is how the Options
/// checkbox disables it.
pub fn snap_to_grid(point: [f64; 2], spacing: f64) -> [f64; 2] {
    if spacing <= 0.0 {
        return point;
    }
    [
        (point[0] / spacing).round() * spacing,
        (point[1] / spacing).round() * spacing,
    ]
}

/// Where a screen position lands in the sketch, or None when the ray misses
/// the sketch plane (an edge-on view has no answer).
pub fn screen_to_sketch(
    pos: Pos2,
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
) -> Option<[f64; 2]> {
    let (origin, normal, x_direction) = plane;
    let ray = Ray3D::from_screen(pos, rect, camera);
    let plane_origin = Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32);
    let plane_normal =
        Vec3::new(normal[0] as f32, normal[1] as f32, normal[2] as f32).normalize_or_zero();
    let distance = ray.intersect_plane(plane_origin, plane_normal)?;
    let hit = ray.origin + ray.direction * distance;
    let (u, v) = sketch_basis(normal, x_direction);
    let local = hit - plane_origin;
    Some([
        local.dot(Vec3::from(u)) as f64,
        local.dot(Vec3::from(v)) as f64,
    ])
}

/// The screen position of a sketch coordinate.
pub fn sketch_to_screen(
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
    point: [f64; 2],
) -> Option<Pos2> {
    let (origin, normal, x_direction) = plane;
    let (u, v) = sketch_basis(normal, x_direction);
    let world = Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32)
        + Vec3::from(u) * point[0] as f32
        + Vec3::from(v) * point[1] as f32;
    crate::viewport::project_world(rect, camera, world)
}

/// The position of a sketch point, when the id names one.
pub fn point_of(sketch: &Sketch, id: Uuid) -> Option<[f64; 2]> {
    sketch.point(id).map(|point| [point.x, point.y])
}

/// A word for what an entity id is, for status messages.
pub fn entity_label(sketch: &Sketch, id: Uuid) -> &'static str {
    if sketch.points.iter().any(|point| point.id == id) {
        "point"
    } else if sketch.lines.iter().any(|line| line.id == id) {
        "line"
    } else if sketch.circles.iter().any(|circle| circle.id == id) {
        "circle"
    } else if sketch.arcs.iter().any(|arc| arc.id == id) {
        "arc"
    } else if sketch.splines.iter().any(|spline| spline.id == id) {
        "spline"
    } else {
        "entity"
    }
}

/// A constraint marker: a short label at a place on the sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintGlyph {
    pub point: [f64; 2],
    pub label: &'static str,
}

/// Where the constraint of an entity should be drawn: a point where it is, a
/// line at its midpoint, a circle or arc at its centre.
fn anchor_of(sketch: &Sketch, id: Uuid) -> Option<[f64; 2]> {
    if let Some(point) = point_of(sketch, id) {
        return Some(point);
    }
    if let Some(line) = sketch.lines.iter().find(|line| line.id == id) {
        let a = point_of(sketch, line.a)?;
        let b = point_of(sketch, line.b)?;
        return Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]);
    }
    if let Some(circle) = sketch.circles.iter().find(|circle| circle.id == id) {
        return point_of(sketch, circle.center);
    }
    if let Some(arc) = sketch.arcs.iter().find(|arc| arc.id == id) {
        return point_of(sketch, arc.center);
    }
    None
}

/// Every constraint that has a readable marker, placed on the geometry it
/// applies to. Dimensions are not glyphs: they are drawn as dimensions.
pub fn constraint_glyphs(sketch: &Sketch) -> Vec<ConstraintGlyph> {
    let mut glyphs = Vec::new();
    let mut push = |point: Option<[f64; 2]>, label: &'static str| {
        if let Some(point) = point {
            glyphs.push(ConstraintGlyph { point, label });
        }
    };
    for constraint in &sketch.constraints {
        match constraint {
            Constraint::Coincident { a, b } => {
                let point = match (point_of(sketch, *a), point_of(sketch, *b)) {
                    (Some(a), Some(b)) => Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]),
                    (Some(a), None) | (None, Some(a)) => Some(a),
                    _ => None,
                };
                push(point, "coincident");
            }
            Constraint::Horizontal { line } => push(anchor_of(sketch, *line), "H"),
            Constraint::Vertical { line } => push(anchor_of(sketch, *line), "V"),
            Constraint::Parallel { a, b } => {
                push(anchor_of(sketch, *a), "||");
                push(anchor_of(sketch, *b), "||");
            }
            Constraint::Perpendicular { a, b } => {
                push(anchor_of(sketch, *a), "perp");
                push(anchor_of(sketch, *b), "perp");
            }
            Constraint::EqualLength { a, b } => {
                push(anchor_of(sketch, *a), "=");
                push(anchor_of(sketch, *b), "=");
            }
            Constraint::EqualRadius { a, b } => {
                push(anchor_of(sketch, *a), "R=");
                push(anchor_of(sketch, *b), "R=");
            }
            Constraint::Tangent { a, b } => {
                push(anchor_of(sketch, *a), "T");
                push(anchor_of(sketch, *b), "T");
            }
            Constraint::Symmetric { a, b, .. } => {
                let point = match (point_of(sketch, *a), point_of(sketch, *b)) {
                    (Some(a), Some(b)) => Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]),
                    _ => None,
                };
                push(point, "sym");
            }
            Constraint::Concentric { a, .. } => push(anchor_of(sketch, *a), "concentric"),
            Constraint::Midpoint { point, .. } => push(point_of(sketch, *point), "M"),
            Constraint::Fixed { point } => push(point_of(sketch, *point), "F"),
            Constraint::Distance { .. } => {}
        }
    }
    glyphs
}

/// A length dimension the viewport draws between two points.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionAnnotation {
    /// The two points the dimension measures.
    pub a: [f64; 2],
    pub b: [f64; 2],
    /// The value the constraint holds the distance at.
    pub value: f64,
    /// The distance the geometry has right now: the two differ while a sketch
    /// is not solved yet.
    pub measured: f64,
    pub text: String,
    /// False for a driven (reference) dimension.
    pub driving: bool,
    /// Weak dimensions come from the Intent Manager, not from the user.
    pub weak: bool,
}

/// Format a length the way a dimension is written: no trailing zeros.
pub fn format_length(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    // -0 would otherwise print as "-0".
    if rounded == 0.0 {
        return "0".to_string();
    }
    format!("{rounded}")
}

/// Every Distance constraint, ready to draw.
///
/// The strength and driving flag come from `constrained_constraints` when the
/// dimension was added through `Sketch::add_dimension`; a dimension pushed
/// straight into `constraints` counts as a strong, driving one.
pub fn dimensions(sketch: &Sketch) -> Vec<DimensionAnnotation> {
    let mut dimensions = Vec::new();
    for constraint in &sketch.constraints {
        let Constraint::Distance { a, b, value } = constraint else {
            continue;
        };
        let (Some(pa), Some(pb)) = (point_of(sketch, *a), point_of(sketch, *b)) else {
            continue;
        };
        let recorded = sketch.constrained_constraints.iter().find(|recorded| {
            matches!(&recorded.constraint, Constraint::Distance { a: ra, b: rb, .. }
                if ra == a && rb == b)
        });
        let driving = recorded.map(|recorded| recorded.is_driving).unwrap_or(true);
        let weak = recorded
            .map(|recorded| recorded.strength == ConstraintStrength::Weak)
            .unwrap_or(false);
        let measured = (pb[0] - pa[0]).hypot(pb[1] - pa[1]);
        let text = format_length(*value);
        dimensions.push(DimensionAnnotation {
            a: pa,
            b: pb,
            value: *value,
            measured,
            text,
            driving,
            weak,
        });
    }
    dimensions
}

/// The points nothing anchors: the ones the user can still drag around.
///
/// A point is anchored when it is Fixed, when it coincides with an anchored
/// point, or when it is the midpoint of a line whose two ends are anchored.
/// Horizontal, Vertical, Distance, Parallel, Symmetric and the rest take
/// freedom away without saying where the point goes, so they do not anchor it:
/// a fully dimensioned rectangle can still slide until one corner is fixed.
pub fn free_points(sketch: &Sketch) -> Vec<Uuid> {
    let mut held: std::collections::HashSet<Uuid> = sketch
        .constraints
        .iter()
        .filter_map(|constraint| match constraint {
            Constraint::Fixed { point } => Some(*point),
            _ => None,
        })
        .collect();

    // A coincidence or a midpoint can pull another point into the anchored
    // set, so this runs to a fixed point rather than once.
    let mut changed = true;
    while changed {
        changed = false;
        for constraint in &sketch.constraints {
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
                    if let Some(line) = sketch.lines.iter().find(|candidate| candidate.id == *line)
                    {
                        if held.contains(&line.a) && held.contains(&line.b) && held.insert(*point) {
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    sketch
        .points
        .iter()
        .filter(|point| !held.contains(&point.id))
        .map(|point| point.id)
        .collect()
}

/// The kind of sketch entity a click found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SketchEntityKind {
    Point,
    Line,
    Circle,
    Arc,
}

/// What a click landed on, with the distance in pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchHit {
    pub id: Uuid,
    pub kind: SketchEntityKind,
    pub distance: f32,
}

/// The sketch entity nearest a screen position, within the threshold in pixels.
///
/// Points win ties: they are the small things a user aims at, and a line's
/// endpoint is also on the line.
pub fn hit_test(
    sketch: &Sketch,
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
    pos: Pos2,
    threshold: f32,
) -> Option<SketchHit> {
    let mut best: Option<SketchHit> = None;
    let mut consider = |candidate: SketchHit| {
        let better = match &best {
            None => true,
            Some(current) => {
                candidate.distance < current.distance - 0.5
                    || ((candidate.distance - current.distance).abs() <= 0.5
                        && rank(candidate.kind) > rank(current.kind))
            }
        };
        if better {
            best = Some(candidate);
        }
    };

    for point in &sketch.points {
        let Some(screen) = sketch_to_screen(rect, camera, plane, [point.x, point.y]) else {
            continue;
        };
        let distance = pos.distance(screen);
        if distance <= threshold {
            consider(SketchHit {
                id: point.id,
                kind: SketchEntityKind::Point,
                distance,
            });
        }
    }

    for line in &sketch.lines {
        let (Some(a), Some(b)) = (point_of(sketch, line.a), point_of(sketch, line.b)) else {
            continue;
        };
        let (Some(sa), Some(sb)) = (
            sketch_to_screen(rect, camera, plane, a),
            sketch_to_screen(rect, camera, plane, b),
        ) else {
            continue;
        };
        let distance = distance_to_segment(pos, sa, sb);
        if distance <= threshold {
            consider(SketchHit {
                id: line.id,
                kind: SketchEntityKind::Line,
                distance,
            });
        }
    }

    for circle in &sketch.circles {
        let Some(center) = point_of(sketch, circle.center) else {
            continue;
        };
        let samples: Vec<[f64; 2]> = (0..=64)
            .map(|step| {
                let angle = step as f64 / 64.0 * std::f64::consts::TAU;
                [
                    center[0] + circle.radius * angle.cos(),
                    center[1] + circle.radius * angle.sin(),
                ]
            })
            .collect();
        if let Some(distance) = polyline_distance(pos, &samples, rect, camera, plane) {
            if distance <= threshold {
                consider(SketchHit {
                    id: circle.id,
                    kind: SketchEntityKind::Circle,
                    distance,
                });
            }
        }
    }

    for arc in &sketch.arcs {
        let Some(center) = point_of(sketch, arc.center) else {
            continue;
        };
        let mut end = arc.end_angle;
        while end < arc.start_angle {
            end += std::f64::consts::TAU;
        }
        let samples: Vec<[f64; 2]> = (0..=48)
            .map(|step| {
                let angle = arc.start_angle + (end - arc.start_angle) * step as f64 / 48.0;
                [
                    center[0] + arc.radius * angle.cos(),
                    center[1] + arc.radius * angle.sin(),
                ]
            })
            .collect();
        if let Some(distance) = polyline_distance(pos, &samples, rect, camera, plane) {
            if distance <= threshold {
                consider(SketchHit {
                    id: arc.id,
                    kind: SketchEntityKind::Arc,
                    distance,
                });
            }
        }
    }

    best
}

fn rank(kind: SketchEntityKind) -> u8 {
    match kind {
        SketchEntityKind::Point => 2,
        _ => 1,
    }
}

fn distance_to_segment(pos: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let length_squared = ab.length_sq();
    if length_squared < 1e-9 {
        return pos.distance(a);
    }
    let t = ((pos - a).dot(ab) / length_squared).clamp(0.0, 1.0);
    pos.distance(a + ab * t)
}

/// The closest a sampled curve comes to a screen position.
fn polyline_distance(
    pos: Pos2,
    samples: &[[f64; 2]],
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
) -> Option<f32> {
    let mut best = f32::INFINITY;
    let mut previous: Option<Pos2> = None;
    for sample in samples {
        let Some(screen) = sketch_to_screen(rect, camera, plane, *sample) else {
            previous = None;
            continue;
        };
        if let Some(previous) = previous {
            best = best.min(distance_to_segment(pos, previous, screen));
        }
        previous = Some(screen);
    }
    best.is_finite().then_some(best)
}

/// The constraint actions the sketch toolbar offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintAction {
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Tangent,
    Equal,
    Symmetric,
    Concentric,
    Midpoint,
    Coincident,
    Dimension,
}

impl ConstraintAction {
    /// What the action needs selected, in the words shown when it is missing.
    pub fn requirement(self) -> &'static str {
        match self {
            ConstraintAction::Horizontal | ConstraintAction::Vertical => "select one line",
            ConstraintAction::Parallel | ConstraintAction::Perpendicular => "select two lines",
            ConstraintAction::Tangent => "select a line and a circle or arc",
            ConstraintAction::Equal => "select two lines or two circles",
            ConstraintAction::Symmetric => "select two points and an axis line",
            ConstraintAction::Concentric => "select two circles or arcs",
            ConstraintAction::Midpoint => "select a point and a line",
            ConstraintAction::Coincident => "select two points",
            ConstraintAction::Dimension => "select a line, or two points",
        }
    }
}

/// Add a constraint to the sketch from the current sketch selection.
///
/// A constraint that would be nonsense for the selection is refused with a
/// sentence naming what to select instead, so the button can explain itself.
pub fn apply_constraint(
    sketch: &mut Sketch,
    selection: &[Uuid],
    action: ConstraintAction,
) -> Result<String, String> {
    let lines = selected_of(sketch, selection, |sketch, id| {
        sketch.lines.iter().any(|line| line.id == id)
    });
    let points = selected_of(sketch, selection, |sketch, id| {
        sketch.points.iter().any(|point| point.id == id)
    });
    let curves = selected_of(sketch, selection, |sketch, id| {
        sketch.circles.iter().any(|circle| circle.id == id)
            || sketch.arcs.iter().any(|arc| arc.id == id)
    });

    let need = |what: &str| -> Result<String, String> {
        Err(format!("{}: {}", action.requirement(), what))
    };

    match action {
        ConstraintAction::Horizontal | ConstraintAction::Vertical => {
            if lines.len() != 1 {
                return need("exactly one line must be selected");
            }
            let line = lines[0];
            sketch
                .constraints
                .push(if action == ConstraintAction::Horizontal {
                    Constraint::Horizontal { line }
                } else {
                    Constraint::Vertical { line }
                });
            Ok(format!(
                "{} constraint added to the selected line.",
                if action == ConstraintAction::Horizontal {
                    "Horizontal"
                } else {
                    "Vertical"
                }
            ))
        }
        ConstraintAction::Parallel | ConstraintAction::Perpendicular => {
            if lines.len() != 2 {
                return need("exactly two lines must be selected");
            }
            sketch
                .constraints
                .push(if action == ConstraintAction::Parallel {
                    Constraint::Parallel {
                        a: lines[0],
                        b: lines[1],
                    }
                } else {
                    Constraint::Perpendicular {
                        a: lines[0],
                        b: lines[1],
                    }
                });
            Ok(format!(
                "{} added between the two selected lines.",
                if action == ConstraintAction::Parallel {
                    "Parallel"
                } else {
                    "Perpendicular"
                }
            ))
        }
        ConstraintAction::Equal => {
            if lines.len() == 2 && curves.is_empty() {
                sketch.constraints.push(Constraint::EqualLength {
                    a: lines[0],
                    b: lines[1],
                });
                return Ok("Equal length added between the two selected lines.".to_string());
            }
            if curves.len() == 2 && lines.is_empty() {
                sketch.constraints.push(Constraint::EqualRadius {
                    a: curves[0],
                    b: curves[1],
                });
                return Ok("Equal radius added between the two selected curves.".to_string());
            }
            need("select two lines, or two curves")
        }
        ConstraintAction::Tangent => {
            if lines.len() != 1 || curves.len() != 1 {
                return need("a line and a curve must be selected");
            }
            sketch.constraints.push(Constraint::Tangent {
                a: lines[0],
                b: curves[0],
            });
            Ok("Tangent added between the line and the curve.".to_string())
        }
        ConstraintAction::Concentric => {
            if curves.len() != 2 {
                return need("exactly two curves must be selected");
            }
            sketch.constraints.push(Constraint::Concentric {
                a: curves[0],
                b: curves[1],
            });
            Ok("Concentric added between the two selected curves.".to_string())
        }
        ConstraintAction::Midpoint => {
            if points.len() != 1 || lines.len() != 1 {
                return need("a point and a line must be selected");
            }
            sketch.constraints.push(Constraint::Midpoint {
                point: points[0],
                line: lines[0],
            });
            Ok("Midpoint added: the point is held at the middle of the line.".to_string())
        }
        ConstraintAction::Coincident => {
            if points.len() != 2 {
                return need("exactly two points must be selected");
            }
            sketch.constraints.push(Constraint::Coincident {
                a: points[0],
                b: points[1],
            });
            Ok("Coincident added: the two points are made one.".to_string())
        }
        ConstraintAction::Symmetric => {
            if points.len() != 2 || lines.len() != 1 {
                return need("two points and one axis line must be selected");
            }
            sketch.constraints.push(Constraint::Symmetric {
                a: points[0],
                b: points[1],
                axis_line: lines[0],
            });
            Ok("Symmetric added about the selected line.".to_string())
        }
        ConstraintAction::Dimension => {
            let (a, b, label) = if lines.len() == 1 && points.is_empty() {
                let line = sketch
                    .lines
                    .iter()
                    .find(|line| line.id == lines[0])
                    .ok_or_else(|| "the selected line is gone".to_string())?;
                (line.a, line.b, "line")
            } else if points.len() == 2 && lines.is_empty() && curves.is_empty() {
                (points[0], points[1], "points")
            } else {
                return need("select a line, or exactly two points");
            };
            if sketch.constraints.iter().any(|constraint| {
                matches!(constraint, Constraint::Distance { a: ra, b: rb, .. }
                    if (ra == &a && rb == &b) || (ra == &b && rb == &a))
            }) {
                return Err("that distance already has a dimension".to_string());
            }
            let (Some(pa), Some(pb)) = (point_of(sketch, a), point_of(sketch, b)) else {
                return Err("the selected geometry has no points".to_string());
            };
            let value = (pb[0] - pa[0]).hypot(pb[1] - pa[1]);
            sketch.add_dimension(a, b, value, true, true);
            Ok(format!(
                "Dimension added: {} = {}",
                label,
                format_length(value)
            ))
        }
    }
}

/// The selected ids that name a given kind, keeping the selection order.
fn selected_of(
    sketch: &Sketch,
    selection: &[Uuid],
    is_kind: impl Fn(&Sketch, Uuid) -> bool,
) -> Vec<Uuid> {
    selection
        .iter()
        .copied()
        .filter(|id| is_kind(sketch, *id))
        .collect()
}

/// Everything the overlay needs. A struct because there are more than a couple.
pub struct OverlayState<'a> {
    pub sketch: &'a Sketch,
    pub plane: SketchPlane,
    /// Sketch entities selected in the viewport.
    pub selection: &'a [Uuid],
    pub hovered: Option<Uuid>,
    pub draft: &'a SketchDraft,
    /// The snapped cursor, in sketch coordinates.
    pub cursor: Option<[f64; 2]>,
}

/// Draw the open sketch: its geometry, its constraints, its dimensions, the
/// points that are still free, and the shape the mouse is about to finish.
pub fn draw_sketch_overlay(
    ui: &mut egui::Ui,
    rect: Rect,
    camera: &OrbitCamera,
    state: &OverlayState<'_>,
) {
    let painter = ui.painter_at(rect);
    let plane = state.plane;
    let sketch = state.sketch;
    let style = |id: Uuid| -> (Color32, f32) {
        if state.selection.contains(&id) {
            (SELECTED, 2.6)
        } else if state.hovered == Some(id) {
            (HOVERED, 2.6)
        } else {
            (GEOMETRY, 1.8)
        }
    };

    for line in &sketch.lines {
        let (Some(a), Some(b)) = (point_of(sketch, line.a), point_of(sketch, line.b)) else {
            continue;
        };
        let (Some(sa), Some(sb)) = (
            sketch_to_screen(rect, camera, plane, a),
            sketch_to_screen(rect, camera, plane, b),
        ) else {
            continue;
        };
        let (colour, width) = style(line.id);
        painter.line_segment([sa, sb], Stroke::new(width, colour));
    }

    for spline in &sketch.splines {
        let samples: Vec<Pos2> = spline
            .control_points
            .iter()
            .filter_map(|id| point_of(sketch, *id))
            .filter_map(|point| sketch_to_screen(rect, camera, plane, point))
            .collect();
        if samples.len() >= 2 {
            let (colour, _) = style(spline.id);
            for shape in Shape::dashed_line(&samples, Stroke::new(1.0_f32, colour), 4.0, 3.0) {
                painter.add(shape);
            }
        }
    }

    for circle in &sketch.circles {
        let Some(center) = point_of(sketch, circle.center) else {
            continue;
        };
        let samples: Vec<Pos2> = (0..=64)
            .filter_map(|step| {
                let angle = step as f64 / 64.0 * std::f64::consts::TAU;
                sketch_to_screen(
                    rect,
                    camera,
                    plane,
                    [
                        center[0] + circle.radius * angle.cos(),
                        center[1] + circle.radius * angle.sin(),
                    ],
                )
            })
            .collect();
        if samples.len() >= 2 {
            let (colour, width) = style(circle.id);
            painter.add(Shape::line(samples, Stroke::new(width, colour)));
        }
    }

    for arc in &sketch.arcs {
        let Some(center) = point_of(sketch, arc.center) else {
            continue;
        };
        let mut end = arc.end_angle;
        while end < arc.start_angle {
            end += std::f64::consts::TAU;
        }
        let samples: Vec<Pos2> = (0..=48)
            .filter_map(|step| {
                let angle = arc.start_angle + (end - arc.start_angle) * step as f64 / 48.0;
                sketch_to_screen(
                    rect,
                    camera,
                    plane,
                    [
                        center[0] + arc.radius * angle.cos(),
                        center[1] + arc.radius * angle.sin(),
                    ],
                )
            })
            .collect();
        if samples.len() >= 2 {
            let (colour, width) = style(arc.id);
            painter.add(Shape::line(samples, Stroke::new(width, colour)));
        }
    }

    // Points, with the ones no constraint holds marked, so the degrees of
    // freedom are visible on the geometry and not only in the panel.
    let free = free_points(sketch);
    for point in &sketch.points {
        let Some(screen) = sketch_to_screen(rect, camera, plane, [point.x, point.y]) else {
            continue;
        };
        let (colour, _) = style(point.id);
        painter.rect_filled(
            Rect::from_center_size(screen, Vec2::splat(3.0)),
            0.0,
            colour,
        );
        if free.contains(&point.id) {
            painter.circle_stroke(screen, 5.0, Stroke::new(1.0_f32, FREE_POINT));
        }
    }

    // Constraint markers, staggered when several share a place.
    let mut used: std::collections::HashMap<(i64, i64), usize> = std::collections::HashMap::new();
    for glyph in constraint_glyphs(sketch) {
        let Some(screen) = sketch_to_screen(rect, camera, plane, glyph.point) else {
            continue;
        };
        let key = (
            (glyph.point[0] * 1000.0).round() as i64,
            (glyph.point[1] * 1000.0).round() as i64,
        );
        let slot = used.entry(key).or_insert(0);
        let offset = *slot as f32 * 13.0;
        *slot += 1;
        painter.text(
            screen + Vec2::new(offset, -11.0),
            Align2::CENTER_CENTER,
            glyph.label,
            FontId::proportional(11.0),
            CONSTRAINT,
        );
    }

    for dimension in dimensions(sketch) {
        draw_dimension(&painter, rect, camera, plane, &dimension);
    }

    // The shape the first click started.
    if let (Some(first), Some(cursor)) = (state.draft.pending(), state.cursor) {
        match state.draft.tool() {
            SketchTool::Rectangle => {
                let corners = [first, [cursor[0], first[1]], cursor, [first[0], cursor[1]]];
                let screen: Vec<Pos2> = corners
                    .iter()
                    .filter_map(|corner| sketch_to_screen(rect, camera, plane, *corner))
                    .collect();
                if screen.len() == 4 {
                    painter.add(Shape::closed_line(screen, Stroke::new(1.4_f32, PREVIEW)));
                }
            }
            SketchTool::Line => {
                if let (Some(a), Some(b)) = (
                    sketch_to_screen(rect, camera, plane, first),
                    sketch_to_screen(rect, camera, plane, cursor),
                ) {
                    painter.line_segment([a, b], Stroke::new(1.4_f32, PREVIEW));
                }
            }
            SketchTool::Circle => {
                let radius = (cursor[0] - first[0]).hypot(cursor[1] - first[1]);
                let samples: Vec<Pos2> = (0..=64)
                    .filter_map(|step| {
                        let angle = step as f64 / 64.0 * std::f64::consts::TAU;
                        sketch_to_screen(
                            rect,
                            camera,
                            plane,
                            [
                                first[0] + radius * angle.cos(),
                                first[1] + radius * angle.sin(),
                            ],
                        )
                    })
                    .collect();
                if samples.len() >= 2 {
                    painter.add(Shape::line(samples, Stroke::new(1.4_f32, PREVIEW)));
                }
            }
            _ => {}
        }
    }

    // Where the next click would land, and at which grid node.
    if let Some(cursor) = state.cursor {
        if let Some(screen) = sketch_to_screen(rect, camera, plane, cursor) {
            let stroke = Stroke::new(1.0_f32, PREVIEW);
            painter.line_segment(
                [screen - Vec2::new(7.0, 0.0), screen + Vec2::new(7.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [screen - Vec2::new(0.0, 7.0), screen + Vec2::new(0.0, 7.0)],
                stroke,
            );
            painter.text(
                screen + Vec2::new(10.0, 14.0),
                Align2::LEFT_TOP,
                format!("{:.1}, {:.1}", cursor[0], cursor[1]),
                FontId::monospace(11.0),
                TEXT,
            );
        }
    }
}

fn draw_dimension(
    painter: &egui::Painter,
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
    dimension: &DimensionAnnotation,
) {
    let (Some(a), Some(b)) = (
        sketch_to_screen(rect, camera, plane, dimension.a),
        sketch_to_screen(rect, camera, plane, dimension.b),
    ) else {
        return;
    };
    let direction = b - a;
    let length = direction.length();
    if length < 1e-3 {
        return;
    }
    let along = direction / length;
    let normal = Vec2::new(-along.y, along.x);
    let offset = normal * DIMENSION_OFFSET;
    let (a2, b2) = (a + offset, b + offset);

    let colour = if !dimension.driving {
        DRIVEN
    } else if dimension.weak {
        WEAK
    } else {
        DIMENSION
    };
    let thin = Stroke::new(1.0_f32, colour);
    let body = Stroke::new(1.4_f32, colour);

    // The extension lines from the geometry to the dimension line.
    painter.line_segment([a, a2], thin);
    painter.line_segment([b, b2], thin);
    // The dimension line, with an arrowhead at each end.
    painter.line_segment([a2, b2], body);
    for (tip, sign) in [(a2, 1.0), (b2, -1.0)] {
        let back = tip + along * sign * 9.0;
        painter.line_segment([tip, back + normal * 3.0], body);
        painter.line_segment([tip, back - normal * 3.0], body);
    }

    let text = if dimension.driving {
        dimension.text.clone()
    } else {
        format!("({})", dimension.text)
    };
    let text_position = a2 + (b2 - a2) * 0.5 + normal * 11.0;
    painter.text(
        text_position,
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        colour,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xy_plane() -> SketchPlane {
        ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])
    }

    /// A camera looking straight down at the XY plane, the view a sketch on it
    /// needs: from the default direction the plane is edge-on and a screen ray
    /// never meets it.
    fn top_camera() -> OrbitCamera {
        let mut camera = OrbitCamera::default();
        camera.look_from(Vec3::Z);
        camera
    }

    #[test]
    fn snapping_rounds_to_the_nearest_grid_line() {
        assert_eq!(snap_to_grid([12.4, -7.6], 5.0), [10.0, -10.0]);
        assert_eq!(snap_to_grid([12.4, -7.6], 0.0), [12.4, -7.6]);
        assert_eq!(snap_to_grid([13.0, 0.0], 2.0), [14.0, 0.0]);
    }

    #[test]
    fn two_clicks_draw_a_rectangle_with_snapped_corners() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Rectangle);

        assert_eq!(
            draft.click(&mut sketch, snap_to_grid([12.0, 7.0], GRID_SPACING)),
            SketchEdit::Pending
        );
        assert!(
            sketch.lines.is_empty(),
            "one click must not draw anything yet"
        );
        assert_eq!(draft.pending(), Some([10.0, 5.0]));

        let result = draft.click(&mut sketch, snap_to_grid([38.0, 26.0], GRID_SPACING));
        assert!(matches!(result, SketchEdit::Rectangle(_)));
        assert_eq!(sketch.lines.len(), 4);
        assert_eq!(sketch.points.len(), 4);
        assert_eq!(draft.pending(), None);

        let corners: Vec<[f64; 2]> = sketch.points.iter().map(|p| [p.x, p.y]).collect();
        for expected in [[10.0, 5.0], [40.0, 5.0], [40.0, 25.0], [10.0, 25.0]] {
            assert!(
                corners.contains(&expected),
                "corner {expected:?} missing from {corners:?}"
            );
        }
        assert_eq!(sketch.constraint_status(), "under-constrained");
    }

    #[test]
    fn a_degenerate_rectangle_keeps_the_first_corner() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Rectangle);
        draft.click(&mut sketch, [0.0, 0.0]);

        // Same y: no area, so nothing is created and the corner is kept.
        assert_eq!(draft.click(&mut sketch, [20.0, 0.0]), SketchEdit::TooSmall);
        assert!(sketch.lines.is_empty());
        assert_eq!(draft.pending(), Some([0.0, 0.0]));

        assert!(matches!(
            draft.click(&mut sketch, [20.0, 20.0]),
            SketchEdit::Rectangle(_)
        ));
    }

    #[test]
    fn line_and_circle_take_two_clicks() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();

        draft.begin(SketchTool::Line);
        draft.click(&mut sketch, [0.0, 0.0]);
        assert!(matches!(
            draft.click(&mut sketch, [30.0, 40.0]),
            SketchEdit::Line(_)
        ));
        assert_eq!(sketch.lines.len(), 1);

        draft.begin(SketchTool::Circle);
        draft.click(&mut sketch, [0.0, 0.0]);
        assert!(matches!(
            draft.click(&mut sketch, [3.0, 4.0]),
            SketchEdit::Circle(_)
        ));
        assert_eq!(sketch.circles.len(), 1);
        assert_eq!(sketch.circles[0].radius, 5.0);
    }

    #[test]
    fn the_point_tool_places_one_point_per_click() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Point);
        assert!(matches!(
            draft.click(&mut sketch, [5.0, 5.0]),
            SketchEdit::Point(_)
        ));
        assert!(matches!(
            draft.click(&mut sketch, [9.0, 1.0]),
            SketchEdit::Point(_)
        ));
        assert_eq!(sketch.points.len(), 2);
    }

    #[test]
    fn escape_drops_the_pending_click_then_the_tool() {
        let mut draft = SketchDraft::default();
        // An armed tool with no first click: Escape leaves the tool.
        draft.begin(SketchTool::Rectangle);
        assert!(draft.cancel(), "the tool is left first");
        assert_eq!(draft.tool(), SketchTool::Select);
        assert!(!draft.cancel(), "nothing left to cancel");

        // With a first click, Escape drops that click and keeps the tool.
        draft.begin(SketchTool::Rectangle);
        let mut sketch = Sketch::new("test");
        draft.click(&mut sketch, [0.0, 0.0]);
        assert!(draft.cancel(), "the pending corner is cancelled");
        assert_eq!(draft.pending(), None);
        assert_eq!(draft.tool(), SketchTool::Rectangle, "the tool stays");

        assert!(draft.cancel(), "then the tool itself is left");
        assert_eq!(draft.tool(), SketchTool::Select);
        assert!(!draft.cancel(), "and then there is nothing left to cancel");
    }

    #[test]
    fn the_screen_and_the_sketch_round_trip_through_the_plane() {
        let camera = top_camera();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let plane = xy_plane();

        for point in [[0.0, 0.0], [25.0, -10.0], [-7.5, 3.0]] {
            let screen = sketch_to_screen(rect, &camera, plane, point).expect("in view");
            let back = screen_to_sketch(screen, rect, &camera, plane).expect("on the plane");
            assert!(
                (back[0] - point[0]).abs() < 1e-3 && (back[1] - point[1]).abs() < 1e-3,
                "{point:?} came back as {back:?}"
            );
        }
    }

    #[test]
    fn a_click_on_a_line_hits_that_line() {
        let camera = top_camera();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let plane = xy_plane();
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(-20.0, 0.0);
        let b = sketch.add_point(20.0, 0.0);
        let line = sketch.add_line(a, b);

        let midpoint = sketch_to_screen(rect, &camera, plane, [0.0, 0.0]).unwrap();
        let hit = hit_test(&sketch, rect, &camera, plane, midpoint, HIT_RADIUS).unwrap();
        assert_eq!(hit.id, line);
        assert_eq!(hit.kind, SketchEntityKind::Line);

        // Far away, nothing is hit.
        let away = midpoint + Vec2::new(0.0, 200.0);
        assert!(hit_test(&sketch, rect, &camera, plane, away, HIT_RADIUS).is_none());
    }

    #[test]
    fn constraints_are_marked_on_the_geometry_they_hold() {
        let mut sketch = Sketch::new("test");
        sketch.add_rectangle(0.0, 0.0, 10.0, 20.0);
        let glyphs = constraint_glyphs(&sketch);
        let labels: Vec<&str> = glyphs.iter().map(|glyph| glyph.label).collect();
        assert_eq!(labels.len(), 4);
        assert_eq!(labels.iter().filter(|label| **label == "H").count(), 2);
        assert_eq!(labels.iter().filter(|label| **label == "V").count(), 2);
    }

    #[test]
    fn a_dimension_reports_the_measured_length() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(30.0, 40.0);
        sketch.add_dimension(a, b, 50.0, true, true);

        let dimensions = dimensions(&sketch);
        assert_eq!(dimensions.len(), 1);
        assert_eq!(dimensions[0].value, 50.0);
        assert_eq!(dimensions[0].measured, 50.0);
        assert_eq!(dimensions[0].text, "50");
        assert!(dimensions[0].driving);
        assert!(!dimensions[0].weak);
    }

    #[test]
    fn a_driven_dimension_is_marked_as_a_reference() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(3.0, 4.0);
        sketch.add_dimension(a, b, 5.0, false, false);

        let dimensions = dimensions(&sketch);
        assert_eq!(dimensions.len(), 1);
        assert!(!dimensions[0].driving);
        assert!(dimensions[0].weak);
    }

    #[test]
    fn free_points_are_the_ones_nothing_anchors() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let c = sketch.add_point(10.0, 10.0);
        let line = sketch.add_line(a, b);

        // Nothing anchors a, b or c yet.
        assert_eq!(free_points(&sketch).len(), 3);

        // Horizontal aligns the line but fixes nothing: only the Fixed point
        // is anchored.
        sketch.constraints.push(Constraint::Fixed { point: a });
        sketch.constraints.push(Constraint::Horizontal { line });
        let free = free_points(&sketch);
        assert!(!free.contains(&a), "a fixed point is anchored");
        assert!(
            free.contains(&b),
            "horizontal alone does not pin b, it only takes away one freedom"
        );
        assert!(free.contains(&c), "c is still free");

        // A coincidence with the anchored point pins b as well.
        sketch.constraints.push(Constraint::Coincident { a, b });
        assert!(!free_points(&sketch).contains(&b));
    }

    #[test]
    fn constraints_need_the_right_selection_and_then_apply() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 1.0);
        let c = sketch.add_point(10.0, 11.0);
        let line = sketch.add_line(a, b);
        let other = sketch.add_line(b, c);

        let error = apply_constraint(&mut sketch, &[line], ConstraintAction::Parallel)
            .expect_err("one line is not enough");
        assert!(error.contains("select two lines"), "{error}");

        apply_constraint(&mut sketch, &[line, other], ConstraintAction::Perpendicular)
            .expect("two lines can be perpendicular");
        assert!(
            sketch
                .constraints
                .iter()
                .any(|constraint| matches!(constraint, Constraint::Perpendicular { .. }))
        );
    }

    #[test]
    fn the_dimension_action_measures_the_selected_line() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(3.0, 4.0);
        let line = sketch.add_line(a, b);

        let message =
            apply_constraint(&mut sketch, &[line], ConstraintAction::Dimension).expect("dimension");
        assert!(message.contains('5'), "{message}");
        let dimensions = dimensions(&sketch);
        assert_eq!(dimensions.len(), 1);
        assert_eq!(dimensions[0].value, 5.0);
        assert!(dimensions[0].driving);

        // A second dimension on the same pair is refused, not doubled.
        assert!(apply_constraint(&mut sketch, &[line], ConstraintAction::Dimension).is_err());
    }
}
