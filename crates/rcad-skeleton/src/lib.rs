//! Skeleton (layout) parts for top-down design.
//!
//! The skeleton module provides top-down design capabilities where a skeleton part
//! contains reference geometry (planes, axes, points, curves) that can be published
//! and consumed by child parts. This implements the Creo-style Publish/Copy Geometry workflow.

use rcad_doc::{
    ConstructionKind, ExternalRefStatus, Feature, FeatureKind, Part, PartKind, Project, PublishType, PublishedRef,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Skeleton creation options
#[derive(Debug, Clone)]
pub struct SkeletonOptions {
    /// Create standard datum planes (Front, Right, Top)
    pub create_standard_planes: bool,
    /// Create standard axes (X, Y, Z)
    pub create_standard_axes: bool,
    /// Create center point at origin
    pub create_center_point: bool,
    /// Auto-publish all created construction geometry
    pub auto_publish: bool,
}

impl Default for SkeletonOptions {
    fn default() -> Self {
        Self {
            create_standard_planes: true,
            create_standard_axes: true,
            create_center_point: true,
            auto_publish: true,
        }
    }
}

/// Create a skeleton part with configurable options.
pub fn create_skeleton(project: &mut Project, name: impl Into<String>, options: SkeletonOptions) -> Uuid {
    let mut part = Part::new_skeleton(name);
    
    if options.create_center_point {
        let point_id = Uuid::new_v4();
        part.features.push(Feature::new(
            "CenterPoint",
            FeatureKind::Construction {
                kind: ConstructionKind::Point,
                origin: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, 0.0],
            },
        ));
        // Update the point's ID to our pre-generated one
        if let Some(f) = part.features.last_mut() {
            f.id = point_id;
        }
    }
    
    if options.create_standard_planes {
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
    }
    
    if options.create_standard_axes {
        part.features.push(Feature::new(
            "XAxis",
            FeatureKind::Construction {
                kind: ConstructionKind::Axis,
                origin: [0.0, 0.0, 0.0],
                direction: [1.0, 0.0, 0.0],
            },
        ));
        part.features.push(Feature::new(
            "YAxis",
            FeatureKind::Construction {
                kind: ConstructionKind::Axis,
                origin: [0.0, 0.0, 0.0],
                direction: [0.0, 1.0, 0.0],
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
    }

    if options.auto_publish {
        for f in &part.features {
            if let FeatureKind::Construction { kind, .. } = f.kind {
                part.published.push(PublishedRef {
                    id: f.id,
                    name: f.name.clone(),
                    kind,
                    description: None,
                    feature_id: f.id,
                    body_id: None,
                    selection_query: String::new(),
                    publish_type: PublishType::Geometry,
                    include_children: false,
                    status: ExternalRefStatus::UpToDate,
                });
            }
        }
    }

    let id = part.id;
    project.parts.push(part);
    id
}

/// Create a solid part driven by a skeleton (Copy Geometry workflow).
pub fn create_part_from_skeleton(
    project: &mut Project,
    name: impl Into<String>,
    skeleton_id: Uuid,
    copy_options: CopyGeometryOptions,
) -> Option<Uuid> {
    let skeleton = project.part(skeleton_id)?;
    if skeleton.kind != PartKind::Skeleton {
        return None;
    }
    
    let mut part = Part::new_solid(name);
    part.skeleton_of = Some(skeleton_id);
    
    // Copy selected published geometry
    for published in &skeleton.published {
        if copy_options.should_copy(&published.kind, &published.name) {
            // Create a construction feature in the child part that references the skeleton
            let ref_feature = Feature::new(
                format!("REF_{}", published.name),
                FeatureKind::Construction {
                    kind: published.kind,
                    origin: [0.0, 0.0, 0.0], // Will be resolved from skeleton
                    direction: [0.0, 0.0, 1.0],
                },
            );
            // Store the reference to the skeleton's published geometry
            part.published.push(PublishedRef {
                id: ref_feature.id,
                name: format!("REF_{}", published.name),
                kind: published.kind,
                description: None,
                feature_id: published.feature_id,
                body_id: published.body_id,
                selection_query: published.selection_query.clone(),
                publish_type: published.publish_type,
                include_children: false,
                status: ExternalRefStatus::UpToDate,
            });
            part.features.push(ref_feature);
        }
    }
    
    let id = part.id;
    project.parts.push(part);
    Some(id)
}

/// Options for Copy Geometry operation
#[derive(Debug, Clone, Default)]
pub struct CopyGeometryOptions {
    pub copy_planes: bool,
    pub copy_axes: bool,
    pub copy_points: bool,
    pub copy_curves: bool,
    pub copy_solids: bool,
    pub specific_names: Option<Vec<String>>, // If specified, only copy these named items
}

impl CopyGeometryOptions {
    pub fn all() -> Self {
        Self {
            copy_planes: true,
            copy_axes: true,
            copy_points: true,
            copy_curves: true,
            copy_solids: true,
            specific_names: None,
        }
    }
    
    pub fn should_copy(&self, kind: &ConstructionKind, name: &str) -> bool {
       if let Some(names) = &self.specific_names {
           return names.iter().any(|n| n == name);
       }
       match kind {
           ConstructionKind::Plane => self.copy_planes,
           ConstructionKind::Axis => self.copy_axes,
           ConstructionKind::Point => self.copy_points,
           ConstructionKind::Curve => self.copy_curves,
           ConstructionKind::Solid => self.copy_solids,
           ConstructionKind::CoordSystem => self.copy_planes, // Treat coordinate system as plane-like
       }
   }
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
        description: None,
        feature_id,
        body_id: None,
        selection_query: String::new(),
        publish_type: crate::PublishType::Geometry,
        include_children: false,
        status: crate::ExternalRefStatus::UpToDate,
    });
    true
}

/// Unpublish a previously published feature.
pub fn unpublish_feature(part: &mut Part, published_id: Uuid) -> bool {
    if part.kind != PartKind::Skeleton {
        return false;
    }
    let len_before = part.published.len();
    part.published.retain(|p| p.id != published_id);
    part.published.len() < len_before
}

/// Update skeleton and propagate changes to child parts.
pub fn update_skeleton_and_propagate(
    project: &mut Project,
    skeleton_id: Uuid,
    update_fn: impl FnOnce(&mut Part),
) -> Vec<Uuid> {
    // First, collect the published refs from the skeleton after update
    let published_refs: Vec<PublishedRef> = {
        // Update the skeleton
        let Some(skeleton) = project.part_mut(skeleton_id) else {
            return Vec::new();
        };
        if skeleton.kind != PartKind::Skeleton {
            return Vec::new();
        }
        
        update_fn(skeleton);
        
        // Re-publish all construction features
        skeleton.published.clear();
        for f in &skeleton.features {
            if let FeatureKind::Construction { kind, .. } = f.kind {
                skeleton.published.push(PublishedRef {
                    id: f.id,
                    name: f.name.clone(),
                    kind,
                    description: None,
                    feature_id: f.id,
                    body_id: None,
                    selection_query: String::new(),
                    publish_type: PublishType::Geometry,
                    include_children: false,
                    status: ExternalRefStatus::UpToDate,
                });
            }
        }
        
        // Clone the published refs for use with child parts
        skeleton.published.clone()
    };
    
    // Collect child part IDs
    let child_ids: Vec<Uuid> = project.parts
        .iter()
        .filter(|p| p.skeleton_of == Some(skeleton_id))
        .map(|p| p.id)
        .collect();
    
    // Find and update child parts
    let mut updated_children = Vec::new();
    for child_id in child_ids {
        if let Some(part) = project.part_mut(child_id) {
            // Update copied references
            part.published.clear();
            for published in &published_refs {
                let ref_feature = Feature::new(
                    format!("REF_{}", published.name),
                    FeatureKind::Construction {
                        kind: published.kind,
                        origin: [0.0, 0.0, 0.0],
                        direction: [0.0, 0.0, 1.0],
                    },
                );
                part.published.push(PublishedRef {
                    id: ref_feature.id,
                    name: format!("REF_{}", published.name),
                    kind: published.kind,
                    description: None,
                    feature_id: published.feature_id,
                    body_id: published.body_id,
                    selection_query: published.selection_query.clone(),
                    publish_type: published.publish_type,
                    include_children: false,
                    status: ExternalRefStatus::UpToDate,
                });
                part.features.push(ref_feature);
            }
            updated_children.push(part.id);
        }
    }
    
    updated_children
}

// ExternalRefStatus is now defined in rcad-doc and imported above

/// Check external reference status for a child part
pub fn check_external_refs(project: &Project, part_id: Uuid) -> Vec<(Uuid, ExternalRefStatus)> {
    let Some(part) = project.part(part_id) else {
        return Vec::new();
    };
    
    let Some(skeleton_id) = part.skeleton_of else {
        return Vec::new();
    };
    
    let Some(skeleton) = project.part(skeleton_id) else {
        return part.published.iter()
            .map(|p| (p.id, ExternalRefStatus::Broken))
            .collect();
    };
    
    part.published.iter()
        .map(|p| {
            let status = if skeleton.published.iter().any(|sp| sp.id == p.id) {
                ExternalRefStatus::UpToDate
            } else {
                ExternalRefStatus::Broken
            };
            (p.id, status)
        })
        .collect()
}

/// Create a skeleton part with default datum planes / axes (legacy API).
pub fn create_skeleton_legacy(project: &mut Project, name: impl Into<String>) -> Uuid {
    create_skeleton(project, name, SkeletonOptions::default())
}

/// Create a solid part driven by a skeleton (legacy API).
pub fn create_part_from_skeleton_legacy(
    project: &mut Project,
    name: impl Into<String>,
    skeleton_id: Uuid,
) -> Option<Uuid> {
    create_part_from_skeleton(project, name, skeleton_id, CopyGeometryOptions::all())
}
