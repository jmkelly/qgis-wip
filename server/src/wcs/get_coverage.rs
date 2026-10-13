//! WCS DescribeCoverage and GetCoverage handlers.
//!
//! `DescribeCoverage` returns the full definition of one coverage (domain
//! set, range type, supported formats and CRSs). `GetCoverage` renders the
//! coverage over a requested subset.
//!
//! Reference: OGC 09-025r2 sections 8.4 (DescribeCoverage) and 8.5 (GetCoverage).

use bytes::Bytes;
use qgis_render::Pixel;
use tracing::debug;

use crate::error::Result;
use crate::fcgi::FcgiResponse;
use crate::query::QueryParameters;
use crate::service::service_exception;
use crate::wms::get_map::{GetMapParams, WmsLayer, load_layers_from_project, render_map};
use crate::wms::project::{Project, ProjectLayer, load_project};

use super::get_capabilities::{
    DEFAULT_EXTENT, OWS_NAMESPACE, WCS_NAMESPACE, coverage_id, xml_escape,
};

/// Default coverage output format.
///
/// Only formats the renderer can encode are advertised; GeoTIFF encoding is
/// tracked separately and is not yet offered.
pub const DEFAULT_COVERAGE_FORMAT: &str = "image/png";

/// Coverage formats this server can encode.
pub const SUPPORTED_COVERAGE_FORMATS: &[&str] = &["image/png"];

/// Maximum output dimension for a rendered coverage.
pub const MAX_COVERAGE_DIMENSION: u32 = 4096;

/// Default output dimension when no size is requested.
pub const DEFAULT_COVERAGE_DIMENSION: u32 = 512;

/// Parsed and validated WCS GetCoverage parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GetCoverageParams {
    /// Coverage identifier (COVERAGEID parameter).
    pub coverage_id: String,
    /// Output MIME type.
    pub format: String,
    /// Requested output CRS, if any.
    pub output_crs: Option<String>,
    /// Requested subset bounding box, if any.
    pub subset: Option<(f64, f64, f64, f64)>,
    /// Requested output width in pixels.
    pub width: u32,
    /// Requested output height in pixels.
    pub height: u32,
    /// Whether the background is transparent.
    pub transparent: bool,
}

impl GetCoverageParams {
    /// Parses GetCoverage parameters from query string parameters.
    pub fn from_query(query: &QueryParameters) -> std::result::Result<Self, FcgiResponse> {
        let id = coverage_id_param(query)?;
        let format = query.get_or("FORMAT", DEFAULT_COVERAGE_FORMAT).to_string();
        if !SUPPORTED_COVERAGE_FORMATS.contains(&format.as_str()) {
            return Err(service_exception(
                "InvalidParameterValue",
                format!(
                    "Unsupported FORMAT '{}'. Supported: {}",
                    format,
                    SUPPORTED_COVERAGE_FORMATS.join(", ")
                ),
            ));
        }

        let output_crs = query.crs().map(|s| s.to_string());
        let subset = match query.get("SUBSET") {
            Some(raw) => Some(parse_subset(raw)?),
            None => None,
        };

        let width = parse_dimension(query.get("OUTPUTWIDTH"), "OUTPUTWIDTH")?
            .unwrap_or(DEFAULT_COVERAGE_DIMENSION);
        let height = parse_dimension(query.get("OUTPUTHEIGHT"), "OUTPUTHEIGHT")?
            .unwrap_or(DEFAULT_COVERAGE_DIMENSION);

        let transparent = query
            .get("TRANSPARENT")
            .map(|v| v.eq_ignore_ascii_case("TRUE"))
            .unwrap_or(false);

        Ok(Self {
            coverage_id: id,
            format,
            output_crs,
            subset,
            width,
            height,
            transparent,
        })
    }

    /// Returns the bounding box the coverage should be rendered over.
    pub fn bounds(&self, project_crs: Option<&str>) -> (f64, f64, f64, f64) {
        let fallback = project_crs.map(|_| DEFAULT_EXTENT).unwrap_or(DEFAULT_EXTENT);
        self.subset.unwrap_or(fallback)
    }
}

