//! Raster block: a rectangular block of pixel data.
//!
//! Matches QGIS `QgsRasterBlock` which stores a width × height block
//! of values in a given data type. The block owns its data and provides
//! typed accessors.

use qgis_geometry::Point;

/// The data type of raster pixel values.
///
/// Matches QGIS `QgsRasterBlock::DataType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RasterDataType {
    /// 8-bit unsigned integer (0–255).
    Byte,
    /// 8-bit signed integer (-128–127).
    Int8,
    /// 16-bit unsigned integer.
    UInt16,
    /// 16-bit signed integer.
    Int16,
    /// 32-bit unsigned integer.
    UInt32,
    /// 32-bit signed integer.
    Int32,
    /// 32-bit floating point.
    Float32,
    /// 64-bit floating point.
    Float64,
    /// 64-bit complex floating point.
    CFloat64,
    /// Unknown / unsupported type.
    Unknown,
}

impl RasterDataType {
    /// Returns the size in bytes of a single pixel of this type.
    pub fn size_bytes(self) -> usize {
        match self {
            RasterDataType::Byte | RasterDataType::Int8 => 1,
            RasterDataType::UInt16 | RasterDataType::Int16 => 2,
            RasterDataType::UInt32 | RasterDataType::Int32 | RasterDataType::Float32 => 4,
            RasterDataType::Float64 | RasterDataType::CFloat64 => 8,
            RasterDataType::Unknown => 0,
        }
    }

    /// Returns true if this type is a floating-point type.
    pub fn is_floating(self) -> bool {
        matches!(
            self,
            RasterDataType::Float32 | RasterDataType::Float64 | RasterDataType::CFloat64
        )
    }

    /// Returns true if this type is a signed integer type.
    pub fn is_signed(self) -> bool {
        matches!(self, RasterDataType::Int8 | RasterDataType::Int16 | RasterDataType::Int32)
    }

    /// Returns true if this type is an unsigned integer type.
    pub fn is_unsigned(self) -> bool {
        matches!(self, RasterDataType::Byte | RasterDataType::UInt16 | RasterDataType::UInt32)
    }

    /// Returns true if this type is a complex type.
    pub fn is_complex(self) -> bool {
        matches!(self, RasterDataType::CFloat64)
    }
}

/// A rectangular block of raster pixel data.
///
/// Matches QGIS `QgsRasterBlock`. Stores width × height pixels in a
/// contiguous `Vec<f64>` buffer (one value per pixel, with NaN for
/// nodata). The block owns its data.
///
/// For multi-band rasters, each band has its own `RasterBlock`.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterBlock {
    /// Width of the block in pixels.
    pub width: usize,
    /// Height of the block in pixels.
    pub height: usize,
    /// Data type of the pixels.
    pub data_type: RasterDataType,
    /// Pixel values in row-major order (width × height values).
    /// NaN represents nodata.
    pub data: Vec<f64>,
    /// The nodata value for this block, if any.
    pub nodata_value: Option<f64>,
}

impl RasterBlock {
    /// Creates a new raster block filled with zeros.
    ///
    /// # Panics
    /// Panics if `width * height` overflows `usize`.
    pub fn new(width: usize, height: usize, data_type: RasterDataType) -> Self {
        let len = width * height;
        Self {
            width,
            height,
            data_type,
            data: vec![0.0; len],
            nodata_value: None,
        }
    }

    /// Creates a new raster block filled with a given value.
    pub fn filled(
        width: usize,
        height: usize,
        data_type: RasterDataType,
        value: f64,
    ) -> Self {
        let len = width * height;
        Self {
            width,
            height,
            data_type,
            data: vec![value; len],
            nodata_value: None,
        }
    }

    /// Creates a new raster block from existing data.
    ///
    /// # Panics
    /// Panics if `data.len() != width * height`.
    pub fn from_data(
        width: usize,
        height: usize,
        data_type: RasterDataType,
        data: Vec<f64>,
    ) -> Self {
        assert_eq!(
            data.len(),
            width * height,
            "data length must equal width * height"
        );
        Self {
            width,
            height,
            data_type,
            data,
            nodata_value: None,
        }
    }

