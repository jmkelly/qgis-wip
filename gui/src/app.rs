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
use qgis_style::StyleModel;

use crate::dialogs;
use crate::layer_panel;
use crate::layer_renderer::{camera_for_extent, load_shapefile_as_layer};
use crate::map_program::MapProgram;
use crate::style_dialog;

/// Main application state.
#[derive(Debug)]
pub struct AppState {
    /// All loaded layers.
    pub registry: LayerRegistry,
    /// Per-layer styles (layer index -> parsed QML style).
    pub styles: HashMap<usize, StyleModel>,
    /// Viewport transform for the map canvas.
    pub camera: Camera,
    /// Project-level CRS.
    pub project_crs: SrsId,
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
            project_crs: SrsId::from_epsg(4326),
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
    /// A `.qml` file was picked from the style file list.
    StyleFilePicked(String),
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
        Message::StyleFilePicked(path) => {
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
            if let Err(e) = crate::project::open_project(state, &path) {
                state.dialog = Some(Dialog::Error { message: e });
            }
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
        // The style-assignment dialog lives in its own module (bead qgis-yjr)
        // so it can grow a real `.qml` file picker without touching the other
        // modal dialogs.
        if let Dialog::StyleAssign { layer_index, path } = dialog {
            let layer_name = state
                .registry
                .layer_at(*layer_index)
                .map(|layer| layer.name.as_str());
            return style_dialog::view(layer_name, path);
        }
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
        Ok(mut layer) => {
            let extent = layer.extent();
            layer.source = Some(path.to_string());
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
pub(crate) fn load_geojson_as_layer(path: &str) -> Result<Layer, String> {
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
///
/// A layer index with no layer is rejected by name (never recorded as a
/// dangling style), and the `.qml` path is validated before parsing so a
/// non-style file is refused rather than silently ignored. On success the
/// status names the renderer the style declares (single symbol, categorized,
/// or graduated) so the user can see all three renderer types are supported.
fn assign_style(state: &mut AppState, layer_index: usize, path: &str) {
    if state.registry.layer_at(layer_index).is_none() {
        state.dialog = Some(Dialog::Error {
            message: format!("No layer at index {} to style", layer_index),
        });
        return;
    }

    match style_dialog::parse_style(path) {
        Ok(model) => {
            let summary = style_dialog::describe_renderer(&model.renderer);
            state.styles.insert(layer_index, model);
            if let Some(layer) = state.registry.layer_at_mut(layer_index) {
                layer.style_ref = Some(path.to_string());
            }
            state.status = format!(
                "Style applied to layer {}: {} [{}]",
                layer_index, path, summary
            );
        }
        Err(e) => {
            state.dialog = Some(Dialog::Error { message: e });
        }
    }
}

/// Saves a `.qgs` project file.
///
/// Serializes the project CRS, layer entries (data source, CRS, visibility,
/// opacity, style reference), and the map viewport. See [`crate::project`].
fn save_project(state: &AppState, path: &str) -> Result<(), String> {
    crate::project::save_project(state, path)
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
            json_to_value(&serde_json::Value::from(std::f64::consts::PI)),
            Value::Float(std::f64::consts::PI)
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
        let _ = update(&mut state, Message::LayerVisibilityToggled(0));
        assert!(!state.registry.layer_at(0).unwrap().visible);
        let _ = update(&mut state, Message::LayerVisibilityToggled(0));
        assert!(state.registry.layer_at(0).unwrap().visible);
    }

    #[test]
    fn layer_delete_removes_and_adjusts_selection() {
        let mut state = AppState::default();
        state.registry.add_layer(Layer::new("a", SrsId::from_epsg(4326)));
        state.registry.add_layer(Layer::new("b", SrsId::from_epsg(4326)));
        state.registry.add_layer(Layer::new("c", SrsId::from_epsg(4326)));
        state.selected_layer = Some(1);

        let _ = update(&mut state, Message::LayerDeleted(1));
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
        let _ = update(&mut state, Message::LayerMoveUp(2));
        assert_eq!(state.registry.layer_at(1).unwrap().name, "c");
        assert_eq!(state.registry.layer_at(2).unwrap().name, "b");

        // Move "c" (now index 1) down -> swap back with "b"
        let _ = update(&mut state, Message::LayerMoveDown(1));
        assert_eq!(state.registry.layer_at(1).unwrap().name, "b");
        assert_eq!(state.registry.layer_at(2).unwrap().name, "c");

        // Move up at index 0 is a no-op
        let _ = update(&mut state, Message::LayerMoveUp(0));
        assert_eq!(state.registry.layer_at(0).unwrap().name, "a");

        // Move down at last index is a no-op
        let _ = update(&mut state, Message::LayerMoveDown(2));
        assert_eq!(state.registry.layer_at(2).unwrap().name, "c");
    }

    #[test]
    fn cancel_clears_dialog() {
        let mut state = AppState {
            dialog: Some(Dialog::FileOpen { path: "test.shp".into() }),
            ..Default::default()
        };
        let _ = update(&mut state, Message::Cancel);
        assert!(state.dialog.is_none());
    }

    #[test]
    fn file_dialog_input_updates_path() {
        let mut state = AppState {
            dialog: Some(Dialog::FileOpen { path: String::new() }),
            ..Default::default()
        };
        let _ = update(&mut state, Message::FileDialogInputChanged("roads.shp".into()));
        match &state.dialog {
            Some(Dialog::FileOpen { path }) => assert_eq!(path, "roads.shp"),
            other => panic!("expected FileOpen, got {:?}", other),
        }
    }

    #[test]
    fn style_dialog_input_updates_path() {
        let mut state = AppState {
            dialog: Some(Dialog::StyleAssign {
                layer_index: 0,
                path: String::new(),
            }),
            ..Default::default()
        };
        let _ = update(&mut state, Message::StyleDialogInputChanged("polys.qml".into()));
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
        let _ = update(&mut state, Message::AssignStyleDialog);
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
        let _ = update(&mut state, Message::AssignStyleDialog);
        assert_eq!(state.selected_layer, Some(0));
        assert!(matches!(state.dialog, Some(Dialog::StyleAssign { .. })));
    }

    #[test]
    fn assign_style_dialog_warns_when_no_layers() {
        let mut state = AppState::default();
        let _ = update(&mut state, Message::AssignStyleDialog);
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

        let _ = update(&mut state, Message::ToggleFileMenu);
        assert!(state.file_menu_open);

        let _ = update(&mut state, Message::ToggleFileMenu);
        assert!(!state.file_menu_open);

        // Selecting an item closes the menu.
        let _ = update(&mut state, Message::ToggleFileMenu);
        assert!(state.file_menu_open);
        let _ = update(&mut state, Message::OpenFileDialog);
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
        let _ = update(&mut state, Message::OpenFileDialog);
        let _ = update(&mut state, Message::FileDialogInputChanged(shp_path.clone()));
        let _ = update(&mut state, Message::FileDialogConfirm);
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
        let _ = update(&mut state, Message::AssignStyleDialog);
        let _ = update(&mut state, Message::StyleDialogInputChanged(qml_path.clone()));
        let _ = update(&mut state, Message::StyleDialogConfirm);
        assert!(
            state.styles.contains_key(&0),
            "style should be applied to layer 0"
        );
        assert!(state.status.contains("Style applied"));

        // 4. Save a .qgs project through the project-save dialog path: open
        //    the dialog, enter the path, confirm.
        let _ = update(&mut state, Message::SaveProjectDialog);
        let _ = update(&mut state, Message::ProjectPathChanged(qgs_path.clone()));
        let _ = update(&mut state, Message::ProjectSaveConfirm);
        let saved = std::fs::read_to_string(&qgs_path).expect("project file should be written");
        assert!(saved.contains("<qgis"));
        assert!(saved.contains("france_parts"));
        assert!(
            saved.contains("polys.qml"),
            "the layer's style reference should be saved"
        );
        assert!(saved.contains("<view"));
        let _ = std::fs::remove_file(&qgs_path);
    }

    /// Bead qgis-mzw acceptance (E15): save a project with multiple styled
    /// layers and restore it with all state intact — layer names, order, CRS,
    /// visibility, opacity and features; per-layer styles; the project CRS;
    /// and the viewport. Driven through the same message pipeline the GUI
    /// event loop uses.
    #[test]
    fn acceptance_project_save_load_round_trip() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let shp_path = root
            .join("../provider-samples/france_parts.shp")
            .to_str()
            .unwrap()
            .to_string();
        let geojson_path = root
            .join("../provider-samples/points.geojson")
            .to_str()
            .unwrap()
            .to_string();
        let polys_qml = root
            .join("../styles/polys.qml")
            .to_str()
            .unwrap()
            .to_string();
        let points_qml = root
            .join("../styles/points.qml")
            .to_str()
            .unwrap()
            .to_string();
        let qgs_path = std::env::temp_dir().join("qgis-mzw-roundtrip.qgs");
        let qgs_path = qgs_path.to_str().unwrap().to_string();

        // ---- build a project with two styled layers ----
        let mut src = AppState::default();

        let _ = update(&mut src, Message::OpenFileDialog);
        let _ = update(&mut src, Message::FileDialogInputChanged(shp_path.clone()));
        let _ = update(&mut src, Message::FileDialogConfirm);
        let _ = update(&mut src, Message::OpenFileDialog);
        let _ = update(&mut src, Message::FileDialogInputChanged(geojson_path.clone()));
        let _ = update(&mut src, Message::FileDialogConfirm);
        assert_eq!(src.registry.len(), 2, "two layers should load");

        src.selected_layer = Some(0);
        let _ = update(&mut src, Message::AssignStyleDialog);
        let _ = update(&mut src, Message::StyleDialogInputChanged(polys_qml.clone()));
        let _ = update(&mut src, Message::StyleDialogConfirm);

        src.selected_layer = Some(1);
        let _ = update(&mut src, Message::AssignStyleDialog);
        let _ = update(&mut src, Message::StyleDialogInputChanged(points_qml.clone()));
        let _ = update(&mut src, Message::StyleDialogConfirm);
        assert_eq!(src.styles.len(), 2, "both layers should be styled");

        // Distinguish the viewport and per-layer state from the defaults.
        src.camera = Camera::new((0.5, 0.25), 7.5, (1024.0, 768.0));
        src.project_crs = SrsId::from_epsg(3857);
        let _ = update(&mut src, Message::LayerVisibilityToggled(1));
        src.registry.layer_at_mut(1).unwrap().opacity = 0.5;

        // ---- save ----
        let _ = update(&mut src, Message::SaveProjectDialog);
        let _ = update(&mut src, Message::ProjectPathChanged(qgs_path.clone()));
        let _ = update(&mut src, Message::ProjectSaveConfirm);
        let saved = std::fs::read_to_string(&qgs_path).expect("project file should be written");
        assert!(saved.contains("<qgis"), "should be QGS XML");

        // ---- restore into a fresh application state ----
        let mut dst = AppState::default();
        let _ = update(&mut dst, Message::OpenProjectDialog);
        let _ = update(&mut dst, Message::ProjectPathChanged(qgs_path.clone()));
        let _ = update(&mut dst, Message::ProjectOpenConfirm);
        assert!(
            dst.dialog.is_none(),
            "opening a valid project should not error: {:?}",
            dst.dialog
        );

        // Layers: count, order, names, CRS, visibility, opacity, features, source.
        assert_eq!(dst.registry.len(), src.registry.len());
        for i in 0..src.registry.len() {
            let a = src.registry.layer_at(i).unwrap();
            let b = dst.registry.layer_at(i).unwrap();
            assert_eq!(a.name, b.name, "layer {i} name");
            assert_eq!(a.crs, b.crs, "layer {i} crs");
            assert_eq!(a.visible, b.visible, "layer {i} visibility");
            assert_eq!(a.opacity, b.opacity, "layer {i} opacity");
            assert_eq!(a.feature_count(), b.feature_count(), "layer {i} features");
            assert_eq!(a.source, b.source, "layer {i} source");
        }

        // Styles: identical parsed models, keyed by the same layer index.
        assert_eq!(dst.styles, src.styles, "styles should round-trip");

        // Project CRS.
        assert_eq!(dst.project_crs, src.project_crs, "project CRS");

        // Viewport: center + zoom.
        assert!(
            (dst.camera.zoom - src.camera.zoom).abs() < 1e-9,
            "zoom should round-trip"
        );
        assert!(
            (dst.camera.center.0 - src.camera.center.0).abs() < 1e-9,
            "viewport center x should round-trip"
        );
        assert!(
            (dst.camera.center.1 - src.camera.center.1).abs() < 1e-9,
            "viewport center y should round-trip"
        );

        let _ = std::fs::remove_file(&qgs_path);
    }

    // -----------------------------------------------------------------------
    // Bead qgis-yjr (E14.4): style assignment changes what the canvas renders
    // -----------------------------------------------------------------------

    /// The fill colors of a rendered geometry list, in order.
    fn fill_colors(geoms: &[qgis_style::symbols::CanvasGeometry]) -> Vec<qgis_style::Color> {
        geoms
            .iter()
            .filter_map(|g| match g {
                qgis_style::symbols::CanvasGeometry::Fill { color, .. } => Some(*color),
                _ => None,
            })
            .collect()
    }

    /// Loads a file through the file-open dialog and returns the fresh state.
    fn state_with_layer(path: &str) -> AppState {
        let mut state = AppState::default();
        let _ = update(&mut state, Message::OpenFileDialog);
        let _ = update(&mut state, Message::FileDialogInputChanged(path.to_string()));
        let _ = update(&mut state, Message::FileDialogConfirm);
        assert_eq!(state.registry.len(), 1, "one layer should load from {path}");
        state
    }

    /// Renders layer 0 with the style currently recorded for it (or the
    /// default style when none has been assigned).
    fn render_layer_0(state: &AppState) -> Vec<qgis_style::symbols::CanvasGeometry> {
        use crate::layer_renderer::render_layer;
        use crate::map_program::default_style_for_layer;

        let layer = state.registry.layer_at(0).unwrap();
        let viewport = (1024.0, 768.0);
        let camera = state.camera.with_viewport(viewport);
        let style = match state.styles.get(&0) {
            Some(s) => s.clone(),
            None => default_style_for_layer(layer),
        };
        render_layer(layer, &style, &camera, viewport)
    }

    /// Assigns `qml` to layer 0 through the style-assignment dialog path.
    fn assign_style_to_layer_0(state: &mut AppState, qml: &str) {
        let _ = update(state, Message::AssignStyleDialog);
        let _ = update(state, Message::StyleDialogInputChanged(qml.to_string()));
        let _ = update(state, Message::StyleDialogConfirm);
        assert!(
            state.styles.contains_key(&0),
            "style should be applied to layer 0 (status: {})",
            state.status
        );
    }

    /// Acceptance (bead qgis-yjr): load a point layer, assign `styles/points.qml`
    /// (categorized on `Class`), and prove the rendered markers change.
    #[test]
    fn assigning_categorized_style_changes_rendering() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let geojson = root
            .join("../provider-samples/points.geojson")
            .to_str()
            .unwrap()
            .to_string();
        let qml = root.join("../styles/points.qml").to_str().unwrap().to_string();

        let mut state = state_with_layer(&geojson);
        let before = render_layer_0(&state);
        assert!(!before.is_empty(), "the default style should render markers");

        assign_style_to_layer_0(&mut state, &qml);
        let after = render_layer_0(&state);

        assert!(
            !after.is_empty(),
            "the categorized style should render the matched classes"
        );
        assert_ne!(
            before, after,
            "rendered geometry must change after assigning a categorized style"
        );
        assert!(
            state.status.contains("categorized on \"Class\""),
            "status should name the renderer type and attribute: {}",
            state.status
        );
    }

    /// Acceptance (bead qgis-yjr): a single-symbol `.qml` changes the fill.
    #[test]
    fn assigning_single_symbol_style_changes_rendering() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let shp = root
            .join("../provider-samples/france_parts.shp")
            .to_str()
            .unwrap()
            .to_string();
        let qml = root
            .join("../styles/inverted_polys_single.qml")
            .to_str()
            .unwrap()
            .to_string();

        let mut state = state_with_layer(&shp);
        let before = fill_colors(&render_layer_0(&state));

        assign_style_to_layer_0(&mut state, &qml);
        let after = fill_colors(&render_layer_0(&state));

        assert_ne!(before, after);
        let single_teal = qgis_style::Color {
            r: 76,
            g: 143,
            b: 160,
            a: 255,
        };
        assert!(
            after.contains(&single_teal),
            "single symbol fill should be teal, got {after:?}"
        );
        assert!(
            state.status.contains("single symbol"),
            "status should name the renderer type: {}",
            state.status
        );
    }

