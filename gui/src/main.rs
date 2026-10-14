//! QGIS GUI application binary (bead qgis-3fz, E14).
//!
//! Launches the iced desktop application with a map canvas, layer panel,
//! menu bar, and file/style/project dialogs.
//!
//! ```bash
//! cargo run --bin qgis-gui
//! ```

use qgis_gui::app;

fn main() -> iced::Result {
    iced::application("QGIS", app::update, app::view)
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 800.0),
            ..iced::window::Settings::default()
        })
        .run()
}
