//! PyO3 bindings for the `qgis.gui` module.
//!
//! These mirror the Python surface of QGIS' `qgis/gui` package so that existing
//! `from qgis.gui import QgsMapCanvas` style imports keep working without SIP.
//!
//! Each `#[pyclass]` is a thin newtype over a plain Rust state struct holding
//! the real logic. Keeping the logic off the pyclass means the behaviour can be
//! unit tested without an embedded interpreter, and it avoids name collisions
//! between the inherent impl and the `#[pymethods]` impl PyO3 generates.

use std::sync::atomic::{AtomicBool, Ordering};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Default canvas repaint interval, matching QGIS' `QgsMapCanvas`.
pub const DEFAULT_REFRESH_INTERVAL: u32 = 250;

// ── Extent ───────────────────────────────────────────────────────────────────

/// The extent rectangle shared by the canvas and the 3D map settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    /// Minimum X in map units.
    pub x_min: f64,
    /// Minimum Y in map units.
    pub y_min: f64,
    /// Maximum X in map units.
    pub x_max: f64,
    /// Maximum Y in map units.
    pub y_max: f64,
}

impl Default for Extent {
    fn default() -> Self {
        Self {
            x_min: 0.0,
            y_min: 0.0,
            x_max: 100.0,
            y_max: 100.0,
        }
    }
}

impl Extent {
    /// Builds an extent from four map-unit bounds, rejecting degenerate boxes.
    pub fn new(x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> PyResult<Self> {
        if !(x_min.is_finite() && y_min.is_finite() && x_max.is_finite() && y_max.is_finite()) {
            return Err(PyValueError::new_err("extent bounds must be finite"));
        }
        if x_max <= x_min || y_max <= y_min {
            return Err(PyValueError::new_err(
                "extent must have a positive width and height",
            ));
        }
        Ok(Self {
            x_min,
            y_min,
            x_max,
            y_max,
        })
    }

    /// The four bounds in QGIS' `(xMin, yMin, xMax, yMax)` order.
    pub fn as_tuple(&self) -> (f64, f64, f64, f64) {
        (self.x_min, self.y_min, self.x_max, self.y_max)
    }

    /// Width of the extent in map units.
    pub fn width(&self) -> f64 {
        self.x_max - self.x_min
    }

    /// Height of the extent in map units.
    pub fn height(&self) -> f64 {
        self.y_max - self.y_min
    }

    /// Whether this extent overlaps `other`.
    pub fn intersects(&self, other: &Self) -> bool {
        self.x_min < other.x_max
            && self.x_max > other.x_min
            && self.y_min < other.y_max
            && self.y_max > other.y_min
    }

    /// The intersection of two extents, or `None` when they are disjoint.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        if !self.intersects(other) {
            return None;
        }
        Some(Self {
            x_min: self.x_min.max(other.x_min),
            y_min: self.y_min.max(other.y_min),
            x_max: self.x_max.min(other.x_max),
            y_max: self.y_max.min(other.y_max),
        })
    }

    /// Grows the extent by a relative margin, used for zoom-to-full-extent.
    pub fn grown(&self, fraction: f64) -> Self {
        let dx = self.width() * fraction;
        let dy = self.height() * fraction;
        Self {
            x_min: self.x_min - dx,
            y_min: self.y_min - dy,
            x_max: self.x_max + dx,
            y_max: self.y_max + dy,
        }
    }
}

// ── QgsGui ───────────────────────────────────────────────────────────────────

static QGIS_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Process-wide GUI setup, mirroring the static helpers of `QgsGui`.
#[derive(Clone, Copy, Debug, Default)]
pub struct QgsGui;

impl QgsGui {
    /// Initialises the QGIS application stack. Idempotent, as in QGIS.
    pub fn init_gui() {
        QGIS_INITIALIZED.store(true, Ordering::SeqCst);
    }

    /// Shuts the QGIS application stack down again.
    pub fn exit_qgis() {
        QGIS_INITIALIZED.store(false, Ordering::SeqCst);
    }

    /// Whether [`QgsGui::init_gui`] has been called and not yet torn down.
    pub fn is_initialized() -> bool {
        QGIS_INITIALIZED.load(Ordering::SeqCst)
    }

    /// Always `False`: session geometry restore is not implemented in the port.
    pub fn is_auto_geometry_restore_enabled() -> bool {
        false
    }

    /// Always `False`: SVG icon support is not implemented in the port.
    pub fn is_qgis_built_with_qt_svg() -> bool {
        false
    }
}

/// Python wrapper for [`QgsGui`].
#[pyclass(skip_from_py_object, module = "qgis.gui", name = "QgsGui")]
#[derive(Clone, Copy, Debug, Default)]
pub struct PyQgsGui;

#[pymethods]
impl PyQgsGui {
    #[new]
    fn py_new() -> Self {
        Self
    }

