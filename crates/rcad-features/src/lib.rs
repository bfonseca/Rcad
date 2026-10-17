//! Helpers to append common modelling features onto a [`Part`].

use rcad_doc::{EdgeSet, EntityId, Feature, FeatureKind, Part, SketchPlacement};
use rcad_geom::Profile2D;
use rcad_param::rect_profile;
use rcad_sketch::Sketch;

/// Push a feature onto the tree and return its id.
fn push_feature(part: &mut Part, feature: Feature) -> EntityId {
    let id = feature.id;
    part.features.push(feature);
    id
}

/// Append a box primitive. Returns the id of the created feature.
pub fn add_box(part: &mut Part, name: &str, size: [f64; 3]) -> EntityId {
    push_feature(part, Feature::new(name, FeatureKind::Box { size }))
}

/// Append a cylinder primitive. Returns the id of the created feature.
pub fn add_cylinder(part: &mut Part, name: &str, radius: f64, height: f64) -> EntityId {
    push_feature(
        part,
        Feature::new(name, FeatureKind::Cylinder { radius, height }),
    )
}

/// Extrude the profile of `sketch`.
///
/// Returns `None` (adding nothing) when the sketch has no usable profile,
/// instead of the previous silent no-op that made the caller believe a
/// feature had been created.
pub fn add_extrude_from_sketch(
    part: &mut Part,
    name: &str,
    sketch: &Sketch,
    height: f64,
    source: Option<EntityId>,
) -> Option<EntityId> {
    let profile = sketch.to_profile()?;
    Some(push_feature(
        part,
        Feature::new(
            name,
            FeatureKind::Extrude {
                profile,
                height,
                placement: None,
                sketch: source,
            },
        ),
    ))
}

/// Append an extrusion of a centred `width` x `depth` rectangle.
pub fn add_extrude_rect(
    part: &mut Part,
    name: &str,
    width: f64,
    depth: f64,
    height: f64,
) -> EntityId {
    let profile = rect_profile(width, depth);
    push_feature(
        part,
        Feature::new(
            name,
            FeatureKind::Extrude {
                profile,
                height,
                placement: None,
                sketch: None,
            },
        ),
    )
}

/// Append an extrusion of an arbitrary profile.
pub fn add_extrude(
    part: &mut Part,
    name: &str,
    profile: Profile2D,
    height: f64,
    placement: Option<SketchPlacement>,
    sketch: Option<EntityId>,
) -> EntityId {
    push_feature(
        part,
        Feature::new(
            name,
            FeatureKind::Extrude {
                profile,
                height,
                placement,
                sketch,
            },
        ),
    )
}

/// Generate a feature name that is unique within `part` (`Box1`, `Box2`, ...).
///
/// Without this every box in a part was called "Box", which made the feature
/// tree and the parent/child relationships unreadable.
pub fn unique_feature_name(part: &Part, base: &str) -> String {
    let mut n = 1usize;
    loop {
        let candidate = format!("{base}{n}");
        if !part.features.iter().any(|f| f.name == candidate) {
            return candidate;
        }
        n += 1;
    }
}

pub fn add_revolve(
    part: &mut Part,
    name: &str,
    profile: Profile2D,
    angle_deg: f64,
    placement: Option<SketchPlacement>,
    sketch: Option<EntityId>,
) -> EntityId {
    push_feature(
        part,
        Feature::new(
            name,
            FeatureKind::Revolve {
                profile,
                angle_deg,
                placement,
                sketch,
            },
        ),
    )
}

pub fn add_fillet(part: &mut Part, name: &str, radius: f64, edges: EdgeSet) -> EntityId {
    push_feature(
        part,
        Feature::new(name, FeatureKind::Fillet { radius, edges }),
    )
}

pub fn add_chamfer(part: &mut Part, name: &str, distance: f64, edges: EdgeSet) -> EntityId {
    push_feature(
        part,
        Feature::new(name, FeatureKind::Chamfer { distance, edges }),
    )
}

