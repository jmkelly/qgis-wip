//! Map canvas program: renders all visible layers with pan/zoom interaction.
//!
//! The `MapProgram` holds an [`LayerRegistry`] and per-layer styles,
//! rendering each visible layer through the E13 rendering pipeline
//! ([`paint_layer`] → spatial-index culling → symbol classification →
//! canvas geometry). Pan and zoom interaction is handled internally via
//! [`MapInteractionState`].

use std::collections::HashMap;
use std::sync::Arc;

use iced::event::Status as EventStatus;
use iced::mouse;
use iced::widget::canvas::{self, Event, Frame, Geometry as CanvasGeometry, Path};
use iced::{Color, Point, Rectangle, Renderer, Theme};

use qgis_core::{Layer, LayerRegistry};
use qgis_crs::Camera;
use qgis_geometry::Geometry;
use qgis_style::{Color as StyleColor, Renderer as StyleRenderer, StyleModel, Symbol};

use crate::app::Message;
use crate::layer_renderer::paint_layer;

/// Canvas program that renders all visible layers from a layer registry.
#[derive(Debug, Clone)]
pub struct MapProgram {
    /// All layers (shared reference).
    pub registry: Arc<LayerRegistry>,
    /// Per-layer styles (layer index -> parsed QML style).
    pub styles: Arc<HashMap<usize, StyleModel>>,
    /// Initial camera (used on first frame or when generation changes).
    pub initial_camera: Camera,
    /// Generation counter: incremented when the viewport should reset
    /// (e.g., when a new layer is loaded).
    pub generation: u64,
}

/// Interaction state for the map canvas (pan/zoom).
#[derive(Debug, Default)]
pub struct MapInteractionState {
    /// Current camera (initialized from `initial_camera` on first frame).
    pub camera: Option<Camera>,
    /// The generation at which the camera was last initialized.
    pub seen_generation: u64,
    /// Whether a left-button drag is in progress.
    pub panning: bool,
    /// Cursor position at the last drag event.
    pub last_cursor: Point,
}

