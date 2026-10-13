//! Python search-path setup for the Python applications and plugins shipped
//! alongside QGIS.
//!
//! The Rust extension does not bundle these Python packages. Instead, it adds
//! the conventional QGIS source/install locations to `sys.path`, so modules
//! such as `processing`, `db_manager`, `MetaSearch`, and `console` can be
//! imported from an installed or source-tree checkout.

use std::env;
use std::path::{Path, PathBuf};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule};

/// Candidate Python directories, in search-precedence order.
fn configured_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // QGIS_PYTHON_PATH points at the `python` package root. Processing is
    // normally in its `plugins` child, whereas db_manager and console may be
    // located directly in the root.
    if let Some(value) = env::var_os("QGIS_PYTHON_PATH") {
        for root in env::split_paths(&value) {
            paths.push(root.join("plugins"));
            paths.push(root);
        }
    }

    // QGIS_PLUGINPATH contains user-provided plugin directories directly.
    if let Some(value) = env::var_os("QGIS_PLUGINPATH") {
        paths.extend(env::split_paths(&value));
    }

    // Support both an installed prefix and a source checkout used as the
    // prefix. Missing directories are ignored when paths are installed.
    if let Some(value) = env::var_os("QGIS_PREFIX_PATH") {
        for prefix in env::split_paths(&value) {
            for root in [prefix.join("share/qgis/python"), prefix.join("python")] {
                paths.push(root.join("plugins"));
                paths.push(root);
            }
        }
    }

    deduplicate_paths(paths)
}

fn deduplicate_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut unique: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !unique.iter().any(|known| same_path(known, &path)) {
            unique.push(path);
        }
    }
    unique
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// Adds a Python plugin directory to `sys.path` with plugin-path precedence.
/// Returns `true` only when the path was newly inserted.
pub(crate) fn insert_plugin_path(py: Python<'_>, path: &Path) -> PyResult<bool> {
    if !path.is_dir() {
        return Err(PyValueError::new_err(format!(
            "plugin path is not a directory: {}",
            path.display()
        )));
    }

    let path = path.canonicalize().map_err(|error| {
        PyValueError::new_err(format!("cannot resolve plugin path: {error}"))
    })?;
    let sys_path = PyModule::import(py, "sys")?
        .getattr("path")?
        .cast_into::<PyList>()?;
    for entry in sys_path.iter() {
        if let Ok(entry) = entry.extract::<PathBuf>() {
            if same_path(&entry, &path) {
                return Ok(false);
            }
        }
    }

    // Add automatic paths in reverse order so their declared precedence is
    // preserved at the front of sys.path.
    sys_path.insert(0, path.to_string_lossy().as_ref())?;
    Ok(true)
}

/// Exposed as `qgis.addPluginPath(path)` for applications that discover a
/// plugin directory after the QGIS package has been imported.
#[pyfunction(name = "addPluginPath")]
fn py_add_plugin_path(py: Python<'_>, path: PathBuf) -> PyResult<bool> {
    insert_plugin_path(py, &path)
}

/// Adds configured QGIS Python roots to `sys.path` during package startup.
pub(crate) fn configure(py: Python<'_>, qgis: &Bound<'_, PyModule>) -> PyResult<()> {
    qgis.add_function(wrap_pyfunction!(py_add_plugin_path, qgis)?)?;

    for path in configured_paths().into_iter().rev() {
        // Environment variables and prefixes are advisory. Ignore absent
        // directories rather than making `import qgis` fail on other hosts.
        if path.is_dir() {
            insert_plugin_path(py, &path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_deduplication_keeps_first_precedence() {
        let first = PathBuf::from("/qgis/python/plugins");
        let duplicate = first.clone();
        let last = PathBuf::from("/qgis/python");
        assert_eq!(deduplicate_paths([first.clone(), last.clone(), duplicate]), [first, last]);
    }
}
