//! Server for the QGIS Rust port.
//!
//! Provides WMS, WFS, WCS, and WMTS OGC web service endpoints via:
//! - FastCGI protocol (for web server integration)
//! - HTTP via axum (for direct access and testing)
//!
//! # Architecture
//!
//! The server has two frontends that share a common routing layer:
//!
//! 1. **FastCGI listener** (`fcgi_listener`) — accepts TCP connections,
//!    parses FastCGI records, converts them to requests, and routes them.
//! 2. **axum HTTP server** (`http_server`) — accepts HTTP requests and
//!    routes them through the same service layer.
//!
//! Both frontends feed into the `service` module, which routes to the
//! appropriate OGC service handler (WMS, WFS, WCS, WMTS).

pub mod auth;
pub mod config;
pub mod config_cache;
pub mod error;
pub mod fcgi;
pub mod fcgi_listener;
pub mod http_server;
pub mod query;
pub mod service;
pub mod wms;

pub use config::ServerConfig;
pub use error::{Result, ServerError};

/// Runs the server with both FastCGI and HTTP listeners.
///
/// This starts the FastCGI listener and HTTP server concurrently.
/// The FastCGI listener handles web server integration, while the
/// HTTP server provides direct access for testing.
pub async fn run(config: ServerConfig) -> Result<()> {
    let fcgi_config = config.clone();
    let http_config = config.clone();

    let fcgi_handle = tokio::spawn(async move {
        tracing::info!("starting FastCGI listener on {}", fcgi_config.bind_address());
        fcgi_listener::run(fcgi_config).await
    });

    let http_handle = tokio::spawn(async move {
        tracing::info!("starting HTTP server on {}", http_config.bind_address());
        http_server::run(http_config).await
    });

    // Wait for either server to finish (they run indefinitely)
    tokio::select! {
        result = fcgi_handle => {
            result.map_err(|e| ServerError::Internal(e.to_string()))??;
        }
        result = http_handle => {
            result.map_err(|e| ServerError::Internal(e.to_string()))??;
        }
    }

    Ok(())
}

/// Runs only the FastCGI listener.
pub async fn run_fcgi(config: ServerConfig) -> Result<()> {
    fcgi_listener::run(config).await
}

/// Runs only the HTTP server.
pub async fn run_http(config: ServerConfig) -> Result<()> {
    http_server::run(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn server_config_custom() {
        let config = ServerConfig::new("0.0.0.0", 9090);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9090);
    }
}
