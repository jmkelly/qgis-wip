//! Server configuration.

use std::time::Duration;

use crate::auth::{
    ApiKeyAuth, AuthCache, TrustedHeaders, DEFAULT_AUTH_CACHE_CAPACITY, DEFAULT_AUTH_CACHE_TTL,
};
use crate::config_cache::{ConfigCache, DEFAULT_CONFIG_CACHE_CAPACITY, DEFAULT_CONFIG_CACHE_TTL};

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host address to bind to.
    pub host: String,
    /// Port to listen on.
    pub port: u16,
    /// HTTP headers trusted to carry caller identity (set by a reverse proxy).
    pub trusted_headers: Vec<String>,
    /// Valid API keys for service authentication.
    pub api_keys: Vec<String>,
    /// TTL for cached authenticated credentials.
    pub auth_cache_ttl: Duration,
    /// Maximum number of cached authenticated credentials.
    pub auth_cache_capacity: usize,
    /// TTL for cached configuration values.
    pub config_cache_ttl: Duration,
    /// Maximum number of cached configuration entries.
    pub config_cache_capacity: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            trusted_headers: Vec::new(),
            api_keys: Vec::new(),
            auth_cache_ttl: DEFAULT_AUTH_CACHE_TTL,
            auth_cache_capacity: DEFAULT_AUTH_CACHE_CAPACITY,
            config_cache_ttl: DEFAULT_CONFIG_CACHE_TTL,
            config_cache_capacity: DEFAULT_CONFIG_CACHE_CAPACITY,
        }
    }
}

impl ServerConfig {
    /// Creates a new server config with the given host and port.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            ..Self::default()
        }
    }

    /// Returns the bind address as a string.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Sets the trusted headers (consuming builder).
    pub fn with_trusted_headers(mut self, headers: Vec<String>) -> Self {
        self.trusted_headers = headers;
        self
    }

    /// Sets the valid API keys (consuming builder).
    pub fn with_api_keys(mut self, keys: Vec<String>) -> Self {
        self.api_keys = keys;
        self
    }

    /// Sets the auth cache TTL (consuming builder).
    pub fn with_auth_cache_ttl(mut self, ttl: Duration) -> Self {
        self.auth_cache_ttl = ttl;
        self
    }

    /// Sets the auth cache capacity (consuming builder).
    pub fn with_auth_cache_capacity(mut self, capacity: usize) -> Self {
        self.auth_cache_capacity = capacity;
        self
    }

    /// Sets the config cache TTL (consuming builder).
    pub fn with_config_cache_ttl(mut self, ttl: Duration) -> Self {
        self.config_cache_ttl = ttl;
        self
    }

    /// Sets the config cache capacity (consuming builder).
    pub fn with_config_cache_capacity(mut self, capacity: usize) -> Self {
        self.config_cache_capacity = capacity;
        self
    }

    /// Builds the trusted header set from the configured header names.
    pub fn trusted_headers(&self) -> TrustedHeaders {
        TrustedHeaders::new(self.trusted_headers.clone())
    }

    /// Builds an auth cache from the configured TTL and capacity.
    pub fn auth_cache(&self) -> AuthCache {
        AuthCache::new(self.auth_cache_ttl, self.auth_cache_capacity)
    }

    /// Builds a config cache from the configured TTL and capacity.
    pub fn config_cache(&self) -> ConfigCache {
        ConfigCache::new(self.config_cache_ttl, self.config_cache_capacity)
    }

    /// Builds an API key authenticator from the configured keys and cache.
    pub fn api_key_auth(&self) -> ApiKeyAuth {
        ApiKeyAuth::from_config(self)
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
        assert_eq!(config.bind_address(), "127.0.0.1:8080");
        assert!(config.trusted_headers.is_empty());
        assert!(config.api_keys.is_empty());
        assert_eq!(config.auth_cache_ttl, Duration::from_secs(300));
        assert_eq!(config.auth_cache_capacity, 1024);
        assert_eq!(config.config_cache_ttl, Duration::from_secs(300));
        assert_eq!(config.config_cache_capacity, 1024);
    }

    #[test]
    fn custom_config() {
        let config = ServerConfig::new("0.0.0.0", 9090);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9090);
        assert_eq!(config.bind_address(), "0.0.0.0:9090");
    }

    #[test]
    fn builder_methods() {
        let config = ServerConfig::default()
            .with_trusted_headers(vec!["X-Qgis-User".to_string()])
            .with_api_keys(vec!["key".to_string()])
            .with_auth_cache_ttl(Duration::from_secs(60))
            .with_auth_cache_capacity(8)
            .with_config_cache_ttl(Duration::from_secs(120))
            .with_config_cache_capacity(16);
        assert_eq!(config.trusted_headers, vec!["X-Qgis-User".to_string()]);
        assert_eq!(config.api_keys, vec!["key".to_string()]);
        assert_eq!(config.auth_cache_ttl, Duration::from_secs(60));
        assert_eq!(config.auth_cache_capacity, 8);
        assert_eq!(config.config_cache_ttl, Duration::from_secs(120));
        assert_eq!(config.config_cache_capacity, 16);
    }

    #[test]
    fn cache_builders() {
        let config = ServerConfig::default()
            .with_trusted_headers(vec!["X-Qgis-User".to_string()])
            .with_api_keys(vec!["k1".to_string()]);
        let auth_cache = config.auth_cache();
        assert_eq!(auth_cache.ttl(), Duration::from_secs(300));
        assert_eq!(auth_cache.capacity(), 1024);
        let config_cache = config.config_cache();
        assert_eq!(config_cache.ttl(), Duration::from_secs(300));
        assert_eq!(config_cache.capacity(), 1024);
        let trusted = config.trusted_headers();
        assert!(trusted.is_trusted("x-qgis-user"));
        let api_auth = config.api_key_auth();
        assert!(api_auth.validate("k1"));
        assert!(!api_auth.validate("k2"));
    }
}
