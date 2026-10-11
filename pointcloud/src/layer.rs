//! Point cloud layer metadata.
//!
//! Matches QGIS `QgsPointCloudLayer` which describes a point cloud
//! data source with its attributes, extent, and point count.

use crate::attribute::standard_las_attributes;
use crate::{PointCloudAttribute, PointCloudAttributeType};

/// Point cloud layer metadata.
///
/// Matches QGIS `QgsPointCloudLayer`. Describes the attributes,
/// extent, and point count of a point cloud data source.
#[derive(Debug, Clone, PartialEq)]
pub struct PointCloudLayer {
    /// Layer name.
    pub name: String,
    /// The CRS as a WKT string (simplified — full WKT2 parsing in E2).
    pub crs_wkt: Option<String>,
    /// Attributes of the point cloud.
    pub attributes: Vec<PointCloudAttribute>,
    /// Total number of points.
    pub point_count: usize,
    /// Bounding box: (min_x, min_y, min_z, max_x, max_y, max_z).
    pub extent: Option<(f64, f64, f64, f64, f64, f64)>,
    /// Scale factors for X, Y, Z coordinates.
    pub scale: (f64, f64, f64),
    /// Offset values for X, Y, Z coordinates.
    pub offset: (f64, f64, f64),
    /// Total size of all attributes in bytes per point.
    pub point_record_length: usize,
}

impl PointCloudLayer {
    /// Creates a new point cloud layer with standard LAS attributes.
    pub fn new(name: impl Into<String>) -> Self {
        let attributes = standard_las_attributes();
        let point_record_length = attributes.iter().map(|a| a.size()).sum();
        Self {
            name: name.into(),
            crs_wkt: None,
            attributes,
            point_count: 0,
            extent: None,
            scale: (0.01, 0.01, 0.01),
            offset: (0.0, 0.0, 0.0),
            point_record_length,
        }
    }

    /// Creates a new point cloud layer with custom attributes.
    pub fn with_attributes(
        name: impl Into<String>,
        attributes: Vec<PointCloudAttribute>,
    ) -> Self {
        let point_record_length = attributes.iter().map(|a| a.size()).sum();
        Self {
            name: name.into(),
            crs_wkt: None,
            attributes,
            point_count: 0,
            extent: None,
            scale: (0.01, 0.01, 0.01),
            offset: (0.0, 0.0, 0.0),
            point_record_length,
        }
    }

    /// Sets the CRS WKT string.
    pub fn set_crs(&mut self, wkt: impl Into<String>) {
        self.crs_wkt = Some(wkt.into());
    }

    /// Returns the CRS WKT string, if set.
    pub fn crs(&self) -> Option<&str> {
        self.crs_wkt.as_deref()
    }

    /// Returns true if the layer has a CRS set.
    pub fn has_crs(&self) -> bool {
        self.crs_wkt.is_some()
    }

    /// Returns the layer name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the attributes.
    pub fn attributes(&self) -> &[PointCloudAttribute] {
        &self.attributes
    }

    /// Returns the number of attributes.
    pub fn attribute_count(&self) -> usize {
        self.attributes.len()
    }

    /// Returns the total number of points.
    pub fn point_count(&self) -> usize {
        self.point_count
    }

    /// Sets the total number of points.
    pub fn set_point_count(&mut self, count: usize) {
        self.point_count = count;
    }

    /// Returns the extent.
    pub fn extent(&self) -> Option<(f64, f64, f64, f64, f64, f64)> {
        self.extent
    }

    /// Sets the extent.
    pub fn set_extent(&mut self, extent: (f64, f64, f64, f64, f64, f64)) {
        self.extent = Some(extent);
    }

    /// Returns the scale factors.
    pub fn scale(&self) -> (f64, f64, f64) {
        self.scale
    }

    /// Sets the scale factors.
    pub fn set_scale(&mut self, scale: (f64, f64, f64)) {
        self.scale = scale;
    }

    /// Returns the offset values.
    pub fn offset(&self) -> (f64, f64, f64) {
        self.offset
    }

    /// Sets the offset values.
    pub fn set_offset(&mut self, offset: (f64, f64, f64)) {
        self.offset = offset;
    }

    /// Returns the point record length in bytes.
    pub fn point_record_length(&self) -> usize {
        self.point_record_length
    }

    /// Returns true if the layer has no points.
    pub fn is_empty(&self) -> bool {
        self.point_count == 0
    }

    /// Finds an attribute by name.
    pub fn attribute_by_name(&self, name: &str) -> Option<&PointCloudAttribute> {
        self.attributes.iter().find(|a| a.name == name)
    }

    /// Returns true if the layer has an attribute with the given name.
    pub fn has_attribute(&self, name: &str) -> bool {
        self.attributes.iter().any(|a| a.name == name)
    }

    /// Returns the index of the classification attribute, if present.
    pub fn classification_attribute_index(&self) -> Option<usize> {
        self.attributes
            .iter()
            .position(|a| a.name == "Classification")
    }

    /// Returns the index of the intensity attribute, if present.
    pub fn intensity_attribute_index(&self) -> Option<usize> {
        self.attributes.iter().position(|a| a.name == "Intensity")
    }

