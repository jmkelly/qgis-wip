//! PyO3 bindings for the `qgis._3d` module.
//!
//! Mirrors the Python surface of QGIS' `qgis/_3d` package so `from qgis._3d
//! import Qgs3DMapSettings` keeps resolving once SIP is retired.
//!
//! As with the `gui` module, each `#[pyclass]` is a newtype over a plain Rust
//! state struct so the behaviour is unit testable without an interpreter.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::gui::Extent;

/// Default output resolution used by `Qgs3DMapSettings`.
pub const DEFAULT_OUTPUT_DPI: f64 = 96.0;

// ── Qgs3DTypes ───────────────────────────────────────────────────────────────

/// How a vector layer's Z values relate to the ground, mirroring
/// `Qgs3DTypes::PlaneAlignment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneAlignment {
    /// The layer does not participate in the 3D scene.
    NotSupported,
    /// Z values are altitudes.
    Vertical,
    /// Z values are offsets from the ground.
    Horizontal,
}

impl PlaneAlignment {
    /// Converts from the integer encoding used on the Python side.
    pub fn from_i64(value: i64) -> PyResult<Self> {
        match value {
            0 => Ok(Self::NotSupported),
            1 => Ok(Self::Vertical),
            2 => Ok(Self::Horizontal),
            other => Err(PyValueError::new_err(format!(
                "unknown plane alignment {other}"
            ))),
        }
    }

    /// The integer encoding used on the Python side.
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::NotSupported => 0,
            Self::Vertical => 1,
            Self::Horizontal => 2,
        }
    }

    /// Whether the alignment places geometry on the vertical axis.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical)
    }
}

/// Whether a 3D origin is absolute or relative to the terrain, mirroring
/// `Qgs3DTypes::OriginType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginType {
    /// Coordinates are in the scene's own CRS.
    Absolute,
    /// Coordinates are offsets from the terrain surface.
    Relative,
}

impl OriginType {
    /// Converts from the integer encoding used on the Python side.
    pub fn from_i64(value: i64) -> PyResult<Self> {
        match value {
            0 => Ok(Self::Absolute),
            1 => Ok(Self::Relative),
            other => Err(PyValueError::new_err(format!("unknown origin type {other}"))),
        }
    }

    /// The integer encoding used on the Python side.
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Absolute => 0,
            Self::Relative => 1,
        }
    }
}

/// Python view of the `Qgs3DTypes` namespace; it holds constants only.
#[pyclass(skip_from_py_object, module = "qgis._3d", name = "Qgs3DTypes")]
#[derive(Clone, Copy, Debug, Default)]
pub struct PyQgs3DTypes;

#[pymethods]
impl PyQgs3DTypes {
    /// `PlaneAlignment.NotSupported`
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const PlaneAlignment_NotSupported: i32 = PlaneAlignment::NotSupported.as_i64() as i32;
    /// `PlaneAlignment.Vertical`
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const PlaneAlignment_Vertical: i32 = PlaneAlignment::Vertical.as_i64() as i32;
    /// `PlaneAlignment.Horizontal`
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const PlaneAlignment_Horizontal: i32 = PlaneAlignment::Horizontal.as_i64() as i32;
    /// `OriginType.Absolute`
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const OriginType_Absolute: i32 = OriginType::Absolute.as_i64() as i32;
    /// `OriginType.Relative`
    #[classattr]
    #[allow(non_snake_case, non_upper_case_globals)]
    const OriginType_Relative: i32 = OriginType::Relative.as_i64() as i32;

    #[new]
    fn py_new() -> Self {
        Self
    }

    /// Maps the scene's Z axis onto the enum used by vector layer renderers.
    #[staticmethod]
    #[pyo3(name = "planeAlignmentForZAxis")]
    fn plane_alignment_for_z_axis(z_is_up: bool) -> i64 {
        if z_is_up {
            PlaneAlignment::Vertical.as_i64()
        } else {
            PlaneAlignment::Horizontal.as_i64()
        }
    }

    fn __repr__(&self) -> &'static str {
        "<Qgs3DTypes (namespace)>"
    }
}

