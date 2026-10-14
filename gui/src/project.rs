//! QGS project persistence (E15): serialize the application state — layers,
//! styles, CRS, and viewport — to a `.qgs` XML file and restore it.
//!
//! The format is a QGIS-compatible subset of the `.qgs` schema: a project CRS
//! (`<projectCrs>`), a layer tree (`<layer-tree-group>`), the full layer
//! definitions (`<projectlayers>/<maplayer>`) carrying each layer's data
//! source, CRS, opacity and style reference, and the map viewport
//! (`<mapcanvas>` + `<view>`). Layers are reloaded from their recorded data
//! source on open; a missing or unreadable source leaves the layer in place
//! with its metadata but no features, and the status bar reports it.

use std::collections::HashMap;
use std::f64::consts::PI;

use qgis_core::{Layer, LayerRegistry};
use qgis_crs::{equirectangular_project, equirectangular_unproject, Camera, SrsId};
use qgis_style::parse_qml_file;
use roxmltree::{Document, ParsingOptions};

use crate::app::{load_geojson_as_layer, AppState};
use crate::layer_renderer::load_shapefile_as_layer;

/// Screen size assumed when a project does not record a viewport size.
const DEFAULT_VIEWPORT: (f64, f64) = (1024.0, 768.0);
/// Pixels spanned by the whole world at zoom 0 (matches `qgis_crs`).
const TILE_SIZE: f64 = 256.0;

/// A serializable project: the document persisted to a `.qgs` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// Human-readable project title.
    pub title: String,
    /// Project-level CRS.
    pub crs: SrsId,
    /// Layers in display (bottom-to-top) order.
    pub layers: Vec<ProjectLayer>,
    /// Map viewport.
    pub viewport: Viewport,
}

/// One layer entry in a [`Project`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectLayer {
    /// Layer name shown in the panel.
    pub name: String,
    /// Data source path/URI the layer was loaded from.
    pub source: Option<String>,
    /// Layer CRS.
    pub crs: SrsId,
    /// Whether the layer is visible.
    pub visible: bool,
    /// Layer opacity in `0.0..=1.0`.
    pub opacity: f32,
    /// Style reference (a `.qml` path) for the layer, if any.
    pub style_ref: Option<String>,
    /// Geometry type of the layer's first feature, for the `geometry`
    /// attribute QGIS expects (informational).
    pub geometry_type: Option<String>,
}

impl ProjectLayer {
    /// The QGIS provider key for this layer's source.
    fn provider(&self) -> &'static str {
        match self.source.as_deref() {
            Some(s) if !s.is_empty() => "ogr",
            _ => "memory",
        }
    }
}

/// Map viewport: center in lon/lat degrees, zoom, and screen size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Center of the viewport as `(lon, lat)` in degrees.
    pub center_lon_lat: (f64, f64),
    /// Zoom level (`0.0` means the whole world spans 256 px).
    pub zoom: f64,
    /// Viewport size in screen pixels `(width, height)`.
    pub size: (f64, f64),
}

impl Viewport {
    /// Captures the viewport from a render [`Camera`].
    pub fn from_camera(camera: &Camera) -> Self {
        let (lon, lat) = equirectangular_unproject(camera.center.0, camera.center.1);
        Self {
            center_lon_lat: (lon, lat),
            zoom: camera.zoom,
            size: camera.viewport,
        }
    }

    /// Rebuilds a render [`Camera`] from the viewport.
    pub fn to_camera(&self) -> Camera {
        Camera::new(
            equirectangular_project(self.center_lon_lat.0, self.center_lon_lat.1),
            self.zoom,
            self.size,
        )
    }