/// Extracts and validates the COVERAGEID parameter.
fn coverage_id_param(query: &QueryParameters) -> std::result::Result<String, FcgiResponse> {
    // WCS 2.0.1 uses COVERAGEID; the legacy COVERAGE spelling is accepted too.
    let id = query
        .get("COVERAGEID")
        .or_else(|| query.get("COVERAGE"))
        .unwrap_or("");

    if id.is_empty() {
        return Err(service_exception(
            "MissingParameterValue",
            "COVERAGEID parameter is required",
        ));
    }

    Ok(id.to_string())
}

/// Parses a `SUBSET` expression of the form
/// `LatLong bbox(minx,miny,maxx,maxy)` (the `Slice` form is not supported).
fn parse_subset(raw: &str) -> std::result::Result<(f64, f64, f64, f64), FcgiResponse> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(service_exception(
            "InvalidParameterValue",
            "SUBSET is empty",
        ));
    }

    if trimmed.contains("Slice(") {
        return Err(service_exception(
            "NoApplicableCode",
            "Slice subsetting is not supported; use bbox() subsetting",
        ));
    }

    let open = trimmed.find('(').ok_or_else(|| {
        service_exception(
            "InvalidParameterValue",
            format!("SUBSET must be of the form 'bbox(minx,miny,maxx,maxy)': {raw}"),
        )
    })?;
    let close = trimmed.rfind(')').ok_or_else(|| {
        service_exception(
            "InvalidParameterValue",
            format!("SUBSET is missing a closing parenthesis: {raw}"),
        )
    })?;
    if close <= open {
        return Err(service_exception(
            "InvalidParameterValue",
            format!("SUBSET has unbalanced parentheses: {raw}"),
        ));
    }

    let body = &trimmed[open + 1..close];
    let values: std::result::Result<Vec<f64>, _> = body.split(',').map(|v| v.trim().parse::<f64>()).collect();
    let values = values.map_err(|_| {
        service_exception(
            "InvalidParameterValue",
            format!("SUBSET bbox contains a non-numeric value: {raw}"),
        )
    })?;

    if values.len() != 4 {
        return Err(service_exception(
            "InvalidParameterValue",
            format!("SUBSET bbox must have 4 values, got {}", values.len()),
        ));
    }

    if values[0] >= values[2] || values[1] >= values[3] {
        return Err(service_exception(
            "InvalidParameterValue",
            format!("SUBSET bbox must be ordered minx,miny,maxx,maxy: {raw}"),
        ));
    }

    Ok((values[0], values[1], values[2], values[3]))
}

/// Parses an output dimension parameter.
fn parse_dimension(
    value: Option<&str>,
    name: &str,
) -> std::result::Result<Option<u32>, FcgiResponse> {
    let Some(raw) = value else {
        return Ok(None);
    };

    let dim = raw.parse::<u32>().map_err(|_| {
        service_exception(
            "InvalidParameterValue",
            format!("{name} must be a positive integer"),
        )
    })?;

    if dim == 0 || dim > MAX_COVERAGE_DIMENSION {
        return Err(service_exception(
            "InvalidParameterValue",
            format!("{name} must be between 1 and {MAX_COVERAGE_DIMENSION}"),
        ));
    }

    Ok(Some(dim))
}

/// Finds a layer in the project by coverage identifier.
fn find_coverage(project: &Project, id: &str) -> Option<ProjectLayer> {
    project
        .layers
        .iter()
        .find(|l| coverage_id(l) == id)
        .cloned()
}

