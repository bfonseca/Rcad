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
    /// Two calibration points in image pixel coordinates.
    pub scale_p0: Option<[f64; 2]>,
    pub scale_p1: Option<[f64; 2]>,
    /// Real-world distance between the two points (model units).
    pub scale_distance: Option<f64>,
    /// Offset of image centre in plane UV.
    pub offset_uv: [f64; 2],
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
            scale_p0: None,
            scale_p1: None,
            scale_distance: None,
            offset_uv: [0.0, 0.0],
        })
    }

    pub fn set_scale_points(&mut self, p0: [f64; 2], p1: [f64; 2], real_distance: f64) {
        self.scale_p0 = Some(p0);
        self.scale_p1 = Some(p1);
        self.scale_distance = Some(real_distance);
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
            scale_p0: None,
            scale_p1: None,
            scale_distance: None,
            offset_uv: [0.0, 0.0],
        };
        u.set_scale_points([0.0, 0.0], [100.0, 0.0], 50.0);
        assert!((u.units_per_pixel().unwrap() - 0.5).abs() < 1e-9);
        let size = u.size_model().unwrap();
        assert!((size[0] - 100.0).abs() < 1e-9);
    }
}
