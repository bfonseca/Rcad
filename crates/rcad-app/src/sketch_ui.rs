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
use rcad_sketch::{Constraint, DimensionOrientation, Sketch, SketchReferenceRole};
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

/// How close, in pixels, a polyline click must be to one of its own points to
/// snap onto it (which is how the user closes the loop).
pub const CHAIN_SNAP_RADIUS: f32 = 11.0;

/// How far a dimension line is drawn from the geometry it measures.
const DIMENSION_OFFSET: f32 = 16.0;

/// Radius, in points, of the target circle on the polyline's first point.
const CLOSE_MARKER: f32 = 8.0;

const GEOMETRY: Color32 = Color32::from_rgb(225, 228, 235);
const SELECTED: Color32 = Color32::from_rgb(255, 170, 60);
const HOVERED: Color32 = Color32::from_rgb(255, 220, 140);
const CONSTRAINT: Color32 = Color32::from_rgb(120, 205, 255);
const DIMENSION: Color32 = Color32::from_rgb(130, 230, 150);
const WEAK: Color32 = Color32::from_rgb(160, 160, 160);
const DRIVEN: Color32 = Color32::from_rgb(120, 190, 210);
const FREE_POINT: Color32 = Color32::from_rgb(255, 130, 130);
/// An entity that still has a degree of freedom, when the highlight is on.
const FREE_ENTITY: Color32 = Color32::from_rgb(255, 190, 100);
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
    /// Connected lines: each click adds a segment; closing on the first point
    /// or a right-click ends the chain.
    LineChain,
    /// Two opposite corners of an axis-aligned rectangle.
    Rectangle,
    /// The centre and one corner of an axis-aligned rectangle.
    RectangleCenter,
    /// Two adjacent corners and then a point that sets the width: a rotated
    /// rectangle.
    RectangleSkewed,
    /// The centre and a point on the circle.
    Circle,
    /// Two points at opposite ends of a diameter.
    CircleDiameter,
    /// Two lines; the radius comes from the strip and the side from the click.
    CircleTangent,
    /// A regular polygon: the centre, then a vertex. The number of sides is
    /// part of the tool.
    Polygon(u8),
    /// Three clicks: the start, the end, and a point the arc passes through.
    Arc3Point,
    /// Click a line near the end the arc should start on, then the arc's end.
    /// The arc leaves that end along the line, so it is tangent by construction.
    ArcTangent,
    /// Three clicks (start, end, shoulder); the rho comes from the strip.
    Conic,
    /// Click where the text goes and type it.
    Text,
    /// Click the control points, then Enter to finish the spline.
    Spline,
    /// The dynamic dimension tool: pick one or two things, then place the text.
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
    /// The rectangle tools, for the floating bar's dropdown.
    pub const RECTANGLES: [SketchTool; 3] = [
        SketchTool::Rectangle,
        SketchTool::RectangleCenter,
        SketchTool::RectangleSkewed,
    ];
    /// The circle tools, for the floating bar's dropdown.
    pub const CIRCLES: [SketchTool; 3] = [
        SketchTool::Circle,
        SketchTool::CircleDiameter,
        SketchTool::CircleTangent,
    ];
    /// The polygon tools, for the floating bar's dropdown.
    pub const POLYGONS: [SketchTool; 3] = [
        SketchTool::Polygon(3),
        SketchTool::Polygon(5),
        SketchTool::Polygon(6),
    ];

    /// The name the sketch panel and the status bar show.
    pub fn label(self) -> &'static str {
        match self {
            SketchTool::Select => "Select",
            SketchTool::Point => "Point",
            SketchTool::Line => "Line",
            SketchTool::LineChain => "Polyline",
            SketchTool::Rectangle => "Rectangle",
            SketchTool::RectangleCenter => "Rectangle (centre)",
            SketchTool::RectangleSkewed => "Skewed rectangle",
            SketchTool::Circle => "Circle",
            SketchTool::CircleDiameter => "Circle (diameter)",
            SketchTool::CircleTangent => "Circle (tangent)",
            SketchTool::Polygon(3) => "Triangle",
            SketchTool::Polygon(5) => "Pentagon",
            SketchTool::Polygon(6) => "Hexagon",
            SketchTool::Polygon(_) => "Polygon",
            SketchTool::Arc3Point => "3-Point Arc",
            SketchTool::ArcTangent => "Tangent Arc",
            SketchTool::Conic => "Conic",
            SketchTool::Text => "Text",
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
                | SketchTool::LineChain
                | SketchTool::Rectangle
                | SketchTool::RectangleCenter
                | SketchTool::RectangleSkewed
                | SketchTool::Circle
                | SketchTool::CircleDiameter
                | SketchTool::CircleTangent
                | SketchTool::Polygon(_)
                | SketchTool::Arc3Point
                | SketchTool::ArcTangent
                | SketchTool::Conic
                | SketchTool::Text
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
                | SketchTool::CircleTangent
        )
    }

    /// The next step, in the words a Creo user expects.
    pub fn hint(self) -> &'static str {
        match self {
            SketchTool::Select => "Click a sketch entity to select it; Ctrl adds.",
            SketchTool::Point => "Click to place a point.",
            SketchTool::Line => "Click the two ends of the line.",
            SketchTool::LineChain => {
                "Click to add segments; close on the first point or right-click to stop."
            }
            SketchTool::Rectangle => "Click two opposite corners of the rectangle.",
            SketchTool::RectangleCenter => "Click the centre, then a corner.",
            SketchTool::RectangleSkewed => {
                "Click two adjacent corners, then a point for the width."
            }
            SketchTool::Circle => "Click the centre, then a point on the circle.",
            SketchTool::CircleDiameter => "Click two points at opposite ends of the diameter.",
            SketchTool::CircleTangent => "Click two lines; the radius comes from the strip.",
            SketchTool::Polygon(_) => "Click the centre, then a vertex.",
            SketchTool::Arc3Point => "Click the start, the end, then a point on the arc.",
            SketchTool::ArcTangent => {
                "Click a line near the end the arc starts on, then the arc's end."
            }
            SketchTool::Conic => "Click the start, the end, then a point the curve passes through.",
            SketchTool::Text => "Click where the text goes, then type it.",
            SketchTool::Spline => "Click the control points, then Enter to finish.",
            SketchTool::Dimension => {
                "Click a line or point, then the second one or the empty space for the text."
            }
            SketchTool::Trim => "Click the end of a line to trim it to its nearest crossing.",
            SketchTool::Extend => "Click the line to grow, then the entity it should reach.",
            SketchTool::Offset => "Click a line to offset it by the strip's distance.",
            SketchTool::Mirror => "Select the geometry, then click the mirror axis line.",
            SketchTool::Pattern => "Select the geometry, then click a line for the direction.",
        }
    }
}

/// What one click did, so the caller can report it and rebuild afterwards.
#[derive(Debug, Clone, PartialEq)]
pub enum SketchEdit {
    /// The first click of a two-click tool: the geometry is not complete yet.
    Pending,
    Point(Uuid),
    Line(Uuid),
    /// A segment of the polyline: the chain continues.
    LineChain(Uuid),
    /// The polyline closed on its first point; the tool is done.
    Closed(Uuid),
    Polygon(Vec<Uuid>),
    Rectangle([Uuid; 4]),
    Circle(Uuid),
    Arc(Uuid),
    Conic(Uuid),
    Spline(Uuid),
    /// Text was added to the sketch at that place.
    Text(Uuid),
    /// The text tool wants a string at this point: the application opens its
    /// in-place editor and then calls add_text.
    TextAt([f64; 2]),
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
                | SketchEdit::LineChain(_)
                | SketchEdit::Closed(_)
                | SketchEdit::Polygon(_)
                | SketchEdit::Rectangle(_)
                | SketchEdit::Circle(_)
                | SketchEdit::Arc(_)
                | SketchEdit::Conic(_)
                | SketchEdit::Spline(_)
                | SketchEdit::Text(_)
        )
    }
}

/// The active sketch tool, the clicks a creation tool has collected, and the
/// entity picks a command tool has collected.
///
/// This is the whole mouse state. Keeping it out of `RcadApp` means the
/// interaction rules can be tested directly.
#[derive(Debug, Clone)]
pub struct SketchDraft {
    tool: SketchTool,
    points: Vec<[f64; 2]>,
    /// The points the polyline has already created in the sketch.
    chain: Vec<Uuid>,
    entities: Vec<Uuid>,
    /// The line and tangency point the tangent-arc tool is building from.
    tangent: Option<TangentArcSpan>,
    /// The rho the conic tool gives the next curve.
    conic_rho: f64,
}

/// Where the tangent-arc tool starts: a line, the end of it, and the direction
/// the arc leaves along.
#[derive(Debug, Clone)]
pub struct TangentArcSpan {
    pub line: Uuid,
    pub point: Uuid,
    pub start: [f64; 2],
    pub direction: [f64; 2],
}

