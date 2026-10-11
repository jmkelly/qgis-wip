//! Criterion benchmark harness for QGIS Rust port hot paths.
//!
//! Ports the hot-path structure of `tests/bench/qgsbench.cpp` (the `QgsBench`
//! render loop and the render / index / expression / raster paths it drives)
//! to criterion. The real Rust implementations of these algorithms do not
//! exist yet (E1+ in the epic), so each benchmark calls a placeholder stub
//! whose signature matches the qgsbench hot path it will eventually exercise.
//!
//! The goal is the criterion infrastructure — groups, naming, harness wiring —
//! not real algorithm performance. See the crate README for the group layout.
//!
//! Pure Rust, zero `unsafe`, per ADR-0001.

#![forbid(unsafe_code)]

pub mod expression;
pub mod index;
pub mod raster;
pub mod render;

// ---------------------------------------------------------------------------
// Shared domain stubs
// ---------------------------------------------------------------------------

/// A 2D point in map coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Creates a new point at the given coordinates.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned bounding box / map extent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub xmin: f64,
    pub ymin: f64,
    pub xmax: f64,
    pub ymax: f64,
}

impl Extent {
    /// Creates a new extent from its bounds.
    pub fn new(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> Self {
        Self {
            xmin,
            ymin,
            xmax,
            ymax,
        }
    }
}

impl Default for Extent {
    fn default() -> Self {
        Self::new(0.0, 0.0, 1.0, 1.0)
    }
}

/// A polygon with exterior and interior rings.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    /// The exterior ring (boundary) of the polygon.
    pub exterior: Vec<Point>,
    /// Interior rings (holes) of the polygon.
    pub interiors: Vec<Vec<Point>>,
}

impl Polygon {
    /// Creates a new polygon from an exterior ring.
    pub fn new(exterior: Vec<Point>) -> Self {
        Self {
            exterior,
            interiors: Vec::new(),
        }
    }

    /// Creates a new polygon with exterior and interior rings.
    pub fn with_holes(exterior: Vec<Point>, interiors: Vec<Vec<Point>>) -> Self {
        Self {
            exterior,
            interiors,
        }
    }

    /// Returns the total number of vertices across all rings.
    pub fn vertex_count(&self) -> usize {
        self.exterior.len() + self.interiors.iter().map(Vec::len).sum::<usize>()
    }
}

/// A coordinate reference system stub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crs {
    /// Authority identifier, e.g. `"EPSG:4326"`.
    pub auth_id: String,
}

impl Crs {
    /// Creates a new CRS stub from an authority identifier.
    pub fn new(auth_id: impl Into<String>) -> Self {
        Self {
            auth_id: auth_id.into(),
        }
    }
}

impl Default for Crs {
    fn default() -> Self {
        Self::new("EPSG:4326")
    }
}

/// A feature: an id plus its geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: u64,
    pub geometry: Polygon,
}

impl Feature {
    /// Creates a new feature from an id and geometry.
    pub fn new(id: u64, geometry: Polygon) -> Self {
        Self { id, geometry }
    }
}

/// A named map layer holding features.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    pub features: Vec<Feature>,
}

impl Layer {
    /// Creates a new layer from a name and its features.
    pub fn new(name: impl Into<String>, features: Vec<Feature>) -> Self {
        Self {
            name: name.into(),
            features,
        }
    }
}

/// Map settings mirroring the subset of `QgsMapSettings` used by qgsbench.
#[derive(Debug, Clone, PartialEq)]
pub struct MapSettings {
    pub extent: Extent,
    pub crs: Crs,
    pub width: u32,
    pub height: u32,
}

impl MapSettings {
    /// Creates new map settings.
    pub fn new(extent: Extent, crs: Crs, width: u32, height: u32) -> Self {
        Self {
            extent,
            crs,
            width,
            height,
        }
    }
}

impl Default for MapSettings {
    fn default() -> Self {
        Self::new(Extent::default(), Crs::default(), 800, 600)
    }
}

/// Render statistics returned by render hot paths, mirroring the
/// user/sys/total/wall timing that `QgsBench::render` collects per iteration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderStats {
    /// Simulated wall-clock nanoseconds for the render pass.
    pub wall_ns: u64,
    /// Number of pixels touched.
    pub pixels: u64,
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_vertex_count_sums_rings() {
        let exterior = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ];
        let hole = vec![
            Point::new(0.25, 0.25),
            Point::new(0.75, 0.25),
            Point::new(0.75, 0.75),
        ];
        let poly = Polygon::with_holes(exterior, vec![hole]);
        assert_eq!(poly.vertex_count(), 7);
    }

    #[test]
    fn map_settings_defaults_match_qgsbench_canvas() {
        let settings = MapSettings::default();
        assert_eq!(settings.width, 800);
        assert_eq!(settings.height, 600);
        assert_eq!(settings.crs.auth_id, "EPSG:4326");
    }
}
