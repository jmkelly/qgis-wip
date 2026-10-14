//! Map rendering pipeline example (bead qgis-kqf / qgis-33c, E13).
//!
//! Loads a vector layer (or synthesizes a representative test layer when given
//! `test.shp` and the file is not present on disk), parses a `.qml` style file,
//! culls features against the camera viewport via `qgis-spatial-index`, and
//! renders visible features into `CanvasGeometry` and `iced` canvas `Path`s.
//!
//! Pass `--window` to open an interactive `iced` canvas window displaying the
//! rendered layer with pan and zoom.
//!
//! ```bash
//! cargo run --example render_layer test.shp styles/polys.qml
//! ```

#![forbid(unsafe_code)]

use std::path::Path;

use iced::event::Status as EventStatus;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry as IcedGeometry, Path as IcedPath};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};
use qgis_core::{Feature, Layer, Value};
use qgis_crs::{Camera, SrsId};
use qgis_geometry::{Geometry, LineString, Point as GeoPoint, Polygon};
use qgis_gui::layer_renderer::{
    build_spatial_index, camera_for_extent, load_shapefile_as_layer, paint_layer, render_layer,
    render_layer_paths, visible_feature_indices_with_index,
};
use qgis_style::symbols::CanvasGeometry;
use qgis_style::{parse_qml_file, Renderer, StyleModel, Symbol};

/// Builds a synthetic layer with polygons, lines, and points when a non-existent
/// demo path such as `test.shp` is supplied on the command line.
fn synthetic_test_layer(name: &str, style: &StyleModel) -> Layer {
    let mut layer = Layer::new(name, SrsId::from_epsg(4326));
    let first_symbol = match &style.renderer {
        Renderer::Single(s) => Some(s),
        Renderer::Categorized { classes, .. } => classes.first().map(|(_, s)| s),
        Renderer::Graduated { ranges, .. } => ranges.first().map(|(_, _, s)| s),
    };

    match first_symbol {
        Some(Symbol::Point { .. }) => {
            for (id, (x, y, name_val, imp)) in [
                (1, (-2.0, -1.0, "Dam", 2)),
                (2, (0.0, 0.0, "Lake", 5)),
                (3, (2.0, 1.5, "Dam", 8)),
            ] {
                let mut f = Feature::new(id, Some(Geometry::Point(GeoPoint::new(x, y))));
                f.set_attribute("Name", Value::Text(name_val.into()));
                f.set_attribute("Importance", Value::Int(imp));
                f.set_attribute("Class", Value::Text("Jet".into()));
                layer.features.push(f);
            }
        }
        Some(Symbol::Line { .. }) => {
            for (id, coords, name_val) in [
                (1, vec![(-3.0, -2.0), (0.0, 0.0), (2.0, -1.0)], "Dam"),
                (2, vec![(-1.0, 1.0), (1.5, 2.5), (4.0, 2.0)], "Lake"),
            ] {
                let ls = LineString::new(
                    coords
                        .into_iter()
                        .map(|(x, y)| GeoPoint::new(x, y))
                        .collect(),
                );
                let mut f = Feature::new(id, Some(Geometry::LineString(ls)));
                f.set_attribute("Name", Value::Text(name_val.into()));
                f.set_attribute("Importance", Value::Int(5));
                layer.features.push(f);
            }
        }
        _ => {
            for (id, (x0, y0, x1, y1), name_val, imp) in [
                (1, (-4.0, -2.0, -1.0, 2.0), "Dam", 3),
                (2, (0.5, -1.5, 4.0, 2.5), "Lake", 7),
                (3, (-1.0, 3.0, 2.0, 5.0), "Dam", 9),
            ] {
                let poly = Polygon::new(vec![
                    GeoPoint::new(x0, y0),
                    GeoPoint::new(x1, y0),
                    GeoPoint::new(x1, y1),
                    GeoPoint::new(x0, y1),
                    GeoPoint::new(x0, y0),
                ]);
                let mut f = Feature::new(id, Some(Geometry::Polygon(poly)));
                f.set_attribute("Name", Value::Text(name_val.into()));
                f.set_attribute("Importance", Value::Int(imp));
                layer.features.push(f);
            }
        }
    }

    layer
}

/// Interactive canvas state when `--window` is passed.
#[derive(Debug, Clone)]
struct LayerCanvasApp {
    layer: Layer,
    style: StyleModel,
    camera: Camera,
}

