//! The `layerpanel` suite — E14.3 agent validation for the layer panel.
//!
//! It drives the real `qgis-gui` application state headlessly (no windowing
//! system): load two spatial files through the file-open dialog path, then
//! assert the panel shows both layers and that toggling visibility, reordering,
//! and deleting all work on the panel. An LLM agent runs
//! `./verify agent-layerpanel` and reads the pass/fail report with no human in
//! the loop — exactly the E16 promise.
//!
//! The panel contents come from `qgis_gui::layer_panel::rows` (never a
//! re-implementation here), so the harness validates the merged widget logic.

use std::path::{Path, PathBuf};

use qgis_gui::app::{update, AppState, Message};
use qgis_gui::layer_panel::rows;

use crate::CheckFn;

/// The two fixtures the acceptance criterion names ("load 2 files"): a
/// shapefile (whose layer name is the file stem) and a GeoJSON file.
const SHAPEFILE: &str = "france_parts.shp";
const GEOJSON: &str = "points.geojson";

/// The panel row names the two fixtures produce.
const SHAPEFILE_LAYER: &str = "france_parts";
const GEOJSON_LAYER: &str = "points.geojson";

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../provider-samples")
        .join(name)
}

/// Loads one file through the same file-open dialog sequence the GUI event
/// loop produces.
fn load(state: &mut AppState, path: &Path) -> Result<(), String> {
    let path = path
        .to_str()
        .ok_or_else(|| format!("fixture path is not UTF-8: {path:?}"))?
        .to_string();
    let _ = update(state, Message::OpenFileDialog);
    let _ = update(state, Message::FileDialogInputChanged(path.clone()));
    let _ = update(state, Message::FileDialogConfirm);
    if state.dialog.is_some() {
        return Err(format!(
            "loading {path:?} raised a dialog instead of loading a layer"
        ));
    }
    Ok(())
}

/// A fresh application state with both acceptance fixtures loaded.
fn two_layer_state() -> Result<AppState, String> {
    let mut state = AppState::default();
    load(&mut state, &sample(SHAPEFILE))?;
    load(&mut state, &sample(GEOJSON))?;
    if state.registry.len() != 2 {
        return Err(format!(
            "expected 2 loaded layers, got {}",
            state.registry.len()
        ));
    }
    Ok(state)
}

pub fn checks() -> Vec<(String, CheckFn)> {
    vec![
        (
            "panel-shows-loaded-layers".into(),
            Box::new(|| panel_shows_loaded_layers().into()),
        ),
        (
            "panel-toggle-visibility".into(),
            Box::new(|| panel_toggle_visibility().into()),
        ),
        (
            "panel-reorder".into(),
            Box::new(|| panel_reorder().into()),
        ),
        (
            "panel-delete".into(),
            Box::new(|| panel_delete().into()),
        ),
        (
            "panel-style-indicator".into(),
            Box::new(|| panel_style_indicator().into()),
        ),
    ]
}

/// Acceptance: after loading two files, both appear in the panel, in load
/// order, and both start visible.
fn panel_shows_loaded_layers() -> Result<(), String> {
    let state = two_layer_state()?;
    let panel = rows(&state);
    if panel.len() != 2 {
        return Err(format!("panel shows {} rows, expected 2", panel.len()));
    }
    let names: Vec<&str> = panel.iter().map(|r| r.name.as_str()).collect();
    if names != vec![SHAPEFILE_LAYER, GEOJSON_LAYER] {
        return Err(format!(
            "panel names {names:?}, expected [{SHAPEFILE_LAYER:?}, {GEOJSON_LAYER:?}]"
        ));
    }
    let indices: Vec<usize> = panel.iter().map(|r| r.index).collect();
    if indices != vec![0, 1] {
        return Err(format!("panel indices {indices:?}, expected [0, 1]"));
    }
    if !panel.iter().all(|r| r.visible) {
        return Err("newly loaded layers should start visible".into());
    }
    Ok(())
}

/// Acceptance: toggling a layer's visibility flips its checkbox, and only it.
fn panel_toggle_visibility() -> Result<(), String> {
    let mut state = two_layer_state()?;

    let _ = update(&mut state, Message::LayerVisibilityToggled(0));
    let panel = rows(&state);
    if panel[0].visible {
        return Err("layer 0 should be hidden after toggling".into());
    }
    if !panel[1].visible {
        return Err("toggling layer 0 must not hide layer 1".into());
    }

    let _ = update(&mut state, Message::LayerVisibilityToggled(0));
    if !rows(&state)[0].visible {
        return Err("toggling again should restore layer 0 visibility".into());
    }
    Ok(())
}

/// Acceptance: reordering moves the layer in the panel, in both directions.
fn panel_reorder() -> Result<(), String> {
    let mut state = two_layer_state()?;

    // Move the second row up: [france_parts, points] -> [points, france_parts].
    let _ = update(&mut state, Message::LayerMoveUp(1));
    let names: Vec<String> = rows(&state).into_iter().map(|r| r.name).collect();
    if names != vec![GEOJSON_LAYER.to_string(), SHAPEFILE_LAYER.to_string()] {
        return Err(format!("after move-up, panel order is {names:?}"));
    }

    // Move it back down.
    let _ = update(&mut state, Message::LayerMoveDown(0));
    let names: Vec<String> = rows(&state).into_iter().map(|r| r.name).collect();
    if names != vec![SHAPEFILE_LAYER.to_string(), GEOJSON_LAYER.to_string()] {
        return Err(format!("after move-down, panel order is {names:?}"));
    }
    Ok(())
}

/// Acceptance: deleting removes the row and the remaining row keeps a valid
/// panel index.
fn panel_delete() -> Result<(), String> {
    let mut state = two_layer_state()?;

    let _ = update(&mut state, Message::LayerDeleted(0));
    let panel = rows(&state);
    if panel.len() != 1 {
        return Err(format!("panel shows {} rows after delete, expected 1", panel.len()));
    }
    if panel[0].name != GEOJSON_LAYER {
        return Err(format!(
            "surviving row is {:?}, expected {GEOJSON_LAYER:?}",
            panel[0].name
        ));
    }
    if panel[0].index != 0 {
        return Err(format!("surviving row index {} != 0", panel[0].index));
    }
    Ok(())
}

/// Acceptance: a layer with a style shows the filled style indicator; a layer
/// without one shows the empty indicator.
fn panel_style_indicator() -> Result<(), String> {
    let mut state = two_layer_state()?;
    if rows(&state).iter().any(|r| r.styled) {
        return Err("no layer should show a style indicator before styling".into());
    }

    // Styling acts on the selected layer, so select the first row first.
    let _ = update(&mut state, Message::LayerSelected(0));

    let qml = Path::new(env!("CARGO_MANIFEST_DIR")).join("../styles/polys.qml");
    let qml = qml
        .to_str()
        .ok_or_else(|| "style fixture path is not UTF-8".to_string())?
        .to_string();
    let _ = update(&mut state, Message::AssignStyleDialog);
    let _ = update(&mut state, Message::StyleDialogInputChanged(qml));
    let _ = update(&mut state, Message::StyleDialogConfirm);

    let panel = rows(&state);
    if !panel[0].styled {
        return Err("layer 0 should show the filled indicator after styling".into());
    }
    if panel[1].styled {
        return Err("layer 1 was not styled but shows the filled indicator".into());
    }
    Ok(())
}
