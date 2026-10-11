//! Map canvas proof of concept for the QGIS Rust port (bead qgis-rez.4).
//!
//! Renders a procedural 100x100 grid of diamond cells (10,000 polygons) on an
//! iced `Canvas` widget, with:
//!
//! - mouse-drag panning,
//! - scroll-wheel zoom centered on the cursor,
//! - a frame-rate (FPS) counter overlaid on the canvas, refreshed every second.
//!
//! The viewport transform is managed by `qgis_crs::Camera`: longitude/latitude
//! (degrees) is projected to world radians with `equirectangular_project` and
//! then mapped to screen pixels by the camera. See ADR-0005.
//!
//! Run with:
//!
//! ```bash
//! cargo run -p qgis-gui --example map_canvas_poc
//! ```

#![forbid(unsafe_code)]

use std::cell::Cell;
use std::time::{Duration, Instant};

use iced::event::Status as EventStatus;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Event, Frame, Geometry, Path};
use iced::{alignment, Color, Element, Font, Length, Pixels, Point, Rectangle, Size, Theme};

use qgis_crs::{equirectangular_project, Camera};

/// Grid dimensions: 100x100 cells cover the whole lon/lat range, so the POC
/// renders exactly 10,000 polygons as required by the bead.
const GRID_COLUMNS: i64 = 100;
const GRID_ROWS: i64 = 100;

/// Longitude span of one cell in degrees (360 / 100).
const CELL_WIDTH_DEG: f64 = 360.0 / GRID_COLUMNS as f64;
/// Latitude span of one cell in degrees (180 / 100).
const CELL_HEIGHT_DEG: f64 = 180.0 / GRID_ROWS as f64;

/// Zoom limits for the camera. The web-mapping zoom range in `qgis-crs` is
/// unbounded, so the POC clamps to a sensible window.
const MIN_ZOOM: f64 = 0.5;
const MAX_ZOOM: f64 = 24.0;

/// Zoom step applied per scroll line (or per 100 scrolled pixels).
const ZOOM_STEP: f64 = 0.5;

/// How often the displayed FPS value is recomputed.
const FPS_REFRESH: Duration = Duration::from_secs(1);

/// Frame-rate counter.
///
/// `iced` renders frames by calling `Program::draw`, which only receives an
/// immutable reference to the canvas state, so the counter uses interior
/// mutability. All access happens on the main thread, hence `Cell`.
#[derive(Debug)]
struct FpsCounter {
    /// Frames rendered since the last refresh.
    frames: Cell<u32>,
    /// When the last refresh happened.
    last_refresh: Cell<Instant>,
    /// Frames per second at the last refresh.
    fps: Cell<f64>,
}

impl FpsCounter {
    fn new() -> Self {
        Self {
            frames: Cell::new(0),
            last_refresh: Cell::new(Instant::now()),
            fps: Cell::new(0.0),
        }
    }

    /// Records one rendered frame and refreshes the FPS reading once
    /// [`FPS_REFRESH`] has elapsed since the last refresh.
    fn record_frame(&self) {
        let frames = self.frames.get() + 1;
        self.frames.set(frames);

        let last = self.last_refresh.get();
        let elapsed = last.elapsed();
        if elapsed >= FPS_REFRESH {
            self.fps.set(frames as f64 / elapsed.as_secs_f64());
            self.frames.set(0);
            self.last_refresh.set(Instant::now());
        }
    }

    /// The most recently computed frames-per-second value.
    fn fps(&self) -> f64 {
        self.fps.get()
    }
}

/// State of the map canvas program.
#[derive(Debug)]
struct MapState {
    /// Viewport transform: world radians <-> screen pixels.
    camera: Camera,
    /// Whether a left-button drag is in progress.
    panning: bool,
    /// Cursor position (screen pixels) at the last drag event.
    last_cursor: Point,
    /// Frame-rate counter for the HUD overlay.
    fps: FpsCounter,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            // Start zoomed out far enough to see a good chunk of the grid.
            camera: Camera::new((0.0, 0.0), 3.0, (1024.0, 768.0)),
            panning: false,
            last_cursor: Point::ORIGIN,
            fps: FpsCounter::new(),
        }
    }
}

