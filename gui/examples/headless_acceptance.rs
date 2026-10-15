//! Headless acceptance scenarios (bead qgis-ggs).
//!
//! Ports the manual GUI acceptance flow from bead qgis-3fz — load a shapefile,
//! render the layer, apply a `.qml` style, save a `.qgs` project — plus the
//! layer-panel, error-handling, and viewport workflows into automated
//! headless scenarios driven through the fluent [`Scenario`] builder. Every
//! scenario runs without an iced window or wgpu: it dispatches the same
//! [`Message`] values the GUI event loop produces and asserts on the resulting
//! state, so the whole acceptance suite executes in milliseconds.
//!
//! ```bash
//! cargo run -p qgis-gui --example headless_acceptance
//! ```

#![forbid(unsafe_code)]

use std::f64::consts::PI;
use std::path::Path;
use std::time::Instant;

use qgis_crs::Camera;
use qgis_gui::app::{Dialog, Message};
use qgis_gui::headless::Scenario;
use qgis_gui::{
    build_spatial_index, camera_for_extent, render_layer, visible_feature_indices,
    visible_feature_indices_with_index,
};
use qgis_style::symbols::CanvasGeometry;

/// Resolves a path relative to this crate's manifest directory, so the example
/// runs from any working directory.
fn sample(rel: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(rel)
        .to_str()
        .expect("manifest directory is valid UTF-8")
        .to_string()
}

/// Opens `path` through the file-open dialog message sequence the GUI emits.
fn open_file(scenario: &mut Scenario, path: &str) {
    scenario
        .send(Message::OpenFileDialog)
        .send(Message::FileDialogInputChanged(path.to_string()))
        .send(Message::FileDialogConfirm);
}

/// Scenario 1: load a shapefile and verify the layer appears with the correct
/// feature count and extent (the qgis-3fz acceptance flow, step 1).
fn scenario_load_shapefile() {
    let shp = sample("../provider-samples/france_parts.shp");
    let mut scenario = Scenario::new();
    open_file(&mut scenario, &shp);
    scenario
        .expect_dialog_absent()
        .expect_layer_count(1)
        .expect_layer_name(0, "france_parts")
        .expect_status_contains("Loaded")
        .expect_state_that("the layer holds the file's 4 features", |state| {
            state
                .registry
                .layer_at(0)
                .is_some_and(|layer| layer.feature_count() == 4)
        })
        .expect_state_that("the layer extent matches the shapefile bounds", |state| {
            let Some((xmin, ymin, xmax, ymax)) =
                state.registry.layer_at(0).and_then(|layer| layer.extent())
            else {
                return false;
            };
            (xmin - (-5.1326269186972695)).abs() < 1e-9
                && (ymin - 46.279190985775415).abs() < 1e-9
                && (xmax - 3.1179289078928605).abs() < 1e-9
                && (ymax - 49.72647410719435).abs() < 1e-9
        });

    let features = scenario.state().registry.layer_at(0).unwrap().feature_count();
    let zoom = scenario.state().camera.zoom;
    println!("  loaded france_parts: {features} features, fitted zoom {zoom:.4}");
}

/// Scenario 2: load a shapefile, assign a QML style, and verify the style
/// model is attached to the layer and renders (qgis-3fz steps 2-3: the
/// rendered layer produces canvas geometry for the styled viewport).
///
/// The style is a single-symbol polygon fill (`polys_shapeburst_symbol.qml`):
/// the categorized `polys.qml` only matches features whose `Name` is `Dam` or
/// `Lake`, which the `france_parts` sample does not contain, so it would
/// classify every feature as unmatched and render nothing.
fn scenario_style_assignment() {
    let shp = sample("../provider-samples/france_parts.shp");
    let qml = sample("../styles/polys_shapeburst_symbol.qml");
    let mut scenario = Scenario::new();
    open_file(&mut scenario, &shp);
    scenario
        .send(Message::AssignStyleDialog)
        .send(Message::StyleDialogInputChanged(qml.clone()))
        .send(Message::StyleDialogConfirm)
        .expect_dialog_absent()
        .expect_status_contains("Style applied")
        .expect_state_that("a style model is attached to layer 0", |state| {
            state.styles.contains_key(&0)
        })
        .expect_state_that("the layer records the style reference", |state| {
            state.registry.layer_at(0).is_some_and(|layer| {
                layer.style_ref.as_deref() == Some(qml.as_str())
            })
        });

    // Render the styled layer through the same pipeline the map canvas uses
    // (qgis-3fz step 2: the rendered layer produces canvas geometry).
    let state = scenario.state();
    let layer = state.registry.layer_at(0).unwrap();
    let style = state.styles.get(&0).unwrap();
    let geometry = render_layer(layer, style, &state.camera, state.camera.viewport);
    assert!(
        geometry
            .iter()
            .any(|g| matches!(g, CanvasGeometry::Fill { .. } | CanvasGeometry::Stroke { .. })),
        "rendered styled layer should produce canvas geometry"
    );
    println!(
        "  style applied to layer 0: {} canvas geometries rendered",
        geometry.len()
    );
}

