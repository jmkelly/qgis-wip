//! WCS GetCapabilities handler.
//!
//! Produces a WCS 2.0.1 `Capabilities` document listing the coverages
//! published by a QGIS project, with the operations the service supports.
//!
//! Reference: OGC 09-025r2 section 7.3 (GetCapabilities response).

use bytes::Bytes;
use tracing::debug;

use crate::error::Result;
use crate::fcgi::FcgiResponse;
use crate::query::QueryParameters;
use crate::service::service_exception;
use crate::wms::project::{Project, ProjectLayer, load_project};

use super::get_coverage::SUPPORTED_COVERAGE_FORMATS;

/// XML namespace for WCS 2.0.1.
pub const WCS_NAMESPACE: &str = "http://www.opengis.net/wcs/2.0";

/// OWS namespace for common OGC web service elements.
pub const OWS_NAMESPACE: &str = "http://www.opengis.net/ows/2.0";

/// Default extent advertised for a coverage with no explicit bounds.
pub const DEFAULT_EXTENT: (f64, f64, f64, f64) = (-180.0, -90.0, 180.0, 90.0);

/// Escapes XML text content.
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Returns true when a project layer should be published as a coverage.
///
/// Raster layers are coverages by definition; vector layers are published as
/// point-wise coverages so a vector project still exposes a usable WCS.
pub fn is_coverage_layer(layer: &ProjectLayer) -> bool {
    matches!(layer.layer_type.as_str(), "raster" | "vector")
}

/// Returns the coverage identifier for a project layer.
pub fn coverage_id(layer: &ProjectLayer) -> String {
    layer.name.clone()
}

/// Builds the `ServiceType` section for WCS.
fn service_type_section() -> String {
    r#"    <ServiceType>urn:ogc:service:wcs</ServiceType>
    <ServiceTypeVersion>2.0.1</ServiceTypeVersion>
    <ServiceTypeVersion>1.1.1</ServiceTypeVersion>
    <ServiceTypeVersion>1.0.0</ServiceTypeVersion>"#
        .to_string()
}

/// Builds the `OperationsMetadata` section for WCS.
fn operations_metadata(base_url: &str) -> String {
    let mut ops = String::new();
    for op in ["GetCapabilities", "DescribeCoverage", "GetCoverage"] {
        ops.push_str(&format!(
            r#"      <ows:Operation name="{op}">
        <ows:DCP>
          <ows:HTTP>
            <ows:Get xlink:href="{base_url}/wcs?"/>
          </ows:HTTP>
        </ows:DCP>
      </ows:Operation>
"#,
            op = op,
            base_url = xml_escape(base_url),
        ));
    }
    ops
}

/// Builds the `<ServiceMetadata>` section.
fn service_metadata(base_url: &str, map: &str) -> String {
    format!(
        r#"    <ServiceMetadataURL xlink:href="{base_url}/wcs?SERVICE=WCS&amp;REQUEST=GetCapabilities&amp;MAP={map}"/>
"#,
        base_url = xml_escape(base_url),
        map = xml_escape(map),
    )
}

/// Builds the `SupportedFormats` list.
fn supported_formats() -> String {
    let formats: String = SUPPORTED_COVERAGE_FORMATS
        .iter()
        .map(|f| format!("      <Format>{}</Format>\n", xml_escape(f)))
        .collect();
    format!("    <SupportedFormats>\n{formats}    </SupportedFormats>\n")
}

/// Builds the `SupportedCRSs` list.
fn supported_crss() -> String {
    r#"    <SupportedCRSs>
      <requestResponseCRSs>EPSG:4326</requestResponseCRSs>
      <requestResponseCRSs>EPSG:3857</requestResponseCRSs>
      <nativeCRSs>EPSG:4326</nativeCRSs>
    </SupportedCRSs>
"#
    .to_string()
}

