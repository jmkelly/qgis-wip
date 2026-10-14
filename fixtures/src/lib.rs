//! Fixture store catalog for the QGIS Rust port oracle (ADR-0004).
//!
//! Loads and validates `manifest.toml` — the catalog of
//! golden input fixtures collected from the QGIS 4 test suite — and
//! iterates the store by category. Pure Rust, zero dependencies: the
//! manifest is parsed with a small hand-rolled parser for a constrained
//! TOML subset (see [`parser`] and the store README).

pub mod parser;

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Fixture categories, mirroring the store's subdirectories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// `.qgs` / `.qgz` QGIS project files.
    QgsProject,
    /// `.qml` layer styles.
    Style,
    /// Expression test cases.
    Expression,
    /// Vector/raster provider samples.
    ProviderSample,
}

impl Category {
    /// The manifest spelling of the category.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::QgsProject => "qgs-project",
            Category::Style => "style",
            Category::Expression => "expression",
            Category::ProviderSample => "provider-sample",
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Category {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "qgs-project" => Ok(Category::QgsProject),
            "style" => Ok(Category::Style),
            "expression" => Ok(Category::Expression),
            "provider-sample" => Ok(Category::ProviderSample),
            other => Err(format!(
                "unknown category '{other}' (expected qgs-project, style, expression or provider-sample)"
            )),
        }
    }
}

/// One cataloged fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureEntry {
    name: String,
    category: Category,
    source: String,
    path: PathBuf,
    sidecars: Vec<PathBuf>,
    description: String,
}

impl FixtureEntry {
    /// The fixture's unique name in the manifest.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The fixture's category.
    pub fn category(&self) -> Category {
        self.category
    }

    /// The fixture's origin: a path in the QGIS 4 tree, or `"synthetic"`.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The fixture file path, relative to the store root.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Additional files required by the fixture (e.g. shapefile sidecars),
    /// relative to the store root.
    pub fn sidecars(&self) -> &[PathBuf] {
        &self.sidecars
    }

    /// What the fixture exercises.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The fixture file path resolved against `store_root`.
    pub fn resolve(&self, store_root: &Path) -> PathBuf {
        store_root.join(&self.path)
    }

    /// The fixture file plus its sidecars, relative to the store root.
    pub fn required_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![self.path.clone()];
        paths.extend(self.sidecars.iter().cloned());
        paths
    }
}

/// A fixture whose file is missing on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Name of the catalog entry with the missing file.
    pub entry: String,
    /// The missing path, relative to the store root.
    pub path: PathBuf,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fixture '{}': missing file {}", self.entry, self.path.display())
    }
}

impl std::error::Error for ValidationError {}

/// The fixture store manifest: catalog metadata plus fixture entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    root: PathBuf,
    meta: Vec<(String, String)>,
    entries: Vec<FixtureEntry>,
}