    /// Returns the total size in bytes for all points.
    pub fn total_size_bytes(&self) -> usize {
        self.point_count * self.point_record_length
    }

    /// Returns the attribute types used by this layer.
    pub fn attribute_types(&self) -> Vec<PointCloudAttributeType> {
        self.attributes
            .iter()
            .map(|a| a.attribute_type)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_cloud_layer_new() {
        let layer = PointCloudLayer::new("test_cloud");
        assert_eq!(layer.name(), "test_cloud");
        assert!(!layer.has_crs());
        assert_eq!(layer.attribute_count(), 14);
        assert_eq!(layer.point_count(), 0);
        assert!(layer.is_empty());
        assert_eq!(layer.extent(), None);
    }

    #[test]
    fn point_cloud_layer_crs() {
        let mut layer = PointCloudLayer::new("test");
        assert!(!layer.has_crs());
        layer.set_crs("EPSG:4326");
        assert!(layer.has_crs());
        assert_eq!(layer.crs(), Some("EPSG:4326"));
    }

    #[test]
    fn point_cloud_layer_point_count() {
        let mut layer = PointCloudLayer::new("test");
        assert_eq!(layer.point_count(), 0);
        layer.set_point_count(10000);
        assert_eq!(layer.point_count(), 10000);
        assert!(!layer.is_empty());
    }

    #[test]
    fn point_cloud_layer_extent() {
        let mut layer = PointCloudLayer::new("test");
        assert_eq!(layer.extent(), None);
        layer.set_extent((0.0, 0.0, 0.0, 100.0, 100.0, 50.0));
        assert_eq!(layer.extent(), Some((0.0, 0.0, 0.0, 100.0, 100.0, 50.0)));
    }

    #[test]
    fn point_cloud_layer_scale_offset() {
        let mut layer = PointCloudLayer::new("test");
        assert_eq!(layer.scale(), (0.01, 0.01, 0.01));
        assert_eq!(layer.offset(), (0.0, 0.0, 0.0));
        layer.set_scale((0.001, 0.001, 0.001));
        layer.set_offset((100.0, 200.0, 300.0));
        assert_eq!(layer.scale(), (0.001, 0.001, 0.001));
        assert_eq!(layer.offset(), (100.0, 200.0, 300.0));
    }

    #[test]
    fn point_cloud_layer_point_record_length() {
        let layer = PointCloudLayer::new("test");
        // Standard LAS: 4+4+4+2+1+1+1+1+1+2+8+2+2+2 = 35
        assert_eq!(layer.point_record_length(), 35);
    }

    #[test]
    fn point_cloud_layer_total_size() {
        let mut layer = PointCloudLayer::new("test");
        layer.set_point_count(1000);
        assert_eq!(layer.total_size_bytes(), 35000);
    }

    #[test]
    fn point_cloud_layer_attribute_by_name() {
        let layer = PointCloudLayer::new("test");
        assert!(layer.has_attribute("X"));
        assert!(layer.has_attribute("Y"));
        assert!(layer.has_attribute("Z"));
        assert!(layer.has_attribute("Intensity"));
        assert!(layer.has_attribute("Classification"));
        assert!(!layer.has_attribute("NonExistent"));

        let x_attr = layer.attribute_by_name("X").unwrap();
        assert_eq!(x_attr.name(), "X");
        assert_eq!(x_attr.attribute_type(), PointCloudAttributeType::Int32);
    }

    #[test]
    fn point_cloud_layer_classification_index() {
        let layer = PointCloudLayer::new("test");
        let idx = layer.classification_attribute_index();
        assert!(idx.is_some());
        assert_eq!(layer.attributes()[idx.unwrap()].name(), "Classification");
    }

    #[test]
    fn point_cloud_layer_intensity_index() {
        let layer = PointCloudLayer::new("test");
        let idx = layer.intensity_attribute_index();
        assert!(idx.is_some());
        assert_eq!(layer.attributes()[idx.unwrap()].name(), "Intensity");
    }

    #[test]
    fn point_cloud_layer_with_custom_attributes() {
        let attrs = vec![
            PointCloudAttribute::new("X", PointCloudAttributeType::Float64),
            PointCloudAttribute::new("Y", PointCloudAttributeType::Float64),
            PointCloudAttribute::new("Z", PointCloudAttributeType::Float64),
        ];
        let layer = PointCloudLayer::with_attributes("custom", attrs);
        assert_eq!(layer.attribute_count(), 3);
        assert_eq!(layer.point_record_length(), 24);
        assert!(layer.has_attribute("X"));
        assert!(!layer.has_attribute("Intensity"));
    }

    #[test]
    fn point_cloud_layer_attribute_types() {
        let layer = PointCloudLayer::new("test");
        let types = layer.attribute_types();
        assert_eq!(types.len(), 14);
        assert_eq!(types[0], PointCloudAttributeType::Int32); // X
        assert_eq!(types[3], PointCloudAttributeType::UInt16); // Intensity
        assert_eq!(types[10], PointCloudAttributeType::Float64); // GpsTime
    }
}
