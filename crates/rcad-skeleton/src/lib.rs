//! Skeleton (layout) parts for top-down design.

use rcad_doc::{
    ConstructionKind, Feature, FeatureKind, Part, PartKind, Project, PublishedRef,
};
use uuid::Uuid;

/// Create a skeleton part with default datum planes / axes.
pub fn create_skeleton(project: &mut Project, name: impl Into<String>) -> Uuid {
    let mut part = Part::new_skeleton(name);
    part.features.push(Feature::new(
        "FrontPlane",
        FeatureKind::Construction {
            kind: ConstructionKind::Plane,
            origin: [0.0, 0.0, 0.0],
            direction: [0.0, 1.0, 0.0],
        },
    ));
    part.features.push(Feature::new(
        "RightPlane",
        FeatureKind::Construction {
            kind: ConstructionKind::Plane,
            origin: [0.0, 0.0, 0.0],
            direction: [1.0, 0.0, 0.0],
        },
    ));
    part.features.push(Feature::new(
        "TopPlane",
        FeatureKind::Construction {
            kind: ConstructionKind::Plane,
            origin: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
        },
    ));
    part.features.push(Feature::new(
        "ZAxis",
        FeatureKind::Construction {
            kind: ConstructionKind::Axis,
            origin: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, 1.0],
        },
    ));

    for f in &part.features {
        if let FeatureKind::Construction { kind, .. } = f.kind {
            part.published.push(PublishedRef {
                id: f.id,
                name: f.name.clone(),
                kind,
            });
        }
    }

    let id = part.id;
    project.parts.push(part);
    id
}

/// Create a solid part driven by a skeleton.
pub fn create_part_from_skeleton(
    project: &mut Project,
    name: impl Into<String>,
    skeleton_id: Uuid,
) -> Option<Uuid> {
    let skeleton = project.part(skeleton_id)?;
    if skeleton.kind != PartKind::Skeleton {
        return None;
    }
    let published = skeleton.published.clone();
    let mut part = Part::new_solid(name);
    part.skeleton_of = Some(skeleton_id);
    part.published = published;
    let id = part.id;
    project.parts.push(part);
    Some(id)
}

/// Publish an existing construction feature from a skeleton.
pub fn publish_feature(part: &mut Part, feature_id: Uuid) -> bool {
    if part.kind != PartKind::Skeleton {
        return false;
    }
    let Some(feature) = part.features.iter().find(|f| f.id == feature_id) else {
        return false;
    };
    let FeatureKind::Construction { kind, .. } = feature.kind else {
        return false;
    };
    if part.published.iter().any(|p| p.id == feature_id) {
        return true;
    }
    part.published.push(PublishedRef {
        id: feature_id,
        name: feature.name.clone(),
        kind,
    });
    true
}
