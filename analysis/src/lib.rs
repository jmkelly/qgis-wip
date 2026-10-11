//! Analysis tools for the QGIS Rust port.
//!
//! Will provide spatial analysis operations (buffer, intersect, union, etc.)
//! using pure-Rust geometry from `qgis-geometry`.

/// Distance between two points in Euclidean space.
pub fn euclidean_distance(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_zero() {
        assert_eq!(euclidean_distance(0.0, 0.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn distance_pythagorean() {
        // 3-4-5 triangle
        assert!((euclidean_distance(0.0, 0.0, 3.0, 4.0) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn distance_symmetric() {
        let d1 = euclidean_distance(1.0, 2.0, 4.0, 6.0);
        let d2 = euclidean_distance(4.0, 6.0, 1.0, 2.0);
        assert!((d1 - d2).abs() < 1e-10);
    }
}