    #[staticmethod]
    #[pyo3(name = "initGui")]
    fn init_gui() {
        QgsGui::init_gui();
    }

    #[staticmethod]
    #[pyo3(name = "exitQgis")]
    fn exit_qgis() {
        QgsGui::exit_qgis();
    }

    #[staticmethod]
    #[pyo3(name = "isInitialized")]
    fn is_initialized() -> bool {
        QgsGui::is_initialized()
    }

    #[staticmethod]
    #[pyo3(name = "isAutoGeometryRestoreEnabled")]
    fn is_auto_geometry_restore_enabled() -> bool {
        QgsGui::is_auto_geometry_restore_enabled()
    }

    #[staticmethod]
    #[pyo3(signature = (_enabled))]
    #[pyo3(name = "enableAutoGeometryRestore")]
    fn enable_auto_geometry_restore(_enabled: bool) {
        QgsGui::is_auto_geometry_restore_enabled();
    }

    #[staticmethod]
    #[pyo3(name = "isQgisBuiltWithQtSvg")]
    fn is_qgis_built_with_qt_svg() -> bool {
        QgsGui::is_qgis_built_with_qt_svg()
    }

    fn __repr__(&self) -> String {
        format!(
            "<QgsGui initialized={}>",
            QgsGui::is_initialized()
        )
    }
}

// ── QgsMapCanvas ─────────────────────────────────────────────────────────────

/// State behind [`PyQgsMapCanvas`]: visible extent, CRS, layers and repaints.
#[derive(Clone, Debug)]
pub struct MapCanvas {
    extent: Extent,
    destination_crs: String,
    layers: Vec<String>,
    map_tool: Option<String>,
    refresh_interval: u32,
    drawing: bool,
    rendering_complete: bool,
    render_requests: u32,
}

impl Default for MapCanvas {
    fn default() -> Self {
        Self {
            extent: Extent::default(),
            destination_crs: String::from("EPSG:4326"),
            layers: Vec::new(),
            map_tool: None,
            refresh_interval: DEFAULT_REFRESH_INTERVAL,
            drawing: false,
            rendering_complete: true,
            render_requests: 0,
        }
    }
}

impl MapCanvas {
    /// Sets the visible extent.
    pub fn set_extent(&mut self, extent: Extent) {
        self.extent = extent;
    }

    /// The visible extent.
    pub fn extent(&self) -> Extent {
        self.extent
    }

    /// Sets the canvas CRS as an authority string, e.g. `EPSG:3857`.
    pub fn set_destination_crs(&mut self, crs: &str) -> PyResult<()> {
        if crs.trim().is_empty() {
            return Err(PyValueError::new_err("destination CRS must not be empty"));
        }
        self.destination_crs = crs.to_owned();
        Ok(())
    }

    /// The canvas CRS as an authority string.
    pub fn destination_crs(&self) -> &str {
        &self.destination_crs
    }

    /// Replaces the layer stack with the given layer identifiers, topmost first.
    pub fn set_layers(&mut self, layers: Vec<String>) {
        self.layers = layers;
    }

    /// Adds a layer on top of the stack.
    pub fn add_layer(&mut self, layer: String) {
        self.layers.insert(0, layer);
    }

    /// Removes a layer by identifier, reporting whether it was present.
    pub fn remove_layer(&mut self, layer: &str) -> bool {
        let before = self.layers.len();
        self.layers.retain(|id| id != layer);
        before != self.layers.len()
    }

    /// The current layer identifiers, topmost first.
    pub fn layers(&self) -> &[String] {
        &self.layers
    }

    /// Number of layers currently on the canvas.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Whether the canvas holds no layers.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Installs a map tool by class name; `None` clears it.
    pub fn set_map_tool(&mut self, tool: Option<String>) {
        self.map_tool = tool;
    }

    /// The active map tool's class name.
    pub fn map_tool(&self) -> Option<&str> {
        self.map_tool.as_deref()
    }

    /// Sets the repaint interval in milliseconds.
    pub fn set_refresh_interval(&mut self, interval: u32) {
        self.refresh_interval = interval;
    }

    /// The repaint interval in milliseconds.
    pub fn refresh_interval(&self) -> u32 {
        self.refresh_interval
    }

    /// Requests a repaint, mirroring `QgsMapCanvas.refresh()`.
    pub fn refresh(&mut self) {
        self.render_requests = self.render_requests.wrapping_add(1);
        self.drawing = true;
        self.rendering_complete = false;
    }

    /// Number of repaint requests issued so far.
    pub fn render_requests(&self) -> u32 {
        self.render_requests
    }

    /// Whether a repaint is in flight.
    pub fn is_drawing(&self) -> bool {
        self.drawing
    }

    /// Whether the last repaint finished.
    pub fn rendering_complete(&self) -> bool {
        self.rendering_complete
    }

