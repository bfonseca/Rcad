use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

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

/// An orbiting camera with no poles.
///
/// The orientation is stored as a rotation rather than yaw/pitch. A yaw/pitch
/// camera has two singularities at straight-up and straight-down: the view
/// direction becomes parallel to the up vector, so the screen roll becomes
/// undefined. Dragging through them either jams the rotation (clamping) or
/// flips the scene half a turn (wrapping). With a rotation there is no such
/// configuration, so dragging stays fluid in every direction.
#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    /// Rotation from camera space to world space: the camera looks along its
    /// local -Z, with local +Y as screen-up.
    pub orientation: Quat,
    pub projection: Projection,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 120.0,
            // New documents open looking along -X, so the screen shows Z up and
            // Y to the right.
            orientation: orientation_from_offset(Vec3::X),
            projection: Projection::default(),
        }
    }
}

/// Build an orientation whose local +Z (target to eye) points along `offset`.
fn orientation_from_offset(offset: Vec3) -> Quat {
    let back = offset.normalize_or_zero();
    if back.length_squared() < 0.5 {
        return Quat::IDENTITY;
    }
    // Pick an up hint that is not parallel to `back`, so the basis is stable.
    let up_hint = if back.dot(OrbitCamera::WORLD_UP).abs() > 0.99 {
        Vec3::Y
    } else {
        OrbitCamera::WORLD_UP
    };
    let right = up_hint.cross(back).normalize_or_zero();
    let up = back.cross(right);
    Quat::from_mat3(&Mat3::from_cols(right, up, back))
}

impl OrbitCamera {
    /// Vertical field of view used by view_proj and fit_to_bounds.
    pub const FOV_Y_DEG: f32 = 45.0;

    /// The world up axis. rcad is a Z-up CAD application: modeling planes and
    /// the datum planes are defined with Z pointing up the screen.
    pub const WORLD_UP: Vec3 = Vec3::Z;

    pub fn eye(&self) -> Vec3 {
        self.target + self.orientation * (Vec3::Z * self.distance)
    }

    /// Unit vector from the eye towards the target.
#[cfg(test)]
    pub fn forward(&self) -> Vec3 {
        -(self.orientation * Vec3::Z)
    }

    /// Screen-up direction. Always perpendicular to the view direction,
    /// because it is a rotation of the camera own Y axis.
    pub fn up_vector(&self) -> Vec3 {
        self.orientation * Vec3::Y
    }

    /// Point the camera at the target from `offset` (target to eye).
    pub fn look_from(&mut self, offset: Vec3) {
        self.orientation = orientation_from_offset(offset);
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
    /// Horizontal motion spins about the world up axis, so the horizon stays
    /// level. Vertical motion tumbles about the camera own right axis, so it
    /// never reaches a limit and never flips the scene.
    pub fn orbit(&mut self, delta: Vec2) {
        const SENSITIVITY: f32 = 0.01;

        let horizontal = Quat::from_axis_angle(Self::WORLD_UP, -delta.x * SENSITIVITY);
        let vertical = Quat::from_axis_angle(Vec3::X, -delta.y * SENSITIVITY);
        self.orientation = (horizontal * self.orientation * vertical).normalize();
    }

    pub fn pan(&mut self, delta: Vec2) {
        // Use the camera own axes: deriving them from the world up vector
        // breaks down when the camera looks straight up or down.
        let right = self.orientation * Vec3::X;
        let up = self.orientation * Vec3::Y;
        let scale = self.distance * 0.001;
        self.target += -right * delta.x * scale + up * delta.y * scale;
    }

    /// Frame a bounding box: centre the camera on it and pull back far enough
    /// for all of it to be visible.
    ///
    /// Used by View > Zoom Fit and automatically after the first rebuild.
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

    fn assert_camera_is_sane(camera: &OrbitCamera) {
        let up = camera.up_vector();
        let forward = camera.forward();
        assert!((up.length() - 1.0).abs() < 1e-3, "up vector is not unit length");
        assert!(
            forward.dot(up).abs() < 1e-3,
            "up vector is not perpendicular to the view direction"
        );
        assert!(camera.eye().is_finite());
        assert!(camera.view_proj(1.5).is_finite());
    }

    /// Every standard view must be well defined, including straight up and
    /// straight down, which a yaw/pitch camera cannot represent.
    #[test]
    fn standard_views_are_well_defined() {
        let directions = [
            Vec3::X,
            -Vec3::X,
            Vec3::Y,
            -Vec3::Y,
            Vec3::Z,
            -Vec3::Z,
        ];
        for direction in directions {
            let mut camera = OrbitCamera::default();
            camera.look_from(direction);
            assert_camera_is_sane(&camera);
            let expected = direction.normalize();
            let actual = (camera.eye() - camera.target).normalize();
            assert!(
                (actual - expected).length() < 1e-3,
                "look_from({direction:?}) put the eye at {actual:?}"
            );
        }
    }

    /// The bug the user hit: dragging up or down must move the camera by the
    /// same amount every step, with no jump when passing straight up.
    #[test]
    fn vertical_drag_is_smooth_all_the_way_over_the_top() {
        let mut camera = OrbitCamera::default();
        let mut previous = camera.eye();
        let mut steps = Vec::new();

        // More than a half turn, so it passes through straight up and beyond.
        for _ in 0..300 {
            camera.orbit(Vec2::new(0.0, 2.0));
            let eye = camera.eye();
            steps.push((eye - previous).length());
            previous = eye;
            assert_camera_is_sane(&camera);
        }

        let smallest = steps.iter().cloned().fold(f32::INFINITY, f32::min);
        let largest = steps.iter().cloned().fold(0.0_f32, f32::max);
        assert!(
            largest / smallest < 1.05,
            "vertical drag is uneven: steps range from {smallest} to {largest}"
        );
    }

    /// A full horizontal turn must come back to where it started.
    #[test]
    fn a_full_turn_returns_to_the_start() {
        let mut camera = OrbitCamera::default();
        let start_forward = camera.forward();
        let per_step = std::f32::consts::TAU / 0.01 / 720.0; // half a degree
        for _ in 0..720 {
            camera.orbit(Vec2::new(per_step, 0.0));
        }
        let end_forward = camera.forward();
        assert!(
            (end_forward - start_forward).length() < 1e-2,
            "a full turn drifted: {start_forward:?} -> {end_forward:?}"
        );
    }

    /// Zoom Fit must actually fit the model on screen.
    #[test]
    fn fit_to_bounds_frames_the_whole_box() {
        let bounds = (Vec3::new(-5.0, 0.0, -10.0), Vec3::new(40.0, 30.0, 8.0));
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
                    "{projection:?}: corner {corner:?} landed outside the view"
                );
            }
        }
    }

    /// Panning must work even when the camera looks straight down.
    #[test]
    fn pan_works_at_the_poles() {
        let mut camera = OrbitCamera::default();
        camera.look_from(Vec3::Z);
        let before = camera.target;
        camera.pan(Vec2::new(50.0, 50.0));
        assert!(camera.target.is_finite());
        assert!((camera.target - before).length() > 1e-6, "pan did nothing");
    }
}