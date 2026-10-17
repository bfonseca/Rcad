//! The project manifest, `rcad.toml`.
//!
//! A project is a set of general definitions shared by every component it
//! contains, plus the folder those components live in. The manifest holds the
//! definitions: tolerances, global variables, project functions and the colour
//! values used by that project.
//!
//! The colour *ids* are the exception: the vocabulary of names belongs to the
//! installation (see `AppSettings::colour_ids`), while the RGB values belong to
//! the project. Components store the id, never the colour, so editing a value
//! here updates every component that uses it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name of the manifest inside a project folder.
pub const MANIFEST_FILE: &str = "rcad.toml";

/// The whole `rcad.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectManifest {
    #[serde(default)]
    pub project: ProjectInfo,
    #[serde(default)]
    pub general_tolerance: Tolerances,
    /// Variables callable from any component or feature of the project.
    #[serde(default)]
    pub global_parameters: BTreeMap<String, f64>,
    /// Named formulas callable from relations, e.g. `wall(d) = d / 10`.
    /// The body is an expression in the same language as relations.
    #[serde(default)]
    pub functions: BTreeMap<String, String>,
    /// Logical colour id -> RGB, owned by the project.
    #[serde(default)]
    pub surface_colors: BTreeMap<String, [u8; 3]>,
    #[serde(default)]
    pub external_tables: ExternalTables,
    #[serde(default)]
    pub startup: Startup,
    /// Display tessellation quality (foundation A5).
    #[serde(default)]
    pub tessellation: DisplayTessellation,
}

fn default_project_name() -> String {
    ProjectInfo::default().name
}

fn default_project_version() -> String {
    ProjectInfo::default().version
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Every field is optional so a hand-written minimal file still loads.
    #[serde(default = "default_project_name")]
    pub name: String,
    #[serde(default = "default_project_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: String,
}

impl Default for ProjectInfo {
    fn default() -> Self {
        Self {
            name: "Untitled project".to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            authors: Vec::new(),
            license: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Tolerances {
    /// Linear tolerance in millimetres.
    pub linear: f64,
    /// Angular tolerance in degrees.
    pub angular: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            linear: 0.01,
            angular: 0.1,
        }
    }
}

/// How finely B-Rep surfaces are tessellated for display.
///
/// This is a *display* setting: it changes the triangle mesh, never the
/// topology names stored in the document. Geometric signatures use a fixed
/// canonical tessellation so changing this value cannot break references.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DisplayTessellation {
    /// Linear deflection. When `relative` is true it is a fraction of the
    /// feature size; otherwise it is an absolute distance in millimetres.
    pub linear: f64,
    /// Angular deflection in degrees: the largest angle between two adjacent
    /// facets. Smaller values make curved surfaces smoother.
    pub angular_deg: f64,
    /// Interpret `linear` relative to the local feature size.
    pub relative: bool,
}

impl Default for DisplayTessellation {
    fn default() -> Self {
        // Finer than cadrum's default (relative 0.004, 0.5 rad ≈ 28.6°):
        // curved faces are the point of the viewport work, and the extra
        // triangles are cheap compared with the rebuild cost measured in A0.
        Self {
            linear: 0.002,
            angular_deg: 11.5,
            relative: true,
        }
    }
}

impl DisplayTessellation {
    pub fn angular_radians(self) -> f64 {
        self.angular_deg.to_radians()
    }

    /// Clamp hand-edited or stale values to a range that cannot freeze the
    /// rebuild or produce an empty mesh.
    ///
    /// A relative linear deflection is a small fraction of the feature size; an
    /// absolute one is a distance in millimetres and therefore has a wider range.
    pub fn clamped(mut self) -> Self {
        self.linear = if self.relative {
            self.linear.clamp(0.0001, 0.05)
        } else {
            self.linear.clamp(0.0001, 5.0)
        };
        self.angular_deg = self.angular_deg.clamp(2.0, 40.0);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalTables {
    pub default_path: String,
    pub default_format: String,
    pub cache_in_memory: bool,
}

impl Default for ExternalTables {
    fn default() -> Self {
        Self {
            default_path: "tables".to_string(),
            default_format: "csv".to_string(),
            cache_in_memory: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Startup {
    pub auto_open_last: bool,
    pub max_recent_projects: usize,
}

impl Default for Startup {
    fn default() -> Self {
        Self {
            auto_open_last: true,
            max_recent_projects: 10,
        }
    }
}

impl ProjectManifest {
    /// A manifest for a brand new project folder.
    pub fn create_default(name: &str) -> Self {
        let mut manifest = Self::default();
        manifest.project.name = name.to_string();
        // A couple of examples so the file explains itself on first open.
        manifest.global_parameters.insert("wall_thickness".to_string(), 2.0);
        manifest
            .functions
            .insert("outer_diameter".to_string(), "wall_thickness * 2 + bore".to_string());
        manifest
    }

    /// Parse a manifest from text.
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|err| err.to_string())
    }

    /// Serialise the manifest.
    pub fn to_text(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|err| err.to_string())
    }

    /// Path of the manifest inside a project folder.
    pub fn path_in(root: &Path) -> PathBuf {
        root.join(MANIFEST_FILE)
    }

    /// Load the manifest from a project folder.
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = Self::path_in(root);
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("could not read {}: {err}", path.display()))?;
        Self::parse(&text).map_err(|err| format!("could not parse {}: {err}", path.display()))
    }