/// Scenario 3: load a shapefile, save a `.qgs` project, reopen it, and verify
/// layers, styles, and the viewport are restored (qgis-3fz step 4).
fn scenario_project_roundtrip() {
    let shp = sample("../provider-samples/france_parts.shp");
    let qml = sample("../styles/polys.qml");
    let project_dir =
        std::env::temp_dir().join(format!("qgis-ggs-acceptance-{}", std::process::id()));
    std::fs::create_dir_all(&project_dir).expect("temp project dir");
    let qgs = project_dir.join("acceptance.qgs");
    let qgs = qgs.to_str().expect("utf-8 path").to_string();

    let mut scenario = Scenario::new();
    open_file(&mut scenario, &shp);
    scenario
        .send(Message::AssignStyleDialog)
        .send(Message::StyleDialogInputChanged(qml.clone()))
        .send(Message::StyleDialogConfirm)
        .expect_status_contains("Style applied");
    let fitted_zoom = scenario.state().camera.zoom;

    scenario
        .send(Message::SaveProjectDialog)
        .send(Message::ProjectPathChanged(qgs.clone()))
        .send(Message::ProjectSaveConfirm)
        .expect_dialog_absent()
        .expect_status_contains("Project saved");

    // Reopen the project through the project-open dialog path in a fresh
    // application state.
    let mut reopened = Scenario::new();
    reopened
        .send(Message::OpenProjectDialog)
        .send(Message::ProjectPathChanged(qgs.clone()))
        .send(Message::ProjectOpenConfirm)
        .expect_dialog_absent()
        .expect_status_contains("Project loaded")
        .expect_layer_count(1)
        .expect_layer_name(0, "france_parts")
        .expect_state_that("features are reloaded from the source", |state| {
            state
                .registry
                .layer_at(0)
                .is_some_and(|layer| layer.feature_count() == 4)
        })
        .expect_state_that("the style model is restored", |state| {
            state.styles.contains_key(&0)
        })
        .expect_state_that("the style reference is restored", |state| {
            state.registry.layer_at(0).is_some_and(|layer| {
                layer.style_ref.as_deref() == Some(qml.as_str())
            })
        })
        .expect_state_that("the viewport zoom is restored", |state| {
            (state.camera.zoom - fitted_zoom).abs() < 1e-9
        });

    let _ = std::fs::remove_file(&qgs);
    let _ = std::fs::remove_dir_all(&project_dir);
    println!("  project roundtrip: layer, style, and viewport restored");
}

/// Scenario 4: multi-layer panel workflow — load two files, reorder, toggle
/// visibility, delete, and verify the registry state after each step.
fn scenario_multi_layer() {
    let shp = sample("../provider-samples/france_parts.shp");
    let points = sample("../provider-samples/points.geojson");
    let mut scenario = Scenario::new();
    open_file(&mut scenario, &shp);
    open_file(&mut scenario, &points);
    scenario
        .expect_layer_count(2)
        .expect_layer_name(0, "france_parts")
        .expect_layer_name(1, "points");

    // Reorder: moving `points` up puts it above `france_parts`.
    scenario
        .send(Message::LayerMoveUp(1))
        .expect_layer_name(0, "points")
        .expect_layer_name(1, "france_parts");

    // Toggle visibility of the layer now at index 0.
    scenario
        .send(Message::LayerVisibilityToggled(0))
        .expect_state_that("layer 0 (points) is hidden", |state| {
            state
                .registry
                .layer_at(0)
                .is_some_and(|layer| !layer.visible)
        })
        .expect_state_that("layer 1 (france_parts) is still visible", |state| {
            state
                .registry
                .layer_at(1)
                .is_some_and(|layer| layer.visible)
        });

    // Delete `france_parts` (index 1): the hidden `points` layer remains.
    scenario
        .send(Message::LayerDeleted(1))
        .expect_status_contains("deleted")
        .expect_layer_count(1)
        .expect_layer_name(0, "points")
        .expect_state_that("the remaining layer keeps its hidden state", |state| {
            state
                .registry
                .layer_at(0)
                .is_some_and(|layer| !layer.visible)
        });
    println!("  multi-layer: reorder, toggle, delete verified");
}

