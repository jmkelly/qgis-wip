//! The `styles` suite — E16.2 agent validation for the QML style engine.
//!
//! It drives the real `qgis-style` engine headlessly: parse each `.qml` fixture
//! in `styles/`, classify a representative feature for the renderer, convert the
//! parsed symbol into its typed, renderable form, and render it to canvas
//! geometry. An LLM agent runs `./verify agent-styles` and reads a pass/fail
//! verdict per fixture and per renderer family, with no human in the loop.
//!
//! The report is deliberately organised by renderer family: every fixture check
//! is named `<rendererType>:<file>` and there is one aggregate `renderer:<type>`
//! check per family. On a failure the detail line names the fixture and the
//! stage (parse, classify, or render) that broke.
//!
//! The engine is used directly (never re-implemented here), so the harness
//! validates merged behavior — the E16 promise. No geometry or style math is
//! hand-written in this module (ADR-0001).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use qgis_geometry::{LineString, Point, Polygon};
use qgis_style::symbols::{from_qml, Geometry, Symbol};
use qgis_style::{classify, parse_qml_file, Renderer};

use crate::{CheckFn, Outcome};

/// The fixture directory shipped in the repository, relative to this crate.
fn styles_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../styles")
}

/// Number of `.qml` fixtures the suite must cover. A fixture added to (or
/// removed from) `styles/` without updating this count fails the run, so the
/// report can never silently shrink.
const EXPECTED_FIXTURES: usize = 11;

/// The three renderer families, in report order.
const RENDERER_TYPES: [&str; 3] = ["singleSymbol", "categorizedSymbol", "graduatedSymbol"];

/// The report name of a renderer family.
fn renderer_type_name(renderer: &Renderer) -> &'static str {
    match renderer {
        Renderer::Single(_) => "singleSymbol",
        Renderer::Categorized { .. } => "categorizedSymbol",
        Renderer::Graduated { .. } => "graduatedSymbol",
    }
}

/// Builds an attribute map a representative feature of this renderer would
/// carry: the first category's value, the midpoint of the first range, or
/// nothing at all for a single symbol.
fn sample_attrs(renderer: &Renderer) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    match renderer {
        Renderer::Single(_) => {}
        Renderer::Categorized { attr, classes } => {
            if let Some((value, _)) = classes.first() {
                attrs.insert(attr.clone(), value.clone());
            }
        }
        Renderer::Graduated { attr, ranges } => {
            if let Some((lower, upper, _)) = ranges.first() {
                attrs.insert(attr.clone(), ((lower + upper) / 2.0).to_string());
            }
        }
    }
    attrs
}

/// A sample feature geometry matching the symbol's kind.
fn sample_geometry(symbol: &Symbol) -> Geometry {
    match symbol {
        Symbol::Point(_) => Geometry::Point(Point::new(0.0, 0.0)),
        Symbol::Line(_) => {
            Geometry::Line(LineString::from_2d_coords(&[(0.0, 0.0), (10.0, 10.0)]))
        }
        Symbol::Polygon(_) => Geometry::Polygon(Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ])),
    }
}

/// The kind of a typed symbol, for failure messages.
fn symbol_kind(symbol: &Symbol) -> &'static str {
    match symbol {
        Symbol::Point(_) => "point",
        Symbol::Line(_) => "line",
        Symbol::Polygon(_) => "polygon",
    }
}

/// Parses, classifies, and renders one `.qml` fixture end-to-end. `Ok(())`
/// means every stage succeeded and the symbol rendered at least one primitive.
fn validate_fixture(path: &Path) -> Result<(), String> {
    let model = parse_qml_file(path).map_err(|e| format!("parse failed: {e}"))?;
    let renderer = renderer_type_name(&model.renderer);

    let attrs = sample_attrs(&model.renderer);
    let raw = classify(&model.renderer, &attrs)
        .ok_or_else(|| format!("classify produced no symbol for {renderer}"))?;
    let symbol = from_qml(&raw);
    let kind = symbol_kind(&symbol);

    let geometry = sample_geometry(&symbol);
    if symbol.render(&geometry).is_empty() {
        return Err(format!("{renderer} rendered no geometry for a {kind} feature"));
    }
    Ok(())
}

/// Every `.qml` fixture in the styles directory, sorted by file name.
fn discover_fixtures(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "qml"))
        .collect();
    files.sort();
    Ok(files)
}

/// Asserts that every fixture of one renderer family parses, classifies, and
/// renders, and that at least one fixture exercises the family at all.
fn renderer_check(dir: &Path, renderer: &str) -> Result<(), String> {
    let files = discover_fixtures(dir)?;
    let mut seen = 0usize;
    for path in &files {
        let model =
            parse_qml_file(path).map_err(|e| format!("{}: parse failed: {e}", path.display()))?;
        if renderer_type_name(&model.renderer) != renderer {
            continue;
        }
        seen += 1;
        validate_fixture(path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if seen == 0 {
        return Err(format!("no fixture exercises the {renderer} renderer"));
    }
    Ok(())
}

pub fn checks() -> Vec<(String, CheckFn)> {
    let dir = styles_dir();
    let mut checks: Vec<(String, CheckFn)> = Vec::new();

    let files = match discover_fixtures(&dir) {
        Ok(files) => files,
        Err(reason) => {
            checks.push((
                "fixtures:dir".to_string(),
                Box::new(move || Outcome::fail(reason.clone())),
            ));
            return checks;
        }
    };

    // Guard the fixture count so a dropped .qml cannot silently shrink the
    // report below the acceptance criteria.
    let count = files.len();
    checks.push((
        "fixtures:count".to_string(),
        Box::new(move || {
            if count == EXPECTED_FIXTURES {
                Outcome::Pass
            } else {
                Outcome::fail(format!(
                    "expected {EXPECTED_FIXTURES} .qml fixtures, found {count}"
                ))
            }
        }),
    ));

    // One check per fixture, named `<rendererType>:<file>` so the renderer
    // family is visible in the report row itself.
    for path in files {
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let renderer = match parse_qml_file(&path) {
            Ok(model) => renderer_type_name(&model.renderer),
            Err(_) => "unknown",
        };
        checks.push((
            format!("{renderer}:{file}"),
            Box::new(move || validate_fixture(&path).into()),
        ));
    }

    // Aggregate verdict per renderer family.
    for renderer in RENDERER_TYPES {
        let dir = dir.clone();
        checks.push((
            format!("renderer:{renderer}"),
            Box::new(move || renderer_check(&dir, renderer).into()),
        ));
    }

    checks
}