    /// Returns the width of the block in pixels.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the height of the block in pixels.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the data type of the block.
    pub fn data_type(&self) -> RasterDataType {
        self.data_type
    }

    /// Returns the total number of pixels in the block.
    pub fn count(&self) -> usize {
        self.width * self.height
    }

    /// Returns true if the block has no pixels.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the value at pixel (x, y).
    ///
    /// # Panics
    /// Panics if `x >= width` or `y >= height`.
    pub fn value(&self, x: usize, y: usize) -> f64 {
        assert!(x < self.width, "x out of bounds");
        assert!(y < self.height, "y out of bounds");
        self.data[y * self.width + x]
    }

    /// Sets the value at pixel (x, y).
    ///
    /// # Panics
    /// Panics if `x >= width` or `y >= height`.
    pub fn set_value(&mut self, x: usize, y: usize, value: f64) {
        assert!(x < self.width, "x out of bounds");
        assert!(y < self.height, "y out of bounds");
        self.data[y * self.width + x] = value;
    }

    /// Returns true if the pixel at (x, y) is nodata.
    pub fn is_nodata(&self, x: usize, y: usize) -> bool {
        let v = self.value(x, y);
        if let Some(nd) = self.nodata_value {
            v == nd
        } else {
            v.is_nan()
        }
    }

    /// Sets the nodata value for this block.
    pub fn set_nodata_value(&mut self, value: Option<f64>) {
        self.nodata_value = value;
    }

    /// Returns the nodata value, if any.
    pub fn nodata_value(&self) -> Option<f64> {
        self.nodata_value
    }

    /// Returns a reference to the underlying data.
    pub fn data(&self) -> &[f64] {
        &self.data
    }

    /// Returns a mutable reference to the underlying data.
    pub fn data_mut(&mut self) -> &mut [f64] {
        &mut self.data
    }

    /// Returns the value at a geographic coordinate using nearest-neighbor
    /// sampling. The coordinate is in the raster's georeferenced space.
    ///
    /// Returns `None` if the coordinate falls outside the raster extent.
    pub fn sample_nearest(
        &self,
        x: f64,
        y: f64,
        geotransform: &[f64; 6],
    ) -> Option<f64> {
        let col = ((x - geotransform[0]) / geotransform[1]).floor() as isize;
        let row = ((y - geotransform[3]) / geotransform[5]).floor() as isize;
        if col < 0 || col >= self.width as isize || row < 0 || row >= self.height as isize {
            return None;
        }
        Some(self.value(col as usize, row as usize))
    }

    /// Returns the value at a geographic coordinate using bilinear
    /// interpolation. Returns `None` if the coordinate falls outside
    /// the raster extent.
    ///
    /// Pixel `i` is a point sample at continuous coordinate `i` (the same
    /// corner convention as [`RasterBlock::sample_nearest`]). Interpolation
    /// happens between the two surrounding point samples; coordinates past
    /// the outermost samples clamp to the edge of the interpolation domain,
    /// so every point inside the raster extent yields a value.
    pub fn sample_bilinear(
        &self,
        x: f64,
        y: f64,
        geotransform: &[f64; 6],
    ) -> Option<f64> {
        let fx = (x - geotransform[0]) / geotransform[1];
        let fy = (y - geotransform[3]) / geotransform[5];

        if fx < 0.0 || fx >= self.width as f64 || fy < 0.0 || fy >= self.height as f64 {
            return None;
        }

        // Clamp into the interpolation domain: the outermost point samples
        // bound it, so coordinates in the outermost pixel's outer half
        // clamp to the domain edge instead of returning None.
        let fx = fx.clamp(0.0, (self.width - 1) as f64);
        let fy = fy.clamp(0.0, (self.height - 1) as f64);

        let x0 = (fx.floor() as usize).min(self.width.saturating_sub(2));
        let y0 = (fy.floor() as usize).min(self.height.saturating_sub(2));
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let v00 = self.value(x0, y0);
        let v10 = self.value(x1, y0);
        let v01 = self.value(x0, y1);
        let v11 = self.value(x1, y1);

        let tx = fx - x0 as f64;
        let ty = fy - y0 as f64;

        let top = v00 * (1.0 - tx) + v10 * tx;
        let bottom = v01 * (1.0 - tx) + v11 * tx;
        Some(top * (1.0 - ty) + bottom * ty)
    }

