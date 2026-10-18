use eframe::egui;

use crate::sketch_ui::SketchTool;

/// Keyboard shortcuts configuration
#[derive(Debug, Clone)]
#[allow(dead_code)] // picker keys are reserved for selection modes
pub struct Shortcuts {
    // Viewport navigation
    pub reset_view: egui::Key,
    pub view_front: egui::Key,
    pub view_back: egui::Key,
    pub view_left: egui::Key,
    pub view_right: egui::Key,
    pub view_top: egui::Key,
    pub view_bottom: egui::Key,
    pub view_iso: egui::Key,
    
    // File operations
    pub new: egui::Key,
    pub open: egui::Key,
    pub save: egui::Key,
    pub save_as: egui::Key,
    pub export_stl: egui::Key,
    pub export_step: egui::Key,
    
    // Edit operations
    pub undo: egui::Key,
    pub redo: egui::Key,
    pub delete: egui::Key,
    
    // Sketch operations
    pub sketch_new: egui::Key,
    pub sketch_solve: egui::Key,
    pub sketch_add_point: egui::Key,
    pub sketch_add_line: egui::Key,
    pub sketch_add_rect: egui::Key,
    
    // Feature operations
    pub feature_extrude: egui::Key,
    pub feature_revolve: egui::Key,
    pub feature_box: egui::Key,
    pub feature_suppress: egui::Key,
    
    // Selection
    pub select_all: egui::Key,
    pub deselect: egui::Key,
    pub pick_mode_object: egui::Key,
    pub pick_mode_face: egui::Key,
    pub pick_mode_edge: egui::Key,
    pub pick_mode_vertex: egui::Key,
    
    // Help
    pub show_shortcuts: egui::Key,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            // Viewport navigation - modifiers handled separately
            reset_view: egui::Key::Home,
            view_front: egui::Key::Num1,
            view_back: egui::Key::Num2,
            view_left: egui::Key::Num3,
            view_right: egui::Key::Num4,
            view_top: egui::Key::Num5,
            view_bottom: egui::Key::Num6,
            view_iso: egui::Key::Num7,
            
            // File operations
            new: egui::Key::N,
            open: egui::Key::O,
            save: egui::Key::S,
            save_as: egui::Key::S,
            export_stl: egui::Key::E,
            export_step: egui::Key::P,
            
            // Edit operations
            undo: egui::Key::Z,
            redo: egui::Key::Y,
            delete: egui::Key::Delete,
            
            // Sketch operations
            sketch_new: egui::Key::K,
            sketch_solve: egui::Key::Enter,
            sketch_add_point: egui::Key::P,
            sketch_add_line: egui::Key::L,
            sketch_add_rect: egui::Key::R,
            
            // Feature operations
            feature_extrude: egui::Key::E,
            feature_revolve: egui::Key::V,
            feature_box: egui::Key::B,
            feature_suppress: egui::Key::H,
            
            // Selection
            select_all: egui::Key::A,
            deselect: egui::Key::Escape,
            pick_mode_object: egui::Key::Num1,
            pick_mode_face: egui::Key::Num2,
            pick_mode_edge: egui::Key::Num3,
            pick_mode_vertex: egui::Key::Num4,
            
            // Help
            show_shortcuts: egui::Key::F1,
        }
    }
}

impl Shortcuts {
    /// Check if a key is pressed with the appropriate modifier
    pub fn is_pressed(&self, ctx: &egui::Context, key: egui::Key, modifier: egui::Modifiers) -> bool {
        ctx.input(|i| i.modifiers == modifier && i.key_pressed(key))
    }
    
    /// Check for Ctrl+Key combination
    pub fn ctrl_pressed(&self, ctx: &egui::Context, key: egui::Key) -> bool {
        self.is_pressed(ctx, key, egui::Modifiers::CTRL)
    }
    
    
    /// Check for Ctrl+Shift+Key combination
    pub fn ctrl_shift_pressed(&self, ctx: &egui::Context, key: egui::Key) -> bool {
        self.is_pressed(ctx, key, egui::Modifiers::CTRL | egui::Modifiers::SHIFT)
    }
    
    /// Check for plain key (no modifiers)
    pub fn plain_pressed(&self, ctx: &egui::Context, key: egui::Key) -> bool {
        self.is_pressed(ctx, key, egui::Modifiers::NONE)
    }
}

