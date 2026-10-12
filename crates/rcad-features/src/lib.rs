//! Helpers to append common modelling features onto a [`Part`].

use rcad_doc::{Feature, FeatureKind, Part};
use rcad_geom::Profile2D;
use rcad_param::rect_profile;
use rcad_sketch::Sketch;

pub fn add_box(part: &mut Part, name: &str, size: [f64; 3]) {
    part.features
        .push(Feature::new(name, FeatureKind::Box { size }));
}

pub fn add_cylinder(part: &mut Part, name: &str, radius: f64, height: f64) {
    part.features.push(Feature::new(
        name,
        FeatureKind::Cylinder { radius, height },
    ));
}

pub fn add_extrude_from_sketch(part: &mut Part, name: &str, sketch: &Sketch, height: f64) {
    if let Some(profile) = sketch.to_profile() {
        part.features.push(Feature::new(
            name,
            FeatureKind::Extrude { profile, height },
        ));
    }
}

pub fn add_extrude_rect(part: &mut Part, name: &str, width: f64, depth: f64, height: f64) {
    let profile = rect_profile(width, depth);
    part.features.push(Feature::new(
        name,
        FeatureKind::Extrude { profile, height },
    ));
}

pub fn add_revolve(part: &mut Part, name: &str, profile: Profile2D, angle_deg: f64) {
    part.features.push(Feature::new(
        name,
        FeatureKind::Revolve { profile, angle_deg },
    ));
}

pub fn add_fillet(part: &mut Part, name: &str, radius: f64) {
    part.features
        .push(Feature::new(name, FeatureKind::Fillet { radius }));
}

pub fn add_chamfer(part: &mut Part, name: &str, distance: f64) {
    part.features
        .push(Feature::new(name, FeatureKind::Chamfer { distance }));
}

pub fn add_surface_extrude(part: &mut Part, name: &str, profile: Profile2D, height: f64) {
    part.features.push(Feature::new(
        name,
        FeatureKind::ExtrudeSurface { profile, height },
    ));
}

pub fn add_sew(part: &mut Part, name: &str, tolerance: f64) {
    part.features
        .push(Feature::new(name, FeatureKind::Sew { tolerance }));
}

pub fn add_loft(part: &mut Part, name: &str, profiles: Vec<Profile2D>, solid: bool) {
    part.features.push(Feature::new(
        name,
        FeatureKind::Loft { profiles, solid },
    ));
}

pub fn add_boundary(part: &mut Part, name: &str, u_curves: Vec<Profile2D>, v_curves: Vec<Profile2D>) {
    part.features.push(Feature::new(
        name,
        FeatureKind::Boundary { u_curves, v_curves },
    ));
}

pub fn add_thicken(part: &mut Part, name: &str, surface_feature: uuid::Uuid, distance: f64, both_sides: bool) {
    part.features.push(Feature::new(
        name,
        FeatureKind::Thicken { surface_feature, distance, both_sides },
    ));
}

/// Feature tree UX helpers

/// Move a feature to a new position in the tree (reorder)
pub fn move_feature(part: &mut Part, feature_id: uuid::Uuid, new_index: usize) -> bool {
    if let Some(current_index) = part.features.iter().position(|f| f.id == feature_id) {
        if current_index == new_index {
            return true;
        }
        let feature = part.features.remove(current_index);
        let insert_index = new_index.min(part.features.len());
        part.features.insert(insert_index, feature);
        true
    } else {
        false
    }
}

/// Set the active body index for Insert Here mode
pub fn set_active_body(part: &mut Part, body_index: Option<usize>) {
    part.active_body_index = body_index;
}

/// Get the insert position for new features (Insert Here mode)
pub fn get_insert_position(part: &Part) -> usize {
    part.active_body_index.map(|i| i + 1).unwrap_or(part.features.len())
}

/// Edit a feature's parameters (returns the feature for editing)
pub fn edit_feature<'a>(part: &'a mut Part, feature_id: uuid::Uuid) -> Option<&'a mut Feature> {
    part.features.iter_mut().find(|f| f.id == feature_id)
}

/// Insert a feature at a specific position (Insert Here mode)
pub fn insert_feature_at(part: &mut Part, index: usize, feature: Feature) {
    let insert_index = index.min(part.features.len());
    part.features.insert(insert_index, feature);
}

pub fn suppress(part: &mut Part, feature_id: uuid::Uuid) {
    if let Some(f) = part.features.iter_mut().find(|f| f.id == feature_id) {
        f.suppressed = true;
    }
}

pub fn unsuppress(part: &mut Part, feature_id: uuid::Uuid) {
    if let Some(f) = part.features.iter_mut().find(|f| f.id == feature_id) {
        f.suppressed = false;
    }
    part.suppressed.retain(|id| *id != feature_id);
}