    /// Returns the minimum non-nodata value in the block.
    pub fn min_value(&self) -> Option<f64> {
        self.data
            .iter()
            .filter(|&&v| !v.is_nan())
            .copied()
            .reduce(f64::min)
    }

    /// Returns the maximum non-nodata value in the block.
    pub fn max_value(&self) -> Option<f64> {
        self.data
            .iter()
            .filter(|&&v| !v.is_nan())
            .copied()
            .reduce(f64::max)
    }

    /// Returns the mean of non-nodata values in the block.
    pub fn mean_value(&self) -> Option<f64> {
        let (sum, count) = self
            .data
            .iter()
            .filter(|&&v| !v.is_nan())
            .fold((0.0, 0usize), |(sum, count), &v| (sum + v, count + 1));
        if count == 0 {
            None
        } else {
            Some(sum / count as f64)
        }
    }

    /// Returns the number of non-nodata pixels.
    pub fn valid_count(&self) -> usize {
        self.data.iter().filter(|&&v| !v.is_nan()).count()
    }

    /// Returns the number of nodata pixels.
    pub fn nodata_count(&self) -> usize {
        self.data.iter().filter(|&&v| v.is_nan()).count()
    }

    /// Returns the pixel coordinates for a given linear index.
    pub fn index_to_coords(&self, index: usize) -> Option<Point> {
        if index >= self.data.len() {
            return None;
        }
        let x = index % self.width;
        let y = index / self.width;
        Some(Point::new(x as f64, y as f64))
    }

