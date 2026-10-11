//! LAS point representation.
//!
//! Matches QGIS `QgsLasPoint` / PDAL point record for LAS/LAZ formats.
//! A LAS point has raw integer X, Y, Z coordinates (scaled and offset
//! by the LAS header) plus standard attributes.

/// A single LAS point with standard attributes.
///
/// Matches the LAS point format used by QGIS and PDAL. Coordinates
/// are stored as raw 32-bit integers; the actual position is computed
/// as `x * scale_x + offset_x` (and similarly for Y, Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LasPoint {
    /// Raw X coordinate (integer).
    pub x: i32,
    /// Raw Y coordinate (integer).
    pub y: i32,
    /// Raw Z coordinate (integer).
    pub z: i32,
    /// Laser return intensity (0–65535).
    pub intensity: u16,
    /// Return number (1–15).
    pub return_number: u8,
    /// Number of returns for this pulse (1–15).
    pub number_of_returns: u8,
    /// Scan angle rank (-90 to +90).
    pub scan_angle_rank: i8,
    /// Classification byte.
    pub classification: u8,
    /// User data byte.
    pub user_data: u8,
    /// Point source ID.
    pub point_source_id: u16,
    /// GPS time (seconds of the week).
    pub gps_time: f64,
    /// Red color component (0–65535).
    pub red: u16,
    /// Green color component (0–65535).
    pub green: u16,
    /// Blue color component (0–65535).
    pub blue: u16,
}