impl Default for SketchDraft {
    fn default() -> Self {
        Self {
            tool: SketchTool::Select,
            points: Vec::new(),
            chain: Vec::new(),
            entities: Vec::new(),
            tangent: None,
            conic_rho: 0.5,
        }
    }
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
        self.chain.clear();
        self.entities.clear();
        self.tangent = None;
    }

    /// The rho the conic tool gives its next curve.
    pub fn conic_rho(&self) -> f64 {
        self.conic_rho
    }

    /// Set the rho the conic tool gives its next curve.
    pub fn set_conic_rho(&mut self, rho: f64) {
        self.conic_rho = rho.clamp(0.05, 0.95);
    }

    /// Start the tangent arc on a line end: the arc leaves along the line.
    pub fn begin_arc_tangent(
        &mut self,
        line: Uuid,
        point: Uuid,
        start: [f64; 2],
        direction: [f64; 2],
    ) {
        self.tool = SketchTool::ArcTangent;
        self.points.clear();
        self.chain.clear();
        self.tangent = Some(TangentArcSpan {
            line,
            point,
            start,
            direction,
        });
        self.points.push(start);
        self.entities = vec![line];
    }

    /// The line end the tangent arc starts from.
    pub fn tangent_arc(&self) -> Option<&TangentArcSpan> {
        self.tangent.as_ref()
    }

    /// Escape or a right-click: drop the collected clicks or picks, then leave
    /// the tool. The geometry already created stays.
    ///
    /// Returns true when something was actually cancelled, so the caller can
    /// fall back to clearing the selection only when the sketch had nothing to
    /// cancel.
    pub fn cancel(&mut self) -> bool {
        if !self.points.is_empty() || !self.chain.is_empty() || !self.entities.is_empty() {
            self.points.clear();
            self.chain.clear();
            self.entities.clear();
            self.tangent = None;
            return true;
        }
        self.tangent = None;
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
                let (first, second) = (self.points[0], self.points[1]);
                let (dx, dy) = (second[0] - first[0], second[1] - first[1]);
                let edit = match self.tool {
                    SketchTool::Line => {
                        if dx.hypot(dy) < 1e-9 {
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
            SketchTool::RectangleCenter => {
                self.points.push(point);
                if self.points.len() < 2 {
                    return SketchEdit::Pending;
                }
                let centre = self.points[0];
                let corner = self.points[1];
                let (dx, dy) = ((corner[0] - centre[0]).abs(), (corner[1] - centre[1]).abs());
                if dx < 1e-9 || dy < 1e-9 {
                    self.points.truncate(1);
                    return SketchEdit::TooSmall;
                }
                self.points.clear();
                SketchEdit::Rectangle(sketch.add_rectangle(
                    centre[0] - dx,
                    centre[1] - dy,
                    centre[0] + dx,
                    centre[1] + dy,
                ))
            }
            SketchTool::RectangleSkewed => {
                self.points.push(point);
                if self.points.len() < 3 {
                    return SketchEdit::Pending;
                }
                let corners =
                    skewed_rectangle_corners(self.points[0], self.points[1], self.points[2]);
                match corners {
                    Some(corners) => {
                        self.points.clear();
                        SketchEdit::Polygon(add_quad(sketch, corners))
                    }
                    None => {
                        self.points.pop();
                        SketchEdit::Refused("A skewed rectangle needs two distinct corners.")
                    }
                }
            }
            SketchTool::Polygon(sides) => {
                self.points.push(point);
                if self.points.len() < 2 {
                    return SketchEdit::Pending;
                }
                let (centre, vertex) = (self.points[0], self.points[1]);
                if (vertex[0] - centre[0]).hypot(vertex[1] - centre[1]) < 1e-9 {
                    self.points.pop();
                    return SketchEdit::TooSmall;
                }
                let corners = regular_polygon(centre, vertex, sides.max(3) as usize);
                self.points.clear();
                SketchEdit::Polygon(add_closed_loop(sketch, &corners))
            }
            SketchTool::CircleDiameter => {
                self.points.push(point);
                if self.points.len() < 2 {
                    return SketchEdit::Pending;
                }
                let (a, b) = (self.points[0], self.points[1]);
                if (b[0] - a[0]).hypot(b[1] - a[1]) < 1e-9 {
                    self.points.pop();
                    return SketchEdit::TooSmall;
                }
                let (centre, radius) = circle_from_diameter(a, b);
                self.points.clear();
                SketchEdit::Circle(sketch.add_circle(centre[0], centre[1], radius))
            }
            SketchTool::LineChain => {
                if self.chain.is_empty() {
                    let id = sketch.add_point(point[0], point[1]);
                    self.chain.push(id);
                    self.points.push(point);
                    return SketchEdit::Pending;
                }
                let first = self.points[0];
                let last = *self.chain.last().expect("a chain has a point");
                let closes = (point[0] - first[0]).hypot(point[1] - first[1]) < 1e-6;
                let next = sketch.add_point(point[0], point[1]);
                let line = sketch.add_line(last, next);
                if closes {
                    self.points.clear();
                    self.chain.clear();
                    SketchEdit::Closed(line)
                } else {
                    self.chain.push(next);
                    self.points.push(point);
                    SketchEdit::LineChain(line)
                }
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
                        self.points.pop();
                        SketchEdit::Refused(
                            "The three points are on a line: no arc passes through them.",
                        )
                    }
                }
            }
            SketchTool::Conic => {
                self.points.push(point);
                if self.points.len() < 3 {
                    return SketchEdit::Pending;
                }
                let (start, end, shoulder) = (self.points[0], self.points[1], self.points[2]);
                if (end[0] - start[0]).hypot(end[1] - start[1]) < 1e-9 {
                    self.points.pop();
                    return SketchEdit::TooSmall;
                }
                // The weight that makes the curve pass through the shoulder:
                // reversing the rational quadratic at t = 1/2.
                let rho = self.conic_rho.clamp(0.05, 0.95);
                let control = [
                    (2.0 * shoulder[0] * (1.0 + rho) - (start[0] + end[0])) / (2.0 * rho),
                    (2.0 * shoulder[1] * (1.0 + rho) - (start[1] + end[1])) / (2.0 * rho),
                ];
                let a = sketch.add_point(start[0], start[1]);
                let c = sketch.add_point(control[0], control[1]);
                let b = sketch.add_point(end[0], end[1]);
                let conic = sketch.add_conic(a, c, b, rho);
                self.points.clear();
                SketchEdit::Conic(conic)
            }
            SketchTool::Text => {
                // The application opens its editor; the sketch gets the text
                // once the user has typed it.
                self.points.clear();
                SketchEdit::TextAt(point)
            }
            SketchTool::Spline => {
                self.points.push(point);
                SketchEdit::Pending
            }
            _ => SketchEdit::NotDrawing,
        }
    }

    /// Finish the text tool: add the text where the click put it.
    pub fn finish_text(
        &mut self,
        sketch: &mut Sketch,
        position: [f64; 2],
        content: &str,
        height: f64,
    ) -> SketchEdit {
        self.points.clear();
        SketchEdit::Text(sketch.add_text(position, height, content))
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

/// The circle that has two points at opposite ends of a diameter.
pub fn circle_from_diameter(a: [f64; 2], b: [f64; 2]) -> ([f64; 2], f64) {
    let centre = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    (centre, (b[0] - a[0]).hypot(b[1] - a[1]) * 0.5)
}

/// Where two infinite lines cross, or None when they are parallel.
pub fn line_intersection(
    a1: [f64; 2],
    a2: [f64; 2],
    b1: [f64; 2],
    b2: [f64; 2],
) -> Option<[f64; 2]> {
    let d1 = [a2[0] - a1[0], a2[1] - a1[1]];
    let d2 = [b2[0] - b1[0], b2[1] - b1[1]];
    let denom = d1[0] * d2[1] - d1[1] * d2[0];
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = ((b1[0] - a1[0]) * d2[1] - (b1[1] - a1[1]) * d2[0]) / denom;
    Some([a1[0] + d1[0] * t, a1[1] + d1[1] * t])
}

/// The corners of a rectangle whose first edge runs from ~a~ to ~b~ and whose
/// width is the perpendicular distance from ~point~ to that edge.
///
/// None when the edge has no length or the point sits on it: there would be no
/// rectangle to draw.
pub fn skewed_rectangle_corners(
    a: [f64; 2],
    b: [f64; 2],
    point: [f64; 2],
) -> Option<[[f64; 2]; 4]> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy);
    if length < 1e-9 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let (nx, ny) = (-uy, ux);
    let width = (point[0] - a[0]) * nx + (point[1] - a[1]) * ny;
    if width.abs() < 1e-9 {
        return None;
    }
    Some([
        a,
        b,
        [b[0] + nx * width, b[1] + ny * width],
        [a[0] + nx * width, a[1] + ny * width],
    ])
}

/// The corners of a regular polygon: the first vertex is ~vertex~, the rest go
/// round the circumcircle around ~centre~.
pub fn regular_polygon(centre: [f64; 2], vertex: [f64; 2], sides: usize) -> Vec<[f64; 2]> {
    let sides = sides.max(3);
    let dx = vertex[0] - centre[0];
    let dy = vertex[1] - centre[1];
    let radius = dx.hypot(dy);
    let start = dy.atan2(dx);
    let step = std::f64::consts::TAU / sides as f64;
    (0..sides)
        .map(|index| {
            let angle = start + step * index as f64;
            [
                centre[0] + radius * angle.cos(),
                centre[1] + radius * angle.sin(),
            ]
        })
        .collect()
}

/// A circle of ~radius~ tangent to both lines, on the side nearest ~near~.
///
/// Each line is offset by the radius on both sides; the four crossings of the
/// offset lines are the possible centres, and the one closest to the click wins.
pub fn circle_tangent_to_two_lines(
    first: ([f64; 2], [f64; 2]),
    second: ([f64; 2], [f64; 2]),
    radius: f64,
    near: [f64; 2],
) -> Option<([f64; 2], f64)> {
    if radius <= 0.0 {
        return None;
    }
    let normal = |line: ([f64; 2], [f64; 2])| -> Option<[f64; 2]> {
        let dx = line.1[0] - line.0[0];
        let dy = line.1[1] - line.0[1];
        let length = dx.hypot(dy);
        (length > 1e-12).then(|| [-dy / length, dx / length])
    };
    let na = normal(first)?;
    let nb = normal(second)?;
    let mut best: Option<([f64; 2], f64)> = None;
    for sign_a in [-1.0, 1.0] {
        for sign_b in [-1.0, 1.0] {
            let a1 = [
                first.0[0] + na[0] * radius * sign_a,
                first.0[1] + na[1] * radius * sign_a,
            ];
            let a2 = [
                first.1[0] + na[0] * radius * sign_a,
                first.1[1] + na[1] * radius * sign_a,
            ];
            let b1 = [
                second.0[0] + nb[0] * radius * sign_b,
                second.0[1] + nb[1] * radius * sign_b,
            ];
            let b2 = [
                second.1[0] + nb[0] * radius * sign_b,
                second.1[1] + nb[1] * radius * sign_b,
            ];
            if let Some(centre) = line_intersection(a1, a2, b1, b2) {
                let distance = (centre[0] - near[0]).hypot(centre[1] - near[1]);
                if best.map_or(true, |(_, best)| distance < best) {
                    best = Some((centre, distance));
                }
            }
        }
    }
    best.map(|(centre, _)| (centre, radius))
}

