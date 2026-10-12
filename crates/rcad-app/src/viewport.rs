use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use glam::{Mat4, Vec3, Vec4};
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
}

/// Pickable entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickTarget {
    Triangle(usize),
    Edge(usize),
    Vertex(usize),
    None,
}

/// Selection state
#[derive(Debug, Default, Clone)]
pub struct SelectionState {
    pub selected_triangle: Option<usize>,
    pub selected_edge: Option<usize>,
    pub selected_vertex: Option<usize>,
    pub hover_triangle: Option<usize>,
    pub hover_edge: Option<usize>,
    pub hover_vertex: Option<usize>,
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

pub fn draw_viewport(
     ui: &mut Ui,
     mesh: &TriangleMesh,
     camera: &mut OrbitCamera,
     underlay_tint: Option<Color32>,
     selected_triangle: Option<usize>,
 ) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());

    if response.dragged_by(egui::PointerButton::Primary) {
        let d = response.drag_delta();
        camera.orbit(glam::Vec2::new(d.x, d.y));
    }
    if response.dragged_by(egui::PointerButton::Middle)
        || response.dragged_by(egui::PointerButton::Secondary)
    {
        let d = response.drag_delta();
        camera.pan(glam::Vec2::new(d.x, d.y));
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            camera.zoom(scroll / 50.0);
        }
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 30, 34));

    // Ground grid
    draw_grid(&painter, rect, camera);

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

    // Axes gizmo
    draw_axes(&painter, rect, camera);

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

fn draw_grid(painter: &egui::Painter, rect: Rect, camera: &OrbitCamera) {
    let vp = view_proj(rect, camera);
    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(50, 54, 60));
    for i in -10..=10 {
        let t = i as f32 * 10.0;
        if let (Some(a), Some(b)) = (
            project(vp, rect, Vec3::new(t, 0.0, -100.0)),
            project(vp, rect, Vec3::new(t, 0.0, 100.0)),
        ) {
            painter.line_segment([a, b], stroke);
        }
        if let (Some(a), Some(b)) = (
            project(vp, rect, Vec3::new(-100.0, 0.0, t)),
            project(vp, rect, Vec3::new(100.0, 0.0, t)),
        ) {
            painter.line_segment([a, b], stroke);
        }
    }
}

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
         let shade = (n.dot(light).clamp(0.15, 1.0) * 220.0) as u8;
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

/// Handle picking interaction in viewport
pub fn handle_picking(
   ui: &mut Ui,
   rect: Rect,
   mesh: &TriangleMesh,
   camera: &OrbitCamera,
   selection: &mut SelectionState,
) {
   if ui.rect_contains_pointer(rect) {
       let pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
       
       // Update hover
       if let Some(tri) = pick_triangle(pos, rect, mesh, camera) {
           selection.hover_triangle = Some(tri);
       } else {
           selection.hover_triangle = None;
       }
       
       // Handle click for selection
       if ui.input(|i| i.pointer.primary_clicked()) {
           if let Some(tri) = pick_triangle(pos, rect, mesh, camera) {
               selection.selected_triangle = Some(tri);
           } else {
               selection.selected_triangle = None;
           }
       }
   } else {
       selection.hover_triangle = None;
   }
}