    /// Acceptance (bead qgis-yjr): a graduated `.qml` changes the marker fill
    /// by classifying the numeric `Importance` attribute.
    #[test]
    fn assigning_graduated_style_changes_rendering() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let geojson = root
            .join("../provider-samples/points.geojson")
            .to_str()
            .unwrap()
            .to_string();
        let qml = root
            .join("../styles/points_graduated_symbol.qml")
            .to_str()
            .unwrap()
            .to_string();

        let mut state = state_with_layer(&geojson);
        // Graduated symbols are markers, not fills; compare the whole geometry.
        let before = render_layer_0(&state);

        assign_style_to_layer_0(&mut state, &qml);
        let after = render_layer_0(&state);

        assert_ne!(before, after, "graduated style must change the geometry");
        assert!(
            state.status.contains("graduated on \"Importance\""),
            "status should name the renderer type and attribute: {}",
            state.status
        );
    }

    /// Picking a `.qml` file from the dialog's file list fills the path input.
    #[test]
    fn style_file_picked_sets_dialog_path() {
        let mut state = AppState {
            dialog: Some(Dialog::StyleAssign {
                layer_index: 0,
                path: String::new(),
            }),
            ..Default::default()
        };
        let _ = update(
            &mut state,
            Message::StyleFilePicked("../styles/polys.qml".into()),
        );
        match &state.dialog {
            Some(Dialog::StyleAssign { path, .. }) => assert_eq!(path, "../styles/polys.qml"),
            other => panic!("expected StyleAssign, got {:?}", other),
        }
    }

    /// A style assigned to a layer index that does not exist is rejected by
    /// name: no dangling style entry, and an error dialog explains why.
    #[test]
    fn assign_style_rejects_missing_layer_by_name() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let qml = root.join("../styles/polys.qml").to_str().unwrap().to_string();

        let mut state = AppState {
            dialog: Some(Dialog::StyleAssign {
                layer_index: 3,
                path: qml,
            }),
            ..Default::default()
        };
        let _ = update(&mut state, Message::StyleDialogConfirm);

        assert!(
            state.styles.is_empty(),
            "no style should be recorded for a missing layer"
        );
        match &state.dialog {
            Some(Dialog::Error { message }) => {
                assert!(message.contains('3'), "error should name the layer: {message}");
            }
            other => panic!("expected an error dialog, got {:?}", other),
        }
    }

    /// A non-`.qml` path is rejected by name before parsing.
    #[test]
    fn assign_style_rejects_non_qml_by_name() {
        let mut state = AppState {
            dialog: Some(Dialog::StyleAssign {
                layer_index: 0,
                path: "../styles/not-a-style.txt".into(),
            }),
            ..Default::default()
        };
        state
            .registry
            .add_layer(Layer::new("a", SrsId::from_epsg(4326)));
        let _ = update(&mut state, Message::StyleDialogConfirm);

        assert!(state.styles.is_empty());
        match &state.dialog {
            Some(Dialog::Error { message }) => assert!(message.contains("qml"), "{message}"),
            other => panic!("expected an error dialog, got {:?}", other),
        }
    }
}
