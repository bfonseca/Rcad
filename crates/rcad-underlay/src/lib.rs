//! Sketch underlays: import an image, set scale, align to a plane.

use std::path::{Path, PathBuf};

use image::GenericImageView;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum UnderlayError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("image: {0}")]
    Image(#[from] image::ImageError),
    #[error("scale undefined")]
    ScaleUndefined,
}

pub type UnderlayId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Underlay {
    pub id: UnderlayId,
    pub name: String,
    pub source_path: PathBuf,
    /// Pixel size of the source image.
    pub pixel_size: [u32; 2],
    /// Opacity 0..1.
    pub opacity: f32,
    pub locked: bool,
    /// Plane origin and normal (same convention as sketches).
    pub plane_origin: [f64; 3],
    pub plane_normal: [f64; 3],
    pub plane_x_axis: [f64; 3],
    pub plane_y_axis: [f64; 3],
    /// Two calibration points in image pixel coordinates.
    pub scale_p0: Option<[f64; 2]>,
    pub scale_p1: Option<[f64; 2]>,
    /// Real-world distance between the two points (model units).
    pub scale_distance: Option<f64>,
    /// Offset of image centre in plane UV.
    pub offset_uv: [f64; 2],
    /// Rotation of image in degrees around centre.
    pub rotation_deg: f64,
    /// Whether underlay is visible.
    pub visible: bool,
    /// Display order (lower = behind).
    pub display_order: i32,
}

impl Underlay {
    pub fn load(path: impl AsRef<Path>, name: impl Into<String>) -> Result<Self, UnderlayError> {
        let path = path.as_ref();
        let img = image::open(path)?;
        let (w, h) = img.dimensions();
        Ok(Self {
            id: Uuid::new_v4(),
            name: name.into(),
            source_path: path.to_path_buf(),
            pixel_size: [w, h],
            opacity: 0.7,
            locked: false,
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: [1.0, 0.0, 0.0],
            plane_y_axis: [0.0, 1.0, 0.0],
            scale_p0: None,
            scale_p1: None,
            scale_distance: None,
            offset_uv: [0.0, 0.0],
            rotation_deg: 0.0,
            visible: true,
            display_order: 0,
        })
    }

    pub fn set_scale_points(&mut self, p0: [f64; 2], p1: [f64; 2], real_distance: f64) {
        self.scale_p0 = Some(p0);
        self.scale_p1 = Some(p1);
        self.scale_distance = Some(real_distance);
    }

    /// Set the underlay plane (origin, normal, x_axis).
    pub fn set_plane(&mut self, origin: [f64; 3], normal: [f64; 3], x_axis: [f64; 3]) {
        use glam::DVec3;
        let n = DVec3::from(normal).normalize();
        let x = DVec3::from(x_axis);
        let y = n.cross(x).normalize();
        self.plane_origin = origin;
        self.plane_normal = n.into();
        self.plane_x_axis = x.into();
        self.plane_y_axis = y.into();
    }

    /// Align underlay to a sketch plane.
    pub fn align_to_sketch_plane(&mut self, origin: [f64; 3], normal: [f64; 3], x_axis: [f64; 3]) {
        self.set_plane(origin, normal, x_axis);
        // Center the image on the plane origin
        self.offset_uv = [0.0, 0.0];
    }

    /// Set rotation in degrees.
    pub fn set_rotation(&mut self, degrees: f64) {
        self.rotation_deg = degrees;
    }

    /// Set visibility.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Set display order (for layering multiple underlays).
    pub fn set_display_order(&mut self, order: i32) {
        self.display_order = order;
    }

    /// Convert a 3D point on the plane to image pixel coordinates.
    pub fn uv_to_pixel(&self, uv: [f64; 2]) -> Result<[f64; 2], UnderlayError> {
        let upp = self.units_per_pixel()?;
        let cx = self.pixel_size[0] as f64 * 0.5;
        let cy = self.pixel_size[1] as f64 * 0.5;
        let px = (uv[0] - self.offset_uv[0]) / upp + cx;
        let py = cy - (uv[1] - self.offset_uv[1]) / upp;
        Ok([px, py])
    }