// ── Qgs3DMapSettings ─────────────────────────────────────────────────────────

/// A point in the 3D scene's coordinate system.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point3D {
    /// Easting / X.
    pub x: f64,
    /// Northing / Y.
    pub y: f64,
    /// Altitude / Z.
    pub z: f64,
}

impl Point3D {
    /// Builds a 3D point.
    pub fn new(x: f64, y: f64, z: f64) -> PyResult<Self> {
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return Err(PyValueError::new_err("point coordinates must be finite"));
        }
        Ok(Self { x, y, z })
    }

    /// The point as `(x, y, z)`.
    pub fn as_tuple(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }
}

/// State behind [`PyQgs3DMapSettings`].
#[derive(Clone, Debug)]
pub struct Map3DMapSettings {
    crs: String,
    extent: Extent,
    origin: Point3D,
    eye_point: Point3D,
    center_point: Point3D,
    terrain_generator: Option<String>,
    light_source: Option<String>,
    camera_controller: Option<String>,
    shadow_enabled: bool,
    render_direction_aware: bool,
    animation_enabled: bool,
    origin_type: OriginType,
    output_dpi: f64,
}

impl Default for Map3DMapSettings {
    fn default() -> Self {
        Self {
            crs: String::from("EPSG:4326"),
            extent: Extent::default(),
            origin: Point3D::default(),
            eye_point: Point3D::new(0.0, -1.0, 1.0).unwrap_or_default(),
            center_point: Point3D::default(),
            terrain_generator: None,
            light_source: None,
            camera_controller: None,
            shadow_enabled: false,
            render_direction_aware: true,
            animation_enabled: false,
            origin_type: OriginType::Absolute,
            output_dpi: DEFAULT_OUTPUT_DPI,
        }
    }
}

impl Map3DMapSettings {
    /// Creates default settings for a scene.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the scene CRS as an authority string.
    pub fn set_crs(&mut self, crs: &str) -> PyResult<()> {
        if crs.trim().is_empty() {
            return Err(PyValueError::new_err("scene CRS must not be empty"));
        }
        self.crs = crs.to_owned();
        Ok(())
    }

    /// The scene CRS as an authority string.
    pub fn crs(&self) -> &str {
        &self.crs
    }

