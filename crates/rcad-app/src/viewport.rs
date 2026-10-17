use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use glam::{Mat4, Vec3, Vec4};
use rcad_doc::{Feature, FeatureKind, ConstructionKind, Part, ProjectManifest};
use rcad_geom::{NamedEdge, TriangleMesh};

use crate::camera::OrbitCamera;
use crate::selection::{Selected, Selection};

/// 3D ray for picking
#[derive(Debug, Clone, Copy)]
pub struct Ray3D {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray3D {
    /// Create ray from screen position
    pub fn from_screen(pos: Pos2, rect: Rect, camera: &OrbitCamera) -> Self {
        let aspect = rect.width() / rect.height().max(1.0);
        let vp = camera.view_proj(aspect);
        
        // Convert screen to NDC
        let x = (pos.x - rect.left()) / rect.width() * 2.0 - 1.0;
        let y = 1.0 - (pos.y - rect.top()) / rect.height() * 2.0;
        
        // Near and far plane points in clip space
        let near_clip = Vec4::new(x, y, -1.0, 1.0);
        let far_clip = Vec4::new(x, y, 1.0, 1.0);
        
        // Inverse view-projection
        let inv_vp = vp.inverse();
        
        let near_world = inv_vp * near_clip;
        let far_world = inv_vp * far_clip;
        
        let near = Vec3::new(
            near_world.x / near_world.w,
            near_world.y / near_world.w,
            near_world.z / near_world.w,
        );
        let far = Vec3::new(
            far_world.x / far_world.w,
            far_world.y / far_world.w,
            far_world.z / far_world.w,
        );
        
        let direction = (far - near).normalize_or_zero();
        
        Self {
            origin: near,
            direction,
        }
    }
    
    /// Test ray-triangle intersection
    pub fn intersect_triangle(&self, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<f32> {
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let h = self.direction.cross(edge2);
        let a = edge1.dot(h);
        
        if a.abs() < 1e-6 {
            return None; // Parallel
        }
        
        let f = 1.0 / a;
        let s = self.origin - v0;
        let u = f * s.dot(h);
        if u < 0.0 || u > 1.0 {
            return None;
        }
        
        let q = s.cross(edge1);
        let v = f * self.direction.dot(q);
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        
        let t = f * edge2.dot(q);
        if t > 1e-4 {
            Some(t)
        } else {
            None
        }
    }
    
    /// Test ray-plane intersection (infinite plane)
    pub fn intersect_plane(&self, plane_origin: Vec3, plane_normal: Vec3) -> Option<f32> {
        let denom = self.direction.dot(plane_normal);
        if denom.abs() < 1e-6 {
            return None; // Parallel
        }
        let t = (plane_origin - self.origin).dot(plane_normal) / denom;
        if t > 1e-4 {
            Some(t)
        } else {
            None
        }
    }
    
    /// Test ray-line intersection (for axes)
    pub fn intersect_line(&self, line_origin: Vec3, line_direction: Vec3, radius: f32) -> Option<f32> {
        // Distance from ray to line
        let w0 = self.origin - line_origin;
        let a = self.direction.dot(self.direction);
        let b = self.direction.dot(line_direction);
        let c = line_direction.dot(line_direction);
        let d = self.direction.dot(w0);
        let e = line_direction.dot(w0);
        
        let denom = a * c - b * b;
        if denom.abs() < 1e-6 {
            return None; // Parallel
        }
        
        let sc = (b * e - c * d) / denom;
        let tc = (a * e - b * d) / denom;
        
        let closest_point_on_ray = self.origin + self.direction * sc;
        let closest_point_on_line = line_origin + line_direction * tc;
        
        let distance = (closest_point_on_ray - closest_point_on_line).length();
        
        if distance < radius && sc > 1e-4 {
            Some(sc)
        } else {
            None
        }
    }
    
    /// Test ray-sphere intersection (for points)
    pub fn intersect_sphere(&self, center: Vec3, radius: f32) -> Option<f32> {
        let oc = self.origin - center;
        let a = self.direction.dot(self.direction);
        let b = 2.0 * oc.dot(self.direction);
        let c = oc.dot(oc) - radius * radius;
        let discriminant = b * b - 4.0 * a * c;
        
        if discriminant < 0.0 {
            return None;
        }
        
        let t1 = (-b - discriminant.sqrt()) / (2.0 * a);
        let t2 = (-b + discriminant.sqrt()) / (2.0 * a);
        
        if t1 > 1e-4 {
            Some(t1)
        } else if t2 > 1e-4 {
            Some(t2)
        } else {
            None
        }
    }
}

// PickTarget enum is kept for future use but currently not directly used
// Kept for potential future picking enhancements
#[allow(dead_code)]
pub enum PickTarget {
    Triangle(usize),
    Edge(usize),
    Vertex(usize),
    Plane(usize),      // Datum plane feature index
    Axis(usize),       // Datum axis feature index
    Point(usize),      // Datum point feature index
    CoordSystem(usize), // Coordinate system feature index
    None,
}


// ConstructionGeometry struct is kept for future use but currently not directly used
// Kept for potential future rendering enhancements
#[allow(dead_code)]
pub struct ConstructionGeometry {
    pub feature_idx: usize,
    pub kind: ConstructionKind,
    pub origin: Vec3,
    pub direction: Vec3,
    pub name: String,
    pub visible: bool,
    pub color: Color32,
}

#[allow(dead_code)]
impl ConstructionGeometry {
    pub fn from_feature(feature: &Feature, idx: usize) -> Option<Self> {
        if let FeatureKind::Construction { kind, origin, direction } = &feature.kind {
            if !feature.visible {
                return None;
            }
            let (color, name) = match kind {
                ConstructionKind::Plane => (Color32::from_rgb(100, 200, 255), format!("{} (Plane)", feature.name)),
                ConstructionKind::Axis => (Color32::from_rgb(255, 200, 100), format!("{} (Axis)", feature.name)),
                ConstructionKind::Point => (Color32::from_rgb(255, 100, 200), format!("{} (Point)", feature.name)),
                ConstructionKind::CoordSystem => (Color32::WHITE, format!("{} (CoordSys)", feature.name)),
                _ => (Color32::GRAY, feature.name.clone()),
            };
            Some(Self {
                feature_idx: idx,
                kind: *kind,
                origin: Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32),
                direction: Vec3::new(direction[0] as f32, direction[1] as f32, direction[2] as f32).normalize_or_zero(),
                name,
                visible: feature.visible,
                color,
            })
        } else {
            None
        }
    }
}

pub fn draw_viewport(
     ui: &mut Ui,
     mesh: &TriangleMesh,
     camera: &mut OrbitCamera,
     underlay_tint: Option<Color32>,
     // The one selection list, shared with the model tree.
     selection: &Selection,
     hover: Option<&Selected>,
     part: Option<&Part>,
    // The open project's rcad.toml: it owns the RGB value of every colour id.
    manifest: &ProjectManifest,
    show_grid: bool,
    grid_spacing: f32,
    // WGPU solid renderer; `None` falls back to the egui painter.
    solid_renderer: Option<&mut crate::render::SolidRenderer>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());

