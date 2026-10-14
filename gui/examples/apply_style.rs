//! Style application demo (bead qgis-hn6, E12.3).
//!
//! Loads a vector layer, parses a QML style, classifies every feature by the
//! renderer's attribute, and reports the symbol assigned to each feature —
//! the `(Feature, Symbol)` pairs the map canvas would draw.
//!
//! ```bash
//! cargo run --example apply_style layer.shp styles/polys.qml
//! ```

#![forbid(unsafe_code)]

use qgis_gui::style_apply::{apply_style_report, read_shapefile_layer};
use qgis_style::{parse_qml_file, Symbol};

/// Human-readable one-line summary of a symbol.
fn describe(symbol: &Symbol) -> String {
    fn rgba(c: &qgis_style::Color) -> String {
        format!("{},{},{},{}", c.r, c.g, c.b, c.a)
    }
    match symbol {
        Symbol::Point {
            shape, size, color, ..
        } => format!("point {} size={} fill={}", shape, size, rgba(color)),
        Symbol::Line { color, width } => format!("line width={} color={}", width, rgba(color)),
        Symbol::Polygon {
            fill,
            outline_width,
            style,
            ..
        } => format!(
            "polygon fill={} outline_width={} style={}",
            rgba(fill),
            outline_width,
            style
        ),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let layer_path = args
        .next()
        .unwrap_or_else(|| "usage: apply_style <layer.shp> <style.qml>".to_string());
    let qml_path = args
        .next()
        .unwrap_or_else(|| "usage: apply_style <layer.shp> <style.qml>".to_string());

    let features = match read_shapefile_layer(&layer_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading {}: {}", layer_path, e);
            std::process::exit(1);
        }
    };
    let model = match parse_qml_file(&qml_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: parsing {}: {}", qml_path, e);
            std::process::exit(1);
        }
    };

    let report = apply_style_report(&model.renderer, &features);
    let renderer = match &model.renderer {
        qgis_style::Renderer::Single(_) => "singleSymbol".to_string(),
        qgis_style::Renderer::Categorized { attr, classes } => {
            format!("categorizedSymbol on \"{}\" ({} classes)", attr, classes.len())
        }
        qgis_style::Renderer::Graduated { attr, ranges } => {
            format!("graduatedSymbol on \"{}\" ({} ranges)", attr, ranges.len())
        }
    };

    println!("layer:  {} ({} features)", layer_path, features.len());
    println!("style:  {} [{}]", qml_path, renderer);
    if let Some(attr) = &report.classified_by {
        println!("classify by: {}", attr);
        for (class, count) in &report.class_counts {
            println!("  {:<24} {}", class, count);
        }
    }
    println!(
        "styled: {}  unmatched: {}",
        report.styled.len(),
        report.unmatched.len()
    );

    for (feature, symbol) in report.styled.iter().take(20) {
        println!(
            "  feature {} -> {}",
            feature.id,
            describe(symbol)
        );
    }
    if report.styled.len() > 20 {
        println!("  ... {} more", report.styled.len() - 20);
    }
    if !report.unmatched.is_empty() {
        println!(
            "  unmatched feature ids: {:?}",
            report.unmatched.iter().take(20).collect::<Vec<_>>()
        );
    }
}