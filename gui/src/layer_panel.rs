//! Layer panel sidebar: list, toggle visibility, reorder, delete layers.
//!
//! Displays all layers from the [`LayerRegistry`] as interactive rows with
//! visibility checkboxes, style indicators, and reordering/deletion controls.

use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{alignment, Color, Element, Length};

use qgis_core::Layer;

use crate::app::{AppState, Message};

/// Creates the layer panel sidebar widget.
pub fn view(state: &AppState) -> Element<Message> {
    let mut rows = Vec::new();
    for (idx, layer) in state.registry.iter().enumerate() {
        rows.push(layer_row(state, idx, layer));
    }

    column![
        text("Layers").size(16),
        scrollable(column(rows).spacing(4)).height(Length::Fill),
        text(format!("{} layer(s)", state.registry.len())).size(12),
    ]
    .padding(8)
    .spacing(8)
    .into()
}

/// A single layer row in the panel.
fn layer_row<'a>(state: &'a AppState, idx: usize, layer: &'a Layer) -> Element<'a, Message> {
    let is_selected = state.selected_layer == Some(idx);
    let style_indicator = if state.styles.contains_key(&idx) {
        "\u{25cf}" // filled circle = styled
    } else {
        "\u{25cb}" // empty circle = default style
    };

    let mut controls = row![
        checkbox("", layer.visible)
            .on_toggle(move |_| Message::LayerVisibilityToggled(idx)),
        button(text(&layer.name).width(Length::Fill))
            .style(button::text)
            .on_press(Message::LayerSelected(idx)),
        text(style_indicator).size(14),
    ];

    if state.registry.len() > 1 {
        controls = controls.push(button(text("\u{2191}")).on_press(Message::LayerMoveUp(idx)));
        controls = controls.push(button(text("\u{2193}")).on_press(Message::LayerMoveDown(idx)));
    }
    controls = controls.push(button(text("\u{00d7}")).on_press(Message::LayerDeleted(idx)));

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