    // Middle mouse button drag = Orbit.
    //
    // drag_motion() uses the raw, unaccelerated pointer motion, which - unlike
    // drag_delta() - is not clamped by the screen edges. With drag_delta() the
    // rotation stopped as soon as the cursor reached the edge of the screen and
    // the user had to let go and start a new drag to keep turning.
    if response.dragged_by(egui::PointerButton::Middle) {
        let d = response.drag_motion();
        camera.orbit(glam::Vec2::new(d.x, d.y));
    }

    // Right mouse button drag = Pan (only when dragged, not clicked)
    if response.dragged_by(egui::PointerButton::Secondary) {
        let d = response.drag_motion();
        camera.pan(glam::Vec2::new(d.x, d.y));
    }
    
    // Scroll wheel = Zoom
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            camera.zoom(scroll / 50.0);
        }
    }
    
    // Right-click (not dragged) = Context menu handled by egui automatically
    // The response will have .secondary_clicked() for context menu

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 30, 34));

    // Ground grid (optional, spacing from the application settings)
    if show_grid {
        draw_grid(&painter, rect, camera, grid_spacing);
    }

    if let Some(tint) = underlay_tint {
        painter.rect_filled(
            Rect::from_center_size(rect.center(), Vec2::splat(rect.width().min(rect.height()) * 0.35)),
            4.0,
            tint,
        );
    }

    if !mesh.is_empty() {
        match solid_renderer {
            Some(renderer) => renderer.paint(
                &painter, rect, mesh, camera, selection, hover, part, manifest,
            ),
            None => draw_mesh(&painter, rect, mesh, camera, selection, hover, part, manifest),
        }
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No solid — add a feature",
            egui::FontId::proportional(16.0),
            Color32::GRAY,
        );
    }

    // Solid edges are only drawn when they matter: selected or hovered.
    draw_edges(&painter, rect, camera, mesh, selection, hover);

    // Draw construction geometry from part features
    if let Some(part) = part {
        draw_construction_geometry(&painter, rect, camera, part, selection, hover);
    }

    // Draw corner CYS (Coordinate System) in bottom-left
    draw_corner_cys(&painter, rect, camera);

    response
}

fn view_proj(rect: Rect, camera: &OrbitCamera) -> Mat4 {
    let aspect = rect.width() / rect.height().max(1.0);
    camera.view_proj(aspect)
}

/// Where a world point lands on screen, or `None` when it is behind the camera
/// or outside the clip range.
pub fn project_world(rect: Rect, camera: &OrbitCamera, point: Vec3) -> Option<Pos2> {
    project(view_proj(rect, camera), rect, point)
}

fn project(vp: Mat4, rect: Rect, p: Vec3) -> Option<Pos2> {
    let clip = vp * Vec4::new(p.x, p.y, p.z, 1.0);
    if clip.w.abs() < 1e-6 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.z < -1.0 || ndc.z > 1.0 {
        return None;
    }
    let x = rect.left() + (ndc.x * 0.5 + 0.5) * rect.width();
    let y = rect.top() + (1.0 - (ndc.y * 0.5 + 0.5)) * rect.height();
    Some(Pos2::new(x, y))
}

/// Ground grid in the XY plane (Z = 0), since Z is up.
///
/// The spacing is a user setting; the extent stays fixed so the grid always
/// covers the visible area.
fn draw_grid(painter: &egui::Painter, rect: Rect, camera: &OrbitCamera, spacing: f32) {
    const EXTENT: f32 = 100.0;
    let spacing = if spacing > 0.0 { spacing } else { 10.0 };
    let steps = (EXTENT / spacing).round().max(1.0) as i32;
    let vp = view_proj(rect, camera);
    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(50, 54, 60));
    for i in -steps..=steps {
        let t = i as f32 * spacing;
        if let (Some(a), Some(b)) = (
            project(vp, rect, Vec3::new(t, -100.0, 0.0)),
            project(vp, rect, Vec3::new(t, 100.0, 0.0)),
        ) {
            painter.line_segment([a, b], stroke);
        }
        if let (Some(a), Some(b)) = (
            project(vp, rect, Vec3::new(-100.0, t, 0.0)),
            project(vp, rect, Vec3::new(100.0, t, 0.0)),
        ) {
            painter.line_segment([a, b], stroke);
        }
    }
}

