//! Application state, messages, and view/update logic for the QGIS GUI.
//!
//! Ties together the layer registry, map canvas, layer panel, menu bar,
//! and modal dialogs into a single [`iced`] application.

use std::collections::HashMap;
use std::sync::Arc;

use iced::widget::canvas::Canvas;
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task};

use qgis_core::{Feature, Layer, LayerRegistry, Value};
use qgis_crs::{Camera, SrsId};
use qgis_providers::geojson;
use qgis_style::{parse_qml_file, StyleModel};

use crate::dialogs;
use crate::layer_panel;
use crate::layer_renderer::{camera_for_extent, load_shapefile_as_layer};
use crate::map_program::MapProgram;

/// Main application state.
#[derive(Debug)]
pub struct AppState {
    /// All loaded layers.
    pub registry: LayerRegistry,
    /// Per-layer styles (layer index -> parsed QML style).
    pub styles: HashMap<usize, StyleModel>,
    /// Viewport transform for the map canvas.
    pub camera: Camera,
    /// Generation counter for viewport reset.
    pub generation: u64,
    /// Currently selected layer index in the panel.
    pub selected_layer: Option<usize>,
    /// Active dialog.
    pub dialog: Option<Dialog>,
    /// Status bar message.
    pub status: String,
    /// Whether the File menu dropdown is open.
    pub file_menu_open: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            registry: LayerRegistry::new(),
            styles: HashMap::new(),
            camera: Camera::new((0.0, 0.0), 3.0, (1024.0, 768.0)),
            generation: 0,
            selected_layer: None,
            dialog: None,
            status: "Ready — use Open File to load a spatial file".into(),
            file_menu_open: false,
        }
    }
}

/// Active modal dialog.
#[derive(Debug)]
pub enum Dialog {
    /// File open dialog with a path input.
    FileOpen { path: String },
    /// Style assignment dialog with a layer index and .qml path.
    StyleAssign { layer_index: usize, path: String },
    /// Project save dialog with a .qgs path.
    ProjectSave { path: String },
    /// Project open dialog with a .qgs path.
    ProjectOpen { path: String },
    /// Error dialog with a message.
    Error { message: String },
}

/// Messages produced by UI interactions.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    // Menu bar actions
    /// Open the file-open dialog.
    OpenFileDialog,
    /// Open the project-save dialog.
    SaveProjectDialog,
    /// Open the project-open dialog.
    OpenProjectDialog,
    /// Open the style-assignment dialog for the selected layer.
    AssignStyleDialog,

    // File dialog
    /// File path input changed.
    FileDialogInputChanged(String),
    /// Confirm file open.
    FileDialogConfirm,

    // Style dialog
    /// Style path input changed.
    StyleDialogInputChanged(String),
    /// Confirm style assignment.
    StyleDialogConfirm,

    // Project dialog
    /// Project path input changed.
    ProjectPathChanged(String),
    /// Confirm project save.
    ProjectSaveConfirm,
    /// Confirm project open.
    ProjectOpenConfirm,

    // Layer panel
    /// Toggle layer visibility.
    LayerVisibilityToggled(usize),
    /// Select a layer.
    LayerSelected(usize),
    /// Delete a layer.
    LayerDeleted(usize),
    /// Move layer up in the panel order.
    LayerMoveUp(usize),
    /// Move layer down in the panel order.
    LayerMoveDown(usize),

    // Common
    /// Cancel/close any dialog.
    Cancel,
    /// Dismiss the error dialog.
    DismissError,
    /// Toggle the File menu dropdown.
    ToggleFileMenu,
}