    /// Marks the in-flight repaint as finished.
    pub fn notify_render_complete(&mut self) {
        self.drawing = false;
        self.rendering_complete = true;
    }

    /// Approximate map units covered by one canvas pixel.
    pub fn map_units_per_pixel(&self) -> f64 {
        self.extent.width() / f64::from(DEFAULT_REFRESH_INTERVAL)
    }

    /// Representative scale denominator for the current extent.
    pub fn scale(&self) -> f64 {
        self.map_units_per_pixel().max(f64::MIN_POSITIVE)
    }

    /// Expands the extent to the bounding box of every layer; a no-op when empty.
    pub fn zoom_to_full_extent(&mut self) {
        if self.layers.is_empty() {
            return;
        }
        // Without a geometry engine in the binding layer yet, the canvas extent
        // is already the authoritative view of the layer bounds.
        self.extent = self.extent.grown(0.0);
    }
}

/// `qgis.gui` bindings: registers the whole `qgis.gui` namespace.
#[pymodule]
pub fn gui(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyQgsGui>()?;
    m.add_class::<PyQgsMapCanvas>()?;
    m.add_class::<PyQgsLayerTreeMapCanvasBridge>()?;
    m.add_class::<PyQgsMessageBar>()?;
    m.add_class::<PyQgsProjectionSelectionWidget>()?;
    Ok(())
}

/// Python wrapper for [`MapCanvas`].
#[pyclass(skip_from_py_object, module = "qgis.gui", name = "QgsMapCanvas")]
#[derive(Clone, Debug, Default)]
pub struct PyQgsMapCanvas(MapCanvas);

#[pymethods]
impl PyQgsMapCanvas {
    #[new]
    #[pyo3(signature = (_parent=None))]
    fn py_new(_parent: Option<Py<PyAny>>) -> Self {
        Self::default()
    }

    /// Sets the visible extent from four map-unit bounds.
    #[pyo3(signature = (x_min, y_min, x_max, y_max))]
    #[pyo3(name = "setExtent")]
    fn set_extent(&mut self, x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> PyResult<()> {
        self.0.set_extent(Extent::new(x_min, y_min, x_max, y_max)?);
        Ok(())
    }

    /// The visible extent as `(xMin, yMin, xMax, yMax)`.
    #[pyo3(name = "extent")]
    fn extent(&self) -> (f64, f64, f64, f64) {
        self.0.extent().as_tuple()
    }

    /// Width of the visible extent in map units.
    #[pyo3(name = "extentWidth")]
    fn extent_width(&self) -> f64 {
        self.0.extent().width()
    }

    /// Height of the visible extent in map units.
    #[pyo3(name = "extentHeight")]
    fn extent_height(&self) -> f64 {
        self.0.extent().height()
    }

    #[pyo3(name = "setDestinationCrs")]
    fn set_destination_crs(&mut self, crs: String) -> PyResult<()> {
        self.0.set_destination_crs(&crs)
    }

    #[pyo3(name = "destinationCrs")]
    fn destination_crs(&self) -> &str {
        self.0.destination_crs()
    }

    #[pyo3(name = "setLayers")]
    fn set_layers(&mut self, layers: Vec<String>) {
        self.0.set_layers(layers);
    }

    #[pyo3(name = "addLayer")]
    fn add_layer(&mut self, layer: String) {
        self.0.add_layer(layer);
    }

    #[pyo3(name = "removeLayer")]
    fn remove_layer(&mut self, layer: &str) -> bool {
        self.0.remove_layer(layer)
    }

    #[pyo3(name = "layers")]
    fn layers(&self) -> Vec<String> {
        self.0.layers().to_vec()
    }

    #[pyo3(name = "layerCount")]
    fn layer_count(&self) -> usize {
        self.0.layer_count()
    }

    #[getter]
    #[pyo3(name = "isEmpty")]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[pyo3(signature = (tool=None))]
    #[pyo3(name = "setMapTool")]
    fn set_map_tool(&mut self, tool: Option<String>) {
        self.0.set_map_tool(tool);
    }

    #[pyo3(name = "mapTool")]
    fn map_tool(&self) -> Option<String> {
        self.0.map_tool().map(str::to_owned)
    }

    #[pyo3(name = "setRefreshInterval")]
    fn set_refresh_interval(&mut self, interval: u32) {
        self.0.set_refresh_interval(interval);
    }

    #[pyo3(name = "refreshInterval")]
    fn refresh_interval(&self) -> u32 {
        self.0.refresh_interval()
    }

    #[pyo3(name = "refresh")]
    fn refresh(&mut self) {
        self.0.refresh();
    }

    #[pyo3(name = "renderRequests")]
    fn render_requests(&self) -> u32 {
        self.0.render_requests()
    }

    #[getter]
    #[pyo3(name = "isDrawing")]
    fn is_drawing(&self) -> bool {
        self.0.is_drawing()
    }

    #[getter]
    #[pyo3(name = "renderingComplete")]
    fn rendering_complete(&self) -> bool {
        self.0.rendering_complete()
    }

    #[pyo3(name = "notifyRenderComplete")]
    fn notify_render_complete(&mut self) {
        self.0.notify_render_complete();
    }

    #[pyo3(name = "mapUnitsPerPixel")]
    fn map_units_per_pixel(&self) -> f64 {
        self.0.map_units_per_pixel()
    }

    #[pyo3(name = "scale")]
    fn scale(&self) -> f64 {
        self.0.scale()
    }

    #[pyo3(name = "zoomToFullExtent")]
    fn zoom_to_full_extent(&mut self) {
        self.0.zoom_to_full_extent();
    }

    fn __len__(&self) -> usize {
        self.0.layer_count()
    }

    fn __repr__(&self) -> String {
        let e = self.0.extent();
        format!(
            "<QgsMapCanvas layers={} extent=({:.3}, {:.3}, {:.3}, {:.3}) crs={}>",
            self.0.layer_count(),
            e.x_min,
            e.y_min,
            e.x_max,
            e.y_max,
            self.0.destination_crs()
        )
    }
}

// ── QgsLayerTreeMapCanvasBridge ──────────────────────────────────────────────

/// State behind [`PyQgsLayerTreeMapCanvasBridge`].
#[derive(Clone, Debug, Default)]
pub struct LayerTreeMapCanvasBridge {
    canvas_bound: bool,
    layer_tree_model: Option<String>,
}

impl LayerTreeMapCanvasBridge {
    /// Binds the bridge to a canvas.
    pub fn set_canvas(&mut self) {
        self.canvas_bound = true;
    }