// Draw world axes at viewport origin (for reference)
// Draw coordinate system triad in bottom-left corner (CYS)
fn draw_corner_cys(painter: &egui::Painter, rect: Rect, camera: &OrbitCamera) {
    // Fixed position in bottom-left corner (screen space)
    let corner_origin = Pos2::new(60.0, rect.height() - 60.0);
    let size = 40.0; // Small size for corner CYS
    
    // Get camera orientation to show world axes correctly
    // We need to project world axes endpoints
    let vp = view_proj(rect, camera);
    
    // World axes endpoints
    let _world_axes = [
        (Vec3::X * size, Color32::RED, "X"),
        (Vec3::Y * size, Color32::GREEN, "Y"),
        (Vec3::Z * size, Color32::BLUE, "Z"),
    ];
    
    // Project world axes from origin
    if let Some(_op) = project(vp, rect, Vec3::ZERO) {
        // Draw the corner CYS at fixed screen position
        // We'll draw it using screen-space coordinates directly
        let axes_2d = [
            (Vec2::new(size, 0.0), Color32::RED, "X"),   // X to the right
            (Vec2::new(0.0, -size), Color32::GREEN, "Y"), // Y up
            (Vec2::new(-size * 0.7, -size * 0.7), Color32::BLUE, "Z"), // Z diagonal (iso view)
        ];
        
        for (axis_vec, color, label) in axes_2d {
            let ep = corner_origin + axis_vec;
            painter.line_segment([corner_origin, ep], Stroke::new(2.0_f32, color));
            
            // Arrowhead
            let arrow_len = 8.0;
            let dir_2d = axis_vec.normalized();
            let perp = Vec2::new(-dir_2d.y, dir_2d.x);
            let arrow1 = ep - dir_2d * arrow_len + perp * 4.0;
            let arrow2 = ep - dir_2d * arrow_len - perp * 4.0;
            painter.line_segment([ep, arrow1], Stroke::new(2.0_f32, color));
            painter.line_segment([ep, arrow2], Stroke::new(2.0_f32, color));
            
            // Axis label
            painter.text(
                ep + dir_2d * 6.0,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                color,
            );
        }
        
        // Origin dot
        painter.circle_filled(corner_origin, 3.0, Color32::WHITE);
    }
}

/// The project colour a triangle should be drawn with.
///
/// Returns `None` when the triangle has no stable face name, when the face is
/// untagged, or when the project does not define a value for its id. The caller
/// then keeps the default shading. Construction geometry carries no name, so it
/// can never match a face colour.
pub fn face_colour(
    face_name: Option<&str>,
    part: Option<&Part>,
    manifest: &ProjectManifest,
) -> Option<[u8; 3]> {
    part?.face_colour(face_name?, manifest)
}

/// Mix `a` towards `b` by `t`, for highlights that do not hide the base colour.
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Apply the viewport lighting to a project colour, so a coloured face still
/// reads as a solid rather than a flat sticker.
pub fn shaded_colour(rgb: [u8; 3], brightness: f32) -> Color32 {
    let scale = |channel: u8| (channel as f32 * brightness).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(scale(rgb[0]), scale(rgb[1]), scale(rgb[2]))
}

/// Colour of a selected element. Creo uses a warm highlight for the picked
/// geometry, which reads well against the default grey shading.
pub const SELECTED_COLOUR: Color32 = Color32::from_rgb(255, 170, 40);
/// Colour of what the cursor is over.
pub const HOVER_COLOUR: Color32 = Color32::from_rgb(120, 200, 255);
/// The plane a sketch is drawn on, and its two axes.
const SKETCH_GRID_COLOUR: Color32 = Color32::from_rgb(78, 96, 122);
const SKETCH_X_COLOUR: Color32 = Color32::from_rgb(214, 96, 96);
const SKETCH_Y_COLOUR: Color32 = Color32::from_rgb(120, 190, 120);

fn draw_mesh(
    painter: &egui::Painter,
    rect: Rect,
    mesh: &TriangleMesh,
    camera: &OrbitCamera,
    selection: &Selection,
    hover: Option<&Selected>,
    part: Option<&Part>,
    manifest: &ProjectManifest,
) {
     let vp = view_proj(rect, camera);
     let eye = camera.eye();
     let mut tris: Vec<(f32, [Pos2; 3], Color32)> = Vec::with_capacity(mesh.triangle_count());

     for (triangle_index, tri) in mesh.indices.chunks_exact(3).enumerate() {
         let i0 = tri[0] as usize;
         let i1 = tri[1] as usize;
         let i2 = tri[2] as usize;
         let p0 = Vec3::from(mesh.positions[i0]);
         let p1 = Vec3::from(mesh.positions[i1]);
         let p2 = Vec3::from(mesh.positions[i2]);
         let n = if mesh.normals.len() == mesh.positions.len() {
             Vec3::from(mesh.normals[i0])
         } else {
             (p1 - p0).cross(p2 - p0).normalize_or_zero()
         };
         let center = (p0 + p1 + p2) / 3.0;
         let view = (eye - center).normalize_or_zero();
         if n.dot(view) < 0.0 {
             continue; // back-face
         }
         let light = Vec3::new(0.4, 0.8, 0.5).normalize();
         // Ambient floor of 0.3 keeps unlit faces readable instead of near-black.
         let brightness = n.dot(light).clamp(0.3, 1.0);
         let shade = (brightness * 220.0) as u8;
         // A triangle whose face is tagged in the document takes the project
         // colour for that name; every other triangle keeps the default shading
         // instead of guessing a colour.
         let mut color = match face_colour(mesh.face_name(triangle_index), part, manifest) {
             Some(rgb) => shaded_colour(rgb, brightness),
             None => Color32::from_rgb(
                 shade,
                 shade.saturating_add(10),
                 shade.saturating_add(30),
             ),
         };

         // Highlight by stable reference, not by triangle: every face of the
         // selection lights up, whatever triangle the picker landed on.
         if let Some(name) = mesh.face_name(triangle_index) {
             let hovered = matches!(hover, Some(Selected::Face { reference, .. }) if reference == name);
             if hovered {
                 color = blend(color, HOVER_COLOUR, 0.35);
             }
             if selection.has_face(name) {
                 // Tint, do not replace: a selected face must still show the
                 // colour the project gave it.
                 color = blend(color, SELECTED_COLOUR, 0.55);
             }
         }
         
         let (Some(a), Some(b), Some(c)) = (
             project(vp, rect, p0),
             project(vp, rect, p1),
             project(vp, rect, p2),
         ) else {
             continue;
         };
         let depth = (center - eye).length();
         tris.push((depth, [a, b, c], color));
     }

     tris.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
     for (_, pts, color) in tris {
         painter.add(Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
     }
}

/// Selected and hovered solid edges, as thick lines over the shading.
fn draw_edges(
    painter: &egui::Painter,
    rect: Rect,
    camera: &OrbitCamera,
    mesh: &TriangleMesh,
    selection: &Selection,
    hover: Option<&Selected>,
) {
    let hovered = match hover {
        Some(Selected::Edge { reference, .. }) => Some(reference.as_str()),
        _ => None,
    };
    let vp = view_proj(rect, camera);
    for edge in &mesh.edges {
        let selected = selection.has_edge(&edge.name);
        let is_hovered = hovered == Some(edge.name.as_str());
        if !selected && !is_hovered {
            continue;
        }
        let points: Vec<Pos2> = edge
            .points
            .iter()
            .filter_map(|point| project(vp, rect, Vec3::from(*point)))
            .collect();
        if points.len() < 2 {
            continue;
        }
        let (colour, width) = if selected {
            (SELECTED_COLOUR, 3.0_f32)
        } else {
            (HOVER_COLOUR, 2.0_f32)
        };
        painter.add(Shape::line(points, Stroke::new(width, colour)));
    }
}

/// Two perpendicular unit vectors spanning the plane with this normal, the
/// first as close to the wanted x direction as the plane allows.
pub fn sketch_basis(normal: [f64; 3], x_direction: [f64; 3]) -> ([f32; 3], [f32; 3]) {
    let n = Vec3::new(normal[0] as f32, normal[1] as f32, normal[2] as f32).normalize_or_zero();
    let preferred =
        Vec3::new(x_direction[0] as f32, x_direction[1] as f32, x_direction[2] as f32)
            .normalize_or_zero();
    let mut u = preferred - n * preferred.dot(n);
    if u.length_squared() < 0.5 {
        // The wanted direction is along the normal: any perpendicular will do.
        let fallback = if n.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
        u = fallback - n * fallback.dot(n);
    }
    let u = u.normalize_or_zero();
    let v = n.cross(u).normalize_or_zero();
    (u.to_array(), v.to_array())
}

/// The grid of a sketch plane: pairs of endpoints, `extent` from the origin each
/// way and `spacing` apart, so the user can see where the sketch lies.
pub fn sketch_grid_lines(
    origin: [f64; 3],
    u: [f32; 3],
    v: [f32; 3],
    extent: f32,
    spacing: f32,
) -> Vec<([f32; 3], [f32; 3])> {
    let origin = Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32);
    let u = Vec3::from(u);
    let v = Vec3::from(v);
    let spacing = spacing.max(1e-3);
    let steps = (extent / spacing).floor().max(1.0) as i32;
    let mut lines = Vec::new();
    for step in -steps..=steps {
        let offset = step as f32 * spacing;
        lines.push((
            origin + v * offset - u * extent,
            origin + v * offset + u * extent,
        ));
        lines.push((
            origin + u * offset - v * extent,
            origin + u * offset + v * extent,
        ));
    }
    lines.into_iter().map(|(a, b)| (a.to_array(), b.to_array())).collect()
}

