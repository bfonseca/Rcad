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
const REFERENCE: Color32 = Color32::from_rgb(130, 145, 170);

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
    /// Three clicks: the start, the end, and a point the arc passes through.
    Arc3Point,
    /// Click the control points, then Enter to finish the spline.
    Spline,
    /// Clicks a line and dimensions its length.
    Dimension,
    /// Clicks a line and trims the clicked end back to its nearest crossing.
    Trim,
    /// Clicks a line to grow, then the entity it should grow to.
    Extend,
    /// Clicks a line and offsets it by the distance in the sketch strip.
    Offset,
    /// Uses the sketch selection as the geometry, the clicked line as the axis.
    Mirror,
    /// Uses the sketch selection as the geometry, the clicked line as direction.
    Pattern,
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
            SketchTool::Arc3Point => "3-Point Arc",
            SketchTool::Spline => "Spline",
            SketchTool::Dimension => "Dimension",
            SketchTool::Trim => "Trim",
            SketchTool::Extend => "Extend",
            SketchTool::Offset => "Offset",
            SketchTool::Mirror => "Mirror",
            SketchTool::Pattern => "Pattern",
        }
    }

    /// True when the tool creates geometry with clicks instead of picking it.
    pub fn creates_geometry(self) -> bool {
        matches!(
            self,
            SketchTool::Point
                | SketchTool::Line
                | SketchTool::Rectangle
                | SketchTool::Circle
                | SketchTool::Arc3Point
                | SketchTool::Spline
        )
    }

    /// True when the tool acts on entities the user clicks.
    pub fn picks_entities(self) -> bool {
        matches!(
            self,
            SketchTool::Dimension
                | SketchTool::Trim
                | SketchTool::Extend
                | SketchTool::Offset
                | SketchTool::Mirror
                | SketchTool::Pattern
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
            SketchTool::Arc3Point => "Click the start, the end, then a point on the arc.",
            SketchTool::Spline => "Click the control points, then Enter to finish.",
            SketchTool::Dimension => "Click a line to dimension its length; double-click to edit.",
            SketchTool::Trim => "Click the end of a line to trim it to its nearest crossing.",
            SketchTool::Extend => "Click the line to grow, then the entity it should reach.",
            SketchTool::Offset => "Click a line to offset it by the strip's distance.",
            SketchTool::Mirror => "Select the geometry, then click the mirror axis line.",
            SketchTool::Pattern => "Select the geometry, then click a line for the direction.",
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
    Arc(Uuid),
    Spline(Uuid),
    /// The two clicks were on top of each other; the first one is kept.
    TooSmall,
    /// The tool cannot make anything of these picks; the sentence says why.
    Refused(&'static str),
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
                | SketchEdit::Arc(_)
                | SketchEdit::Spline(_)
        )
    }
}

/// The active sketch tool, the clicks a creation tool has collected, and the
/// entity picks a command tool has collected.
///
/// This is the whole mouse state. Keeping it out of `RcadApp` means the
/// interaction rules can be tested directly.
#[derive(Debug, Clone, Default)]
pub struct SketchDraft {
    tool: SketchTool,
    points: Vec<[f64; 2]>,
    entities: Vec<Uuid>,
}

impl SketchDraft {
    pub fn tool(&self) -> SketchTool {
        self.tool
    }

    /// The first click of a two-click tool, in sketch coordinates.
    pub fn pending(&self) -> Option<[f64; 2]> {
        self.points.first().copied()
    }

    /// The clicks a creation tool has collected so far.
    pub fn pending_points(&self) -> &[[f64; 2]] {
        &self.points
    }

    /// The first entity a command tool has picked, if it needs two.
    pub fn pending_entity(&self) -> Option<Uuid> {
        self.entities.first().copied()
    }

    /// Start a tool: any half-finished shape is dropped.
    pub fn begin(&mut self, tool: SketchTool) {
        self.tool = tool;
        self.points.clear();
        self.entities.clear();
    }

    /// Escape: drop the collected clicks or picks, then leave the tool.
    ///
    /// Returns true when something was actually cancelled, so the caller can
    /// fall back to clearing the selection only when the sketch had nothing to
    /// cancel.
    pub fn cancel(&mut self) -> bool {
        if !self.points.is_empty() {
            self.points.clear();
            return true;
        }
        if !self.entities.is_empty() {
            self.entities.clear();
            return true;
        }
        if self.tool != SketchTool::Select {
            self.tool = SketchTool::Select;
            return true;
        }
        false
    }