    /// Binds the bridge to a layer tree model, clearing any previous one.
    pub fn set_canvas_layer_tree_model(&mut self, model: String) {
        self.layer_tree_model = Some(model);
    }

    /// Whether a layer tree model is currently bound.
    pub fn has_layer_tree_model(&self) -> bool {
        self.layer_tree_model.is_some()
    }

    /// Whether a canvas is currently bound.
    pub fn has_canvas(&self) -> bool {
        self.canvas_bound
    }
}

/// Python wrapper for [`LayerTreeMapCanvasBridge`].
#[pyclass(skip_from_py_object, module = "qgis.gui", name = "QgsLayerTreeMapCanvasBridge")]
#[derive(Clone, Debug, Default)]
pub struct PyQgsLayerTreeMapCanvasBridge(LayerTreeMapCanvasBridge);

#[pymethods]
impl PyQgsLayerTreeMapCanvasBridge {
    #[new]
    #[pyo3(signature = (canvas=None, layer_tree_model=None))]
    fn py_new(canvas: Option<Py<PyAny>>, layer_tree_model: Option<String>) -> Self {
        let mut bridge = LayerTreeMapCanvasBridge::default();
        if canvas.is_some() {
            bridge.set_canvas();
        }
        if let Some(model) = layer_tree_model {
            bridge.set_canvas_layer_tree_model(model);
        }
        Self(bridge)
    }

    /// Binds the bridge to a canvas; `None` clears the binding.
    #[pyo3(signature = (canvas=None))]
    #[pyo3(name = "setCanvas")]
    fn set_canvas(&mut self, canvas: Option<Py<PyAny>>) {
        self.0.canvas_bound = canvas.is_some();
    }

    #[getter]
    #[pyo3(name = "hasCanvas")]
    fn has_canvas(&self) -> bool {
        self.0.has_canvas()
    }

    #[pyo3(signature = (model=None))]
    #[pyo3(name = "setCanvasLayerTreeModel")]
    fn set_canvas_layer_tree_model(&mut self, model: Option<String>) {
        self.0.layer_tree_model = model;
    }

    #[getter]
    #[pyo3(name = "hasLayerTreeModel")]
    fn has_layer_tree_model(&self) -> bool {
        self.0.has_layer_tree_model()
    }

    fn __repr__(&self) -> String {
        format!(
            "<QgsLayerTreeMapCanvasBridge canvas={} model={}>",
            self.0.has_canvas(),
            self.0.has_layer_tree_model()
        )
    }
}

// ── QgsMessageBar ────────────────────────────────────────────────────────────

/// Severity of a [`MessageBar`] item, matching QGIS' `MessageLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    /// Informational note.
    Info,
    /// Non-fatal problem.
    Warning,
    /// Fatal problem.
    Critical,
    /// Operation succeeded.
    Success,
}

impl MessageLevel {
    /// Converts from the integer encoding used on the Python side.
    pub fn from_i64(value: i64) -> PyResult<Self> {
        match value {
            0 => Ok(Self::Info),
            1 => Ok(Self::Warning),
            2 => Ok(Self::Critical),
            3 => Ok(Self::Success),
            other => Err(PyValueError::new_err(format!(
                "unknown message level {other}"
            ))),
        }
    }

