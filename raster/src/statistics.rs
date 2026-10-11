//! Band statistics for raster data.
//!
//! Matches QGIS `QgsRasterBandStats` which tracks min, max, mean,
//! standard deviation, and histogram information for a raster band.

/// Statistics for a single raster band.
///
/// Matches QGIS `QgsRasterBandStats`.
#[derive(Debug, Clone, PartialEq)]
pub struct BandStatistics {
    /// Minimum value (excluding nodata).
    pub minimum: Option<f64>,
    /// Maximum value (excluding nodata).
    pub maximum: Option<f64>,
    /// Mean value (excluding nodata).
    pub mean: Option<f64>,
    /// Standard deviation (excluding nodata).
    pub std_dev: Option<f64>,
    /// Number of valid (non-nodata) pixels.
    pub valid_count: usize,
    /// Number of nodata pixels.
    pub nodata_count: usize,
    /// Sum of all valid values.
    pub sum: f64,
    /// Sum of squares of all valid values.
    pub sum_squares: f64,
    /// Histogram bins (empty if no histogram was computed).
    pub histogram: Vec<usize>,
    /// Histogram bin lower edges.
    pub histogram_edges: Vec<f64>,
}

impl BandStatistics {
    /// Creates a new, empty statistics object.
    pub fn new() -> Self {
        Self {
            minimum: None,
            maximum: None,
            mean: None,
            std_dev: None,
            valid_count: 0,
            nodata_count: 0,
            sum: 0.0,
            sum_squares: 0.0,
            histogram: Vec::new(),
            histogram_edges: Vec::new(),
        }
    }

    /// Computes statistics from a slice of values.
    ///
    /// NaN values are treated as nodata.
    pub fn from_data(data: &[f64]) -> Self {
        let mut stats = Self::new();
        for &v in data {
            if v.is_nan() {
                stats.nodata_count += 1;
            } else {
                stats.sum += v;
                stats.sum_squares += v * v;
                stats.valid_count += 1;
                stats.minimum = Some(stats.minimum.map_or(v, |m: f64| m.min(v)));
                stats.maximum = Some(stats.maximum.map_or(v, |m: f64| m.max(v)));
            }
        }
        if stats.valid_count > 0 {
            let n = stats.valid_count as f64;
            stats.mean = Some(stats.sum / n);
            let mean = stats.mean.unwrap();
            let variance = stats.sum_squares / n - mean * mean;
            stats.std_dev = Some(variance.max(0.0).sqrt());
        }
        stats
    }

    /// Returns true if the statistics contain no valid data.
    pub fn is_empty(&self) -> bool {
        self.valid_count == 0
    }

    /// Returns the range (max - min), if both are available.
    pub fn range(&self) -> Option<f64> {
        match (self.minimum, self.maximum) {
            (Some(min), Some(max)) => Some(max - min),
            _ => None,
        }
    }

    /// Returns the variance, if available.
    pub fn variance(&self) -> Option<f64> {
        self.std_dev.map(|sd| sd * sd)
    }

    /// Computes a histogram with the given number of bins over [min, max].
    ///
    /// Does nothing if there are no valid values or if `bin_count == 0`.
    pub fn compute_histogram(&mut self, bin_count: usize) {
        if self.valid_count == 0 || bin_count == 0 {
            return;
        }
        let (min, max) = match (self.minimum, self.maximum) {
            (Some(min), Some(max)) => (min, max),
            _ => return,
        };
        self.histogram = vec![0; bin_count];
        self.histogram_edges = (0..=bin_count)
            .map(|i| min + (max - min) * i as f64 / bin_count as f64)
            .collect();
        // Histogram computation requires the raw data; this is a
        // placeholder for the statistics-only API. The caller should
        // use `compute_histogram_from_data` for actual histogram data.
    }