    /// Replace the entity picks, after a command tool has consumed them.
    pub fn set_picks(&mut self, picks: Vec<Uuid>) {
        self.entities = picks;
    }

    /// Feed one click, in sketch coordinates, already snapped to the grid.
    pub fn click(&mut self, sketch: &mut Sketch, point: [f64; 2]) -> SketchEdit {
        match self.tool {
            SketchTool::Point => SketchEdit::Point(sketch.add_point(point[0], point[1])),
            SketchTool::Line | SketchTool::Rectangle | SketchTool::Circle => {
                self.points.push(point);
                if self.points.len() < 2 {
                    return SketchEdit::Pending;
                }
                let first = self.points[0];
                let second = self.points[1];
                let dx = second[0] - first[0];
                let dy = second[1] - first[1];
                let edit = match self.tool {
                    SketchTool::Line => {
                        if dx.hypot(dy) < 1e-9 {
                            // Keep the first click: the next one can still end
                            // the line.
                            self.points.truncate(1);
                            return SketchEdit::TooSmall;
                        }
                        let a = sketch.add_point(first[0], first[1]);
                        let b = sketch.add_point(second[0], second[1]);
                        SketchEdit::Line(sketch.add_line(a, b))
                    }
                    SketchTool::Rectangle => {
                        if dx.abs() < 1e-9 || dy.abs() < 1e-9 {
                            self.points.truncate(1);
                            return SketchEdit::TooSmall;
                        }
                        SketchEdit::Rectangle(
                            sketch.add_rectangle(first[0], first[1], second[0], second[1]),
                        )
                    }
                    _ => {
                        let radius = dx.hypot(dy);
                        if radius < 1e-9 {
                            self.points.truncate(1);
                            return SketchEdit::TooSmall;
                        }
                        SketchEdit::Circle(sketch.add_circle(first[0], first[1], radius))
                    }
                };
                self.points.clear();
                edit
            }
            SketchTool::Arc3Point => {
                self.points.push(point);
                if self.points.len() < 3 {
                    return SketchEdit::Pending;
                }
                let (start, end, through) = (self.points[0], self.points[1], self.points[2]);
                match arc_through_points(start, end, through) {
                    Some((centre, radius, start_angle, end_angle)) => {
                        let arc =
                            sketch.add_arc(centre[0], centre[1], radius, start_angle, end_angle);
                        self.points.clear();
                        SketchEdit::Arc(arc)
                    }
                    None => {
                        // Keep the first two clicks so another third one can
                        // still bend the arc.
                        self.points.pop();
                        SketchEdit::Refused(
                            "The three points are on a line: no arc passes through them.",
                        )
                    }
                }
            }
            SketchTool::Spline => {
                self.points.push(point);
                SketchEdit::Pending
            }
            _ => SketchEdit::NotDrawing,
        }
    }

