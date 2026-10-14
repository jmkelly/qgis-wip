//! The `oracle` suite — E16.4 agent validation for the load/style/save
//! pipeline against the QGIS C++ reference.
//!
//! The E14 pipeline (load → style → save) emits three families of text
//! artifacts: geometry (canonical WKT), styles (canonical renderer/symbol
//! descriptions) and projects (canonical project summaries). This suite drives
//! the real Rust implementation headlessly over the committed fixtures of each
//! family, then diffs the canonical output against the QGIS C++ golden recorded
//! for that fixture using [`qgis_oracle::pipeline`].
//!
//! An LLM agent runs `./verify agent-oracle` and reads:
//!
//! * one `pipeline` row per family proving the Rust pipeline runs over every
//!   fixture;
//! * one row per fixture (`<kind>/<file>`) giving the diff verdict against its
//!   QGIS C++ golden;
//! * a `reference` row per family summarising golden coverage;
//! * a `cpp` row per family, a live Rust-vs-C++ diff when `QGIS_CPP_BINARY` is
//!   configured.
//!
//! Goldens are recorded from the C++ reference by the orchestrator
//! (`oracle record`), so before they exist the per-fixture rows report `skip`
//! with the missing golden path — the agent can tell "no reference available"
//! from "reference drifted". The engine is used directly (never re-implemented
//! here), per ADR-0001.

use std::path::{Path, PathBuf};

use qgis_geometry::{read_wkt, write_wkt};
use qgis_gui::project::{read_qgs, Project};
use qgis_oracle::output::strip_backend_line;
use qgis_oracle::{
    compare_to_golden, BackendKind, Diff, Fixture, GoldenStore, PipelineKind, Reference,
};
use qgis_style::{parse_qml_file, Renderer, StyleModel, Symbol};

use crate::{CheckFn, Outcome};

/// The repository root, resolved from this crate so the suite behaves the same
/// under `cargo test` and `./verify agent-oracle`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The committed fixture directory for a pipeline family.
fn fixture_dir(kind: PipelineKind) -> PathBuf {
    match kind {
        PipelineKind::Geometry => repo_root().join("oracle/fixtures/geometry"),
        PipelineKind::Styles => repo_root().join("styles"),
        PipelineKind::Projects => repo_root().join("qgs-projects"),
    }
}

/// The fixture extension of a pipeline family.
fn extension(kind: PipelineKind) -> &'static str {
    match kind {
        PipelineKind::Geometry => "wkt",
        PipelineKind::Styles => "qml",
        PipelineKind::Projects => "qgs",
    }
}

/// Every fixture of a family, sorted by path. An empty family is an error: a
/// dropped fixture must not silently shrink the report.
fn discover(kind: PipelineKind) -> Result<Vec<PathBuf>, String> {
    let dir = fixture_dir(kind);
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let ext = extension(kind);
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some(ext))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("no .{ext} fixtures in {}", dir.display()));
    }
    Ok(files)
}

/// Runs the Rust load/style/save pipeline for one fixture and returns the
/// canonical artifact the oracle compares (a `backend:`-headed text block).
///
/// Public so the acceptance test — and future golden-recording tooling — can
/// construct a reference without duplicating the canonical format.
pub fn canonical_artifact(kind: PipelineKind, path: &Path) -> Result<String, String> {
    let body = match kind {
        PipelineKind::Geometry => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
            let geometry = read_wkt(text.trim()).map_err(|e| format!("WKT parse failed: {e}"))?;
            write_wkt(&geometry)
        }
        PipelineKind::Styles => {
            let model = parse_qml_file(path).map_err(|e| format!("QML parse failed: {e}"))?;
            style_body(&model)
        }
        PipelineKind::Projects => {
            let xml = std::fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
            let project = read_qgs(&xml).map_err(|e| format!("QGS parse failed: {e}"))?;
            project_body(&project)
        }
    };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("fixture");
    Ok(format!(
        "backend: rust/0.1.0\nfixture: {name}\nkind: {}\n---\n{body}",
        kind.name()
    ))
}

