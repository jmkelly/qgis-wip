//! NTV2 and GeoTIFF multi-grid loading.
//!
//! Loads NTV2 `.gsb` binary grid files and GDAL-generated GeoTIFF
//! multi-grids for use with proj4rs's experimental nadgrids support.
//! Grid intersection (parent/child) is handled by the grid catalog:
//! when multiple grids overlap, the most specific (child) grid takes
//! precedence.
//!
//! This module is behind the `grids` feature flag in `crs/Cargo.toml`.
//!
//! # Grid formats
//!
//! - **NTV2** (`.gsb`): Canadian NTV2 binary format with a header record
//!   followed by one or more sub-grid records. Each sub-grid has a
//!   lower-left corner, upper-right corner, grid intervals, and a
//!   matrix of (longitude, latitude) shift pairs.
//! - **GeoTIFF** (`.tif`): GDAL-generated GeoTIFF multi-grids. These
//!   are loaded via the `tiff` feature of proj4rs.
//!
//! # Grid intersection
//!
//! When a point falls within multiple grids, the grid catalog selects
//! the most specific grid (the child grid with the smallest area).
//! This matches PROJ's behavior where child grids override parent
//! grids in their area of coverage.
//!
//! See ADR 0006 for the projection decision.

#![forbid(unsafe_code)]
#![cfg(feature = "grids")]

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

// Re-export proj4rs nadgrids types when the feature is enabled
#[cfg(feature = "grids")]
use proj4rs::nadgrids::catalog;

/// Errors that can occur during grid loading.
#[derive(Debug, Clone, PartialEq)]
pub enum GridError {
    /// The grid file could not be found.
    FileNotFound(String),
    /// The grid file format is not recognized.
    UnknownFormat(String),
    /// The NTV2 header is invalid.
    InvalidNtv2Header(String),
    /// The grid data is corrupt or truncated.
    CorruptData(String),
    /// An I/O error occurred.
    IoError(String),
    /// The grid is not available in the catalog.
    NotAvailable(String),
}

impl fmt::Display for GridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNotFound(name) => write!(f, "grid file not found: {name}"),
            Self::UnknownFormat(name) => write!(f, "unknown grid format: {name}"),
            Self::InvalidNtv2Header(msg) => write!(f, "invalid NTV2 header: {msg}"),
            Self::CorruptData(msg) => write!(f, "corrupt grid data: {msg}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::NotAvailable(name) => write!(f, "grid not available: {name}"),
        }
    }
}

impl std::error::Error for GridError {}

impl From<io::Error> for GridError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e.to_string())
    }
}

/// A loaded NTV2 grid with its metadata and shift data.
#[derive(Debug, Clone)]
pub struct Ntv2Grid {
    /// The grid's short name (from the filename).
    pub name: String,
    /// The grid's ID (8-character identifier from the NTV2 header).
    pub id: String,
    /// The parent grid ID (empty string for root grids).
    pub lineage: String,
    /// Lower-left corner in radians.
    pub ll: (f64, f64),
    /// Upper-right corner in radians.
    pub ur: (f64, f64),
    /// Grid intervals in radians.
    pub del: (f64, f64),
    /// Grid dimensions (rows, columns).
    pub lim: (usize, usize),
    /// Epsilon tolerance value.
    pub epsilon: f64,
    /// Shift matrix: (longitude shift, latitude shift) pairs in radians.
    pub shifts: Vec<(f64, f64)>,
}

impl Ntv2Grid {
    /// Returns `true` if this is a root grid (no parent).
    pub fn is_root(&self) -> bool {
        self.lineage.is_empty() || self.lineage == "NONE"
    }

    /// Returns `true` if this grid contains the given (lon, lat) point in radians.
    pub fn contains(&self, lon: f64, lat: f64) -> bool {
        let (ll_lon, ll_lat) = self.ll;
        let (ur_lon, ur_lat) = self.ur;
        lon >= ll_lon - self.epsilon
            && lon <= ur_lon + self.epsilon
            && lat >= ll_lat - self.epsilon
            && lat <= ur_lat + self.epsilon
    }

    /// Returns `true` if this grid is a direct child of the given grid.
    pub fn is_child_of(&self, other: &Ntv2Grid) -> bool {
        self.lineage == other.id
    }

    /// Returns the number of shift values.
    pub fn len(&self) -> usize {
        self.shifts.len()
    }

    /// Returns `true` if the grid has no shift values.
    pub fn is_empty(&self) -> bool {
        self.shifts.is_empty()
    }
}