    /// The map extent in degrees: `(xmin, ymin, xmax, ymax)`.
    pub fn extent_degrees(&self) -> (f64, f64, f64, f64) {
        let world_per_px = 2.0 * PI / (TILE_SIZE * 2.0_f64.powf(self.zoom));
        let half_w = self.size.0 / 2.0 * world_per_px;
        let half_h = self.size.1 / 2.0 * world_per_px;
        let cx = self.center_lon_lat.0.to_radians();
        let cy = self.center_lon_lat.1.to_radians();
        (
            (cx - half_w).to_degrees(),
            (cy - half_h).to_degrees(),
            (cx + half_w).to_degrees(),
            (cy + half_h).to_degrees(),
        )
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Serializes a [`Project`] to QGS XML.
pub fn write_qgs(project: &Project) -> String {
    let mut s = String::new();
    s.push_str("<!DOCTYPE qgis PUBLIC 'http://mrcc.com/qgis.dtd' 'SYSTEM'>\n");
    s.push_str(&format!(
        "<qgis version=\"4.0.0\" projectname=\"{}\">\n",
        esc_attr(&project.title)
    ));
    s.push_str(&format!("  <title>{}</title>\n", esc_text(&project.title)));

    s.push_str("  <projectCrs>\n    <spatialrefsys>\n");
    s.push_str(&format!("      <srid>{}</srid>\n", project.crs.epsg()));
    s.push_str(&format!("      <authid>EPSG:{}</authid>\n", project.crs.epsg()));
    s.push_str("    </spatialrefsys>\n  </projectCrs>\n");

    s.push_str("  <layer-tree-group>\n");
    for (i, layer) in project.layers.iter().enumerate() {
        s.push_str(&format!(
            "    <layer-tree-layer name=\"{}\" checked=\"{}\" expanded=\"1\" source=\"{}\" providerKey=\"{}\" id=\"{}\"/>\n",
            esc_attr(&layer.name),
            if layer.visible { "Qt::Checked" } else { "Qt::Unchecked" },
            esc_attr(layer.source.as_deref().unwrap_or("")),
            layer.provider(),
            esc_attr(&layer_id(layer, i)),
        ));
    }
    s.push_str("  </layer-tree-group>\n");

    s.push_str("  <projectlayers>\n");
    for (i, layer) in project.layers.iter().enumerate() {
        let geometry = layer.geometry_type.as_deref().unwrap_or("Unknown");
        s.push_str(&format!(
            "    <maplayer type=\"vector\" geometry=\"{}\">\n",
            esc_attr(geometry)
        ));
        s.push_str(&format!("      <id>{}</id>\n", esc_text(&layer_id(layer, i))));
        s.push_str(&format!(
            "      <datasource>{}</datasource>\n",
            esc_text(layer.source.as_deref().unwrap_or(""))
        ));
        s.push_str(&format!(
            "      <layername>{}</layername>\n",
            esc_text(&layer.name)
        ));
        s.push_str("      <srs>\n        <spatialrefsys>\n");
        s.push_str(&format!("          <srid>{}</srid>\n", layer.crs.epsg()));
        s.push_str(&format!(
            "          <authid>EPSG:{}</authid>\n",
            layer.crs.epsg()
        ));
        s.push_str("        </spatialrefsys>\n      </srs>\n");
        s.push_str(&format!(
            "      <provider>{}</provider>\n",
            esc_text(layer.provider())
        ));
        s.push_str(&format!(
            "      <layerOpacity>{}</layerOpacity>\n",
            layer.opacity
        ));
        if let Some(style_ref) = &layer.style_ref {
            s.push_str(&format!(
                "      <styleRef>{}</styleRef>\n",
                esc_text(style_ref)
            ));
        }
        s.push_str("    </maplayer>\n");
    }
    s.push_str("  </projectlayers>\n");

    let (xmin, ymin, xmax, ymax) = project.viewport.extent_degrees();
    s.push_str("  <mapcanvas name=\"theMapCanvas\">\n");
    s.push_str("    <units>degrees</units>\n");
    s.push_str("    <extent>\n");
    s.push_str(&format!("      <xmin>{xmin}</xmin>\n"));
    s.push_str(&format!("      <ymin>{ymin}</ymin>\n"));
    s.push_str(&format!("      <xmax>{xmax}</xmax>\n"));
    s.push_str(&format!("      <ymax>{ymax}</ymax>\n"));
    s.push_str("    </extent>\n");
    s.push_str("  </mapcanvas>\n");

    s.push_str(&format!(
        "  <view center=\"{},{}\" zoom=\"{}\" width=\"{}\" height=\"{}\"/>\n",
        project.viewport.center_lon_lat.0,
        project.viewport.center_lon_lat.1,
        project.viewport.zoom,
        project.viewport.size.0,
        project.viewport.size.1,
    ));
    s.push_str("</qgis>\n");
    s
}

/// Stable, unique layer id derived from the name and position.
fn layer_id(layer: &ProjectLayer, index: usize) -> String {
    let mut slug = String::with_capacity(layer.name.len());
    for ch in layer.name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            slug.push(ch);
        } else {
            slug.push('_');
        }
    }
    if slug.is_empty() {
        slug.push_str("layer");
    }
    format!("{slug}_{index}")
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Parses QGS XML into a [`Project`].
///
/// Accepts the format written by [`write_qgs`] as well as the wider QGIS
/// schema: layer definitions are read from `<projectlayers>/<maplayer>` and
/// fall back to `<layer-tree-group>/<layer-tree-layer>`; the viewport is read
/// from `<view>` and falls back to the `<mapcanvas>` extent.
pub fn read_qgs(xml: &str) -> Result<Project, String> {
    let doc = Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        },
    )
    .map_err(|e| format!("invalid QGS XML: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("qgis") {
        return Err("not a QGS project: missing <qgis> root element".into());
    }

    let title = root
        .children()
        .find(|n| n.has_tag_name("title"))
        .and_then(|n| n.text())
        .unwrap_or("")
        .to_string();

    let crs = parse_srs(&root).unwrap_or_else(|| SrsId::from_epsg(4326));

    // Display order and visibility live in the layer tree, keyed by layer id.
    let mut tree_order: Vec<String> = Vec::new();
    let mut visibility: HashMap<String, bool> = HashMap::new();
    for tree_layer in root
        .descendants()
        .filter(|n| n.has_tag_name("layer-tree-layer"))
    {
        if let Some(id) = tree_layer.attribute("id") {
            tree_order.push(id.to_string());
            let checked = tree_layer.attribute("checked").unwrap_or("Qt::Checked");
            visibility.insert(id.to_string(), checked == "Qt::Checked");
        }
    }

    let mut layers: Vec<(String, ProjectLayer)> = Vec::new();
    for node in root
        .descendants()
        .filter(|n| n.has_tag_name("maplayer"))
    {
        let id = child_text(&node, "id").unwrap_or_default();
        let name = child_text(&node, "layername").unwrap_or_else(|| id.clone());
        let source = child_text(&node, "datasource")
            .filter(|s| !s.is_empty())
            .or_else(|| node.attribute("datasource").map(str::to_string));
        let layer_crs = node
            .children()
            .find(|n| n.has_tag_name("srs"))
            .and_then(|srs| parse_srs(&srs))
            .unwrap_or(crs);
        let opacity = child_text(&node, "layerOpacity")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(1.0);
        let style_ref = child_text(&node, "styleRef").filter(|s| !s.is_empty());
        let visible = visibility.get(&id).copied().unwrap_or(true);
        layers.push((
            id,
            ProjectLayer {
                name,
                source,
                crs: layer_crs,
                visible,
                opacity,
                style_ref,
                geometry_type: node.attribute("geometry").map(str::to_string),
            },
        ));
    }

    // Order layers by their position in the layer tree (QGIS display order).
    if !tree_order.is_empty() {
        layers.sort_by_key(|(id, _)| {
            tree_order
                .iter()
                .position(|tree_id| tree_id == id)
                .unwrap_or(usize::MAX)
        });
    }
    let mut layers: Vec<ProjectLayer> = layers.into_iter().map(|(_, layer)| layer).collect();

    // Older projects have no <projectlayers>; read the layer tree instead.
    if layers.is_empty() {
        for tree_layer in root
            .descendants()
            .filter(|n| n.has_tag_name("layer-tree-layer"))
        {
            let id = tree_layer.attribute("id").unwrap_or("").to_string();
            let name = tree_layer.attribute("name").unwrap_or(&id).to_string();
            let source = tree_layer
                .attribute("source")
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let visible = tree_layer
                .attribute("checked")
                .map(|c| c == "Qt::Checked")
                .unwrap_or(true);
            layers.push(ProjectLayer {
                name,
                source,
                crs,
                visible,
                opacity: 1.0,
                style_ref: None,
                geometry_type: None,
            });
        }
    }

    let viewport = read_viewport(&root);

    Ok(Project {
        title,
        crs,
        layers,
        viewport,
    })
}

