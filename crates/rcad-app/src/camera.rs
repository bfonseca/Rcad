use glam::{Mat4, Vec2, Vec3};

/// How the 3D scene is projected onto the screen.
///
/// Parametric CAD is normally viewed orthographically: parallel edges stay
/// parallel, so you can judge alignment by eye. Perspective is useful for a
/// quick sense of depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Projection {
    Perspective,
    #[default]
    Orthographic,
}

#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub projection: Projection,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 120.0,
            yaw: 0.0,
            pitch: 0.7, // ~45 degrees, looking down at XY plane (Z up)
            projection: Projection::default(),
        }
    }
}

impl OrbitCamera {
    /// Vertical field of view used by [`Self::view_proj`] and [`Self::fit_to_bounds`].
    pub const FOV_Y_DEG: f32 = 45.0;

    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        self.target + Vec3::new(cp * cy, sp, cp * sy) * self.distance
    }

    /// World up vector for the view matrix.
    ///
    /// Looking straight down a pole (the Top and Bottom standard views) makes
    /// the world up vector parallel to the view direction, which degenerates
    /// look_at: the screen orientation then depends on floating-point noise.
    /// Fall back to another world axis in that case.
    pub fn up_vector(&self) -> Vec3 {
        let forward = (self.target - self.eye()).normalize_or_zero();
        if forward.dot(Vec3::Y).abs() > 0.999 {
            Vec3::Z
        } else {
            Vec3::Y
        }
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, self.up_vector());
        let aspect = aspect.max(0.1);
        let proj = match self.projection {
            Projection::Perspective => {
                Mat4::perspective_rh(Self::FOV_Y_DEG.to_radians(), aspect, 0.1, 10_000.0)
            }
            Projection::Orthographic => {
                // Match the perspective framing at the orbit distance so the
                // two projections show the same amount of the model.
                let half_h = self.distance * (Self::FOV_Y_DEG.to_radians() * 0.5).tan();
                let half_w = half_h * aspect;
                Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, -10_000.0, 10_000.0)
            }
        };
        proj * view
    }

    /// Orbit by a pointer-motion delta, roughly 0.01 radians per point.
    ///
    /// Pitch wraps through the poles instead of stopping there. The old clamp
    /// made the view jam at straight-up / straight-down, so a continued drag
    /// did nothing and the user had to release the mouse and start again.
    /// Wrapping keeps the eye position continuous across the pole, so the user
    /// can keep spinning in any direction.
    pub fn orbit(&mut self, delta: Vec2) {
        const SENSITIVITY: f32 = 0.01;
        const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;
        const TAU: f32 = std::f32::consts::TAU;

        self.yaw = (self.yaw + delta.x * SENSITIVITY).rem_euclid(TAU);
        self.pitch += delta.y * SENSITIVITY;

        // Reflect the pitch about the pole and turn the yaw half a turn. The
        // two changes cancel out in `eye()`, so the camera does not jump.
        for _ in 0..8 {
            if self.pitch > HALF_PI {
                self.pitch = std::f32::consts::PI - self.pitch;
            } else if self.pitch < -HALF_PI {
                self.pitch = -std::f32::consts::PI - self.pitch;
            } else {
                break;
            }
            self.yaw = (self.yaw + std::f32::consts::PI).rem_euclid(TAU);
        }
        self.pitch = self.pitch.clamp(-HALF_PI, HALF_PI);
    }

    pub fn pan(&mut self, delta: Vec2) {
        let eye = self.eye();
        let forward = (self.target - eye).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        let scale = self.distance * 0.001;
        self.target += -right * delta.x * scale + up * delta.y * scale;
    }

    /// Frame a bounding box: centre the camera on it and pull back far enough
    /// for all of it to be visible.
    ///
    /// Used by View > Zoom Fit and automatically after the first rebuild - the
    /// app had no way to frame the model, so it opened at a fixed distance.
    pub fn fit_to_bounds(&mut self, min: Vec3, max: Vec3) {
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.5);
        let half_fov = Self::FOV_Y_DEG.to_radians() * 0.5;
        self.target = center;
        self.distance = (radius / half_fov.sin() * 1.3).clamp(1.0, 10_000.0);
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (1.0 - scroll * 0.1)).clamp(1.0, 10_000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Looking straight down a pole used to make the view matrix degenerate,
    /// which made the Top and Bottom standard views render at a random roll.
    #[test]
    fn up_vector_stays_usable_at_the_poles() {
        let mut camera = OrbitCamera::default();
        for pitch in [
            -std::f32::consts::FRAC_PI_2,
            -1.0,
            0.0,
            1.0,
            std::f32::consts::FRAC_PI_2,
        ] {
            camera.pitch = pitch;
            let forward = (camera.target - camera.eye()).normalize_or_zero();
            let up = camera.up_vector();
            assert!(
                forward.dot(up).abs() < 0.999,
                "up vector is parallel to the view direction at pitch {pitch}"
            );
            assert!(camera.view_proj(1.5).is_finite());
        }
    }

    /// Zoom Fit must actually fit the model on screen.
    #[test]
    fn fit_to_bounds_frames_the_whole_box() {
        let bounds = (
            Vec3::new(-5.0, 0.0, -10.0),
            Vec3::new(40.0, 30.0, 8.0),
        );
        let (min, max) = bounds;
        let corners = [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(max.x, max.y, max.z),
        ];

        for projection in [Projection::Orthographic, Projection::Perspective] {
            let mut camera = OrbitCamera {
                projection,
                ..Default::default()
            };
            camera.fit_to_bounds(min, max);
            assert_eq!(camera.target, (min + max) * 0.5);

            let vp = camera.view_proj(1.5);
            for corner in corners {
                let clip = vp * corner.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                assert!(
                    ndc.x.abs() <= 1.01 && ndc.y.abs() <= 1.01,
                    "{projection:?}: corner {corner:?} landed outside the view at {ndc:?}"
                );
            }
        }
    }
}

    /// A continued vertical drag must keep turning instead of jamming at the
    /// pole (the old code clamped pitch to +/- 90 degrees).
    #[test]
    fn orbit_keeps_turning_past_the_poles() {
        let mut camera = OrbitCamera::default();
        for _ in 0..500 {
            camera.orbit(Vec2::new(0.0, 10.0));
            assert!(camera.pitch >= -std::f32::consts::FRAC_PI_2 - 1e-5);
            assert!(camera.pitch <= std::f32::consts::FRAC_PI_2 + 1e-5);
            assert!(camera.eye().is_finite());
            assert!(camera.view_proj(1.5).is_finite());
        }
    }

    /// Crossing a pole must not teleport the camera: the reflection of the
    /// pitch and the half-turn of the yaw have to cancel out.
    #[test]
    fn crossing_a_pole_keeps_the_camera_continuous() {
        let mut camera = OrbitCamera {
            pitch: std::f32::consts::FRAC_PI_2 - 0.02,
            yaw: 0.4,
            ..Default::default()
        };
        let before = camera.eye();
        camera.orbit(Vec2::new(0.0, 3.0)); // 0.03 rad: just past the pole
        let after = camera.eye();
        let jump = (after - before).length() / camera.distance;
        assert!(jump < 0.05, "camera jumped {jump} of its distance crossing a pole");
    }

    /// Yaw must accumulate without bound so horizontal spinning never stops.
    #[test]
    fn yaw_accumulates_through_a_full_turn() {
        let mut camera = OrbitCamera::default();
        let start = camera.eye();
        let per_frame = std::f32::consts::TAU / 0.01 / 360.0; // 1 degree per frame
        for _ in 0..360 {
            camera.orbit(Vec2::new(per_frame, 0.0));
        }
        let end = camera.eye();
        assert!(
            (end - start).length() < 0.01,
            "a full turn did not return to the start: {start} vs {end}"
        );
    }
