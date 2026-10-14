//! Feature Tree Panel - Creo-style model tree on the right side of the viewport.
//!
//! This implements the Creo-style model tree with feature icons for visibility,
//! suppression, and relations/parameters. Solid background, no hover effects.

use eframe::egui;
use rcad_doc::{EntityId, Feature, Part};
use std::collections::HashMap;

/// State for a tree node (expanded/collapsed)
#[derive(Debug, Clone, Default)]
pub struct TreeNodeState {
    pub expanded: bool,
    pub selected: bool,
}

/// History overlay state
#[derive(Debug, Clone)]
pub struct HistoryOverlay {
    pub visible: bool,
    pub hovered: bool,
    pub collapsed: bool,              // Collapsed to top
    pub collapsed_height: f32,        // Height when collapsed
    pub tree_states: HashMap<EntityId, TreeNodeState>,
    pub insert_here_index: Option<usize>,
    pub rollback_index: Option<usize>,
    pub selected_feature: Option<EntityId>,
    pub show_parents: bool,
    pub show_children: bool,
    pub search_filter: String,
    pub context_menu_open: bool,
    pub context_menu_feature: Option<EntityId>,
    pub context_menu_pos: egui::Pos2,
    pub width: f32,                   // User-adjustable width
    pub min_width: f32,
    pub max_width: f32,
    pub resizing: bool,               // Whether user is dragging resize handle
}

impl Default for HistoryOverlay {
    fn default() -> Self {
        Self {
            visible: true,
            hovered: false,
            collapsed: false,
            collapsed_height: 32.0,
            tree_states: HashMap::new(),
            insert_here_index: None,
            rollback_index: None,
            selected_feature: None,
            show_parents: false,
            show_children: false,
            search_filter: String::new(),
            context_menu_open: false,
            context_menu_feature: None,
            context_menu_pos: egui::Pos2::ZERO,
            width: 280.0,      // Reduced from 320
            min_width: 200.0,
            max_width: 500.0,
            resizing: false,
        }
    }
}

impl HistoryOverlay {
    /// Get or create tree state for a feature
    pub fn get_node_state(&mut self, feature_id: EntityId) -> &mut TreeNodeState {
        self.tree_states.entry(feature_id).or_default()
    }
    
    /// Check if a feature is expanded
    pub fn is_expanded(&self, feature_id: EntityId) -> bool {
        self.tree_states.get(&feature_id).map(|s| s.expanded).unwrap_or(true)
    }
    
    /// Toggle expansion state
    pub fn toggle_expanded(&mut self, feature_id: EntityId) {
        self.get_node_state(feature_id).expanded = !self.is_expanded(feature_id);
    }
    
    /// Set selected feature
    pub fn select_feature(&mut self, feature_id: Option<EntityId>) {
        self.selected_feature = feature_id;
    }
    
    /// Set insert here position
    pub fn set_insert_here(&mut self, index: Option<usize>) {
        self.insert_here_index = index;
    }
    
    /// Set rollback position
    pub fn set_rollback(&mut self, index: Option<usize>) {
        self.rollback_index = index;
    }
    
    /// Open context menu for a feature
    pub fn open_context_menu(&mut self, feature_id: EntityId, pos: egui::Pos2) {
        self.context_menu_open = true;
        self.context_menu_feature = Some(feature_id);
        self.context_menu_pos = pos;
    }
    
    /// Close context menu
    pub fn close_context_menu(&mut self) {
        self.context_menu_open = false;
        self.context_menu_feature = None;
    }
    