/// Reads the viewport, preferring the explicit `<view>` element and falling
/// back to the `<mapcanvas>` extent.
fn read_viewport(root: &roxmltree::Node) -> Viewport {
    if let Some(view) = root.descendants().find(|n| n.has_tag_name("view")) {
        if let Some(center) = view.attribute("center") {
            let mut parts = center.split(',');
            if let (Some(lon), Some(lat)) = (parts.next(), parts.next()) {
                if let (Ok(lon), Ok(lat)) = (lon.trim().parse(), lat.trim().parse()) {
                    let zoom = view
                        .attribute("zoom")
                        .and_then(|z| z.trim().parse().ok())
                        .unwrap_or(0.0);
                    let width = view
                        .attribute("width")
                        .and_then(|w| w.trim().parse().ok())
                        .unwrap_or(DEFAULT_VIEWPORT.0);
                    let height = view
                        .attribute("height")
                        .and_then(|h| h.trim().parse().ok())
                        .unwrap_or(DEFAULT_VIEWPORT.1);
                    return Viewport {
                        center_lon_lat: (lon, lat),
                        zoom,
                        size: (width, height),
                    };
                }
            }
        }
    }

    // Fall back to the <mapcanvas> extent (preferring the map canvas over any
    // per-layer metadata extent).
    let canvas_extent = root
        .descendants()
        .find(|n| n.has_tag_name("mapcanvas"))
        .and_then(|canvas| canvas.children().find(|n| n.has_tag_name("extent")))
        .or_else(|| root.descendants().find(|n| n.has_tag_name("extent")));
    if let Some(extent) = canvas_extent {
        let value = |tag: &str| -> Option<f64> {
            extent
                .children()
                .find(|n| n.has_tag_name(tag))
                .and_then(|n| n.text())
                .and_then(|t| t.trim().parse().ok())
        };
        if let (Some(xmin), Some(ymin), Some(xmax), Some(ymax)) =
            (value("xmin"), value("ymin"), value("xmax"), value("ymax"))
        {
            let size = DEFAULT_VIEWPORT;
            let width_rad = (xmax - xmin).to_radians();
            let zoom = if width_rad > 0.0 {
                (2.0 * PI * size.0 / (TILE_SIZE * width_rad)).log2()
            } else {
                0.0
            };
            return Viewport {
                center_lon_lat: ((xmin + xmax) / 2.0, (ymin + ymax) / 2.0),
                zoom,
                size,
            };
        }
    }

    Viewport {
        center_lon_lat: (0.0, 0.0),
        zoom: 0.0,
        size: DEFAULT_VIEWPORT,
    }
}