/// Draw the plane a sketch is being drawn on: its grid, and its two axes
/// brighter so the origin and the directions are readable.
pub fn draw_sketch_plane(
    ui: &mut egui::Ui,
    rect: Rect,
    camera: &OrbitCamera,
    origin: [f64; 3],
    u: [f32; 3],
    v: [f32; 3],
    extent: f32,
    spacing: f32,
) {
    let painter = ui.painter_at(rect);
    let vp = view_proj(rect, camera);
    let project_3d = |point: [f32; 3]| project(vp, rect, Vec3::from(point));
    for (a, b) in sketch_grid_lines(origin, u, v, extent, spacing) {
        let (Some(pa), Some(pb)) = (project_3d(a), project_3d(b)) else {
            continue;
        };
        painter.add(Shape::line_segment(
            [pa, pb],
            Stroke::new(1.0_f32, SKETCH_GRID_COLOUR),
        ));
    }
    let origin = Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32);
    let u = Vec3::from(u);
    let v = Vec3::from(v);
    for (direction, colour) in [(u, SKETCH_X_COLOUR), (v, SKETCH_Y_COLOUR)] {
        let (Some(pa), Some(pb)) = (
            project_3d((origin - direction * extent).to_array()),
            project_3d((origin + direction * extent).to_array()),
        ) else {
            continue;
        };
        painter.add(Shape::line_segment([pa, pb], Stroke::new(2.0_f32, colour)));
    }
}

fn draw_construction_geometry(
    painter: &egui::Painter,
    rect: Rect,
    camera: &OrbitCamera,
    part: &Part,
    selection: &Selection,
    hover: Option<&Selected>,
) {
    let vp = view_proj(rect, camera);
    let eye = camera.eye();
    
    // Collect all construction geometry with their screen-space depths for proper sorting
    let mut construction_items: Vec<(f32, usize, &Feature, ConstructionKind, Vec3, Vec3)> = Vec::new();
    
    for (idx, feature) in part.features.iter().enumerate() {
        if feature.suppressed || part.suppressed.contains(&feature.id) {
            continue;
        }
        if !feature.visible {
            continue;
        }
        
        if let FeatureKind::Construction { kind, origin, direction } = &feature.kind {
            let origin_v = Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32);
            let dir_v = Vec3::new(direction[0] as f32, direction[1] as f32, direction[2] as f32).normalize_or_zero();
            
            // Compute screen-space depth for sorting
            if let Some(_center_pos) = project(vp, rect, origin_v) {
                // Use camera distance as depth
                let depth = (origin_v - eye).length();
                construction_items.push((depth, idx, feature, *kind, origin_v, dir_v));
            }
        }
    }
    
    // Sort by depth (back to front for proper transparency)
    construction_items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    
    // Draw in sorted order
    let hovered_feature = hover.and_then(Selected::feature);
    for (_, idx, feature, kind, origin_v, dir_v) in construction_items {
        let is_selected = selection.has_feature(feature.id) || hovered_feature == Some(feature.id);
        
        match kind {
            ConstructionKind::Plane => {
                draw_datum_plane(painter, rect, camera, vp, eye, origin_v, dir_v, feature, idx, is_selected);
            }
            ConstructionKind::Axis => {
                draw_datum_axis(painter, rect, camera, vp, eye, origin_v, dir_v, feature, idx, is_selected);
            }
            ConstructionKind::Point => {
                draw_datum_point(painter, rect, camera, vp, eye, origin_v, feature, idx, is_selected);
            }
            ConstructionKind::CoordSystem => {
                draw_coord_system(painter, rect, camera, vp, eye, origin_v, dir_v, feature, idx, is_selected);
            }
            _ => {}
        }
    }
}

