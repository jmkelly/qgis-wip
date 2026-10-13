//! WMTS GetCapabilities handler.
//!
//! Produces a WMTS 1.0.0 `Capabilities` document describing the service,
//! its operations, and the layers and tile matrix sets published by a
//! QGIS project.
//!
//! Reference: OGC 07-057r7 section 6.2 (GetCapabilities response).

use bytes::Bytes;
use tracing::debug;

use crate::error::Result;
use crate::fcgi::FcgiResponse;
use crate::query::QueryParameters;
use crate::service::service_exception;
use crate::wms::project::{Project, load_project};

use super::get_tile::WMTS_TILE_FORMATS;
use super::tile_matrix::{GEOGRAPHIC_LEVELS, TileMatrixSet, geographic_matrix_set, tile_matrix_set};

/// XML namespace for WMTS 1.0.0.
pub const WMTS_NAMESPACE: &str = "http://www.opengis.net/wmts/1.0";

/// OWS namespace for common OGC web service elements.
pub const OWS_NAMESPACE: &str = "http://www.opengis.net/ows/1.1";

/// Default style identifier advertised for WMTS layers.
pub const DEFAULT_STYLE: &str = "default";

/// Escapes XML text content.
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Builds a `<Style>` element.
fn style_element() -> String {
    format!(
        r#"        <Style isDefault="true">
          <ows:Identifier>{DEFAULT_STYLE}</ows:Identifier>
        </Style>"#
    )
}

/// Builds the `<Formats>` element for a layer or tile matrix set.
fn formats_element(indent: &str) -> String {
    let formats: String = WMTS_TILE_FORMATS
        .iter()
        .map(|f| format!("{indent}<Format>{}</Format>\n", xml_escape(f)))
        .collect();
    format!("{indent}<Formats>\n{formats}{indent}</Formats>")
}

/// Builds a `<TileMatrixSetLink>` for a matrix set.
fn tile_matrix_set_link(set: &TileMatrixSet) -> String {
    format!(
        r#"        <TileMatrixSetLink>
          <TileMatrixSet>{}</TileMatrixSet>
        </TileMatrixSetLink>"#,
        xml_escape(&set.identifier)
    )
}

/// Builds a `<TileMatrixSet>` element with all levels.
fn tile_matrix_set_element(set: &TileMatrixSet) -> String {
    let mut out = format!(
        r#"      <TileMatrixSet id="{}">
        <ows:SupportedCRS>{}</ows:SupportedCRS>
        <ows:WellKnownScaleSet>{}</ows:WellKnownScaleSet>"#,
        xml_escape(&set.identifier),
        xml_escape(&set.crs),
        well_known_scale_set(&set.identifier),
    );

    // TileMatrixSetBrief is the child of the TileMatrixSet definition in
    // WMTS 1.0.0; the full TileMatrix list lives inside it.
    out.push_str("\n        <TileMatrixSetBrief>\n");
    for matrix in set.matrices() {
        out.push_str(&format!(
            r#"          <TileMatrix>
            <ows:Identifier>{}</ows:Identifier>
            <ScaleDenominator>{}</ScaleDenominator>
            <TopLeftCorner>{:.6} {:.6}</TopLeftCorner>
            <TileWidth>{}</TileWidth>
            <TileHeight>{}</TileHeight>
            <MatrixWidth>{}</MatrixWidth>
            <MatrixHeight>{}</MatrixHeight>
          </TileMatrix>
"#,
            xml_escape(matrix.identifier()),
            matrix.scale_denominator(),
            matrix.top_left_corner[0],
            matrix.top_left_corner[1],
            matrix.tile_width,
            matrix.tile_height,
            matrix.matrix_width,
            matrix.matrix_height,
        ));
    }
    out.push_str("        </TileMatrixSetBrief>\n");
    out.push_str("      </TileMatrixSet>");

    out
}

/// Returns the OGC well-known scale set name for a matrix set identifier.
fn well_known_scale_set(identifier: &str) -> &'static str {
    match identifier {
        "EPSG4326" => "urn:ogc:def:wkss:OGC:1.0:Geographic",
        "WebMercatorQuad" => "urn:ogc:def:wkss:OGC:1.0:WebMercatorQuad",
        _ => "urn:ogc:def:wkss:OGC:1.0:CRSBase",
    }
}

