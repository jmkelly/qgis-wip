//! axum HTTP server for direct HTTP access to OGC services.
//!
//! Provides an HTTP interface alongside the FastCGI interface, useful for
//! testing and direct access without a web server frontend.

use axum::{
    body::Body,
    extract::{Query as AxumQuery, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use bytes::Bytes;
use std::collections::HashMap;
use tracing::debug;

use crate::error::ServerError;
use crate::query::QueryParameters;
use crate::service::{route, ServiceType};

/// Shared application state for the HTTP server.
#[derive(Clone)]
pub struct AppState {
    /// Server configuration.
    pub config: crate::config::ServerConfig,
}

/// Builds the axum router with all OGC service routes.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/wms", get(ogc_handler))
        .route("/wfs", get(ogc_handler))
        .route("/wcs", get(ogc_handler))
        .route("/wmts", get(ogc_handler))
        .route("/cgi-bin/qgis_mapserv.fcgi", get(ogc_handler))
        .with_state(state)
}

/// Root handler — returns service info.
async fn root_handler(State(state): State<AppState>) -> Response {
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<QGISServer>
  <Service>WMS</Service>
  <Service>WFS</Service>
  <Service>WCS</Service>
  <Service>WMTS</Service>
  <Host>{}</Host>
  <Port>{}</Port>
</QGISServer>"#,
        state.config.host, state.config.port
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/xml")
        .body(Body::from(body))
        .unwrap()
}

/// OGC service handler — routes to the appropriate service.
async fn ogc_handler(
    State(_state): State<AppState>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
    _headers: HeaderMap,
) -> Response {
    debug!("HTTP handler received request with {} query params", params.len());

    // Build query string from axum query params
    let query_string = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");

    let query = match QueryParameters::parse(&query_string) {
        Ok(q) => q,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/xml")
                .body(Body::from(format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="InvalidParameterValue">{}</ServiceException>
</ServiceExceptionReport>"#,
                    e
                )))
                .unwrap();
        }
    };

    // Check for required SERVICE parameter
    let service_name = query.service().unwrap_or("");
    if service_name.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/xml")
            .body(Body::from(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="MissingParameterValue">SERVICE parameter is required</ServiceException>
</ServiceExceptionReport>"#,
            ))
            .unwrap();
    }

    // Validate service type
    if ServiceType::from_name(service_name).is_none() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/xml")
            .body(Body::from(format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="InvalidParameterValue">Unsupported service: {}</ServiceException>
</ServiceExceptionReport>"#,
                service_name
            )))
            .unwrap();
    }

    // Build a minimal FcgiRequest from the HTTP request
    let fcgi_request = crate::fcgi::FcgiRequest {
        request_id: 0,
        params: {
            let mut p = crate::fcgi::NameValuePairs::new();
            p.insert("REQUEST_METHOD".to_string(), "GET".to_string());
            p.insert("QUERY_STRING".to_string(), query_string);
            p.insert("REQUEST_URI".to_string(), "/".to_string());
            p
        },
        body: Bytes::new(),
    };

    match route(&fcgi_request) {
        Ok(response) => {
            let status = if response.app_status == 0 {
                StatusCode::OK
            } else {
                StatusCode::from_u16(response.app_status as u16)
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            };

            // Detect content type from response body
            let content_type = detect_content_type(&response.body);

            Response::builder()
                .status(status)
                .header("Content-Type", content_type)
                .body(Body::from(response.body))
                .unwrap()
        }
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header("Content-Type", "application/xml")
            .body(Body::from(format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<ServiceExceptionReport version="1.3.0" xmlns="http://www.opengis.net/ogc">
  <ServiceException code="InternalError">{}</ServiceException>
</ServiceExceptionReport>"#,
                e
            )))
            .unwrap(),
    }
}

/// Detects the content type from the response body.
fn detect_content_type(body: &Bytes) -> &'static str {
    // Check for PNG magic bytes
    if body.len() >= 8 && &body[0..8] == b"\x89PNG\r\n\x1a\n" {
        return "image/png";
    }
    // Default to XML
    "application/xml"
}

/// Runs the axum HTTP server.
pub async fn run(config: crate::config::ServerConfig) -> Result<(), ServerError> {
    let state = AppState {
        config: config.clone(),
    };

    let app = build_router(state);
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("HTTP server listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    use tower::util::ServiceExt;

    #[test]
    fn build_router_compiles() {
        let config = ServerConfig::default();
        let state = AppState { config };
        let _router = build_router(state);
    }

    #[tokio::test]
    async fn root_handler_returns_xml() {
        let config = ServerConfig::default();
        let state = AppState { config };
        let app = build_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ogc_handler_missing_service() {
        let config = ServerConfig::default();
        let state = AppState { config };
        let app = build_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/wms")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ogc_handler_with_service() {
        let config = ServerConfig::default();
        let state = AppState { config };
        let app = build_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/wms?SERVICE=WMS&REQUEST=GetCapabilities&MAP=/tmp/test.qgs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ogc_handler_unsupported_service() {
        let config = ServerConfig::default();
        let state = AppState { config };
        let app = build_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/wms?SERVICE=XYZ&REQUEST=GetCapabilities")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn osgi_handler_fcgi_path() {
        let config = ServerConfig::default();
        let state = AppState { config };
        let app = build_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/cgi-bin/qgis_mapserv.fcgi?SERVICE=WMS&REQUEST=GetCapabilities&MAP=/tmp/test.qgs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