/// Update function for the application.
pub fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        // ---- Menu bar ----
        Message::OpenFileDialog => {
            state.file_menu_open = false;
            state.dialog = Some(Dialog::FileOpen { path: String::new() });
            Task::none()
        }
        Message::SaveProjectDialog => {
            state.file_menu_open = false;
            let default_path = state
                .registry
                .iter()
                .next()
                .map(|l| format!("{}.qgs", l.name))
                .unwrap_or_else(|| "project.qgs".into());
            state.dialog = Some(Dialog::ProjectSave { path: default_path });
            Task::none()
        }
        Message::OpenProjectDialog => {
            state.file_menu_open = false;
            state.dialog = Some(Dialog::ProjectOpen { path: String::new() });
            Task::none()
        }
        Message::AssignStyleDialog => {
            if let Some(idx) = state.selected_layer {
                state.dialog = Some(Dialog::StyleAssign {
                    layer_index: idx,
                    path: String::new(),
                });
            } else if !state.registry.is_empty() {
                state.selected_layer = Some(0);
                state.dialog = Some(Dialog::StyleAssign {
                    layer_index: 0,
                    path: String::new(),
                });
            } else {
                state.status = "No layers to style — load a file first".into();
            }
            Task::none()
        }

        // ---- File dialog ----
        Message::FileDialogInputChanged(path) => {
            if let Some(Dialog::FileOpen { path: p }) = &mut state.dialog {
                *p = path;
            }
            Task::none()
        }
        Message::FileDialogConfirm => {
            let path = match &state.dialog {
                Some(Dialog::FileOpen { path }) => path.clone(),
                _ => return Task::none(),
            };
            state.dialog = None;
            load_layer_from_path(state, &path);
            Task::none()
        }

        // ---- Style dialog ----
        Message::StyleDialogInputChanged(path) => {
            if let Some(Dialog::StyleAssign { path: p, .. }) = &mut state.dialog {
                *p = path;
            }
            Task::none()
        }
        Message::StyleDialogConfirm => {
            let (layer_index, path) = match &state.dialog {
                Some(Dialog::StyleAssign { layer_index, path }) => (*layer_index, path.clone()),
                _ => return Task::none(),
            };
            state.dialog = None;
            assign_style(state, layer_index, &path);
            Task::none()
        }

        // ---- Project dialog ----
        Message::ProjectPathChanged(path) => {
            match &mut state.dialog {
                Some(Dialog::ProjectSave { path: p }) => *p = path,
                Some(Dialog::ProjectOpen { path: p }) => *p = path,
                _ => {}
            }
            Task::none()
        }
        Message::ProjectSaveConfirm => {
            let path = match &state.dialog {
                Some(Dialog::ProjectSave { path }) => path.clone(),
                _ => return Task::none(),
            };
            state.dialog = None;
            match save_project(state, &path) {
                Ok(_) => state.status = format!("Project saved to {}", path),
                Err(e) => state.dialog = Some(Dialog::Error { message: e }),
            }
            Task::none()
        }
        Message::ProjectOpenConfirm => {
            let path = match &state.dialog {
                Some(Dialog::ProjectOpen { path }) => path.clone(),
                _ => return Task::none(),
            };
            state.dialog = None;
            state.status = format!("Project open not yet implemented: {}", path);
            Task::none()
        }

        // ---- Layer panel ----
        Message::LayerVisibilityToggled(idx) => {
            if let Some(layer) = state.registry.layer_at_mut(idx) {
                layer.visible = !layer.visible;
            }
            Task::none()
        }
        Message::LayerSelected(idx) => {
            state.selected_layer = Some(idx);
            Task::none()
        }
        Message::LayerDeleted(idx) => {
            state.registry.remove_layer(idx);
            if state.registry.is_empty() {
                state.selected_layer = None;
            } else if let Some(selected) = state.selected_layer {
                if selected == idx {
                    state.selected_layer = Some(0);
                } else if selected > idx {
                    state.selected_layer = Some(selected - 1);
                }
            }
            // Remove style for deleted layer and shift remaining indices down.
            let mut new_styles = HashMap::new();
            for (old_idx, style) in state.styles.drain() {
                if old_idx < idx {
                    new_styles.insert(old_idx, style);
                } else if old_idx > idx {
                    new_styles.insert(old_idx - 1, style);
                }
                // Skip old_idx == idx (the deleted layer's style).
            }
            state.styles = new_styles;
            state.status = format!("Layer at index {} deleted", idx);
            Task::none()
        }
        Message::LayerMoveUp(idx) => {
            if idx > 0 {
                state.registry.move_layer(idx, idx - 1);
                // Swap styles between the two positions.
                let style_a = state.styles.remove(&idx);
                let style_b = state.styles.remove(&(idx - 1));
                if let Some(s) = style_a {
                    state.styles.insert(idx - 1, s);
                }
                if let Some(s) = style_b {
                    state.styles.insert(idx, s);
                }
                if state.selected_layer == Some(idx) {
                    state.selected_layer = Some(idx - 1);
                }
            }
            Task::none()
        }
        Message::LayerMoveDown(idx) => {
            if idx + 1 < state.registry.len() {
                state.registry.move_layer(idx, idx + 1);
                // Swap styles between the two positions.
                let style_a = state.styles.remove(&idx);
                let style_b = state.styles.remove(&(idx + 1));
                if let Some(s) = style_a {
                    state.styles.insert(idx + 1, s);
                }
                if let Some(s) = style_b {
                    state.styles.insert(idx, s);
                }
                if state.selected_layer == Some(idx) {
                    state.selected_layer = Some(idx + 1);
                }
            }
            Task::none()
        }

        // ---- Common ----
        Message::Cancel => {
            state.dialog = None;
            Task::none()
        }
        Message::DismissError => {
            state.dialog = None;
            Task::none()
        }
        Message::ToggleFileMenu => {
            state.file_menu_open = !state.file_menu_open;
            Task::none()
        }
    }
}

