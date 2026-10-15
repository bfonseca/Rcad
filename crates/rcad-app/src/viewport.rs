use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use glam::{Mat4, Vec3, Vec4};
use rcad_doc::{Feature, FeatureKind, ConstructionKind, Part};
use rcad_geom::TriangleMesh;

use crate::camera::OrbitCamera;

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

/// Selection state
#[derive(Debug, Default, Clone)]
pub struct SelectionState {
    pub selected_triangle: Option<usize>,
    pub selected_edge: Option<usize>,
    pub selected_vertex: Option<usize>,
    pub selected_construction: Option<usize>, // Feature index for construction geometry
    pub hover_triangle: Option<usize>,
    pub hover_edge: Option<usize>,
    pub hover_vertex: Option<usize>,
    pub hover_construction: Option<usize>,
    pub pick_mode: PickMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickMode {
    #[default]
    Object,
    Face,
    Edge,
    Vertex,
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
     selected_triangle: Option<usize>,
     selected_construction: Option<usize>,
     part: Option<&Part>,
    show_grid: bool,
    grid_spacing: f32,
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
        draw_mesh(&painter, rect, mesh, camera, selected_triangle);
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No solid — add a feature",
            egui::FontId::proportional(16.0),
            Color32::GRAY,
        );
    }

    // Draw construction geometry from part features
    if let Some(part) = part {
        draw_construction_geometry(&painter, rect, camera, part, selected_construction);
    }

    // Draw corner CYS (Coordinate System) in bottom-left
    draw_corner_cys(&painter, rect, camera);

    response
}

fn view_proj(rect: Rect, camera: &OrbitCamera) -> Mat4 {
    let aspect = rect.width() / rect.height().max(1.0);
    camera.view_proj(aspect)
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
fn draw_axes(painter: &egui::Painter, rect: Rect, camera: &OrbitCamera) {
    let vp = view_proj(rect, camera);
    let o = Vec3::ZERO;
    let axes = [
        (Vec3::X * 40.0, Color32::RED),
        (Vec3::Y * 40.0, Color32::GREEN),
        (Vec3::Z * 40.0, Color32::BLUE),
    ];
    if let Some(op) = project(vp, rect, o) {
        for (end, color) in axes {
            if let Some(ep) = project(vp, rect, end) {
                painter.line_segment([op, ep], Stroke::new(2.0_f32, color));
            }
        }
    }
}

// Draw coordinate system triad in bottom-left corner (CYS)
fn draw_corner_cys(painter: &egui::Painter, rect: Rect, camera: &OrbitCamera) {
    // Fixed position in bottom-left corner (screen space)
    let corner_origin = Pos2::new(60.0, rect.height() - 60.0);
    let size = 40.0; // Small size for corner CYS
    
    // Get camera orientation to show world axes correctly
    // We need to project world axes endpoints
    let vp = view_proj(rect, camera);
    
    // World axes endpoints
    let world_axes = [
        (Vec3::X * size, Color32::RED, "X"),
        (Vec3::Y * size, Color32::GREEN, "Y"),
        (Vec3::Z * size, Color32::BLUE, "Z"),
    ];
    
    // Project world axes from origin
    if let Some(op) = project(vp, rect, Vec3::ZERO) {
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

fn draw_mesh(painter: &egui::Painter, rect: Rect, mesh: &TriangleMesh, camera: &OrbitCamera, selected_triangle: Option<usize>) {
     let vp = view_proj(rect, camera);
     let eye = camera.eye();
     let mut tris: Vec<(f32, [Pos2; 3], Color32)> = Vec::with_capacity(mesh.triangle_count());

     for tri in mesh.indices.chunks_exact(3) {
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
        let shade = (n.dot(light).clamp(0.3, 1.0) * 220.0) as u8;
         let mut color = Color32::from_rgb(shade, shade.saturating_add(10), shade.saturating_add(30));
         
         // Highlight selected triangle
         if let Some(selected) = selected_triangle {
             let triangle_index = tri.as_ptr() as usize / std::mem::size_of::<[u32; 3]>();
             if triangle_index == selected {
                 color = Color32::YELLOW;
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

fn draw_construction_geometry(painter: &egui::Painter, rect: Rect, camera: &OrbitCamera, part: &Part, selected_construction: Option<usize>) {
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
            if let Some(center_pos) = project(vp, rect, origin_v) {
                // Use camera distance as depth
                let depth = (origin_v - eye).length();
                construction_items.push((depth, idx, feature, *kind, origin_v, dir_v));
            }
        }
    }
    
    // Sort by depth (back to front for proper transparency)
    construction_items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    
    // Draw in sorted order
    for (_, idx, feature, kind, origin_v, dir_v) in construction_items {
        let is_selected = selected_construction == Some(idx);
        
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

/// Handle picking interaction in viewport
pub fn handle_picking(
   ui: &mut Ui,
   rect: Rect,
   mesh: &TriangleMesh,
   camera: &OrbitCamera,
   selection: &mut SelectionState,
   part: &Part,
) {
    if ui.rect_contains_pointer(rect) {
        let pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
        
        // Update hover for triangles (solid geometry)
        if let Some(tri) = pick_triangle(pos, rect, mesh, camera) {
            selection.hover_triangle = Some(tri);
        } else {
            selection.hover_triangle = None;
        }
        
        // Update hover for construction geometry
        if let Some(feat_idx) = pick_construction(pos, rect, part, camera) {
            selection.hover_construction = Some(feat_idx);
        } else {
            selection.hover_construction = None;
        }
        
        // Handle click for selection
        if ui.input(|i| i.pointer.primary_clicked()) {
            // Try construction geometry first (it's on top visually)
            if let Some(feat_idx) = pick_construction(pos, rect, part, camera) {
                selection.selected_construction = Some(feat_idx);
                selection.selected_triangle = None;
            } else if let Some(tri) = pick_triangle(pos, rect, mesh, camera) {
                selection.selected_triangle = Some(tri);
                selection.selected_construction = None;
            }
            // Note: Don't clear selection on empty click - let the app handle deselection logic
            // to preserve history tree selection
        }
    } else {
        selection.hover_triangle = None;
        selection.hover_construction = None;
    }
}
