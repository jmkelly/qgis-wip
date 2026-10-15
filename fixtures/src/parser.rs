//! Minimal parser for the constrained TOML subset used by fixture
//! manifests:
//!
//! - `[meta]` sections with free-form `key = "value"` string pairs
//! - `[fixture.<name>]` sections with the keys `category`, `source`,
//!   `path`, `description` (all required strings) and `sidecars`
//!   (optional array of strings)
//! - `#` comments (whole-line and inline) and blank lines
//! - basic string escapes: `\"`, `\\`, `\n`, `\t`
//!
//! Anything outside that subset is rejected with a line-numbered
//! error, so the manifest stays hand-checkable.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::{Category, FixtureEntry};

/// A manifest syntax error, with the 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number of the offending construct.
    pub line: usize,
    /// What was wrong.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Which `[...]` section a line belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Section {
    Meta,
    Fixture(String),
}

/// Accumulated fields of the `[fixture.<name>]` section being parsed.
#[derive(Debug, Default)]
struct FixtureFields {
    category: Option<Category>,
    source: Option<String>,
    path: Option<String>,
    sidecars: Option<Vec<String>>,
    description: Option<String>,
}

impl FixtureFields {
    /// Builds the entry, reporting the first missing required key.
    fn into_entry(self, name: &str) -> Result<FixtureEntry, String> {
        let category = self.category.ok_or("missing key 'category'")?;
        let source = self.source.ok_or("missing key 'source'")?;
        let path = self.path.ok_or("missing key 'path'")?;
        let description = self.description.ok_or("missing key 'description'")?;
        Ok(FixtureEntry {
            name: name.to_string(),
            category,
            source,
            path: PathBuf::from(path),
            sidecars: self.sidecars.unwrap_or_default().into_iter().map(PathBuf::from).collect(),
            description,
        })
    }
}

/// The section currently being parsed: its kind, the fixture name (empty
/// for `[meta]`), the line its header is on, and its fields.
type CurrentSection = (Section, String, usize, FixtureFields);

/// The parsed form of a manifest: free-form `[meta]` pairs plus the
/// cataloged fixture entries.
pub type ParsedManifest = (Vec<(String, String)>, Vec<FixtureEntry>);

/// Parses manifest text into `(meta, entries)`.
pub fn parse_manifest(text: &str) -> Result<ParsedManifest, ParseError> {
    let mut meta: Vec<(String, String)> = Vec::new();
    let mut entries: Vec<FixtureEntry> = Vec::new();
    let mut section: Option<CurrentSection> = None;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = strip_inline_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[') {
            let name = parse_section_header(name, line_no)?;
            if let Some((Section::Fixture(_), prev_name, prev_line, prev_fields)) = section.take() {
                entries.push(build_entry(prev_name, prev_fields, prev_line)?);
            }
            let fixture_name = match &name {
                Section::Fixture(n) => n.clone(),
                Section::Meta => String::new(),
            };
            section = Some((name, fixture_name, line_no, FixtureFields::default()));
            continue;
        }

        let (key, value) = parse_key_value(line, line_no)?;
        let (sec, _, section_line, fields) = section.as_mut().ok_or_else(|| ParseError {
            line: line_no,
            message: "key-value pair outside of any section".to_string(),
        })?;
        match sec {
            Section::Meta => meta.push((key, parse_string(&value, line_no)?)),
            Section::Fixture(_) => {
                const KEYS: [&str; 5] = ["category", "source", "path", "sidecars", "description"];
                if !KEYS.contains(&key.as_str()) {
                    return Err(ParseError {
                        line: line_no,
                        message: format!(
                            "unknown key '{key}' (expected one of {})",
                            KEYS.join(", ")
                        ),
                    });
                }
                match key.as_str() {
                    "category" => {
                        fields.category = Some(
                            Category::from_str(&parse_string(&value, line_no)?)
                                .map_err(|message| ParseError { line: line_no, message })?,
                        );
                    }
                    "source" => fields.source = Some(parse_string(&value, line_no)?),
                    "path" => fields.path = Some(parse_string(&value, line_no)?),
                    "description" => fields.description = Some(parse_string(&value, line_no)?),
                    "sidecars" => {
                        fields.sidecars = Some(parse_string_array(&value, line_no)?);
                    }
                    _ => unreachable!("key whitelist checked above"),
                }
                let _ = section_line;
            }
        }
    }

    if let Some((Section::Fixture(_), name, section_line, fields)) = section.take() {
        entries.push(build_entry(name, fields, section_line)?);
    }

    let mut seen: Vec<&str> = Vec::new();
    for entry in &entries {
        if seen.contains(&entry.name.as_str()) {
            return Err(ParseError {
                line: 0,
                message: format!("duplicate fixture name '{}'", entry.name),
            });
        }
        seen.push(&entry.name);
    }

    Ok((meta, entries))
}