    /// Render the history overlay
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        part: &Part,
        on_action: &mut dyn FnMut(HistoryOverlayAction),
    ) {
        if !self.visible {
            return;
        }
        
        // Handle collapsed state - show only header bar at top
        if self.collapsed {
            let collapsed_rect = egui::Rect::from_min_size(
                egui::pos2(ui.max_rect().right() - self.width, ui.max_rect().top()),
                egui::vec2(self.width, self.collapsed_height),
            );
            
            // Solid background for collapsed state
            ui.painter().rect_filled(collapsed_rect, 0.0, egui::Color32::from_gray(30));
            
            let mut header_ui = ui.new_child(egui::UiBuilder::new().max_rect(collapsed_rect).layout(*ui.layout()));
            header_ui.horizontal(|ui| {
                ui.strong("Model Tree");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("⬇").on_hover_text("Expand").clicked() {
                        self.collapsed = false;
                    }
                });
            });
            return;
        }
        
        // Normal expanded state - always solid background, no hover effects
        let content_width = self.width;
        let content_rect = egui::Rect::from_min_size(
            egui::pos2(ui.max_rect().right() - content_width, ui.max_rect().top()),
            egui::vec2(content_width, ui.max_rect().height()),
        );
        
        // Add resize handle on left edge of content
        let resize_handle_width = 8.0;
        let resize_handle_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.left() - resize_handle_width, content_rect.top()),
            egui::vec2(resize_handle_width, content_rect.height()),
        );
        
        let resize_response = ui.interact(
            resize_handle_rect,
            ui.id().with("history_overlay_resize"),
            egui::Sense::drag(),
        );
        
        if resize_response.dragged() {
            self.resizing = true;
            let new_width = (self.width - resize_response.drag_delta().x).clamp(self.min_width, self.max_width);
            self.width = new_width;
        }
        if !resize_response.dragged() && self.resizing {
            self.resizing = false;
        }
        
        // Change cursor when hovering resize handle
        if resize_response.hovered() || self.resizing {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        
        // Always solid background - no hover darkening
        ui.painter().rect_filled(content_rect, 0.0, egui::Color32::from_gray(30));
        
        let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(content_rect).layout(*ui.layout()));
        
        // Header
        content_ui.horizontal(|ui| {
            ui.strong("Model Tree");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🔍").on_hover_text("Search").clicked() {
                    // Toggle search
                }
                if ui.small_button("⚙").on_hover_text("Options").clicked() {
                    // Show options
                }
                // Collapse to top button
                if ui.small_button("⬆").on_hover_text("Collapse to top").clicked() {
                    self.collapsed = true;
                }
                if ui.small_button("✕").on_hover_text("Hide").clicked() {
                    self.visible = false;
                }
            });
        });
        
        content_ui.separator();
        
        // Search box
        content_ui.horizontal(|ui| {
            ui.label("🔍");
            ui.text_edit_singleline(&mut self.search_filter);
            if ui.small_button("✕").clicked() {
                self.search_filter.clear();
            }
        });
        
        content_ui.separator();
        
        // Feature tree
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(&mut content_ui, |ui| {
                self.render_feature_tree(ui, part, on_action);
            });
        
        // Context menu
        if self.context_menu_open {
            self.show_context_menu(&mut content_ui, on_action);
        }
    }
    
    /// Render the feature tree recursively
    fn render_feature_tree(
        &mut self,
        ui: &mut egui::Ui,
        part: &Part,
        on_action: &mut dyn FnMut(HistoryOverlayAction),
    ) {
        let features = &part.features;
        
        // Compute parent-child relationships
        let mut children_map: HashMap<EntityId, Vec<usize>> = HashMap::new();
        for (i, feature) in features.iter().enumerate() {
            for parent_id in &feature.parent_features {
                children_map.entry(*parent_id).or_default().push(i);
            }
        }
        
        // Find root features (no parents)
        let root_indices: Vec<usize> = features.iter().enumerate()
            .filter(|(_, f)| f.parent_features.is_empty())
            .map(|(i, _)| i)
            .collect();
        
        // Render each root feature
        for root_idx in root_indices {
            self.render_feature_node(ui, part, root_idx, 0, &children_map, on_action);
        }
    }
    
    /// Render a single feature node with its children
    fn render_feature_node(
        &mut self,
        ui: &mut egui::Ui,
        part: &Part,
        feature_idx: usize,
        depth: usize,
        children_map: &HashMap<EntityId, Vec<usize>>,
        on_action: &mut dyn FnMut(HistoryOverlayAction),
    ) {
        let feature = &part.features[feature_idx];
        
        // Filter by search
        if !self.search_filter.is_empty() {
            let filter = self.search_filter.to_lowercase();
            if !feature.name.to_lowercase().contains(&filter) {
                // Check if any child matches
                let mut has_match = false;
                if let Some(child_indices) = children_map.get(&feature.id) {
                    for child_idx in child_indices {
                        if part.features[*child_idx].name.to_lowercase().contains(&filter) {
                            has_match = true;
                            break;
                        }
                    }
                }
                if !has_match {
                    return; // Skip this node and its children
                }
            }
        }
        
        let is_selected = self.selected_feature == Some(feature.id);
        let has_children = children_map.contains_key(&feature.id);
        let is_expanded = self.is_expanded(feature.id);
        
        // Determine visual state
        let (icon, label_color, strikethrough) = if feature.suppressed {
            ("🚫", egui::Color32::GRAY, true)
        } else if self.is_feature_error(feature) {
            ("❌", egui::Color32::LIGHT_RED, false)
        } else if self.is_feature_warning(feature) {
            ("⚠️", egui::Color32::YELLOW, false)
        } else {
            ("✅", egui::Color32::WHITE, false)
        };
        
        // Feature visibility - check if feature has visible flag
        let is_visible = feature.visible;
        let visibility_icon = if is_visible { "👁" } else { "🚫" };
        let visibility_color = if is_visible { egui::Color32::WHITE } else { egui::Color32::GRAY };
        
        // Suppression state
        let is_suppressed = feature.suppressed;
        let suppression_icon = if is_suppressed { "⛔" } else { "✅" };
        let suppression_color = if is_suppressed { egui::Color32::GRAY } else { egui::Color32::WHITE };
        
        // Relations/parameters indicator
        let has_relations = !feature.parameters.numeric.is_empty()
            || !feature.parameters.string.is_empty()
            || !feature.parameters.boolean.is_empty();
        let relations_icon = "⚙";
        
        // Indent based on depth
        let indent = depth as f32 * 16.0;
        
        ui.horizontal(|ui| {
            ui.add_space(indent);
            
            // Expand/collapse toggle
            if has_children {
                let toggle_icon = if is_expanded { "▼" } else { "▶" };
                if ui.small_button(toggle_icon).clicked() {
                    self.toggle_expanded(feature.id);
                }
            } else {
                ui.add_space(16.0);
            }
            
            // Insert Here marker
            if self.insert_here_index == Some(feature_idx) {
                ui.label(egui::RichText::new("▶").color(egui::Color32::GREEN).strong());
            } else {
                ui.add_space(16.0);
            }
            
            // Rollback marker
            if self.rollback_index == Some(feature_idx) {
                ui.label(egui::RichText::new("⏪").color(egui::Color32::ORANGE).strong());
            }
            
            // Feature icon
            ui.label(icon);
            
            // Feature name
            let mut label = egui::RichText::new(&feature.name)
                .size(13.0)
                .color(label_color);
            if strikethrough {
                label = label.strikethrough();
            }
            if is_selected {
                label = label.background_color(egui::Color32::from_gray(60));
            }
            
            let response = ui.selectable_label(is_selected, label);
            
            // Click to select
            if response.clicked() {
                on_action(HistoryOverlayAction::SelectFeature(feature.id));
            }
            
            // Right-click for context menu
            if response.secondary_clicked() {
                on_action(HistoryOverlayAction::OpenContextMenu(feature.id, response.rect.min));
            }
            
            // Drag to reorder
            if response.dragged() {
                on_action(HistoryOverlayAction::DragFeature(feature.id));
            }
            
            // Drop target
            if response.hovered() && ui.input(|i| i.pointer.any_released()) {
                on_action(HistoryOverlayAction::DropFeature(feature.id));
            }
            
            // Action icons (right side)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Relations/Parameters icon (click to open dialog)
                let rel_response = ui.small_button(egui::RichText::new(relations_icon).size(14.0).color(egui::Color32::WHITE))
                    .on_hover_text("Relations & Parameters");
                if rel_response.clicked() {
                    on_action(HistoryOverlayAction::OpenRelationsDialog(feature.id));
                }
                
                // Suppress/Resume icon
                let sup_response = ui.small_button(egui::RichText::new(suppression_icon).size(14.0).color(suppression_color))
                    .on_hover_text(if is_suppressed { "Resume (Ctrl+H)" } else { "Suppress (Ctrl+H)" });
                if sup_response.clicked() {
                    on_action(HistoryOverlayAction::ToggleSuppression(feature.id));
                }
                
                // Show/Hide icon
                let vis_response = ui.small_button(egui::RichText::new(visibility_icon).size(14.0).color(visibility_color))
                    .on_hover_text(if is_visible { "Hide" } else { "Show" });
                if vis_response.clicked() {
                    on_action(HistoryOverlayAction::ToggleVisibility(feature.id));
                }
            });
            
            // External reference indicator
            if feature.parameters.string.contains_key("is_external_ref") {
                ui.label(egui::RichText::new("🔗").size(10.0));
                let status_str = feature.parameters.string.get("external_ref_status")
                    .cloned()
                    .unwrap_or_else(|| "uptodate".to_string());
                let status_color = match status_str.as_str() {
                    "uptodate" => egui::Color32::GREEN,
                    "outofdate" => egui::Color32::YELLOW,
                    "broken" => egui::Color32::RED,
                    _ => egui::Color32::GRAY,
                };
                ui.label(egui::RichText::new("●").size(8.0).color(status_color));
            }
        });
        
        // Render children if expanded
        let node_state = self.get_node_state(feature.id);
        if node_state.expanded {
            if let Some(child_indices) = children_map.get(&feature.id) {
                for child_idx in child_indices {
                    self.render_feature_node(ui, part, *child_idx, depth + 1, children_map, on_action);
                }
            }
        }
    }
    
    /// Check if feature has error (placeholder)
    fn is_feature_error(&self, feature: &Feature) -> bool {
        // In real implementation, check rebuild results
        false
    }
    
    /// Check if feature has warning (placeholder)
    fn is_feature_warning(&self, feature: &Feature) -> bool {
        false
    }
    
    /// Show context menu
    fn show_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        on_action: &mut dyn FnMut(HistoryOverlayAction),
    ) {
        if let Some(feature_id) = self.context_menu_feature {
            egui::Area::new(egui::Id::new("history_context_menu"))
                .fixed_pos(self.context_menu_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::menu(ui.style())
                        .show(ui, |ui| {
                            ui.set_min_width(200.0);
                            
                            // Edit Definition
                            if ui.button("Edit Definition...  Ctrl+E").clicked() {
                                on_action(HistoryOverlayAction::EditDefinition(feature_id));
                                self.close_context_menu();
                            }
                            ui.separator();
                            
                            // Suppress/Resume
                            if ui.button("Suppress  Ctrl+H").clicked() {
                                on_action(HistoryOverlayAction::SuppressFeature(feature_id));
                                self.close_context_menu();
                            }
                            if ui.button("Resume").clicked() {
                                on_action(HistoryOverlayAction::ResumeFeature(feature_id));
                                self.close_context_menu();
                            }
                            ui.separator();
                            
                            // Insert Here / Rollback
                            if ui.button("Insert Here").clicked() {
                                on_action(HistoryOverlayAction::SetInsertHere(feature_id));
                                self.close_context_menu();
                            }
                            if ui.button("Rollback Here").clicked() {
                                on_action(HistoryOverlayAction::SetRollback(feature_id));
                                self.close_context_menu();
                            }
                            ui.separator();
                            
                            // Rename
                            if ui.button("Rename  F2").clicked() {
                                on_action(HistoryOverlayAction::RenameFeature(feature_id));
                                self.close_context_menu();
                            }
                            
                            // Delete
                            if ui.button("Delete  Del").clicked() {
                                on_action(HistoryOverlayAction::DeleteFeature(feature_id));
                                self.close_context_menu();
                            }
                            ui.separator();
                            
                            // Copy/Paste
                            if ui.button("Copy  Ctrl+C").clicked() {
                                on_action(HistoryOverlayAction::CopyFeature(feature_id));
                                self.close_context_menu();
                            }
                            if ui.button("Paste  Ctrl+V").clicked() {
                                on_action(HistoryOverlayAction::PasteFeature(feature_id));
                                self.close_context_menu();
                            }
                            if ui.button("Paste Special...").clicked() {
                                on_action(HistoryOverlayAction::PasteSpecialFeature(feature_id));
                                self.close_context_menu();
                            }
                            ui.separator();
                            
                            // Parents/Children
                            if ui.button("Parents").clicked() {
                                on_action(HistoryOverlayAction::ShowParents(feature_id));
                                self.close_context_menu();
                            }
                            if ui.button("Children").clicked() {
                                on_action(HistoryOverlayAction::ShowChildren(feature_id));
                                self.close_context_menu();
                            }
                            ui.separator();
                            
                            // Info
                            if ui.button("Information...").clicked() {
                                on_action(HistoryOverlayAction::ShowInfo(feature_id));
                                self.close_context_menu();
                            }
                        });
                });
        }
    }
}

/// Actions that can be triggered from the history overlay
#[derive(Debug, Clone)]
pub enum HistoryOverlayAction {
    SelectFeature(EntityId),
    OpenContextMenu(EntityId, egui::Pos2),
    EditDefinition(EntityId),
    SuppressFeature(EntityId),
    ResumeFeature(EntityId),
    SetInsertHere(EntityId),
    SetRollback(EntityId),
    RenameFeature(EntityId),
    DeleteFeature(EntityId),
    CopyFeature(EntityId),
    PasteFeature(EntityId),
    PasteSpecialFeature(EntityId),
    ShowParents(EntityId),
    ShowChildren(EntityId),
    ShowInfo(EntityId),
    DragFeature(EntityId),
    DropFeature(EntityId),
    ToggleVisibility(EntityId),
    ToggleSuppression(EntityId),
    OpenRelationsDialog(EntityId),
}