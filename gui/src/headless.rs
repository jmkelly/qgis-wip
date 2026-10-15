//! Headless automation harness for the QGIS GUI application.
//!
//! Wraps [`crate::app::AppState`] and drives it through the real
//! [`crate::app::update`] function, so GUI interaction can be scripted and
//! asserted without an iced runtime, a window, or a GPU. The module
//! deliberately touches no iced/wgpu types: it only consumes app state and
//! message values, and discards the `Task` that [`crate::app::update`]
//! returns.
//!
//! ```no_run
//! use qgis_gui::app::Message;
//! use qgis_gui::headless::HeadlessDriver;
//!
//! let mut driver = HeadlessDriver::new();
//! driver.dispatch(Message::OpenFileDialog);
//! assert!(driver.try_dialog().is_some());
//! ```

use qgis_core::LayerRegistry;

use crate::app::{update, AppState, Dialog, Message};

/// A dispatchable, queryable wrapper around [`AppState`].
///
/// [`HeadlessDriver`] is the headless twin of the iced application: it feeds
/// [`Message`] values through the same [`update`] path the GUI event loop
/// uses and exposes the resulting state for assertions, with no display or
/// GPU involvement.
#[derive(Debug, Default)]
pub struct HeadlessDriver {
    state: AppState,
}

impl HeadlessDriver {
    /// Creates a driver over a default (empty) [`AppState`].
    pub fn new() -> Self {
        Self { state: AppState::default() }
    }

    /// Dispatches `message` through [`update`] and stores the resulting state.
    ///
    /// The `Task` returned by [`update`] is discarded: the headless harness
    /// performs no asynchronous work.
    pub fn dispatch(&mut self, message: Message) {
        let _task = update(&mut self.state, message);
    }

    /// Returns the current application state for querying.
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Returns the active dialog, if any, for dialog assertions.
    pub fn try_dialog(&self) -> Option<&Dialog> {
        self.state.dialog.as_ref()
    }

    /// Returns the layer registry for layer assertions.
    pub fn registry(&self) -> &LayerRegistry {
        &self.state.registry
    }
}

/// A fluent, declarative builder for scripting GUI interaction sequences with
/// inline assertions.
///
/// [`Scenario`] wraps a [`HeadlessDriver`] and records every [`Message`] it
/// sends, so a failed assertion can report the whole message chain that led to
/// the failure. Each method returns `&mut Self`, so a scenario reads as one
/// chained expression: send a message, assert on the resulting state, send the
/// next message, and so on.
///
/// Assertions panic on failure with the assertion description and the message
/// history, which makes a broken scenario self-explanatory without a debugger.
///
/// ```
/// use qgis_gui::app::Message;
/// use qgis_gui::headless::Scenario;
///
/// let mut scenario = Scenario::new();
/// scenario
///     .send(Message::ZoomIn)
///     .expect_state(|state| state.camera.zoom > 3.0)
///     .expect_status_contains("Ready");
/// ```
#[derive(Debug, Default)]
pub struct Scenario {
    driver: HeadlessDriver,
    history: Vec<Message>,
}

impl Scenario {
    /// Creates a scenario over a default (empty) application state.
    pub fn new() -> Self {
        Self {
            driver: HeadlessDriver::new(),
            history: Vec::new(),
        }
    }

    /// Returns the underlying driver for ad-hoc queries.
    pub fn driver(&self) -> &HeadlessDriver {
        &self.driver
    }

    /// Returns the current application state.
    pub fn state(&self) -> &AppState {
        self.driver.state()
    }

    /// Returns every message sent so far, in order.
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Dispatches `message` through the driver and records it in the history.
    pub fn send(&mut self, message: Message) -> &mut Self {
        self.driver.dispatch(message.clone());
        self.history.push(message);
        self
    }

    /// Asserts that `predicate` holds for the current state.
    ///
    /// The failure message names the assertion only as `state predicate`; use
    /// [`Scenario::expect_state_that`] when the assertion needs a description.
    pub fn expect_state(&mut self, predicate: impl FnOnce(&AppState) -> bool) -> &mut Self {
        self.expect_state_that("state predicate", predicate)
    }