/// Parses the EPSG code from a `<spatialrefsys>` (or any ancestor of one).
fn parse_srs(node: &roxmltree::Node) -> Option<SrsId> {
    let authid = node
        .descendants()
        .find(|n| n.has_tag_name("authid"))
        .and_then(|n| n.text());
    if let Some(authid) = authid {
        if let Some(code) = authid.trim().strip_prefix("EPSG:") {
            if let Ok(epsg) = code.trim().parse::<i32>() {
                return Some(SrsId::from_epsg(epsg));
            }
        }
    }
    let srid = node
        .descendants()
        .find(|n| n.has_tag_name("srid"))
        .and_then(|n| n.text());
    if let Some(srid) = srid {
        if let Ok(epsg) = srid.trim().parse::<i32>() {
            if epsg > 0 {
                return Some(SrsId::from_epsg(epsg));
            }
        }
    }
    None
}

/// Returns the trimmed text of the first direct child with the given tag.
fn child_text(node: &roxmltree::Node, tag: &str) -> Option<String> {
    node.children()
        .find(|n| n.has_tag_name(tag))
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
}

// ---------------------------------------------------------------------------
// AppState integration
// ---------------------------------------------------------------------------

/// Captures the current application state as a serializable [`Project`].
pub fn project_from_state(state: &AppState) -> Project {
    let layers = state
        .registry
        .iter()
        .map(|layer| ProjectLayer {
            name: layer.name.clone(),
            source: layer.source.clone(),
            crs: layer.crs,
            visible: layer.visible,
            opacity: layer.opacity,
            style_ref: layer.style_ref.clone(),
            geometry_type: layer
                .features
                .first()
                .map(|f| f.geometry_type().to_string()),
        })
        .collect();
    Project {
        title: "QGIS Rust Project".into(),
        crs: state.project_crs,
        layers,
        viewport: Viewport::from_camera(&state.camera),
    }
}