    /// Sets the scene extent from four map-unit bounds.
    pub fn set_extent(&mut self, x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> PyResult<()> {
        self.extent = Extent::new(x_min, y_min, x_max, y_max)?;
        Ok(())
    }

    /// The scene extent.
    pub fn extent(&self) -> Extent {
        self.extent
    }

    /// Sets the scene origin.
    pub fn set_origin(&mut self, origin: Point3D) {
        self.origin = origin;
    }

    /// The scene origin.
    pub fn origin(&self) -> Point3D {
        self.origin
    }

    /// Sets the camera eye position.
    pub fn set_eye_point(&mut self, point: Point3D) {
        self.eye_point = point;
    }

    /// The camera eye position.
    pub fn eye_point(&self) -> Point3D {
        self.eye_point
    }

    /// Sets the point the camera orbits around.
    pub fn set_center_point(&mut self, point: Point3D) {
        self.center_point = point;
    }

    /// The point the camera orbits around.
    pub fn center_point(&self) -> Point3D {
        self.center_point
    }

    /// Installs a terrain generator by class name; `None` disables terrain.
    pub fn set_terrain_generator(&mut self, generator: Option<String>) {
        self.terrain_generator = generator;
    }

    /// The installed terrain generator's class name.
    pub fn terrain_generator(&self) -> Option<&str> {
        self.terrain_generator.as_deref()
    }

    /// Installs a light source by class name.
    pub fn set_light_source(&mut self, light_source: Option<String>) {
        self.light_source = light_source;
    }

    /// The installed light source's class name.
    pub fn light_source(&self) -> Option<&str> {
        self.light_source.as_deref()
    }

    /// Installs a camera controller by class name.
    pub fn set_camera_controller(&mut self, controller: Option<String>) {
        self.camera_controller = controller;
    }

    /// The installed camera controller's class name.
    pub fn camera_controller(&self) -> Option<&str> {
        self.camera_controller.as_deref()
    }

    /// Enables or disables shadow casting.
    pub fn set_shadow_enabled(&mut self, enabled: bool) {
        self.shadow_enabled = enabled;
    }

    /// Whether shadow casting is enabled.
    pub fn shadow_enabled(&self) -> bool {
        self.shadow_enabled
    }

    /// Enables or disables direction-aware rendering.
    pub fn set_render_direction_aware_enabled(&mut self, enabled: bool) {
        self.render_direction_aware = enabled;
    }

    /// Whether direction-aware rendering is enabled.
    pub fn render_direction_aware_enabled(&self) -> bool {
        self.render_direction_aware
    }

    /// Enables or disables rendering animation.
    pub fn set_animation_enabled(&mut self, enabled: bool) {
        self.animation_enabled = enabled;
    }

    /// Whether rendering animation is enabled.
    pub fn animation_enabled(&self) -> bool {
        self.animation_enabled
    }

    /// Sets whether the origin is absolute or terrain-relative.
    pub fn set_origin_type(&mut self, origin_type: OriginType) {
        self.origin_type = origin_type;
    }

    /// Whether the origin is absolute or terrain-relative.
    pub fn origin_type(&self) -> OriginType {
        self.origin_type
    }

    /// Sets the output resolution in dots per inch.
    pub fn set_output_dpi(&mut self, dpi: f64) -> PyResult<()> {
        if !(dpi.is_finite() && dpi > 0.0) {
            return Err(PyValueError::new_err("output DPI must be a positive number"));
        }
        self.output_dpi = dpi;
        Ok(())
    }

    /// The output resolution in dots per inch.
    pub fn output_dpi(&self) -> f64 {
        self.output_dpi
    }

    /// Reports whether the settings are complete enough to render a scene.
    pub fn is_complete(&self) -> bool {
        !self.crs.is_empty() && self.camera_controller.is_some()
    }
}

/// Python wrapper for [`Map3DMapSettings`].
#[pyclass(skip_from_py_object, module = "qgis._3d", name = "Qgs3DMapSettings")]
#[derive(Clone, Debug, Default)]
pub struct PyQgs3DMapSettings(Map3DMapSettings);

#[pymethods]
impl PyQgs3DMapSettings {
    #[new]
    #[pyo3(signature = (_scene=None))]
    fn py_new(_scene: Option<Py<PyAny>>) -> Self {
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

    #[pyo3(signature = (x_min, y_min, x_max, y_max))]
    #[pyo3(name = "setExtent")]
    fn set_extent(&mut self, x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> PyResult<()> {
        self.0.set_extent(x_min, y_min, x_max, y_max)
    }

    #[pyo3(name = "extent")]
    fn extent(&self) -> (f64, f64, f64, f64) {
        self.0.extent().as_tuple()
    }

    #[pyo3(signature = (x=0.0, y=0.0, z=0.0))]
    #[pyo3(name = "setOrigin")]
    fn set_origin(&mut self, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.0.set_origin(Point3D::new(x, y, z)?);
        Ok(())
    }

    #[pyo3(name = "origin")]
    fn origin(&self) -> (f64, f64, f64) {
        self.0.origin().as_tuple()
    }

    #[pyo3(signature = (x, y, z))]
    #[pyo3(name = "setEyePoint")]
    fn set_eye_point(&mut self, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.0.set_eye_point(Point3D::new(x, y, z)?);
        Ok(())
    }

    #[pyo3(name = "eyePoint")]
    fn eye_point(&self) -> (f64, f64, f64) {
        self.0.eye_point().as_tuple()
    }

    #[pyo3(signature = (x, y, z))]
    #[pyo3(name = "setCenterPoint")]
    fn set_center_point(&mut self, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.0.set_center_point(Point3D::new(x, y, z)?);
        Ok(())
    }

    #[pyo3(name = "centerPoint")]
    fn center_point(&self) -> (f64, f64, f64) {
        self.0.center_point().as_tuple()
    }

    #[pyo3(signature = (generator=None))]
    #[pyo3(name = "setTerrainGenerator")]
    fn set_terrain_generator(&mut self, generator: Option<String>) {
        self.0.set_terrain_generator(generator);
    }

    #[pyo3(name = "terrainGenerator")]
    fn terrain_generator(&self) -> Option<String> {
        self.0.terrain_generator().map(str::to_owned)
    }

    #[pyo3(signature = (light_source=None))]
    #[pyo3(name = "setLightSource")]
    fn set_light_source(&mut self, light_source: Option<String>) {
        self.0.set_light_source(light_source);
    }

    #[pyo3(name = "lightSource")]
    fn light_source(&self) -> Option<String> {
        self.0.light_source().map(str::to_owned)
    }

    #[pyo3(signature = (controller=None))]
    #[pyo3(name = "setCameraController")]
    fn set_camera_controller(&mut self, controller: Option<String>) {
        self.0.set_camera_controller(controller);
    }

    #[pyo3(name = "cameraController")]
    fn camera_controller(&self) -> Option<String> {
        self.0.camera_controller().map(str::to_owned)
    }

    #[pyo3(signature = (enabled))]
    #[pyo3(name = "setShadowEnabled")]
    fn set_shadow_enabled(&mut self, enabled: bool) {
        self.0.set_shadow_enabled(enabled);
    }

    #[getter]
    #[pyo3(name = "shadowEnabled")]
    fn shadow_enabled(&self) -> bool {
        self.0.shadow_enabled()
    }

    #[pyo3(signature = (enabled))]
    #[pyo3(name = "setRenderDirectionAwareEnabled")]
    fn set_render_direction_aware_enabled(&mut self, enabled: bool) {
        self.0.set_render_direction_aware_enabled(enabled);
    }

    #[getter]
    #[pyo3(name = "renderDirectionAwareEnabled")]
    fn render_direction_aware_enabled(&self) -> bool {
        self.0.render_direction_aware_enabled()
    }

    #[pyo3(signature = (enabled))]
    #[pyo3(name = "setAnimationEnabled")]
    fn set_animation_enabled(&mut self, enabled: bool) {
        self.0.set_animation_enabled(enabled);
    }

    #[getter]
    #[pyo3(name = "animationEnabled")]
    fn animation_enabled(&self) -> bool {
        self.0.animation_enabled()
    }

    #[pyo3(signature = (origin_type))]
    #[pyo3(name = "setOriginType")]
    fn set_origin_type(&mut self, origin_type: i64) -> PyResult<()> {
        self.0.set_origin_type(OriginType::from_i64(origin_type)?);
        Ok(())
    }

    #[pyo3(name = "originType")]
    fn origin_type(&self) -> i64 {
        self.0.origin_type().as_i64()
    }

    #[pyo3(signature = (dpi))]
    #[pyo3(name = "setOutputDpi")]
    fn set_output_dpi(&mut self, dpi: f64) -> PyResult<()> {
        self.0.set_output_dpi(dpi)
    }

    #[pyo3(name = "outputDpi")]
    fn output_dpi(&self) -> f64 {
        self.0.output_dpi()
    }

    #[pyo3(name = "isComplete")]
    fn is_complete(&self) -> bool {
        self.0.is_complete()
    }

    fn __repr__(&self) -> String {
        format!(
            "<Qgs3DMapSettings crs={} extent={:?} dpi={}>",
            self.0.crs(),
            self.0.extent().as_tuple(),
            self.0.output_dpi()
        )
    }
}

// ── Qgs3DMapScene ────────────────────────────────────────────────────────────

/// State behind [`PyQgs3DMapScene`]: camera pose and view direction.
#[derive(Clone, Debug)]
pub struct Map3DScene {
    viewing_direction: Point3D,
    camera_pose: Point3D,
    origin: Point3D,
    update_count: u32,
    camera_changed: bool,
    view_dir_x: f64,
    view_dir_y: f64,
}

impl Default for Map3DScene {
    fn default() -> Self {
        Self {
            viewing_direction: Point3D::new(0.0, -1.0, 0.0).unwrap_or_default(),
            camera_pose: Point3D::new(0.0, -1.0, 1.0).unwrap_or_default(),
            origin: Point3D::default(),
            update_count: 0,
            camera_changed: false,
            view_dir_x: 0.0,
            view_dir_y: -1.0,
        }
    }
}

impl Map3DScene {
    /// Creates an empty scene.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the direction the camera looks at.
    pub fn set_viewing_direction(&mut self, direction: Point3D) -> PyResult<()> {
        let length = (direction.x.powi(2) + direction.y.powi(2) + direction.z.powi(2)).sqrt();
        if length < f64::EPSILON {
            return Err(PyValueError::new_err("viewing direction must not be a zero vector"));
        }
        self.viewing_direction = direction;
        self.view_dir_x = direction.x / length;
        self.view_dir_y = direction.y / length;
        Ok(())
    }

    /// The direction the camera looks at.
    pub fn viewing_direction(&self) -> Point3D {
        self.viewing_direction
    }

    /// Sets the camera position.
    pub fn set_camera_pose(&mut self, pose: Point3D) {
        if pose != self.camera_pose {
            self.camera_changed = true;
        }
        self.camera_pose = pose;
    }

    /// The camera position.
    pub fn camera_pose(&self) -> Point3D {
        self.camera_pose
    }

    /// Sets the scene origin.
    pub fn set_origin(&mut self, origin: Point3D) {
        self.origin = origin;
    }

    /// The scene origin.
    pub fn origin(&self) -> Point3D {
        self.origin
    }

    /// Requests an update, mirroring `Qgs3DMapScene.update()`.
    pub fn update(&mut self) {
        self.update_count = self.update_count.wrapping_add(1);
    }

    /// Number of updates requested so far.
    pub fn update_count(&self) -> u32 {
        self.update_count
    }

    /// Whether the camera moved since construction or the last `reset`.
    pub fn has_camera_changed(&self) -> bool {
        self.camera_changed
    }

    /// Clears the camera-changed flag.
    pub fn reset_camera_changed(&mut self) {
        self.camera_changed = false;
    }
}

/// Python wrapper for [`Map3DScene`].
#[pyclass(skip_from_py_object, module = "qgis._3d", name = "Qgs3DMapScene")]
#[derive(Clone, Debug, Default)]
pub struct PyQgs3DMapScene(Map3DScene);

#[pymethods]
impl PyQgs3DMapScene {
    #[new]
    #[pyo3(signature = (settings=None))]
    fn py_new(py: Python<'_>, settings: Option<Py<PyAny>>) -> Self {
        let mut scene = Map3DScene::new();
        if let Some(settings) = settings {
            // QGIS seeds a scene from its settings' origin; fall back to the
            // default origin when the caller passes something without one.
            if let Ok(origin) = settings.call_method0(py, "origin") {
                if let Ok((x, y, z)) = origin.extract::<(f64, f64, f64)>(py) {
                    scene.set_origin(Point3D::new(x, y, z).unwrap_or_default());
                }
            }
        }
        Self(scene)
    }

    #[pyo3(signature = (x, y, z))]
    #[pyo3(name = "setViewingDirection")]
    fn set_viewing_direction(&mut self, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.0.set_viewing_direction(Point3D::new(x, y, z)?)
    }

    #[pyo3(name = "viewingDirection")]
    fn viewing_direction(&self) -> (f64, f64, f64) {
        self.0.viewing_direction().as_tuple()
    }

    #[pyo3(signature = (x, y, z))]
    #[pyo3(name = "setCameraPose")]
    fn set_camera_pose(&mut self, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.0.set_camera_pose(Point3D::new(x, y, z)?);
        Ok(())
    }

    #[pyo3(name = "cameraPose")]
    fn camera_pose(&self) -> (f64, f64, f64) {
        self.0.camera_pose().as_tuple()
    }

    #[pyo3(signature = (x=0.0, y=0.0, z=0.0))]
    #[pyo3(name = "setOrigin")]
    fn set_origin(&mut self, x: f64, y: f64, z: f64) -> PyResult<()> {
        self.0.set_origin(Point3D::new(x, y, z)?);
        Ok(())
    }

    #[pyo3(name = "origin")]
    fn origin(&self) -> (f64, f64, f64) {
        self.0.origin().as_tuple()
    }

    #[pyo3(name = "viewingDirectionX")]
    fn viewing_direction_x(&self) -> f64 {
        self.0.view_dir_x
    }

    #[pyo3(name = "viewingDirection_x")]
    fn viewing_direction_x_snake(&self) -> f64 {
        self.0.view_dir_x
    }

    #[pyo3(name = "viewingDirectionY")]
    fn viewing_direction_y(&self) -> f64 {
        self.0.view_dir_y
    }

    #[pyo3(name = "viewingDirection_y")]
    fn viewing_direction_y_snake(&self) -> f64 {
        self.0.view_dir_y
    }

    #[pyo3(name = "update")]
    fn update(&mut self) {
        self.0.update();
    }

    #[pyo3(name = "updateCount")]
    fn update_count(&self) -> u32 {
        self.0.update_count()
    }

    #[pyo3(name = "hasCameraChanged")]
    fn has_camera_changed(&self) -> bool {
        self.0.has_camera_changed()
    }

    #[pyo3(name = "resetCameraChanged")]
    fn reset_camera_changed(&mut self) {
        self.0.reset_camera_changed();
    }

    fn __repr__(&self) -> String {
        let d = self.0.viewing_direction();
        format!(
            "<Qgs3DMapScene origin={:?} direction={:?} updates={}>",
            self.0.origin().as_tuple(),
            d.as_tuple(),
            self.0.update_count()
        )
    }
}

// ── Qgs3DRenderer ────────────────────────────────────────────────────────────

/// State behind [`PyQgs3DRenderer`].
///
/// `Qgs3DRenderer` is an abstract base class in QGIS: concrete renderers live
/// in `QgsVectorLayer3DRenderer`, `QgsMeshLayer3DRenderer` and
/// `QgsPointCloudLayer3DRenderer`. Instantiating the base directly raises, the
/// same way the SIP bindings do.
#[derive(Clone, Debug, Default)]
pub struct Renderer3D {
    renderer_type: String,
    enabled: bool,
}

impl Renderer3D {
    /// Creates a renderer of the given type.
    pub fn new(renderer_type: &str) -> Self {
        Self {
            renderer_type: renderer_type.to_owned(),
            enabled: true,
        }
    }

    /// The renderer's type identifier.
    pub fn renderer_type(&self) -> &str {
        &self.renderer_type
    }

    /// Enables or disables the renderer.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether the renderer is enabled.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Whether the renderer can draw anything.
    pub fn is_valid(&self) -> bool {
        !self.renderer_type.is_empty()
    }
}

/// Python wrapper for [`Renderer3D`], registered as `Qgs3DRenderer`.
#[pyclass(skip_from_py_object, module = "qgis._3d", name = "Qgs3DRenderer")]
#[derive(Clone, Debug, Default)]
pub struct PyQgs3DRenderer(Renderer3D);

#[pymethods]
impl PyQgs3DRenderer {
    #[new]
    #[pyo3(signature = (renderer_type=None))]
    fn py_new(renderer_type: Option<&str>) -> PyResult<Self> {
        // Matches SIP: the abstract base refuses construction.
        if renderer_type.is_none() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Qgs3DRenderer is abstract and cannot be instantiated; \
                 use a concrete subclass such as QgsVectorLayer3DRenderer",
            ));
        }
        Ok(Self(Renderer3D::new(renderer_type.unwrap_or_default())))
    }

