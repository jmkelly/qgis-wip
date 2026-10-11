/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host address to bind to.
    pub host: String,
    /// Port to listen on.
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

    /// Returns the bind address as a string.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
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

    #[test]
    fn bind_address() {
        let config = ServerConfig::new("192.168.1.1", 8888);
        assert_eq!(config.bind_address(), "192.168.1.1:8888");
    }
}
