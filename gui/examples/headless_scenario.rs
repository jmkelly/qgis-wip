//! Fluent headless scenario demo (bead qgis-2lq).
//!
//! Scripts a realistic GUI workflow — open a shapefile, assert the layer
//! appears, assign a QML style, then drive the viewport (zoom, pan, reset) —
//! with the chainable [`Scenario`] builder. It finishes by running a
//! deliberately wrong assertion and printing the panic the builder raises, so
//! the message-history output a broken scenario produces is visible.
//!
//! ```bash
//! cargo run -p qgis-gui --example headless_scenario
//! ```

#![forbid(unsafe_code)]

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use qgis_gui::app::Message;
use qgis_gui::headless::Scenario;

/// Resolves a path relative to this crate's manifest directory, so the example
/// runs from any working directory.
fn sample(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(rel)
        .to_str()
        .expect("manifest directory is valid UTF-8")
        .to_string()
}

fn main() {
    let shp = sample("../provider-samples/france_parts.shp");
    let qml = sample("../styles/polys.qml");

    println!("scenario: load a shapefile, style it, and drive the viewport");

    let mut scenario = Scenario::new();
    scenario
        .expect_layer_count(0)
        .expect_status_contains("Ready")
        // Open the file through the same dialog messages the GUI emits.
        .send(Message::OpenFileDialog)
        .expect_dialog_present()
        .send(Message::FileDialogInputChanged(shp))
        .send(Message::FileDialogConfirm)
        .expect_dialog_absent()
        .expect_layer_count(1)
        .expect_layer_name(0, "france_parts")
        .expect_status_contains("Loaded");

    let feature_count = scenario.state().registry.layer_at(0).unwrap().features.len();
    let fitted_zoom = scenario.state().camera.zoom;
    println!("  loaded france_parts: {feature_count} features, fitted zoom {fitted_zoom:.4}");

    // Assign a QML style to the layer.
    scenario
        .send(Message::AssignStyleDialog)
        .send(Message::StyleDialogInputChanged(qml))
        .send(Message::StyleDialogConfirm)
        .expect_dialog_absent()
        .expect_status_contains("Style applied")
        .expect_state(|state| state.styles.contains_key(&0));
    println!("  style applied to layer 0");

    // Viewport control through the same dispatch path as the GUI.
    let zoom_before = scenario.state().camera.zoom;
    scenario
        .send(Message::ZoomIn)
        .expect_state(|state| state.camera.zoom > zoom_before);
    println!("  zoom in: {zoom_before:.4} -> {:.4}", scenario.state().camera.zoom);

    let center_before = scenario.state().camera.center;
    scenario
        .send(Message::Pan { dx: 120.0, dy: -40.0 })
        .expect_state(|state| state.camera.center != center_before);
    println!("  pan: center {:?} -> {:?}", center_before, scenario.state().camera.center);

    scenario
        .send(Message::ResetCamera)
        .expect_state(|state| (state.camera.zoom - fitted_zoom).abs() < 1e-9);
    println!("  reset: fitted zoom {:.4}", scenario.state().camera.zoom);

    // Deleting the layer empties the registry.
    scenario.send(Message::LayerDeleted(0)).expect_layer_count(0);
    println!("  deleted layer: registry empty");

    println!("scenario passed after {} messages\n", scenario.history().len());

    demonstrate_failure();
}

/// Runs a deliberately wrong assertion and prints the panic the builder raises,
/// proving that failures carry the full message history.
fn demonstrate_failure() {
    println!("scenario: show what a failed assertion looks like");

    let mut scenario = Scenario::new();
    scenario
        .send(Message::OpenFileDialog)
        .send(Message::FileDialogInputChanged("missing.shp".into()))
        .send(Message::FileDialogConfirm);

    let payload = catch_unwind(AssertUnwindSafe(|| {
        scenario.expect_layer_count(42);
    }))
    .expect_err("the assertion is expected to fail");

    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "<non-string panic payload>".to_string());
    println!("{message}");
}
