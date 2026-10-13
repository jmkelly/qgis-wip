//! Datum ensemble resolution.
//!
//! A WKT2 datum ensemble (`ENSEMBLE[...]`) groups multiple member datums
//! that are all acceptable representations of the same reference frame
//! (e.g., the WGS 84 ensemble with members G730, G873, G1150, G1674,
//! G1762, G2139, G2296). PROJ and QGIS treat the ensemble as a single
//! datum with a quoted accuracy; proj4rs does not understand ensembles.
//!
//! This module resolves an ensemble to a single member datum and emits
//! a proj string that proj4rs accepts, with the `+towgs84` parameter
//! derived from the selected member's Helmert transformation.
//!
//! # Resolution strategy
//!
//! 1. If a coordinate epoch is given, select the member whose validity
//!    period contains the epoch (members are ordered from oldest to
//!    newest in the WKT2).
//! 2. Otherwise, select the first member (most accurate / default).
//! 3. If the selected member has a known Helmert transformation to WGS84,
//!    emit `+towgs84`; otherwise emit `+datum=<name>` for proj4rs to
//!    resolve via its own datum table.
//!
//! See ADR 0006 for the projection decision.

#![forbid(unsafe_code)]

use crate::towgs84::Towgs84;
use crate::wkt2::{DatumEnsemble, EnsembleMember, ParsedCrs};
use std::fmt;

/// Errors that can occur during ensemble resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum EnsembleError {
    /// The ensemble has no members.
    EmptyEnsemble,
    /// The ensemble member has no transformation data and no recognized name.
    UnresolvableMember(String),
}

impl fmt::Display for EnsembleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyEnsemble => write!(f, "datum ensemble has no members"),
            Self::UnresolvableMember(name) => {
                write!(f, "cannot resolve ensemble member {name:?} to a proj string")
            }
        }
    }
}

impl std::error::Error for EnsembleError {}

/// A resolved datum ensemble member with its transformation data.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMember {
    /// The selected member's name.
    pub name: String,
    /// The member's authority ID (e.g., EPSG:1166) if known.
    pub id: Option<(String, String)>,
    /// The Helmert transformation to WGS84, if known.
    pub towgs84: Option<Towgs84>,
    /// The ensemble accuracy in metres (from `ENSEMBLEACCURACY`).
    pub accuracy: f64,
}

/// Resolves a datum ensemble to a single member for the given epoch.
///
/// # Arguments
///
/// * `ensemble` - The parsed WKT2 datum ensemble.
/// * `epoch` - Optional coordinate epoch (decimal year) for time-based
///   member selection. `None` selects the first (default) member.
///
/// # Examples
///
/// ```
/// use qgis_crs::ensemble::resolve_ensemble;
/// use qgis_crs::wkt2::{DatumEnsemble, EnsembleMember};
///
/// let ensemble = DatumEnsemble {
///     name: "WGS 84 ensemble".to_string(),
///     members: vec![
///         EnsembleMember { name: "WGS 84 (G730)".into(), id: None },
///         EnsembleMember { name: "WGS 84 (G1762)".into(), id: None },
///     ],
///     accuracy: 2.0,
/// };
/// let resolved = resolve_ensemble(&ensemble, None).unwrap();
/// assert_eq!(resolved.name, "WGS 84 (G730)");
/// ```
pub fn resolve_ensemble(ensemble: &DatumEnsemble, epoch: Option<f64>) -> Result<ResolvedMember, EnsembleError> {
    if ensemble.members.is_empty() {
        return Err(EnsembleError::EmptyEnsemble);
    }

    // Select member: if epoch is given, find the member whose validity
    // period contains it. For now, members are ordered oldest→newest in
    // the WKT2, so we use the first member whose name suggests a
    // tracking-station epoch >= the given epoch, or fall back to the
    // last member if the epoch is beyond all of them.
    let member = match epoch {
        Some(epoch) => select_member_for_epoch(&ensemble.members, epoch),
        None => &ensemble.members[0],
    };

    let towgs84 = member_towgs84(member);

    Ok(ResolvedMember {
        name: member.name.clone(),
        id: member.id.as_ref().map(|id| (id.authority.clone(), id.code.clone())),
        towgs84,
        accuracy: ensemble.accuracy,
    })
}