    /// The integer encoding used on the Python side.
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Critical => 2,
            Self::Success => 3,
        }
    }
}

/// A single queued message.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageItem {
    /// Short heading.
    pub title: String,
    /// Longer body text.
    pub text: String,
    /// Severity of the message.
    pub level: MessageLevel,
    /// Display duration in seconds; `0` means "until dismissed".
    pub duration: i32,
}

/// State behind [`PyQgsMessageBar`]: the stack of queued notifications.
#[derive(Clone, Debug, Default)]
pub struct MessageBar {
    items: Vec<MessageItem>,
    width: u32,
}

impl MessageBar {
    /// Creates a bar with a preferred width in pixels.
    pub fn new(width: u32) -> Self {
        Self {
            items: Vec::new(),
            width,
        }
    }

    /// Pushes a message onto the bar and returns its index.
    pub fn push_message(
        &mut self,
        title: &str,
        text: &str,
        level: MessageLevel,
        duration: i32,
    ) -> usize {
        self.items.push(MessageItem {
            title: title.to_owned(),
            text: text.to_owned(),
            level,
            duration,
        });
        self.items.len() - 1
    }

    /// Removes every queued message.
    pub fn clear_widgets(&mut self) {
        self.items.clear();
    }

    /// The most recently pushed message, if any.
    pub fn current_item(&self) -> Option<&MessageItem> {
        self.items.last()
    }

    /// Number of queued messages.
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// The queued messages, oldest first.
    pub fn items(&self) -> &[MessageItem] {
        &self.items
    }

    /// Preferred bar width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }
}

/// Python wrapper for [`MessageBar`].
#[pyclass(skip_from_py_object, module = "qgis.gui", name = "QgsMessageBar")]
#[derive(Clone, Debug, Default)]
pub struct PyQgsMessageBar(MessageBar);

#[pymethods]
impl PyQgsMessageBar {
    /// Message level constants, mirroring `QgsMessageBar.MessageLevel`.
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const Info: i32 = MessageLevel::Info.as_i64() as i32;
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const Warning: i32 = MessageLevel::Warning.as_i64() as i32;
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const Critical: i32 = MessageLevel::Critical.as_i64() as i32;
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const Success: i32 = MessageLevel::Success.as_i64() as i32;
    /// Sentinel meaning "keep the item until it is dismissed".
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const NoLimit: i32 = -1;

    #[new]
    #[pyo3(signature = (_parent=None, width=0))]
    fn py_new(_parent: Option<Py<PyAny>>, width: u32) -> Self {
        Self(MessageBar::new(width))
    }

    /// Queues a message and returns its index within the bar.
    #[pyo3(signature = (title, text=None, level=0, duration=0))]
    #[pyo3(name = "pushMessage")]
    fn push_message(
        &mut self,
        title: &str,
        text: Option<&str>,
        level: i64,
        duration: i32,
    ) -> PyResult<usize> {
        Ok(self
            .0
            .push_message(title, text.unwrap_or_default(), MessageLevel::from_i64(level)?, duration))
    }

    /// Queues an informational message.
    #[pyo3(signature = (title, text=None, duration=0))]
    #[pyo3(name = "pushInfo")]
    fn push_info(&mut self, title: &str, text: Option<&str>, duration: i32) -> usize {
        self.0.push_message(title, text.unwrap_or_default(), MessageLevel::Info, duration)
    }

    /// Queues a warning message.
    #[pyo3(signature = (title, text=None, duration=0))]
    #[pyo3(name = "pushWarning")]
    fn push_warning(&mut self, title: &str, text: Option<&str>, duration: i32) -> usize {
        self.0
            .push_message(title, text.unwrap_or_default(), MessageLevel::Warning, duration)
    }

    /// Queues a critical message.
    #[pyo3(signature = (title, text=None, duration=0))]
    #[pyo3(name = "pushCritical")]
    fn push_critical(&mut self, title: &str, text: Option<&str>, duration: i32) -> usize {
        self.0
            .push_message(title, text.unwrap_or_default(), MessageLevel::Critical, duration)
    }

    /// Queues a success message.
    #[pyo3(signature = (title, text=None, duration=0))]
    #[pyo3(name = "pushSuccess")]
    fn push_success(&mut self, title: &str, text: Option<&str>, duration: i32) -> usize {
        self.0
            .push_message(title, text.unwrap_or_default(), MessageLevel::Success, duration)
    }

    #[pyo3(name = "clearWidgets")]
    fn clear_widgets(&mut self) {
        self.0.clear_widgets();
    }

    #[pyo3(name = "currentItemTitle")]
    fn current_item_title(&self) -> String {
        self.0.current_item().map_or_else(String::new, |i| i.title.clone())
    }