fn build_entry(
    name: String,
    fields: FixtureFields,
    line_no: usize,
) -> Result<FixtureEntry, ParseError> {
    fields
        .into_entry(&name)
        .map_err(|message| ParseError { line: line_no, message })
}

fn parse_section_header(rest: &str, line_no: usize) -> Result<Section, ParseError> {
    let name = rest
        .strip_suffix(']')
        .ok_or_else(|| ParseError {
            line: line_no,
            message: "unterminated section header".to_string(),
        })?
        .trim();
    if name.is_empty() {
        return Err(ParseError {
            line: line_no,
            message: "empty section name".to_string(),
        });
    }
    if name == "meta" {
        return Ok(Section::Meta);
    }
    if let Some(fixture_name) = name.strip_prefix("fixture.") {
        let fixture_name = fixture_name.trim();
        if fixture_name.is_empty() {
            return Err(ParseError {
                line: line_no,
                message: "empty fixture name".to_string(),
            });
        }
        return Ok(Section::Fixture(fixture_name.to_string()));
    }
    Err(ParseError {
        line: line_no,
        message: format!("unknown section '[{name}]' (expected [meta] or [fixture.<name>])"),
    })
}

fn parse_key_value(line: &str, line_no: usize) -> Result<(String, String), ParseError> {
    let (key, rest) = line.split_once('=').ok_or_else(|| ParseError {
        line: line_no,
        message: format!("expected 'key = \"value\"', got '{line}'"),
    })?;
    let key = key.trim();
    if key.is_empty() {
        return Err(ParseError {
            line: line_no,
            message: "empty key".to_string(),
        });
    }
    Ok((key.to_string(), rest.trim().to_string()))
}

/// Parses a quoted string value, handling the supported escapes.
fn parse_string(raw: &str, line_no: usize) -> Result<String, ParseError> {
    let mut chars = raw.chars();
    if chars.next() != Some('"') {
        return Err(ParseError {
            line: line_no,
            message: format!("expected quoted string, got '{raw}'"),
        });
    }
    let mut out = String::new();
    let mut escaped = false;
    let mut closed = false;
    for ch in chars {
        if escaped {
            match ch {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => {
                    return Err(ParseError {
                        line: line_no,
                        message: format!("unsupported escape '\\{other}'"),
                    });
                }
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            closed = true;
        } else if closed {
            return Err(ParseError {
                line: line_no,
                message: format!("unexpected trailing content after string in '{raw}'"),
            });
        } else {
            out.push(ch);
        }
    }
    if !closed {
        return Err(ParseError {
            line: line_no,
            message: "unterminated string".to_string(),
        });
    }
    Ok(out)
}

/// Parses an inline array of quoted strings: `["a", "b"]`.
fn parse_string_array(raw: &str, line_no: usize) -> Result<Vec<String>, ParseError> {
    let inner = raw
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| ParseError {
            line: line_no,
            message: format!("expected array of strings, got '{raw}'"),
        })?
        .trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut parts: Vec<&str> = inner.split(',').collect();
    if parts.last().is_some_and(|p| p.trim().is_empty()) {
        parts.pop();
    }
    let mut items = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            return Err(ParseError {
                line: line_no,
                message: "empty item in string array".to_string(),
            });
        }
        items.push(parse_string(part, line_no)?);
    }
    Ok(items)
}