/// Builds a `<CoverageOffering>` element for a project layer.
fn coverage_offering(layer: &ProjectLayer) -> String {
    let id = coverage_id(layer);
    let formats: String = SUPPORTED_COVERAGE_FORMATS
        .iter()
        .map(|f| format!("      <SupportedFormat>{}</SupportedFormat>\n", xml_escape(f)))
        .collect();

    format!(
        r#"    <CoverageOffering gml:id="{id}">
      <CoverageId>{id}</CoverageId>
      <ows:Title>{title}</ows:Title>
      <ows:WGS84BoundingBox>
        <ows:LowerCorner>{minx} {miny}</ows:LowerCorner>
        <ows:UpperCorner>{maxx} {maxy}</ows:UpperCorner>
      </ows:WGS84BoundingBox>
      <boundedBy>
        <gml:Envelope srsName="urn:ogc:def:crs:OGC:1.3:CRS84">
          <gml:lowerCorner>{minx} {miny}</gml:lowerCorner>
          <gml:upperCorner>{maxx} {maxy}</gml:upperCorner>
        </gml:Envelope>
      </boundedBy>
      <SupportedFormats>
{formats}      </SupportedFormats>
      <SupportedCRSs>
        <requestResponseCRSs>EPSG:4326</requestResponseCRSs>
        <requestResponseCRSs>EPSG:3857</requestResponseCRSs>
        <nativeCRSs>{native_crs}</nativeCRSs>
      </SupportedCRSs>
    </CoverageOffering>
"#,
        id = xml_escape(&id),
        title = xml_escape(&layer.name),
        minx = DEFAULT_EXTENT.0,
        miny = DEFAULT_EXTENT.1,
        maxx = DEFAULT_EXTENT.2,
        maxy = DEFAULT_EXTENT.3,
        formats = formats,
        native_crs = xml_escape(layer.crs.as_deref().unwrap_or("EPSG:4326")),
    )
}

/// Builds the WCS GetCapabilities document for a project.
pub fn capabilities_document(project: &Project, base_url: &str, map: &str) -> String {
    let title = if project.name.is_empty() {
        "QGIS Rust Server".to_string()
    } else {
        project.name.clone()
    };

    let mut contents = String::new();
    for layer in &project.layers {
        if !layer.visible || !is_coverage_layer(layer) {
            continue;
        }
        contents.push_str(&coverage_offering(layer));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<wcs:Capabilities version="2.0.1"
  xmlns:wcs="{wcs}"
  xmlns:ows="{ows}"
  xmlns:gml="http://www.opengis.net/gml/3.2"
  xmlns:xlink="http://www.w3.org/1999/xlink">
  <ows:ServiceIdentification>
    <ows:Title>{title}</ows:Title>
    <ows:Abstract>WCS service for {title}</ows:Abstract>
    <ows:ServiceType>urn:ogc:service:wcs</ows:ServiceType>
    <ows:ServiceTypeVersion>2.0.1</ows:ServiceTypeVersion>
  </ows:ServiceIdentification>
  <ows:ServiceProvider>
    <ows:ProviderName>QGIS Rust Server</ows:ProviderName>
    <ows:ProviderSite xlink:href="{base_url}"/>
  </ows:ServiceProvider>
  <ows:OperationsMetadata>
{operations}  </ows:OperationsMetadata>
  <wcs:ServiceMetadata>
{service_type}{formats}{crss}{service_metadata}  </wcs:ServiceMetadata>
  <wcs:Contents>
{contents}  </wcs:Contents>
</wcs:Capabilities>
"#,
        wcs = WCS_NAMESPACE,
        ows = OWS_NAMESPACE,
        title = xml_escape(&title),
        base_url = xml_escape(base_url),
        operations = operations_metadata(base_url),
        service_type = service_type_section(),
        formats = supported_formats(),
        crss = supported_crss(),
        service_metadata = service_metadata(base_url, map),
        contents = contents,
    )
}

