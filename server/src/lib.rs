//! Server for the QGIS Rust port.
//!
//! Will provide WMS, WFS, and other OGC web service endpoints.

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl ServerConfig {
    /// Creates a new server config with the given host and port.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn custom_config() {
        let config = ServerConfig::new("0.0.0.0", 9090);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9090);
    }
}