    /// Write the manifest into a project folder, creating the folder if needed.
    pub fn save(&self, root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root).map_err(|err| err.to_string())?;
        let path = Self::path_in(root);
        let text = self.to_text()?;
        std::fs::write(&path, text).map_err(|err| format!("could not write {}: {err}", path.display()))
    }

    /// Colour ids used by the project but not defined in the installation
    /// vocabulary. The UI offers to add them.
    pub fn colours_missing_from(&self, vocabulary: &[String]) -> Vec<String> {
        self.surface_colors
            .keys()
            .filter(|id| !vocabulary.iter().any(|known| known == *id))
            .cloned()
            .collect()
    }

    /// Look up a colour value by logical id.
    pub fn colour(&self, id: &str) -> Option<[u8; 3]> {
        self.surface_colors.get(id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_round_trips_through_toml() {
        let mut manifest = ProjectManifest::create_default("Cliente A");
        manifest.surface_colors.insert("fit_zone".to_string(), [0, 255, 0]);
        manifest.general_tolerance.linear = 0.005;

        let text = manifest.to_text().expect("serialise");
        let parsed = ProjectManifest::parse(&text).expect("parse");

        assert_eq!(parsed.project.name, "Cliente A");
        assert_eq!(parsed.colour("fit_zone"), Some([0, 255, 0]));
        assert_eq!(parsed.general_tolerance.linear, 0.005);
        assert_eq!(parsed.functions.get("outer_diameter").map(String::as_str), Some("wall_thickness * 2 + bore"));
    }

    /// A minimal file must still load: every section is optional.
    #[test]
    fn a_minimal_manifest_falls_back_to_defaults() {
        let parsed = ProjectManifest::parse("[project]\nname = \"Tiny\"\n").expect("parse");
        assert_eq!(parsed.project.name, "Tiny");
        assert_eq!(parsed.general_tolerance.linear, Tolerances::default().linear);
        assert_eq!(parsed.tessellation.angular_deg, DisplayTessellation::default().angular_deg);
        assert!(parsed.surface_colors.is_empty());
    }

    #[test]
    fn display_tessellation_round_trips_and_is_clamped() {
        let mut manifest = ProjectManifest::create_default("P");
        manifest.tessellation = DisplayTessellation {
            linear: 0.001,
            angular_deg: 8.0,
            relative: false,
        };
        let parsed = ProjectManifest::parse(&manifest.to_text().expect("serialise")).expect("parse");
        assert!((parsed.tessellation.linear - 0.001).abs() < 1e-12);
        assert!((parsed.tessellation.angular_deg - 8.0).abs() < 1e-12);
        assert!(!parsed.tessellation.relative);

        let clamped = DisplayTessellation {
            linear: 0.0,
            angular_deg: 500.0,
            relative: true,
        }
        .clamped();
        assert!(clamped.linear > 0.0);
        assert!(clamped.angular_deg <= 40.0);

        // Absolute millimetres keep their wider range.
        let absolute = DisplayTessellation {
            linear: 1.0,
            angular_deg: 12.0,
            relative: false,
        }
        .clamped();
        assert!((absolute.linear - 1.0).abs() < 1e-12);
    }

    #[test]
    fn broken_toml_reports_an_error_instead_of_panicking() {
        assert!(ProjectManifest::parse("this is not toml").is_err());
    }

    /// The vocabulary lives in the app settings, the values in the project, so
    /// the UI has to be able to spot ids the installation does not know about.
    #[test]
    fn colours_unknown_to_the_vocabulary_are_reported() {
        let mut manifest = ProjectManifest::create_default("P");
        manifest.surface_colors.insert("fit_zone".to_string(), [1, 2, 3]);
        manifest.surface_colors.insert("thread".to_string(), [4, 5, 6]);

        let unknown = manifest.colours_missing_from(&["fit_zone".to_string()]);

        assert_eq!(unknown, vec!["thread".to_string()]);
    }
}