/// Add the four corners of a rectangle as a closed loop, keeping it a rectangle
/// with two parallels and one perpendicular.
fn add_quad(sketch: &mut Sketch, corners: [[f64; 2]; 4]) -> Vec<Uuid> {
    let lines = add_closed_loop(sketch, &corners);
    if lines.len() == 4 {
        sketch.constraints.push(Constraint::Parallel {
            a: lines[0],
            b: lines[2],
        });
        sketch.constraints.push(Constraint::Parallel {
            a: lines[1],
            b: lines[3],
        });
        sketch.constraints.push(Constraint::Perpendicular {
            a: lines[0],
            b: lines[1],
        });
    }
    lines
}

/// Add points round a loop, with equal-length constraints between the sides, so
/// a regular polygon stays regular.
fn add_closed_loop(sketch: &mut Sketch, corners: &[[f64; 2]]) -> Vec<Uuid> {
    let points: Vec<Uuid> = corners
        .iter()
        .map(|corner| sketch.add_point(corner[0], corner[1]))
        .collect();
    let lines: Vec<Uuid> = (0..points.len())
        .map(|index| sketch.add_line(points[index], points[(index + 1) % points.len()]))
        .collect();
    for line in lines.iter().skip(1) {
        sketch.constraints.push(Constraint::EqualLength {
            a: lines[0],
            b: *line,
        });
    }
    lines
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

/// The arc that leaves \`start\` along \`direction\` and ends at \`end\`.
///
/// The centre sits on the normal to the tangent through the start, so the arc is
/// tangent to whatever gave \`direction\` (the line the user picked). None when
/// the two points coincide or the end lies on the tangent itself.
pub fn tangent_arc(
    start: [f64; 2],
    direction: [f64; 2],
    end: [f64; 2],
) -> Option<([f64; 2], f64, f64, f64)> {
    let length = direction[0].hypot(direction[1]);
    if length < 1e-9 {
        return None;
    }
    let (dx, dy) = (direction[0] / length, direction[1] / length);
    let (vx, vy) = (end[0] - start[0], end[1] - start[1]);
    if vx.hypot(vy) < 1e-9 {
        return None;
    }
    let mut normal = [-dy, dx];
    let mut radius = (vx * vx + vy * vy) / (2.0 * (vx * normal[0] + vy * normal[1]));
    if !radius.is_finite() || radius.abs() < 1e-9 {
        return None;
    }
    if radius < 0.0 {
        radius = -radius;
        normal = [-normal[0], -normal[1]];
    }
    let centre = [start[0] + normal[0] * radius, start[1] + normal[1] * radius];
    let start_angle = (start[1] - centre[1]).atan2(start[0] - centre[0]);
    let end_angle = (end[1] - centre[1]).atan2(end[0] - centre[0]);
    let mut sweep = end_angle - start_angle;
    while sweep > std::f64::consts::PI {
        sweep -= std::f64::consts::TAU;
    }
    while sweep < -std::f64::consts::PI {
        sweep += std::f64::consts::TAU;
    }
    // The tangent of a counter-clockwise arc at its start is the radial turned
    // left; go the other way round when that does not point along \`direction\`.
    let radial = [start[0] - centre[0], start[1] - centre[1]];
    let counter_clockwise = [-radial[1], radial[0]];
    let tangent = if sweep >= 0.0 {
        counter_clockwise
    } else {
        [-counter_clockwise[0], -counter_clockwise[1]]
    };
    if tangent[0] * dx + tangent[1] * dy < 0.0 {
        sweep = if sweep >= 0.0 {
            sweep - std::f64::consts::TAU
        } else {
            sweep + std::f64::consts::TAU
        };
    }
    Some((centre, radius, start_angle, start_angle + sweep))
}

/// Where \`point\` lands when dropped perpendicular on the line through \`a\` and \`b\`.
pub fn foot_on_line(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> Option<[f64; 2]> {
    let direction = [b[0] - a[0], b[1] - a[1]];
    let length_squared = direction[0] * direction[0] + direction[1] * direction[1];
    if length_squared < 1e-18 {
        return None;
    }
    let t = ((point[0] - a[0]) * direction[0] + (point[1] - a[1]) * direction[1]) / length_squared;
    Some([a[0] + direction[0] * t, a[1] + direction[1] * t])
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
            // Dimensions are drawn as dimensions, not as glyphs.
            Constraint::Distance { .. }
            | Constraint::Angle { .. }
            | Constraint::LineDistance { .. }
            | Constraint::HorizontalDistance { .. }
            | Constraint::VerticalDistance { .. }
            | Constraint::Radius { .. }
            | Constraint::Diameter { .. }
            | Constraint::PointLineDistance { .. }
            | Constraint::LineCurveDistance { .. } => {}
        }
    }
    glyphs
}

/// Whether a dimension on a circle or an arc reports its radius or its
/// diameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DimensionSize {
    #[default]
    Radius,
    Diameter,
}

/// How a dimension is drawn and what it measures.
#[derive(Debug, Clone, PartialEq)]
pub enum DimensionKind {
    /// A length between two points, measured along the line or projected on an
    /// axis.
    Linear {
        a: [f64; 2],
        b: [f64; 2],
        orientation: DimensionOrientation,
    },
    /// The angle between two lines.
    Angular {
        first: ([f64; 2], [f64; 2]),
        second: ([f64; 2], [f64; 2]),
    },
    /// The perpendicular distance between two parallel lines.
    LineDistance {
        first: ([f64; 2], [f64; 2]),
        second: ([f64; 2], [f64; 2]),
    },
    /// The radius or the diameter of a circle or an arc.
    Radial {
        centre: [f64; 2],
        rim: [f64; 2],
        diameter: bool,
    },
    /// The perpendicular distance from a point to a line.
    PointDistance {
        point: [f64; 2],
        foot: [f64; 2],
        line: ([f64; 2], [f64; 2]),
    },
    /// The gap between a line and the tangent of a circle or an arc.
    TangentDistance {
        line: ([f64; 2], [f64; 2]),
        centre: [f64; 2],
        radius: f64,
        touch: [f64; 2],
        foot: [f64; 2],
    },
}

/// A dimension the viewport draws.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionAnnotation {
    pub kind: DimensionKind,
    /// The value the constraint holds, or the measurement a driven one reports.
    pub value: f64,
    /// What the geometry measures right now; the two differ while unsolved.
    pub measured: f64,
    pub text: String,
    /// False for a driven (reference) dimension.
    pub driving: bool,
    /// Weak dimensions come from the Intent Manager, not from the user.
    pub weak: bool,
    /// Where the dimension's constraint sits in `Sketch::constraints`, so an
    /// edit can reach it.
    pub constraint_index: usize,
    /// Where the user placed the text, in sketch coordinates.
    pub text_position: Option<[f64; 2]>,
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

/// The two ends of a line, when the line exists.
fn line_points(sketch: &Sketch, id: Uuid) -> Option<([f64; 2], [f64; 2])> {
    let line = sketch.lines.iter().find(|line| line.id == id)?;
    Some((point_of(sketch, line.a)?, point_of(sketch, line.b)?))
}

/// The unsigned angle between two directions, in degrees.
pub fn angle_between(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] * b[1] - a[1] * b[0])
        .atan2(a[0] * b[0] + a[1] * b[1])
        .to_degrees()
        .abs()
}

/// The perpendicular distance between two lines (0 when they cross).
pub fn line_distance(first: ([f64; 2], [f64; 2]), second: ([f64; 2], [f64; 2])) -> f64 {
    let dx = first.1[0] - first.0[0];
    let dy = first.1[1] - first.0[1];
    let length = dx.hypot(dy);
    if length < 1e-12 {
        return 0.0;
    }
    ((second.0[0] - first.0[0]) * dy - (second.0[1] - first.0[1]) * dx).abs() / length
}

/// The centre and radius of a circle or an arc.
fn curve_geometry(sketch: &Sketch, id: Uuid) -> Option<([f64; 2], f64)> {
    if let Some(circle) = sketch.circles.iter().find(|circle| circle.id == id) {
        return Some((point_of(sketch, circle.center)?, circle.radius));
    }
    let arc = sketch.arcs.iter().find(|arc| arc.id == id)?;
    Some((point_of(sketch, arc.center)?, arc.radius))
}

/// The direction a radius dimension points in when the user did not place it:
/// the middle of an arc, or +X of a circle.
fn curve_radial_direction(sketch: &Sketch, id: Uuid) -> [f64; 2] {
    if let Some(arc) = sketch.arcs.iter().find(|arc| arc.id == id) {
        let mut end = arc.end_angle;
        while end < arc.start_angle {
            end += std::f64::consts::TAU;
        }
        let angle = (arc.start_angle + end) * 0.5;
        return [angle.cos(), angle.sin()];
    }
    [1.0, 0.0]
}

