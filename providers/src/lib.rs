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