fn draw_datum_plane(
    painter: &egui::Painter,
    rect: Rect,
    _camera: &OrbitCamera,
    vp: Mat4,
    _eye: Vec3,
    origin: Vec3,
    normal: Vec3,
    feature: &Feature,
    _idx: usize,
    is_selected: bool,
) {
    // Draw a plane as a transparent solid (not a grid)
    let size = 60.0; // Reduced size
    let half = size * 0.5;
    
    // Create two perpendicular vectors in the plane
    let u = if normal.dot(Vec3::Z).abs() < 0.9 {
        Vec3::Z.cross(normal).normalize_or_zero()
    } else {
        Vec3::X.cross(normal).normalize_or_zero()
    };
    let v = normal.cross(u);
    
    let corners = [
        origin + u * half + v * half,
        origin - u * half + v * half,
        origin - u * half - v * half,
        origin + u * half - v * half,
    ];
    
    let mut projected = Vec::new();
    for corner in &corners {
        if let Some(p) = project(vp, rect, *corner) {
            projected.push(p);
        }
    }
    
    if projected.len() == 4 {
        let center = (corners[0] + corners[1] + corners[2] + corners[3]) / 4.0;
        
        // Datum planes are a faint tint plus an outline. These used
        // from_rgba_premultiplied() with un-premultiplied values, which
        // saturated the fill and hid the model behind seven opaque rectangles.
        let (base_color, edge_color) = if is_selected {
            (Color32::from_rgba_unmultiplied(120, 200, 255, 64), Color32::YELLOW)
        } else {
            (
                Color32::from_rgba_unmultiplied(110, 180, 255, 22),
                Color32::from_rgb(90, 150, 215),
            )
        };
        
        // Draw filled quad with transparency (transparent solid)
        painter.add(Shape::convex_polygon(projected.clone(), base_color, Stroke::NONE));
        
        // Draw boundary edges (thicker when selected)
        let edge_width = if is_selected { 2.0_f32 } else { 1.0_f32 };
        for i in 0..4 {
            painter.line_segment([projected[i], projected[(i + 1) % 4]], Stroke::new(edge_width, edge_color));
        }
        
        // Label only the selected plane: naming all seven at once buried the
        // model under overlapping text.
        if let Some(center_pos) = project(vp, rect, center).filter(|_| is_selected) {
            painter.text(
                center_pos + Vec2::new(0.0, -20.0),
                egui::Align2::CENTER_CENTER,
                &feature.name,
                egui::FontId::proportional(10.0),
                edge_color,
            );
        }
    }
}

fn draw_datum_axis(
    painter: &egui::Painter,
    rect: Rect,
    _camera: &OrbitCamera,
    vp: Mat4,
    _eye: Vec3,
    origin: Vec3,
    direction: Vec3,
    feature: &Feature,
    _idx: usize,
    is_selected: bool,
) {
    let length = 60.0; // Reduced size
    let end = origin + direction * length;
    
    if let (Some(o), Some(e)) = (project(vp, rect, origin), project(vp, rect, end)) {
        // Draw axis as a line with arrow
        let (color, line_width) = if is_selected {
            (Color32::YELLOW, 3.0_f32)
        } else {
            (Color32::from_rgb(255, 180, 80), 2.0_f32)
        };
        painter.line_segment([o, e], Stroke::new(line_width, color));
        
        // Draw arrowhead
        let arrow_len = 10.0; // Smaller arrow
        let arrow_angle = 0.5; // radians
        let dir_2d = (e - o).normalized();
        let perp = Vec2::new(-dir_2d.y, dir_2d.x);
        
        let arrow1 = e - dir_2d * arrow_len + perp * arrow_len * arrow_angle;
        let arrow2 = e - dir_2d * arrow_len - perp * arrow_len * arrow_angle;
        
        painter.line_segment([e, arrow1], Stroke::new(line_width, color));
        painter.line_segment([e, arrow2], Stroke::new(line_width, color));
        
        // Draw label at end
        painter.text(
            e + dir_2d * 8.0 + perp * 5.0,
            egui::Align2::LEFT_CENTER,
            &feature.name,
            egui::FontId::proportional(10.0),
            color,
        );
    }
}