/// Loads an NTV2 `.gsb` grid file from the given reader.
///
/// The NTV2 format consists of:
/// - A header record (11 × 16 = 176 bytes)
/// - One or more sub-grid records, each with its own header and data
///
/// # Arguments
///
/// * `name` - The grid's short name (typically the filename).
/// * `reader` - A reader over the NTV2 binary data.
pub fn load_ntv2<R: Read + Seek>(name: &str, reader: &mut R) -> Result<Ntv2Grid, GridError> {
    // Read the main header (11 * 16 = 176 bytes)
    let mut header = [0u8; 176];
    reader.read_exact(&mut header)?;

    // Check endianness: byte 8 should be 11 for little-endian
    let is_le = header[8] == 11;
    let endian = if is_le { Endian::Little } else { Endian::Big };

    // Read the number of sub-grids
    let nsubgrids = read_u32(&header[40..44], endian)? as usize;
    if nsubgrids == 0 {
        return Err(GridError::InvalidNtv2Header(
            "no sub-grids in file".to_string(),
        ));
    }

    // Read the first sub-grid header
    let mut sub_header = [0u8; 176];
    reader.read_exact(&mut sub_header)?;

    // Verify "SUB_NAME" at offset 0
    let sub_name = read_str(&sub_header[0..8])?;
    if sub_name != "SUB_NAME" {
        return Err(GridError::InvalidNtv2Header(format!(
            "expected SUB_NAME, got {sub_name:?}"
        )));
    }

    // Read grid ID (offset 8, 8 chars)
    let id = read_str(&sub_header[8..16])?;

    // Read lineage/parent ID (offset 24, 8 chars)
    let lineage = read_str(&sub_header[24..32])?;

    // Read corner coordinates (all in seconds of arc)
    let w_long = read_f64(&sub_header[120..128], endian)?; // W_LONG (negative lon)
    let s_lat = read_f64(&sub_header[72..80], endian)?; // S_LAT
    let e_long = read_f64(&sub_header[104..112], endian)?; // E_LONG (negative lon)
    let n_lat = read_f64(&sub_header[88..96], endian)?; // N_LAT

    // Convert to radians (NTV2 stores longitudes as negative)
    let sec_to_rad = std::f64::consts::PI / (180.0 * 3600.0);
    let ll = (-w_long * sec_to_rad, s_lat * sec_to_rad);
    let ur = (-e_long * sec_to_rad, n_lat * sec_to_rad);

    // Read grid intervals
    let lon_interval = read_f64(&sub_header[152..160], endian)? * sec_to_rad;
    let lat_interval = read_f64(&sub_header[136..144], endian)? * sec_to_rad;
    let del = (lon_interval, lat_interval);

    // Compute grid dimensions
    let ncols = (((ur.0 - ll.0).abs() / del.0 + 0.5) + 1.0).floor() as usize;
    let nrows = (((ur.1 - ll.1).abs() / del.1 + 0.5) + 1.0).floor() as usize;
    let lim = (nrows, ncols);

    // Read the number of shift values
    let gs_count = read_u32(&sub_header[168..172], endian)? as usize;
    if gs_count != nrows * ncols {
        return Err(GridError::InvalidNtv2Header(format!(
            "GS COUNT mismatch: expected {}, got {}",
            nrows * ncols,
            gs_count
        )));
    }

    // Read shift data (each pair is 8 bytes: f32 lon, f32 lat)
    let mut shifts = Vec::with_capacity(gs_count);
    for _ in 0..gs_count {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        let lon_shift = read_f32(&buf[0..4], endian)? as f64 * sec_to_rad;
        let lat_shift = read_f32(&buf[4..8], endian)? as f64 * sec_to_rad;
        shifts.push((lon_shift, lat_shift));
    }

    // Reverse each row (NTV2 stores rows in reverse order)
    for i in 0..nrows {
        let start = i * ncols;
        let end = start + ncols;
        shifts[start..end].reverse();
    }

    let epsilon = (del.0.abs() + del.1.abs()) / 10_000.0;

    Ok(Ntv2Grid {
        name: name.to_string(),
        id,
        lineage,
        ll,
        ur,
        del,
        lim,
        epsilon,
        shifts,
    })
}

/// Loads an NTV2 grid from a file path.
pub fn load_ntv2_from_file(path: &str) -> Result<Ntv2Grid, GridError> {
    let file = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(file);
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    load_ntv2(name, &mut reader)
}