/// View function for the application.
pub fn view(state: &AppState) -> Element<'_, Message> {
    if let Some(dialog) = &state.dialog {
        return dialogs::view(dialog);
    }

    let menu = menu_bar(state);
    let map = map_canvas(state);
    let panel = layer_panel::view(state);
    let status = status_bar(state);

    column![
        menu,
        row![
            container(map).width(Length::Fill).height(Length::Fill),
            container(panel)
                .width(Length::Fixed(280.0))
                .height(Length::Fill),
        ]
        .height(Length::Fill),
        status,
    ]
    .into()
}

/// Menu bar widget: a "File" dropdown menu with the three required items.
fn menu_bar(state: &AppState) -> Element<'_, Message> {
    let file_menu_button = if state.file_menu_open {
        button(text("File ▼")).on_press(Message::ToggleFileMenu)
    } else {
        button(text("File")).on_press(Message::ToggleFileMenu)
    };

    let mut menu_column = column![file_menu_button];

    if state.file_menu_open {
        for (label, message) in file_menu_items() {
            menu_column = menu_column.push(
                button(text(label).width(Length::Fill))
                    .style(button::text)
                    .on_press(message),
            );
        }
    }

    row![
        menu_column.spacing(4),
        button(text("Assign Style")).on_press(Message::AssignStyleDialog),
    ]
    .spacing(8)
    .padding(8)
    .into()
}

/// The items in the File menu: (label, message) pairs in display order.
fn file_menu_items() -> Vec<(&'static str, Message)> {
    vec![
        ("Open Spatial File", Message::OpenFileDialog),
        ("Save Project", Message::SaveProjectDialog),
        ("Open Project", Message::OpenProjectDialog),
    ]
}

