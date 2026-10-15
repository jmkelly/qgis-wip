//! Layer panel sidebar: list, toggle visibility, reorder, delete layers.
//!
//! Displays all layers from the [`LayerRegistry`] as interactive rows with
//! visibility checkboxes, style indicators, and reordering/deletion controls.
//!
//! The panel's contents are produced by [`rows`], a pure function over the
//! application state, so the widget can be asserted headlessly (no windowing
//! system) — see the tests below and the `./verify agent-layerpanel` gate.

use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{alignment, Color, Element, Length};

use crate::app::{AppState, Message};

/// One row of the layer panel, decoupled from `iced` so its contents can be
/// asserted without a windowing system.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRow {
    /// The layer's index in the [`LayerRegistry`], i.e. its position in the panel.
    pub index: usize,
    /// The layer's display name.
    pub name: String,
    /// Whether the layer is currently visible (the checkbox state).
    pub visible: bool,
    /// Whether a style has been assigned (filled vs. empty style indicator).
    pub styled: bool,
    /// Whether this row is the selected layer.
    pub selected: bool,
}

/// Builds the panel's rows from the application state.
///
/// This is the single source of truth for what the panel shows: [`view`]
/// renders exactly these rows, in registry order.
pub fn rows(state: &AppState) -> Vec<PanelRow> {
    state
        .registry
        .iter()
        .enumerate()
        .map(|(index, layer)| PanelRow {
            index,
            name: layer.name.clone(),
            visible: layer.visible,
            styled: state.styles.contains_key(&index),
            selected: state.selected_layer == Some(index),
        })
        .collect()
}

/// Creates the layer panel sidebar widget.
pub fn view(state: &AppState) -> Element<'_, Message> {
    let panel_rows = rows(state);
    let layer_count = panel_rows.len();
    let widgets: Vec<Element<'static, Message>> = panel_rows
        .into_iter()
        .map(|panel_row| layer_row(layer_count, panel_row))
        .collect();

    column![
        text("Layers").size(16),
        scrollable(column(widgets).spacing(4)).height(Length::Fill),
        text(format!("{} layer(s)", layer_count)).size(12),
    ]
    .padding(8)
    .spacing(8)
    .into()
}

/// A single layer row in the panel.
fn layer_row(layer_count: usize, panel_row: PanelRow) -> Element<'static, Message> {
    let idx = panel_row.index;
    let style_indicator = if panel_row.styled {
        "\u{25cf}" // filled circle = styled
    } else {
        "\u{25cb}" // empty circle = default style
    };

    let mut controls = row![
        checkbox("", panel_row.visible)
            .on_toggle(move |_| Message::LayerVisibilityToggled(idx)),
        button(text(panel_row.name).width(Length::Fill))
            .style(button::text)
            .on_press(Message::LayerSelected(idx)),
        text(style_indicator).size(14),
    ];

    if layer_count > 1 {
        controls = controls.push(button(text("\u{2191}")).on_press(Message::LayerMoveUp(idx)));
        controls = controls.push(button(text("\u{2193}")).on_press(Message::LayerMoveDown(idx)));
    }
    controls = controls.push(button(text("\u{00d7}")).on_press(Message::LayerDeleted(idx)));

    let is_selected = panel_row.selected;
    container(controls.spacing(4).align_y(alignment::Vertical::Center))
        .style(move |_theme| {
            container::Style {
                background: Some(if is_selected {
                    Color::from_rgb(0.9, 0.9, 1.0).into()
                } else {
                    Color::TRANSPARENT.into()
                }),
                ..Default::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::update;

    fn layer(name: &str) -> qgis_core::Layer {
        qgis_core::Layer::new(name, qgis_crs::SrsId::from_epsg(4326))
    }

    fn state_with(names: &[&str]) -> AppState {
        let mut state = AppState::default();
        for name in names {
            state.registry.add_layer(layer(name));
        }
        state
    }

    fn fixture(relative: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(relative)
            .to_str()
            .unwrap()
            .to_string()
    }

    /// Bead qgis-0wy acceptance: after loading two files through the file-open
    /// dialog path, both appear in the panel, in load order, both visible.
    #[test]
    fn acceptance_two_loaded_files_both_appear_in_panel() {
        let shp = fixture("../provider-samples/france_parts.shp");
        let geojson = fixture("../provider-samples/points.geojson");

        let mut state = AppState::default();
        for path in [shp, geojson] {
            let _ = update(&mut state, Message::OpenFileDialog);
            let _ = update(&mut state, Message::FileDialogInputChanged(path));
            let _ = update(&mut state, Message::FileDialogConfirm);
        }

        let panel = rows(&state);
        assert_eq!(panel.len(), 2, "both loaded files must appear in the panel");
        assert_eq!(panel[0].name, "france_parts");
        assert_eq!(panel[1].name, "points");
        assert_eq!(panel[0].index, 0);
        assert_eq!(panel[1].index, 1);
        assert!(
            panel.iter().all(|r| r.visible),
            "newly loaded layers start visible"
        );
        assert_eq!(
            state.selected_layer,
            Some(1),
            "the last loaded layer is selected"
        );
        assert!(panel[1].selected);
    }

    #[test]
    fn rows_follow_registry_order_and_index() {
        let state = state_with(&["a", "b", "c"]);
        let panel = rows(&state);
        let names: Vec<&str> = panel.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
        let indices: Vec<usize> = panel.iter().map(|r| r.index).collect();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn toggling_visibility_flips_the_panel_checkbox() {
        let mut state = state_with(&["a", "b"]);
        assert!(rows(&state)[0].visible);

        let _ = update(&mut state, Message::LayerVisibilityToggled(0));
        assert!(!rows(&state)[0].visible, "row 0 should now be hidden");
        assert!(rows(&state)[1].visible, "row 1 is unaffected");

        let _ = update(&mut state, Message::LayerVisibilityToggled(0));
        assert!(rows(&state)[0].visible, "toggling again restores visibility");
    }

    #[test]
    fn reordering_moves_the_row_and_keeps_it_selected() {
        let mut state = state_with(&["a", "b", "c"]);
        state.selected_layer = Some(2);

        let _ = update(&mut state, Message::LayerMoveUp(2));
        let panel = rows(&state);
        let names: Vec<&str> = panel.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a", "c", "b"]);
        assert!(panel[1].selected, "the moved row stays selected");

        let _ = update(&mut state, Message::LayerMoveDown(1));
        let panel = rows(&state);
        let names: Vec<&str> = panel.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn deleting_removes_the_row_and_shifts_the_rest() {
        let mut state = state_with(&["a", "b", "c"]);
        let _ = update(&mut state, Message::LayerDeleted(1));

        let panel = rows(&state);
        let names: Vec<&str> = panel.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["a", "c"]);
        let indices: Vec<usize> = panel.iter().map(|r| r.index).collect();
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn style_indicator_tracks_assigned_styles() {
        let shp = fixture("../provider-samples/france_parts.shp");
        let qml = fixture("../styles/polys.qml");

        let mut state = AppState::default();
        let _ = update(&mut state, Message::OpenFileDialog);
        let _ = update(&mut state, Message::FileDialogInputChanged(shp));
        let _ = update(&mut state, Message::FileDialogConfirm);
        assert!(!rows(&state)[0].styled, "no style yet -> empty indicator");

        let _ = update(&mut state, Message::AssignStyleDialog);
        let _ = update(&mut state, Message::StyleDialogInputChanged(qml));
        let _ = update(&mut state, Message::StyleDialogConfirm);
        assert!(rows(&state)[0].styled, "assigned style -> filled indicator");
    }
}