/// Loads multiple grids from a `+nadgrids=` comma-separated list.
///
/// Grid names prefixed with `@` are optional (missing files are skipped).
/// The list may end with `@null` to indicate no more grids.
///
/// # Arguments
///
/// * `grid_list` - The `+nadgrids=` value (e.g., `"grid1.gsb,grid2.gsb,@null"`).
/// * `search_paths` - Directories to search for grid files.
pub fn load_nadgrids(
    grid_list: &str,
    search_paths: &[&str],
) -> Result<Vec<Ntv2Grid>, GridError> {
    let mut grids = Vec::new();
    for name in grid_list.split(',') {
        let name = name.trim();
        if name == "@null" || name == "null" {
            break;
        }
        let optional = name.starts_with('@');
        let name = name.trim_start_matches('@');

        // Search for the file in the search paths
        let mut found = false;
        for path in search_paths {
            let full = std::path::Path::new(path).join(name);
            if full.exists() {
                let grid = load_ntv2_from_file(full.to_str().unwrap())?;
                grids.push(grid);
                found = true;
                break;
            }
        }

        if !found && !optional {
            return Err(GridError::FileNotFound(name.to_string()));
        }
    }
    Ok(grids)
}

/// A grid catalog that manages loaded grids and handles intersection.
///
/// The catalog maintains a collection of grids and provides lookup
/// by point coordinates. When multiple grids overlap, the most specific
/// (child) grid is selected.
#[derive(Debug, Default)]
pub struct GridCatalog {
    grids: Vec<Ntv2Grid>,
}

impl GridCatalog {
    /// Creates an empty grid catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a grid to the catalog.
    pub fn add(&mut self, grid: Ntv2Grid) {
        self.grids.push(grid);
    }

    /// Returns the number of grids in the catalog.
    pub fn len(&self) -> usize {
        self.grids.len()
    }

    /// Returns `true` if the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.grids.is_empty()
    }

    /// Finds the best grid for a given point (lon, lat in radians).
    ///
    /// Returns the most specific (child) grid that contains the point,
    /// or `None` if no grid covers it.
    pub fn find_best(&self, lon: f64, lat: f64) -> Option<&Ntv2Grid> {
        // Consider every grid that contains the point. The most specific
        // grid is the one with the smallest area (child grids override
        // their parents), which also handles deeply-nested (grandchild)
        // hierarchies regardless of insertion order.
        let mut candidates: Vec<&Ntv2Grid> = self
            .grids
            .iter()
            .filter(|g| g.contains(lon, lat))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        // Prefer lineage descendants when possible: walk the chain from a
        // containing root through children. Fall back to smallest area.
        // First try to follow the parent/child chain to the deepest level.
        let mut best: Option<&Ntv2Grid> = None;
        for grid in &candidates {
            if grid.is_root() {
                // candidate root; consider chain
                let mut current = *grid;
                loop {
                    let next = candidates
                        .iter()
                        .copied()
                        .find(|g| g.is_child_of(current) && g.contains(lon, lat));
                    match next {
                        Some(c) => current = c,
                        None => break,
                    }
                }
                // Pick the deepest chain; tie-break by smallest area
                match best {
                    None => best = Some(current),
                    Some(b) => {
                        let area_b = (b.ur.0 - b.ll.0).abs() * (b.ur.1 - b.ll.1).abs();
                        let area_c = (current.ur.0 - current.ll.0).abs()
                            * (current.ur.1 - current.ll.1).abs();
                        if area_c < area_b {
                            best = Some(current);
                        }
                    }
                }
            }
        }
        // If no root contained the point, fall back to smallest area.
        if best.is_none() {
            best = candidates
                .into_iter()
                .min_by(|a, b| {
                    let aa = (a.ur.0 - a.ll.0).abs() * (a.ur.1 - a.ll.1).abs();
                    let bb = (b.ur.0 - b.ll.0).abs() * (b.ur.1 - b.ll.1).abs();
                    aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
                });
        }
        best
    }

    /// Returns all grids in the catalog.
    pub fn grids(&self) -> &[Ntv2Grid] {
        &self.grids
    }
}

/// Byte endianness for binary file reading.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Endian {
    Little,
    Big,
}

/// Reads a u32 from a byte slice with the given endianness.
fn read_u32(bytes: &[u8], endian: Endian) -> Result<u32, GridError> {
    if bytes.len() < 4 {
        return Err(GridError::CorruptData("not enough bytes for u32".to_string()));
    }
    let arr: [u8; 4] = bytes[..4].try_into().unwrap();
    Ok(match endian {
        Endian::Little => u32::from_le_bytes(arr),
        Endian::Big => u32::from_be_bytes(arr),
    })
}