/// A stable, deterministic description of a parsed QML style — the artifact
/// the style family diffs. Symbol fields are rendered with their `Debug` form
/// so a parser change is visible in the diff.
fn style_body(model: &StyleModel) -> String {
    let mut out = String::new();
    match &model.renderer {
        Renderer::Single(symbol) => {
            out.push_str("renderer: singleSymbol\n");
            out.push_str(&format!("symbol: {}\n", symbol_summary(symbol)));
        }
        Renderer::Categorized { attr, classes } => {
            out.push_str("renderer: categorizedSymbol\n");
            out.push_str(&format!("attr: {attr}\n"));
            out.push_str(&format!("classes: {}\n", classes.len()));
            for (value, symbol) in classes {
                out.push_str(&format!("class {value}: {}\n", symbol_summary(symbol)));
            }
        }
        Renderer::Graduated { attr, ranges } => {
            out.push_str("renderer: graduatedSymbol\n");
            out.push_str(&format!("attr: {attr}\n"));
            out.push_str(&format!("ranges: {}\n", ranges.len()));
            for (lower, upper, symbol) in ranges {
                out.push_str(&format!("range {lower}..{upper}: {}\n", symbol_summary(symbol)));
            }
        }
    }
    out
}

/// One line summarising a parsed QML symbol.
fn symbol_summary(symbol: &Symbol) -> String {
    match symbol {
        Symbol::Point { shape, size, color, outline } => {
            format!("Point shape={shape} size={size} color={color:?} outline={outline:?}")
        }
        Symbol::Line { color, width } => format!("Line width={width} color={color:?}"),
        Symbol::Polygon { fill, outline, outline_width, style } => format!(
            "Polygon style={style} fill={fill:?} outline={outline:?} outline_width={outline_width}"
        ),
    }
}

/// A stable, deterministic description of a loaded QGS project — the artifact
/// the project family diffs.
fn project_body(project: &Project) -> String {
    let mut out = String::new();
    out.push_str(&format!("title: {}\n", project.title));
    out.push_str(&format!("crs: EPSG:{}\n", project.crs.epsg()));
    out.push_str(&format!("layers: {}\n", project.layers.len()));
    for (index, layer) in project.layers.iter().enumerate() {
        out.push_str(&format!(
            "layer[{index}]: name={} crs=EPSG:{} visible={} opacity={} style={} geometry={}\n",
            layer.name,
            layer.crs.epsg(),
            layer.visible,
            layer.opacity,
            layer.style_ref.as_deref().unwrap_or("-"),
            layer.geometry_type.as_deref().unwrap_or("-"),
        ));
    }
    let viewport = &project.viewport;
    out.push_str(&format!(
        "viewport: center={},{} zoom={} size={}x{}\n",
        viewport.center_lon_lat.0,
        viewport.center_lon_lat.1,
        viewport.zoom,
        viewport.size.0,
        viewport.size.1,
    ));
    out
}

/// The stem used to name a fixture's golden.
fn stem(path: &Path) -> &str {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("fixture")
}

/// Aggregate check: the Rust pipeline runs over every fixture of a family.
fn pipeline_outcome(kind: PipelineKind) -> Outcome {
    let files = match discover(kind) {
        Ok(files) => files,
        Err(reason) => return Outcome::fail(reason),
    };
    for path in &files {
        if let Err(reason) = canonical_artifact(kind, path) {
            return Outcome::fail(format!("{}: {reason}", path.display()));
        }
    }
    Outcome::Pass
}

/// Per-fixture check: diff the Rust artifact against its QGIS C++ golden.
fn reference_outcome(kind: PipelineKind, path: &Path) -> Outcome {
    let actual = match canonical_artifact(kind, path) {
        Ok(actual) => actual,
        Err(reason) => return Outcome::fail(format!("pipeline failed: {reason}")),
    };
    let store = GoldenStore::from_env();
    match compare_to_golden(&store, kind, stem(path), &actual) {
        Ok(Reference::Match) => Outcome::Pass,
        Ok(Reference::Mismatch(diff)) => {
            Outcome::fail(format!("golden drift:\n{}", diff.render("golden", "rust")))
        }
        Ok(Reference::Unavailable(reason)) => Outcome::skip(reason),
        Err(e) => Outcome::fail(format!("golden store error: {e}")),
    }
}