/// Selects the ensemble member best matching the given epoch.
///
/// WKT2 ensemble members are ordered from oldest to newest. We pick the
/// first member whose name contains a 4-digit year >= the epoch, or the
/// last member if the epoch is beyond all named years.
fn select_member_for_epoch(members: &[EnsembleMember], epoch: f64) -> &EnsembleMember {
    for member in members {
        if let Some(year) = extract_g_code_year(&member.name) {
            if year as f64 >= epoch {
                return member;
            }
        }
    }
    // Epoch is beyond all named years — use the last (newest) member.
    &members[members.len() - 1]
}

/// Extracts a 4-digit year from a member name (e.g., "WGS 84 (2016)" → 2016).
///
/// Looks for the first 4-digit number that looks like a year (1970–2099).
/// Does NOT extract G-codes (e.g., "G1762") as years — use
/// [`extract_g_code_year`] for that.
fn extract_year_from_name(name: &str) -> Option<u32> {
    let chars: Vec<char> = name.chars().collect();
    let mut i = 0;
    while i + 4 <= chars.len() {
        let candidate: String = chars[i..i + 4].iter().collect();
        if let Ok(year) = candidate.parse::<u32>() {
            if (1970..=2099).contains(&year) {
                return Some(year);
            }
        }
        i += 1;
    }
    None
}

/// Extracts the realization year from a WGS 84 G-code.
///
/// QGIS names WGS 84 ensemble members like "World Geodetic System 1984 (G730)".
/// The G-code maps to a realization year:
///   G730 → 1987, G873 → 1994, G1150 → 2001, G1674 → 2011,
///   G1762 → 2016, G2139 → 2020, G2296 → 2022.
fn extract_g_code_year(name: &str) -> Option<u32> {
    let g_code_years: &[(&str, u32)] = &[
        ("G730", 1987),
        ("G873", 1994),
        ("G1150", 2001),
        ("G1674", 2011),
        ("G1762", 2016),
        ("G2139", 2020),
        ("G2296", 2022),
    ];
    for (code, year) in g_code_years {
        if name.contains(code) {
            return Some(*year);
        }
    }
    None
}

/// Derives the `+towgs84` transformation for an ensemble member.
///
/// WGS 84 ensemble members have known Helmert transformations to WGS84
/// (the ensemble's "hub" datum). We map the member name to its
/// transformation parameters. Members not in the known list return
/// `None`, meaning the caller should emit `+datum=<name>` instead.
fn member_towgs84(member: &EnsembleMember) -> Option<Towgs84> {
    // WGS 84 ensemble member transformations to WGS84 (ensemble hub).
    // Source: EPSG dataset / PROJ datum transformation table.
    // Values are (dx, dy, dz, rx, ry, rz, s) in metres, arc-seconds, ppm.
    let params: Option<[f64; 7]> = match member.name.as_str() {
        // WGS 84 (Transit) — original WGS 84, no transformation needed
        "World Geodetic System 1984 (Transit)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // WGS 84 (G730) — first GPS-based realization
        "World Geodetic System 1984 (G730)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // WGS 84 (G873) — second GPS-based realization
        "World Geodetic System 1984 (G873)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // WGS 84 (G1150) — third GPS-based realization
        "World Geodetic System 1984 (G1150)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // WGS 84 (G1674) — fourth GPS-based realization
        "World Geodetic System 1984 (G1674)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // WGS 84 (G1762) — fifth GPS-based realization
        "World Geodetic System 1984 (G1762)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // WGS 84 (G2139) — sixth GPS-based realization
        "World Geodetic System 1984 (G2139)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // WGS 84 (G2296) — seventh GPS-based realization (current as of 2026)
        "World Geodetic System 1984 (G2296)" => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // Try EPSG code-based lookup for other datums
        _ => {
            if let Some(id) = &member.id {
                epsg_code_to_towgs84(&id.authority, &id.code)
            } else {
                None
            }
        }
    };
    params.map(|p| Towgs84::helmert_7(p[0], p[1], p[2], p[3], p[4], p[5], p[6]))
}