pub fn add_surface_extrude(part: &mut Part, name: &str, profile: Profile2D, height: f64) -> EntityId {
    push_feature(
        part,
        Feature::new(name, FeatureKind::ExtrudeSurface { profile, height }),
    )
}

pub fn add_sew(part: &mut Part, name: &str, tolerance: f64) -> EntityId {
    push_feature(part, Feature::new(name, FeatureKind::Sew { tolerance }))
}

pub fn add_loft(part: &mut Part, name: &str, profiles: Vec<Profile2D>, solid: bool) -> EntityId {
    push_feature(
        part,
        Feature::new(name, FeatureKind::Loft { profiles, solid }),
    )
}

pub fn add_boundary(
    part: &mut Part,
    name: &str,
    u_curves: Vec<Profile2D>,
    v_curves: Vec<Profile2D>,
) -> EntityId {
    push_feature(
        part,
        Feature::new(name, FeatureKind::Boundary { u_curves, v_curves }),
    )
}

pub fn add_thicken(
    part: &mut Part,
    name: &str,
    surface_feature: uuid::Uuid,
    distance: f64,
    both_sides: bool,
) -> EntityId {
    push_feature(
        part,
        Feature::new(
            name,
            FeatureKind::Thicken { surface_feature, distance, both_sides },
        ),
    )
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

/// Select the active body that new features attach to.
pub fn set_active_body(part: &mut Part, body_index: Option<usize>) {
    part.active_body_index = body_index;
}

/// Get the insert position for new features (Insert Here mode).
///
/// Honours `insert_here_index`, which is what the history overlay sets; the
/// previous implementation read `active_body_index` (a *body* index) and used
/// it as a *feature* index, so the marker and the insert point disagreed.
pub fn get_insert_position(part: &Part) -> usize {
    match part.insert_here_index {
        Some(i) => (i + 1).min(part.features.len()),
        None => part.features.len(),
    }
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

#[cfg(test)]
mod helper_tests {
    use super::*;
    use rcad_doc::Part;

    #[test]
    fn unique_names_do_not_collide() {
        let mut part = Part::new_solid("P");
        assert_eq!(unique_feature_name(&part, "Box"), "Box1");
        add_box(&mut part, "Box1", [1.0, 1.0, 1.0]);
        assert_eq!(unique_feature_name(&part, "Box"), "Box2");
    }

    #[test]
    fn add_helpers_return_the_new_feature_id() {
        let mut part = Part::new_solid("P");
        let id = add_box(&mut part, "Base", [1.0, 2.0, 3.0]);
        assert_eq!(part.features.len(), 1);
        assert_eq!(part.features[0].id, id);
    }

    #[test]
    fn insert_position_follows_the_insert_here_marker() {
        let mut part = Part::new_solid("P");
        add_box(&mut part, "A", [1.0, 1.0, 1.0]);
        add_box(&mut part, "B", [1.0, 1.0, 1.0]);
        assert_eq!(get_insert_position(&part), 2);
        part.insert_here_index = Some(0);
        assert_eq!(get_insert_position(&part), 1);
    }

    #[test]
    fn extrude_from_empty_sketch_reports_failure() {
        let mut part = Part::new_solid("P");
        let sketch = Sketch::new("empty");
        assert!(add_extrude_from_sketch(&mut part, "E", &sketch, 5.0, None).is_none());
        assert!(part.features.is_empty(), "a feature was added for an empty sketch");
    }

    #[test]
    fn extrude_from_rectangle_uses_the_sketch_profile() {
        let mut part = Part::new_solid("P");
        let mut sketch = Sketch::new("rect");
        sketch.add_rectangle(0.0, 0.0, 4.0, 2.0);

        let id = add_extrude_from_sketch(&mut part, "E", &sketch, 5.0, None)
            .expect("feature created");

        assert_eq!(part.features.len(), 1);
        assert_eq!(part.features[0].id, id);
        match &part.features[0].kind {
            FeatureKind::Extrude {
                profile, height, ..
            } => {
                assert_eq!(*height, 5.0);
                assert_eq!(profile.outer.len(), 4, "the sketch profile was not used");
            }
            other => panic!("expected an extrude, got {other:?}"),
        }
    }
}