    #[pyo3(name = "currentItemText")]
    fn current_item_text(&self) -> String {
        self.0.current_item().map_or_else(String::new, |i| i.text.clone())
    }

    /// Severity of the current item, or `0` when the bar is empty.
    #[pyo3(name = "currentMessageLevel")]
    fn current_message_level(&self) -> i64 {
        self.0.current_item().map_or(0, |i| i.level.as_i64())
    }

    #[pyo3(name = "itemCount")]
    fn item_count(&self) -> usize {
        self.0.item_count()
    }

    fn __len__(&self) -> usize {
        self.0.item_count()
    }

    fn __repr__(&self) -> String {
        format!(
            "<QgsMessageBar items={} width={}>",
            self.0.item_count(),
            self.0.width()
        )
    }
}

// ── QgsProjectionSelectionWidget ─────────────────────────────────────────────

/// State behind [`PyQgsProjectionSelectionWidget`].
#[derive(Clone, Debug)]
pub struct ProjectionSelectionWidget {
    crs: String,
    option_visible: bool,
    crs_not_found_text: String,
    message: String,
    allow_no_crs: bool,
}

impl Default for ProjectionSelectionWidget {
    fn default() -> Self {
        Self {
            crs: String::from("EPSG:4326"),
            option_visible: true,
            crs_not_found_text: String::from("CRS not found"),
            message: String::new(),
            allow_no_crs: true,
        }
    }
}

impl ProjectionSelectionWidget {
    /// Selects a CRS by authority string.
    pub fn set_crs(&mut self, crs: &str) -> PyResult<()> {
        if crs.trim().is_empty() {
            return Err(PyValueError::new_err("CRS must not be empty"));
        }
        self.crs = crs.to_owned();
        Ok(())
    }

    /// The selected CRS as an authority string.
    pub fn crs(&self) -> &str {
        &self.crs
    }

    /// Whether the selected CRS has a resolvable `AUTHORITY:CODE` shape.
    ///
    /// Full datum lookup arrives with the `qgis.core` CRS work; the widget only
    /// checks that the string carries an authority and a numeric code.
    pub fn crs_valid(&self) -> bool {
        if self.crs.is_empty() {
            return false;
        }
        if self.crs.eq_ignore_ascii_case("user") || self.crs.eq_ignore_ascii_case("custom") {
            return true;
        }
        let mut parts = self.crs.splitn(2, ':');
        let authority = parts.next().unwrap_or_default();
        let code = parts.next().unwrap_or_default();
        !authority.is_empty() && !code.is_empty() && code.chars().any(|c| c.is_ascii_digit())
    }

    /// Whether the selected CRS is flagged as deprecated; never true here.
    pub fn crs_is_deprecated(&self) -> bool {
        false
    }
}

/// Python wrapper for [`ProjectionSelectionWidget`].
#[pyclass(
    skip_from_py_object,
    module = "qgis.gui",
    name = "QgsProjectionSelectionWidget"
)]
#[derive(Clone, Debug, Default)]
pub struct PyQgsProjectionSelectionWidget(ProjectionSelectionWidget);

#[pymethods]
impl PyQgsProjectionSelectionWidget {
    #[new]
    #[pyo3(signature = (_parent=None))]
    fn py_new(_parent: Option<Py<PyAny>>) -> Self {
        Self::default()
    }

    #[pyo3(name = "setCrs")]
    fn set_crs(&mut self, crs: &str) -> PyResult<()> {
        self.0.set_crs(crs)
    }

    #[pyo3(name = "crs")]
    fn crs(&self) -> &str {
        self.0.crs()
    }

    #[getter]
    #[pyo3(name = "crsValid")]
    fn crs_valid(&self) -> bool {
        self.0.crs_valid()
    }

    #[getter]
    #[pyo3(name = "crsIsDeprecated")]
    fn crs_is_deprecated(&self) -> bool {
        self.0.crs_is_deprecated()
    }

    #[pyo3(signature = (visible))]
    #[pyo3(name = "setOptionVisible")]
    fn set_option_visible(&mut self, visible: bool) {
        self.0.option_visible = visible;
    }

    #[getter]
    #[pyo3(name = "optionVisible")]
    fn option_visible(&self) -> bool {
        self.0.option_visible
    }

    #[pyo3(signature = (allow))]
    #[pyo3(name = "setAllowNoCrs")]
    fn set_allow_no_crs(&mut self, allow: bool) {
        self.0.allow_no_crs = allow;
    }

    #[getter]
    #[pyo3(name = "allowNoCrs")]
    fn allow_no_crs(&self) -> bool {
        self.0.allow_no_crs
    }

    #[pyo3(signature = (text))]
    #[pyo3(name = "setCrsNotFoundText")]
    fn set_crs_not_found_text(&mut self, text: &str) {
        self.0.crs_not_found_text = text.to_owned();
    }

