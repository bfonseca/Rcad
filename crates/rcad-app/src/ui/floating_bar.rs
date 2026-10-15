//! Floating quick-action bar, centred at the top of the viewport.
//!
//! Creo-style: a small strip of icons that stays out of the way. Groups such
//! as the datum visibility controls open a small menu when the button is held
//! down, and perform the common action when it is clicked.

use eframe::egui;
use rcad_doc::{ConstructionKind, FeatureKind, Part};

use super::toolbar::{IconTextures, IconTheme, ToolbarAction};

/// How long the button must be held to open the group menu instead of acting.
const LONG_PRESS_SECONDS: f64 = 0.35;

/// A family of datum geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumKind {
    CoordSystem,
    Point,
    Axis,
    Plane,
}

impl DatumKind {
    pub const ALL: [DatumKind; 4] = [
        DatumKind::CoordSystem,
        DatumKind::Point,
        DatumKind::Axis,
        DatumKind::Plane,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DatumKind::CoordSystem => "Coordinate systems",
            DatumKind::Point => "Points",
            DatumKind::Axis => "Axes",
            DatumKind::Plane => "Planes",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            DatumKind::CoordSystem => "datum-datum-csys",
            DatumKind::Point => "datum-datum-point",
            DatumKind::Axis => "datum-datum-axis",
            DatumKind::Plane => "datum-datum-plane",
        }
    }

    /// Does this family contain the given construction kind?
    pub fn matches_construction(self, kind: ConstructionKind) -> bool {
        matches!(
            (self, kind),
            (DatumKind::CoordSystem, ConstructionKind::CoordSystem)
                | (DatumKind::Point, ConstructionKind::Point)
                | (DatumKind::Axis, ConstructionKind::Axis)
                | (DatumKind::Plane, ConstructionKind::Plane)
        )
    }
}

/// What the bar asks the application to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloatingAction {
    /// `None` means every datum kind, `Some` means just that one.
    ToggleDatums(Option<DatumKind>),
    Toolbar(ToolbarAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Datums,
    Analysis,
}

/// The floating bar state.
#[derive(Debug, Clone)]
pub struct FloatingBar {
    pub visible: bool,
    icon_textures: IconTextures,
    /// Which group button is held down, and when the press started.
    press: Option<(Group, f64)>,
    /// Which group menu is open, and where to anchor it.
    open: Option<(Group, egui::Rect)>,
}

impl Default for FloatingBar {
    fn default() -> Self {
        Self {
            visible: true,
            icon_textures: IconTextures::default(),
            press: None,
            open: None,
        }
    }
}