/// Map canvas widget.
fn map_canvas(state: &AppState) -> Element<'_, Message> {
    Canvas::new(MapProgram {
        registry: Arc::new(state.registry.clone()),
        styles: Arc::new(state.styles.clone()),
        initial_camera: state.camera,
        generation: state.generation,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Status bar widget.
fn status_bar(state: &AppState) -> Element<'_, Message> {
    text(&state.status).size(12).into()
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Loads a layer from a file path, detecting format by extension.
///
/// Supports `.shp` (shapefile), `.geojson`/`.json` (GeoJSON), and `.gpkg`
/// (GeoPackage — not yet implemented). On success, adds the layer to the
/// registry and fits the viewport to the layer extent.
fn load_layer_from_path(state: &mut AppState, path: &str) {
    let result = if path.ends_with(".shp") {
        load_shapefile_as_layer(path)
    } else if path.ends_with(".geojson") || path.ends_with(".json") {
        load_geojson_as_layer(path)
    } else if path.ends_with(".gpkg") {
        Err("GeoPackage loading not yet supported".into())
    } else {
        Err(format!("Unsupported file format: {}", path))
    };

    match result {
        Ok(layer) => {
            let extent = layer.extent();
            state.registry.add_layer(layer);
            state.selected_layer = Some(state.registry.len() - 1);
            if let Some(extent) = extent {
                state.camera = camera_for_extent(extent, (1024.0, 768.0));
                state.generation += 1;
            }
            state.status = format!("Loaded: {}", path);
        }
        Err(e) => {
            state.dialog = Some(Dialog::Error { message: e });
        }
    }
}

/// Loads a GeoJSON file into a [`Layer`].
fn load_geojson_as_layer(path: &str) -> Result<Layer, String> {
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let fc = geojson::FeatureCollection::from_json(&data)?;
    let name = path
        .rsplit('/')
        .next()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "layer".into());
    let mut layer = Layer::new(name, SrsId::from_epsg(4326));
    for f in fc.features {
        let geometry = f.geometry;
        let properties = f.properties;
        let mut feature = Feature::new(layer.feature_count() as i64, geometry);
        for (k, v) in properties {
            feature.set_attribute(k, json_to_value(&v));
        }
        layer.features.push(feature);
    }
    Ok(layer)
}

/// Converts a [`serde_json::Value`] to a [`qgis_core::Value`].
fn json_to_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(f64::NAN))
            }
        }
        serde_json::Value::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
}

/// Assigns a `.qml` style to a layer by index.
fn assign_style(state: &mut AppState, layer_index: usize, path: &str) {
    match parse_qml_file(path) {
        Ok(model) => {
            state.styles.insert(layer_index, model);
            state.status = format!("Style applied to layer {}: {}", layer_index, path);
        }
        Err(e) => {
            state.dialog = Some(Dialog::Error { message: e });
        }
    }
}

