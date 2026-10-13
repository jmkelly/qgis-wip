//! Python-level smoke tests for the `qgis` package.
//!
//! These exercise the real import paths (`from qgis.gui import ...`) through a
//! live CPython interpreter, which is what the 487k lines of existing QGIS
//! Python actually depend on. The Rust-side unit tests in `gui.rs` /
//! `three_d.rs` cover behaviour; these cover the module surface.

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Builds the `qgis` package exactly as the extension module does, installs it
/// in `sys.modules`, then runs `source` as Python.
fn run(source: &str) -> PyResult<()> {
    Python::attach(|py| with_installed_qgis(py, |py| run_module(py, source, "test_qgis_pyo3")))
}

/// Like [`run`], but expects `source` to raise and returns the exception type name.
fn run_expecting_error(source: &str) -> PyResult<String> {
    Python::attach(|py| {
        with_installed_qgis(py, |py| match run_module(py, source, "expected_error") {
            Ok(()) => Err(pyo3::exceptions::PyAssertionError::new_err(
                "expected the snippet to raise, but it succeeded",
            )),
            Err(err) => Ok(err.get_type(py).name()?.to_string()),
        })
    })
}

/// Registers the `qgis` package in `sys.modules`, then calls `body`.
fn with_installed_qgis<R>(
    py: Python<'_>,
    body: impl FnOnce(Python<'_>) -> PyResult<R>,
) -> PyResult<R> {
    let package = PyModule::new(py, "qgis")?;
    qgis_pyo3::qgis(&package)?;

    let modules: Bound<'_, PyDict> = PyModule::import(py, "sys")?
        .getattr("modules")?
        .cast_into()?;
    modules.set_item("qgis", &package)?;

    body(py)
}

/// Compiles and runs `source` as a module named `name`.
fn run_module(py: Python<'_>, source: &str, name: &str) -> PyResult<()> {
    let code = std::ffi::CString::new(source).expect("test source must be a valid C string");
    let file = std::ffi::CString::new(format!("{name}.py")).expect("valid file name");
    let module_name = std::ffi::CString::new(name).expect("valid module name");
    // `from_code` executes the module body; any exception it raises is returned
    // as a `PyErr`, which is what these tests assert on.
    PyModule::from_code(py, code.as_c_str(), file.as_c_str(), module_name.as_c_str())?;
    Ok(())
}

#[test]
fn qgis_package_exposes_the_historical_import_paths() {
    run(
        r#"
import qgis
assert qgis.__version__

import qgis.core
import qgis.gui
import qgis._3d
import qgis.processing

from qgis.gui import (
    QgsGui,
    QgsMapCanvas,
    QgsLayerTreeMapCanvasBridge,
    QgsMessageBar,
    QgsProjectionSelectionWidget,
)
from qgis._3d import (
    Qgs3DMapScene,
    Qgs3DMapSettings,
    Qgs3DTypes,
    Qgs3DRenderer,
)
"#,
    )
    .expect("qgis import paths must resolve");
}

#[test]
fn gui_classes_instantiate_and_expose_their_api() {
    run(
        r#"
from qgis.gui import QgsGui, QgsMapCanvas, QgsLayerTreeMapCanvasBridge, QgsMessageBar
from qgis.gui import QgsProjectionSelectionWidget

QgsGui.initGui()
assert QgsGui.isInitialized()

canvas = QgsMapCanvas()
assert canvas.layerCount() == 0
assert canvas.isEmpty
assert canvas.refreshInterval() == 250
assert canvas.destinationCrs() == "EPSG:4326"

canvas.setExtent(0.0, 0.0, 10.0, 10.0)
assert canvas.extent() == (0.0, 0.0, 10.0, 10.0)

canvas.setDestinationCrs("EPSG:3857")
assert canvas.destinationCrs() == "EPSG:3857"

canvas.addLayer("roads")
canvas.addLayer("rivers")
assert canvas.layers() == ["rivers", "roads"]
assert canvas.layerCount() == 2
assert len(canvas) == 2
assert canvas.removeLayer("roads") is True
assert canvas.removeLayer("roads") is False

canvas.refresh()
assert canvas.isDrawing is True
assert canvas.renderingComplete is False
canvas.notifyRenderComplete()
assert canvas.isDrawing is False
assert canvas.renderingComplete is True

bridge = QgsLayerTreeMapCanvasBridge()
assert bridge.hasCanvas is False
assert bridge.hasLayerTreeModel is False
bridge.setCanvas(object())
bridge.setCanvasLayerTreeModel("model")
assert bridge.hasCanvas is True
assert bridge.hasLayerTreeModel is True

bar = QgsMessageBar()
bar.pushMessage("Saved", "layer written", QgsMessageBar.Success, 5)
bar.pushWarning("Careful", "check the CRS")
assert len(bar) == 2
assert bar.currentItemTitle() == "Careful"
assert bar.currentItemText() == "check the CRS"
assert bar.currentMessageLevel() == QgsMessageBar.Warning
bar.clearWidgets()
assert bar.itemCount() == 0
assert (QgsMessageBar.Info, QgsMessageBar.Warning,
        QgsMessageBar.Critical, QgsMessageBar.Success) == (0, 1, 2, 3)

widget = QgsProjectionSelectionWidget()
assert widget.crs() == "EPSG:4326"
assert widget.crsValid is True
widget.setCrs("nonsense")
assert widget.crsValid is False

QgsGui.exitQgis()
assert QgsGui.isInitialized() is False
"#,
    )
    .expect("qgis.gui must be usable from Python");
}

#[test]
fn gui_rejects_invalid_arguments() {
    let canvas = run_expecting_error(
        r#"
from qgis.gui import QgsMapCanvas
canvas = QgsMapCanvas()
canvas.setExtent(0.0, 0.0, 0.0, 10.0)
"#,
    )
    .expect("degenerate extent must be rejected");
    assert_eq!(canvas, "ValueError");

    let level = run_expecting_error(
        r#"
from qgis.gui import QgsMessageBar
QgsMessageBar().pushMessage("t", "x", 42, 0)
"#,
    )
    .expect("unknown message level must be rejected");
    assert_eq!(level, "ValueError");
}

#[test]
fn three_d_classes_instantiate_and_expose_their_api() {
    run(
        r#"
from qgis._3d import Qgs3DMapScene, Qgs3DMapSettings, Qgs3DTypes, Qgs3DRenderer

settings = Qgs3DMapSettings()
assert settings.crs() == "EPSG:4326"
assert settings.outputDpi() == 96.0
settings.setOutputDpi(144.0)
assert settings.outputDpi() == 144.0

settings.setExtent(-5.0, -5.0, 5.0, 5.0)
assert settings.extent() == (-5.0, -5.0, 5.0, 5.0)

settings.setOrigin(1.0, 2.0, 3.0)
assert settings.origin() == (1.0, 2.0, 3.0)
settings.setEyePoint(4.0, 5.0, 6.0)
assert settings.eyePoint() == (4.0, 5.0, 6.0)
settings.setCenterPoint(7.0, 8.0, 9.0)
assert settings.centerPoint() == (7.0, 8.0, 9.0)

settings.setTerrainGenerator("QgsDemTerrainGenerator")
assert settings.terrainGenerator() == "QgsDemTerrainGenerator"
settings.setLightSource("QgsSunLightSource")
assert settings.lightSource() == "QgsSunLightSource"
settings.setCameraController("QgsTerrainCameraController")
assert settings.isComplete() is True

settings.setShadowEnabled(True)
assert settings.shadowEnabled is True
settings.setAnimationEnabled(True)
assert settings.animationEnabled is True

scene = Qgs3DMapScene(settings)
assert scene.origin() == (1.0, 2.0, 3.0)
scene.setViewingDirection(0.0, -5.0, 0.0)
assert scene.viewingDirection() == (0.0, -5.0, 0.0)
assert scene.viewingDirection_x() == 0.0
assert abs(scene.viewingDirection_y() + 1.0) < 1e-12

scene.setCameraPose(0.0, 0.0, 9.0)
assert scene.cameraPose() == (0.0, 0.0, 9.0)
assert scene.hasCameraChanged() is True
scene.resetCameraChanged()
assert scene.hasCameraChanged() is False

scene.update()
scene.update()
assert scene.updateCount() == 2

assert Qgs3DTypes.PlaneAlignment_Vertical == 1
assert Qgs3DTypes.PlaneAlignment_Horizontal == 2
assert Qgs3DTypes.OriginType_Absolute == 0
assert Qgs3DTypes.OriginType_Relative == 1
assert Qgs3DTypes.planeAlignmentForZAxis(True) == 1

renderer = Qgs3DRenderer("QgsVectorLayer3DRenderer")
assert renderer.rendererType() == "QgsVectorLayer3DRenderer"
assert renderer.enabled is True
assert renderer.isValid() is True
"#,
    )
    .expect("qgis._3d must be usable from Python");
}

#[test]
fn three_d_rejects_invalid_arguments() {
    let dpi = run_expecting_error(
        r#"
from qgis._3d import Qgs3DMapSettings
Qgs3DMapSettings().setOutputDpi(0.0)
"#,
    )
    .expect("non-positive DPI must be rejected");
    assert_eq!(dpi, "ValueError");

    let direction = run_expecting_error(
        r#"
from qgis._3d import Qgs3DMapScene
Qgs3DMapScene().setViewingDirection(0.0, 0.0, 0.0)
"#,
    )
    .expect("zero-length viewing direction must be rejected");
    assert_eq!(direction, "ValueError");
}

#[test]
fn abstract_renderer_refuses_construction() {
    let name = run_expecting_error(
        r#"
from qgis._3d import Qgs3DRenderer
Qgs3DRenderer()
"#,
    )
    .expect("Qgs3DRenderer is abstract and must refuse construction");
    assert_eq!(name, "TypeError");
}

#[test]
fn processing_import_path_still_resolves() {
    run(
        r#"
from qgis.processing import QgsProcessingAlgorithm, QgsProcessingFeedback
assert QgsProcessingAlgorithm() is not None
assert QgsProcessingFeedback() is not None
"#,
    )
    .expect("qgis.processing must keep working");
}

#[test]
fn qgis_python_plugin_directories_are_importable() {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);
    let fixture = std::env::temp_dir().join(format!(
        "qgis-pyo3-plugin-paths-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    let python_root = fixture.join("python");
    let plugins_root = python_root.join("plugins");

    for (root, package, marker) in [
        (&plugins_root, "processing", "processing fixture"),
        (&python_root, "db_manager", "db_manager fixture"),
        (&plugins_root, "MetaSearch", "MetaSearch fixture"),
        (&python_root, "console", "console fixture"),
    ] {
        let package_dir = root.join(package);
        fs::create_dir_all(&package_dir).expect("create plugin package fixture");
        fs::write(
            package_dir.join("__init__.py"),
            format!("MARKER = {marker:?}\n"),
        )
        .expect("write plugin package fixture");
    }

    let source = format!(
        r#"
import qgis
assert qgis.addPluginPath({plugins:?}) is True
assert qgis.addPluginPath({python:?}) is True
assert qgis.addPluginPath({python:?}) is False
import processing, db_manager, MetaSearch, console
assert processing.MARKER == "processing fixture"
assert db_manager.MARKER == "db_manager fixture"
assert MetaSearch.MARKER == "MetaSearch fixture"
assert console.MARKER == "console fixture"
"#,
        plugins = plugins_root.to_string_lossy(),
        python = python_root.to_string_lossy(),
    );
    run(&source).expect("QGIS Python and plugin packages must be importable");
    fs::remove_dir_all(fixture).expect("remove plugin package fixture");
}



#[test]
fn debug_gui3() {
    run(
        r#"
from qgis.gui import QgsMapCanvas
canvas = QgsMapCanvas()
canvas.addLayer("roads")
print("remove true:", canvas.removeLayer("roads") is True)
print("remove false:", canvas.removeLayer("roads") is False)
"#,
    )
    .expect("debug gui3");
}




#[test]
fn debug_3d_seq() {
    run(
        r#"
from qgis._3d import Qgs3DMapScene, Qgs3DMapSettings
settings = Qgs3DMapSettings()
settings.setOrigin(1.0, 2.0, 3.0)
settings.setCameraController("QgsTerrainCameraController")
scene = Qgs3DMapScene(settings)
print("origin", scene.origin())
scene.setViewingDirection(0.0, -5.0, 0.0)
print("vd", scene.viewingDirection())
assert scene.viewingDirection() == (0.0, -5.0, 0.0)
print("ok")
"#,
    )
    .expect("debug 3d seq");
}
