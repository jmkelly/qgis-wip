# QGIS Rust Port

Headless-first pure-Rust reimplementation of QGIS C++ that preserves QGIS 4.x behavior for agents to verify against.

## Language

**Rewrite**:
A from-scratch pure-Rust replacement of all C++ in `src/`, not a line-by-line port.
_Avoid_: port, migration, binding

**Parity**:
All C++ QtTest suites and Python `test_*.py` runnable on open stack green against the Rust implementation.
_Avoid_: compatibility, close enough, demo parity

**Pure-Rust boundary**:
No FFI to GDAL, GEOS, PROJ, Qt, or other C/C++; missing capability is rebuilt in Rust.
_Avoid_: pragmatic FFI, shim, wrapper

**Approved crate exception**:
A memory-safe pure-Rust third-party crate is not FFI and is permitted where it removes a large rebuild
risk — currently `proj4rs` for cartographic projection math and `crs-definitions` for the EPSG dataset
(ADR 0006). Must stay pure Rust (`cargo geiger`), GPL-3.0-compatible, and pinned exact in
`rust/Cargo.toml`.
_Avoid_: vendoring C, silent dependency, unpinned crate

**Oracle**:
A harness running identical inputs through C++ and Rust and diffing outputs to decide correctness.
_Avoid_: manual test, eyeball check

**Workspace slice**:
One small Rust crate mapping to one QGIS module with its own `<5 min` `cargo test`.
_Avoid_: monolith crate, giant port PR

**Python compat**:
Identical `qgis.core` / `qgis.gui` import paths served by new PyO3 bindings, not SIP/PyQt6.
_Avoid_: Python untouched, SIP reuse

**Rendering Proof of Concept**:
A minimal iced+wgpu application that renders a map canvas with 10k polygons and pan/zoom interaction to validate that the retained GUI stack can achieve interactive frame rates.
_Avoid_: GUI demo, mockup, prototype