    /// Finish the spline the spline tool has been collecting (Enter).
    pub fn finish(&mut self, sketch: &mut Sketch) -> Option<SketchEdit> {
        if self.tool != SketchTool::Spline || self.points.len() < 2 {
            return None;
        }
        let control: Vec<Uuid> = self
            .points
            .iter()
            .map(|point| sketch.add_point(point[0], point[1]))
            .collect();
        let degree = 3.min(control.len() as u32 - 1).max(1);
        let spline = sketch.add_spline(&control, degree);
        self.points.clear();
        Some(SketchEdit::Spline(spline))
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

/// The circle through three points, and the arc from `start` to `end` that
/// passes through `through`: centre, radius, start angle, end angle (radians,
/// counter-clockwise).
///
/// Returns None when the points are collinear or the start and end coincide:
/// no circle passes through three points on a line.
pub fn arc_through_points(
    start: [f64; 2],
    end: [f64; 2],
    through: [f64; 2],
) -> Option<([f64; 2], f64, f64, f64)> {
    let (ax, ay) = (start[0], start[1]);
    let (bx, by) = (end[0], end[1]);
    let (cx, cy) = (through[0], through[1]);

    // Twice the signed area of the triangle: zero when the points are collinear.
    let determinant = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if determinant.abs() < 1e-9 {
        return None;
    }
    let a2 = ax * ax + ay * ay;
    let b2 = bx * bx + by * by;
    let c2 = cx * cx + cy * cy;
    let centre = [
        (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / determinant,
        (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / determinant,
    ];
    let radius = (ax - centre[0]).hypot(ay - centre[1]);
    if radius < 1e-9 {
        return None;
    }

    let angle_of = |point: [f64; 2]| (point[1] - centre[1]).atan2(point[0] - centre[0]);
    let start_angle = angle_of(start);
    let end_angle = angle_of(end);
    if (end_angle - start_angle).rem_euclid(std::f64::consts::TAU) < 1e-9 {
        return None;
    }
    let through_angle = angle_of(through);

    // The arc from start to end passing through the third point is the counter
    // clockwise one when that point lies inside it; otherwise the arc runs the
    // other way round.
    let tau = std::f64::consts::TAU;
    let end_offset = (end_angle - start_angle).rem_euclid(tau);
    let through_offset = (through_angle - start_angle).rem_euclid(tau);
    Some(if through_offset <= end_offset {
        (centre, radius, start_angle, end_angle)
    } else {
        (centre, radius, end_angle, start_angle)
    })
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
    /// Where the dimension's `Distance` constraint sits in `Sketch::constraints`,
    /// so an edit can reach it.
    pub constraint_index: usize,
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
    for (constraint_index, constraint) in sketch.constraints.iter().enumerate() {
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
            constraint_index,
        });
    }
    dimensions
}

/// The screen geometry of a dimension: the two measured points, the offset
/// dimension line, and where the value is written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimensionScreen {
    pub a: Pos2,
    pub b: Pos2,
    pub line_a: Pos2,
    pub line_b: Pos2,
    pub text: Pos2,
}

/// Where a dimension lands on screen, using the same offset the drawing uses.
pub fn dimension_screen_geometry(
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
    dimension: &DimensionAnnotation,
) -> Option<DimensionScreen> {
    let a = sketch_to_screen(rect, camera, plane, dimension.a)?;
    let b = sketch_to_screen(rect, camera, plane, dimension.b)?;
    let direction = b - a;
    let length = direction.length();
    if length < 1e-3 {
        return None;
    }
    let along = direction / length;
    let normal = Vec2::new(-along.y, along.x);
    let offset = normal * DIMENSION_OFFSET;
    let line_a = a + offset;
    let line_b = b + offset;
    Some(DimensionScreen {
        a,
        b,
        line_a,
        line_b,
        text: line_a + (line_b - line_a) * 0.5 + normal * 11.0,
    })
}

/// The index in `Sketch::constraints` of the dimension under a screen position.
pub fn dimension_hit_test(
    sketch: &Sketch,
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
    pos: Pos2,
    threshold: f32,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for dimension in dimensions(sketch) {
        let Some(screen) = dimension_screen_geometry(rect, camera, plane, &dimension) else {
            continue;
        };
        let distance = distance_to_segment(pos, screen.line_a, screen.line_b)
            .min(pos.distance(screen.text))
            .min(distance_to_segment(pos, screen.a, screen.line_a))
            .min(distance_to_segment(pos, screen.b, screen.line_b));
        if distance <= threshold && best.map_or(true, |(_, best)| distance < best) {
            best = Some((dimension.constraint_index, distance));
        }
    }
    best.map(|(index, _)| index)
}

/// Move a sketch point to a place, in sketch coordinates.
pub fn move_point(sketch: &mut Sketch, point: Uuid, to: [f64; 2]) -> bool {
    match sketch.point_mut(point) {
        Some(target) => {
            target.x = to[0];
            target.y = to[1];
            true
        }
        None => false,
    }
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

/// Parameters the command tools take from the sketch strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolParams {
    pub offset_distance: f64,
    pub pattern_count: usize,
    pub pattern_spacing: f64,
}

impl Default for ToolParams {
    fn default() -> Self {
        Self {
            offset_distance: 5.0,
            pattern_count: 3,
            pattern_spacing: 10.0,
        }
    }
}

/// Feed one entity pick to a command tool (trim, extend, offset, mirror,
/// pattern). Returns the sentence to show, or the reason it cannot be done.
///
/// `picks` is the command tool's own memory: Extend needs two clicks, the rest
/// need one, so it is cleared as soon as the operation runs.
pub fn command_step(
    sketch: &mut Sketch,
    tool: SketchTool,
    picks: &mut Vec<Uuid>,
    selection: &[Uuid],
    cursor: [f64; 2],
    hit: Option<&SketchHit>,
    params: &ToolParams,
) -> Result<String, String> {
    let hit = hit.ok_or_else(|| "Nothing under the cursor: click an entity.".to_string())?;
    match tool {
        SketchTool::Trim => {
            if hit.kind != SketchEntityKind::Line {
                return Err("Trim works on lines.".to_string());
            }
            sketch
                .trim_entity(hit.id, cursor)
                .map_err(|error| error.to_string())?;
            Ok("Line trimmed back to its nearest crossing.".to_string())
        }
        SketchTool::Extend => {
            if picks.is_empty() {
                if hit.kind != SketchEntityKind::Line {
                    return Err("Extend grows a line: click a line first.".to_string());
                }
                picks.push(hit.id);
                return Ok("Now click the entity the line should reach.".to_string());
            }
            let line = picks[0];
            picks.clear();
            sketch
                .extend_entity(line, hit.id)
                .map_err(|error| error.to_string())?;
            Ok("Line extended to the target.".to_string())
        }
        SketchTool::Offset => {
            if hit.kind != SketchEntityKind::Line {
                return Err("Offset works on lines.".to_string());
            }
            sketch
                .offset_entity(hit.id, params.offset_distance)
                .map_err(|error| error.to_string())?;
            Ok(format!(
                "Offset line created {} mm away.",
                format_length(params.offset_distance)
            ))
        }
        SketchTool::Mirror => {
            if selection.is_empty() {
                return Err("Select the geometry to mirror first.".to_string());
            }
            let created = sketch
                .mirror_entities(selection, hit.id)
                .map_err(|error| error.to_string())?;
            Ok(format!("{} mirrored entities created.", created.len()))
        }
        SketchTool::Pattern => {
            if selection.is_empty() {
                return Err("Select the geometry to pattern first.".to_string());
            }
            let line = sketch
                .lines
                .iter()
                .find(|line| line.id == hit.id)
                .ok_or_else(|| "Pick a line to give the pattern direction.".to_string())?;
            let (from, to) = (
                point_of(sketch, line.a)
                    .ok_or_else(|| "The direction line has no start point.".to_string())?,
                point_of(sketch, line.b)
                    .ok_or_else(|| "The direction line has no end point.".to_string())?,
            );
            let direction = [to[0] - from[0], to[1] - from[1]];
            let created = sketch
                .linear_pattern(
                    selection,
                    direction,
                    params.pattern_spacing,
                    params.pattern_count.max(2),
                )
                .map_err(|error| error.to_string())?;
            Ok(format!("{} patterned entities created.", created.len()))
        }
        other => Err(format!("{} does not pick entities.", other.label())),
    }
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
    /// The dimension selected in the viewport, by its index in the sketch
    /// constraints.
    pub selected_dimension: Option<usize>,
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

    // Reference geometry projected from the model: dashed and behind
    // everything the sketch itself owns.
    for reference in &sketch.reference_geometry {
        let samples: Vec<Pos2> = reference
            .projected_points
            .iter()
            .filter_map(|point| sketch_to_screen(rect, camera, plane, *point))
            .collect();
        if samples.len() >= 2 {
            for shape in Shape::dashed_line(&samples, Stroke::new(1.0_f32, REFERENCE), 5.0, 4.0) {
                painter.add(shape);
            }
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
        let selected = state.selected_dimension == Some(dimension.constraint_index);
        draw_dimension(&painter, rect, camera, plane, &dimension, selected);
    }

    // The shape the clicks so far would finish.
    if let Some(cursor) = state.cursor {
        let tool = state.draft.tool();
        let screen = |point: [f64; 2]| sketch_to_screen(rect, camera, plane, point);
        match tool {
            SketchTool::Rectangle => {
                if let Some(first) = state.draft.pending() {
                    let corners = [first, [cursor[0], first[1]], cursor, [first[0], cursor[1]]];
                    let corners: Vec<Pos2> = corners
                        .iter()
                        .filter_map(|corner| screen(*corner))
                        .collect();
                    if corners.len() == 4 {
                        painter.add(Shape::closed_line(corners, Stroke::new(1.4_f32, PREVIEW)));
                    }
                }
            }
            SketchTool::Line => {
                if let (Some(first), Some(end)) = (state.draft.pending(), screen(cursor)) {
                    if let Some(start) = screen(first) {
                        painter.line_segment([start, end], Stroke::new(1.4_f32, PREVIEW));
                    }
                }
            }
            SketchTool::Circle => {
                if let Some(first) = state.draft.pending() {
                    let radius = (cursor[0] - first[0]).hypot(cursor[1] - first[1]);
                    let samples: Vec<Pos2> = (0..=64)
                        .filter_map(|step| {
                            let angle = step as f64 / 64.0 * std::f64::consts::TAU;
                            screen([
                                first[0] + radius * angle.cos(),
                                first[1] + radius * angle.sin(),
                            ])
                        })
                        .collect();
                    if samples.len() >= 2 {
                        painter.add(Shape::line(samples, Stroke::new(1.4_f32, PREVIEW)));
                    }
                }
            }
            SketchTool::Arc3Point => match state.draft.pending_points() {
                [first] => {
                    if let (Some(a), Some(b)) = (screen(*first), screen(cursor)) {
                        painter.line_segment([a, b], Stroke::new(1.0_f32, PREVIEW));
                    }
                }
                [first, second] => {
                    let arc = arc_through_points(*first, *second, cursor);
                    let samples: Vec<Pos2> = match arc {
                        Some((centre, radius, start_angle, end_angle)) => {
                            let end = if end_angle < start_angle {
                                end_angle + std::f64::consts::TAU
                            } else {
                                end_angle
                            };
                            (0..=48)
                                .filter_map(|step| {
                                    let angle =
                                        start_angle + (end - start_angle) * step as f64 / 48.0;
                                    screen([
                                        centre[0] + radius * angle.cos(),
                                        centre[1] + radius * angle.sin(),
                                    ])
                                })
                                .collect()
                        }
                        None => Vec::new(),
                    };
                    if samples.len() >= 2 {
                        painter.add(Shape::line(samples, Stroke::new(1.4_f32, PREVIEW)));
                    } else if let (Some(a), Some(b)) = (screen(*first), screen(*second)) {
                        painter.line_segment([a, b], Stroke::new(1.0_f32, PREVIEW));
                    }
                }
                _ => {}
            },
            SketchTool::Spline => {
                let mut points = state.draft.pending_points().to_vec();
                points.push(cursor);
                let samples: Vec<Pos2> = points.iter().filter_map(|point| screen(*point)).collect();
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
    selected: bool,
) {
    let Some(screen) = dimension_screen_geometry(rect, camera, plane, dimension) else {
        return;
    };
    let (a, b) = (screen.a, screen.b);
    let (line_a, line_b) = (screen.line_a, screen.line_b);
    let direction = b - a;
    // The geometry helper only returns a dimension with a real length.
    let along = direction / direction.length();
    let normal = Vec2::new(-along.y, along.x);

    let colour = if selected {
        SELECTED
    } else if !dimension.driving {
        DRIVEN
    } else if dimension.weak {
        WEAK
    } else {
        DIMENSION
    };
    let thin = Stroke::new(1.0_f32, colour);
    let body = Stroke::new(if selected { 2.2_f32 } else { 1.4_f32 }, colour);

    // The extension lines from the geometry to the dimension line.
    painter.line_segment([a, line_a], thin);
    painter.line_segment([b, line_b], thin);
    // The dimension line, with an arrowhead at each end.
    painter.line_segment([line_a, line_b], body);
    for (tip, sign) in [(line_a, 1.0), (line_b, -1.0)] {
        let back = tip + along * sign * 9.0;
        painter.line_segment([tip, back + normal * 3.0], body);
        painter.line_segment([tip, back - normal * 3.0], body);
    }

    let text = if dimension.driving {
        dimension.text.clone()
    } else {
        format!("({})", dimension.text)
    };
    painter.text(
        screen.text,
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        colour,
    );
    if selected {
        painter.circle_stroke(screen.text, 9.0, Stroke::new(1.0_f32, SELECTED));
    }
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

    #[test]
    fn three_points_define_the_arc_through_them() {
        // The top half of the circle of radius 10.
        let (centre, radius, start, end) =
            arc_through_points([-10.0, 0.0], [10.0, 0.0], [0.0, 10.0]).expect("an arc");
        assert!(
            (centre[0]).abs() < 1e-6 && (centre[1]).abs() < 1e-6,
            "{centre:?}"
        );
        assert!((radius - 10.0).abs() < 1e-6);
        let middle = (start + end) * 0.5;
        assert!(
            (10.0 * middle.cos()).abs() < 1e-6 && (10.0 * middle.sin() - 10.0).abs() < 1e-6,
            "the arc should pass through the top: {start}..{end}"
        );

        // Three points on a line define no circle.
        assert_eq!(
            arc_through_points([0.0, 0.0], [10.0, 0.0], [20.0, 0.0]),
            None
        );
    }

    #[test]
    fn the_arc_tool_takes_three_clicks() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Arc3Point);

        assert_eq!(draft.click(&mut sketch, [-10.0, 0.0]), SketchEdit::Pending);
        assert_eq!(draft.click(&mut sketch, [10.0, 0.0]), SketchEdit::Pending);
        assert!(matches!(
            draft.click(&mut sketch, [0.0, 10.0]),
            SketchEdit::Arc(_)
        ));
        assert_eq!(sketch.arcs.len(), 1);
        let centre = sketch.point(sketch.arcs[0].center).unwrap();
        assert!((centre.x).abs() < 1e-6 && (centre.y).abs() < 1e-6);
        assert_eq!(draft.pending_points().len(), 0);
    }

    #[test]
    fn a_collinear_third_click_is_refused_and_keeps_the_first_two() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Arc3Point);
        draft.click(&mut sketch, [0.0, 0.0]);
        draft.click(&mut sketch, [10.0, 0.0]);

        assert!(matches!(
            draft.click(&mut sketch, [20.0, 0.0]),
            SketchEdit::Refused(_)
        ));
        assert!(sketch.arcs.is_empty());
        assert_eq!(draft.pending_points().len(), 2);

        // A point off the line finishes it.
        assert!(matches!(
            draft.click(&mut sketch, [5.0, 5.0]),
            SketchEdit::Arc(_)
        ));
    }

    #[test]
    fn the_spline_tool_collects_control_points_until_enter() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Spline);
        for point in [[0.0, 0.0], [10.0, 5.0], [20.0, 0.0]] {
            assert_eq!(draft.click(&mut sketch, point), SketchEdit::Pending);
        }
        assert!(sketch.splines.is_empty());