/// Builds a `<Layer>` element for a project layer.
fn layer_element(
    layer: &crate::wms::project::ProjectLayer,
    matrix_sets: &[TileMatrixSet],
    base_url: &str,
) -> String {
    let mut out = format!(
        r#"      <Layer>
        <ows:Title>{}</ows:Title>
        <ows:Abstract>{}</ows:Abstract>
        <ows:WGS84BoundingBox>
          <ows:LowerCorner>-180.0 -90.0</ows:LowerCorner>
          <ows:UpperCorner>180.0 90.0</ows:UpperCorner>
        </ows:WGS84BoundingBox>
        <ows:Identifier>{}</ows:Identifier>"#,
        xml_escape(&layer.name),
        xml_escape(&layer.name),
        xml_escape(&layer.name),
    );
    out.push('\n');
    out.push_str(&style_element());
    out.push('\n');
    out.push_str(&formats_element("        "));
    out.push('\n');
    out.push_str(&format!(
        r#"        <ResourceURL format="{format}" resourceType="tile" template="{base_url}/wmts?SERVICE=WMTS&amp;REQUEST=GetTile&amp;LAYER={layer}&amp;STYLE={style}&amp;TILEMATRIXSET={{TileMatrixSet}}&amp;TILEMATRIX={{TileMatrix}}&amp;TILEROW={{TileRow}}&amp;TILECOL={{TileCol}}"/>"#,
        format = xml_escape(WMTS_TILE_FORMATS[0]),
        base_url = xml_escape(base_url),
        layer = xml_escape(&layer.name),
        style = DEFAULT_STYLE,
    ));
    out.push('\n');
    for set in matrix_sets {
        out.push_str(&tile_matrix_set_link(set));
        out.push('\n');
    }
    out.push_str("      </Layer>");

    out
}

/// Returns the tile matrix sets advertised by the service.
fn advertised_matrix_sets() -> Vec<TileMatrixSet> {
    let mut sets = vec![geographic_matrix_set()];
    if let Some(mercator) = tile_matrix_set("WebMercatorQuad") {
        sets.push(mercator);
    }
    sets
}

/// Builds the WMTS GetCapabilities document for a project.
///
/// `base_url` is the externally reachable base URL used in operation
/// hrefs; `project` supplies the published layers.
pub fn capabilities_document(project: &Project, base_url: &str, map: &str) -> String {
    let matrix_sets = advertised_matrix_sets();
    let title = if project.name.is_empty() {
        "QGIS Rust Server".to_string()
    } else {
        project.name.clone()
    };

    let mut contents = String::new();
    for layer in &project.layers {
        if !layer.visible {
            continue;
        }
        contents.push_str(&layer_element(layer, &matrix_sets, base_url));
        contents.push('\n');
    }

    let mut matrix_set_docs = String::new();
    for set in &matrix_sets {
        matrix_set_docs.push_str(&tile_matrix_set_element(set));
        matrix_set_docs.push('\n');
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Capabilities version="1.0.0"
  xmlns="{ns}"
  xmlns:ows="{ows}"
  xmlns:xlink="http://www.w3.org/1999/xlink">
  <ows:ServiceIdentification>
    <ows:Title>{title}</ows:Title>
    <ows:Abstract>WMTS service for {title}</ows:Abstract>
    <ows:ServiceType>OGC WMTS</ows:ServiceType>
    <ows:ServiceTypeVersion>1.0.0</ows:ServiceTypeVersion>
  </ows:ServiceIdentification>
  <ows:ServiceProvider>
    <ows:ProviderName>QGIS Rust Server</ows:ProviderName>
    <ows:ProviderSite xlink:href="{base_url}"/>
  </ows:ServiceProvider>
  <ows:OperationsMetadata>
    <ows:Operation name="GetCapabilities">
      <ows:DCP>
        <ows:HTTP>
          <ows:Get xlink:href="{base_url}/wmts?"/>
        </ows:HTTP>
      </ows:DCP>
    </ows:Operation>
    <ows:Operation name="GetTile">
      <ows:DCP>
        <ows:HTTP>
          <ows:Get xlink:href="{base_url}/wmts?"/>
        </ows:HTTP>
      </ows:DCP>
    </ows:Operation>
    <ows:Operation name="GetFeatureInfo">
      <ows:DCP>
        <ows:HTTP>
          <ows:Get xlink:href="{base_url}/wmts?"/>
        </ows:HTTP>
      </ows:DCP>
    </ows:Operation>
  </ows:OperationsMetadata>
  <Contents>
{contents}{matrix_set_docs}  </Contents>
  <ServiceMetadataURL xlink:href="{base_url}/wmts?SERVICE=WMTS&amp;REQUEST=GetCapabilities&amp;MAP={map}"/>
</Capabilities>
"#,
        ns = WMTS_NAMESPACE,
        ows = OWS_NAMESPACE,
        title = xml_escape(&title),
        base_url = xml_escape(base_url),
        map = xml_escape(map),
        contents = contents,
        matrix_set_docs = matrix_set_docs,
    )
}