/// WCS GetCapabilities handler.
pub fn handle_get_capabilities(query: &QueryParameters, base_url: &str) -> Result<FcgiResponse> {
    debug!("handling WCS GetCapabilities");

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
        project.layers.push(ProjectLayer {
            id: "l1".to_string(),
            name: "Elevation".to_string(),
            datasource: "./dem.tif".to_string(),
            provider: "gdal".to_string(),
            layer_type: "raster".to_string(),
            geometry_type: None,
            visible: true,
            crs: Some("EPSG:32633".to_string()),
        });
        project.layers.push(ProjectLayer {
            id: "l2".to_string(),
            name: "Hidden".to_string(),
            datasource: "./x.tif".to_string(),
            provider: "gdal".to_string(),
            layer_type: "raster".to_string(),
            geometry_type: None,
            visible: false,
            crs: None,
        });
        project
    }

    #[test]
    fn document_root_and_namespaces() {
        let xml = capabilities_document(&sample_project(), "http://localhost:8080", "/p.qgs");
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<wcs:Capabilities version=\"2.0.1\""));
        assert!(xml.contains(WCS_NAMESPACE));
        assert!(xml.contains(OWS_NAMESPACE));
    }

    #[test]
    fn document_declares_all_operations() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("name=\"GetCapabilities\""));
        assert!(xml.contains("name=\"DescribeCoverage\""));
        assert!(xml.contains("name=\"GetCoverage\""));
    }

    #[test]
    fn document_lists_coverages() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("<CoverageId>Elevation</CoverageId>"));
        assert!(xml.contains("gml:Envelope"));
        assert!(xml.contains("urn:ogc:def:crs:OGC:1.3:CRS84"));
    }

    #[test]
    fn document_skips_hidden_layers() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(!xml.contains("<CoverageId>Hidden</CoverageId>"));
    }

    #[test]
    fn document_records_native_crs() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("<nativeCRSs>EPSG:32633</nativeCRSs>"));
    }

    #[test]
    fn document_defaults_native_crs() {
        let mut project = sample_project();
        project.layers[0].crs = None;
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<nativeCRSs>EPSG:4326</nativeCRSs>"));
    }

    #[test]
    fn document_declares_service_metadata() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("ServiceMetadataURL"));
        assert!(xml.contains("urn:ogc:service:wcs"));
        assert!(xml.contains("2.0.1"));
        assert!(xml.contains("1.1.1"));
    }

    #[test]
    fn document_lists_supported_formats_and_crss() {
        let xml = capabilities_document(&sample_project(), "http://h", "/p.qgs");
        assert!(xml.contains("SupportedFormats"));
        assert!(xml.contains("image/png"));
        assert!(xml.contains("SupportedCRSs"));
        assert!(xml.contains("EPSG:3857"));
    }

    #[test]
    fn document_with_no_visible_layers() {
        let mut project = sample_project();
        project.layers.clear();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<wcs:Contents>"));
        assert!(!xml.contains("CoverageOffering"));
    }

    #[test]
    fn document_uses_project_name() {
        let mut project = sample_project();
        project.name = "Terrain".to_string();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<ows:Title>Terrain</ows:Title>"));
    }

    #[test]
    fn document_falls_back_to_server_name() {
        let mut project = sample_project();
        project.name = String::new();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<ows:Title>QGIS Rust Server</ows:Title>"));
    }

    #[test]
    fn document_escapes_coverage_ids() {
        let mut project = sample_project();
        project.layers[0].name = "A & B".to_string();
        let xml = capabilities_document(&project, "http://h", "/p.qgs");
        assert!(xml.contains("<CoverageId>A &amp; B</CoverageId>"));
    }

    #[test]
    fn coverage_layer_filter() {
        let mut layer = sample_project().layers.remove(0);
        assert!(is_coverage_layer(&layer));
        layer.layer_type = "plugin".to_string();
        assert!(!is_coverage_layer(&layer));
    }

    #[test]
    fn coverage_id_matches_layer_name() {
        let layer = &sample_project().layers[0];
        assert_eq!(coverage_id(layer), "Elevation");
    }

    #[test]
    fn xml_escape_special_chars() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
        assert_eq!(xml_escape("<x>"), "&lt;x&gt;");
        assert_eq!(xml_escape("'q'"), "&apos;q&apos;");
    }

    #[test]
    fn handle_get_capabilities_missing_map() {
        let query = QueryParameters::parse("SERVICE=WCS&REQUEST=GetCapabilities").unwrap();
        let resp = handle_get_capabilities(&query, "http://h").unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("MissingParameterValue"));
        assert!(body.contains("MAP"));
    }

    #[test]
    fn handle_get_capabilities_bad_project() {
        let query =
            QueryParameters::parse("SERVICE=WCS&REQUEST=GetCapabilities&MAP=/nonexistent.qgs").unwrap();
        let resp = handle_get_capabilities(&query, "http://h").unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NoApplicableCode"));
    }
}