/// Builds the `CoverageDescription` document for a coverage.
pub fn describe_coverage_document(layer: &ProjectLayer) -> String {
    let id = coverage_id(layer);
    let formats: String = SUPPORTED_COVERAGE_FORMATS
        .iter()
        .map(|f| format!("      <SupportedFormat>{}</SupportedFormat>\n", xml_escape(f)))
        .collect();
    let native_crs = layer.crs.as_deref().unwrap_or("EPSG:4326");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<wcs:CoverageDescriptions xmlns:wcs="{wcs}" xmlns:ows="{ows}" xmlns:gml="http://www.opengis.net/gml/3.2">
  <wcs:CoverageDescription gml:id="{id}">
    <gml:boundedBy>
      <gml:Envelope srsName="urn:ogc:def:crs:OGC:1.3:CRS84" axisLabels="Lat Long">
        <gml:lowerCorner>{minx} {miny}</gml:lowerCorner>
        <gml:upperCorner>{maxx} {maxy}</gml:upperCorner>
      </gml:Envelope>
    </gml:boundedBy>
    <wcs:CoverageId>{id}</wcs:CoverageId>
    <gml:domainSet>
      <gml:RectifiedGrid dimension="2" gml:id="grid-{id}">
        <gml:limits>
          <gml:GridEnvelope>
            <gml:low>0 0</gml:low>
            <gml:high>{width} {height}</gml:high>
          </gml:GridEnvelope>
        </gml:limits>
        <gml:axisName>x</gml:axisName>
        <gml:axisName>y</gml:axisName>
        <gml:origin>
          <gml:Point gml:id="origin-{id}" srsName="urn:ogc:def:crs:OGC:1.3:CRS84">
            <gml:pos>{minx} {maxy}</gml:pos>
          </gml:Point>
        </gml:origin>
        <gml:offsetVector srsName="urn:ogc:def:crs:OGC:1.3:CRS84">{dx} 0</gml:offsetVector>
        <gml:offsetVector srsName="urn:ogc:def:crs:OGC:1.3:CRS84">0 {dy}</gml:offsetVector>
      </gml:RectifiedGrid>
    </gml:domainSet>
    <wcs:ServiceParameters>
      <wcs:CoverageSubtype>RectifiedGridCoverage</wcs:CoverageSubtype>
      <wcs:nativeFormat>image/png</wcs:nativeFormat>
    </wcs:ServiceParameters>
    <wcs:rangeType>
      <swe:DataRecord xmlns:swe="http://www.opengis.net/swe/2.0">
        <swe:field name="bands">
          <swe:Quantity>
            <swe:constraint>
              <swe:AllowedValues>
                <swe:interval>{minv} {maxv}</swe:interval>
              </swe:AllowedValues>
            </swe:constraint>
          </swe:Quantity>
        </swe:field>
      </swe:DataRecord>
    </wcs:rangeType>
    <wcs:SupportedCRSs>
      <wcs:requestResponseCRSs>EPSG:4326</wcs:requestResponseCRSs>
      <wcs:requestResponseCRSs>EPSG:3857</wcs:requestResponseCRSs>
      <wcs:nativeCRSs>{native_crs}</wcs:nativeCRSs>
    </wcs:SupportedCRSs>
    <wcs:SupportedFormats>
{formats}    </wcs:SupportedFormats>
    <wcs:SupportedInterpolations>
      <wcs:interpolationMethod>nearest neighbor</wcs:interpolationMethod>
      <wcs:interpolationMethod>bilinear</wcs:interpolationMethod>
    </wcs:SupportedInterpolations>
  </wcs:CoverageDescription>
</wcs:CoverageDescriptions>
"#,
        wcs = WCS_NAMESPACE,
        ows = OWS_NAMESPACE,
        id = xml_escape(&id),
        minx = DEFAULT_EXTENT.0,
        miny = DEFAULT_EXTENT.1,
        maxx = DEFAULT_EXTENT.2,
        maxy = DEFAULT_EXTENT.3,
        width = DEFAULT_COVERAGE_DIMENSION,
        height = DEFAULT_COVERAGE_DIMENSION,
        dx = (DEFAULT_EXTENT.2 - DEFAULT_EXTENT.0) / DEFAULT_COVERAGE_DIMENSION as f64,
        dy = (DEFAULT_EXTENT.1 - DEFAULT_EXTENT.3) / DEFAULT_COVERAGE_DIMENSION as f64,
        minv = 0,
        maxv = 255,
        native_crs = xml_escape(native_crs),
        formats = formats,
    )
}

