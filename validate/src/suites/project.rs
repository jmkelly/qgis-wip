//! The `project` suite — validates E15 (QGS project save/load) acceptance
//! criteria by driving the real persistence code in `qgis-gui` headlessly:
//! save a multi-layer project to a `.qgs` file, read it back, and compare
//! layer count, names, CRS, styles, and viewport. An LLM agent runs
//! `./verify agent-project` and reads the pass/fail report with no human in
//! the loop — exactly the E16 promise.
//!
//! The checks use `qgis-gui`'s `write_qgs`/`read_qgs` directly (never
//! hand-written persistence logic, per ADR-0001/0006), so the harness
//! validates real merged behavior.

use qgis_crs::SrsId;
use qgis_gui::project::{read_qgs, write_qgs, Project, ProjectLayer, Viewport};

use crate::CheckFn;

/// Number of layers in the sample project — multi-layer, per the bead.
const LAYER_COUNT: usize = 3;

/// A multi-layer sample project with distinct CRS, styles, opacity, and
/// visibility per layer, so every comparison has something to catch.
fn sample_project() -> Project {
    Project {
        title: "agent-project sample".into(),
        crs: SrsId::from_epsg(3857),
        layers: vec![
            ProjectLayer {
                name: "parcels".into(),
                source: Some("/data/parcels.shp".into()),
                crs: SrsId::from_epsg(4326),
                visible: true,
                opacity: 1.0,
                style_ref: Some("/styles/polys.qml".into()),
                geometry_type: Some("Polygon".into()),
            },
            ProjectLayer {
                // XML metacharacter in the name — escaping must survive.
                name: "roads & rails".into(),
                source: Some("/data/roads.shp".into()),
                crs: SrsId::from_epsg(4326),
                visible: false,
                opacity: 0.5,
                style_ref: Some("/styles/lines.qml".into()),
                geometry_type: Some("LineString".into()),
            },
            ProjectLayer {
                name: "sensors".into(),
                source: None,
                crs: SrsId::from_epsg(3857),
                visible: true,
                opacity: 0.75,
                style_ref: None,
                geometry_type: Some("Point".into()),
            },
        ],
        viewport: Viewport {
            center_lon_lat: (2.5, 47.5),
            zoom: 6.25,
            size: (1024.0, 768.0),
        },
    }
}

/// Writes the sample project to a real `.qgs` file and reads it back.
/// Returns the original project, the reparsed project, and the saved XML.
fn roundtrip() -> Result<(Project, Project, String), String> {
    let project = sample_project();
    let path = std::env::temp_dir().join(format!(
        "qgis-l0t-project-{}.qgs",
        std::process::id()
    ));
    let path_str = path
        .to_str()
        .ok_or_else(|| "temp path is not valid UTF-8".to_string())?;
    std::fs::write(path_str, write_qgs(&project)).map_err(|e| format!("save failed: {e}"))?;
    let xml =
        std::fs::read_to_string(path_str).map_err(|e| format!("read failed: {e}"))?;
    let parsed = read_qgs(&xml)?;
    let _ = std::fs::remove_file(path_str);
    Ok((project, parsed, xml))
}

pub fn checks() -> Vec<(String, CheckFn)> {
    vec![
        (
            "project-saves-qgis-xml".into(),
            Box::new(|| project_saves_qgis_xml().into()),
        ),
        (
            "project-roundtrip-layer-count".into(),
            Box::new(|| project_roundtrip_layer_count().into()),
        ),
        (
            "project-roundtrip-layer-names".into(),
            Box::new(|| project_roundtrip_layer_names().into()),
        ),
        (
            "project-roundtrip-crs".into(),
            Box::new(|| project_roundtrip_crs().into()),
        ),
        (
            "project-roundtrip-styles".into(),
            Box::new(|| project_roundtrip_styles().into()),
        ),
        (
            "project-roundtrip-viewport".into(),
            Box::new(|| project_roundtrip_viewport().into()),
        ),
        (
            "project-roundtrip-full-equality".into(),
            Box::new(|| project_roundtrip_full_equality().into()),
        ),
    ]
}

fn project_saves_qgis_xml() -> Result<(), String> {
    let (_, _, xml) = roundtrip()?;
    if !xml.starts_with("<!DOCTYPE qgis") {
        return Err("saved file has no QGIS doctype".into());
    }
    if !xml.contains("<qgis version=") {
        return Err("saved file has no <qgis> root element".into());
    }
    let maplayer_count = xml.matches("<maplayer ").count();
    if maplayer_count != LAYER_COUNT {
        return Err(format!(
            "saved file has {maplayer_count} <maplayer> entries, expected {LAYER_COUNT}"
        ));
    }
    Ok(())
}

fn project_roundtrip_layer_count() -> Result<(), String> {
    let (original, parsed, _) = roundtrip()?;
    if parsed.layers.len() != LAYER_COUNT || parsed.layers.len() != original.layers.len() {
        return Err(format!(
            "layer count {} after round-trip, expected {}",
            parsed.layers.len(),
            LAYER_COUNT
        ));
    }
    Ok(())
}

fn project_roundtrip_layer_names() -> Result<(), String> {
    let (original, parsed, _) = roundtrip()?;
    let original_names: Vec<&str> = original.layers.iter().map(|l| l.name.as_str()).collect();
    let parsed_names: Vec<&str> = parsed.layers.iter().map(|l| l.name.as_str()).collect();
    if parsed_names != original_names {
        return Err(format!(
            "layer names {parsed_names:?} after round-trip, expected {original_names:?}"
        ));
    }
    Ok(())
}

fn project_roundtrip_crs() -> Result<(), String> {
    let (original, parsed, _) = roundtrip()?;
    if parsed.crs != original.crs {
        return Err(format!(
            "project CRS {} after round-trip, expected {}",
            parsed.crs.epsg(),
            original.crs.epsg()
        ));
    }
    for (i, (before, after)) in original.layers.iter().zip(parsed.layers.iter()).enumerate() {
        if before.crs != after.crs {
            return Err(format!(
                "layer {i} CRS {} after round-trip, expected {}",
                after.crs.epsg(),
                before.crs.epsg()
            ));
        }
    }
    Ok(())
}

fn project_roundtrip_styles() -> Result<(), String> {
    let (original, parsed, _) = roundtrip()?;
    for (i, (before, after)) in original.layers.iter().zip(parsed.layers.iter()).enumerate() {
        if before.style_ref != after.style_ref {
            return Err(format!(
                "layer {i} ({}) style ref {:?} after round-trip, expected {:?}",
                before.name, after.style_ref, before.style_ref
            ));
        }
    }
    Ok(())
}

fn project_roundtrip_viewport() -> Result<(), String> {
    let (original, parsed, _) = roundtrip()?;
    let before = &original.viewport;
    let after = &parsed.viewport;
    if after.center_lon_lat != before.center_lon_lat {
        return Err(format!(
            "viewport center {:?} after round-trip, expected {:?}",
            after.center_lon_lat, before.center_lon_lat
        ));
    }
    if after.zoom != before.zoom {
        return Err(format!(
            "viewport zoom {} after round-trip, expected {}",
            after.zoom, before.zoom
        ));
    }
    if after.size != before.size {
        return Err(format!(
            "viewport size {:?} after round-trip, expected {:?}",
            after.size, before.size
        ));
    }
    Ok(())
}

fn project_roundtrip_full_equality() -> Result<(), String> {
    let (original, parsed, _) = roundtrip()?;
    if parsed != original {
        return Err(
            "reparsed project differs from the original (see the per-field checks for detail)".into(),
        );
    }
    Ok(())
}
