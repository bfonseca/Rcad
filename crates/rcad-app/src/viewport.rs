use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use glam::{Mat4, Vec3, Vec4};
use rcad_geom::TriangleMesh;

use crate::camera::OrbitCamera;

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
    let stroke = Stroke::new(1.0, Color32::from_rgb(50, 54, 60));
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
                painter.line_segment([op, ep], Stroke::new(2.0, color));
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