/// The direction from a centre towards the text, when the text was placed.
fn direction_to(centre: [f64; 2], target: Option<[f64; 2]>) -> Option<[f64; 2]> {
    let target = target?;
    let direction = [target[0] - centre[0], target[1] - centre[1]];
    let length = direction[0].hypot(direction[1]);
    (length > 1e-9).then(|| [direction[0] / length, direction[1] / length])
}

/// Every dimension the sketch holds, ready to draw.
///
/// The value, strength, driving flag and placement all come from the Intent
/// Manager record, so editing a dimension edits what is drawn.
pub fn dimensions(sketch: &Sketch) -> Vec<DimensionAnnotation> {
    let mut dimensions = Vec::new();
    for (constraint_index, constraint) in sketch.constraints.iter().enumerate() {
        let driving = sketch
            .dimension_is_driving(constraint_index)
            .unwrap_or(true);
        let weak = sketch.dimension_is_weak(constraint_index);
        let text_position = sketch.dimension_text_position(constraint_index);
        let orientation = sketch.dimension_orientation(constraint_index);
        let (kind, value, measured, text) = match constraint {
            Constraint::Distance { a, b, value } => {
                let (Some(pa), Some(pb)) = (point_of(sketch, *a), point_of(sketch, *b)) else {
                    continue;
                };
                let measured = match orientation {
                    DimensionOrientation::Aligned => (pb[0] - pa[0]).hypot(pb[1] - pa[1]),
                    DimensionOrientation::Horizontal => (pb[0] - pa[0]).abs(),
                    DimensionOrientation::Vertical => (pb[1] - pa[1]).abs(),
                };
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::Linear {
                        a: pa,
                        b: pb,
                        orientation,
                    },
                    *value,
                    measured,
                    format_length(shown),
                )
            }
            Constraint::HorizontalDistance { a, b, value } => {
                let (Some(pa), Some(pb)) = (point_of(sketch, *a), point_of(sketch, *b)) else {
                    continue;
                };
                let measured = (pb[0] - pa[0]).abs();
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::Linear {
                        a: pa,
                        b: pb,
                        orientation: DimensionOrientation::Horizontal,
                    },
                    *value,
                    measured,
                    format_length(shown),
                )
            }
            Constraint::VerticalDistance { a, b, value } => {
                let (Some(pa), Some(pb)) = (point_of(sketch, *a), point_of(sketch, *b)) else {
                    continue;
                };
                let measured = (pb[1] - pa[1]).abs();
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::Linear {
                        a: pa,
                        b: pb,
                        orientation: DimensionOrientation::Vertical,
                    },
                    *value,
                    measured,
                    format_length(shown),
                )
            }
            Constraint::Radius { curve, value } => {
                let Some((centre, radius)) = curve_geometry(sketch, *curve) else {
                    continue;
                };
                let direction = direction_to(centre, text_position)
                    .unwrap_or_else(|| curve_radial_direction(sketch, *curve));
                let rim = [
                    centre[0] + direction[0] * radius,
                    centre[1] + direction[1] * radius,
                ];
                let measured = radius;
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::Radial {
                        centre,
                        rim,
                        diameter: false,
                    },
                    *value,
                    measured,
                    format!("R{}", format_length(shown)),
                )
            }
            Constraint::Diameter { curve, value } => {
                let Some((centre, radius)) = curve_geometry(sketch, *curve) else {
                    continue;
                };
                let direction = direction_to(centre, text_position)
                    .unwrap_or_else(|| curve_radial_direction(sketch, *curve));
                let rim = [
                    centre[0] + direction[0] * radius,
                    centre[1] + direction[1] * radius,
                ];
                let measured = radius * 2.0;
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::Radial {
                        centre,
                        rim,
                        diameter: true,
                    },
                    *value,
                    measured,
                    format!("Ø{}", format_length(shown)),
                )
            }
            Constraint::PointLineDistance { point, line, value } => {
                let (Some(p), Some((a, b))) =
                    (point_of(sketch, *point), line_points(sketch, *line))
                else {
                    continue;
                };
                let Some(foot) = foot_on_line(p, a, b) else {
                    continue;
                };
                let measured = (p[0] - foot[0]).hypot(p[1] - foot[1]);
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::PointDistance {
                        point: p,
                        foot,
                        line: (a, b),
                    },
                    *value,
                    measured,
                    format_length(shown),
                )
            }
            Constraint::LineCurveDistance { line, curve, value } => {
                let (Some((a, b)), Some((centre, radius))) =
                    (line_points(sketch, *line), curve_geometry(sketch, *curve))
                else {
                    continue;
                };
                let Some(foot) = foot_on_line(centre, a, b) else {
                    continue;
                };
                let distance = (centre[0] - foot[0]).hypot(centre[1] - foot[1]);
                let measured = (distance - radius).max(0.0);
                let direction = if distance > 1e-9 {
                    [
                        (foot[0] - centre[0]) / distance,
                        (foot[1] - centre[1]) / distance,
                    ]
                } else {
                    [0.0, 1.0]
                };
                let touch = [
                    centre[0] + direction[0] * radius,
                    centre[1] + direction[1] * radius,
                ];
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::TangentDistance {
                        line: (a, b),
                        centre,
                        radius,
                        touch,
                        foot,
                    },
                    *value,
                    measured,
                    format_length(shown),
                )
            }
            Constraint::Angle { a, b, value } => {
                let (Some(first), Some(second)) =
                    (line_points(sketch, *a), line_points(sketch, *b))
                else {
                    continue;
                };
                let d1 = [first.1[0] - first.0[0], first.1[1] - first.0[1]];
                let d2 = [second.1[0] - second.0[0], second.1[1] - second.0[1]];
                let measured = angle_between(d1, d2);
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::Angular { first, second },
                    *value,
                    measured,
                    format!("{}°", format_length(shown)),
                )
            }
            Constraint::LineDistance { a, b, value } => {
                let (Some(first), Some(second)) =
                    (line_points(sketch, *a), line_points(sketch, *b))
                else {
                    continue;
                };
                let measured = line_distance(first, second);
                let shown = if driving { *value } else { measured };
                (
                    DimensionKind::LineDistance { first, second },
                    *value,
                    measured,
                    format_length(shown),
                )
            }
            _ => continue,
        };
        dimensions.push(DimensionAnnotation {
            kind,
            value,
            measured,
            text,
            driving,
            weak,
            constraint_index,
            text_position,
        });
    }
    dimensions
}

/// What a dimension looks like on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DimensionScreenKind {
    /// A measured line between two points.
    Linear {
        a: Pos2,
        b: Pos2,
        line_a: Pos2,
        line_b: Pos2,
    },
    /// An arc of `radius` around a vertex, starting at `start_angle`.
    Angular {
        centre: Pos2,
        radius: f32,
        start_angle: f32,
        sweep: f32,
    },
    /// The perpendicular between two lines.
    LineDistance { from: Pos2, to: Pos2 },
    /// A radius or diameter: from the centre (or the far rim) to the rim.
    Radial { from: Pos2, to: Pos2 },
    /// The perpendicular from a point to a line.
    PointDistance { point: Pos2, foot: Pos2 },
    /// The gap between a line and a circle's tangent.
    TangentDistance { from: Pos2, to: Pos2 },
}

/// The screen geometry of a dimension: what to draw and where the text goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimensionScreen {
    pub kind: DimensionScreenKind,
    pub text: Pos2,
}

/// Where two screen-space lines cross.
fn screen_line_intersection(a1: Pos2, a2: Pos2, b1: Pos2, b2: Pos2) -> Option<Pos2> {
    let d1 = a2 - a1;
    let d2 = b2 - b1;
    let denom = d1.x * d2.y - d1.y * d2.x;
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = ((b1.x - a1.x) * d2.y - (b1.y - a1.y) * d2.x) / denom;
    Some(a1 + d1 * t)
}