/// WCS DescribeCoverage handler.
pub fn handle_describe_coverage(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WCS DescribeCoverage");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    let id = match coverage_id_param(query) {
        Ok(id) => id,
        Err(resp) => return Ok(resp),
    };

    let project = match load_project(map_path) {
        Ok(p) => p,
        Err(e) => {
            return Ok(service_exception(
                "NoApplicableCode",
                format!("unable to load project: {e}"),
            ));
        }
    };

    let Some(layer) = find_coverage(&project, &id) else {
        return Ok(service_exception(
            "NoApplicableCode",
            format!("coverage '{id}' is not published by this service"),
        ));
    };

    let body = describe_coverage_document(&layer);
    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// Renders a coverage over the requested subset.
pub fn render_coverage(
    params: &GetCoverageParams,
    layers: &[WmsLayer],
    project_crs: Option<&str>,
) -> std::result::Result<Vec<u8>, FcgiResponse> {
    let bbox = params.bounds(project_crs);
    let crs = params
        .output_crs
        .clone()
        .or_else(|| project_crs.map(|c| c.to_string()))
        .unwrap_or_else(|| "EPSG:4326".to_string());

    let map_params = GetMapParams {
        layers: vec![params.coverage_id.clone()],
        styles: vec!["default".to_string()],
        crs,
        bbox,
        width: params.width,
        height: params.height,
        format: params.format.clone(),
        transparent: params.transparent,
        bg_color: Pixel::rgb(255, 255, 255),
        exceptions: String::new(),
    };

    render_map(&map_params, layers)
}