/// Trims an inline `#` comment, respecting quoted strings.
fn strip_inline_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut prev_backslash = false;
    for (idx, ch) in line.char_indices() {
        if in_string {
            if prev_backslash {
                prev_backslash = false;
            } else if ch == '\\' {
                prev_backslash = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == '#' {
            return &line[..idx];
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meta_and_fixture_sections() {
        let text = "[meta]\nname = \"store\"\nversion = \"1\"\n\n[fixture.alpha]\ncategory = \"style\"\nsource = \"tests/testdata/points.qml\"\npath = \"styles/points.qml\"\ndescription = \"a point style\"\n";
        let (meta, entries) = parse_manifest(text).unwrap();
        assert_eq!(meta.len(), 2);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name(), "alpha");
        assert_eq!(entries[0].category(), Category::Style);
    }

    #[test]
    fn parses_sidecars_array() {
        let text = "[fixture.shp]\ncategory = \"provider-sample\"\nsource = \"s.shp\"\npath = \"p/s.shp\"\nsidecars = [\"p/s.shx\", \"p/s.dbf\",]\ndescription = \"d\"\n";
        let (_, entries) = parse_manifest(text).unwrap();
        assert_eq!(entries[0].sidecars().len(), 2);
    }

    #[test]
    fn empty_sidecars_array_is_ok() {
        let text = "[fixture.s]\ncategory = \"style\"\nsource = \"s\"\npath = \"s.qml\"\nsidecars = []\ndescription = \"d\"\n";
        let (_, entries) = parse_manifest(text).unwrap();
        assert_eq!(entries[0].sidecars().len(), 0);
    }

    #[test]
    fn handles_comments_blank_lines_and_escapes() {
        let text = "# leading comment\n\n[meta]\nname = \"a\\nb\" # inline comment\n";
        let (meta, _) = parse_manifest(text).unwrap();
        assert_eq!(meta[0].1, "a\nb");
    }

    #[test]
    fn hash_inside_quoted_string_is_not_a_comment() {
        // A '#' inside a quoted value (with escaped quotes) must be kept;
        // only the trailing inline comment is stripped.
        let text = "[fixture.a]\ncategory = \"style\"\nsource = \"s\"\npath = \"p\"\ndescription = \"color #ff0000 in \\\"theme\\\"\" # trailing comment\n";
        let (_, entries) = parse_manifest(text).unwrap();
        assert_eq!(entries[0].description(), "color #ff0000 in \"theme\"");
    }

    #[test]
    fn rejects_unknown_section() {
        let err = parse_manifest("[bogus]\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("unknown section"), "message: {}", err.message);
    }

    #[test]
    fn rejects_key_outside_section() {
        let err = parse_manifest("name = \"x\"\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("outside"), "message: {}", err.message);
    }

    #[test]
    fn rejects_unknown_fixture_key() {
        let text = "[fixture.a]\ncategory = \"style\"\nsource = \"s\"\npath = \"p\"\nbogus = \"x\"\ndescription = \"d\"\n";
        let err = parse_manifest(text).unwrap_err();
        assert_eq!(err.line, 5);
        assert!(err.message.contains("unknown key"), "message: {}", err.message);
    }

    #[test]
    fn rejects_missing_required_key() {
        let text = "[fixture.a]\ncategory = \"style\"\n";
        let err = parse_manifest(text).unwrap_err();
        assert!(err.message.contains("source"), "message: {}", err.message);
    }

    #[test]
    fn rejects_bad_category() {
        let text = "[fixture.a]\ncategory = \"bogus\"\nsource = \"s\"\npath = \"p\"\ndescription = \"d\"\n";
        let err = parse_manifest(text).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("unknown category"), "message: {}", err.message);
    }

    #[test]
    fn rejects_duplicate_fixture_name() {
        let text = "[fixture.a]\ncategory = \"style\"\nsource = \"s\"\npath = \"p\"\ndescription = \"d\"\n\n[fixture.a]\ncategory = \"style\"\nsource = \"s2\"\npath = \"p2\"\ndescription = \"d2\"\n";
        let err = parse_manifest(text).unwrap_err();
        assert!(err.message.contains("duplicate"), "message: {}", err.message);
    }

    #[test]
    fn rejects_unterminated_string() {
        let text = "[fixture.a]\ncategory = \"style\n";
        let err = parse_manifest(text).unwrap_err();
        assert!(err.message.contains("unterminated"), "message: {}", err.message);
    }

    #[test]
    fn rejects_bare_value() {
        let text = "[fixture.a]\ncategory = style\n";
        let err = parse_manifest(text).unwrap_err();
        assert!(err.message.contains("quoted string"), "message: {}", err.message);
    }

    #[test]
    fn rejects_malformed_array() {
        let text = "[fixture.a]\ncategory = \"style\"\nsource = \"s\"\npath = \"p\"\nsidecars = [\"x\" \"y\"]\ndescription = \"d\"\n";
        let err = parse_manifest(text).unwrap_err();
        assert!(err.message.contains("trailing content"), "message: {}", err.message);
    }

    #[test]
    fn rejects_unsupported_escape() {
        let text = "[fixture.a]\ncategory = \"style\"\nsource = \"s\"\npath = \"p\"\ndescription = \"a\\qb\"\n";
        let err = parse_manifest(text).unwrap_err();
        assert!(err.message.contains("escape"), "message: {}", err.message);
    }

    #[test]
    fn parse_error_display_includes_line() {
        let err = ParseError { line: 7, message: "boom".to_string() };
        assert_eq!(err.to_string(), "line 7: boom");
    }
}