    /// Returns the linear index for given pixel coordinates.
    pub fn coords_to_index(&self, x: usize, y: usize) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(y * self.width + x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_data_type_size_bytes() {
        assert_eq!(RasterDataType::Byte.size_bytes(), 1);
        assert_eq!(RasterDataType::Int8.size_bytes(), 1);
        assert_eq!(RasterDataType::UInt16.size_bytes(), 2);
        assert_eq!(RasterDataType::Int16.size_bytes(), 2);
        assert_eq!(RasterDataType::UInt32.size_bytes(), 4);
        assert_eq!(RasterDataType::Int32.size_bytes(), 4);
        assert_eq!(RasterDataType::Float32.size_bytes(), 4);
        assert_eq!(RasterDataType::Float64.size_bytes(), 8);
        assert_eq!(RasterDataType::CFloat64.size_bytes(), 8);
        assert_eq!(RasterDataType::Unknown.size_bytes(), 0);
    }

    #[test]
    fn raster_data_type_flags() {
        assert!(RasterDataType::Float32.is_floating());
        assert!(RasterDataType::Float64.is_floating());
        assert!(RasterDataType::CFloat64.is_floating());
        assert!(!RasterDataType::Byte.is_floating());

        assert!(RasterDataType::Int8.is_signed());
        assert!(RasterDataType::Int16.is_signed());
        assert!(RasterDataType::Int32.is_signed());
        assert!(!RasterDataType::Byte.is_signed());

        assert!(RasterDataType::Byte.is_unsigned());
        assert!(RasterDataType::UInt16.is_unsigned());
        assert!(RasterDataType::UInt32.is_unsigned());
        assert!(!RasterDataType::Int16.is_unsigned());

        assert!(RasterDataType::CFloat64.is_complex());
        assert!(!RasterDataType::Float64.is_complex());
    }

    #[test]
    fn raster_block_new() {
        let block = RasterBlock::new(3, 2, RasterDataType::Float64);
        assert_eq!(block.width(), 3);
        assert_eq!(block.height(), 2);
        assert_eq!(block.count(), 6);
        assert_eq!(block.data_type(), RasterDataType::Float64);
        assert!(!block.is_empty());
        assert_eq!(block.data.len(), 6);
        assert!(block.data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn raster_block_filled() {
        let block = RasterBlock::filled(2, 2, RasterDataType::Byte, 42.0);
        assert_eq!(block.count(), 4);
        assert!(block.data.iter().all(|&v| v == 42.0));
    }

    #[test]
    fn raster_block_from_data() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let block = RasterBlock::from_data(3, 2, RasterDataType::Float64, data);
        assert_eq!(block.width(), 3);
        assert_eq!(block.height(), 2);
        assert_eq!(block.value(0, 0), 1.0);
        assert_eq!(block.value(2, 0), 3.0);
        assert_eq!(block.value(0, 1), 4.0);
        assert_eq!(block.value(2, 1), 6.0);
    }

    #[test]
    fn raster_block_set_value() {
        let mut block = RasterBlock::new(2, 2, RasterDataType::Float64);
        block.set_value(1, 0, 99.0);
        assert_eq!(block.value(1, 0), 99.0);
        assert_eq!(block.value(0, 0), 0.0);
    }

    #[test]
    fn raster_block_is_empty() {
        let block = RasterBlock::new(0, 5, RasterDataType::Float64);
        assert!(block.is_empty());
        let block = RasterBlock::new(5, 0, RasterDataType::Float64);
        assert!(block.is_empty());
    }

    #[test]
    fn raster_block_nodata() {
        let mut block = RasterBlock::new(2, 2, RasterDataType::Float64);
        block.set_nodata_value(Some(-9999.0));
        block.set_value(0, 0, -9999.0);
        block.set_value(1, 0, 42.0);
        assert!(block.is_nodata(0, 0));
        assert!(!block.is_nodata(1, 0));
        assert_eq!(block.nodata_value(), Some(-9999.0));
    }

    #[test]
    fn raster_block_nodata_nan() {
        let mut block = RasterBlock::new(2, 2, RasterDataType::Float64);
        block.set_value(0, 0, f64::NAN);
        assert!(block.is_nodata(0, 0));
        assert!(!block.is_nodata(1, 0));
    }

    #[test]
    fn raster_block_min_max_mean() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let block = RasterBlock::from_data(3, 2, RasterDataType::Float64, data);
        assert_eq!(block.min_value(), Some(1.0));
        assert_eq!(block.max_value(), Some(6.0));
        assert_eq!(block.mean_value(), Some(3.5));
    }

    #[test]
    fn raster_block_min_max_with_nodata() {
        let data = vec![1.0, f64::NAN, 3.0, f64::NAN, 5.0, 6.0];
        let block = RasterBlock::from_data(3, 2, RasterDataType::Float64, data);
        assert_eq!(block.min_value(), Some(1.0));
        assert_eq!(block.max_value(), Some(6.0));
        assert_eq!(block.mean_value(), Some(3.75));
        assert_eq!(block.valid_count(), 4);
        assert_eq!(block.nodata_count(), 2);
    }

    #[test]
    fn raster_block_all_nodata() {
        let data = vec![f64::NAN, f64::NAN];
        let block = RasterBlock::from_data(2, 1, RasterDataType::Float64, data);
        assert_eq!(block.min_value(), None);
        assert_eq!(block.max_value(), None);
        assert_eq!(block.mean_value(), None);
        assert_eq!(block.valid_count(), 0);
    }

    #[test]
    fn raster_block_index_coords() {
        let block = RasterBlock::new(3, 2, RasterDataType::Float64);
        assert_eq!(block.index_to_coords(0), Some(Point::new(0.0, 0.0)));
        assert_eq!(block.index_to_coords(1), Some(Point::new(1.0, 0.0)));
        assert_eq!(block.index_to_coords(3), Some(Point::new(0.0, 1.0)));
        assert_eq!(block.index_to_coords(5), Some(Point::new(2.0, 1.0)));
        assert_eq!(block.index_to_coords(6), None);

        assert_eq!(block.coords_to_index(0, 0), Some(0));
        assert_eq!(block.coords_to_index(2, 1), Some(5));
        assert_eq!(block.coords_to_index(3, 0), None);
        assert_eq!(block.coords_to_index(0, 2), None);
    }

    #[test]
    fn raster_block_sample_nearest() {
        // Geotransform: origin (100, 200), pixel size (10, -10)
        let geotransform = [100.0, 10.0, 0.0, 200.0, 0.0, -10.0];
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let block = RasterBlock::from_data(2, 2, RasterDataType::Float64, data);

        // Pixel (0, 0) center is at (105, 195)
        assert_eq!(block.sample_nearest(105.0, 195.0, &geotransform), Some(1.0));
        // Pixel (1, 0) center is at (115, 195)
        assert_eq!(block.sample_nearest(115.0, 195.0, &geotransform), Some(2.0));
        // Pixel (0, 1) center is at (105, 185)
        assert_eq!(block.sample_nearest(105.0, 185.0, &geotransform), Some(3.0));
        // Outside extent
        assert_eq!(block.sample_nearest(50.0, 195.0, &geotransform), None);
    }

    #[test]
    fn raster_block_sample_bilinear() {
        let geotransform = [0.0, 1.0, 0.0, 0.0, 0.0, -1.0];
        let data = vec![0.0, 10.0, 20.0, 30.0];
        let block = RasterBlock::from_data(2, 2, RasterDataType::Float64, data);

        // Continuous coords: fx = (x - gt0) / gt1, fy = (y - gt3) / gt5; pixel
        // i is a point sample at coordinate i. At (0.5, -0.5) the sample
        // sits at the shared corner of all four pixels, so bilinear weights
        // them equally: (0+10+20+30)/4 = 15.0.
        assert_eq!(block.sample_bilinear(0.5, -0.5, &geotransform), Some(15.0));
        // (1.5, -0.5) is inside the last pixel's cell but past the last
        // sample center (fx = width - 1 = 1.0); the coordinate clamps to
        // the domain edge, giving the last column's vertical midpoint:
        // (10+30)/2 = 20.0. (Previously expected 15.0 — unattainable: see
        // the note below.)
        assert_eq!(block.sample_bilinear(1.5, -0.5, &geotransform), Some(20.0));
        // (1.0, -0.5) lies exactly on sample 1's coordinate (the shared
        // edge of pixels 0 and 1); the window clamps to the last two
        // samples with full weight on column 1: (10+30)/2 = 20.0.
        // (Previously expected 5.0 — unattainable: at fy = 0.5 the first
        // assertion forces ty = 0.5, and then this point's value is
        // 10 + 10*tx >= 10 for every tx. The old 5.0 came from mixing a
        // center-space y convention into an otherwise corner-space test.)
        assert_eq!(block.sample_bilinear(1.0, -0.5, &geotransform), Some(20.0));
        // Left extent edge: fx = 0 clamps to the first sample; the value
        // is the first column's vertical midpoint: (0+20)/2 = 10.0.
        assert_eq!(block.sample_bilinear(0.0, -0.5, &geotransform), Some(10.0));
        // Bottom extent edge: fy = 1.5 clamps to the last sample row;
        // the value is the last row's horizontal midpoint: (20+30)/2 = 25.0.
        assert_eq!(block.sample_bilinear(0.5, -1.5, &geotransform), Some(25.0));
        // Outside the extent on any axis: None.
        assert_eq!(block.sample_bilinear(-0.5, -0.5, &geotransform), None);
        assert_eq!(block.sample_bilinear(0.5, 0.5, &geotransform), None);
        assert_eq!(block.sample_bilinear(2.5, -0.5, &geotransform), None);
    }

    #[test]
    fn raster_block_data_accessors() {
        let mut block = RasterBlock::new(2, 2, RasterDataType::Float64);
        block.data_mut()[0] = 42.0;
        assert_eq!(block.data()[0], 42.0);
    }
}