/// WCS GetCoverage handler.
pub fn handle_get_coverage(query: &QueryParameters) -> Result<FcgiResponse> {
    let params = match GetCoverageParams::from_query(query) {
        Ok(p) => p,
        Err(resp) => return Ok(resp),
    };

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    debug!(
        coverage = %params.coverage_id,
        format = %params.format,
        size = format!("{}x{}", params.width, params.height),
        "WCS GetCoverage request"
    );

    let project = match load_project(map_path) {
        Ok(p) => p,
        Err(e) => {
            return Ok(service_exception(
                "NoApplicableCode",
                format!("unable to load project: {e}"),
            ));
        }
    };

    if find_coverage(&project, &params.coverage_id).is_none() {
        return Ok(service_exception(
            "NoApplicableCode",
            format!(
                "coverage '{}' is not published by this service",
                params.coverage_id
            ),
        ));
    }

    let layers = load_layers_from_project(map_path, std::slice::from_ref(&params.coverage_id));

    match render_coverage(&params, &layers, project.crs.as_deref()) {
        Ok(bytes) => Ok(FcgiResponse::new(Bytes::from(bytes))),
        Err(resp) => Ok(resp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::{Geometry, LineString, Point};
    use std::collections::HashMap;

    use crate::wms::get_map::{LayerStyle, WmsFeature};

    /// Returns the body of an error response as a string.
    fn err_body(result: std::result::Result<GetCoverageParams, FcgiResponse>) -> String {
        String::from_utf8_lossy(&result.unwrap_err().body).to_string()
    }

    fn make_query(params: &[(&str, &str)]) -> QueryParameters {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        QueryParameters::parse(&query_string).unwrap()
    }

    fn base_params() -> GetCoverageParams {
        GetCoverageParams {
            coverage_id: "roads".to_string(),
            format: DEFAULT_COVERAGE_FORMAT.to_string(),
            output_crs: None,
            subset: None,
            width: 64,
            height: 64,
            transparent: false,
        }
    }

    #[test]
    fn parse_full_params() {
        let query = make_query(&[
            ("COVERAGEID", "roads"),
            ("FORMAT", "image/png"),
            ("CRS", "EPSG:3857"),
            ("SUBSET", "LatLong bbox(0,0,10,10)"),
            ("OUTPUTWIDTH", "256"),
            ("OUTPUTHEIGHT", "128"),
            ("TRANSPARENT", "TRUE"),
        ]);
        let params = GetCoverageParams::from_query(&query).unwrap();
        assert_eq!(params.coverage_id, "roads");
        assert_eq!(params.format, "image/png");
        assert_eq!(params.output_crs.as_deref(), Some("EPSG:3857"));
        assert_eq!(params.subset, Some((0.0, 0.0, 10.0, 10.0)));
        assert_eq!(params.width, 256);
        assert_eq!(params.height, 128);
        assert!(params.transparent);
    }

    #[test]
    fn parse_defaults() {
        let query = make_query(&[("COVERAGEID", "roads")]);
        let params = GetCoverageParams::from_query(&query).unwrap();
        assert_eq!(params.format, DEFAULT_COVERAGE_FORMAT);
        assert_eq!(params.width, DEFAULT_COVERAGE_DIMENSION);
        assert_eq!(params.height, DEFAULT_COVERAGE_DIMENSION);
        assert!(params.subset.is_none());
        assert!(!params.transparent);
    }

    #[test]
    fn parse_accepts_legacy_coverage_param() {
        let query = make_query(&[("COVERAGE", "roads")]);
        let params = GetCoverageParams::from_query(&query).unwrap();
        assert_eq!(params.coverage_id, "roads");
    }

    #[test]
    fn parse_missing_coverage_id() {
        let query = make_query(&[("MAP", "/tmp/p.qgs")]);
        let body = err_body(GetCoverageParams::from_query(&query));
        assert!(body.contains("COVERAGEID"));
        assert!(body.contains("MissingParameterValue"));
    }

    #[test]
    fn parse_rejects_unknown_format() {
        let query = make_query(&[("COVERAGEID", "roads"), ("FORMAT", "image/tiff")]);
        let body = err_body(GetCoverageParams::from_query(&query));
        assert!(body.contains("InvalidParameterValue"));
    }

    #[test]
    fn parse_subset_bbox() {
        assert_eq!(
            parse_subset("LatLong bbox(-10.5,-20.5,30.5,40.5)").unwrap(),
            (-10.5, -20.5, 30.5, 40.5)
        );
        assert_eq!(parse_subset("bbox(0,0,1,1)").unwrap(), (0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn parse_subset_rejects_bad_input() {
        assert!(parse_subset("").is_err());
        assert!(parse_subset("LatLong bbox(0,0,1)").is_err());
        assert!(parse_subset("LatLong bbox(a,b,c,d)").is_err());
        assert!(parse_subset("LatLong bbox(0,0)").is_err());
        assert!(parse_subset("garbage").is_err());
    }

    #[test]
    fn parse_subset_rejects_unordered_bbox() {
        let err = parse_subset("bbox(10,0,5,10)").unwrap_err();
        let body = String::from_utf8_lossy(&err.body);
        assert!(body.contains("InvalidParameterValue"));
    }

    #[test]
    fn parse_subset_rejects_slice() {
        let err = parse_subset("LatLong Slice(dim(1))").unwrap_err();
        let body = String::from_utf8_lossy(&err.body);
        assert!(body.contains("NoApplicableCode"));
        assert!(body.contains("Slice"));
    }

    #[test]
    fn parse_dimension_bounds() {
        assert_eq!(parse_dimension(None, "OUTPUTWIDTH").unwrap(), None);
        assert_eq!(parse_dimension(Some("10"), "OUTPUTWIDTH").unwrap(), Some(10));
        assert!(parse_dimension(Some("0"), "OUTPUTWIDTH").is_err());
        assert!(parse_dimension(Some("-5"), "OUTPUTWIDTH").is_err());
        assert!(parse_dimension(Some("abc"), "OUTPUTWIDTH").is_err());
        assert!(parse_dimension(Some("99999"), "OUTPUTWIDTH").is_err());
    }

    #[test]
    fn parse_rejects_oversized_output() {
        let query = make_query(&[("COVERAGEID", "roads"), ("OUTPUTWIDTH", "100000")]);
        assert!(GetCoverageParams::from_query(&query).is_err());
    }

    #[test]
    fn bounds_default_to_world() {
        let params = base_params();
        assert_eq!(params.bounds(Some("EPSG:4326")), DEFAULT_EXTENT);
        assert_eq!(params.bounds(None), DEFAULT_EXTENT);
    }

    #[test]
    fn bounds_use_subset() {
        let mut params = base_params();
        params.subset = Some((1.0, 2.0, 3.0, 4.0));
        assert_eq!(params.bounds(None), (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn find_coverage_by_id() {
        let mut project = Project {
            name: "P".to_string(),
            crs: None,
            layers: Vec::new(),
            styles: Default::default(),
        };
        project.layers.push(ProjectLayer {
            id: "1".to_string(),
            name: "roads".to_string(),
            datasource: String::new(),
            provider: String::new(),
            layer_type: "raster".to_string(),
            geometry_type: None,
            visible: true,
            crs: None,
        });
        assert!(find_coverage(&project, "roads").is_some());
        assert!(find_coverage(&project, "missing").is_none());
    }

    #[test]
    fn describe_document_is_complete() {
        let mut project = Project {
            name: "P".to_string(),
            crs: Some("EPSG:4326".to_string()),
            layers: Vec::new(),
            styles: Default::default(),
        };
        project.layers.push(ProjectLayer {
            id: "1".to_string(),
            name: "roads".to_string(),
            datasource: String::new(),
            provider: String::new(),
            layer_type: "raster".to_string(),
            geometry_type: None,
            visible: true,
            crs: Some("EPSG:32633".to_string()),
        });

        let xml = describe_coverage_document(&project.layers[0]);
        assert!(xml.contains("<wcs:CoverageDescriptions"));
        assert!(xml.contains("<wcs:CoverageId>roads</wcs:CoverageId>"));
        assert!(xml.contains("RectifiedGrid"));
        assert!(xml.contains("GridEnvelope"));
        assert!(xml.contains("offsetVector"));
        assert!(xml.contains("EPSG:32633"));
        assert!(xml.contains("image/png"));
    }

    #[test]
    fn render_coverage_returns_png() {
        let params = base_params();
        let line = LineString::new(vec![Point::new(-10.0, 0.0), Point::new(10.0, 0.0)]);
        let layers = vec![WmsLayer {
            name: "roads".to_string(),
            features: vec![WmsFeature {
                geometry: Geometry::LineString(line),
                attributes: HashMap::new(),
            }],
            style: LayerStyle::default(),
        }];
        let png = render_coverage(&params, &layers, Some("EPSG:4326")).unwrap();
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn render_coverage_with_no_layers() {
        let png = render_coverage(&base_params(), &[], None).unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn describe_coverage_missing_map() {
        let query = make_query(&[("COVERAGEID", "roads")]);
        let resp = handle_describe_coverage(&query).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("MAP"));
    }

    #[test]
    fn describe_coverage_missing_id() {
        let query = make_query(&[("MAP", "/tmp/p.qgs")]);
        let resp = handle_describe_coverage(&query).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("COVERAGEID"));
    }

    #[test]
    fn describe_coverage_unknown_project() {
        let query = make_query(&[("MAP", "/nonexistent.qgs"), ("COVERAGEID", "roads")]);
        let resp = handle_describe_coverage(&query).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NoApplicableCode"));
    }

    #[test]
    fn get_coverage_missing_map() {
        let query = make_query(&[("COVERAGEID", "roads")]);
        let resp = handle_get_coverage(&query).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("MAP"));
    }

    #[test]
    fn get_coverage_unknown_project() {
        let query = make_query(&[("MAP", "/nonexistent.qgs"), ("COVERAGEID", "roads")]);
        let resp = handle_get_coverage(&query).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NoApplicableCode"));
    }
}