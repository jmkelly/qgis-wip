//! Load/style/save pipeline support for the oracle harness (E14/E16.4).
//!
//! The E14 pipeline (load → style → save) emits three families of text
//! artifacts: geometry (canonical WKT), styles (canonical renderer/symbol
//! descriptions) and projects (canonical project summaries). This module makes
//! the oracle harness aware of those families: it classifies a fixture by
//! extension, names the golden that holds its QGIS C++ reference, and compares
//! a Rust-side artifact against that reference.
//!
//! Pure Rust, zero dependencies (ADR-0001): it reuses the crate's own
//! [`Diff`](crate::diff::Diff) and [`GoldenStore`](crate::goldens::GoldenStore).
//! The canonical *body* of each artifact is produced by the caller (the
//! `qgis-validate` `oracle` suite drives the real `qgis-geometry`,
//! `qgis-style` and `qgis-gui` crates); this module owns only the
//! classification and reference-comparison contract.

use std::fmt;
use std::path::Path;

use crate::diff::Diff;
use crate::goldens::{GoldenError, GoldenStore, Mode};

/// One family of load/style/save pipeline artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PipelineKind {
    /// Canonical WKT emitted for a geometry fixture (`.wkt`).
    Geometry,
    /// Canonical renderer/symbol description emitted for a `.qml` style.
    Styles,
    /// Canonical project summary emitted for a `.qgs` project.
    Projects,
}

impl PipelineKind {
    /// Every kind, in report order.
    pub const ALL: [PipelineKind; 3] =
        [PipelineKind::Geometry, PipelineKind::Styles, PipelineKind::Projects];

    /// Report and golden-store name: `geometry`, `styles`, `projects`.
    pub fn name(self) -> &'static str {
        match self {
            PipelineKind::Geometry => "geometry",
            PipelineKind::Styles => "styles",
            PipelineKind::Projects => "projects",
        }
    }

    /// Classifies a fixture by its file extension. Extensions the pipeline
    /// does not emit (`.qgz` archives, raster/provider inputs, ...) are not
    /// pipeline artifacts and return `None`.
    pub fn from_extension(ext: &str) -> Option<PipelineKind> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "wkt" => Some(PipelineKind::Geometry),
            "qml" => Some(PipelineKind::Styles),
            "qgs" => Some(PipelineKind::Projects),
            _ => None,
        }
    }

    /// Classifies a fixture path by extension.
    pub fn from_path(path: &Path) -> Option<PipelineKind> {
        path.extension().and_then(|e| e.to_str()).and_then(PipelineKind::from_extension)
    }

    /// The golden name for a fixture stem: `<kind>/<stem>`.
    pub fn golden_name(self, stem: &str) -> String {
        format!("{}/{}", self.name(), stem)
    }
}

impl fmt::Display for PipelineKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The verdict of comparing one pipeline artifact against its QGIS C++
/// golden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// A golden exists and matches the Rust output.
    Match,
    /// A golden exists and differs; the unified diff is attached.
    Mismatch(Box<Diff>),
    /// No golden has been recorded yet, so the comparison cannot run. The
    /// string names what is missing, so an agent can tell "no reference
    /// available" from "reference drifted".
    Unavailable(String),
}

impl Reference {
    /// True when the artifact and its reference differ.
    pub fn is_mismatch(&self) -> bool {
        matches!(self, Reference::Mismatch(_))
    }
}

/// Compares a Rust-side pipeline artifact against the QGIS C++ golden for
/// `kind`/`stem` in `store`.
///
/// A missing golden is [`Reference::Unavailable`], not an error: the
/// orchestrator records goldens from the C++ reference (`oracle record`), and
/// until then the harness reports the gap rather than a false failure. Both
/// sides have their leading `backend:` provenance line stripped before the
/// comparison (see [`GoldenStore::compare`]), so a golden recorded from the
/// `cpp` backend verifies against the `rust` artifact and vice versa.
pub fn compare_to_golden(
    store: &GoldenStore,
    kind: PipelineKind,
    stem: &str,
    actual: &str,
) -> Result<Reference, GoldenError> {
    let name = kind.golden_name(stem);
    match store.compare(&name, actual, Mode::Verify) {
        Ok(None) => Ok(Reference::Match),
        Ok(Some(diff)) => Ok(Reference::Mismatch(Box::new(diff))),
        Err(GoldenError::MissingGolden { .. }) => Ok(Reference::Unavailable(format!(
            "no QGIS C++ golden at {}",
            store.golden_path(&name).display()
        ))),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("qgis-oracle-pipeline-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn kinds_are_named_and_ordered() {
        assert_eq!(PipelineKind::ALL.map(PipelineKind::name), ["geometry", "styles", "projects"]);
    }

    #[test]
    fn classifies_pipeline_extensions() {
        assert_eq!(PipelineKind::from_extension("wkt"), Some(PipelineKind::Geometry));
        assert_eq!(PipelineKind::from_extension(".QML"), Some(PipelineKind::Styles));
        assert_eq!(
            PipelineKind::from_path(Path::new("/a/b/proj.qgs")),
            Some(PipelineKind::Projects)
        );
    }

    #[test]
    fn rejects_non_pipeline_extensions() {
        assert_eq!(PipelineKind::from_extension("qgz"), None);
        assert_eq!(PipelineKind::from_extension("tif"), None);
        assert_eq!(PipelineKind::from_path(Path::new("/a/b/noext")), None);
    }

    #[test]
    fn golden_name_is_namespaced_by_kind() {
        assert_eq!(PipelineKind::Geometry.golden_name("point"), "geometry/point");
        assert_eq!(PipelineKind::Styles.golden_name("points"), "styles/points");
    }

    #[test]
    fn missing_golden_is_unavailable_not_an_error() {
        let dir = temp_dir("missing");
        let store = GoldenStore::new(&dir);
        let verdict = compare_to_golden(&store, PipelineKind::Geometry, "point", "POINT (1 2)\n")
            .expect("missing golden is not an error");
        match verdict {
            Reference::Unavailable(reason) => {
                assert!(reason.contains("geometry/point.expected"), "reason: {reason}");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn matching_golden_is_a_match() {
        let dir = temp_dir("match");
        fs::create_dir_all(dir.join("styles")).unwrap();
        fs::write(dir.join("styles/points.expected"), "backend: cpp/1\n---\nbody\n").unwrap();
        let store = GoldenStore::new(&dir);
        let verdict = compare_to_golden(
            &store,
            PipelineKind::Styles,
            "points",
            "backend: rust/1\n---\nbody\n",
        )
        .unwrap();
        assert_eq!(verdict, Reference::Match);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drifted_golden_is_a_mismatch() {
        let dir = temp_dir("mismatch");
        fs::create_dir_all(dir.join("projects")).unwrap();
        fs::write(dir.join("projects/joins.expected"), "backend: cpp/1\n---\nold\n").unwrap();
        let store = GoldenStore::new(&dir);
        let verdict = compare_to_golden(
            &store,
            PipelineKind::Projects,
            "joins",
            "backend: rust/1\n---\nnew\n",
        )
        .unwrap();
        assert!(verdict.is_mismatch(), "verdict: {verdict:?}");
        match verdict {
            Reference::Mismatch(diff) => assert!(!diff.is_empty()),
            other => panic!("expected Mismatch, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
