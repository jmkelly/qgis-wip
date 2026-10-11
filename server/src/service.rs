//! OGC service routing and request handlers.
//!
//! Routes requests to the appropriate OGC service handler (WMS, WFS, WCS, WMTS)
//! based on the SERVICE and REQUEST query parameters.

use bytes::Bytes;
use tracing::{debug, info};

use crate::error::{Result, ServerError};
use crate::fcgi::{FcgiRequest, FcgiResponse};
use crate::query::QueryParameters;

/// Supported OGC service types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    /// Web Map Service.
    Wms,
    /// Web Feature Service.
    Wfs,
    /// Web Coverage Service.
    Wcs,
    /// Web Map Tile Service.
    Wmts,
}

impl ServiceType {
    /// Parses a service type from a name string.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "WMS" => Some(Self::Wms),
            "WFS" => Some(Self::Wfs),
            "WCS" => Some(Self::Wcs),
            "WMTS" => Some(Self::Wmts),
            _ => None,
        }
    }

    /// Returns the service name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wms => "WMS",
            Self::Wfs => "WFS",
            Self::Wcs => "WCS",
            Self::Wmts => "WMTS",
        }
    }
}

/// Supported OGC request types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    /// GetCapabilities request.
    GetCapabilities,
    /// DescribeFeatureType request (WFS).
    DescribeFeatureType,
    /// GetFeature request (WFS).
    GetFeature,
    /// GetMap request (WMS).
    GetMap,
    /// GetFeatureInfo request (WMS).
    GetFeatureInfo,
    /// GetCoverage request (WCS).
    GetCoverage,
    /// GetTile request (WMTS).
    GetTile,
    /// GetLegendGraphic request (WMS).
    GetLegendGraphic,
    /// Unknown request type.
    Unknown,
}

impl RequestType {
    /// Parses a request type from a name string.
    pub fn from_name(s: &str) -> Self {
        match s {
            "GetCapabilities" => Self::GetCapabilities,
            "DescribeFeatureType" => Self::DescribeFeatureType,
            "GetFeature" => Self::GetFeature,
            "GetMap" => Self::GetMap,
            "GetFeatureInfo" => Self::GetFeatureInfo,
            "GetCoverage" => Self::GetCoverage,
            "GetTile" => Self::GetTile,
            "GetLegendGraphic" => Self::GetLegendGraphic,
            _ => Self::Unknown,
        }
    }

    /// Returns the request type name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GetCapabilities => "GetCapabilities",
            Self::DescribeFeatureType => "DescribeFeatureType",
            Self::GetFeature => "GetFeature",
            Self::GetMap => "GetMap",
            Self::GetFeatureInfo => "GetFeatureInfo",
            Self::GetCoverage => "GetCoverage",
            Self::GetTile => "GetTile",
            Self::GetLegendGraphic => "GetLegendGraphic",
            Self::Unknown => "Unknown",
        }
    }
}

/// Routes a request to the appropriate service handler.
pub fn route(request: &FcgiRequest) -> Result<FcgiResponse> {
    let query = QueryParameters::parse(request.query_string())?;

    let service_name = query.service().unwrap_or("");
    let service = ServiceType::from_name(service_name)
        .ok_or_else(|| ServerError::UnsupportedService(service_name.to_string()))?;

    let request_name = query.request().unwrap_or("");
    let request_type = RequestType::from_name(request_name);

    info!(
        service = service.as_str(),
        request = request_type.as_str(),
        uri = %request.request_uri(),
        "routing request"
    );

    match (service, request_type) {
        (ServiceType::Wms, RequestType::GetCapabilities) => handle_wms_get_capabilities(&query),
        (ServiceType::Wms, RequestType::GetMap) => crate::wms::handle_get_map(&query),
        (ServiceType::Wms, RequestType::GetFeatureInfo) => {
            crate::wms::handle_get_feature_info(&query)
        }
        (ServiceType::Wms, RequestType::GetLegendGraphic) => handle_wms_get_legend_graphic(&query),
        (ServiceType::Wfs, RequestType::GetCapabilities) => handle_wfs_get_capabilities(&query),
        (ServiceType::Wfs, RequestType::DescribeFeatureType) => {
            handle_wfs_describe_feature_type(&query)
        }
        (ServiceType::Wfs, RequestType::GetFeature) => handle_wfs_get_feature(&query),
        (ServiceType::Wcs, RequestType::GetCapabilities) => handle_wcs_get_capabilities(&query),
        (ServiceType::Wcs, RequestType::GetCoverage) => handle_wcs_get_coverage(&query),
        (ServiceType::Wmts, RequestType::GetCapabilities) => handle_wmts_get_capabilities(&query),
        (ServiceType::Wmts, RequestType::GetTile) => handle_wmts_get_tile(&query),
        (service, RequestType::Unknown) => Err(ServerError::UnsupportedRequestType(format!(
            "{} for {}",
            request_name,
            service.as_str()
        ))),
        (service, request_type) => Err(ServerError::UnsupportedRequestType(format!(
            "{} for {}",
            request_type.as_str(),
            service.as_str()
        ))),
    }
}