impl canvas::Program<Message> for MapProgram {
    type State = MapInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (EventStatus, Option<Message>) {
        // Reset camera if this is the first frame or the generation changed
        // (e.g., new layer loaded -> fit viewport to new extent).
        if state.camera.is_none() || state.seen_generation != self.generation {
            state.camera = Some(self.initial_camera);
            state.seen_generation = self.generation;
        }

        if let Some(cam) = &mut state.camera {
            cam.viewport = (bounds.width as f64, bounds.height as f64);
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.panning = true;
                state.last_cursor = cursor.position_in(bounds).unwrap_or(Point::ORIGIN);
                (EventStatus::Captured, None)
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.panning = false;
                (EventStatus::Captured, None)
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.panning => {
                if let Some(cam) = &mut state.camera {
                    let before = cam.screen_to_world((
                        state.last_cursor.x as f64,
                        state.last_cursor.y as f64,
                    ));
                    let after =
                        cam.screen_to_world((position.x as f64, position.y as f64));
                    cam.center.0 += before.0 - after.0;
                    cam.center.1 += before.1 - after.1;
                }
                state.last_cursor = position;
                (EventStatus::Captured, None)
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / 100.0,
                };
                if lines != 0.0 {
                    if let Some(pos) = cursor.position_in(bounds) {
                        if let Some(cam) = &mut state.camera {
                            let screen = (pos.x as f64, pos.y as f64);
                            let before = cam.screen_to_world(screen);
                            cam.zoom = (cam.zoom + lines as f64 * 0.5).clamp(-2.0, 24.0);
                            let after = cam.screen_to_world(screen);
                            cam.center.0 += before.0 - after.0;
                            cam.center.1 += before.1 - after.1;
                        }
                    }
                    return (EventStatus::Captured, None);
                }
                (EventStatus::Ignored, None)
            }
            _ => (EventStatus::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<CanvasGeometry<Renderer>> {
        let viewport = (bounds.width as f64, bounds.height as f64);
        let cam = state.camera.unwrap_or(self.initial_camera).with_viewport(viewport);

        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill(
            &Path::rectangle(Point::ORIGIN, bounds.size()),
            Color::from_rgb(0.96, 0.96, 0.98),
        );

        for (idx, layer) in self.registry.iter().enumerate() {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }
            let style = match self.styles.get(&idx) {
                Some(s) => s.clone(),
                None => default_style_for_layer(layer),
            };
            paint_layer(&mut frame, layer, &style, &cam, viewport);
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.panning {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        }
    }
}

/// Creates a default style for a layer based on its first feature's geometry type.
pub(crate) fn default_style_for_layer(layer: &Layer) -> StyleModel {
    let symbol = match layer.features.first().and_then(|f| f.geometry.as_ref()) {
        Some(Geometry::Point(_)) | Some(Geometry::MultiPoint(_)) => Symbol::Point {
            shape: "circle".into(),
            size: 6.0,
            color: StyleColor {
                r: 100,
                g: 150,
                b: 200,
                a: 255,
            },
            outline: StyleColor { r: 0, g: 0, b: 0, a: 255 },
        },
        Some(Geometry::LineString(_)) | Some(Geometry::MultiLineString(_)) => Symbol::Line {
            color: StyleColor {
                r: 50,
                g: 100,
                b: 200,
                a: 255,
            },
            width: 2.0,
        },
        _ => Symbol::Polygon {
            fill: StyleColor {
                r: 150,
                g: 200,
                b: 150,
                a: 255,
            },
            outline: StyleColor { r: 0, g: 0, b: 0, a: 255 },
            outline_width: 0.5,
            style: "solid".into(),
        },
    };
    StyleModel {
        renderer: StyleRenderer::Single(symbol),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_core::Feature;
    use qgis_geometry::{LineString, Point as GeoPoint, Polygon};

    #[test]
    fn default_style_for_point_layer_uses_point_symbol() {
        let mut layer = Layer::new("pts", qgis_crs::SrsId::from_epsg(4326));
        layer
            .features
            .push(Feature::new(1, Some(Geometry::Point(GeoPoint::new(0.0, 0.0)))));

        let style = default_style_for_layer(&layer);
        match &style.renderer {
            StyleRenderer::Single(Symbol::Point { shape, size, .. }) => {
                assert_eq!(shape, "circle");
                assert_eq!(*size, 6.0);
            }
            other => panic!("expected point symbol, got {:?}", other),
        }
    }

    #[test]
    fn default_style_for_line_layer_uses_line_symbol() {
        let mut layer = Layer::new("roads", qgis_crs::SrsId::from_epsg(4326));
        layer.features.push(Feature::new(
            1,
            Some(Geometry::LineString(LineString::from_2d_coords(&[
                (0.0, 0.0),
                (1.0, 1.0),
            ]))),
        ));

        let style = default_style_for_layer(&layer);
        match &style.renderer {
            StyleRenderer::Single(Symbol::Line { width, .. }) => {
                assert_eq!(*width, 2.0);
            }
            other => panic!("expected line symbol, got {:?}", other),
        }
    }

    #[test]
    fn default_style_for_polygon_layer_uses_polygon_symbol() {
        let mut layer = Layer::new("areas", qgis_crs::SrsId::from_epsg(4326));
        layer.features.push(Feature::new(
            1,
            Some(Geometry::Polygon(Polygon::new(vec![
                GeoPoint::new(0.0, 0.0),
                GeoPoint::new(1.0, 0.0),
                GeoPoint::new(1.0, 1.0),
            ]))),
        ));

        let style = default_style_for_layer(&layer);
        match &style.renderer {
            StyleRenderer::Single(Symbol::Polygon { outline_width, .. }) => {
                assert_eq!(*outline_width, 0.5);
            }
            other => panic!("expected polygon symbol, got {:?}", other),
        }
    }

    #[test]
    fn default_style_for_empty_layer_falls_back_to_polygon() {
        let layer = Layer::new("empty", qgis_crs::SrsId::from_epsg(4326));
        let style = default_style_for_layer(&layer);
        assert!(matches!(
            &style.renderer,
            StyleRenderer::Single(Symbol::Polygon { .. })
        ));
    }
}