/// The canvas program: an empty marker type, all state lives in [`MapState`].
#[derive(Debug, Default)]
struct MapCanvas;

impl canvas::Program<Message> for MapCanvas {
    type State = MapState;

    fn update(
        &self,
        state: &mut MapState,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (EventStatus, Option<Message>) {
        // Keep the camera's viewport in sync with the widget bounds so the
        // world->screen transform matches the actual drawing area.
        state.camera.viewport = (bounds.width as f64, bounds.height as f64);

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
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.panning {
                    // Pan in world space: shift the center by the world-space
                    // distance the cursor moved, so the map sticks to the
                    // cursor at every zoom level.
                    let before = state
                        .camera
                        .screen_to_world((state.last_cursor.x as f64, state.last_cursor.y as f64));
                    let after = state
                        .camera
                        .screen_to_world((position.x as f64, position.y as f64));
                    state.camera.center.0 += before.0 - after.0;
                    state.camera.center.1 += before.1 - after.1;
                    state.last_cursor = position;
                    (EventStatus::Captured, None)
                } else {
                    (EventStatus::Ignored, None)
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                // Scroll up (positive y) zooms in, scroll down zooms out.
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / 100.0,
                };
                if lines != 0.0 {
                    if let Some(position) = cursor.position_in(bounds) {
                        let screen = (position.x as f64, position.y as f64);
                        // Keep the world point under the cursor fixed while
                        // the zoom level changes.
                        let before = state.camera.screen_to_world(screen);
                        state.camera.zoom = (state.camera.zoom + lines as f64 * ZOOM_STEP)
                            .clamp(MIN_ZOOM, MAX_ZOOM);
                        let after = state.camera.screen_to_world(screen);
                        state.camera.center.0 += before.0 - after.0;
                        state.camera.center.1 += before.1 - after.1;
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
        state: &MapState,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let viewport = (bounds.width as f64, bounds.height as f64);
        let camera = state.camera.with_viewport(viewport);

        // Count this frame for the HUD (interior mutability: draw is &self).
        state.fps.record_frame();

        let mut frame = Frame::new(renderer, bounds.size());

        // Ocean-style backdrop.
        frame.fill(
            &Path::rectangle(Point::ORIGIN, bounds.size()),
            Color::from_rgb(0.04, 0.07, 0.11),
        );

        // Viewport culling: only cells whose lon/lat range intersects the
        // visible world rectangle are turned into geometry.
        let top_left = camera.screen_to_world((0.0, 0.0));
        let bottom_right = camera.screen_to_world(viewport);
        let lon_min = top_left.0.to_degrees().min(bottom_right.0.to_degrees());
        let lon_max = top_left.0.to_degrees().max(bottom_right.0.to_degrees());
        let lat_min = top_left.1.to_degrees().min(bottom_right.1.to_degrees());
        let lat_max = top_left.1.to_degrees().max(bottom_right.1.to_degrees());

        let col_min = (((lon_min + 180.0) / CELL_WIDTH_DEG).floor() as i64).clamp(0, GRID_COLUMNS);
        let col_max = (((lon_max + 180.0) / CELL_WIDTH_DEG).ceil() as i64).clamp(0, GRID_COLUMNS);
        let row_min = (((90.0 - lat_max) / CELL_HEIGHT_DEG).floor() as i64).clamp(0, GRID_ROWS);
        let row_max = (((90.0 - lat_min) / CELL_HEIGHT_DEG).ceil() as i64).clamp(0, GRID_ROWS);

        for col in col_min..col_max {
            for row in row_min..row_max {
                // Cell bounds in degrees; row 0 is the northernmost strip.
                let lon0 = -180.0 + col as f64 * CELL_WIDTH_DEG;
                let lon1 = lon0 + CELL_WIDTH_DEG;
                let lat1 = 90.0 - row as f64 * CELL_HEIGHT_DEG;
                let lat0 = lat1 - CELL_HEIGHT_DEG;
                let lon_mid = (lon0 + lon1) / 2.0;
                let lat_mid = (lat0 + lat1) / 2.0;

                // Diamond: vertices at the edge midpoints of the cell.
                let diamond = [
                    (lon_mid, lat1),
                    (lon1, lat_mid),
                    (lon_mid, lat0),
                    (lon0, lat_mid),
                ];
                let path = Path::new(|builder| {
                    for (i, (lon, lat)) in diamond.into_iter().enumerate() {
                        // lon/lat degrees -> world radians -> screen pixels.
                        let (world_x, world_y) = equirectangular_project(lon, lat);
                        let (sx, sy) = camera.world_to_screen((world_x, world_y));
                        if i == 0 {
                            builder.move_to(Point::new(sx as f32, sy as f32));
                        } else {
                            builder.line_to(Point::new(sx as f32, sy as f32));
                        }
                    }
                    builder.close();
                });

                // Colour by position so panning/zooming is visible: hue
                // sweeps with column, lightness with row.
                let hue = (col as f64 / GRID_COLUMNS as f64 * 0.75
                    + row as f64 / GRID_ROWS as f64 * 0.25)
                    % 1.0;
                let (r, g, b) = hue_to_rgb(hue);
                frame.fill(&path, Color::from_rgb(r, g, b));
            }
        }

        // HUD: frame rate plus camera state, drawn as a canvas overlay.
        let (center_lon, center_lat) =
            qgis_crs::equirectangular_unproject(camera.center.0, camera.center.1);
        let hud = format!(
            "FPS: {:.1}  |  zoom: {:.1}  |  center: {:.3}, {:.3}  |  drag to pan, scroll to zoom",
            state.fps.fps(),
            camera.zoom,
            center_lon,
            center_lat
        );
        frame.fill_text(iced::widget::canvas::Text {
            content: hud,
            position: Point::new(12.0, 10.0),
            size: Pixels(16.0),
            color: Color::WHITE,
            font: Font::DEFAULT,
            horizontal_alignment: alignment::Horizontal::Left,
            vertical_alignment: alignment::Vertical::Top,
            ..Default::default()
        });

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &MapState,
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

/// Maps a hue in `[0, 1)` to an `(r, g, b)` triple in `[0, 1]`.
///
/// Standard HSV-style rainbow ramp with full saturation and mid lightness;
/// pure arithmetic, no lookup tables.
fn hue_to_rgb(hue: f64) -> (f32, f32, f32) {
    let sector = (hue * 6.0).rem_euclid(6.0);
    let fraction = sector.fract();
    let (r, g, b) = match sector as i64 {
        0 => (1.0, fraction, 0.0),
        1 => (1.0 - fraction, 1.0, 0.0),
        2 => (0.0, 1.0, fraction),
        3 => (0.0, 1.0 - fraction, 1.0),
        4 => (fraction, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - fraction),
    };
    (r as f32, g as f32, b as f32)
}

/// The POC application: a single full-window map canvas.
#[derive(Debug, Default)]
struct MapCanvasPoc;

/// Message type of the POC application.
///
/// All map interaction (pan, zoom) is handled inside the canvas program, so
/// the application itself never receives messages.
#[derive(Debug, Clone, Copy)]
enum Message {}

/// Application update: no-op, the canvas program owns all interaction.
fn update(_state: &mut MapCanvasPoc, _message: Message) {}

/// Application view: the map canvas fills the window.
fn view<'a>(_state: &'a MapCanvasPoc) -> Element<'a, Message> {
    Canvas::new(MapCanvas)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn main() -> iced::Result {
    iced::application("QGIS map canvas POC", update, view)
        .window(iced::window::Settings {
            size: Size::new(1024.0, 768.0),
            ..iced::window::Settings::default()
        })
        .run()
}