/// Scenario 5: error handling — a nonexistent file opens the error dialog, and
/// an invalid style path degrades gracefully (error dialog, no panic, no
/// style attached).
fn scenario_error_handling() {
    let shp = sample("../provider-samples/france_parts.shp");
    let mut scenario = Scenario::new();

    open_file(&mut scenario, "/nonexistent/dir/file.shp");
    scenario
        .expect_layer_count(0)
        .expect_state_that("a missing file opens the error dialog", |state| {
            matches!(state.dialog, Some(Dialog::Error { .. }))
        });
    scenario.send(Message::DismissError).expect_dialog_absent();

    open_file(&mut scenario, &shp);
    scenario
        .send(Message::AssignStyleDialog)
        .send(Message::StyleDialogInputChanged("/nonexistent/style.qml".into()))
        .send(Message::StyleDialogConfirm)
        .expect_state_that("an invalid style path opens the error dialog", |state| {
            matches!(state.dialog, Some(Dialog::Error { .. }))
        })
        .expect_state_that("no style is attached after a failed assignment", |state| {
            state.styles.is_empty()
        })
        .expect_layer_count(1);
    scenario.send(Message::DismissError).expect_dialog_absent();
    println!("  error handling: missing file and invalid style degrade gracefully");
}

/// Scenario 6: zoom in increases the camera zoom by exactly 1.25x; zoom out
/// decreases it by the same factor.
fn scenario_zoom() {
    let mut scenario = Scenario::new();
    let start = scenario.state().camera.zoom;

    scenario
        .send(Message::ZoomIn)
        .expect_state_that("zoom in multiplies zoom by 1.25", |state| {
            (state.camera.zoom - start * 1.25).abs() < 1e-12
        })
        .send(Message::ZoomIn)
        .expect_state_that("a second zoom in multiplies by 1.25 again", |state| {
            (state.camera.zoom - start * 1.25 * 1.25).abs() < 1e-12
        })
        .send(Message::ZoomOut)
        .expect_state_that("zoom out divides zoom by 1.25", |state| {
            (state.camera.zoom - start * 1.25).abs() < 1e-12
        })
        .send(Message::ZoomOut)
        .expect_state_that("a second zoom out returns to the start zoom", |state| {
            (state.camera.zoom - start).abs() < 1e-12
        });
    println!(
        "  zoom: {start:.4} -> {:.4}",
        scenario.state().camera.zoom
    );
}

/// Scenario 7: pan shifts the camera center by the expected world-space
/// delta: a screen delta `(dx, dy)` moves the center by `(-dx, +dy) / k` with
/// `k = 256 * 2^zoom / 2π` (the equirectangular world scale per pixel).
fn scenario_pan() {
    let mut scenario = Scenario::new();
    let before = scenario.state().camera;
    let k = 256.0 * 2.0_f64.powf(before.zoom) / (2.0 * PI);
    let (dx, dy) = (100.0, 50.0);
    let expected = (before.center.0 - dx / k, before.center.1 + dy / k);

    scenario
        .send(Message::Pan { dx, dy })
        .expect_state_that("pan shifts center by (-dx, +dy) / k", |state| {
            (state.camera.center.0 - expected.0).abs() < 1e-12
                && (state.camera.center.1 - expected.1).abs() < 1e-12
        })
        .expect_state_that("pan leaves the zoom unchanged", |state| {
            state.camera.zoom == before.zoom
        });
    println!(
        "  pan: center ({:.6}, {:.6}) -> ({:.6}, {:.6})",
        before.center.0,
        before.center.1,
        scenario.state().camera.center.0,
        scenario.state().camera.center.1
    );
}

