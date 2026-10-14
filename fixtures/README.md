# qgis-fixtures

Loads and validates the fixture store catalog at `manifest.toml` in the
workspace root. Pure Rust, zero
dependencies — the manifest is parsed with a hand-rolled parser for a
constrained TOML subset (`src/parser.rs`).

## API

```rust,no_run
use qgis_fixtures::{Category, Manifest};

let manifest = Manifest::load_default()?;          // or Manifest::load(path)
manifest.validate()?;                              // every listed file exists
for fixture in manifest.by_category(Category::Style) {
    let path = fixture.resolve(manifest.store_root());
    // ...
}
```

- `Manifest::load_default()` reads `$QGIS_FIXTURES_MANIFEST` if set,
  else `../manifest.toml` relative to this crate.
- `Manifest::validate()` checks every entry file — and sidecar —
  exists on disk and reports all problems at once.
- `Category` mirrors the store subdirectories: `qgs-project`, `style`,
  `expression`, `provider-sample`.

## Tests

`cargo test -p qgis-fixtures` parses the real store manifest, asserts
category coverage and counts, verifies every cataloged file exists,
and verifies the reverse: every file in the store is cataloged.