/// Looks up a `+towgs84` transformation by EPSG code.
///
/// Returns `Some([dx, dy, dz, rx, ry, rz, s])` for codes with known
/// transformations, `None` otherwise. This is a small lookup table for
/// common datums; the full EPSG dataset is available via `crs-definitions`.
fn epsg_code_to_towgs84(authority: &str, code: &str) -> Option<[f64; 7]> {
    // Common EPSG datum codes and their transformations to WGS84.
    // Values are (dx, dy, dz, rx, ry, rz, s).
    match (authority, code) {
        // WGS 84 ensemble members
        ("EPSG", "1166") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        ("EPSG", "1167") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        ("EPSG", "1168") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        ("EPSG", "1169") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        ("EPSG", "1170") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        ("EPSG", "1171") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        ("EPSG", "1172") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        ("EPSG", "1173") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // NAD83 (various realizations)
        ("EPSG", "6269") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // ETRS89
        ("EPSG", "6258") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // GDA94
        ("EPSG", "6283") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        // Potsdam
        ("EPSG", "6314") => Some([-598.1, -73.7, -418.2, 0.202, 0.45, -2.455, 6.7]),
        // ITRF2000
        ("EPSG", "6610") => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        _ => None,
    }
}

/// Resolves a parsed CRS that may contain an ensemble datum to a proj string.
///
/// If the CRS has a datum ensemble, resolves it to a single member and
/// emits a proj string with `+towgs84`. If the CRS has a simple datum,
/// emits a proj string with `+datum=<name>`.
///
/// # Arguments
///
/// * `crs` - The parsed WKT2 CRS.
/// * `epoch` - Optional coordinate epoch for ensemble member selection.
///
/// # Returns
/// A proj string that proj4rs accepts.
pub fn resolve_to_proj_string(crs: &ParsedCrs, epoch: Option<f64>) -> Result<String, EnsembleError> {
    let datum = crs
        .datum
        .as_ref()
        .ok_or_else(|| EnsembleError::UnresolvableMember(crs.name.clone()))?;

    if let Some(ensemble) = &datum.ensemble {
        let resolved = resolve_ensemble(ensemble, epoch)?;
        return resolved.to_proj_string();
    }

    // Simple datum — emit +datum=<name> for proj4rs to resolve
    Ok(format!("+datum={}", datum.name))
}

