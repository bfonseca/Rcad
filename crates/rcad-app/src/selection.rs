//! One ordered selection, shared by the viewport and the model tree.
//!
//! Creo keeps a single selection list: the order matters (a feature reads the
//! first item as its placement and the rest as orientation), Ctrl adds and
//! removes, and every entry is a reference that a feature can store. This is
//! that list. Entries carry the *stable* reference (face name, edge name,
//! feature id), never an index, so a selection can go straight into
//! `FeatureReferences` and survive a parameter change.

use uuid::Uuid;

/// One selected thing, with the reference a feature would store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selected {
    /// A face of the solid, named by its stable reference.
    Face {
        reference: String,
        label: String,
        /// Triangle that was picked, for highlighting. Not part of identity.
        triangle: usize,
    },
    /// An edge of the solid, named by its stable reference.
    Edge { reference: String, label: String },
    /// Datum geometry: a plane, axis, point or coordinate system.
    Datum { feature: Uuid, label: String },
    /// A feature node in the model tree.
    Feature { feature: Uuid, label: String },
}

impl Selected {
    /// The reference a feature would store, when this item has one.
    pub fn reference(&self) -> Option<&str> {
        match self {
            Selected::Face { reference, .. } | Selected::Edge { reference, .. } => Some(reference),
            _ => None,
        }
    }

    /// The feature this item refers to, when it is a datum or a tree node.
    pub fn feature(&self) -> Option<Uuid> {
        match self {
            Selected::Datum { feature, .. } | Selected::Feature { feature, .. } => Some(*feature),
            _ => None,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Selected::Face { label, .. }
            | Selected::Edge { label, .. }
            | Selected::Datum { label, .. }
            | Selected::Feature { label, .. } => label,
        }
    }

    /// A word for the interface: "face", "edge", "plane", "feature".
    pub fn kind(&self) -> &'static str {
        match self {
            Selected::Face { .. } => "face",
            Selected::Edge { .. } => "edge",
            Selected::Datum { .. } => "datum",
            Selected::Feature { .. } => "feature",
        }
    }

    /// Identity, ignoring the triangle a face happened to be picked through:
    /// the same face picked after a rebuild is the same item.
    pub fn same(&self, other: &Selected) -> bool {
        match (self, other) {
            (Selected::Face { reference: a, .. }, Selected::Face { reference: b, .. }) => a == b,
            (Selected::Edge { reference: a, .. }, Selected::Edge { reference: b, .. }) => a == b,
            (Selected::Datum { feature: a, .. }, Selected::Datum { feature: b, .. }) => a == b,
            (Selected::Feature { feature: a, .. }, Selected::Feature { feature: b, .. }) => a == b,
            _ => false,
        }
    }
}

/// How a click should combine with the selection, the way a CAD user expects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClickMode {
    /// Ctrl (or Cmd): add when absent, remove when present.
    pub toggle: bool,
    /// Shift: add, never remove.
    pub extend: bool,
}

impl ClickMode {
    /// A plain click, for tests and for callers with no modifiers to read.
    #[cfg(test)]
    pub const PLAIN: Self = Self {
        toggle: false,
        extend: false,
    };

    pub fn from_modifiers(modifiers: &egui::Modifiers) -> Self {
        Self {
            toggle: modifiers.command || modifiers.ctrl,
            extend: modifiers.shift,
        }
    }
}

/// The selection list. Order is the order things were added.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    items: Vec<Selected>,
}

impl Selection {
    pub fn items(&self) -> &[Selected] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The last item added: the one a command treats as its seed.
    pub fn primary(&self) -> Option<&Selected> {
        self.items.last()
    }

    pub fn contains(&self, item: &Selected) -> bool {
        self.items.iter().any(|selected| selected.same(item))
    }

    /// Replace the whole selection with one item.
    pub fn replace(&mut self, item: Selected) {
        self.items.clear();
        self.items.push(item);
    }

    /// Add the item when it is absent, remove it when present.
    /// Returns true when it was added.
    pub fn toggle(&mut self, item: Selected) -> bool {
        if let Some(index) = self.items.iter().position(|selected| selected.same(&item)) {
            self.items.remove(index);
            false
        } else {
            self.items.push(item);
            true
        }
    }

    /// Add the item unless it is already there, keeping the order.
    pub fn extend(&mut self, item: Selected) {
        if !self.contains(&item) {
            self.items.push(item);
        }
    }

    /// Apply a click with modifiers. Returns true when the item is now selected.
    pub fn click(&mut self, item: Selected, mode: ClickMode) -> bool {
        if mode.toggle {
            self.toggle(item)
        } else if mode.extend {
            self.extend(item);
            true
        } else {
            self.replace(item);
            true
        }
    }