/// Aggregate check: summarise golden coverage for a family.
fn reference_aggregate(kind: PipelineKind) -> Outcome {
    let files = match discover(kind) {
        Ok(files) => files,
        Err(reason) => return Outcome::fail(reason),
    };
    let store = GoldenStore::from_env();
    let (mut matched, mut drifted, mut missing, mut errored) = (0usize, 0usize, 0usize, 0usize);
    for path in &files {
        match canonical_artifact(kind, path) {
            Ok(actual) => match compare_to_golden(&store, kind, stem(path), &actual) {
                Ok(Reference::Match) => matched += 1,
                Ok(Reference::Mismatch(_)) => drifted += 1,
                Ok(Reference::Unavailable(_)) => missing += 1,
                Err(_) => errored += 1,
            },
            Err(_) => errored += 1,
        }
    }
    if drifted > 0 || errored > 0 {
        Outcome::fail(format!(
            "{matched} matched, {drifted} drifted, {missing} without a golden, {errored} errored"
        ))
    } else if matched == 0 {
        Outcome::skip(format!(
            "no QGIS C++ goldens recorded for {kind} ({missing} fixtures awaiting a reference)"
        ))
    } else {
        Outcome::Pass
    }
}

/// Aggregate check: a live Rust-vs-C++ diff over a family, when the C++
/// reference binary is configured.
fn cpp_aggregate(kind: PipelineKind) -> Outcome {
    if std::env::var("QGIS_CPP_BINARY").map(|v| v.trim().is_empty()).unwrap_or(true) {
        return Outcome::skip("QGIS_CPP_BINARY not set; no live C++ reference to diff against");
    }
    let files = match discover(kind) {
        Ok(files) => files,
        Err(reason) => return Outcome::fail(reason),
    };
    let (mut compared, mut drifted) = (0usize, 0usize);
    for path in &files {
        let rust = match canonical_artifact(kind, path) {
            Ok(artifact) => artifact,
            Err(reason) => return Outcome::fail(format!("{}: {reason}", path.display())),
        };
        let fixture = match Fixture::load(path) {
            Ok(fixture) => fixture,
            Err(e) => return Outcome::fail(format!("{}: {e}", path.display())),
        };
        let cpp = match BackendKind::Cpp.run(&fixture) {
            Ok(output) => output,
            Err(e) => {
                return Outcome::fail(format!("C++ backend failed on {}: {e}", path.display()))
            }
        };
        let diff = Diff::between(&strip_backend_line(&cpp.stdout), &strip_backend_line(&rust));
        compared += 1;
        if !diff.is_empty() {
            drifted += 1;
        }
    }
    if drifted > 0 {
        Outcome::fail(format!("{drifted} of {compared} fixtures drifted from the C++ reference"))
    } else {
        Outcome::Pass
    }
}

pub fn checks() -> Vec<(String, CheckFn)> {
    let mut checks: Vec<(String, CheckFn)> = Vec::new();

    for kind in PipelineKind::ALL {
        let name = kind.name();

        checks.push((format!("{name}:pipeline"), Box::new(move || pipeline_outcome(kind))));

        match discover(kind) {
            Ok(files) => {
                for path in files {
                    let file =
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("fixture").to_string();
                    checks.push((
                        format!("{name}/{file}"),
                        Box::new(move || reference_outcome(kind, &path)),
                    ));
                }
            }
            Err(reason) => {
                checks.push((
                    format!("{name}:discover"),
                    Box::new(move || Outcome::fail(reason.clone())),
                ));
            }
        }

        checks.push((format!("{name}:reference"), Box::new(move || reference_aggregate(kind))));

        checks.push((format!("{name}:cpp"), Box::new(move || cpp_aggregate(kind))));
    }

    checks
}