impl LasPoint {
    /// Creates a new LAS point with default values.
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self {
            x,
            y,
            z,
            intensity: 0,
            return_number: 1,
            number_of_returns: 1,
            scan_angle_rank: 0,
            classification: 0,
            user_data: 0,
            point_source_id: 0,
            gps_time: 0.0,
            red: 0,
            green: 0,
            blue: 0,
        }
    }

    /// Returns the raw X coordinate.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Returns the raw Y coordinate.
    pub fn y(&self) -> i32 {
        self.y
    }

    /// Returns the raw Z coordinate.
    pub fn z(&self) -> i32 {
        self.z
    }

    /// Returns the scaled X coordinate.
    pub fn scaled_x(&self, scale: f64, offset: f64) -> f64 {
        self.x as f64 * scale + offset
    }

    /// Returns the scaled Y coordinate.
    pub fn scaled_y(&self, scale: f64, offset: f64) -> f64 {
        self.y as f64 * scale + offset
    }

    /// Returns the scaled Z coordinate.
    pub fn scaled_z(&self, scale: f64, offset: f64) -> f64 {
        self.z as f64 * scale + offset
    }

    /// Returns the intensity.
    pub fn intensity(&self) -> u16 {
        self.intensity
    }

    /// Returns the return number.
    pub fn return_number(&self) -> u8 {
        self.return_number
    }

    /// Returns the number of returns.
    pub fn number_of_returns(&self) -> u8 {
        self.number_of_returns
    }

    /// Returns the classification.
    pub fn classification(&self) -> u8 {
        self.classification
    }

    /// Returns the GPS time.
    pub fn gps_time(&self) -> f64 {
        self.gps_time
    }

    /// Returns the RGB color as (r, g, b) in 0–255 range.
    pub fn rgb(&self) -> (u8, u8, u8) {
        let r = ((self.red as u32 * 255 + 32767) / 65535) as u8;
        let g = ((self.green as u32 * 255 + 32767) / 65535) as u8;
        let b = ((self.blue as u32 * 255 + 32767) / 65535) as u8;
        (r, g, b)
    }

    /// Returns true if this is the only return (return_number == number_of_returns).
    pub fn is_single_return(&self) -> bool {
        self.return_number == self.number_of_returns
    }

    /// Returns true if this is the first return.
    pub fn is_first_return(&self) -> bool {
        self.return_number == 1
    }

    /// Returns true if this is a ground classification (class 2).
    pub fn is_ground(&self) -> bool {
        self.classification == 2
    }

    /// Returns true if this is a vegetation classification (class 3, 4, 5).
    pub fn is_vegetation(&self) -> bool {
        matches!(self.classification, 3 | 4 | 5)
    }

    /// Returns true if this is a building classification (class 6).
    pub fn is_building(&self) -> bool {
        self.classification == 6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_point_new() {
        let p = LasPoint::new(100, 200, 300);
        assert_eq!(p.x(), 100);
        assert_eq!(p.y(), 200);
        assert_eq!(p.z(), 300);
        assert_eq!(p.intensity(), 0);
        assert_eq!(p.return_number(), 1);
        assert_eq!(p.number_of_returns(), 1);
        assert_eq!(p.classification(), 0);
    }

    #[test]
    fn las_point_scaled() {
        let p = LasPoint::new(1000, 2000, 3000);
        // scale = 0.01, offset = 0.0
        assert_eq!(p.scaled_x(0.01, 0.0), 10.0);
        assert_eq!(p.scaled_y(0.01, 0.0), 20.0);
        assert_eq!(p.scaled_z(0.01, 0.0), 30.0);
        // scale = 0.01, offset = 100.0
        assert_eq!(p.scaled_x(0.01, 100.0), 110.0);
        assert_eq!(p.scaled_y(0.01, 100.0), 120.0);
        assert_eq!(p.scaled_z(0.01, 100.0), 130.0);
    }

    #[test]
    fn las_point_rgb() {
        let mut p = LasPoint::new(0, 0, 0);
        p.red = 65535;
        p.green = 0;
        p.blue = 0;
        let (r, g, b) = p.rgb();
        assert_eq!(r, 255);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn las_point_rgb_mid() {
        let mut p = LasPoint::new(0, 0, 0);
        p.red = 32768;
        p.green = 32768;
        p.blue = 32768;
        let (r, g, b) = p.rgb();
        assert!(r >= 127 && r <= 128);
        assert!(g >= 127 && g <= 128);
        assert!(b >= 127 && b <= 128);
    }

    #[test]
    fn las_point_single_return() {
        let mut p = LasPoint::new(0, 0, 0);
        p.return_number = 1;
        p.number_of_returns = 1;
        assert!(p.is_single_return());
        p.number_of_returns = 3;
        assert!(!p.is_single_return());
    }

    #[test]
    fn las_point_first_return() {
        let mut p = LasPoint::new(0, 0, 0);
        p.return_number = 1;
        assert!(p.is_first_return());
        p.return_number = 2;
        assert!(!p.is_first_return());
    }

    #[test]
    fn las_point_classification() {
        let mut p = LasPoint::new(0, 0, 0);
        p.classification = 2;
        assert!(p.is_ground());
        assert!(!p.is_vegetation());
        assert!(!p.is_building());
        p.classification = 4;
        assert!(!p.is_ground());
        assert!(p.is_vegetation());
        assert!(!p.is_building());
        p.classification = 6;
        assert!(!p.is_ground());
        assert!(!p.is_vegetation());
        assert!(p.is_building());
    }

    #[test]
    fn las_point_accessors() {
        let p = LasPoint {
            x: 1,
            y: 2,
            z: 3,
            intensity: 100,
            return_number: 1,
            number_of_returns: 2,
            scan_angle_rank: -10,
            classification: 7,
            user_data: 42,
            point_source_id: 12,
            gps_time: 123.456,
            red: 1000,
            green: 2000,
            blue: 3000,
        };
        assert_eq!(p.x(), 1);
        assert_eq!(p.y(), 2);
        assert_eq!(p.z(), 3);
        assert_eq!(p.intensity(), 100);
        assert_eq!(p.return_number(), 1);
        assert_eq!(p.number_of_returns(), 2);
        assert_eq!(p.scan_angle_rank, -10);
        assert_eq!(p.classification(), 7);
        assert_eq!(p.user_data, 42);
        assert_eq!(p.point_source_id, 12);
        assert_eq!(p.gps_time(), 123.456);
        assert_eq!(p.red, 1000);
        assert_eq!(p.green, 2000);
        assert_eq!(p.blue, 3000);
    }
}