/// Saves the current application state to a `.qgs` project file.
pub fn save_project(state: &AppState, path: &str) -> Result<(), String> {
    let xml = write_qgs(&project_from_state(state));
    std::fs::write(path, xml).map_err(|e| e.to_string())
}

/// Opens a `.qgs` project file into `state`, reloading layers from their
/// recorded data sources and re-parsing their style references.
pub fn open_project(state: &mut AppState, path: &str) -> Result<(), String> {
    let xml =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let project = read_qgs(&xml)?;

    let mut registry = LayerRegistry::new();
    let mut styles = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    for (index, project_layer) in project.layers.iter().enumerate() {
        let mut layer = match project_layer.source.as_deref() {
            Some(source) if !source.is_empty() => match load_source_layer(source) {
                Ok(mut loaded) => {
                    loaded.name = project_layer.name.clone();
                    loaded
                }
                Err(e) => {
                    warnings.push(format!("{}: {e}", project_layer.name));
                    Layer::new(project_layer.name.clone(), project_layer.crs)
                }
            },
            _ => Layer::new(project_layer.name.clone(), project_layer.crs),
        };
        layer.crs = project_layer.crs;
        layer.visible = project_layer.visible;
        layer.opacity = project_layer.opacity;
        layer.source = project_layer.source.clone();
        layer.style_ref = project_layer.style_ref.clone();

        if let Some(style_ref) = &project_layer.style_ref {
            match parse_qml_file(style_ref) {
                Ok(model) => {
                    styles.insert(index, model);
                }
                Err(e) => warnings.push(format!("style {style_ref}: {e}")),
            }
        }

        registry.add_layer(layer);
    }

    let layer_count = project.layers.len();
    state.registry = registry;
    state.styles = styles;
    state.camera = project.viewport.to_camera();
    state.project_crs = project.crs;
    state.selected_layer = if layer_count == 0 { None } else { Some(0) };
    state.generation += 1;
    state.status = if warnings.is_empty() {
        format!("Project loaded from {path} ({layer_count} layers)")
    } else {
        format!(
            "Project loaded from {path} with {} warning(s): {}",
            warnings.len(),
            warnings.join("; ")
        )
    };
    Ok(())
}

