# SIP to PyO3 Migration Mapping

This document is the migration map for moving the QGIS Python bindings from the
legacy SIP/PyQt6 stack (generated against the C++ codebase) to the new PyO3
bindings exposed by the Rust workspace (`pyo3/` crate, `qgis-pyo3`).

## 1. Background

- Legacy: bindings are produced with **SIP** (sip-4/sip-5/PyQt5/PyQt6) layered
  over PyQt6/Qt. They expose `qgis.core`, `qgis.gui`, `qgis.analysis`,
  `qgis.3d`, `qgis.processing`, etc.
- New: bindings are hand-written/derived with **PyO3**, compiled into a native
  extension module shipped as `qgis` from the `qgis-pyo3` crate.
- Goal: keep "Python stays as-is" — identical import paths and API surface, so
  existing `test_*.py` continue to pass.

## 2. SIP Binding Audit (Python source tree)

The SIP/PyQt layer is consumed through the following public module paths and
binding groups. All of these need a PyO3 equivalent for full compat parity.

| SIP module            | Role                                    | PyO3 status     |
|-----------------------|-----------------------------------------|-----------------|
| `qgis.core`           | Geometry, CRS, layers, features, fields | Stub (`_core`)  |
| `qgis.gui`            | Canvas, widgets, message bar             | Stub (5 classes)|
| `qgis._3d`            | 3D map scene/settings                   | Stub (4 classes)|
| `qgis.processing`     | Processing framework + params           | Stub (11 cls)   |
| `qgis.analysis`       | Vector/raster analysis                  | Not yet         |
| `qgis.networkanalysis`| Network graph                           | Not yet         |
| `qgis.server`         | QGIS Server                             | Not yet         |
| `qgis.testing`        | Test infra (QgsRuntimeProfiler)         | Not yet         |

SIP artifacts (`.sip` interface files, `configure.py`, `qgis.sip`) are not
present in this Rust workspace; the canonical interface definition lives in
upstream QGIS. This mapping is derived from the public Python API surface
those files generate.

## 3. Mapping: SIP -> PyO3

### Class wrapping

| SIP construct                         | PyO3 equivalent                                  |
|---------------------------------------|--------------------------------------------------|
| `%ModuleHeaderCode` / `%Import`       | `use pyo3::prelude::*;` + crate deps             |
| `class QgsFoo / PyName`               | `#[pyclass] struct QgsFoo`                        |
| Constructor `QgsFoo(int x = 0)`        | `#[pymethods] impl { #[new] fn new(x: i32) }`     |
| Public method                         | `#[pymethod] fn method(&self, ...) -> ...`        |
| Const method                          | `&self` (immutable borrow)                        |
| Static method                         | `#[staticmethod]`                                 |
| Class method                          | `#[classmethod]`                                  |

### Special-handling patterns

| SIP feature          | SIP idiom                       | PyO3 approach                          |
|----------------------|---------------------------------|----------------------------------------|
| Signals / slots (Qt) | `pyqtSignal` / `pyqtSlot`       | `pyo3` has no Qt meta-object; expose observer/callback hooks or `QgsSignal` wrappers; document where lost |
| Properties           | `Q_PROPERTY` / `@property`      | `#[getter]`/`#[setter]` on `#[pymethods]` |
| Enums                | `enum` in sip, scoped/unscoped  | `#[pyclass(eq, eq_int)]` or `#[derive(...)]` enum + `#[pymethods]` |
| `None`/null pointers | SIP auto-None                   | `Option<T>` parameters + `None` support  |
| Implicit sharing     | Qt shared classes               | Rust `Arc`/`Clone` semantics; careful mutability |
| Operator overloads   | `__eq__`, `__add__`             | `#[pymethods]` dunder methods          |
| Type conversion      | `sip.voidptr`, `sip.cast`       | `#[pyclass]` opaque wrappers; no voidptr |
| Threading / GIL      | `release GIL` for long ops       | `py.allow_threads(...)`                |

### 4. SIP features with no direct PyO3 equivalent (proposed alternatives)

1. **Qt signal/slot meta-object system** — PyO3 cannot register QMetaObject. Alternative: expose Rust `Fn` callbacks / observer traits; keep PyQt6 for GUI event wiring, or ship a thin PyQt6 bridge.
2. **SIP type heap / `sip.wrappertype`** — no equivalent; use `#[pyclass]`.
3. **`sip.cast` / `sip.voidptr`** — replace with typed opaque handles.
4. **Multiple inheritance layouts from C++** — model with composition / traits.
5. **`%VirtualCatcherCode` / director classes** — implement via Rust traits + `#[pyclass(subclass)]`.
6. **Reference counting via Qt shared data** — use `Arc` + explicit clones.

## 5. Prioritization (by usage frequency)

Most-used bindings first, keyed to test files `test_*.py` and Console usage:

1. `qgis.core` — QgsProject, QgsVectorLayer, QgsRasterLayer, QgsGeometry, QgsPointXY, QgsRectangle, QgsCoordinateReferenceSystem, QgsField, QgsFeature, QgsVectorLayer, providers.
2. `qgis.gui` — QgsMapCanvas (subset), QgsMessageBar.
3. `qgis.processing` — QgsProcessingAlgorithm, parameters, feedback, context.
4. `qgis.analysis` — vector/raster analysis algorithms.
5. `qgis._3d` — 3D map scene/settings.
6. `qgis.networkanalysis`, `qgis.server`, `qgis.testing`.

## 6. Migration Checklist

- [ ] Ports `qgis.core` classes consumed by `test_core_*.py`.
- [ ] Ports geometry/CRS/layer APIs.
- [ ] Ports GUI subset consumed by `test_gui_*.py`.
- [ ] Ports processing framework.
- [ ] Ports analysis + network + server as needed.
- [ ] Validates against existing Python `test_*.py` (oracle).
- [ ] Documents signal/slot loss + callback alternative.
- [ ] Establishes `pyo3` wheel build + `import qgis` smoke test.

## 7. Verification

- `./verify test --crate qgis-pyo3`
- Python smoke: `python -c "import qgis.core, qgis.gui; print(qgis.__version__)"`

## 8. References

- ADR 0003: PyO3 Python compat, not SIP reuse.
- `pyo3/src/lib.rs` current binding surface.
- Upstream QGIS SIP files (external): `python/core/auto_generated/`, `python/gui/`.