fn draw_datum_point(
    painter: &egui::Painter,
    rect: Rect,
    _camera: &OrbitCamera,
    vp: Mat4,
    _eye: Vec3,
    origin: Vec3,
    feature: &Feature,
    _idx: usize,
    is_selected: bool,
) {
    if let Some(pos) = project(vp, rect, origin) {
        let (color, size, line_width) = if is_selected {
            (Color32::YELLOW, 10.0, 3.0_f32)
        } else {
            (Color32::from_rgb(255, 100, 200), 8.0, 2.0_f32)
        };
        
        // Draw crosshair
        painter.line_segment(
            [pos + Vec2::new(-size, 0.0), pos + Vec2::new(size, 0.0)],
            Stroke::new(line_width, color)
        );
        painter.line_segment(
            [pos + Vec2::new(0.0, -size), pos + Vec2::new(0.0, size)],
            Stroke::new(line_width, color)
        );
        
        // Draw circle
        painter.circle_stroke(pos, size, Stroke::new(line_width, color));
        
        // Draw label
        painter.text(
            pos + Vec2::new(10.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            &feature.name,
            egui::FontId::proportional(11.0),
            color,
        );
    }
}

fn draw_coord_system(
    painter: &egui::Painter,
    rect: Rect,
    _camera: &OrbitCamera,
    vp: Mat4,
    _eye: Vec3,
    origin: Vec3,
    direction: Vec3, // This is the Z direction in the feature
    feature: &Feature,
    _idx: usize,
    is_selected: bool,
) {
    let size = 40.0; // Reduced size
    
    // For coordinate system, we use the feature's direction as Z, and compute X, Y
    // The origin is the feature origin, direction is the Z axis
    let z_axis = direction;
    let x_axis = if z_axis.dot(Vec3::Z).abs() < 0.9 {
        Vec3::Z.cross(z_axis).normalize_or_zero()
    } else {
        Vec3::X.cross(z_axis).normalize_or_zero()
    };
    let y_axis = z_axis.cross(x_axis);
    
    let axes = [
        (x_axis * size, Color32::RED, "X"),
        (y_axis * size, Color32::GREEN, "Y"),
        (z_axis * size, Color32::BLUE, "Z"),
    ];
    
    if let Some(op) = project(vp, rect, origin) {
        let line_width = if is_selected { 3.0_f32 } else { 2.0_f32 };
        
        for (axis_vec, color, label) in axes {
            let end = origin + axis_vec;
            if let Some(ep) = project(vp, rect, end) {
                // Draw axis line (skip if it would be behind camera)
                painter.line_segment([op, ep], Stroke::new(line_width, color));
                
                // Arrowhead
                let arrow_len = 8.0;
                let dir_2d = (ep - op).normalized();
                let perp = Vec2::new(-dir_2d.y, dir_2d.x);
                let arrow1 = ep - dir_2d * arrow_len + perp * 4.0;
                let arrow2 = ep - dir_2d * arrow_len - perp * 4.0;
                painter.line_segment([ep, arrow1], Stroke::new(line_width, color));
                painter.line_segment([ep, arrow2], Stroke::new(line_width, color));
                
                // Axis label
                painter.text(
                    ep + dir_2d * 6.0,
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(10.0),
                    color,
                );
            }
        }
        
        // Origin label
        painter.text(
            op + Vec2::new(0.0, -25.0),
            egui::Align2::CENTER_BOTTOM,
            &feature.name,
            egui::FontId::proportional(11.0),
            if is_selected { Color32::YELLOW } else { Color32::WHITE },
        );
        
        // Draw small origin sphere/cross
        let origin_size = 4.0;
        let origin_color = if is_selected { Color32::YELLOW } else { Color32::WHITE };
        painter.line_segment(
            [op + Vec2::new(-origin_size, 0.0), op + Vec2::new(origin_size, 0.0)],
            Stroke::new(2.0_f32, origin_color)
        );
        painter.line_segment(
            [op + Vec2::new(0.0, -origin_size), op + Vec2::new(0.0, origin_size)],
            Stroke::new(2.0_f32, origin_color)
        );
    }
}

/// Pick triangle under mouse cursor
pub fn pick_triangle(
   pos: Pos2,
   rect: Rect,
   mesh: &TriangleMesh,
   camera: &OrbitCamera,
) -> Option<usize> {
   let ray = Ray3D::from_screen(pos, rect, camera);
   
   let mut closest_t = f32::INFINITY;
   let mut closest_tri = None;
   
   for (tri_idx, tri) in mesh.indices.chunks_exact(3).enumerate() {
       let i0 = tri[0] as usize;
       let i1 = tri[1] as usize;
       let i2 = tri[2] as usize;
       
       let v0 = Vec3::from(mesh.positions[i0]);
       let v1 = Vec3::from(mesh.positions[i1]);
       let v2 = Vec3::from(mesh.positions[i2]);
       
       if let Some(t) = ray.intersect_triangle(v0, v1, v2) {
           if t < closest_t {
               closest_t = t;
               closest_tri = Some(tri_idx);
           }
       }
   }
   
   closest_tri
}

/// Pick construction geometry under mouse cursor
pub fn pick_construction(
    pos: Pos2,
    rect: Rect,
    part: &Part,
    camera: &OrbitCamera,
) -> Option<usize> {
    let ray = Ray3D::from_screen(pos, rect, camera);
    
    let mut closest_t = f32::INFINITY;
    let mut closest_feature = None;
    
    for (idx, feature) in part.features.iter().enumerate() {
        if feature.suppressed || part.suppressed.contains(&feature.id) {
            continue;
        }
        if !feature.visible {
            continue;
        }
        
        if let FeatureKind::Construction { kind, origin, direction } = &feature.kind {
            let origin_v = Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32);
            let dir_v = Vec3::new(direction[0] as f32, direction[1] as f32, direction[2] as f32).normalize_or_zero();
            
            let hit = match kind {
                ConstructionKind::Plane => {
                    // Test intersection with infinite plane, then check bounds
                    if let Some(t) = ray.intersect_plane(origin_v, dir_v) {
                        let hit_point = ray.origin + ray.direction * t;
                        // Check if hit point is within the plane bounds (80x80)
                        let to_hit = hit_point - origin_v;
                        let u = if dir_v.dot(Vec3::Z).abs() < 0.9 {
                            Vec3::Z.cross(dir_v).normalize_or_zero()
                        } else {
                            Vec3::X.cross(dir_v).normalize_or_zero()
                        };
                        let v = dir_v.cross(u);
                        let u_coord = to_hit.dot(u);
                        let v_coord = to_hit.dot(v);
                        if u_coord.abs() <= 40.0 && v_coord.abs() <= 40.0 {
                            Some(t)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                ConstructionKind::Axis => {
                    // Test intersection with line (cylinder of small radius)
                    ray.intersect_line(origin_v, dir_v, 3.0)
                }
                ConstructionKind::Point => {
                    // Test intersection with sphere
                    ray.intersect_sphere(origin_v, 5.0)
                }
                ConstructionKind::CoordSystem => {
                    // Test intersection with any of the three axes
                    let z_axis = dir_v;
                    let x_axis = if z_axis.dot(Vec3::Z).abs() < 0.9 {
                        Vec3::Z.cross(z_axis).normalize_or_zero()
                    } else {
                        Vec3::X.cross(z_axis).normalize_or_zero()
                    };
                    let y_axis = z_axis.cross(x_axis);
                    
                    let axes = [x_axis, y_axis, z_axis];
                    let mut best_t = None;
                    for axis in axes {
                        if let Some(t) = ray.intersect_line(origin_v, axis, 3.0) {
                            if best_t.is_none() || t < best_t.unwrap() {
                                best_t = Some(t);
                            }
                        }
                    }
                    best_t
                }
                _ => None,
            };
            
            if let Some(t) = hit {
                if t < closest_t {
                    closest_t = t;
                    closest_feature = Some(idx);
                }
            }
        }
    }
    
    closest_feature
}
/// How far, in pixels, the cursor may be from an edge and still pick it.
pub const EDGE_PICK_TOLERANCE: f32 = 6.0;

/// Everything under the cursor, most specific first.
///
/// An edge wins over the face it lies on, and datum geometry wins over the face
/// behind it. The list is what a right-click cycles through, so the user can
/// reach the face under an edge, or the datum over a face.
pub fn pick_candidates(
    pos: Pos2,
    rect: Rect,
    mesh: &TriangleMesh,
    camera: &OrbitCamera,
    part: Option<&Part>,
) -> Vec<Selected> {
    let mut candidates = Vec::new();

    if let Some(edge) = pick_edge(pos, rect, mesh, camera) {
        candidates.push(edge);
    }

    if let Some(part) = part {
        if let Some(index) = pick_construction(pos, rect, part, camera) {
            if let Some(feature) = part.features.get(index) {
                candidates.push(Selected::Datum {
                    feature: feature.id,
                    label: feature.name.clone(),
                });
            }
        }
    }

    if let Some(triangle) = pick_triangle(pos, rect, mesh, camera) {
        if let Some(reference) = mesh.face_name(triangle) {
            candidates.push(Selected::Face {
                reference: reference.to_string(),
                label: mesh.face_label(triangle).unwrap_or(reference).to_string(),
                triangle,
            });
        }
    }

    candidates
}

/// The nearest solid edge whose polyline passes close to the cursor.
pub fn pick_edge(
    pos: Pos2,
    rect: Rect,
    mesh: &TriangleMesh,
    camera: &OrbitCamera,
) -> Option<Selected> {
    let vp = view_proj(rect, camera);
    let mut best: Option<(f32, &NamedEdge)> = None;
    for edge in &mesh.edges {
        let points: Vec<Pos2> = edge
            .points
            .iter()
            .filter_map(|point| project(vp, rect, Vec3::from(*point)))
            .collect();
        for pair in points.windows(2) {
            let distance = distance_to_segment(pos, pair[0], pair[1]);
            if distance <= EDGE_PICK_TOLERANCE && best.map_or(true, |(closest, _)| distance < closest)
            {
                best = Some((distance, edge));
            }
        }
    }
    best.map(|(_, edge)| Selected::Edge {
        reference: edge.name.clone(),
        label: if edge.label.is_empty() {
            edge.name.clone()
        } else {
            edge.label.clone()
        },
    })
}

/// Distance from a point to a screen-space segment.
fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let length_squared = ab.length_sq();
    if length_squared <= f32::EPSILON {
        return (point - a).length();
    }
    let t = ((point - a).dot(ab) / length_squared).clamp(0.0, 1.0);
    (point - (a + ab * t)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_with(id: &str, rgb: [u8; 3]) -> ProjectManifest {
        let mut manifest = ProjectManifest::default();
        manifest.surface_colors.insert(id.to_string(), rgb);
        manifest
    }

    /// A one-triangle mesh with a kernel face id and its stable name.
    fn one_triangle_mesh(kernel_face_id: u64, name: &str) -> TriangleMesh {
        let mut mesh = TriangleMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            face_ids: vec![kernel_face_id],
            ..Default::default()
        };
        mesh.face_names.insert(
            kernel_face_id,
            rcad_geom::NamedFace {
                name: name.to_string(),
                signature: name.to_string(),
                label: name.to_string(),
            },
        );
        mesh
    }

    /// Untagged faces keep the default shading: the viewport never invents a
    /// colour for a face the document did not tag.
    #[test]
    fn an_untagged_face_keeps_the_default_shading() {
        let manifest = manifest_with("fit zone", [0, 255, 0]);
        let part = Part::new_solid("P");
        let mesh = one_triangle_mesh(7, "F:plane:a1");

        assert_eq!(face_colour(mesh.face_name(0), Some(&part), &manifest), None);
        assert_eq!(face_colour(None, Some(&part), &manifest), None);
        assert_eq!(face_colour(mesh.face_name(0), None, &manifest), None);
    }

    /// Construction geometry carries no stable name, so it can never pick up
    /// the colour of a solid face.
    #[test]
    fn construction_triangles_never_match_a_face_colour() {
        let manifest = manifest_with("fit zone", [0, 255, 0]);
        let mut part = Part::new_solid("P");
        part.set_face_colour("F:plane:a1", "fit zone");

        let mut mesh = one_triangle_mesh(1, "F:plane:a1");
        mesh.face_ids = vec![rcad_geom::NO_FACE_ID];

        assert_eq!(mesh.face_name(0), None);
        assert_eq!(face_colour(mesh.face_name(0), Some(&part), &manifest), None);
    }

    #[test]
    fn a_tagged_face_resolves_to_the_project_colour() {
        let manifest = manifest_with("fit zone", [0, 255, 0]);
        let mut part = Part::new_solid("P");
        part.set_face_colour("F:plane:a1", "fit zone");
        let mesh = one_triangle_mesh(7, "F:plane:a1");

        assert_eq!(
            face_colour(mesh.face_name(0), Some(&part), &manifest),
            Some([0, 255, 0])
        );
    }

    #[test]
    fn an_id_the_project_does_not_define_falls_back_to_shading() {
        let manifest = ProjectManifest::default();
        let mut part = Part::new_solid("P");
        part.set_face_colour("F:plane:a1", "fit zone");
        let mesh = one_triangle_mesh(7, "F:plane:a1");

        assert_eq!(face_colour(mesh.face_name(0), Some(&part), &manifest), None);
    }

    /// The name is what ties a stored colour to a face: the kernel address may
    /// be different in the next process, the name is not.
    #[test]
    fn the_same_name_resolves_whatever_the_kernel_address_is() {
        let manifest = manifest_with("fit zone", [0, 255, 0]);
        let mut part = Part::new_solid("P");
        part.set_face_colour("F:plane:a1", "fit zone");

        let first = one_triangle_mesh(0x7f00_0000_0001, "F:plane:a1");
        let second = one_triangle_mesh(0x7f00_0000_9999, "F:plane:a1");

        assert_ne!(first.face_id(0), second.face_id(0));
        assert_eq!(
            face_colour(first.face_name(0), Some(&part), &manifest),
            Some([0, 255, 0])
        );
        assert_eq!(
            face_colour(second.face_name(0), Some(&part), &manifest),
            Some([0, 255, 0])
        );
    }

    /// Step E verification: editing the RGB in rcad.toml changes the display,
    /// and the component itself is not touched.
    #[test]
    fn editing_the_project_changes_the_colour_without_touching_the_part() {
        let mut part = Part::new_solid("P");
        part.set_face_colour("F:plane:a1", "fit zone");
        let mesh = one_triangle_mesh(7, "F:plane:a1");

        let before = face_colour(
            mesh.face_name(0),
            Some(&part),
            &manifest_with("fit zone", [0, 255, 0]),
        );
        let after = face_colour(
            mesh.face_name(0),
            Some(&part),
            &manifest_with("fit zone", [255, 0, 0]),
        );

        assert_eq!(before, Some([0, 255, 0]));
        assert_eq!(after, Some([255, 0, 0]));
        assert_eq!(
            part.face_colour_id("F:plane:a1"),
            Some("fit zone"),
            "the part stores the id, never the RGB value"
        );
    }

    #[test]
    fn shading_scales_the_project_colour_but_keeps_its_hue() {
        assert_eq!(shaded_colour([0, 255, 0], 1.0), Color32::from_rgb(0, 255, 0));
        assert_eq!(shaded_colour([0, 255, 0], 0.5), Color32::from_rgb(0, 128, 0));
        assert_eq!(
            shaded_colour([255, 255, 255], 1.0),
            Color32::from_rgb(255, 255, 255),
            "a full-brightness colour must not overflow"
        );
    }

    /// An edge is picked when the cursor is close to its polyline on screen,
    /// which is how the solid edges became selectable at all.
    #[test]
    fn an_edge_is_picked_from_its_polyline() {
        let mut mesh = TriangleMesh {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        mesh.edges.push(rcad_geom::NamedEdge {
            name: "E:line:test".to_string(),
            signature: "E:line:test".to_string(),
            label: "top edge".to_string(),
            points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
            ..Default::default()
        });

        let mut camera = OrbitCamera::default();
        camera.look_from(Vec3::Z);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let vp = view_proj(rect, &camera);
        let midpoint = project(vp, rect, Vec3::new(5.0, 0.0, 0.0)).expect("the edge projects");

        let picked = pick_edge(midpoint, rect, &mesh, &camera).expect("the edge should be picked");
        assert_eq!(picked.reference(), Some("E:line:test"));

        let far = Pos2::new(midpoint.x + 200.0, midpoint.y + 120.0);
        assert!(pick_edge(far, rect, &mesh, &camera).is_none());
    }

    /// The sketch basis spans the plane: unit vectors, perpendicular to each
    /// other and to the normal, the first as close to the wanted x as possible.
    #[test]
    fn the_sketch_basis_spans_the_plane() {
        let (u, v) = sketch_basis([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let u = Vec3::from(u);
        let v = Vec3::from(v);
        assert!((u.length() - 1.0).abs() < 1e-5);
        assert!((v.length() - 1.0).abs() < 1e-5);
        assert!(u.dot(v).abs() < 1e-5, "u and v must be perpendicular");
        assert!(u.dot(Vec3::Z).abs() < 1e-5, "u must lie in the plane");
        assert!(u.dot(Vec3::X) > 0.99, "u should follow the wanted x");

        // A wanted x along the normal cannot be honoured: any perpendicular
        // will do, but it must still be a basis of the plane.
        let (u, v) = sketch_basis([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        let u = Vec3::from(u);
        let v = Vec3::from(v);
        assert!((u.length() - 1.0).abs() < 1e-5);
        assert!(u.dot(Vec3::Z).abs() < 1e-5);
        assert!(u.dot(v).abs() < 1e-5);
    }

    /// The grid lies on the plane, is centred on the origin and spans the whole
    /// extent in both directions.
    #[test]
    fn the_sketch_grid_lies_on_the_plane() {
        let (u, v) = sketch_basis([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        let lines = sketch_grid_lines([10.0, 5.0, 20.0], u, v, 20.0, 5.0);

        assert_eq!(lines.len(), 2 * (2 * 4 + 1), "nine lines each way");
        for (a, b) in &lines {
            let (a, b) = (Vec3::from(*a), Vec3::from(*b));
            let midpoint = (a + b) * 0.5;
            assert!(midpoint.y == 5.0, "every line lies in the plane");
            assert!(
                ((b - a).length() - 40.0).abs() < 1e-3,
                "each line spans the whole extent"
            );
        }
    }

    /// A0 baseline: the cost of the current linear picking scan.
    ///
    /// This is a measurement, not an assertion, so it is ignored by default.
    /// Run it explicitly:
    ///   cargo test --release -p rcad-app baseline_picking_cost -- --ignored --nocapture
    #[test]
    #[ignore = "A0 baseline measurement; run with --ignored --nocapture"]
    fn baseline_picking_cost() {
        use std::time::Instant;

        let side: usize = 300; // 2 * 300 * 300 = 180_000 triangles
        let stride = side + 1;
        let mut positions = Vec::with_capacity(stride * stride);
        let mut normals = Vec::with_capacity(stride * stride);
        let mut indices = Vec::new();
        for y in 0..stride {
            for x in 0..stride {
                positions.push([x as f32 - side as f32 * 0.5, y as f32 - side as f32 * 0.5, 0.0]);
                normals.push([0.0, 0.0, 1.0]);
            }
        }
        for y in 0..side {
            for x in 0..side {
                let i = (y * stride + x) as u32;
                indices.extend([
                    i,
                    i + 1,
                    i + stride as u32 + 1,
                    i,
                    i + stride as u32 + 1,
                    i + stride as u32,
                ]);
            }
        }
        let mesh = TriangleMesh {
            positions,
            normals,
            indices,
            ..Default::default()
        };
        let mut camera = OrbitCamera {
            distance: 900.0,
            ..Default::default()
        };
        // Look at the grid from an isometric direction: along the grid plane the
        // ray would be coplanar with the triangles and never intersect them.
        camera.look_from(Vec3::new(1.0, 1.0, 1.0));
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0));
        // Off the exact grid vertex at the centre: the ray must land inside a
        // triangle so the measurement includes a real hit.
        let cursor = rect.center() + Vec2::new(3.0, 0.0);

        let runs = 200;
        let started = Instant::now();
        let mut hits = 0usize;
        for _ in 0..runs {
            if pick_triangle(cursor, rect, &mesh, &camera).is_some() {
                hits += 1;
            }
        }
        let per_pick_ms = started.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        eprintln!(
            "baseline_picking: triangles={} runs={} hits={} avg_ms={per_pick_ms:.3}",
            mesh.triangle_count(),
            runs,
            hits
        );
    }
}