/// WMTS GetCapabilities handler.
pub fn handle_get_capabilities(query: &QueryParameters, base_url: &str) -> Result<FcgiResponse> {
    debug!("handling WMTS GetCapabilities");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    let project = match load_project(map_path) {
        Ok(p) => p,
        Err(e) => {
            return Ok(service_exception(
                "NoApplicableCode",
                format!("unable to load project: {e}"),
            ));
        }
    };

    let body = capabilities_document(&project, base_url, map_path);
    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// Number of levels advertised by the geographic matrix set.
///
/// Re-exported so callers can size level-based UI without importing the
/// tile matrix module directly.
pub const ADVERTISED_LEVELS: usize = GEOGRAPHIC_LEVELS;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project() -> Project {
        let mut project = Project {
            name: "Demo".to_string(),
            crs: Some("EPSG:4326".to_string()),
            layers: Vec::new(),
            styles: Default::default(),
        };
        project.layers.push(crate::wms::project::ProjectLayer {
            id: "l1".to_string(),
            name: "Roads".to_string(),
            datasource: "./roads.geojson".to_string(),
            provider: "geojson".to_string(),
            layer_type: "vector".to_string(),
            geometry_type: Some("LineString".to_string()),
            visible: true,
            crs: Some("EPSG:4326".to_string()),
        });
        project
    }

    #[test]
    fn document_is_well_formed_root() {
        let xml = capabilities_document(&sample_project(), "http://localhost:8080", "/p.qgs");
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<Capabilities version=\"1.0.0\""));
        assert!(xml.contains(WMTS_NAMESPACE));
        assert!(xml.contains(OWS_NAMESPACE));
    }

    #[test]
    fn document_declares_operations() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("name=\"GetCapabilities\""));
        assert!(xml.contains("name=\"GetTile\""));
        assert!(xml.contains("name=\"GetFeatureInfo\""));
    }

    #[test]
    fn document_lists_layers() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("<ows:Identifier>Roads</ows:Identifier>"));
        assert!(xml.contains("ResourceURL"));
        assert!(xml.contains("resourceType=\"tile\""));
    }

    #[test]
    fn document_skips_hidden_layers() {
        let mut project = sample_project();
        project.layers[0].visible = false;
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(!xml.contains("<ows:Identifier>Roads</ows:Identifier>"));
    }

    #[test]
    fn document_includes_tile_matrix_sets() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("TileMatrixSet id=\"EPSG4326\""));
        assert!(xml.contains("TileMatrixSet id=\"WebMercatorQuad\""));
        assert!(xml.contains("urn:ogc:def:wkss:OGC:1.0:Geographic"));
    }

    #[test]
    fn document_includes_all_levels() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert_eq!(xml.matches("<TileMatrix>").count(), GEOGRAPHIC_LEVELS * 2);
    }

    #[test]
    fn document_lists_formats() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("<Format>image/png</Format>"));
    }

    #[test]
    fn document_includes_metadata_url() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("ServiceMetadataURL"));
        assert!(xml.contains("SERVICE=WMTS"));
    }

    #[test]
    fn document_with_no_layers() {
        let mut project = sample_project();
        project.layers.clear();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<Contents>"));
        assert!(!xml.contains("<Layer>"));
    }

    #[test]
    fn document_uses_project_name_as_title() {
        let mut project = sample_project();
        project.name = "My Map".to_string();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<ows:Title>My Map</ows:Title>"));
    }

    #[test]
    fn document_falls_back_to_server_name() {
        let mut project = sample_project();
        project.name = String::new();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<ows:Title>QGIS Rust Server</ows:Title>"));
    }

    #[test]
    fn xml_escape_special_chars() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
        assert_eq!(xml_escape("<x>"), "&lt;x&gt;");
        assert_eq!(xml_escape("\"q\""), "&quot;q&quot;");
    }

    #[test]
    fn document_escapes_layer_names() {
        let mut project = sample_project();
        project.layers[0].name = "Roads & <Bridges>".to_string();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("Roads &amp; &lt;Bridges&gt;"));
        assert!(!xml.contains("<Bridges>"));
    }

    #[test]
    fn handle_get_capabilities_missing_map() {
        let query = QueryParameters::parse("SERVICE=WMTS&REQUEST=GetCapabilities").unwrap();
        let resp = handle_get_capabilities(&query, "http://h").unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("MissingParameterValue"));
        assert!(body.contains("MAP"));
    }

    #[test]
    fn handle_get_capabilities_bad_project() {
        let query =
            QueryParameters::parse("SERVICE=WMTS&REQUEST=GetCapabilities&MAP=/nonexistent.qgs").unwrap();
        let resp = handle_get_capabilities(&query, "http://h").unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NoApplicableCode"));
    }
}