/// Where a dimension lands on screen.
///
/// The text position is the one the user clicked when placing the dimension;
/// without one the drawing picks the classic offset. A linear dimension then
/// runs along itself, or horizontally/vertically at the text's height/column.
pub fn dimension_screen_geometry(
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
    dimension: &DimensionAnnotation,
) -> Option<DimensionScreen> {
    let to_screen = |point: [f64; 2]| sketch_to_screen(rect, camera, plane, point);
    let placed = dimension.text_position.and_then(to_screen);
    match &dimension.kind {
        DimensionKind::Linear { a, b, orientation } => {
            let pa = to_screen(*a)?;
            let pb = to_screen(*b)?;
            let (line_a, line_b, default_text) = match orientation {
                DimensionOrientation::Horizontal => {
                    let y = placed.map(|text| text.y).unwrap_or(pa.y - DIMENSION_OFFSET);
                    let line_a = Pos2::new(pa.x, y);
                    let line_b = Pos2::new(pb.x, y);
                    let text = placed
                        .unwrap_or(Pos2::new(line_a.x + (line_b.x - line_a.x) * 0.5, y - 11.0));
                    (line_a, line_b, text)
                }
                DimensionOrientation::Vertical => {
                    let x = placed.map(|text| text.x).unwrap_or(pa.x - DIMENSION_OFFSET);
                    let line_a = Pos2::new(x, pa.y);
                    let line_b = Pos2::new(x, pb.y);
                    let text = placed
                        .unwrap_or(Pos2::new(x - 11.0, line_a.y + (line_b.y - line_a.y) * 0.5));
                    (line_a, line_b, text)
                }
                DimensionOrientation::Aligned => {
                    let direction = pb - pa;
                    let length = direction.length();
                    if length < 1e-3 {
                        return None;
                    }
                    let along = direction / length;
                    let normal = Vec2::new(-along.y, along.x);
                    let offset = placed
                        .map(|text| (text - (pa + (pb - pa) * 0.5)).dot(normal))
                        .unwrap_or(DIMENSION_OFFSET);
                    let line_a = pa + normal * offset;
                    let line_b = pb + normal * offset;
                    let text = placed.unwrap_or(line_a + (line_b - line_a) * 0.5 + normal * 11.0);
                    (line_a, line_b, text)
                }
            };
            Some(DimensionScreen {
                kind: DimensionScreenKind::Linear {
                    a: pa,
                    b: pb,
                    line_a,
                    line_b,
                },
                text: default_text,
            })
        }
        DimensionKind::Angular { first, second } => {
            let first_a = to_screen(first.0)?;
            let first_b = to_screen(first.1)?;
            let second_a = to_screen(second.0)?;
            let second_b = to_screen(second.1)?;
            let centre = screen_line_intersection(first_a, first_b, second_a, second_b)?;
            let text = placed.unwrap_or_else(|| {
                let a = first_b - centre;
                let b = second_b - centre;
                let sum = (a.normalized() + b.normalized()) * 40.0;
                centre
                    + if sum.length() < 1.0 {
                        Vec2::new(30.0, -30.0)
                    } else {
                        sum
                    }
            });
            let radius = (text - centre).length().max(18.0);
            let start_angle = (first_b - centre).angle();
            let end_angle = (second_b - centre).angle();
            let mut sweep = end_angle - start_angle;
            while sweep > std::f32::consts::PI {
                sweep -= std::f32::consts::TAU;
            }
            while sweep < -std::f32::consts::PI {
                sweep += std::f32::consts::TAU;
            }
            Some(DimensionScreen {
                kind: DimensionScreenKind::Angular {
                    centre,
                    radius,
                    start_angle,
                    sweep,
                },
                text,
            })
        }
        DimensionKind::LineDistance { first, second } => {
            let first_a = to_screen(first.0)?;
            let first_b = to_screen(first.1)?;
            let second_a = to_screen(second.0)?;
            let second_b = to_screen(second.1)?;
            let direction = first_b - first_a;
            let length = direction.length();
            if length < 1e-3 {
                return None;
            }
            let along = direction / length;
            let normal = Vec2::new(-along.y, along.x);
            let text = placed
                .unwrap_or_else(|| first_a + (first_b - first_a) * 0.5 + normal * DIMENSION_OFFSET);
            let project = |line_a: Pos2, line_b: Pos2| {
                let line = line_b - line_a;
                let line_length = line.length();
                if line_length < 1e-3 {
                    line_a
                } else {
                    line_a + line / line_length * (text - line_a).dot(line / line_length)
                }
            };
            Some(DimensionScreen {
                kind: DimensionScreenKind::LineDistance {
                    from: project(first_a, first_b),
                    to: project(second_a, second_b),
                },
                text,
            })
        }
        DimensionKind::Radial {
            centre,
            rim,
            diameter,
        } => {
            let centre = to_screen(*centre)?;
            let rim = to_screen(*rim)?;
            let radius = (rim - centre).length();
            if radius < 1e-3 {
                return None;
            }
            let direction = (rim - centre) / radius;
            let text = placed.unwrap_or(rim + direction * 14.0);
            let from = if *diameter {
                centre - direction * radius
            } else {
                centre
            };
            Some(DimensionScreen {
                kind: DimensionScreenKind::Radial { from, to: rim },
                text,
            })
        }
        DimensionKind::PointDistance { point, foot, .. } => {
            let point = to_screen(*point)?;
            let foot = to_screen(*foot)?;
            let text = placed.unwrap_or_else(|| {
                let middle = Pos2::new((point.x + foot.x) * 0.5, (point.y + foot.y) * 0.5);
                middle + Vec2::new(12.0, -12.0)
            });
            Some(DimensionScreen {
                kind: DimensionScreenKind::PointDistance { point, foot },
                text,
            })
        }
        DimensionKind::TangentDistance { touch, foot, .. } => {
            let from = to_screen(*touch)?;
            let to = to_screen(*foot)?;
            let text = placed.unwrap_or_else(|| {
                let middle = Pos2::new((from.x + to.x) * 0.5, (from.y + to.y) * 0.5);
                middle + Vec2::new(12.0, 0.0)
            });
            Some(DimensionScreen {
                kind: DimensionScreenKind::TangentDistance { from, to },
                text,
            })
        }
    }
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
        let distance = match screen.kind {
            DimensionScreenKind::Linear {
                a,
                b,
                line_a,
                line_b,
            } => distance_to_segment(pos, line_a, line_b)
                .min(pos.distance(screen.text))
                .min(distance_to_segment(pos, a, line_a))
                .min(distance_to_segment(pos, b, line_b)),
            DimensionScreenKind::Angular {
                centre,
                radius,
                start_angle,
                sweep,
            } => {
                let mut distance = pos.distance(screen.text);
                let mut previous = None;
                for step in 0..=24 {
                    let angle = start_angle + sweep * step as f32 / 24.0;
                    let point = centre + Vec2::new(angle.cos(), angle.sin()) * radius;
                    if let Some(previous) = previous {
                        distance = distance.min(distance_to_segment(pos, previous, point));
                    }
                    previous = Some(point);
                }
                distance
            }
            DimensionScreenKind::LineDistance { from, to }
            | DimensionScreenKind::Radial { from, to }
            | DimensionScreenKind::TangentDistance { from, to } => {
                distance_to_segment(pos, from, to).min(pos.distance(screen.text))
            }
            DimensionScreenKind::PointDistance { point, foot } => {
                distance_to_segment(pos, point, foot).min(pos.distance(screen.text))
            }
        };
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
    let held = sketch.anchored_points();
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
    /// Whether the pattern repeats along a line or around a point.
    pub pattern_mode: PatternMode,
    /// Total sweep of a circular pattern, in degrees.
    pub pattern_angle: f64,
    /// Radius of a circle drawn tangent to two lines.
    pub circle_radius: f64,
    /// Middle weight of the conic tool: 0.5 is a parabola.
    pub conic_rho: f64,
    /// Height of a text note, in sketch units.
    pub text_height: f64,
}

/// How the pattern tool repeats the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternMode {
    /// Along the line that is clicked, at the strip's spacing.
    #[default]
    Linear,
    /// Around the point that is clicked, spread over the strip's sweep.
    Circular,
}

impl ToolParams {
    /// The label of the pattern mode, for the strip's toggle.
    pub fn pattern_mode_label(mode: PatternMode) -> &'static str {
        match mode {
            PatternMode::Linear => "Linear",
            PatternMode::Circular => "Circular",
        }
    }
}