    /// Asserts that `predicate` holds, reporting `description` on failure.
    pub fn expect_state_that(
        &mut self,
        description: &str,
        predicate: impl FnOnce(&AppState) -> bool,
    ) -> &mut Self {
        if !predicate(self.driver.state()) {
            self.fail(&format!("state predicate `{description}` was false"));
        }
        self
    }

    /// Asserts that the status bar contains `needle`.
    pub fn expect_status_contains(&mut self, needle: &str) -> &mut Self {
        let status = self.driver.state().status.clone();
        if !status.contains(needle) {
            self.fail(&format!("status {status:?} does not contain {needle:?}"));
        }
        self
    }

    /// Asserts that a modal dialog is open.
    pub fn expect_dialog_present(&mut self) -> &mut Self {
        if self.driver.try_dialog().is_none() {
            self.fail("expected a dialog to be present, but none is open");
        }
        self
    }

    /// Asserts that no modal dialog is open.
    pub fn expect_dialog_absent(&mut self) -> &mut Self {
        if let Some(dialog) = self.driver.try_dialog() {
            self.fail(&format!("expected no dialog, but {dialog:?} is open"));
        }
        self
    }

    /// Asserts that the layer registry holds exactly `expected` layers.
    pub fn expect_layer_count(&mut self, expected: usize) -> &mut Self {
        let actual = self.driver.registry().len();
        if actual != expected {
            self.fail(&format!("expected {expected} layer(s), found {actual}"));
        }
        self
    }

    /// Asserts that the layer at `index` is named `expected`.
    pub fn expect_layer_name(&mut self, index: usize, expected: &str) -> &mut Self {
        match self.driver.registry().layer_at(index) {
            Some(layer) if layer.name == expected => {}
            Some(layer) => self
                .fail(&format!("layer {index} is named {:?}, expected {expected:?}", layer.name)),
            None => {
                let count = self.driver.registry().len();
                self.fail(&format!("no layer at index {index} (registry holds {count} layer(s))"));
            }
        }
        self
    }