    /// Computes a histogram from raw data with the given number of bins.
    pub fn compute_histogram_from_data(&mut self, data: &[f64], bin_count: usize) {
        if self.valid_count == 0 || bin_count == 0 {
            return;
        }
        let (min, max) = match (self.minimum, self.maximum) {
            (Some(min), Some(max)) => (min, max),
            _ => return,
        };
        self.histogram = vec![0; bin_count];
        self.histogram_edges = (0..=bin_count)
            .map(|i| min + (max - min) * i as f64 / bin_count as f64)
            .collect();
        let range = max - min;
        for &v in data {
            if v.is_nan() {
                continue;
            }
            let bin = if range == 0.0 {
                0
            } else {
                let raw = ((v - min) / range * bin_count as f64).floor() as usize;
                raw.min(bin_count - 1)
            };
            self.histogram[bin] += 1;
        }
    }
}

impl Default for BandStatistics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_statistics_new() {
        let stats = BandStatistics::new();
        assert!(stats.is_empty());
        assert_eq!(stats.valid_count, 0);
        assert_eq!(stats.nodata_count, 0);
        assert_eq!(stats.minimum, None);
        assert_eq!(stats.maximum, None);
        assert_eq!(stats.mean, None);
        assert_eq!(stats.std_dev, None);
    }

    #[test]
    fn band_statistics_from_data() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = BandStatistics::from_data(&data);
        assert_eq!(stats.valid_count, 5);
        assert_eq!(stats.nodata_count, 0);
        assert_eq!(stats.minimum, Some(1.0));
        assert_eq!(stats.maximum, Some(5.0));
        assert_eq!(stats.mean, Some(3.0));
        assert!(stats.std_dev.unwrap() > 1.41);
        assert!(stats.std_dev.unwrap() < 1.42);
    }

    #[test]
    fn band_statistics_with_nodata() {
        let data = vec![1.0, f64::NAN, 3.0, f64::NAN, 5.0];
        let stats = BandStatistics::from_data(&data);
        assert_eq!(stats.valid_count, 3);
        assert_eq!(stats.nodata_count, 2);
        assert_eq!(stats.minimum, Some(1.0));
        assert_eq!(stats.maximum, Some(5.0));
        assert_eq!(stats.mean, Some(3.0));
    }

    #[test]
    fn band_statistics_all_nodata() {
        let data = vec![f64::NAN, f64::NAN];
        let stats = BandStatistics::from_data(&data);
        assert!(stats.is_empty());
        assert_eq!(stats.valid_count, 0);
        assert_eq!(stats.nodata_count, 2);
        assert_eq!(stats.mean, None);
    }

    #[test]
    fn band_statistics_range() {
        let data = vec![1.0, 5.0, 3.0];
        let stats = BandStatistics::from_data(&data);
        assert_eq!(stats.range(), Some(4.0));
    }

    #[test]
    fn band_statistics_variance() {
        let data = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let stats = BandStatistics::from_data(&data);
        let mean = stats.mean.unwrap();
        assert!((mean - 5.0).abs() < 1e-10);
        let variance = stats.variance().unwrap();
        assert!((variance - 4.0).abs() < 1e-10);
    }

    #[test]
    fn band_statistics_single_value() {
        let data = vec![42.0];
        let stats = BandStatistics::from_data(&data);
        assert_eq!(stats.minimum, Some(42.0));
        assert_eq!(stats.maximum, Some(42.0));
        assert_eq!(stats.mean, Some(42.0));
        assert_eq!(stats.std_dev, Some(0.0));
    }

    #[test]
    fn band_statistics_histogram() {
        let data = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let mut stats = BandStatistics::from_data(&data);
        stats.compute_histogram_from_data(&data, 5);
        assert_eq!(stats.histogram.len(), 5);
        assert_eq!(stats.histogram_edges.len(), 6);
        assert_eq!(stats.histogram.iter().sum::<usize>(), 10);
        // Each bin should have 2 values
        for &count in &stats.histogram {
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn band_statistics_histogram_single_value() {
        let data = vec![5.0, 5.0, 5.0];
        let mut stats = BandStatistics::from_data(&data);
        stats.compute_histogram_from_data(&data, 3);
        assert_eq!(stats.histogram.len(), 3);
        // All values in the first bin (range is 0)
        assert_eq!(stats.histogram[0], 3);
        assert_eq!(stats.histogram[1], 0);
        assert_eq!(stats.histogram[2], 0);
    }

    #[test]
    fn band_statistics_default() {
        let stats = BandStatistics::default();
        assert!(stats.is_empty());
    }
}