impl Manifest {
    /// Parses the manifest at `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Manifest, ManifestError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let (meta, entries) = parser::parse_manifest(&text)
            .map_err(|e| ManifestError::Parse { line: e.line, message: e.message })?;
        let root = path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        Ok(Manifest { root, meta, entries })
    }

    /// Loads the store manifest: `$QGIS_FIXTURES_MANIFEST` if set, else
    /// `../manifest.toml` relative to this crate.
    pub fn load_default() -> Result<Manifest, ManifestError> {
        let path = match std::env::var_os("QGIS_FIXTURES_MANIFEST") {
            Some(dir) => PathBuf::from(dir),
            None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../manifest.toml"),
        };
        Manifest::load(&path)
    }

    /// The directory containing the manifest: the fixture store root.
    pub fn store_root(&self) -> &Path {
        &self.root
    }

    /// Free-form catalog metadata from the `[meta]` section, in file order.
    pub fn meta(&self) -> &[(String, String)] {
        &self.meta
    }

    /// All fixture entries, in manifest order.
    pub fn entries(&self) -> &[FixtureEntry] {
        &self.entries
    }

    /// Looks up a fixture by name.
    pub fn find(&self, name: &str) -> Option<&FixtureEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Iterates the fixtures of one category.
    pub fn by_category(&self, category: Category) -> impl Iterator<Item = &FixtureEntry> + '_ {
        self.entries.iter().filter(move |e| e.category == category)
    }

    /// Verifies every listed fixture file — and sidecar — exists on disk.
    /// Returns all problems at once, not just the first.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        for entry in &self.entries {
            for rel in entry.required_paths() {
                if !self.root.join(&rel).is_file() {
                    errors.push(ValidationError { entry: entry.name.clone(), path: rel });
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Errors that can occur while loading a manifest.
#[derive(Debug)]
pub enum ManifestError {
    /// The file could not be read.
    Io {
        /// The manifest path that failed.
        path: PathBuf,
        /// Underlying I/O error message.
        message: String,
    },
    /// The manifest did not parse.
    Parse {
        /// 1-based line number of the offending construct.
        line: usize,
        /// What was wrong.
        message: String,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Io { path, message } => {
                write!(f, "cannot read manifest {}: {message}", path.display())
            }
            ManifestError::Parse { line, message } => {
                write!(f, "manifest parse error on line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp_manifest(text: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "qgis-fixtures-manifest-{}-{}.toml",
            std::process::id(),
            text.len()
        ));
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn load_reads_meta_and_entries() {
        let path = write_temp_manifest(
            "[meta]\nname = \"test-store\"\n\n[fixture.alpha]\ncategory = \"style\"\nsource = \"tests/testdata/points.qml\"\npath = \"styles/points.qml\"\ndescription = \"a point style\"\n",
        );
        let manifest = Manifest::load(&path).unwrap();
        assert_eq!(manifest.meta().len(), 1);
        assert_eq!(manifest.meta()[0], ("name".to_string(), "test-store".to_string()));
        assert_eq!(manifest.entries().len(), 1);
        let entry = &manifest.entries()[0];
        assert_eq!(entry.name(), "alpha");
        assert_eq!(entry.category(), Category::Style);
        assert_eq!(entry.source(), "tests/testdata/points.qml");
        assert_eq!(entry.path(), Path::new("styles/points.qml"));
        assert_eq!(entry.sidecars().len(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_parses_sidecars_array() {
        let path = write_temp_manifest(
            "[fixture.shp]\ncategory = \"provider-sample\"\nsource = \"tests/testdata/bug5598.shp\"\npath = \"provider-samples/bug5598.shp\"\nsidecars = [\"provider-samples/bug5598.shx\", \"provider-samples/bug5598.dbf\"]\ndescription = \"shapefile\"\n",
        );
        let manifest = Manifest::load(&path).unwrap();
        let entry = manifest.find("shp").unwrap();
        assert_eq!(entry.sidecars().len(), 2);
        assert_eq!(entry.required_paths().len(), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_rejects_missing_file() {
        let err = Manifest::load("/nonexistent/definitely-missing-manifest.toml").unwrap_err();
        match err {
            ManifestError::Io { .. } => {}
            other => panic!("expected Io, got {other}"),
        }
    }

    #[test]
    fn load_reports_parse_error_line() {
        let path = write_temp_manifest("[fixture.bad]\ncategory = \"style\"\n");
        let err = Manifest::load(&path).unwrap_err();
        match err {
            ManifestError::Parse { line, message } => {
                assert_eq!(line, 1);
                assert!(message.contains("source"), "message: {message}");
            }
            other => panic!("expected Parse, got {other}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn store_root_is_manifest_parent() {
        let path = write_temp_manifest("[fixture.a]\ncategory = \"style\"\nsource = \"synthetic\"\npath = \"a.qml\"\ndescription = \"d\"\n");
        let manifest = Manifest::load(&path).unwrap();
        assert_eq!(manifest.store_root(), path.parent().unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn category_display_and_from_str_round_trip() {
        for cat in [
            Category::QgsProject,
            Category::Style,
            Category::Expression,
            Category::ProviderSample,
        ] {
            assert_eq!(Category::from_str(cat.as_str()).unwrap(), cat);
            assert_eq!(cat.to_string(), cat.as_str());
        }
        assert!(Category::from_str("bogus").is_err());
    }

    #[test]
    fn validation_error_display_is_human_readable() {
        let err = ValidationError {
            entry: "joins".to_string(),
            path: PathBuf::from("qgs-projects/joins.qgs"),
        };
        let msg = err.to_string();
        assert!(msg.contains("joins"), "message: {msg}");
        assert!(msg.contains("qgs-projects/joins.qgs"), "message: {msg}");
    }
}