    /// Panics with `description` followed by the full message chain so far.
    fn fail(&self, description: &str) -> ! {
        let mut chain = String::new();
        if self.history.is_empty() {
            chain.push_str("  (no messages sent)\n");
        } else {
            for (index, message) in self.history.iter().enumerate() {
                chain.push_str(&format!("  {}. {message:?}\n", index + 1));
            }
        }
        panic!(
            "scenario assertion failed: {description}\n\
             message history ({}):\n{chain}\
             last status: {:?}",
            self.history.len(),
            self.driver.state().status,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_crs::Camera;

    /// Absolute path to a provider sample, relative to this crate's manifest.
    fn sample(rel: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(rel)
            .to_str()
            .unwrap()
            .to_string()
    }

    /// Opens `path` through the file-open dialog message sequence.
    fn load(driver: &mut HeadlessDriver, path: &str) {
        driver.dispatch(Message::OpenFileDialog);
        driver.dispatch(Message::FileDialogInputChanged(path.to_string()));
        driver.dispatch(Message::FileDialogConfirm);
    }

    #[test]
    fn new_starts_from_default_state() {
        let driver = HeadlessDriver::new();
        assert!(driver.registry().is_empty());
        assert!(driver.state().styles.is_empty());
        assert_eq!(driver.state().camera.zoom, 3.0);
        assert_eq!(driver.state().camera.viewport, (1024.0, 768.0));
        assert!(driver.try_dialog().is_none());
        assert!(driver.state().status.contains("Ready"));
    }

    #[test]
    fn open_file_dialog_flow_through_dispatch() {
        let mut driver = HeadlessDriver::new();
        driver.dispatch(Message::OpenFileDialog);
        assert!(matches!(driver.try_dialog(), Some(Dialog::FileOpen { .. })));

        driver.dispatch(Message::FileDialogInputChanged("roads.shp".into()));
        match driver.try_dialog() {
            Some(Dialog::FileOpen { path }) => assert_eq!(path, "roads.shp"),
            other => panic!("expected FileOpen, got {other:?}"),
        }

        driver.dispatch(Message::Cancel);
        assert!(driver.try_dialog().is_none());
    }

    #[test]
    fn dispatch_open_and_confirm_loads_layer() {
        let shp = sample("../provider-samples/france_parts.shp");
        let mut driver = HeadlessDriver::new();
        load(&mut driver, &shp);

        assert!(
            driver.try_dialog().is_none(),
            "valid file must not error: {:?}",
            driver.try_dialog()
        );
        assert_eq!(driver.registry().len(), 1);
        let layer = driver.registry().layer_at(0).unwrap();
        assert_eq!(layer.name, "france_parts");
        assert!(!layer.features.is_empty());
        assert!(driver.state().status.contains("Loaded"));
        assert!(
            (driver.state().camera.zoom - 3.0).abs() > f64::EPSILON,
            "camera should fit the loaded extent"
        );
    }

    #[test]
    fn style_assignment_through_dispatch() {
        let shp = sample("../provider-samples/france_parts.shp");
        let qml = sample("../styles/polys.qml");
        let mut driver = HeadlessDriver::new();
        load(&mut driver, &shp);

        driver.dispatch(Message::AssignStyleDialog);
        driver.dispatch(Message::StyleDialogInputChanged(qml));
        driver.dispatch(Message::StyleDialogConfirm);

        assert!(driver.state().styles.contains_key(&0));
        assert!(driver.try_dialog().is_none());
        assert!(driver.state().status.contains("Style applied"));
    }

    #[test]
    fn error_path_missing_file_shows_dialog() {
        let mut driver = HeadlessDriver::new();
        load(&mut driver, "/nonexistent/dir/file.shp");
        assert!(matches!(driver.try_dialog(), Some(Dialog::Error { .. })));
        assert!(driver.registry().is_empty());
    }

    #[test]
    fn error_path_unsupported_extension_shows_dialog() {
        let mut driver = HeadlessDriver::new();
        load(&mut driver, "data.csv");
        assert!(matches!(driver.try_dialog(), Some(Dialog::Error { .. })));
        assert!(driver.registry().is_empty());
    }

    #[test]
    fn zoom_in_increases_zoom_through_dispatch() {
        let mut driver = HeadlessDriver::new();
        let before = driver.state().camera.zoom;
        driver.dispatch(Message::ZoomIn);
        let after = driver.state().camera.zoom;
        assert!(after > before, "zoom should increase: {before} -> {after}");
        assert!((after - before * 1.25).abs() < 1e-12);
    }

    #[test]
    fn zoom_out_decreases_zoom_through_dispatch() {
        let mut driver = HeadlessDriver::new();
        let before = driver.state().camera.zoom;
        driver.dispatch(Message::ZoomOut);
        let after = driver.state().camera.zoom;
        assert!(after < before, "zoom should decrease: {before} -> {after}");
        assert!((after - before / 1.25).abs() < 1e-12);
    }

    #[test]
    fn pan_shifts_center_by_screen_delta() {
        let mut driver = HeadlessDriver::new();
        let before = driver.state().camera;
        // Independent expectation: a screen delta of (dx, dy) moves the world
        // center by (-dx, +dy) / (scale / 2π), with scale = 256 * 2^zoom.
        let scale = 256.0 * 2.0_f64.powf(before.zoom);
        let k = scale / (2.0 * std::f64::consts::PI);
        let expected_x = before.center.0 - 100.0 / k;
        let expected_y = before.center.1 + 50.0 / k;

        driver.dispatch(Message::Pan { dx: 100.0, dy: 50.0 });

        let after = driver.state().camera;
        assert!(
            (after.center.0 - expected_x).abs() < 1e-12,
            "center x: {} vs {}",
            after.center.0,
            expected_x
        );
        assert!(
            (after.center.1 - expected_y).abs() < 1e-12,
            "center y: {} vs {}",
            after.center.1,
            expected_y
        );
    }

    #[test]
    fn set_camera_jumps_to_exact_camera() {
        let mut driver = HeadlessDriver::new();
        let camera = Camera::new((0.5, -0.25), 7.5, (800.0, 600.0));
        driver.dispatch(Message::SetCamera(camera));
        assert_eq!(driver.state().camera, camera);
    }

    #[test]
    fn reset_camera_fits_layer_extent_and_bumps_generation() {
        let shp = sample("../provider-samples/france_parts.shp");
        let mut driver = HeadlessDriver::new();
        load(&mut driver, &shp);

        let extent = driver.registry().layer_at(0).unwrap().extent().unwrap();
        let viewport = driver.state().camera.viewport;

        driver.dispatch(Message::SetCamera(Camera::new((0.0, 0.0), 1.0, viewport)));
        let generation_before = driver.state().generation;

        driver.dispatch(Message::ResetCamera);

        let expected = crate::layer_renderer::camera_for_extent(extent, viewport);
        assert_eq!(
            driver.state().generation,
            generation_before + 1,
            "reset should bump the viewport generation"
        );
        assert_eq!(driver.state().camera, expected);
    }

    #[test]
    fn state_reflects_every_dispatch() {
        let mut driver = HeadlessDriver::new();
        assert_eq!(driver.state().selected_layer, None);
        driver.dispatch(Message::LayerSelected(0));
        assert_eq!(driver.state().selected_layer, Some(0));
    }
}

#[cfg(test)]
mod scenario_tests {
    use super::*;
    use std::any::Any;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    fn sample(rel: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(rel)
            .to_str()
            .unwrap()
            .to_string()
    }

    fn panic_text(payload: &(dyn Any + Send)) -> String {
        if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else {
            "<non-string panic payload>".to_string()
        }
    }

    #[test]
    fn scenario_chains_send_and_state_assertions() {
        let mut scenario = Scenario::new();
        scenario
            .send(Message::ZoomIn)
            .expect_state(|s| s.camera.zoom > 3.0)
            .send(Message::ZoomOut)
            .expect_state(|s| (s.camera.zoom - 3.0).abs() < 1e-12);
    }

    #[test]
    fn scenario_dialog_present_and_absent() {
        let mut scenario = Scenario::new();
        scenario
            .send(Message::OpenFileDialog)
            .expect_dialog_present()
            .send(Message::Cancel)
            .expect_dialog_absent();
    }

    #[test]
    fn scenario_layer_count_and_name_after_load() {
        let shp = sample("../provider-samples/france_parts.shp");
        let mut scenario = Scenario::new();
        scenario
            .expect_layer_count(0)
            .send(Message::OpenFileDialog)
            .send(Message::FileDialogInputChanged(shp))
            .send(Message::FileDialogConfirm)
            .expect_dialog_absent()
            .expect_layer_count(1)
            .expect_layer_name(0, "france_parts")
            .expect_status_contains("Loaded");
    }

    #[test]
    fn scenario_expect_state_that_reports_description() {
        let mut scenario = Scenario::new();
        scenario.send(Message::ZoomIn);
        let payload = catch_unwind(AssertUnwindSafe(|| {
            scenario.expect_state_that("zoom is exactly 3.0", |s| s.camera.zoom == 3.0);
        }))
        .unwrap_err();
        let text = panic_text(payload.as_ref());
        assert!(text.contains("zoom is exactly 3.0"), "{text}");
        assert!(text.contains("message history"), "{text}");
        assert!(text.contains("ZoomIn"), "{text}");
    }

    #[test]
    fn scenario_failure_includes_message_chain() {
        let mut scenario = Scenario::new();
        scenario
            .send(Message::OpenFileDialog)
            .send(Message::FileDialogInputChanged("/no/such/file.shp".into()))
            .send(Message::FileDialogConfirm);
        let payload = catch_unwind(AssertUnwindSafe(|| {
            scenario.expect_layer_count(1);
        }))
        .unwrap_err();
        let text = panic_text(payload.as_ref());
        assert!(text.contains("expected 1 layer(s), found 0"), "{text}");
        assert!(text.contains("1. OpenFileDialog"), "{text}");
        assert!(text.contains("2. FileDialogInputChanged"), "{text}");
        assert!(text.contains("3. FileDialogConfirm"), "{text}");
    }
}