#[derive(Debug)]
struct CanvasInteractionState {
    camera: Option<Camera>,
    panning: bool,
    last_cursor: Point,
}

impl Default for CanvasInteractionState {
    fn default() -> Self {
        Self {
            camera: None,
            panning: false,
            last_cursor: Point::ORIGIN,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Message {}

impl canvas::Program<Message> for LayerCanvasApp {
    type State = CanvasInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (EventStatus, Option<Message>) {
        let cam = state.camera.get_or_insert(self.camera);
        cam.viewport = (bounds.width as f64, bounds.height as f64);

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
                let before =
                    cam.screen_to_world((state.last_cursor.x as f64, state.last_cursor.y as f64));
                let after = cam.screen_to_world((position.x as f64, position.y as f64));
                cam.center.0 += before.0 - after.0;
                cam.center.1 += before.1 - after.1;
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
                        let screen = (pos.x as f64, pos.y as f64);
                        let before = cam.screen_to_world(screen);
                        cam.zoom = (cam.zoom + lines as f64 * 0.5).clamp(-2.0, 24.0);
                        let after = cam.screen_to_world(screen);
                        cam.center.0 += before.0 - after.0;
                        cam.center.1 += before.1 - after.1;
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
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<IcedGeometry<iced::Renderer>> {
        let viewport = (bounds.width as f64, bounds.height as f64);
        let cam = state.camera.unwrap_or(self.camera).with_viewport(viewport);
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill(
            &IcedPath::rectangle(Point::ORIGIN, bounds.size()),
            Color::from_rgb(0.96, 0.96, 0.98),
        );
        paint_layer(&mut frame, &self.layer, &self.style, &cam, viewport);
        vec![frame.into_geometry()]
    }
}

fn update(_app: &mut LayerCanvasApp, _msg: Message) {}

fn view(app: &LayerCanvasApp) -> Element<'_, Message> {
    Canvas::new(app)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: render_layer <layer.shp> <style.qml> [--window]");
        std::process::exit(1);
    }
    let layer_path = &args[0];
    let qml_path = &args[1];
    let open_window = args.iter().any(|a| a == "--window" || a == "--gui");

    let style = match parse_qml_file(qml_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error parsing QML style {}: {}", qml_path, e);
            std::process::exit(1);
        }
    };

    let layer = if Path::new(layer_path).exists() {
        match load_shapefile_as_layer(layer_path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error loading shapefile {}: {}", layer_path, e);
                std::process::exit(1);
            }
        }
    } else {
        synthetic_test_layer(layer_path, &style)
    };

    let viewport = (1024.0, 768.0);
    let extent = layer.extent().unwrap_or((-10.0, -10.0, 10.0, 10.0));
    let camera = camera_for_extent(extent, viewport);

    let index = build_spatial_index(&layer);
    let visible = visible_feature_indices_with_index(&index, &camera, viewport);
    let geometries = render_layer(&layer, &style, &camera, viewport);
    let paths = render_layer_paths(&layer, &style, &camera, viewport);

    let fills = geometries
        .iter()
        .filter(|g| matches!(g, CanvasGeometry::Fill { .. }))
        .count();
    let strokes = geometries
        .iter()
        .filter(|g| matches!(g, CanvasGeometry::Stroke { .. }))
        .count();

    println!("layer:      {} ({} features)", layer.name, layer.feature_count());
    println!("extent:     {:?}", extent);
    println!("style:      {}", qml_path);
    println!(
        "viewport:   {}x{} (zoom {:.2})",
        viewport.0 as u32, viewport.1 as u32, camera.zoom
    );
    println!(
        "culled:     {}/{} visible features in spatial index",
        visible.len(),
        index.len()
    );
    println!(
        "rendered:   {} canvas primitives ({} fills, {} strokes, {} iced paths)",
        geometries.len(),
        fills,
        strokes,
        paths.len()
    );

    if open_window {
        let initial_app = LayerCanvasApp {
            layer,
            style,
            camera,
        };
        let _ = iced::application("QGIS Layer Renderer", update, view)
            .window(iced::window::Settings {
                size: Size::new(viewport.0 as f32, viewport.1 as f32),
                ..iced::window::Settings::default()
            })
            .run_with(move || (initial_app.clone(), iced::Task::none()));
    }
}