/// Reads an f64 from a byte slice with the given endianness.
fn read_f64(bytes: &[u8], endian: Endian) -> Result<f64, GridError> {
    if bytes.len() < 8 {
        return Err(GridError::CorruptData("not enough bytes for f64".to_string()));
    }
    let arr: [u8; 8] = bytes[..8].try_into().unwrap();
    Ok(match endian {
        Endian::Little => f64::from_le_bytes(arr),
        Endian::Big => f64::from_be_bytes(arr),
    })
}

/// Reads an f32 from a byte slice with the given endianness.
fn read_f32(bytes: &[u8], endian: Endian) -> Result<f32, GridError> {
    if bytes.len() < 4 {
        return Err(GridError::CorruptData("not enough bytes for f32".to_string()));
    }
    let arr: [u8; 4] = bytes[..4].try_into().unwrap();
    Ok(match endian {
        Endian::Little => f32::from_le_bytes(arr),
        Endian::Big => f32::from_be_bytes(arr),
    })
}

/// Reads a fixed-length string from a byte slice, trimming trailing spaces.
fn read_str(bytes: &[u8]) -> Result<String, GridError> {
    let s = std::str::from_utf8(bytes)
        .map_err(|_| GridError::CorruptData("invalid UTF-8 in string".to_string()))?;
    Ok(s.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntv2_grid_contains() {
        let grid = Ntv2Grid {
            name: "test".to_string(),
            id: "TESTGRID".to_string(),
            lineage: "".to_string(),
            ll: (0.0, 0.0),
            ur: (1.0, 1.0),
            del: (0.01, 0.01),
            lim: (100, 100),
            epsilon: 1e-10,
            shifts: vec![(0.0, 0.0); 10000],
        };
        assert!(grid.contains(0.5, 0.5));
        assert!(grid.contains(0.0, 0.0));
        assert!(!grid.contains(1.5, 0.5));
        assert!(!grid.contains(0.5, 1.5));
        assert!(!grid.contains(-0.1, 0.5));
    }

    #[test]
    fn ntv2_grid_is_child_of() {
        let parent = Ntv2Grid {
            name: "parent".to_string(),
            id: "PARENT  ".to_string(),
            lineage: "".to_string(),
            ll: (0.0, 0.0),
            ur: (2.0, 2.0),
            del: (0.02, 0.02),
            lim: (100, 100),
            epsilon: 1e-10,
            shifts: vec![(0.0, 0.0); 10000],
        };
        let child = Ntv2Grid {
            name: "child".to_string(),
            id: "CHILD   ".to_string(),
            lineage: "PARENT  ".to_string(),
            ll: (0.5, 0.5),
            ur: (1.5, 1.5),
            del: (0.01, 0.01),
            lim: (100, 100),
            epsilon: 1e-10,
            shifts: vec![(0.0, 0.0); 10000],
        };
        assert!(child.is_child_of(&parent));
        assert!(!parent.is_child_of(&child));
    }

    #[test]
    fn grid_catalog_find_best() {
        let mut catalog = GridCatalog::new();
        catalog.add(Ntv2Grid {
            name: "root".to_string(),
            id: "ROOT    ".to_string(),
            lineage: "".to_string(),
            ll: (0.0, 0.0),
            ur: (2.0, 2.0),
            del: (0.02, 0.02),
            lim: (100, 100),
            epsilon: 1e-10,
            shifts: vec![(0.0, 0.0); 10000],
        });
        catalog.add(Ntv2Grid {
            name: "child".to_string(),
            id: "CHILD   ".to_string(),
            lineage: "ROOT    ".to_string(),
            ll: (0.5, 0.5),
            ur: (1.5, 1.5),
            del: (0.01, 0.01),
            lim: (100, 100),
            epsilon: 1e-10,
            shifts: vec![(0.0, 0.0); 10000],
        });

        // Point in child area → child grid
        let best = catalog.find_best(1.0, 1.0);
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "child");

        // Point in root but outside child → root grid
        let best = catalog.find_best(0.1, 0.1);
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "root");

        // Point outside all grids → None
        let best = catalog.find_best(3.0, 3.0);
        assert!(best.is_none());
    }

    #[test]
    fn read_str_trims_trailing_spaces() {
        assert_eq!(read_str(b"TESTGRID").unwrap(), "TESTGRID");
        assert_eq!(read_str(b"PARENT  ").unwrap(), "PARENT");
    }

    #[test]
    fn error_display() {
        let err = GridError::FileNotFound("test.gsb".to_string());
        assert_eq!(format!("{err}"), "grid file not found: test.gsb");
    }
}
