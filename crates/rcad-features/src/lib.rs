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
