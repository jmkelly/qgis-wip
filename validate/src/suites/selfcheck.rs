//! The `selfcheck` suite — the parent epic's end-to-end demonstration.
//!
//! It validates E1 (`qgis-geometry`) acceptance criteria by driving the real
//! crate headlessly: parse WKT, compute area, round-trip WKT, and evaluate a
//! spatial predicate. An LLM agent runs `./verify agent selfcheck` and reads
//! the pass/fail report with no human in the loop — exactly the E16 promise.
//!
//! The checks use `qgis-geometry` directly (never hand-written geometry math,
//! per ADR-0001/0006), so the harness is validating real merged behavior.

use qgis_geometry::{contains, read_wkt, write_wkt, Geometry, Polygon};

use crate::CheckFn;

/// A 4x4 square polygon in WKT — area 16.0, perimeter 16.0.
const SQUARE_WKT: &str = "POLYGON((0 0,4 0,4 4,0 4,0 0))";

pub fn checks() -> Vec<(String, CheckFn)> {
    vec![
        ("wkt-parse-polygon".into(), Box::new(wkt_parse_polygon)),
        ("polygon-area-positive".into(), Box::new(polygon_area_positive)),
        ("wkt-roundtrip".into(), Box::new(wkt_roundtrip)),
        ("predicate-contains".into(), Box::new(predicate_contains)),
    ]
}

fn parse_square() -> Result<Polygon, String> {
    match read_wkt(SQUARE_WKT) {
        Ok(Geometry::Polygon(p)) => Ok(p),
        Ok(_) => Err("expected a Polygon geometry".into()),
        Err(e) => Err(format!("WKT parse failed: {e}")),
    }
}

fn wkt_parse_polygon() -> Result<(), String> {
    parse_square().map(|_| ())
}

fn polygon_area_positive() -> Result<(), String> {
    let polygon = parse_square()?;
    let area = polygon.area_abs();
    if area <= 0.0 {
        return Err(format!("area not positive: {area}"));
    }
    if (area - 16.0).abs() > 1e-9 {
        return Err(format!("area {area} != expected 16.0"));
    }
    Ok(())
}

fn wkt_roundtrip() -> Result<(), String> {
    let polygon = parse_square()?;
    let emitted = write_wkt(&Geometry::Polygon(polygon.clone()));
    match read_wkt(&emitted) {
        Ok(roundtripped) if roundtripped == Geometry::Polygon(polygon) => Ok(()),
        Ok(_) => Err("round-tripped geometry differs from the original".into()),
        Err(e) => Err(format!("round-trip reparse failed: {e}")),
    }
}

fn predicate_contains() -> Result<(), String> {
    let polygon = parse_square()?;
    let interior = match read_wkt("POINT(2 2)") {
        Ok(Geometry::Point(p)) => p,
        Ok(_) => return Err("expected a Point geometry".into()),
        Err(e) => return Err(format!("point parse failed: {e}")),
    };
    if !contains(&Geometry::Polygon(polygon), &Geometry::Point(interior)) {
        return Err("polygon does not contain its interior point".into());
    }
    Ok(())
}