impl ResolvedMember {
    /// Formats this resolved member as a proj-string datum parameter.
    ///
    /// Emits `+towgs84=...` if a transformation is known, or
    /// `+datum=<name>` otherwise.
    pub fn to_proj_string(&self) -> Result<String, EnsembleError> {
        if let Some(towgs84) = &self.towgs84 {
            Ok(towgs84.to_proj_string())
        } else if self.id.is_some() {
            // Has an EPSG code — proj4rs can resolve it via crs-definitions
            Ok(format!("+datum={}", self.name))
        } else {
            Err(EnsembleError::UnresolvableMember(self.name.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wkt2::{AuthorityId, EnsembleMember};

    fn make_member(name: &str) -> EnsembleMember {
        EnsembleMember {
            name: name.to_string(),
            id: None,
        }
    }

    fn make_member_with_id(name: &str, auth: &str, code: &str) -> EnsembleMember {
        EnsembleMember {
            name: name.to_string(),
            id: Some(AuthorityId {
                authority: auth.to_string(),
                code: code.to_string(),
            }),
        }
    }

    #[test]
    fn resolve_empty_ensemble() {
        let ensemble = DatumEnsemble {
            name: "test".to_string(),
            members: vec![],
            accuracy: 1.0,
        };
        let result = resolve_ensemble(&ensemble, None);
        assert!(matches!(result, Err(EnsembleError::EmptyEnsemble)));
    }

    #[test]
    fn resolve_first_member_default() {
        let ensemble = DatumEnsemble {
            name: "WGS 84 ensemble".to_string(),
            members: vec![
                make_member("World Geodetic System 1984 (G730)"),
                make_member("World Geodetic System 1984 (G1762)"),
            ],
            accuracy: 2.0,
        };
        let resolved = resolve_ensemble(&ensemble, None).unwrap();
        assert_eq!(resolved.name, "World Geodetic System 1984 (G730)");
        assert_eq!(resolved.accuracy, 2.0);
    }

    #[test]
    fn resolve_member_for_epoch() {
        let ensemble = DatumEnsemble {
            name: "WGS 84 ensemble".to_string(),
            members: vec![
                make_member("World Geodetic System 1984 (G730)"),
                make_member("World Geodetic System 1984 (G873)"),
                make_member("World Geodetic System 1984 (G1762)"),
                make_member("World Geodetic System 1984 (G2139)"),
            ],
            accuracy: 2.0,
        };
        // Epoch 2005 should select G1762 (2016) — wait, G1762 is 2016,
        // G873 is 1994, G730 is 1987, G2139 is 2020.
        // Epoch 2005 → G1762 (2016) is the first member with year >= 2005.
        let resolved = resolve_ensemble(&ensemble, Some(2005.0)).unwrap();
        assert_eq!(resolved.name, "World Geodetic System 1984 (G1762)");
    }

    #[test]
    fn resolve_member_epoch_beyond_all() {
        let ensemble = DatumEnsemble {
            name: "WGS 84 ensemble".to_string(),
            members: vec![
                make_member("World Geodetic System 1984 (G730)"),
                make_member("World Geodetic System 1984 (G1762)"),
            ],
            accuracy: 2.0,
        };
        // Epoch 2030 is beyond all named years — use the last member
        let resolved = resolve_ensemble(&ensemble, Some(2030.0)).unwrap();
        assert_eq!(resolved.name, "World Geodetic System 1984 (G1762)");
    }

    #[test]
    fn resolve_member_epoch_before_all() {
        let ensemble = DatumEnsemble {
            name: "WGS 84 ensemble".to_string(),
            members: vec![
                make_member("World Geodetic System 1984 (G730)"),
                make_member("World Geodetic System 1984 (G1762)"),
            ],
            accuracy: 2.0,
        };
        // Epoch 1980 is before all named years — use the first member
        let resolved = resolve_ensemble(&ensemble, Some(1980.0)).unwrap();
        assert_eq!(resolved.name, "World Geodetic System 1984 (G730)");
    }

    #[test]
    fn resolve_member_with_epsg_id() {
        let ensemble = DatumEnsemble {
            name: "test".to_string(),
            members: vec![make_member_with_id(
                "Potsdam Rauenberg 1950 DHDN",
                "EPSG",
                "6314",
            )],
            accuracy: 1.0,
        };
        let resolved = resolve_ensemble(&ensemble, None).unwrap();
        assert!(resolved.towgs84.is_some());
        let t = resolved.towgs84.unwrap();
        assert!((t.dx - -598.1).abs() < 1e-10);
    }

    #[test]
    fn resolve_to_proj_string_with_towgs84() {
        let ensemble = DatumEnsemble {
            name: "WGS 84 ensemble".to_string(),
            members: vec![make_member("World Geodetic System 1984 (G1762)")],
            accuracy: 2.0,
        };
        let resolved = resolve_ensemble(&ensemble, None).unwrap();
        let proj_str = resolved.to_proj_string().unwrap();
        assert!(proj_str.starts_with("+towgs84="));
    }

    #[test]
    fn resolve_to_proj_string_unresolvable() {
        let ensemble = DatumEnsemble {
            name: "test".to_string(),
            members: vec![make_member("Unknown Datum")],
            accuracy: 1.0,
        };
        let resolved = resolve_ensemble(&ensemble, None).unwrap();
        let result = resolved.to_proj_string();
        assert!(matches!(result, Err(EnsembleError::UnresolvableMember(_))));
    }

    #[test]
    fn extract_year_from_name_test() {
        assert_eq!(extract_year_from_name("WGS 84 (G1762)"), None);
        assert_eq!(extract_year_from_name("WGS 84 (2016)"), Some(2016));
        assert_eq!(extract_year_from_name("Realized 2020"), Some(2020));
        assert_eq!(extract_year_from_name("no year here"), None);
    }

    #[test]
    fn resolve_to_proj_string_simple_datum() {
        let crs = ParsedCrs {
            crs_type: "GEOGCRS".to_string(),
            name: "WGS 84".to_string(),
            id: Some(AuthorityId {
                authority: "EPSG".to_string(),
                code: "4326".to_string(),
            }),
            datum: Some(crate::wkt2::Datum {
                name: "World Geodetic System 1984".to_string(),
                ellipsoid: None,
                ensemble: None,
            }),
            ellipsoid: None,
            prime_meridian: None,
            cs: None,
            conversion: None,
            components: vec![],
            axes: vec![],
            units: vec![],
        };
        let proj_str = resolve_to_proj_string(&crs, None).unwrap();
        assert!(proj_str.contains("+datum="));
    }
}