/// Generates an OGC ServiceException report.
pub fn service_exception(code: &str, message: impl Into<String>) -> FcgiResponse {
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="{}">{}</ServiceException>
</ServiceExceptionReport>"#,
        code,
        message.into()
    );
    FcgiResponse::new(Bytes::from(body))
}

/// WMS GetCapabilities handler.
fn handle_wms_get_capabilities(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WMS GetCapabilities");

    let version = query.get_or("VERSION", "1.3.0");
    let map_path = query.map().unwrap_or("");

    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Load project and generate capabilities XML
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<WMS_Capabilities version="{}" xmlns="http://www.opengis.net/wms">
  <Service>
    <Name>WMS</Name>
    <Title>QGIS Rust Server</Title>
  </Service>
  <Capability>
    <Request>
      <GetCapabilities>
        <Format>text/xml</Format>
      </GetCapabilities>
      <GetMap>
        <Format>image/png</Format>
      </GetMap>
    </Request>
  </Capability>
</WMS_Capabilities>"#,
        version
    );

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WMS GetLegendGraphic handler.
fn handle_wms_get_legend_graphic(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WMS GetLegendGraphic");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Generate legend graphic
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="NotImplemented">GetLegendGraphic not yet implemented</ServiceException>
</ServiceExceptionReport>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WFS GetCapabilities handler.
fn handle_wfs_get_capabilities(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WFS GetCapabilities");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Load project and generate capabilities XML
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<WFS_Capabilities version="2.0.0" xmlns="http://www.opengis.net/wfs/2.0">
  <ServiceIdentification>
    <Title>QGIS Rust Server</Title>
  </ServiceIdentification>
</WFS_Capabilities>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WFS DescribeFeatureType handler.
fn handle_wfs_describe_feature_type(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WFS DescribeFeatureType");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Describe feature types from project
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="NotImplemented">DescribeFeatureType not yet implemented</ServiceException>
</ServiceExceptionReport>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WFS GetFeature handler.
fn handle_wfs_get_feature(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WFS GetFeature");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Query features from project
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="NotImplemented">GetFeature not yet implemented</ServiceException>
</ServiceExceptionReport>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WCS GetCapabilities handler.
fn handle_wcs_get_capabilities(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WCS GetCapabilities");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Load project and generate capabilities XML
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<WCS_Capabilities version="2.0.1" xmlns="http://www.opengis.net/wcs/2.0">
  <ServiceIdentification>
    <Title>QGIS Rust Server</Title>
  </ServiceIdentification>
</WCS_Capabilities>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WCS GetCoverage handler.
fn handle_wcs_get_coverage(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WCS GetCoverage");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Get coverage from project
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="NotImplemented">GetCoverage not yet implemented</ServiceException>
</ServiceExceptionReport>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WMTS GetCapabilities handler.
fn handle_wmts_get_capabilities(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WMTS GetCapabilities");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Load project and generate capabilities XML
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<Capabilities version="1.0.0" xmlns="http://www.opengis.net/wmts/1.0">
  <ServiceIdentification>
    <Title>QGIS Rust Server</Title>
  </ServiceIdentification>
</Capabilities>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// WMTS GetTile handler.
fn handle_wmts_get_tile(query: &QueryParameters) -> Result<FcgiResponse> {
    debug!("handling WMTS GetTile");

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    // TODO: Render tile from project
    let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="NotImplemented">GetTile not yet implemented</ServiceException>
</ServiceExceptionReport>"#;

    Ok(FcgiResponse::new(Bytes::from(body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fcgi::NameValuePairs;

    fn make_request(query_string: &str) -> FcgiRequest {
        let mut params = NameValuePairs::new();
        params.insert("REQUEST_METHOD".to_string(), "GET".to_string());
        params.insert("QUERY_STRING".to_string(), query_string.to_string());
        params.insert("REQUEST_URI".to_string(), "/cgi-bin/qgis_mapserv.fcgi".to_string());

        FcgiRequest {
            request_id: 1,
            params,
            body: Bytes::new(),
        }
    }

    #[test]
    fn service_type_from_str() {
        assert_eq!(ServiceType::from_name("WMS"), Some(ServiceType::Wms));
        assert_eq!(ServiceType::from_name("wms"), Some(ServiceType::Wms));
        assert_eq!(ServiceType::from_name("WFS"), Some(ServiceType::Wfs));
        assert_eq!(ServiceType::from_name("WCS"), Some(ServiceType::Wcs));
        assert_eq!(ServiceType::from_name("WMTS"), Some(ServiceType::Wmts));
        assert_eq!(ServiceType::from_name("XYZ"), None);
    }

    #[test]
    fn service_type_as_str() {
        assert_eq!(ServiceType::Wms.as_str(), "WMS");
        assert_eq!(ServiceType::Wfs.as_str(), "WFS");
        assert_eq!(ServiceType::Wcs.as_str(), "WCS");
        assert_eq!(ServiceType::Wmts.as_str(), "WMTS");
    }

    #[test]
    fn request_type_from_str() {
        assert_eq!(RequestType::from_name("GetCapabilities"), RequestType::GetCapabilities);
        assert_eq!(RequestType::from_name("GetMap"), RequestType::GetMap);
        assert_eq!(RequestType::from_name("GetFeature"), RequestType::GetFeature);
        assert_eq!(RequestType::from_name("GetCoverage"), RequestType::GetCoverage);
        assert_eq!(RequestType::from_name("GetTile"), RequestType::GetTile);
        assert_eq!(RequestType::from_name("Unknown"), RequestType::Unknown);
    }

    #[test]
    fn route_wms_get_capabilities() {
        let req = make_request("SERVICE=WMS&REQUEST=GetCapabilities&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("WMS_Capabilities"));
    }

    #[test]
    fn route_wms_get_map() {
        let req = make_request("SERVICE=WMS&REQUEST=GetMap&MAP=/tmp/test.qgs&LAYERS=layer1");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("ServiceExceptionReport"));
    }

    #[test]
    fn route_wfs_get_capabilities() {
        let req = make_request("SERVICE=WFS&REQUEST=GetCapabilities&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("WFS_Capabilities"));
    }

    #[test]
    fn route_wcs_get_capabilities() {
        let req = make_request("SERVICE=WCS&REQUEST=GetCapabilities&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("WCS_Capabilities"));
    }

    #[test]
    fn route_wmts_get_capabilities() {
        let req = make_request("SERVICE=WMTS&REQUEST=GetCapabilities&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("Capabilities"));
    }

    #[test]
    fn route_unsupported_service() {
        let req = make_request("SERVICE=XYZ&REQUEST=GetCapabilities");
        let result = route(&req);
        assert!(result.is_err());
    }

    #[test]
    fn route_missing_map() {
        let req = make_request("SERVICE=WMS&REQUEST=GetCapabilities");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("MissingParameterValue"));
    }

    #[test]
    fn route_unknown_request() {
        let req = make_request("SERVICE=WMS&REQUEST=UnknownRequest&MAP=/tmp/test.qgs");
        let result = route(&req);
        assert!(result.is_err());
    }

    #[test]
    fn service_exception_format() {
        let resp = service_exception("TestCode", "Test message");
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("TestCode"));
        assert!(body.contains("Test message"));
        assert!(body.contains("ServiceExceptionReport"));
    }

    #[test]
    fn wms_get_map_missing_layers() {
        let req = make_request("SERVICE=WMS&REQUEST=GetMap&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("LAYERS"));
    }

    #[test]
    fn wms_get_feature_info() {
        let req = make_request("SERVICE=WMS&REQUEST=GetFeatureInfo&MAP=/tmp/test.qgs&LAYERS=layer1&QUERY_LAYERS=layer1&CRS=EPSG:4326&BBOX=-180,-90,180,90&WIDTH=800&HEIGHT=600&I=400&J=300");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("<html"));
    }

    #[test]
    fn wms_get_legend_graphic() {
        let req = make_request("SERVICE=WMS&REQUEST=GetLegendGraphic&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NotImplemented"));
    }

    #[test]
    fn wfs_describe_feature_type() {
        let req = make_request("SERVICE=WFS&REQUEST=DescribeFeatureType&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NotImplemented"));
    }

    #[test]
    fn wfs_get_feature() {
        let req = make_request("SERVICE=WFS&REQUEST=GetFeature&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NotImplemented"));
    }

    #[test]
    fn wcs_get_coverage() {
        let req = make_request("SERVICE=WCS&REQUEST=GetCoverage&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NotImplemented"));
    }

    #[test]
    fn wmts_get_tile() {
        let req = make_request("SERVICE=WMTS&REQUEST=GetTile&MAP=/tmp/test.qgs");
        let resp = route(&req).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("NotImplemented"));
    }
}