/// Saves a `.qgs` project file (minimal XML format).
///
/// Serializes layer names, visibility, opacity, style references, and
/// the current viewport (center + zoom).
fn save_project(state: &AppState, path: &str) -> Result<(), String> {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<qgis version=\"4.0.0\">\n");
    xml.push_str("  <title>QGIS Rust Project</title>\n");
    xml.push_str("  <projectLayers>\n");
    for (idx, layer) in state.registry.iter().enumerate() {
        let style_ref = state
            .styles
            .get(&idx)
            .map(|s| format!("{:?}", s.renderer))
            .unwrap_or_else(|| "default".into());
        xml.push_str(&format!(
            "    <layer name=\"{}\" visible=\"{}\" opacity=\"{}\" style=\"{}\"/>\n",
            layer.name, layer.visible, layer.opacity, style_ref
        ));
    }
    xml.push_str("  </projectLayers>\n");
    xml.push_str(&format!(
        "  <view center=\"{:.6},{:.6}\" zoom=\"{:.4}\"/>\n",
        state.camera.center.0, state.camera.center.1, state.camera.zoom
    ));
    xml.push_str("</qgis>\n");
    std::fs::write(path, xml).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::Point as GeoPoint;

    #[test]
    fn default_state_is_empty_with_world_view() {
        let state = AppState::default();
        assert!(state.registry.is_empty());
        assert!(state.styles.is_empty());
        assert_eq!(state.registry.len(), 0);
        assert_eq!(state.camera.zoom, 3.0);
        assert!(state.status.contains("Ready"));
    }

    #[test]
    fn json_to_value_converts_all_types() {
        assert_eq!(json_to_value(&serde_json::Value::Null), Value::Null);
        assert_eq!(
            json_to_value(&serde_json::Value::Bool(true)),
            Value::Bool(true)
        );
        assert_eq!(
            json_to_value(&serde_json::Value::from(42)),
            Value::Int(42)
        );
        assert_eq!(
            json_to_value(&serde_json::Value::from(3.14)),
            Value::Float(3.14)
        );
        assert_eq!(
            json_to_value(&serde_json::Value::from("hello")),
            Value::Text("hello".into())
        );
    }

    #[test]
    fn save_project_writes_valid_xml() {
        let mut state = AppState::default();
        let mut layer = Layer::new("test_layer", SrsId::from_epsg(4326));
        layer.features.push(Feature::new(
            1,
            Some(qgis_geometry::Geometry::Point(GeoPoint::new(0.0, 0.0))),
        ));
        state.registry.add_layer(layer);

        let path = std::env::temp_dir().join("test_project.qgs");
        let path_str = path.to_str().unwrap();
        save_project(&state, path_str).unwrap();

        let content = std::fs::read_to_string(path_str).unwrap();
        assert!(content.contains("<qgis"));
        assert!(content.contains("test_layer"));
        assert!(content.contains("</qgis>"));

        // Clean up
        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn layer_visibility_toggle_flips_flag() {
        let mut state = AppState::default();
        let mut layer = Layer::new("toggle_test", SrsId::from_epsg(4326));
        layer.features.push(Feature::new(
            1,
            Some(qgis_geometry::Geometry::Point(GeoPoint::new(0.0, 0.0))),
        ));
        state.registry.add_layer(layer);

        assert!(state.registry.layer_at(0).unwrap().visible);
        update(&mut state, Message::LayerVisibilityToggled(0));
        assert!(!state.registry.layer_at(0).unwrap().visible);
        update(&mut state, Message::LayerVisibilityToggled(0));
        assert!(state.registry.layer_at(0).unwrap().visible);
    }

    #[test]
    fn layer_delete_removes_and_adjusts_selection() {
        let mut state = AppState::default();
        state.registry.add_layer(Layer::new("a", SrsId::from_epsg(4326)));
        state.registry.add_layer(Layer::new("b", SrsId::from_epsg(4326)));
        state.registry.add_layer(Layer::new("c", SrsId::from_epsg(4326)));
        state.selected_layer = Some(1);

        update(&mut state, Message::LayerDeleted(1));
        assert_eq!(state.registry.len(), 2);
        assert_eq!(state.registry.layer_at(0).unwrap().name, "a");
        assert_eq!(state.registry.layer_at(1).unwrap().name, "c");
        assert_eq!(state.selected_layer, Some(0));
    }

    #[test]
    fn layer_move_up_and_down_reorders() {
        let mut state = AppState::default();
        state.registry.add_layer(Layer::new("a", SrsId::from_epsg(4326)));
        state.registry.add_layer(Layer::new("b", SrsId::from_epsg(4326)));
        state.registry.add_layer(Layer::new("c", SrsId::from_epsg(4326)));

        // Move "c" (index 2) up -> should swap with "b"
        update(&mut state, Message::LayerMoveUp(2));
        assert_eq!(state.registry.layer_at(1).unwrap().name, "c");
        assert_eq!(state.registry.layer_at(2).unwrap().name, "b");

        // Move "c" (now index 1) down -> swap back with "b"
        update(&mut state, Message::LayerMoveDown(1));
        assert_eq!(state.registry.layer_at(1).unwrap().name, "b");
        assert_eq!(state.registry.layer_at(2).unwrap().name, "c");

        // Move up at index 0 is a no-op
        update(&mut state, Message::LayerMoveUp(0));
        assert_eq!(state.registry.layer_at(0).unwrap().name, "a");

        // Move down at last index is a no-op
        update(&mut state, Message::LayerMoveDown(2));
        assert_eq!(state.registry.layer_at(2).unwrap().name, "c");
    }

    #[test]
    fn cancel_clears_dialog() {
        let mut state = AppState::default();
        state.dialog = Some(Dialog::FileOpen { path: "test.shp".into() });
        update(&mut state, Message::Cancel);
        assert!(state.dialog.is_none());
    }

    #[test]
    fn file_dialog_input_updates_path() {
        let mut state = AppState::default();
        state.dialog = Some(Dialog::FileOpen { path: String::new() });
        update(&mut state, Message::FileDialogInputChanged("roads.shp".into()));
        match &state.dialog {
            Some(Dialog::FileOpen { path }) => assert_eq!(path, "roads.shp"),
            other => panic!("expected FileOpen, got {:?}", other),
        }
    }

    #[test]
    fn style_dialog_input_updates_path() {
        let mut state = AppState::default();
        state.dialog = Some(Dialog::StyleAssign {
            layer_index: 0,
            path: String::new(),
        });
        update(&mut state, Message::StyleDialogInputChanged("polys.qml".into()));
        match &state.dialog {
            Some(Dialog::StyleAssign { path, .. }) => assert_eq!(path, "polys.qml"),
            other => panic!("expected StyleAssign, got {:?}", other),
        }
    }

    #[test]
    fn assign_style_dialog_opens_for_selected_layer() {
        let mut state = AppState::default();
        state.registry.add_layer(Layer::new("a", SrsId::from_epsg(4326)));
        state.selected_layer = Some(0);
        update(&mut state, Message::AssignStyleDialog);
        match &state.dialog {
            Some(Dialog::StyleAssign { layer_index, .. }) => assert_eq!(*layer_index, 0),
            other => panic!("expected StyleAssign, got {:?}", other),
        }
    }

    #[test]
    fn assign_style_dialog_selects_first_layer_if_none_selected() {
        let mut state = AppState::default();
        state.registry.add_layer(Layer::new("a", SrsId::from_epsg(4326)));
        state.selected_layer = None;
        update(&mut state, Message::AssignStyleDialog);
        assert_eq!(state.selected_layer, Some(0));
        assert!(matches!(state.dialog, Some(Dialog::StyleAssign { .. })));
    }

    #[test]
    fn assign_style_dialog_warns_when_no_layers() {
        let mut state = AppState::default();
        update(&mut state, Message::AssignStyleDialog);
        assert!(state.dialog.is_none());
        assert!(state.status.contains("No layers"));
    }

    /// Bead qgis-cci acceptance: the menu bar must be a "File" menu containing
    /// exactly "Open Spatial File", "Save Project", and "Open Project" items,
    /// wired to the LayerRegistry through the file-open / project-save /
    /// project-open dialog paths.
    #[test]
    fn menu_bar_has_file_menu_with_required_items() {
        // The menu bar is built from a declarative list of (label, message)
        // pairs. This test pins the exact structure the bead requires: a
        // "File" menu with three items in the specified order.
        let items = file_menu_items();
        assert_eq!(
            items,
            vec![
                ("Open Spatial File", Message::OpenFileDialog),
                ("Save Project", Message::SaveProjectDialog),
                ("Open Project", Message::OpenProjectDialog),
            ]
        );
    }

    /// The File menu dropdown toggles open and closed, and closes when a
    /// menu item is selected.
    #[test]
    fn file_menu_toggles_and_closes_on_selection() {
        let mut state = AppState::default();
        assert!(!state.file_menu_open);

        update(&mut state, Message::ToggleFileMenu);
        assert!(state.file_menu_open);

        update(&mut state, Message::ToggleFileMenu);
        assert!(!state.file_menu_open);

        // Selecting an item closes the menu.
        update(&mut state, Message::ToggleFileMenu);
        assert!(state.file_menu_open);
        update(&mut state, Message::OpenFileDialog);
        assert!(!state.file_menu_open);
        assert!(matches!(state.dialog, Some(Dialog::FileOpen { .. })));
    }

    /// Acceptance pipeline (bead qgis-3fz): load a shapefile, display it on
    /// the map canvas, apply a `.qml` style, and save a `.qgs` project —
    /// driven through the same message pipeline the GUI event loop uses.
    #[test]
    fn acceptance_load_display_style_save() {
        use qgis_style::symbols::CanvasGeometry;

        use crate::layer_renderer::render_layer;
        use crate::map_program::default_style_for_layer;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let shp_path = root
            .join("../provider-samples/france_parts.shp")
            .to_str()
            .unwrap()
            .to_string();
        let qml_path = root
            .join("../styles/polys.qml")
            .to_str()
            .unwrap()
            .to_string();
        let qgs_path = std::env::temp_dir().join("qgis-3fz-acceptance.qgs");
        let qgs_path = qgs_path.to_str().unwrap().to_string();

        let mut state = AppState::default();

        // 1. Load a shapefile through the file-open dialog path: open the
        //    dialog, enter the path, confirm — the same event sequence the
        //    GUI event loop produces.
        update(&mut state, Message::OpenFileDialog);
        update(&mut state, Message::FileDialogInputChanged(shp_path.clone()));
        update(&mut state, Message::FileDialogConfirm);
        assert_eq!(state.registry.len(), 1, "shapefile should load as one layer");
        let layer = state.registry.layer_at(0).unwrap().clone();
        assert_eq!(layer.name, "france_parts");
        assert!(!layer.features.is_empty(), "layer should have features");
        assert!(
            (state.camera.zoom - 3.0).abs() > f64::EPSILON,
            "camera should fit the loaded extent"
        );
        assert!(state.status.contains("Loaded"));

        // 2. Display the layer on the map canvas: the E13 rendering pipeline
        //    produces canvas geometry (fills/strokes) for the visible
        //    viewport. (The final `Frame::fill`/`stroke` calls are iced glue
        //    over the GPU renderer and run in the desktop app.)
        let viewport = (1024.0, 768.0);
        let camera = state.camera.with_viewport(viewport);
        let style = default_style_for_layer(&layer);
        let paths = render_layer(&layer, &style, &camera, viewport);
        assert!(
            paths
                .iter()
                .any(|p| matches!(p, CanvasGeometry::Fill { .. } | CanvasGeometry::Stroke { .. })),
            "visible features should produce canvas geometry"
        );

        // 3. Apply a .qml style through the style-assignment dialog path:
        //    open the dialog for the selected layer, enter the path, confirm.
        update(&mut state, Message::AssignStyleDialog);
        update(&mut state, Message::StyleDialogInputChanged(qml_path.clone()));
        update(&mut state, Message::StyleDialogConfirm);
        assert!(
            state.styles.contains_key(&0),
            "style should be applied to layer 0"
        );
        assert!(state.status.contains("Style applied"));

        // 4. Save a .qgs project through the project-save dialog path: open
        //    the dialog, enter the path, confirm.
        update(&mut state, Message::SaveProjectDialog);
        update(&mut state, Message::ProjectPathChanged(qgs_path.clone()));
        update(&mut state, Message::ProjectSaveConfirm);
        let saved = std::fs::read_to_string(&qgs_path).expect("project file should be written");
        assert!(saved.contains("<qgis"));
        assert!(saved.contains("france_parts"));
        assert!(
            saved.contains("Categorized") || saved.contains("Single"),
            "style reference should be saved"
        );
        assert!(saved.contains("<view"));
        let _ = std::fs::remove_file(&qgs_path);
    }
}