    pub fn remove(&mut self, item: &Selected) -> bool {
        match self.items.iter().position(|selected| selected.same(item)) {
            Some(index) => {
                self.items.remove(index);
                true
            }
            None => false,
        }
    }

    /// Face references in selection order. The sketch placement collector is
    /// the first caller; until it lands this is exercised by the tests.
    #[allow(dead_code)]
    pub fn face_references(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|item| match item {
                Selected::Face { reference, .. } => Some(reference.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Edge references in selection order, for a fillet or chamfer to store.
    #[allow(dead_code)]
    pub fn edge_references(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|item| match item {
                Selected::Edge { reference, .. } => Some(reference.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn feature_ids(&self) -> Vec<Uuid> {
        self.items.iter().filter_map(Selected::feature).collect()
    }

    /// Triangle of the primary face, for the face-colour control.
    pub fn primary_face_triangle(&self) -> Option<usize> {
        self.items.iter().rev().find_map(|item| match item {
            Selected::Face { triangle, .. } => Some(*triangle),
            _ => None,
        })
    }

    pub fn has_face(&self, reference: &str) -> bool {
        self.items.iter().any(|item| matches!(item, Selected::Face { reference: r, .. } if r == reference))
    }

    pub fn has_edge(&self, reference: &str) -> bool {
        self.items.iter().any(|item| matches!(item, Selected::Edge { reference: r, .. } if r == reference))
    }

    pub fn has_feature(&self, feature: Uuid) -> bool {
        self.feature_ids().contains(&feature)
    }

    /// A one-line description for the selection bar: "face Box1 top + edge ...".
    pub fn summary(&self) -> String {
        self.items
            .iter()
            .map(|item| format!("{} {}", item.kind(), item.label()))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(reference: &str) -> Selected {
        Selected::Face {
            reference: reference.to_string(),
            label: reference.to_string(),
            triangle: 0,
        }
    }

    fn plane(id: Uuid) -> Selected {
        Selected::Datum {
            feature: id,
            label: "RIGHT_PLANE".to_string(),
        }
    }

    #[test]
    fn a_plain_click_replaces_the_selection() {
        let mut selection = Selection::default();
        selection.click(face("A"), ClickMode::PLAIN);
        selection.click(face("B"), ClickMode::PLAIN);

        assert_eq!(selection.len(), 1);
        assert_eq!(selection.primary().unwrap().reference(), Some("B"));
    }

    /// The order matters: a command reads the first item as placement and the
    /// next as orientation, so Ctrl must append, not sort.
    #[test]
    fn ctrl_click_appends_in_order_and_removes_on_repeat() {
        let id = Uuid::new_v4();
        let mut selection = Selection::default();
        selection.click(face("TOP"), ClickMode::PLAIN);
        selection.click(plane(id), ClickMode { toggle: true, ..Default::default() });

        assert_eq!(selection.len(), 2);
        assert_eq!(selection.items()[0].reference(), Some("TOP"));
        assert_eq!(selection.items()[1].feature(), Some(id));

        selection.click(plane(id), ClickMode { toggle: true, ..Default::default() });
        assert_eq!(selection.len(), 1);
        assert_eq!(selection.items()[0].reference(), Some("TOP"));
    }

    #[test]
    fn shift_adds_without_removing() {
        let mut selection = Selection::default();
        let mode = ClickMode { toggle: false, extend: true };
        selection.click(face("A"), mode);
        selection.click(face("A"), mode);

        assert_eq!(selection.len(), 1);
    }

    /// The same face picked through a different triangle is the same item: a
    /// rebuild changes the triangle, not the face.
    #[test]
    fn identity_ignores_the_triangle() {
        let mut selection = Selection::default();
        selection.replace(Selected::Face {
            reference: "F:x".to_string(),
            label: "top".to_string(),
            triangle: 3,
        });
        let picked_again = Selected::Face {
            reference: "F:x".to_string(),
            label: "top".to_string(),
            triangle: 99,
        };

        assert!(selection.contains(&picked_again));
        selection.toggle(picked_again);
        assert!(selection.is_empty());
    }

    #[test]
    fn references_keep_the_selection_order() {
        let id = Uuid::new_v4();
        let mut selection = Selection::default();
        selection.click(Selected::Edge { reference: "e1".to_string(), label: "edge 1".to_string() }, ClickMode::PLAIN);
        selection.extend(Selected::Edge { reference: "e2".to_string(), label: "edge 2".to_string() });
        selection.extend(plane(id));

        assert_eq!(selection.edge_references(), vec!["e1", "e2"]);
        assert_eq!(selection.feature_ids(), vec![id]);
        assert_eq!(selection.summary(), "edge edge 1 + edge edge 2 + datum RIGHT_PLANE");
    }
}
