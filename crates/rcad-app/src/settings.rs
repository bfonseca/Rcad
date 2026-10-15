//! Application settings, persisted between sessions.
//!
//! Stored as a small `key = value` file under the user config directory, so it
//! can be edited by hand as well as from the Options dialog. The format is
//! deliberately trivial: it keeps the app free of a serialisation dependency
//! and stays readable if something goes wrong.

use std::path::PathBuf;

/// Everything the user can adjust that should survive a restart.
#[derive(Debug, Clone, PartialEq)]
pub struct AppSettings {
    /// Width of the left tool palette, in points.
    pub toolbar_width: f32,
    /// Width of the model tree overlay, in points.
    pub history_width: f32,
    /// Whether the ground grid is drawn in the viewport.
    pub show_grid: bool,
    /// Distance between grid lines, in model units.
    pub grid_spacing: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            toolbar_width: 190.0,
            history_width: 260.0,
            show_grid: true,
            grid_spacing: 10.0,
        }
    }
}

/// Bounds so a hand-edited or stale file cannot make the UI unusable.
const TOOLBAR_WIDTH: (f32, f32) = (120.0, 420.0);
const HISTORY_WIDTH: (f32, f32) = (180.0, 600.0);
const GRID_SPACING: (f32, f32) = (1.0, 500.0);

impl AppSettings {
    /// Where the settings file lives: `$XDG_CONFIG_HOME/rcad/settings.conf`,
    /// falling back to `~/.config/rcad/settings.conf`.
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("rcad").join("settings.conf"))
    }

    /// Load the settings, falling back to the defaults for anything missing.
    ///
    /// A missing or unreadable file is not an error: the app starts with
    /// defaults instead.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    /// Write the settings out, creating the directory if needed.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_text())
    }

    fn clamp_all(&mut self) {
        self.toolbar_width = self.toolbar_width.clamp(TOOLBAR_WIDTH.0, TOOLBAR_WIDTH.1);
        self.history_width = self.history_width.clamp(HISTORY_WIDTH.0, HISTORY_WIDTH.1);
        self.grid_spacing = self.grid_spacing.clamp(GRID_SPACING.0, GRID_SPACING.1);
    }

    /// Parse the `key = value` format, ignoring unknown keys and junk lines.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "toolbar_width" => {
                    if let Ok(parsed) = value.parse() {
                        settings.toolbar_width = parsed;
                    }
                }
                "history_width" => {
                    if let Ok(parsed) = value.parse() {
                        settings.history_width = parsed;
                    }
                }
                "show_grid" => settings.show_grid = value.eq_ignore_ascii_case("true"),
                "grid_spacing" => {
                    if let Ok(parsed) = value.parse::<f32>() {
                        settings.grid_spacing = parsed;
                    }
                }
                _ => {}
            }
        }
        settings.clamp_all();
        settings
    }

    pub fn to_text(&self) -> String {
        format!(
            "# rcad application settings\n\
             # Edited by Tools > Options; safe to edit by hand.\n\
             toolbar_width = {:.1}\n\
             history_width = {:.1}\n\
             show_grid = {}\n\
             grid_spacing = {:.1}\n",
            self.toolbar_width, self.history_width, self.show_grid, self.grid_spacing,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_the_file_format() {
        let settings = AppSettings {
            toolbar_width: 210.0,
            history_width: 300.0,
            show_grid: false,
            grid_spacing: 25.0,
        };
        assert_eq!(AppSettings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn unknown_keys_and_junk_are_ignored() {
        let parsed = AppSettings::parse(
            "# a comment\nnonsense line\nunknown = 5\nshow_grid = false\n",
        );
        assert!(!parsed.show_grid);
        assert_eq!(parsed.toolbar_width, AppSettings::default().toolbar_width);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let parsed = AppSettings::parse("toolbar_width = 5000\ngrid_spacing = 0\n");
        assert_eq!(parsed.toolbar_width, TOOLBAR_WIDTH.1);
        assert_eq!(parsed.grid_spacing, GRID_SPACING.0);
    }
}