    #[pyo3(name = "rendererType")]
    fn renderer_type(&self) -> &str {
        self.0.renderer_type()
    }

    #[pyo3(signature = (enabled))]
    #[pyo3(name = "setEnabled")]
    fn set_enabled(&mut self, enabled: bool) {
        self.0.set_enabled(enabled);
    }

    #[getter]
    #[pyo3(name = "enabled")]
    fn enabled(&self) -> bool {
        self.0.enabled()
    }

    #[pyo3(name = "isValid")]
    fn is_valid(&self) -> bool {
        self.0.is_valid()
    }

    fn __repr__(&self) -> String {
        format!(
            "<Qgs3DRenderer type={} enabled={}>",
            self.0.renderer_type(),
            self.0.enabled()
        )
    }
}

/// `qgis._3d` bindings: registers the whole `qgis._3d` namespace.
#[pymodule]
#[pyo3(name = "_3d")]
pub fn three_d(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyQgs3DMapScene>()?;
    m.add_class::<PyQgs3DMapSettings>()?;
    m.add_class::<PyQgs3DTypes>()?;
    m.add_class::<PyQgs3DRenderer>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point3d_rejects_non_finite_coordinates() {
        assert!(Point3D::new(f64::NAN, 0.0, 0.0).is_err());
        assert!(Point3D::new(0.0, f64::INFINITY, 0.0).is_err());
        let p = Point3D::new(1.0, 2.0, 3.0).unwrap();
        assert_eq!(p.as_tuple(), (1.0, 2.0, 3.0));
    }

    #[test]
    fn plane_alignment_round_trips() {
        for a in [
            PlaneAlignment::NotSupported,
            PlaneAlignment::Vertical,
            PlaneAlignment::Horizontal,
        ] {
            assert_eq!(PlaneAlignment::from_i64(a.as_i64()).unwrap(), a);
        }
        assert!(PlaneAlignment::from_i64(7).is_err());
        assert!(PlaneAlignment::Vertical.is_vertical());
        assert!(!PlaneAlignment::Horizontal.is_vertical());
        assert!(!PlaneAlignment::NotSupported.is_vertical());
    }

    #[test]
    fn origin_type_round_trips() {
        for o in [OriginType::Absolute, OriginType::Relative] {
            assert_eq!(OriginType::from_i64(o.as_i64()).unwrap(), o);
        }
        assert!(OriginType::from_i64(9).is_err());
    }

    #[test]
    fn settings_defaults() {
        let s = Map3DMapSettings::new();
        assert_eq!(s.crs(), "EPSG:4326");
        assert_eq!(s.extent().as_tuple(), (0.0, 0.0, 100.0, 100.0));
        assert_eq!(s.output_dpi(), DEFAULT_OUTPUT_DPI);
        assert!(!s.shadow_enabled());
        assert!(s.render_direction_aware_enabled());
        assert!(!s.animation_enabled());
        assert!(s.terrain_generator().is_none());
        assert_eq!(s.origin_type(), OriginType::Absolute);
    }

    #[test]
    fn settings_reject_bad_crs_and_dpi() {
        let mut s = Map3DMapSettings::new();
        assert!(s.set_crs("  ").is_err());
        s.set_crs("EPSG:3857").unwrap();
        assert_eq!(s.crs(), "EPSG:3857");
        assert!(s.set_output_dpi(0.0).is_err());
        assert!(s.set_output_dpi(-1.0).is_err());
        assert!(s.set_output_dpi(f64::NAN).is_err());
        s.set_output_dpi(144.0).unwrap();
        assert_eq!(s.output_dpi(), 144.0);
    }

    #[test]
    fn settings_extent_validates() {
        let mut s = Map3DMapSettings::new();
        assert!(s.set_extent(0.0, 0.0, 0.0, 1.0).is_err());
        s.set_extent(-5.0, -5.0, 5.0, 5.0).unwrap();
        assert_eq!(s.extent().as_tuple(), (-5.0, -5.0, 5.0, 5.0));
    }

    #[test]
    fn settings_optional_subsystems_install_and_clear() {
        let mut s = Map3DMapSettings::new();
        s.set_terrain_generator(Some("QgsDemTerrainGenerator".into()));
        s.set_light_source(Some("QgsSunLightSource".into()));
        s.set_camera_controller(Some("QgsTerrainCameraController".into()));
        assert_eq!(s.terrain_generator(), Some("QgsDemTerrainGenerator"));
        assert_eq!(s.light_source(), Some("QgsSunLightSource"));
        assert_eq!(s.camera_controller(), Some("QgsTerrainCameraController"));
        s.set_terrain_generator(None);
        assert!(s.terrain_generator().is_none());
        assert!(s.is_complete());
        s.set_camera_controller(None);
        assert!(!s.is_complete());
    }

    #[test]
    fn settings_toggles_round_trip() {
        let mut s = Map3DMapSettings::new();
        s.set_shadow_enabled(true);
        s.set_render_direction_aware_enabled(false);
        s.set_animation_enabled(true);
        s.set_origin_type(OriginType::Relative);
        assert!(s.shadow_enabled());
        assert!(!s.render_direction_aware_enabled());
        assert!(s.animation_enabled());
        assert_eq!(s.origin_type(), OriginType::Relative);
    }

    #[test]
    fn settings_points_round_trip() {
        let mut s = Map3DMapSettings::new();
        s.set_origin(Point3D::new(1.0, 2.0, 3.0).unwrap());
        s.set_eye_point(Point3D::new(4.0, 5.0, 6.0).unwrap());
        s.set_center_point(Point3D::new(7.0, 8.0, 9.0).unwrap());
        assert_eq!(s.origin().as_tuple(), (1.0, 2.0, 3.0));
        assert_eq!(s.eye_point().as_tuple(), (4.0, 5.0, 6.0));
        assert_eq!(s.center_point().as_tuple(), (7.0, 8.0, 9.0));
    }

    #[test]
    fn scene_defaults_look_north_from_south() {
        let s = Map3DScene::new();
        assert_eq!(s.viewing_direction().as_tuple(), (0.0, -1.0, 0.0));
        assert_eq!(s.camera_pose().as_tuple(), (0.0, -1.0, 1.0));
        assert_eq!(s.update_count(), 0);
        assert!(!s.has_camera_changed());
        assert_eq!(s.origin().as_tuple(), (0.0, 0.0, 0.0));
    }

    #[test]
    fn scene_normalizes_viewing_direction() {
        let mut s = Map3DScene::new();
        s.set_viewing_direction(Point3D::new(0.0, -5.0, 0.0).unwrap()).unwrap();
        assert!((s.view_dir_x - 0.0).abs() < 1e-12);
        assert!((s.view_dir_y + 1.0).abs() < 1e-12);
    }

    #[test]
    fn scene_rejects_zero_viewing_direction() {
        let mut s = Map3DScene::new();
        assert!(s.set_viewing_direction(Point3D::default()).is_err());
    }

    #[test]
    fn scene_tracks_camera_changes() {
        let mut s = Map3DScene::new();
        s.set_camera_pose(Point3D::new(0.0, 0.0, 5.0).unwrap());
        assert!(s.has_camera_changed());
        s.reset_camera_changed();
        assert!(!s.has_camera_changed());
        s.set_camera_pose(Point3D::new(0.0, 0.0, 5.0).unwrap());
        assert!(!s.has_camera_changed());
    }

    #[test]
    fn scene_update_counts() {
        let mut s = Map3DScene::new();
        s.update();
        s.update();
        s.update();
        assert_eq!(s.update_count(), 3);
    }

    #[test]
    fn scene_origin_round_trips() {
        let mut s = Map3DScene::new();
        s.set_origin(Point3D::new(10.0, 20.0, 30.0).unwrap());
        assert_eq!(s.origin().as_tuple(), (10.0, 20.0, 30.0));
    }

    #[test]
    fn renderer_type_and_validity() {
        let mut r = Renderer3D::new("QgsVectorLayer3DRenderer");
        assert_eq!(r.renderer_type(), "QgsVectorLayer3DRenderer");
        assert!(r.is_valid());
        assert!(r.enabled());
        r.set_enabled(false);
        assert!(!r.enabled());
        assert!(!Renderer3D::new("").is_valid());
    }
}