/// Handle all keyboard shortcuts for the application
pub fn handle_shortcuts(app: &mut crate::app::RcadApp, ctx: &egui::Context, shortcuts: &Shortcuts) {
    // While a text field has the keyboard, every digit and letter belongs to
    // it: no shortcut may fire (a dimension value of "50" must not extrude,
    // for instance). This applies to every text field in the application.
    if ctx.wants_keyboard_input() || ctx.memory(|memory| memory.focused().is_some()) {
        return;
    }

    // Viewport navigation (no modifiers for number keys)
    if shortcuts.plain_pressed(ctx, shortcuts.view_front) {
        app.set_view_front();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.view_back) {
        app.set_view_back();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.view_left) {
        app.set_view_left();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.view_right) {
        app.set_view_right();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.view_top) {
        app.set_view_top();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.view_bottom) {
        app.set_view_bottom();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.view_iso) {
        app.set_view_iso();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.reset_view) {
        app.reset_view();
    }
    
    // File operations (Ctrl+Key)
    if shortcuts.ctrl_pressed(ctx, shortcuts.new) {
        app.new_document();
    }
    if shortcuts.ctrl_pressed(ctx, shortcuts.open) {
        app.open_document();
    }
    if shortcuts.ctrl_pressed(ctx, shortcuts.save) {
        app.save_document();
    }
    if shortcuts.ctrl_shift_pressed(ctx, shortcuts.save_as) {
        app.save_document_as();
    }
    if shortcuts.ctrl_shift_pressed(ctx, shortcuts.export_stl) {
        app.export_stl();
    }
    if shortcuts.ctrl_shift_pressed(ctx, shortcuts.export_step) {
        app.export_step();
    }
    
    // Edit operations
    if shortcuts.ctrl_pressed(ctx, shortcuts.undo) {
        app.undo();
    }
    if shortcuts.ctrl_pressed(ctx, shortcuts.redo) || shortcuts.ctrl_shift_pressed(ctx, shortcuts.undo) {
        app.redo();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.delete) {
        app.delete_selected();
    }
    
    // Sketch operations
    if shortcuts.plain_pressed(ctx, shortcuts.sketch_new) {
        app.new_sketch();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.sketch_solve) {
        // Enter finishes the spline being drawn, and solves when nothing is
        // half-finished.
        app.finish_sketch_shape_or_solve();
    }
    // The sketch keys arm a tool; the shape arrives with the clicks, so the
    // key and the toolbar button do the same thing.
    if shortcuts.plain_pressed(ctx, shortcuts.sketch_add_point) {
        app.set_sketch_tool(SketchTool::Point);
    }
    if shortcuts.plain_pressed(ctx, shortcuts.sketch_add_line) {
        app.set_sketch_tool(SketchTool::Line);
    }
    if shortcuts.plain_pressed(ctx, shortcuts.sketch_add_rect) {
        app.set_sketch_tool(SketchTool::Rectangle);
    }
    
    // Feature operations
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::E)) {
        app.edit_definition_selected();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.feature_extrude) {
        app.open_feature_editor(crate::app::FeatureEditorKind::Extrude, None);
    }
    if shortcuts.plain_pressed(ctx, shortcuts.feature_revolve) {
        app.open_feature_editor(crate::app::FeatureEditorKind::Revolve, None);
    }
    if shortcuts.plain_pressed(ctx, shortcuts.feature_box) {
        app.add_box_feature();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.feature_suppress) {
        app.suppress_feature();
    }
    
    // Selection
    if shortcuts.ctrl_pressed(ctx, shortcuts.select_all) {
        app.select_all();
    }
    if shortcuts.plain_pressed(ctx, shortcuts.deselect) {
        // Escape steps back through the sketch first: the pending click, then
        // the tool, and only then the selection.
        app.escape();
    }
    
    // Help - show shortcuts dialog
    if shortcuts.plain_pressed(ctx, shortcuts.show_shortcuts) {
        app.show_shortcuts_help();
    }
}

/// Get a list of all shortcuts with descriptions for display
pub fn shortcuts_list() -> Vec<(&'static str, &'static str)> {
    vec![
        // Viewport Navigation
        ("Viewport Navigation", ""),
        ("1", "Front view"),
        ("2", "Back view"),
        ("3", "Left view"),
        ("4", "Right view"),
        ("5", "Top view"),
        ("6", "Bottom view"),
        ("7", "Isometric view"),
        ("Home", "Reset view"),
        ("Middle Mouse + Drag", "Orbit"),
        ("Right Mouse + Drag", "Pan"),
        ("Scroll Wheel", "Zoom"),
        ("Right Click", "Context Menu"),
        ("", ""),
        // File Operations
        ("File Operations", ""),
        ("Ctrl+N", "New document"),
        ("Ctrl+O", "Open document"),
        ("Ctrl+S", "Save document"),
        ("Ctrl+Shift+S", "Save As"),
        ("Ctrl+Shift+E", "Export STL"),
        ("Ctrl+Shift+P", "Export STEP"),
        ("", ""),
        // Edit Operations
        ("Edit Operations", ""),
        ("Ctrl+Z", "Undo"),
        ("Ctrl+Y / Ctrl+Shift+Z", "Redo"),
        ("Delete", "Delete selected"),
        ("", ""),
        // Sketch Operations
        ("Sketch Operations", ""),
        ("K", "New sketch"),
        ("Enter", "Solve constraints"),
        ("P", "Point tool (click to place)"),
        ("L", "Line tool (click two ends)"),
        ("R", "Rectangle tool (click two corners)"),
        ("", ""),
        // Feature Operations
        ("Feature Operations", ""),
        ("E", "Extrude sketch"),
        ("V", "Revolve sketch"),
        ("B", "Add box"),
        ("H", "Suppress feature"),
        ("", ""),
        // Selection
        ("Selection", ""),
        ("Ctrl+A", "Select all"),
        ("Escape", "Cancel the pending click, then the tool, then the selection"),
        ("", ""),
        // Help
        ("Help", ""),
        ("F1", "Show this dialog"),
    ]
}