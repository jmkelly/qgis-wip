//! Raster layer metadata.
//!
//! Matches QGIS `QgsRasterLayer` which describes a raster data source
//! with its dimensions, CRS, extent, and band information.

use qgis_geometry::Point;

/// Raster layer metadata.
///
/// Matches QGIS `QgsRasterLayer` / `QgsRasterInterface`. Describes the
/// dimensions, CRS, extent, and band structure of a raster data source.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterLayer {
    /// Layer name.
    pub name: String,
    /// Number of bands.
    pub band_count: usize,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Data type shared by all bands.
    pub data_type: crate::RasterDataType,
    /// The CRS as a WKT string (simplified — full WKT2 parsing in E2).
    pub crs_wkt: Option<String>,
    /// Geotransform coefficients: [origin_x, pixel_width, rotation_x, origin_y, rotation_y, pixel_height].
    pub geotransform: [f64; 6],
    /// No-data value for the layer, if any.
    pub nodata_value: Option<f64>,
    /// Band names.
    pub band_names: Vec<String>,
}

impl RasterLayer {
    /// Creates a new raster layer with default geotransform.
    ///
    /// Default geotransform places the origin at (0, 0) with unit pixel size
    /// and no rotation: `[0, 1, 0, 0, 0, -1]` (y-axis flipped for
    /// top-to-bottom raster convention).
    pub fn new(
        name: impl Into<String>,
        width: usize,
        height: usize,
        band_count: usize,
        data_type: crate::RasterDataType,
    ) -> Self {
        Self {
            name: name.into(),
            band_count,
            width,
            height,
            data_type,
            crs_wkt: None,
            geotransform: [0.0, 1.0, 0.0, 0.0, 0.0, -1.0],
            nodata_value: None,
            band_names: Vec::new(),
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

    /// Sets the geotransform.
    pub fn set_geotransform(&mut self, gt: [f64; 6]) {
        self.geotransform = gt;
    }

    /// Returns the geotransform.
    pub fn geotransform(&self) -> &[f64; 6] {
        &self.geotransform
    }

    /// Returns the extent of the layer as (min_x, min_y, max_x, max_y).
    pub fn extent(&self) -> (f64, f64, f64, f64) {
        let min_x = self.geotransform[0];
        let max_y = self.geotransform[3];
        let max_x = min_x + self.width as f64 * self.geotransform[1];
        let min_y = max_y + self.height as f64 * self.geotransform[5];
        (min_x, min_y, max_x, max_y)
    }

    /// Returns the width of the layer in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the height of the layer in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the number of bands.
    pub fn band_count(&self) -> usize {
        self.band_count
    }

    /// Returns the data type.
    pub fn data_type(&self) -> crate::RasterDataType {
        self.data_type
    }

    /// Returns the pixel width (from the geotransform).
    pub fn pixel_width(&self) -> f64 {
        self.geotransform[1]
    }

    /// Returns the pixel height (from the geotransform, typically negative).
    pub fn pixel_height(&self) -> f64 {
        self.geotransform[5]
    }

    /// Returns the origin (top-left corner) as a point.
    pub fn origin(&self) -> Point {
        Point::new(self.geotransform[0], self.geotransform[3])
    }

    /// Returns the layer name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sets the band names.
    pub fn set_band_names(&mut self, names: Vec<String>) {
        self.band_names = names;
    }

    /// Returns the band names.
    pub fn band_names(&self) -> &[String] {
        &self.band_names
    }

    /// Returns the total number of pixels across all bands.
    pub fn total_pixels(&self) -> usize {
        self.width * self.height * self.band_count
    }

    /// Returns true if the layer has no data (zero width or height).
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Converts a geographic coordinate to pixel coordinates.
    /// Returns None if the geotransform is not invertible.
    pub fn geo_to_pixel(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let gt = &self.geotransform;
        let det = gt[1] * gt[5] - gt[2] * gt[4];
        if det.abs() < 1e-15 {
            return None;
        }
        let col = (gt[5] * (x - gt[0]) - gt[2] * (y - gt[3])) / det;
        let row = (-gt[4] * (x - gt[0]) + gt[1] * (y - gt[3])) / det;
        Some((col, row))
    }

    /// Converts pixel coordinates to a geographic coordinate.
    pub fn pixel_to_geo(&self, col: f64, row: f64) -> Point {
        let gt = &self.geotransform;
        Point::new(
            gt[0] + col * gt[1] + row * gt[2],
            gt[3] + col * gt[4] + row * gt[5],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_layer_new() {
        let layer = RasterLayer::new("test", 100, 200, 3, crate::RasterDataType::Float64);
        assert_eq!(layer.name(), "test");
        assert_eq!(layer.width(), 100);
        assert_eq!(layer.height(), 200);
        assert_eq!(layer.band_count(), 3);
        assert_eq!(layer.data_type(), crate::RasterDataType::Float64);
        assert!(!layer.has_crs());
        assert!(!layer.is_empty());
    }

    #[test]
    fn raster_layer_crs() {
        let mut layer = RasterLayer::new("test", 10, 10, 1, crate::RasterDataType::Byte);
        assert!(!layer.has_crs());
        layer.set_crs("EPSG:4326");
        assert!(layer.has_crs());
        assert_eq!(layer.crs(), Some("EPSG:4326"));
    }

    #[test]
    fn raster_layer_extent_default_gt() {
        let layer = RasterLayer::new("test", 100, 200, 1, crate::RasterDataType::Byte);
        // Default GT: [0, 1, 0, 0, 0, -1]
        // extent = (0, -200, 100, 0)
        let (min_x, min_y, max_x, max_y) = layer.extent();
        assert_eq!(min_x, 0.0);
        assert_eq!(min_y, -200.0);
        assert_eq!(max_x, 100.0);
        assert_eq!(max_y, 0.0);
    }

    #[test]
    fn raster_layer_extent_custom_gt() {
        let mut layer = RasterLayer::new("test", 100, 200, 1, crate::RasterDataType::Byte);
        layer.set_geotransform([500.0, 10.0, 0.0, 1000.0, 0.0, -10.0]);
        let (min_x, min_y, max_x, max_y) = layer.extent();
        assert_eq!(min_x, 500.0);
        assert_eq!(min_y, -1000.0);
        assert_eq!(max_x, 1500.0);
        assert_eq!(max_y, 1000.0);
    }

    #[test]
    fn raster_layer_pixel_size() {
        let mut layer = RasterLayer::new("test", 10, 10, 1, crate::RasterDataType::Byte);
        assert_eq!(layer.pixel_width(), 1.0);
        assert_eq!(layer.pixel_height(), -1.0);
        layer.set_geotransform([0.0, 0.5, 0.0, 0.0, 0.0, -0.5]);
        assert_eq!(layer.pixel_width(), 0.5);
        assert_eq!(layer.pixel_height(), -0.5);
    }

    #[test]
    fn raster_layer_origin() {
        let mut layer = RasterLayer::new("test", 10, 10, 1, crate::RasterDataType::Byte);
        layer.set_geotransform([100.0, 1.0, 0.0, 200.0, 0.0, -1.0]);
        let origin = layer.origin();
        assert_eq!(origin.x(), 100.0);
        assert_eq!(origin.y(), 200.0);
    }

    #[test]
    fn raster_layer_band_names() {
        let mut layer = RasterLayer::new("test", 10, 10, 3, crate::RasterDataType::Byte);
        assert!(layer.band_names().is_empty());
        layer.set_band_names(vec![
            "Red".to_string(),
            "Green".to_string(),
            "Blue".to_string(),
        ]);
        assert_eq!(layer.band_names().len(), 3);
        assert_eq!(layer.band_names()[0], "Red");
    }

    #[test]
    fn raster_layer_total_pixels() {
        let layer = RasterLayer::new("test", 100, 200, 3, crate::RasterDataType::Float64);
        assert_eq!(layer.total_pixels(), 60000);
    }

    #[test]
    fn raster_layer_is_empty() {
        let layer = RasterLayer::new("test", 0, 100, 1, crate::RasterDataType::Byte);
        assert!(layer.is_empty());
        let layer = RasterLayer::new("test", 100, 0, 1, crate::RasterDataType::Byte);
        assert!(layer.is_empty());
    }

    #[test]
    fn raster_layer_geo_to_pixel() {
        let mut layer = RasterLayer::new("test", 100, 100, 1, crate::RasterDataType::Byte);
        layer.set_geotransform([500.0, 10.0, 0.0, 1000.0, 0.0, -10.0]);
        // Origin maps to pixel (0, 0)
        let (col, row) = layer.geo_to_pixel(500.0, 1000.0).unwrap();
        assert_eq!(col, 0.0);
        assert_eq!(row, 0.0);
        // One pixel right and down
        let (col, row) = layer.geo_to_pixel(510.0, 990.0).unwrap();
        assert_eq!(col, 1.0);
        assert_eq!(row, 1.0);
    }

    #[test]
    fn raster_layer_pixel_to_geo() {
        let mut layer = RasterLayer::new("test", 100, 100, 1, crate::RasterDataType::Byte);
        layer.set_geotransform([500.0, 10.0, 0.0, 1000.0, 0.0, -10.0]);
        let p = layer.pixel_to_geo(0.0, 0.0);
        assert_eq!(p.x(), 500.0);
        assert_eq!(p.y(), 1000.0);
        let p = layer.pixel_to_geo(1.0, 1.0);
        assert_eq!(p.x(), 510.0);
        assert_eq!(p.y(), 990.0);
    }

    #[test]
    fn raster_layer_geo_pixel_roundtrip() {
        let mut layer = RasterLayer::new("test", 100, 100, 1, crate::RasterDataType::Byte);
        layer.set_geotransform([500.0, 10.0, 0.0, 1000.0, 0.0, -10.0]);
        for (col, row) in [(0.0, 0.0), (50.0, 50.0), (99.0, 99.0)] {
            let p = layer.pixel_to_geo(col, row);
            let (c, r) = layer.geo_to_pixel(p.x(), p.y()).unwrap();
            assert!((c - col).abs() < 1e-10, "col {col} -> {c}");
            assert!((r - row).abs() < 1e-10, "row {row} -> {r}");
        }
    }
}
