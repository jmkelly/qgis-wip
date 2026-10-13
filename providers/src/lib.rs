//! Data providers for the QGIS Rust port.
//!
//! Replaces GDAL/SpatiaLite/PDAL/MDAL functionality with pure-Rust implementations.
//! See ADR-0001 for the no-FFI policy decision.

/// Common interface for all data providers.
pub trait Provider {
    /// Returns the provider name.
    fn name(&self) -> &str;
    /// Returns true if the provider is valid and ready for use.
    fn is_valid(&self) -> bool;
}

/// A placeholder provider implementation.
#[derive(Debug, Clone)]
pub struct PlaceholderProvider {
    name: String,
}

impl PlaceholderProvider {
    /// Creates a new placeholder provider.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Provider for PlaceholderProvider {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_valid(&self) -> bool {
        true
    }
}

pub mod dbase;
pub mod vsi;
pub mod vrt;
pub mod shapefile;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_name() {
        let p = PlaceholderProvider::new("test");
        assert_eq!(p.name(), "test");
    }

    #[test]
    fn placeholder_valid() {
        let p = PlaceholderProvider::new("test");
        assert!(p.is_valid());
    }
}

pub mod geojson;
pub mod gpkg;

pub mod driver_manager {
    use std::collections::BTreeMap;
    #[derive(Debug,Clone)]
    pub struct DriverManager { providers: BTreeMap<String,String> }
    impl DriverManager {
        pub fn new() -> Self { Self { providers: BTreeMap::new() } }
        pub fn register(&mut self, pattern: &str, name: &str) { self.providers.insert(pattern.to_string(), name.to_string()); }
        pub fn lookup(&self, uri: &str) -> Option<&str> { self.providers.get(uri).map(|s| s.as_str()) }
        pub fn list(&self) -> Vec<&str> { self.providers.values().map(|s| s.as_str()).collect() }
    }
    impl Default for DriverManager { fn default() -> Self { Self::new() } }
}