        let edit = draft.finish(&mut sketch).expect("the spline is finished");
        assert!(matches!(edit, SketchEdit::Spline(_)));
        assert_eq!(sketch.splines.len(), 1);
        assert_eq!(sketch.splines[0].degree, 2, "min(3, points - 1)");
        assert_eq!(sketch.points.len(), 3);
        assert!(draft.finish(&mut sketch).is_none());
    }

    #[test]
    fn the_trim_tool_shortens_the_clicked_end_to_the_crossing() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(20.0, 0.0);
        let line = sketch.add_line(a, b);
        let c = sketch.add_point(10.0, -10.0);
        let d = sketch.add_point(10.0, 10.0);
        sketch.add_line(c, d);

        let hit = SketchHit {
            id: line,
            kind: SketchEntityKind::Line,
            distance: 0.0,
        };
        let mut picks = Vec::new();
        let message = command_step(
            &mut sketch,
            SketchTool::Trim,
            &mut picks,
            &[],
            [20.0, 0.0],
            Some(&hit),
            &ToolParams::default(),
        )
        .expect("trim");
        assert!(message.contains("trimmed"), "{message}");
        let b = sketch.point(b).unwrap();
        assert!((b.x - 10.0).abs() < 1e-6, "the end did not move: {b:?}");
    }

    #[test]
    fn the_extend_tool_takes_two_picks() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(5.0, 0.0);
        let line = sketch.add_line(a, b);
        let c = sketch.add_point(10.0, -5.0);
        let d = sketch.add_point(10.0, 5.0);
        let target = sketch.add_line(c, d);

        let mut picks = Vec::new();
        let first = SketchHit {
            id: line,
            kind: SketchEntityKind::Line,
            distance: 0.0,
        };
        let message = command_step(
            &mut sketch,
            SketchTool::Extend,
            &mut picks,
            &[],
            [0.0, 0.0],
            Some(&first),
            &ToolParams::default(),
        )
        .expect("the first pick is remembered");
        assert!(message.contains("Now click"), "{message}");
        assert_eq!(picks, vec![line]);

        let second = SketchHit {
            id: target,
            kind: SketchEntityKind::Line,
            distance: 0.0,
        };
        command_step(
            &mut sketch,
            SketchTool::Extend,
            &mut picks,
            &[],
            [10.0, 0.0],
            Some(&second),
            &ToolParams::default(),
        )
        .expect("extend");
        assert!(picks.is_empty());
        let b = sketch.point(b).unwrap();
        assert!((b.x - 10.0).abs() < 1e-6, "the line did not grow: {b:?}");
    }

    #[test]
    fn the_offset_tool_copies_a_line_at_the_panel_distance() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(a, b);
        let hit = SketchHit {
            id: line,
            kind: SketchEntityKind::Line,
            distance: 0.0,
        };
        command_step(
            &mut sketch,
            SketchTool::Offset,
            &mut Vec::new(),
            &[],
            [5.0, 0.0],
            Some(&hit),
            &ToolParams {
                offset_distance: 5.0,
                ..Default::default()
            },
        )
        .expect("offset");

        assert_eq!(sketch.lines.len(), 2);
        let copy = &sketch.lines[1];
        let (c1, c2) = (sketch.point(copy.a).unwrap(), sketch.point(copy.b).unwrap());
        assert!((c1.y - 5.0).abs() < 1e-6 && (c2.y - 5.0).abs() < 1e-6);
    }

    #[test]
    fn the_mirror_tool_uses_the_selection_and_the_clicked_axis() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(a, b);
        let c = sketch.add_point(0.0, -10.0);
        let d = sketch.add_point(0.0, 10.0);
        let axis = sketch.add_line(c, d);

        let hit = SketchHit {
            id: axis,
            kind: SketchEntityKind::Line,
            distance: 0.0,
        };
        let mut picks = Vec::new();
        command_step(
            &mut sketch,
            SketchTool::Mirror,
            &mut picks,
            &[line],
            [0.0, 0.0],
            Some(&hit),
            &ToolParams::default(),
        )
        .expect("mirror");
        assert_eq!(sketch.lines.len(), 3, "the original, the axis and the copy");
    }

    #[test]
    fn the_pattern_tool_copies_the_selection_along_the_clicked_line() {
        let mut sketch = Sketch::new("test");
        let circle = sketch.add_circle(0.0, 0.0, 2.0);
        let a = sketch.add_point(0.0, -10.0);
        let b = sketch.add_point(10.0, -10.0);
        let direction = sketch.add_line(a, b);

        let hit = SketchHit {
            id: direction,
            kind: SketchEntityKind::Line,
            distance: 0.0,
        };
        let mut picks = Vec::new();
        command_step(
            &mut sketch,
            SketchTool::Pattern,
            &mut picks,
            &[circle],
            [5.0, -10.0],
            Some(&hit),
            &ToolParams {
                pattern_count: 3,
                pattern_spacing: 10.0,
                ..Default::default()
            },
        )
        .expect("pattern");
        assert_eq!(sketch.circles.len(), 3, "the original and two copies");
    }

    #[test]
    fn a_dimension_is_found_where_it_is_drawn() {
        let camera = top_camera();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let plane = xy_plane();
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(-20.0, 0.0);
        let b = sketch.add_point(20.0, 0.0);
        let index = sketch.constraints.len();
        sketch.add_dimension(a, b, 40.0, true, true);

        let annotation = dimensions(&sketch).into_iter().next().expect("a dimension");
        let screen =
            dimension_screen_geometry(rect, &camera, plane, &annotation).expect("on screen");
        assert_eq!(annotation.constraint_index, index);
        assert_eq!(
            dimension_hit_test(&sketch, rect, &camera, plane, screen.text, HIT_RADIUS),
            Some(index)
        );
        let away = screen.text + Vec2::new(0.0, 200.0);
        assert!(dimension_hit_test(&sketch, rect, &camera, plane, away, HIT_RADIUS).is_none());
    }

    #[test]
    fn a_point_can_be_moved() {
        let mut sketch = Sketch::new("test");
        let id = sketch.add_point(0.0, 0.0);
        assert!(move_point(&mut sketch, id, [5.0, 7.0]));
        let point = sketch.point(id).unwrap();
        assert_eq!([point.x, point.y], [5.0, 7.0]);
        assert!(!move_point(&mut sketch, Uuid::new_v4(), [0.0, 0.0]));
    }

    #[test]
    fn escape_also_drops_the_entity_picks_of_a_command_tool() {
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Extend);
        let id = Uuid::new_v4();
        draft.set_picks(vec![id]);
        assert_eq!(draft.pending_entity(), Some(id));

        assert!(draft.cancel());
        assert_eq!(draft.pending_entity(), None);
        assert_eq!(draft.tool(), SketchTool::Extend, "the tool stays");
    }
}