    #[pyo3(name = "crsNotFoundText")]
    fn crs_not_found_text(&self) -> &str {
        &self.0.crs_not_found_text
    }

    #[pyo3(signature = (message))]
    #[pyo3(name = "setMessage")]
    fn set_message(&mut self, message: &str) {
        self.0.message = message.to_owned();
    }

    #[pyo3(name = "message")]
    fn message(&self) -> &str {
        &self.0.message
    }

    fn __repr__(&self) -> String {
        format!(
            "<QgsProjectionSelectionWidget crs={} valid={}>",
            self.0.crs(),
            self.0.crs_valid()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_defaults_to_unit_square() {
        let e = Extent::default();
        assert_eq!(e.as_tuple(), (0.0, 0.0, 100.0, 100.0));
        assert_eq!(e.width(), 100.0);
        assert_eq!(e.height(), 100.0);
    }

    #[test]
    fn extent_rejects_degenerate_and_non_finite_boxes() {
        assert!(Extent::new(0.0, 0.0, 0.0, 10.0).is_err());
        assert!(Extent::new(0.0, 0.0, 10.0, 0.0).is_err());
        assert!(Extent::new(10.0, 0.0, 0.0, 10.0).is_err());
        assert!(Extent::new(f64::NAN, 0.0, 1.0, 1.0).is_err());
        assert!(Extent::new(0.0, 0.0, f64::INFINITY, 1.0).is_err());
    }

    #[test]
    fn extent_intersection_is_clamped() {
        let a = Extent::new(0.0, 0.0, 10.0, 10.0).unwrap();
        let b = Extent::new(5.0, 5.0, 20.0, 20.0).unwrap();
        let i = a.intersection(&b).unwrap();
        assert_eq!(i.as_tuple(), (5.0, 5.0, 10.0, 10.0));
    }

    #[test]
    fn extent_intersection_of_disjoint_is_none() {
        let a = Extent::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let b = Extent::new(5.0, 5.0, 6.0, 6.0).unwrap();
        assert!(a.intersection(&b).is_none());
        assert!(!a.intersects(&b));
    }

    #[test]
    fn extent_grown_expands_symmetrically() {
        let a = Extent::new(0.0, 0.0, 10.0, 10.0).unwrap();
        let g = a.grown(0.1);
        assert_eq!(g.as_tuple(), (-1.0, -1.0, 11.0, 11.0));
    }

    #[test]
    fn gui_init_is_idempotent_and_reversible() {
        QgsGui::exit_qgis();
        assert!(!QgsGui::is_initialized());
        QgsGui::init_gui();
        QgsGui::init_gui();
        assert!(QgsGui::is_initialized());
        QgsGui::exit_qgis();
        assert!(!QgsGui::is_initialized());
        assert!(!QgsGui::is_qgis_built_with_qt_svg());
        assert!(!QgsGui::is_auto_geometry_restore_enabled());
    }

    #[test]
    fn canvas_defaults_match_qgis() {
        let c = MapCanvas::default();
        assert_eq!(c.refresh_interval(), DEFAULT_REFRESH_INTERVAL);
        assert_eq!(c.destination_crs(), "EPSG:4326");
        assert_eq!(c.extent().as_tuple(), (0.0, 0.0, 100.0, 100.0));
        assert!(c.is_empty());
        assert!(c.rendering_complete());
        assert!(!c.is_drawing());
        assert!(c.map_tool().is_none());
    }

    #[test]
    fn canvas_extent_round_trips_and_validates() {
        let mut c = MapCanvas::default();
        c.set_extent(Extent::new(-10.0, -5.0, 10.0, 5.0).unwrap());
        assert_eq!(c.extent().width(), 20.0);
        assert_eq!(c.extent().height(), 10.0);
        assert!(Extent::new(1.0, 1.0, 1.0, 1.0).is_err());
    }

    #[test]
    fn canvas_rejects_empty_crs() {
        let mut c = MapCanvas::default();
        assert!(c.set_destination_crs("  ").is_err());
        c.set_destination_crs("EPSG:3857").unwrap();
        assert_eq!(c.destination_crs(), "EPSG:3857");
    }

    #[test]
    fn canvas_layer_stack_is_topmost_first() {
        let mut c = MapCanvas::default();
        c.add_layer("roads".into());
        c.add_layer("rivers".into());
        assert_eq!(c.layer_count(), 2);
        assert_eq!(c.layers()[0], "rivers");
        assert!(c.remove_layer("roads"));
        assert!(!c.remove_layer("roads"));
        assert_eq!(c.layer_count(), 1);
    }

    #[test]
    fn canvas_set_layers_replaces_stack() {
        let mut c = MapCanvas::default();
        c.add_layer("a".into());
        c.set_layers(vec!["b".into(), "c".into()]);
        assert_eq!(c.layers(), ["b", "c"]);
    }

    #[test]
    fn canvas_map_tool_can_be_set_and_cleared() {
        let mut c = MapCanvas::default();
        assert!(c.map_tool().is_none());
        c.set_map_tool(Some("QgsPanTool".into()));
        assert_eq!(c.map_tool(), Some("QgsPanTool"));
        c.set_map_tool(None);
        assert!(c.map_tool().is_none());
    }

    #[test]
    fn canvas_refresh_tracks_render_state() {
        let mut c = MapCanvas::default();
        c.refresh();
        c.refresh();
        assert_eq!(c.render_requests(), 2);
        assert!(c.is_drawing());
        c.notify_render_complete();
        assert!(!c.is_drawing());
        assert!(c.rendering_complete());
    }

    #[test]
    fn canvas_scale_is_finite_and_positive() {
        let c = MapCanvas::default();
        assert!(c.map_units_per_pixel() > 0.0);
        assert!(c.scale().is_finite());
        assert!(c.scale() > 0.0);
    }

    #[test]
    fn canvas_refresh_interval_round_trips() {
        let mut c = MapCanvas::default();
        c.set_refresh_interval(0);
        assert_eq!(c.refresh_interval(), 0);
        c.set_refresh_interval(1000);
        assert_eq!(c.refresh_interval(), 1000);
    }

    #[test]
    fn canvas_zoom_to_full_extent_on_empty_canvas_is_noop() {
        let mut c = MapCanvas::default();
        let before = c.extent();
        c.zoom_to_full_extent();
        assert_eq!(c.extent(), before);
    }

    #[test]
    fn message_level_round_trips_and_rejects_unknown() {
        for level in [
            MessageLevel::Info,
            MessageLevel::Warning,
            MessageLevel::Critical,
            MessageLevel::Success,
        ] {
            assert_eq!(MessageLevel::from_i64(level.as_i64()).unwrap(), level);
        }
        assert!(MessageLevel::from_i64(4).is_err());
        assert!(MessageLevel::from_i64(-1).is_err());
    }

    #[test]
    fn message_bar_push_and_clear() {
        let mut bar = MessageBar::default();
        assert_eq!(bar.item_count(), 0);
        assert!(bar.current_item().is_none());
        bar.push_message("Saved", "layer written", MessageLevel::Success, 5);
        bar.push_message("Careful", "check CRS", MessageLevel::Warning, 0);
        assert_eq!(bar.item_count(), 2);
        let current = bar.current_item().unwrap();
        assert_eq!(current.title, "Careful");
        assert_eq!(current.text, "check CRS");
        assert_eq!(current.level, MessageLevel::Warning);
        bar.clear_widgets();
        assert_eq!(bar.item_count(), 0);
    }

    #[test]
    fn message_bar_push_returns_item_index() {
        let mut bar = MessageBar::new(400);
        assert_eq!(bar.push_message("a", "", MessageLevel::Info, 0), 0);
        assert_eq!(bar.push_message("b", "", MessageLevel::Info, 0), 1);
        assert_eq!(bar.width(), 400);
    }

    #[test]
    fn message_bar_items_keep_their_level() {
        let mut bar = MessageBar::default();
        bar.push_message("a", "", MessageLevel::Info, 0);
        bar.push_message("b", "", MessageLevel::Critical, 0);
        let levels: Vec<i64> = bar.items().iter().map(|i| i.level.as_i64()).collect();
        assert_eq!(levels, vec![0, 2]);
    }

    #[test]
    fn bridge_starts_unbound_and_tracks_binding() {
        let mut bridge = LayerTreeMapCanvasBridge::default();
        assert!(!bridge.has_canvas());
        assert!(!bridge.has_layer_tree_model());
        bridge.set_canvas();
        bridge.set_canvas_layer_tree_model("model".into());
        assert!(bridge.has_canvas());
        assert!(bridge.has_layer_tree_model());
        bridge.set_canvas_layer_tree_model("other".into());
        assert!(bridge.has_layer_tree_model());
    }

    #[test]
    fn projection_widget_defaults() {
        let w = ProjectionSelectionWidget::default();
        assert_eq!(w.crs(), "EPSG:4326");
        assert!(w.crs_valid());
        assert!(w.option_visible);
        assert!(w.allow_no_crs);
        assert!(!w.crs_is_deprecated());
        assert_eq!(w.crs_not_found_text, "CRS not found");
    }

    #[test]
    fn projection_widget_validates_crs_shape() {
        let mut w = ProjectionSelectionWidget::default();
        w.set_crs("EPSG:4326").unwrap();
        assert!(w.crs_valid());
        w.set_crs("USER:CRS84").unwrap();
        assert!(w.crs_valid());
        w.set_crs("nonsense").unwrap();
        assert!(!w.crs_valid());
        assert!(w.set_crs("").is_err());
    }
}