impl FloatingBar {
    /// Width of the bar, so the caller can centre it.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        part: Option<&Part>,
        sketching: bool,
        on_action: &mut dyn FnMut(FloatingAction),
    ) {
        if !self.visible {
            return;
        }

        let theme = if ui.visuals().dark_mode {
            IconTheme::Dark
        } else {
            IconTheme::Light
        };

        let frame = egui::Frame::popup(ui.style())
            .inner_margin(egui::Margin::symmetric(6, 4));

        egui::Area::new(egui::Id::new("rcad_floating_bar"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;

                            // Datum visibility: click toggles everything, hold
                            // opens the per-kind list.
                            self.group_button(
                                ui,
                                Group::Datums,
                                "datum-datum-plane",
                                theme,
                                on_action,
                            );

                            // Analysis tools, same interaction.
                            self.group_button(
                                ui,
                                Group::Analysis,
                                "general-measure",
                                theme,
                                on_action,
                            );

                            ui.separator();

                            // A few view shortcuts that are worth one click.
                            self.icon_button(ui, "view-zoom-fit", "Zoom fit", theme, on_action, || {
                                FloatingAction::Toolbar(ToolbarAction::ViewZoomFit)
                            });
                            self.icon_button(ui, "view-view-isometric", "Isometric", theme, on_action, || {
                                FloatingAction::Toolbar(ToolbarAction::ViewIso)
                            });
                            self.icon_button(ui, "view-view-front", "Front", theme, on_action, || {
                                FloatingAction::Toolbar(ToolbarAction::ViewFront)
                            });
                            self.icon_button(ui, "view-view-top", "Top", theme, on_action, || {
                                FloatingAction::Toolbar(ToolbarAction::ViewTop)
                            });
                            self.icon_button(ui, "view-display-shaded", "Shaded", theme, on_action, || {
                                FloatingAction::Toolbar(ToolbarAction::ViewShaded)
                            });
                            self.icon_button(ui, "view-display-wireframe", "Wireframe", theme, on_action, || {
                                FloatingAction::Toolbar(ToolbarAction::ViewWireframe)
                            });
                        });

                        // Second row: only while a sketch is being edited.
                        if sketching {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for (icon, tooltip, action) in [
                                    ("sketch-line", "Line (L)", ToolbarAction::SketchLine),
                                    ("sketch-rectangle", "Rectangle (R)", ToolbarAction::SketchRectangle),
                                    ("sketch-circle-center", "Circle", ToolbarAction::SketchCircle),
                                    ("sketch-arc-3point", "Arc", ToolbarAction::SketchArc),
                                    ("sketch-spline", "Spline", ToolbarAction::SketchSpline),
                                    ("sketch-point", "Point (P)", ToolbarAction::SketchPoint),
                                    ("sketch-trim", "Trim", ToolbarAction::SketchTrim),
                                    ("sketch-offset-edge", "Offset", ToolbarAction::SketchOffset),
                                    ("sketch-mirror-sketch", "Mirror", ToolbarAction::SketchMirror),
                                    ("constraints-horizontal", "Horizontal", ToolbarAction::SketchConstraintHorizontal),
                                    ("constraints-vertical", "Vertical", ToolbarAction::SketchConstraintVertical),
                                    ("sketch-dimension", "Dimension", ToolbarAction::SketchDimension),
                                ] {
                                    self.icon_button(ui, icon, tooltip, theme, on_action, || {
                                        FloatingAction::Toolbar(action.clone())
                                    });
                                }
                            });
                        }
                    });
                });
            });

        self.show_open_menu(ui, part, on_action);
    }

    /// Click = act, hold = open the group menu.
    fn group_button(
        &mut self,
        ui: &mut egui::Ui,
        group: Group,
        icon: &str,
        theme: IconTheme,
        on_action: &mut dyn FnMut(FloatingAction),
    ) {
        let (tooltip, short_action) = match group {
            Group::Datums => (
                "Click: show/hide all datums. Hold: choose CYS, points, axes or planes.",
                FloatingAction::ToggleDatums(None),
            ),
            Group::Analysis => (
                "Click: measure. Hold: choose an analysis tool.",
                FloatingAction::Toolbar(ToolbarAction::AnalysisMeasure),
            ),
        };

        let response = self.icon_response(ui, icon, tooltip, theme);

        if response.is_pointer_button_down_on() {
            if self.press.map(|(g, _)| g) != Some(group) {
                self.press = Some((group, ui.input(|i| i.time)));
            }
            // Keep frames coming so the hold can be timed.
            ui.ctx().request_repaint();
        } else if let Some((pressed_group, started)) = self.press.take() {
            if pressed_group == group {
                let held = ui.input(|i| i.time) - started;
                if held >= LONG_PRESS_SECONDS {
                    self.open = Some((group, response.rect));
                }
            }
        }

        if response.clicked() {
            // A click only arrives when it was not a hold.
            on_action(short_action);
        }
    }

    fn icon_button(
        &mut self,
        ui: &mut egui::Ui,
        icon: &str,
        tooltip: &str,
        theme: IconTheme,
        on_action: &mut dyn FnMut(FloatingAction),
        make: impl FnOnce() -> FloatingAction,
    ) {
        let response = self.icon_response(ui, icon, tooltip, theme);
        if response.clicked() {
            on_action(make());
        }
    }

    fn icon_response(
        &mut self,
        ui: &mut egui::Ui,
        icon: &str,
        tooltip: &str,
        theme: IconTheme,
    ) -> egui::Response {
        let texture = self.icon_textures.get(ui.ctx(), icon, theme);
        let button = match texture {
            Some(texture) => egui::Button::image(
                egui::Image::from_texture(&texture).fit_to_exact_size(egui::vec2(20.0, 20.0)),
            ),
            None => egui::Button::new(egui::RichText::new("•").size(14.0)),
        };
        ui.add(button.min_size(egui::vec2(26.0, 26.0)).corner_radius(4.0))
            .on_hover_text(tooltip)
    }

    /// The open group menu, drawn under its button.
    fn show_open_menu(
        &mut self,
        ui: &mut egui::Ui,
        part: Option<&Part>,
        on_action: &mut dyn FnMut(FloatingAction),
    ) {
        let Some((group, anchor)) = self.open else {
            return;
        };

        let mut close = false;
        let mut chosen: Option<FloatingAction> = None;

        egui::Area::new(egui::Id::new("rcad_floating_menu"))
            .fixed_pos(egui::pos2(anchor.center().x - 90.0, anchor.bottom() + 4.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    match group {
                        Group::Datums => {
                            ui.label(egui::RichText::new("Show datums").strong());
                            for kind in DatumKind::ALL {
                                let mut shown = datums_shown(part, kind);
                                if ui.checkbox(&mut shown, kind.label()).clicked() {
                                    chosen = Some(FloatingAction::ToggleDatums(Some(kind)));
                                }
                            }
                            ui.separator();
                            if ui.button("Show all").clicked() {
                                chosen = Some(FloatingAction::ToggleDatums(None));
                            }
                        }
                        Group::Analysis => {
                            ui.label(egui::RichText::new("Analysis").strong());
                            for (label, action) in [
                                ("Measure", ToolbarAction::AnalysisMeasure),
                                ("Mass properties", ToolbarAction::AnalysisMassProperties),
                                ("Curvature", ToolbarAction::AnalysisCurvature),
                                ("Draft check", ToolbarAction::AnalysisDraft),
                                ("Thickness", ToolbarAction::AnalysisThickness),
                                ("Interference", ToolbarAction::AnalysisInterference),
                                ("Clearance", ToolbarAction::AnalysisClearance),
                            ] {
                                if ui.button(label).clicked() {
                                    chosen = Some(FloatingAction::Toolbar(action));
                                }
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        // Clicking anywhere else dismisses the menu.
        if ui.input(|i| i.pointer.any_click()) && chosen.is_none() {
            close = true;
        }

        if let Some(action) = chosen {
            on_action(action);
            close = true;
        }
        if close {
            self.open = None;
        }
    }
}

/// Is any datum of this kind currently visible?
pub fn datums_shown(part: Option<&Part>, kind: DatumKind) -> bool {
    let Some(part) = part else {
        return false;
    };
    part.features.iter().any(|feature| {
        feature.visible
            && matches!(&feature.kind, FeatureKind::Construction { kind: c, .. } if kind.matches_construction(*c))
    })
}