/// Scenario 8: SetCamera jumps to an exact camera; ResetCamera fits the loaded
/// layer extent and bumps the viewport generation.
fn scenario_set_and_reset_camera() {
    let shp = sample("../provider-samples/france_parts.shp");
    let mut scenario = Scenario::new();

    let exact = Camera::new((0.5, -0.25), 7.5, (800.0, 600.0));
    scenario
        .send(Message::SetCamera(exact))
        .expect_state_that("SetCamera jumps to the exact camera", |state| {
            state.camera == exact
        });

    open_file(&mut scenario, &shp);
    let extent = scenario
        .state()
        .registry
        .layer_at(0)
        .and_then(|layer| layer.extent());
    let generation = scenario.state().generation;

    scenario
        .send(Message::SetCamera(Camera::new(
            (0.0, 0.0),
            1.0,
            (1024.0, 768.0),
        )))
        .send(Message::ResetCamera)
        .expect_state_that("ResetCamera fits the layer extent", |state| {
            extent.is_some_and(|e| state.camera == camera_for_extent(e, state.camera.viewport))
        })
        .expect_state_that("ResetCamera bumps the viewport generation", |state| {
            state.generation == generation + 1
        });
    println!(
        "  set/reset: camera at zoom {:.4} after reset",
        scenario.state().camera.zoom
    );
}

/// Scenario 9: zoom + render — the visible feature count changes with the
/// viewport, proving spatial-index culling works headlessly: the fitted camera
/// shows every feature, and a camera zoomed 8 steps in shows a strict subset.
fn scenario_zoom_culling() {
    let shp = sample("../provider-samples/france_parts.shp");
    let mut scenario = Scenario::new();
    open_file(&mut scenario, &shp);

    // The fitted camera (set by the file-open path) shows every feature.
    scenario.expect_state_that(
        "the fitted viewport shows every feature",
        |state| {
            let Some(layer) = state.registry.layer_at(0) else {
                return false;
            };
            visible_feature_indices(layer, &state.camera, state.camera.viewport).len()
                == layer.feature_count()
        },
    );

    // Zoom 8 steps in: the viewport spans roughly 1/256 of the fitted degrees,
    // so only the features whose bounding boxes intersect the narrow view are
    // returned — the spatial index does the culling.
    let fitted = scenario.state().camera;
    scenario
        .send(Message::SetCamera(Camera::new(
            fitted.center,
            fitted.zoom + 8.0,
            fitted.viewport,
        )))
        .expect_state_that(
            "zooming in culls features through the spatial index",
            |state| {
                let Some(layer) = state.registry.layer_at(0) else {
                    return false;
                };
                let index = build_spatial_index(layer);
                let visible =
                    visible_feature_indices_with_index(&index, &state.camera, state.camera.viewport)
                        .len();
                visible >= 1 && visible < layer.feature_count()
            },
        );

    let layer = scenario.state().registry.layer_at(0).unwrap();
    let index = build_spatial_index(layer);
    let visible =
        visible_feature_indices_with_index(&index, &scenario.state().camera, fitted.viewport).len();
    println!("  zoom + render: {}/{} features visible after zoom-in", visible, layer.feature_count());
}

fn main() {
    let scenarios: [(&str, fn()); 9] = [
        ("1. load shapefile: feature count and extent", scenario_load_shapefile),
        ("2. assign QML style: style model attached + renders", scenario_style_assignment),
        ("3. save .qgs, reopen: layers and styles restored", scenario_project_roundtrip),
        (
            "4. multi-layer: reorder, toggle visibility, delete",
            scenario_multi_layer,
        ),
        ("5. error handling: missing file, invalid style", scenario_error_handling),
        ("6. zoom in/out: camera zoom changes", scenario_zoom),
        ("7. pan: camera center world-space delta", scenario_pan),
        ("8. SetCamera / ResetCamera: exact and fitted", scenario_set_and_reset_camera),
        (
            "9. zoom + render: spatial index culling",
            scenario_zoom_culling,
        ),
    ];

    println!("headless acceptance scenarios (bead qgis-ggs)");
    for (name, run) in scenarios {
        let start = Instant::now();
        run();
        println!("  {name}: passed in {:?}", start.elapsed());
    }
    println!("all {} scenarios passed (headless: no window, no wgpu)", scenarios.len());
}
