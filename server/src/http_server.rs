//! axum HTTP server for direct HTTP access to OGC services.
//!
//! Provides an HTTP interface alongside the FastCGI interface, useful for
//! testing and direct access without a web server frontend.

use axum::{
    body::Body,
    extract::{ConnectInfo, OriginalUri, Query as AxumQuery, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Router,
};
use bytes::Bytes;
use std::collections::HashMap;
use std::net::SocketAddr;
use tracing::debug;

use crate::error::ServerError;
use crate::query::QueryParameters;
use crate::service::{route, ServiceType};

/// Shared application state for the HTTP server.
#[derive(Clone)]
pub struct AppState {
    /// Server configuration.
    pub config: crate::config::ServerConfig,
    /// Shared authentication policy/cache.
    auth: crate::auth::Authenticator,
    /// Shared rate limiter.
    rate_limiter: crate::rate_limit::RateLimiter,
    /// Shared cache for static OGC responses.
    response_cache: crate::response_cache::ResponseCache,
}

impl AppState {
    /// Creates runtime state and shared caches from server configuration.
    pub fn new(config: crate::config::ServerConfig) -> Self {
        let auth = crate::auth::Authenticator::from_config(&config);
        let response_cache = crate::response_cache::ResponseCache::new(
            config.response_cache_ttl,
            config.response_cache_capacity,
        );
        Self {
            config,
            auth,
            rate_limiter: crate::rate_limit::RateLimiter::default(),
            response_cache,
        }
    }
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
        .route_layer(middleware::from_fn_with_state(state.clone(), security_middleware))
        .with_state(state)
}

/// Enforces configured credentials and per-client request limits.
async fn security_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !state.auth.authenticate(
        request.headers().get("authorization").and_then(|v| v.to_str().ok()),
        request.headers().get("x-api-key").and_then(|v| v.to_str().ok()),
    ) {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header("WWW-Authenticate", "Basic realm=\"QGIS Server\"")
            .body(Body::from("Unauthorized"))
            .unwrap();
    }

    let client = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip().to_string())
        .or_else(|| {
            let trusted = state.config.trusted_headers();
            let headers: HashMap<String, String> = request
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
                })
                .collect();
            trusted.extract(&headers).map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown-client".to_owned());
    if !state
        .rate_limiter
        .allow(&client, state.config.rate_limit_requests_per_second)
    {
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Retry-After", "1")
            .body(Body::from("Too Many Requests"))
            .unwrap();
    }

    next.run(request).await
}

/// Builds the externally reachable base URL for this server.
fn base_url(config: &crate::config::ServerConfig, headers: &HeaderMap) -> String {
    match headers.get("host").and_then(|v| v.to_str().ok()) {
        Some(host) if !host.is_empty() => format!("http://{host}"),
        _ => format!("http://{}:{}", config.host, config.port),
    }
}

/// Root handler — renders the landing page service catalog.
async fn root_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let base = base_url(&state.config, &headers);
    let body = crate::landing::render(&base, &state.config.host, state.config.port);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Body::from(body))
        .unwrap()
}

/// OGC service handler — routes to the appropriate service.
async fn ogc_handler(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
    headers: HeaderMap,
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

    let request_type = query.request().unwrap_or("");
    let is_cacheable = matches!(
        request_type.to_ascii_lowercase().as_str(),
        "getcapabilities" | "describefeaturetype" | "describecoverage"
    );
    // Keep the original URI in the key so distinct map selections and
    // encodings cannot collide, independent of HashMap iteration order.
    let cache_key = uri.to_string();
    if is_cacheable {
        if let Some(cached) = state.response_cache.get(&cache_key) {
            return Response::builder()
                .status(StatusCode::from_u16(cached.status).unwrap_or(StatusCode::OK))
                .header("Content-Type", cached.content_type)
                .header("X-QGIS-Cache", "HIT")
                .body(Body::from(cached.body))
                .unwrap();
        }
    }

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
            if let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) {
                p.insert("HTTP_HOST".to_string(), host.to_string());
            }
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

            let response_body = response.body;
            if is_cacheable && status == StatusCode::OK {
                state.response_cache.insert(
                    cache_key,
                    crate::response_cache::CachedResponse {
                        status: status.as_u16(),
                        content_type: content_type.to_owned(),
                        body: response_body.clone(),
                    },
                );
            }
            Response::builder()
                .status(status)
                .header("Content-Type", content_type)
                .header("X-QGIS-Cache", "MISS")
                .body(Body::from(response_body))
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
    let state = AppState::new(config.clone());

    let app = build_router(state);
    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("HTTP server listening on {}", addr);

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
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
        let state = AppState::new(config);
        let _router = build_router(state);
    }

    #[tokio::test]
    async fn root_handler_returns_xml() {
        let config = ServerConfig::default();
        let state = AppState::new(config);
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
        let state = AppState::new(config);
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
        let state = AppState::new(config);
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
        let state = AppState::new(config);
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
        let state = AppState::new(config);
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
