//! Query parameter parsing for OGC service requests.
//!
//! Parses query strings from FastCGI PARAMS or HTTP query strings into
//! a structured representation for OGC service routing.

use std::collections::HashMap;

use crate::error::{Result, ServerError};

/// Parsed query parameters.
#[derive(Debug, Clone, Default)]
pub struct QueryParameters {
    /// Raw parameter map (key -> value).
    params: HashMap<String, String>,
}

impl QueryParameters {
    /// Creates a new empty query parameters set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a query string (e.g. "SERVICE=WMS&REQUEST=GetCapabilities").
    ///
    /// The query string should not include the leading '?'.
    pub fn parse(query_string: &str) -> Result<Self> {
        let mut params = HashMap::new();

        if query_string.is_empty() {
            return Ok(Self { params });
        }

        for pair in query_string.split('&') {
            if pair.is_empty() {
                continue;
            }

            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            let value = parts.next().unwrap_or("");

            if key.is_empty() {
                continue;
            }

            let decoded_key = percent_decode(key)?;
            let decoded_value = percent_decode(value)?;

            params.insert(decoded_key, decoded_value);
        }

        Ok(Self { params })
    }

    /// Returns a reference to the raw parameter map.
    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }

    /// Gets a parameter value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(|s| s.as_str())
    }

    /// Gets a required parameter value.
    pub fn require(&self, key: &'static str) -> Result<&str> {
        self.get(key)
            .ok_or(ServerError::MissingParameter(key))
    }

    /// Gets a parameter value or a default.
    pub fn get_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).unwrap_or(default)
    }

    /// Returns the service name (SERVICE parameter).
    pub fn service(&self) -> Option<&str> {
        self.get("SERVICE")
    }

    /// Returns the request type (REQUEST parameter).
    pub fn request(&self) -> Option<&str> {
        self.get("REQUEST")
    }

    /// Returns the version (VERSION parameter).
    pub fn version(&self) -> Option<&str> {
        self.get("VERSION")
    }

    /// Returns the layers (LAYERS parameter), comma-separated.
    pub fn layers(&self) -> Vec<&str> {
        self.get("LAYERS")
            .map(|s| s.split(',').collect())
            .unwrap_or_default()
    }

    /// Returns the CRS (CRS or SRS parameter).
    pub fn crs(&self) -> Option<&str> {
        self.get("CRS").or_else(|| self.get("SRS"))
    }

    /// Returns the map parameter (MAP parameter — path to .qgs project file).
    pub fn map(&self) -> Option<&str> {
        self.get("MAP")
    }

    /// Returns true if the parameter map is empty.
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    /// Returns the number of parameters.
    pub fn len(&self) -> usize {
        self.params.len()
    }
}

/// Percent-decodes a string.
fn percent_decode(input: &str) -> Result<String> {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err(ServerError::InvalidQueryEncoding(input.to_string()));
                }
                let hex = &input[i + 1..i + 3];
                let byte = u8::from_str_radix(hex, 16)
                    .map_err(|_| ServerError::InvalidQueryEncoding(input.to_string()))?;
                result.push(byte as char);
                i += 3;
            }
            b'+' => {
                result.push(' ');
                i += 1;
            }
            c => {
                result.push(c as char);
                i += 1;
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        let params = QueryParameters::parse("").unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn parse_single() {
        let params = QueryParameters::parse("SERVICE=WMS").unwrap();
        assert_eq!(params.get("SERVICE"), Some("WMS"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn parse_multiple() {
        let params = QueryParameters::parse("SERVICE=WMS&REQUEST=GetCapabilities").unwrap();
        assert_eq!(params.get("SERVICE"), Some("WMS"));
        assert_eq!(params.get("REQUEST"), Some("GetCapabilities"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn parse_percent_encoded() {
        let params = QueryParameters::parse("MAP=/path/to/my%20map.qgs").unwrap();
        assert_eq!(params.get("MAP"), Some("/path/to/my map.qgs"));
    }

    #[test]
    fn parse_plus_as_space() {
        let params = QueryParameters::parse("NAME=hello+world").unwrap();
        assert_eq!(params.get("NAME"), Some("hello world"));
    }

    #[test]
    fn parse_value_with_equals() {
        let params = QueryParameters::parse("FILTER=foo=bar").unwrap();
        assert_eq!(params.get("FILTER"), Some("foo=bar"));
    }

    #[test]
    fn parse_empty_value() {
        let params = QueryParameters::parse("KEY=").unwrap();
        assert_eq!(params.get("KEY"), Some(""));
    }

    #[test]
    fn parse_invalid_percent_encoding() {
        let result = QueryParameters::parse("KEY=%ZZ");
        assert!(result.is_err());
    }

    #[test]
    fn parse_truncated_percent_encoding() {
        let result = QueryParameters::parse("KEY=%2");
        assert!(result.is_err());
    }

    #[test]
    fn require_existing() {
        let params = QueryParameters::parse("SERVICE=WMS").unwrap();
        assert_eq!(params.require("SERVICE").unwrap(), "WMS");
    }

    #[test]
    fn require_missing() {
        let params = QueryParameters::parse("").unwrap();
        assert!(params.require("SERVICE").is_err());
    }

    #[test]
    fn get_or_default() {
        let params = QueryParameters::parse("A=1").unwrap();
        assert_eq!(params.get_or("A", "default"), "1");
        assert_eq!(params.get_or("B", "default"), "default");
    }

    #[test]
    fn service_helper() {
        let params = QueryParameters::parse("SERVICE=WFS&REQUEST=GetCapabilities").unwrap();
        assert_eq!(params.service(), Some("WFS"));
        assert_eq!(params.request(), Some("GetCapabilities"));
    }

    #[test]
    fn layers_helper() {
        let params = QueryParameters::parse("LAYERS=layer1,layer2,layer3").unwrap();
        assert_eq!(params.layers(), vec!["layer1", "layer2", "layer3"]);
    }

    #[test]
    fn layers_empty() {
        let params = QueryParameters::parse("").unwrap();
        assert!(params.layers().is_empty());
    }

    #[test]
    fn crs_helper() {
        let params = QueryParameters::parse("CRS=EPSG:4326").unwrap();
        assert_eq!(params.crs(), Some("EPSG:4326"));
    }

    #[test]
    fn crs_fallback_to_srs() {
        let params = QueryParameters::parse("SRS=EPSG:3857").unwrap();
        assert_eq!(params.crs(), Some("EPSG:3857"));
    }

    #[test]
    fn map_helper() {
        let params = QueryParameters::parse("MAP=/home/user/project.qgs").unwrap();
        assert_eq!(params.map(), Some("/home/user/project.qgs"));
    }

    #[test]
    fn version_helper() {
        let params = QueryParameters::parse("VERSION=1.3.0").unwrap();
        assert_eq!(params.version(), Some("1.3.0"));
    }

    #[test]
    fn params_accessor() {
        let params = QueryParameters::parse("A=1&B=2").unwrap();
        let raw = params.params();
        assert_eq!(raw.get("A").unwrap(), "1");
        assert_eq!(raw.get("B").unwrap(), "2");
    }

    #[test]
    fn skip_empty_pairs() {
        let params = QueryParameters::parse("A=1&&B=2&").unwrap();
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn skip_empty_keys() {
        let params = QueryParameters::parse("=value&A=1").unwrap();
        assert_eq!(params.len(), 1);
    }
}