/// Loads a layer from a data source path, dispatching on the file extension.
fn load_source_layer(path: &str) -> Result<Layer, String> {
    if path.ends_with(".shp") {
        load_shapefile_as_layer(path)
    } else if path.ends_with(".geojson") || path.ends_with(".json") {
        load_geojson_as_layer(path)
    } else if path.ends_with(".gpkg") {
        Err("GeoPackage project layers are not yet supported".into())
    } else {
        Err(format!("unsupported data source: {path}"))
    }
}

// ---------------------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------------------

/// Escapes text content for XML.
fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escapes an attribute value for XML.
fn esc_attr(s: &str) -> String {
    esc_text(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use qgis_core::Feature;
    use qgis_geometry::{Geometry, Point};

    fn sample_project() -> Project {
        Project {
            title: "Sample".into(),
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
                    name: "roads & rails".into(),
                    source: Some("/data/roads.shp".into()),
                    crs: SrsId::from_epsg(4326),
                    visible: false,
                    opacity: 0.5,
                    style_ref: None,
                    geometry_type: Some("LineString".into()),
                },
            ],
            viewport: Viewport {
                center_lon_lat: (2.5, 47.5),
                zoom: 6.25,
                size: (1024.0, 768.0),
            },
        }
    }

    #[test]
    fn qgs_round_trips_project_state() {
        let project = sample_project();
        let xml = write_qgs(&project);
        let parsed = read_qgs(&xml).expect("written XML should parse");
        assert_eq!(parsed, project);
    }

    #[test]
    fn write_qgs_escapes_xml_metacharacters() {
        let mut project = sample_project();
        project.layers[0].name = "a<b>&\"c".into();
        let xml = write_qgs(&project);
        assert!(!xml.contains("a<b>"), "raw metacharacters must be escaped");
        let parsed = read_qgs(&xml).unwrap();
        assert_eq!(parsed.layers[0].name, "a<b>&\"c");
    }

    #[test]
    fn read_qgs_reads_real_qgis_layer_tree_without_projectlayers() {
        let xml = r#"<!DOCTYPE qgis PUBLIC 'http://mrcc.com/qgis.dtd' 'SYSTEM'>
<qgis version="3.7.0-Master">
  <projectCrs><spatialrefsys><authid>EPSG:3857</authid></spatialrefsys></projectCrs>
  <layer-tree-group>
    <layer-tree-layer name="Land" checked="Qt::Checked" source="./polys.shp" providerKey="ogr" id="polys1"/>
    <layer-tree-layer name="Roads" checked="Qt::Unchecked" source="./lines.shp" providerKey="ogr" id="lines1"/>
  </layer-tree-group>
  <mapcanvas>
    <extent><xmin>0</xmin><ymin>0</ymin><xmax>10</xmax><ymax>10</ymax></extent>
  </mapcanvas>
</qgis>"#;
        let project = read_qgs(xml).unwrap();
        assert_eq!(project.crs, SrsId::from_epsg(3857));
        assert_eq!(project.layers.len(), 2);
        assert_eq!(project.layers[0].name, "Land");
        assert!(project.layers[0].visible);
        assert_eq!(project.layers[0].source.as_deref(), Some("./polys.shp"));
        assert!(!project.layers[1].visible);
        assert_eq!(project.viewport.center_lon_lat, (5.0, 5.0));
        assert!(project.viewport.zoom > 0.0);
    }

    #[test]
    fn read_qgs_rejects_non_qgis_xml() {
        assert!(read_qgs("<html><body/></html>").is_err());
        assert!(read_qgs("not xml").is_err());
    }

    #[test]
    fn read_qgs_parses_bundled_qgis_sample_projects() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let samples = [
            ("../qgs-projects/xyztiles.qgs", vec!["Land", "Roads"], 3857),
            (
                "../qgs-projects/layer_rendering_order_qgis3_project.qgs",
                vec!["x", "y"],
                4326,
            ),
        ];
        for (file, expected_names, expected_epsg) in samples {
            let path = root.join(file);
            let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {file}: {e}"));
            let project = read_qgs(&xml).unwrap_or_else(|e| panic!("parse {file}: {e}"));
            let names: Vec<&str> = project.layers.iter().map(|l| l.name.as_str()).collect();
            assert_eq!(names, expected_names, "{file}");
            assert_eq!(project.crs.epsg(), expected_epsg, "{file} project CRS");
        }
    }

    #[test]
    fn viewport_from_camera_round_trips_within_tolerance() {
        let camera = Camera::new((0.5, 0.25), 7.5, (1024.0, 768.0));
        let viewport = Viewport::from_camera(&camera);
        let restored = viewport.to_camera();
        assert!((restored.zoom - camera.zoom).abs() < 1e-12);
        assert!((restored.center.0 - camera.center.0).abs() < 1e-12);
        assert!((restored.center.1 - camera.center.1).abs() < 1e-12);
    }

    #[test]
    fn open_project_restores_layer_metadata_and_geometry() {
        // A real shapefile so the layer can be reloaded from its source.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let shp = root.join("../provider-samples/france_parts.shp");
        let shp = shp.to_str().unwrap();

        let mut state = AppState::default();
        let mut layer = Layer::new("france_parts", SrsId::from_epsg(4326));
        layer.features.push(Feature::new(
            0,
            Some(Geometry::Point(Point::new(1.0, 2.0))),
        ));
        layer.source = Some(shp.to_string());
        layer.visible = false;
        layer.opacity = 0.25;
        state.registry.add_layer(layer);
        state.camera = Camera::new((0.5, 0.25), 5.0, (1024.0, 768.0));

        let dir = std::env::temp_dir();
        let path = dir.join(format!("qgis-mzw-open-{}.qgs", std::process::id()));
        let path_str = path.to_str().unwrap();
        save_project(&state, path_str).unwrap();

        let mut restored = AppState::default();
        open_project(&mut restored, path_str).unwrap();

        assert_eq!(restored.registry.len(), 1);
        let layer = restored.registry.layer_at(0).unwrap();
        assert_eq!(layer.name, "france_parts");
        assert_eq!(layer.source.as_deref(), Some(shp));
        assert!(!layer.visible);
        assert_eq!(layer.opacity, 0.25);
        assert!(
            layer.feature_count() > 1,
            "features should be reloaded from the source, got {}",
            layer.feature_count()
        );
        assert!((restored.camera.zoom - 5.0).abs() < 1e-12);

        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn open_project_keeps_layer_when_source_is_missing() {
        let project = Project {
            title: "Missing".into(),
            crs: SrsId::from_epsg(4326),
            layers: vec![ProjectLayer {
                name: "ghost".into(),
                source: Some("/nonexistent/ghost.shp".into()),
                crs: SrsId::from_epsg(4326),
                visible: true,
                opacity: 1.0,
                style_ref: None,
                geometry_type: None,
            }],
            viewport: Viewport {
                center_lon_lat: (0.0, 0.0),
                zoom: 3.0,
                size: DEFAULT_VIEWPORT,
            },
        };
        let path = std::env::temp_dir().join(format!(
            "qgis-mzw-missing-{}.qgs",
            std::process::id()
        ));
        let path_str = path.to_str().unwrap();
        std::fs::write(path_str, write_qgs(&project)).unwrap();

        let mut state = AppState::default();
        open_project(&mut state, path_str).unwrap();

        assert_eq!(state.registry.len(), 1, "invalid layer should still be listed");
        let layer = state.registry.layer_at(0).unwrap();
        assert_eq!(layer.name, "ghost");
        assert_eq!(layer.feature_count(), 0);
        assert!(state.status.contains("warning"), "status: {}", state.status);

        let _ = std::fs::remove_file(path_str);
    }
}
