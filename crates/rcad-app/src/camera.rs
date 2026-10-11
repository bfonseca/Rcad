use glam::{Mat4, Vec2, Vec3};

#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 120.0,
            yaw: 0.7,
            pitch: 0.5,
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        self.target + Vec3::new(cp * cy, sp, cp * sy) * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(45f32.to_radians(), aspect.max(0.1), 0.1, 10_000.0);
        proj * view
    }

    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw += delta.x * 0.01;
        self.pitch = (self.pitch + delta.y * 0.01).clamp(-1.5, 1.5);
    }

    pub fn pan(&mut self, delta: Vec2) {
        let eye = self.eye();
        let forward = (self.target - eye).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        let scale = self.distance * 0.001;
        self.target += -right * delta.x * scale + up * delta.y * scale;
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (1.0 - scroll * 0.1)).clamp(1.0, 10_000.0);
    }
}