    /// Convert 3D world point to image pixel (if on plane).
    pub fn world_to_pixel(&self, point_3d: [f64; 3]) -> Result<[f64; 2], UnderlayError> {
        use glam::DVec3;
        let p = DVec3::from(point_3d) - DVec3::from(self.plane_origin);
        let x_axis = DVec3::from(self.plane_x_axis);
        let y_axis = DVec3::from(self.plane_y_axis);
        let uv = [p.dot(x_axis), p.dot(y_axis)];
        self.uv_to_pixel(uv)
    }

    /// Snap a world point to nearest feature on underlay (for tracing).
    /// Returns the snapped UV coordinate and the pixel distance.
    pub fn snap_to_feature(&self, point_3d: [f64; 3], snap_threshold_px: f64) -> Result<Option<[f64; 2]>, UnderlayError> {
        // Convert world point to pixel
        let pixel = self.world_to_pixel(point_3d)?;
        // For now, just return the pixel if within threshold of image bounds
        let w = self.pixel_size[0] as f64;
        let h = self.pixel_size[1] as f64;
        if pixel[0] >= -snap_threshold_px && pixel[0] <= w + snap_threshold_px &&
           pixel[1] >= -snap_threshold_px && pixel[1] <= h + snap_threshold_px {
            let uv = self.pixel_to_uv(pixel[0], pixel[1])?;
            Ok(Some(uv))
        } else {
            Ok(None)
        }
    }

    /// Model units per image pixel.
    pub fn units_per_pixel(&self) -> Result<f64, UnderlayError> {
        let (p0, p1, dist) = match (self.scale_p0, self.scale_p1, self.scale_distance) {
            (Some(a), Some(b), Some(d)) if d > 0.0 => (a, b, d),
            _ => return Err(UnderlayError::ScaleUndefined),
        };
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let pixel_dist = (dx * dx + dy * dy).sqrt().max(1e-9);
        Ok(dist / pixel_dist)
    }

    /// Width / height of the underlay in model units.
    pub fn size_model(&self) -> Result<[f64; 2], UnderlayError> {
        let upp = self.units_per_pixel()?;
        Ok([
            self.pixel_size[0] as f64 * upp,
            self.pixel_size[1] as f64 * upp,
        ])
    }

    /// Map a pixel coordinate to plane UV (centred).
    pub fn pixel_to_uv(&self, px: f64, py: f64) -> Result<[f64; 2], UnderlayError> {
        let upp = self.units_per_pixel()?;
        let cx = self.pixel_size[0] as f64 * 0.5;
        let cy = self.pixel_size[1] as f64 * 0.5;
        Ok([
            (px - cx) * upp + self.offset_uv[0],
            (cy - py) * upp + self.offset_uv[1],
        ])
    }

    pub fn load_rgba8(&self) -> Result<image::RgbaImage, UnderlayError> {
        Ok(image::open(&self.source_path)?.to_rgba8())
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UnderlayStore {
    pub items: Vec<Underlay>,
}

impl UnderlayStore {
    pub fn add(&mut self, underlay: Underlay) -> UnderlayId {
        let id = underlay.id;
        self.items.push(underlay);
        id
    }

    pub fn get(&self, id: UnderlayId) -> Option<&Underlay> {
        self.items.iter().find(|u| u.id == id)
    }

    pub fn get_mut(&mut self, id: UnderlayId) -> Option<&mut Underlay> {
        self.items.iter_mut().find(|u| u.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_math() {
        let mut u = Underlay {
            id: Uuid::new_v4(),
            name: "t".into(),
            source_path: PathBuf::from("x.png"),
            pixel_size: [200, 100],
            opacity: 1.0,
            locked: false,
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: [1.0, 0.0, 0.0],
            plane_y_axis: [0.0, 1.0, 0.0],
            scale_p0: None,
            scale_p1: None,
            scale_distance: None,
            offset_uv: [0.0, 0.0],
            rotation_deg: 0.0,
            visible: true,
            display_order: 0,
        };
        u.set_scale_points([0.0, 0.0], [100.0, 0.0], 50.0);
        assert!((u.units_per_pixel().unwrap() - 0.5).abs() < 1e-9);
        let size = u.size_model().unwrap();
        assert!((size[0] - 100.0).abs() < 1e-9);
    }
}