impl Default for ToolParams {
    fn default() -> Self {
        Self {
            offset_distance: 5.0,
            pattern_count: 3,
            pattern_spacing: 10.0,
            pattern_mode: PatternMode::Linear,
            pattern_angle: 360.0,
            circle_radius: 10.0,
            conic_rho: 0.5,
            text_height: 5.0,
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
        SketchTool::CircleTangent => {
            if hit.kind != SketchEntityKind::Line {
                return Err("Pick two lines for the tangent circle.".to_string());
            }
            picks.push(hit.id);
            if picks.len() < 2 {
                return Ok("Now click the second line.".to_string());
            }
            let (first_line, second_line) = (picks[0], picks[1]);
            picks.clear();
            let ends = |sketch: &Sketch, id: Uuid| -> Option<([f64; 2], [f64; 2])> {
                let line = sketch.lines.iter().find(|line| line.id == id)?;
                Some((point_of(sketch, line.a)?, point_of(sketch, line.b)?))
            };
            let (Some(first), Some(second)) = (ends(sketch, first_line), ends(sketch, second_line))
            else {
                return Err("One of the lines is gone.".to_string());
            };
            match circle_tangent_to_two_lines(first, second, params.circle_radius, cursor) {
                Some((centre, radius)) => {
                    let circle = sketch.add_circle(centre[0], centre[1], radius);
                    sketch.constraints.push(Constraint::Tangent {
                        a: circle,
                        b: first_line,
                    });
                    sketch.constraints.push(Constraint::Tangent {
                        a: circle,
                        b: second_line,
                    });
                    Ok(format!(
                        "Tangent circle of radius {}.",
                        format_length(radius)
                    ))
                }
                None => Err("Those lines are parallel: no finite tangent circle.".to_string()),
            }
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
            let count = params.pattern_count.max(2);
            match params.pattern_mode {
                PatternMode::Linear => {
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
                        .linear_pattern(selection, direction, params.pattern_spacing, count)
                        .map_err(|error| error.to_string())?;
                    Ok(format!("{} patterned entities created.", created.len()))
                }
                PatternMode::Circular => {
                    if hit.kind != SketchEntityKind::Point {
                        return Err("Circular pattern: click the centre point.".to_string());
                    }
                    let centre = point_of(sketch, hit.id)
                        .ok_or_else(|| "The centre point is gone.".to_string())?;
                    let step = params.pattern_angle.to_radians() / count as f64;
                    let created = sketch
                        .circular_pattern(selection, centre, step, count)
                        .map_err(|error| error.to_string())?;
                    Ok(format!(
                        "{} patterned entities created around the point.",
                        created.len()
                    ))
                }
            }
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
        }
    }
}

/// Add a constraint to the sketch from the current sketch selection.
///
/// A constraint that would be nonsense for the selection is refused with a
/// sentence naming what to select instead, so the button can explain itself.

/// A short name for the constraint table: what the constraint is, and for a
/// dimension its value.
pub fn constraint_label(constraint: &Constraint) -> String {
    match constraint {
        Constraint::Coincident { .. } => "Coincident".to_string(),
        Constraint::Distance { value, .. } => {
            format!("Distance {}", format_length(*value))
        }
        Constraint::Horizontal { .. } => "Horizontal".to_string(),
        Constraint::Vertical { .. } => "Vertical".to_string(),
        Constraint::Fixed { .. } => "Fixed".to_string(),
        Constraint::Tangent { .. } => "Tangent".to_string(),
        Constraint::Symmetric { .. } => "Symmetric".to_string(),
        Constraint::EqualRadius { .. } => "Equal radius".to_string(),
        Constraint::EqualLength { .. } => "Equal length".to_string(),
        Constraint::Parallel { .. } => "Parallel".to_string(),
        Constraint::Perpendicular { .. } => "Perpendicular".to_string(),
        Constraint::Midpoint { .. } => "Midpoint".to_string(),
        Constraint::Concentric { .. } => "Concentric".to_string(),
        Constraint::Angle { value, .. } => format!("Angle {}°", format_length(*value)),
        Constraint::LineDistance { value, .. } => {
            format!("Line distance {}", format_length(*value))
        }
        Constraint::HorizontalDistance { value, .. } => {
            format!("Horizontal distance {}", format_length(*value))
        }
        Constraint::VerticalDistance { value, .. } => {
            format!("Vertical distance {}", format_length(*value))
        }
        Constraint::Radius { value, .. } => format!("Radius {}", format_length(*value)),
        Constraint::Diameter { value, .. } => format!("Diameter {}", format_length(*value)),
        Constraint::PointLineDistance { value, .. } => {
            format!("Point-line distance {}", format_length(*value))
        }
        Constraint::LineCurveDistance { value, .. } => {
            format!("Tangent distance {}", format_length(*value))
        }
    }
}

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
    /// The dimension whose value is being edited in place: its number is not
    /// drawn, the editor shows itself.
    pub editing_dimension: Option<usize>,
    /// Draw the entities that still have a degree of freedom in another colour.
    pub show_free_dof: bool,
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
    // Which entities are still free, when the highlight is on: computed once
    // for the whole frame.
    let unconstrained: std::collections::HashSet<Uuid> = if state.show_free_dof {
        sketch.find_unconstrained_entities().into_iter().collect()
    } else {
        std::collections::HashSet::new()
    };
    let style = |id: Uuid| -> (Color32, f32) {
        if state.selection.contains(&id) {
            (SELECTED, 2.6)
        } else if state.hovered == Some(id) {
            (HOVERED, 2.6)
        } else if unconstrained.contains(&id) {
            (FREE_ENTITY, 2.0)
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

    // Conics: rational quadratics, sampled for the screen.
    for conic in &sketch.conics {
        let samples: Vec<Pos2> = sketch
            .conic_points(conic.id, 48)
            .iter()
            .filter_map(|point| sketch_to_screen(rect, camera, plane, *point))
            .collect();
        if samples.len() >= 2 {
            let (colour, width) = style(conic.id);
            painter.add(Shape::line(samples, Stroke::new(width, colour)));
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

    // Text notes: the interface font, sized by the sketch height.
    for text in &sketch.texts {
        let Some(screen) = sketch_to_screen(rect, camera, plane, text.position) else {
            continue;
        };
        let size = sketch_to_screen(
            rect,
            camera,
            plane,
            [text.position[0] + text.height, text.position[1]],
        )
        .map(|edge| (edge - screen).length())
        .unwrap_or(12.0)
        .clamp(4.0, 90.0);
        painter.text(
            screen,
            Align2::LEFT_BOTTOM,
            &text.content,
            FontId::proportional(size),
            TEXT,
        );
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
        let editing = state.editing_dimension == Some(dimension.constraint_index);
        draw_dimension(&painter, rect, camera, plane, &dimension, selected, editing);
    }

    // The two references that position the sketch: R1 along the X axis, R2
    // along the Y axis, so the coordinate system is anchored to something the
    // user (and the feature) can name.
    if let Some(origin) = sketch_to_screen(rect, camera, plane, [0.0, 0.0]) {
        for (role, direction, sign) in [
            (SketchReferenceRole::Orientation, Vec2::new(1.0, 0.0), 1.0),
            (SketchReferenceRole::Automatic, Vec2::new(0.0, 1.0), 1.0),
        ] {
            if let Some(reference) = sketch.reference(role) {
                painter.text(
                    origin + direction * 70.0 * sign,
                    Align2::CENTER_CENTER,
                    format!(
                        "{}: {}",
                        if role == SketchReferenceRole::Orientation {
                            "R1"
                        } else {
                            "R2"
                        },
                        reference.label
                    ),
                    FontId::proportional(10.0),
                    CONSTRAINT,
                );
            }
        }
    }

    // The shape the clicks so far would finish.
    if let Some(cursor) = state.cursor {
        let tool = state.draft.tool();
        let screen = |point: [f64; 2]| sketch_to_screen(rect, camera, plane, point);
        let pending = state.draft.pending();
        match tool {
            SketchTool::Rectangle | SketchTool::RectangleCenter => {
                if let Some(centre) = pending {
                    let corners: [[f64; 2]; 4] = if tool == SketchTool::Rectangle {
                        [
                            centre,
                            [cursor[0], centre[1]],
                            cursor,
                            [centre[0], cursor[1]],
                        ]
                    } else {
                        let dx = (cursor[0] - centre[0]).abs();
                        let dy = (cursor[1] - centre[1]).abs();
                        [
                            [centre[0] - dx, centre[1] - dy],
                            [centre[0] + dx, centre[1] - dy],
                            [centre[0] + dx, centre[1] + dy],
                            [centre[0] - dx, centre[1] + dy],
                        ]
                    };
                    let corners: Vec<Pos2> = corners
                        .iter()
                        .filter_map(|corner| screen(*corner))
                        .collect();
                    if corners.len() == 4 {
                        painter.add(Shape::closed_line(corners, Stroke::new(1.4_f32, PREVIEW)));
                    }
                }
            }
            SketchTool::RectangleSkewed => match state.draft.pending_points() {
                [first] => {
                    if let (Some(a), Some(b)) = (screen(*first), screen(cursor)) {
                        painter.line_segment([a, b], Stroke::new(1.0_f32, PREVIEW));
                    }
                }
                [first, second] => {
                    if let Some(corners) = skewed_rectangle_corners(*first, *second, cursor) {
                        let corners: Vec<Pos2> = corners
                            .iter()
                            .filter_map(|corner| screen(*corner))
                            .collect();
                        if corners.len() == 4 {
                            painter.add(Shape::closed_line(corners, Stroke::new(1.4_f32, PREVIEW)));
                        }
                    } else if let (Some(a), Some(b)) = (screen(*first), screen(*second)) {
                        painter.line_segment([a, b], Stroke::new(1.0_f32, PREVIEW));
                    }
                }
                _ => {}
            },
            SketchTool::Line => {
                if let (Some(first), Some(end)) = (pending, screen(cursor)) {
                    if let Some(start) = screen(first) {
                        painter.line_segment([start, end], Stroke::new(1.4_f32, PREVIEW));
                    }
                }
            }
            SketchTool::LineChain => {
                let mut points = state.draft.pending_points().to_vec();
                points.push(cursor);
                let samples: Vec<Pos2> = points.iter().filter_map(|point| screen(*point)).collect();
                if samples.len() >= 2 {
                    painter.add(Shape::line(samples, Stroke::new(1.4_f32, PREVIEW)));
                }
                // The closing point is the first one: show it as a target.
                if let Some(first) = state.draft.pending_points().first() {
                    if let Some(target) = screen(*first) {
                        painter.circle_stroke(target, CLOSE_MARKER, Stroke::new(1.4_f32, PREVIEW));
                    }
                }
            }
            SketchTool::Circle => {
                if let Some(first) = pending {
                    draw_preview_circle(&painter, &screen, first, cursor);
                }
            }
            SketchTool::CircleDiameter => {
                if let Some(first) = pending {
                    let (centre, radius) = circle_from_diameter(first, cursor);
                    draw_preview_circle(&painter, &screen, centre, [centre[0] + radius, centre[1]]);
                }
            }
            SketchTool::Polygon(sides) => {
                if let Some(centre) = pending {
                    if (cursor[0] - centre[0]).hypot(cursor[1] - centre[1]) > 1e-9 {
                        let corners = regular_polygon(centre, cursor, sides.max(3) as usize);
                        let corners: Vec<Pos2> = corners
                            .iter()
                            .filter_map(|corner| screen(*corner))
                            .collect();
                        if corners.len() >= 3 {
                            painter.add(Shape::closed_line(corners, Stroke::new(1.4_f32, PREVIEW)));
                        }
                    }
                }
            }
            SketchTool::ArcTangent => {
                if let Some(span) = state.draft.tangent_arc() {
                    if let Some((centre, radius, start_angle, end_angle)) =
                        tangent_arc(span.start, span.direction, cursor)
                    {
                        let mut end = end_angle;
                        while end < start_angle {
                            end += std::f64::consts::TAU;
                        }
                        let samples: Vec<Pos2> = (0..=48)
                            .filter_map(|step| {
                                let angle = start_angle + (end - start_angle) * step as f64 / 48.0;
                                screen([
                                    centre[0] + radius * angle.cos(),
                                    centre[1] + radius * angle.sin(),
                                ])
                            })
                            .collect();
                        if samples.len() >= 2 {
                            painter.add(Shape::line(samples, Stroke::new(1.4_f32, PREVIEW)));
                        }
                    }
                }
            }
            SketchTool::Conic => match state.draft.pending_points() {
                [start] => {
                    if let (Some(a), Some(b)) = (screen(*start), screen(cursor)) {
                        painter.line_segment([a, b], Stroke::new(1.0_f32, PREVIEW));
                    }
                }
                [start, end] => {
                    // The preview uses the same shoulder the click would.
                    let rho = state.draft.conic_rho();
                    let control = [
                        (2.0 * cursor[0] * (1.0 + rho) - (start[0] + end[0])) / (2.0 * rho),
                        (2.0 * cursor[1] * (1.0 + rho) - (start[1] + end[1])) / (2.0 * rho),
                    ];
                    let samples: Vec<Pos2> = (0..=32)
                        .filter_map(|step| {
                            let t = step as f64 / 32.0;
                            let b0 = (1.0 - t) * (1.0 - t);
                            let b1 = 2.0 * (1.0 - t) * t * rho;
                            let b2 = t * t;
                            let denominator = (b0 + b1 + b2).max(1e-12);
                            screen([
                                (b0 * start[0] + b1 * control[0] + b2 * end[0]) / denominator,
                                (b0 * start[1] + b1 * control[1] + b2 * end[1]) / denominator,
                            ])
                        })
                        .collect();
                    if samples.len() >= 2 {
                        painter.add(Shape::line(samples, Stroke::new(1.4_f32, PREVIEW)));
                    }
                }
                _ => {}
            },
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

    // The snap cross and the coordinate readout belong to the tools that build
    // geometry; with a picking tool the system cursor is left alone.
    if state.draft.tool().creates_geometry() {
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
}

/// The ghost circle of a circle tool: centre and a point on it.
fn draw_preview_circle(
    painter: &egui::Painter,
    screen: &impl Fn([f64; 2]) -> Option<Pos2>,
    centre: [f64; 2],
    on: [f64; 2],
) {
    let radius = (on[0] - centre[0]).hypot(on[1] - centre[1]);
    let samples: Vec<Pos2> = (0..=64)
        .filter_map(|step| {
            let angle = step as f64 / 64.0 * std::f64::consts::TAU;
            screen([
                centre[0] + radius * angle.cos(),
                centre[1] + radius * angle.sin(),
            ])
        })
        .collect();
    if samples.len() >= 2 {
        painter.add(Shape::line(samples, Stroke::new(1.4_f32, PREVIEW)));
    }
}

fn draw_dimension(
    painter: &egui::Painter,
    rect: Rect,
    camera: &OrbitCamera,
    plane: SketchPlane,
    dimension: &DimensionAnnotation,
    selected: bool,
    editing: bool,
) {
    let Some(screen) = dimension_screen_geometry(rect, camera, plane, dimension) else {
        return;
    };
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

    match screen.kind {
        DimensionScreenKind::Linear {
            a,
            b,
            line_a,
            line_b,
        } => {
            let direction = line_b - line_a;
            let length = direction.length();
            if length < 1e-3 {
                return;
            }
            let along = direction / length;
            let normal = Vec2::new(-along.y, along.x);
            // Extension lines from the geometry to the dimension line.
            painter.line_segment([a, line_a], thin);
            painter.line_segment([b, line_b], thin);
            painter.line_segment([line_a, line_b], body);
            for (tip, sign) in [(line_a, 1.0), (line_b, -1.0)] {
                let back = tip + along * sign * 9.0;
                painter.line_segment([tip, back + normal * 3.0], body);
                painter.line_segment([tip, back - normal * 3.0], body);
            }
        }
        DimensionScreenKind::Angular {
            centre,
            radius,
            start_angle,
            sweep,
        } => {
            let samples: Vec<Pos2> = (0..=24)
                .map(|step| {
                    let angle = start_angle + sweep * step as f32 / 24.0;
                    centre + Vec2::new(angle.cos(), angle.sin()) * radius
                })
                .collect();
            for pair in samples.windows(2) {
                painter.line_segment([pair[0], pair[1]], body);
            }
            // A short extension of each line out to the arc.
            if let (Some(first), Some(last)) = (samples.first(), samples.last()) {
                painter.line_segment([centre, *first], thin);
                painter.line_segment([centre, *last], thin);
                painter.circle_filled(*first, 2.0, colour);
                painter.circle_filled(*last, 2.0, colour);
            }
        }
        DimensionScreenKind::Radial { from, to } => {
            let direction = to - from;
            let length = direction.length();
            if length < 1e-3 {
                return;
            }
            let along = direction / length;
            let normal = Vec2::new(-along.y, along.x);
            painter.line_segment([from, to], body);
            // An arrow at the rim, pointing outwards.
            let back = to - along * 8.0;
            painter.line_segment([to, back + normal * 3.0], body);
            painter.line_segment([to, back - normal * 3.0], body);
        }
        DimensionScreenKind::PointDistance { point, foot } => {
            painter.line_segment([point, foot], body);
            painter.circle_filled(point, 2.5, colour);
            let direction = point - foot;
            let length = direction.length();
            if length > 1e-3 {
                let normal = Vec2::new(-direction.y / length, direction.x / length);
                painter.line_segment([foot + normal * 5.0, foot - normal * 5.0], body);
            }
        }
        DimensionScreenKind::TangentDistance { from, to } => {
            painter.line_segment([from, to], body);
            painter.circle_filled(from, 2.0, colour);
            let direction = to - from;
            let length = direction.length();
            if length > 1e-3 {
                let along = direction / length;
                let normal = Vec2::new(-along.y, along.x);
                let back = to - along * 8.0;
                painter.line_segment([to, back + normal * 3.0], body);
                painter.line_segment([to, back - normal * 3.0], body);
            }
        }
        DimensionScreenKind::LineDistance { from, to } => {
            let direction = to - from;
            let length = direction.length();
            if length < 1e-3 {
                return;
            }
            let along = direction / length;
            let normal = Vec2::new(-along.y, along.x);
            painter.line_segment([from, to], body);
            for (tip, sign) in [(from, 1.0), (to, -1.0)] {
                let back = tip + along * sign * 9.0;
                painter.line_segment([tip, back + normal * 3.0], body);
                painter.line_segment([tip, back - normal * 3.0], body);
            }
        }
    }

    if editing {
        // The in-place editor draws its own field over the number.
        return;
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
    fn a_tangent_arc_leaves_its_line_along_it() {
        // Start at (10, 0), tangent towards +X, end at (20, 10).
        let (centre, radius, start_angle, end_angle) =
            tangent_arc([10.0, 0.0], [1.0, 0.0], [20.0, 10.0]).expect("an arc");
        let start = [
            centre[0] + radius * start_angle.cos(),
            centre[1] + radius * start_angle.sin(),
        ];
        assert!(
            (start[0] - 10.0).abs() < 1e-6 && (start[1]).abs() < 1e-6,
            "{start:?}"
        );
        let end = [
            centre[0] + radius * end_angle.cos(),
            centre[1] + radius * end_angle.sin(),
        ];
        assert!(
            (end[0] - 20.0).abs() < 1e-6 && (end[1] - 10.0).abs() < 1e-6,
            "{end:?}"
        );
        // The tangent at the start runs along +X: the radial turned left.
        let radial = [start[0] - centre[0], start[1] - centre[1]];
        let tangent = if end_angle > start_angle {
            [-radial[1], radial[0]]
        } else {
            [radial[1], -radial[0]]
        };
        let length = tangent[0].hypot(tangent[1]);
        assert!(
            (tangent[0] / length - 1.0).abs() < 1e-6,
            "the arc does not leave along the line: {tangent:?}"
        );
        // The end lies on the tangent line itself: no finite arc.
        assert!(tangent_arc([10.0, 0.0], [1.0, 0.0], [30.0, 0.0]).is_none());
    }

    #[test]
    fn the_conic_tool_passes_through_the_shoulder() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Conic);
        draft.click(&mut sketch, [0.0, 0.0]);
        draft.click(&mut sketch, [20.0, 0.0]);
        assert!(matches!(
            draft.click(&mut sketch, [10.0, 10.0]),
            SketchEdit::Conic(_)
        ));
        assert_eq!(sketch.conics.len(), 1);
        let points = sketch.conic_points(sketch.conics[0].id, 20);
        let middle = points[10];
        assert!((middle[0] - 10.0).abs() < 1e-6, "{middle:?}");
        assert!((middle[1] - 10.0).abs() < 1e-6, "{middle:?}");
        assert!((sketch.conics[0].rho - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_text_tool_asks_for_the_words() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::Text);
        assert_eq!(
            draft.click(&mut sketch, [5.0, 5.0]),
            SketchEdit::TextAt([5.0, 5.0])
        );
        assert!(
            sketch.texts.is_empty(),
            "the app adds the text after typing"
        );
        let edit = draft.finish_text(&mut sketch, [5.0, 5.0], "RCAD", 4.0);
        assert!(matches!(edit, SketchEdit::Text(_)));
        assert_eq!(sketch.texts.len(), 1);
        assert_eq!(sketch.texts[0].content, "RCAD");
    }

    #[test]
    fn a_circular_pattern_repeats_around_the_clicked_point() {
        let mut sketch = Sketch::new("test");
        let corners = sketch.add_rectangle(10.0, 0.0, 20.0, 5.0);
        let selection = vec![sketch.lines[0].id, sketch.lines[1].id];
        let centre = sketch.add_point(0.0, 0.0);
        let params = ToolParams {
            pattern_mode: PatternMode::Circular,
            pattern_angle: 360.0,
            pattern_count: 4,
            ..ToolParams::default()
        };
        let hit = SketchHit {
            id: centre,
            kind: SketchEntityKind::Point,
            distance: 0.0,
        };
        let message = command_step(
            &mut sketch,
            SketchTool::Pattern,
            &mut Vec::new(),
            &selection,
            [0.0, 0.0],
            Some(&hit),
            &params,
        )
        .expect("a circular pattern");
        assert!(message.contains("around the point"), "{message}");
        // The rectangle's four lines plus three rotated copies of the two
        // selected ones.
        assert_eq!(sketch.lines.len(), 4 + 6);
        let _ = corners;
    }

    #[test]
    fn the_constraint_table_names_every_constraint() {
        let mut sketch = Sketch::new("test");
        let corners = sketch.add_rectangle(0.0, 0.0, 10.0, 5.0);
        let circle = sketch.add_circle(0.0, 0.0, 3.0);
        sketch.add_dimension_constraint(
            Constraint::Radius {
                curve: circle,
                value: 3.0,
            },
            true,
            true,
            None,
            DimensionOrientation::Aligned,
        );
        let labels: Vec<String> = sketch.constraints.iter().map(constraint_label).collect();
        assert!(labels.contains(&"Horizontal".to_string()));
        assert!(labels.contains(&"Radius 3".to_string()));
        let _ = corners;
    }

    #[test]
    fn the_new_dimensions_have_screen_geometry() {
        let camera = top_camera();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let plane = xy_plane();
        let mut sketch = Sketch::new("test");
        let circle = sketch.add_circle(0.0, 0.0, 10.0);
        sketch.add_dimension_constraint(
            Constraint::Radius {
                curve: circle,
                value: 10.0,
            },
            true,
            true,
            Some([18.0, 0.0]),
            DimensionOrientation::Aligned,
        );
        let dimensions = dimensions(&sketch);
        assert_eq!(dimensions.len(), 1);
        assert!(matches!(dimensions[0].kind, DimensionKind::Radial { .. }));
        assert_eq!(dimensions[0].text, "R10");
        let screen =
            dimension_screen_geometry(rect, &camera, plane, &dimensions[0]).expect("on screen");
        assert!(matches!(screen.kind, DimensionScreenKind::Radial { .. }));
        assert_eq!(
            dimension_hit_test(&sketch, rect, &camera, plane, screen.text, HIT_RADIUS),
            Some(dimensions[0].constraint_index)
        );
    }

    #[test]
    fn the_centre_rectangle_is_centred_on_the_first_click() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::RectangleCenter);
        draft.click(&mut sketch, [10.0, 10.0]);
        assert!(matches!(
            draft.click(&mut sketch, [30.0, 20.0]),
            SketchEdit::Rectangle(_)
        ));
        let corners: Vec<[f64; 2]> = sketch.points.iter().map(|p| [p.x, p.y]).collect();
        for expected in [[-10.0, 0.0], [30.0, 0.0], [30.0, 20.0], [-10.0, 20.0]] {
            assert!(
                corners.contains(&expected),
                "corner {expected:?} missing from {corners:?}"
            );
        }
    }

    #[test]
    fn the_skewed_rectangle_follows_the_first_edge() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::RectangleSkewed);
        // First edge along +X, width 5 upwards.
        draft.click(&mut sketch, [0.0, 0.0]);
        draft.click(&mut sketch, [10.0, 0.0]);
        assert!(matches!(
            draft.click(&mut sketch, [0.0, 5.0]),
            SketchEdit::Polygon(_)
        ));
        let corners: Vec<[f64; 2]> = sketch.points.iter().map(|p| [p.x, p.y]).collect();
        for expected in [[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]] {
            assert!(
                corners.contains(&expected),
                "corner {expected:?} missing from {corners:?}"
            );
        }
        // It stays a rectangle: two parallels and one perpendicular.
        assert!(
            sketch
                .constraints
                .iter()
                .any(|constraint| matches!(constraint, Constraint::Parallel { .. }))
        );
        assert!(
            sketch
                .constraints
                .iter()
                .any(|constraint| matches!(constraint, Constraint::Perpendicular { .. }))
        );
    }

    #[test]
    fn the_polygon_tools_build_a_regular_polygon() {
        for (sides, tool) in [
            (3, SketchTool::Polygon(3)),
            (5, SketchTool::Polygon(5)),
            (6, SketchTool::Polygon(6)),
        ] {
            let mut sketch = Sketch::new("test");
            let mut draft = SketchDraft::default();
            draft.begin(tool);
            draft.click(&mut sketch, [0.0, 0.0]);
            assert!(matches!(
                draft.click(&mut sketch, [10.0, 0.0]),
                SketchEdit::Polygon(_)
            ));
            assert_eq!(sketch.points.len(), sides, "{tool:?}");
            assert_eq!(sketch.lines.len(), sides, "{tool:?}");
            // Every side is the same length.
            let lengths: Vec<f64> = sketch
                .lines
                .iter()
                .map(|line| {
                    let a = sketch.point(line.a).unwrap();
                    let b = sketch.point(line.b).unwrap();
                    (b.x - a.x).hypot(b.y - a.y)
                })
                .collect();
            let first = lengths[0];
            assert!(
                lengths.iter().all(|length| (length - first).abs() < 1e-6),
                "{lengths:?}"
            );
        }
    }

    #[test]
    fn the_diameter_circle_puts_both_points_on_the_rim() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::CircleDiameter);
        draft.click(&mut sketch, [-10.0, 0.0]);
        assert!(matches!(
            draft.click(&mut sketch, [10.0, 0.0]),
            SketchEdit::Circle(_)
        ));
        assert_eq!(sketch.circles.len(), 1);
        let circle = &sketch.circles[0];
        let centre = sketch.point(circle.center).unwrap();
        assert!((centre.x).abs() < 1e-9 && (centre.y).abs() < 1e-9);
        assert!((circle.radius - 10.0).abs() < 1e-9);
    }

    #[test]
    fn the_polyline_snaps_closed_and_leaves_the_tool() {
        let mut sketch = Sketch::new("test");
        let mut draft = SketchDraft::default();
        draft.begin(SketchTool::LineChain);
        draft.click(&mut sketch, [0.0, 0.0]);
        draft.click(&mut sketch, [10.0, 0.0]);
        draft.click(&mut sketch, [10.0, 10.0]);
        // The app snaps the closing click exactly onto the first point.
        assert!(matches!(
            draft.click(&mut sketch, [0.0, 0.0]),
            SketchEdit::Closed(_)
        ));
        assert_eq!(sketch.lines.len(), 3);
        assert_eq!(draft.pending_points().len(), 0);
        assert_eq!(
            draft.tool(),
            SketchTool::LineChain,
            "the app leaves the tool"
        );
    }

    #[test]
    fn a_tangent_circle_sits_on_both_lines() {
        // Two lines meeting at a right angle: a circle of radius 5 fits in the
        // corner at (5, 5).
        let first = ([-20.0, 0.0], [20.0, 0.0]);
        let second = ([0.0, -20.0], [0.0, 20.0]);
        let (centre, radius) =
            circle_tangent_to_two_lines(first, second, 5.0, [5.0, 5.0]).expect("a circle");
        assert!((radius - 5.0).abs() < 1e-9);
        assert!(
            (centre[0] - 5.0).abs() < 1e-6 && (centre[1] - 5.0).abs() < 1e-6,
            "{centre:?}"
        );

        // Parallel lines never meet, so no tangent circle of that radius.
        let parallel = ([0.0, 10.0], [10.0, 10.0]);
        assert!(circle_tangent_to_two_lines(first, parallel, 5.0, [0.0, 0.0]).is_none());
    }

    #[test]
    fn angle_and_line_distance_helpers_agree_with_hand_values() {
        assert!((angle_between([1.0, 0.0], [0.0, 1.0]) - 90.0).abs() < 1e-9);
        assert!((angle_between([1.0, 0.0], [1.0, 0.0])).abs() < 1e-9);
        assert!((angle_between([1.0, 0.0], [-1.0, 0.0]) - 180.0).abs() < 1e-9);
        let distance = line_distance(([0.0, 0.0], [20.0, 0.0]), ([0.0, 7.0], [20.0, 7.0]));
        assert!((distance - 7.0).abs() < 1e-9, "{distance}");
    }

    #[test]
    fn an_angle_constraint_is_drawn_as_an_angular_dimension() {
        let camera = top_camera();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let plane = xy_plane();
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(-20.0, 0.0);
        let b = sketch.add_point(20.0, 0.0);
        let first = sketch.add_line(a, b);
        let c = sketch.add_point(0.0, -20.0);
        let d = sketch.add_point(0.0, 20.0);
        let second = sketch.add_line(c, d);
        sketch.add_dimension_constraint(
            Constraint::Angle {
                a: first,
                b: second,
                value: 90.0,
            },
            true,
            true,
            Some([10.0, 10.0]),
            DimensionOrientation::Aligned,
        );

        let dimensions = dimensions(&sketch);
        assert_eq!(dimensions.len(), 1);
        assert!(matches!(dimensions[0].kind, DimensionKind::Angular { .. }));
        assert_eq!(dimensions[0].text, "90°");
        let screen =
            dimension_screen_geometry(rect, &camera, plane, &dimensions[0]).expect("on screen");
        assert!(matches!(screen.kind, DimensionScreenKind::Angular { .. }));
        assert_eq!(
            dimension_hit_test(&sketch, rect, &camera, plane, screen.text, HIT_RADIUS),
            Some(dimensions[0].constraint_index)
        );
    }

    #[test]
    fn a_horizontal_dimension_measures_the_x_distance() {
        let mut sketch = Sketch::new("test");
        let a = sketch.add_point(0.0, 0.0);
        let b = sketch.add_point(30.0, 40.0);
        sketch.add_dimension_constraint(
            Constraint::HorizontalDistance { a, b, value: 30.0 },
            true,
            true,
            Some([15.0, 60.0]),
            DimensionOrientation::Horizontal,
        );
        let dimensions = dimensions(&sketch);
        assert_eq!(dimensions.len(), 1);
        assert_eq!(dimensions[0].measured, 30.0);
        assert_eq!(dimensions[0].text, "30");
        assert!(matches!(
            dimensions[0].kind,
            DimensionKind::Linear {
                orientation: DimensionOrientation::Horizontal,
                ..
            }
        ));
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
