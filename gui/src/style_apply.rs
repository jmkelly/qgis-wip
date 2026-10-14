//! Style application: classify features by attribute and assign a symbol.
//!
//! Bridges the [`qgis_core::Feature`] attribute model onto the [`qgis_style`]
//! renderers so a layer plus a parsed `.qml` style can be turned into a
//! `Vec<(Feature, Symbol)>` pair list that the map canvas draws directly.
//!
//! Renderer semantics mirror QGIS:
//!
//! - `Single` assigns its one symbol to every feature.
//! - `Categorized` matches the attribute value against the class labels; the
//!   first matching class wins.
//! - `Graduated` parses the attribute as a number and matches the first range
//!   whose bounds contain it (inclusive on both ends).
//!
//! Features that match no class/range are *unmatched*: they are excluded from
//! the styled output (QGIS does not draw them unless the renderer defines a
//! catch-all), and their ids are reported so callers can surface the gap.

use std::collections::BTreeMap;

use qgis_core::{Feature, Value};
use qgis_style::{Renderer, StyleModel, Symbol};

/// Renders a feature attribute as the string used for class/range matching.
///
/// Numeric values are formatted without a trailing `.0` so a DBF integer `1`
/// matches a QML class label of `"1"` rather than `"1.0"`.
pub fn value_to_match_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Value::Text(s) => s.clone(),
    }
}

/// Flattens a feature's attributes into the string map the style crate matches on.
pub fn feature_attributes(feature: &Feature) -> BTreeMap<String, String> {
    feature
        .attributes
        .iter()
        .map(|(k, v)| (k.clone(), value_to_match_string(v)))
        .collect()
}

/// Returns the symbol the renderer assigns to `feature`, or `None` when the
/// feature matches no class or range.
pub fn classify_feature(renderer: &Renderer, feature: &Feature) -> Option<Symbol> {
    qgis_style::classify(renderer, &feature_attributes(feature))
}

/// Classifies every feature, dropping the ones the renderer does not cover.
pub fn apply_style(renderer: &Renderer, features: &[Feature]) -> Vec<(Feature, Symbol)> {
    features
        .iter()
        .filter_map(|f| classify_feature(renderer, f).map(|s| (f.clone(), s)))
        .collect()
}

/// Result of applying a renderer to a feature set, with per-class statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleReport {
    /// Styled features in input order, ready for rendering.
    pub styled: Vec<(Feature, Symbol)>,
    /// Ids of features the renderer matched no class/range for.
    pub unmatched: Vec<i64>,
    /// Number of styled features per class value (categorized) or range label
    /// (graduated); empty for a single-symbol renderer.
    pub class_counts: BTreeMap<String, usize>,
    /// The attribute the renderer classifies by, if it uses one.
    pub classified_by: Option<String>,
}

/// Applies `model`'s renderer to `features`, reporting unmatched ids and class
/// histogram alongside the styled pairs.
pub fn apply_style_report(renderer: &Renderer, features: &[Feature]) -> StyleReport {
    let classified_by = match renderer {
        Renderer::Single(_) => None,
        Renderer::Categorized { attr, .. } | Renderer::Graduated { attr, .. } => Some(attr.clone()),
    };

    let mut styled = Vec::new();
    let mut unmatched = Vec::new();
    let mut class_counts: BTreeMap<String, usize> = BTreeMap::new();

    for feature in features {
        match classify_feature(renderer, feature) {
            Some(symbol) => {
                if let Some(attr) = &classified_by {
                    let key = feature
                        .attribute(attr)
                        .map(value_to_match_string)
                        .unwrap_or_default();
                    *class_counts.entry(key).or_insert(0) += 1;
                }
                styled.push((feature.clone(), symbol));
            }
            None => unmatched.push(feature.id),
        }
    }

    StyleReport {
        styled,
        unmatched,
        class_counts,
        classified_by,
    }
}

/// Convenience wrapper: apply a whole [`StyleModel`] to a feature set.
pub fn apply_model(model: &StyleModel, features: &[Feature]) -> Vec<(Feature, Symbol)> {
    apply_style(&model.renderer, features)
}

/// Reads a shapefile (`.shp` + `.dbf`) into [`Feature`]s ready for styling.
pub fn read_shapefile_layer(path: impl AsRef<std::path::Path>) -> Result<Vec<Feature>, String> {
    let raw = qgis_providers::shapefile::read_shapefile(path)?;
    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            let mut feature = Feature::new(i as i64, shape_to_geometry(&f.geometry));
            for (k, v) in f.attributes {
                feature.set_attribute(k, json_to_value(&v));
            }
            feature
        })
        .collect())
}

fn shape_to_geometry(shape: &qgis_providers::shapefile::Shape) -> Option<qgis_geometry::Geometry> {
    use qgis_geometry::{Geometry, LineString, Point, Polygon};
    match shape {
        qgis_providers::shapefile::Shape::Null => None,
        qgis_providers::shapefile::Shape::Point { x, y } => {
            Some(Geometry::Point(Point::new(*x, *y)))
        }
        qgis_providers::shapefile::Shape::PointZ { x, y, z } => {
            let mut p = Point::new(*x, *y);
            p.z = Some(*z);
            Some(Geometry::Point(p))
        }
        qgis_providers::shapefile::Shape::Polyline { parts } => {
            let lines: Vec<LineString> = parts
                .iter()
                .map(|part| LineString::new(part.iter().map(|(x, y)| Point::new(*x, *y)).collect()))
                .collect();
            match lines.len() {
                0 => None,
                1 => Some(Geometry::LineString(lines.into_iter().next().unwrap())),
                _ => Some(Geometry::MultiLineString(
                    qgis_geometry::MultiLineString::new(lines),
                )),
            }
        }
        qgis_providers::shapefile::Shape::Polygon { parts } => {
            if parts.is_empty() {
                return None;
            }
            let exterior: Vec<Point> = parts[0].iter().map(|(x, y)| Point::new(*x, *y)).collect();
            let holes: Vec<Vec<Point>> = parts[1..]
                .iter()
                .map(|part| part.iter().map(|(x, y)| Point::new(*x, *y)).collect())
                .collect();
            let polygon = if holes.is_empty() {
                Polygon::new(exterior)
            } else {
                Polygon::with_holes(exterior, holes)
            };
            Some(Geometry::Polygon(polygon))
        }
    }
}

fn json_to_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(f64::NAN))
            }
        }
        serde_json::Value::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_style::Color;

    fn poly(fill: (u8, u8, u8)) -> Symbol {
        Symbol::Polygon {
            fill: Color {
                r: fill.0,
                g: fill.1,
                b: fill.2,
                a: 255,
            },
            outline: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            outline_width: 0.3,
            style: "solid".into(),
        }
    }

    fn marker(color: u8) -> Symbol {
        Symbol::Point {
            shape: "circle".into(),
            size: 4.0,
            color: Color {
                r: color,
                g: color,
                b: color,
                a: 255,
            },
            outline: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }
    }

    fn feature(id: i64, attrs: &[(&str, Value)]) -> Feature {
        let mut f = Feature::new(id, None);
        for (k, v) in attrs {
            f.set_attribute(*k, v.clone());
        }
        f
    }

    fn categorized() -> Renderer {
        Renderer::Categorized {
            attr: "Name".into(),
            classes: vec![("Dam".into(), poly((1, 2, 3))), ("Lake".into(), poly((4, 5, 6)))],
        }
    }

    #[test]
    fn single_symbol_applies_to_every_feature() {
        let renderer = Renderer::Single(poly((9, 9, 9)));
        let features = vec![
            feature(1, &[("Name", Value::Text("Dam".into()))]),
            feature(2, &[("Name", Value::Text("Unlisted".into()))]),
        ];
        let styled = apply_style(&renderer, &features);
        assert_eq!(styled.len(), 2);
        assert!(styled.iter().all(|(_, s)| *s == poly((9, 9, 9))));

        let report = apply_style_report(&renderer, &features);
        assert!(report.classified_by.is_none());
        assert!(report.class_counts.is_empty());
        assert!(report.unmatched.is_empty());
    }

    #[test]
    fn categorized_matches_class_labels() {
        let renderer = categorized();
        let features = vec![
            feature(1, &[("Name", Value::Text("Dam".into()))]),
            feature(2, &[("Name", Value::Text("Lake".into()))]),
            feature(3, &[("Name", Value::Text("Swamp".into()))]),
            feature(4, &[("Other", Value::Text("Dam".into()))]),
        ];
        let report = apply_style_report(&renderer, &features);
        assert_eq!(report.styled.len(), 2);
        assert_eq!(report.styled[0].1, poly((1, 2, 3)));
        assert_eq!(report.styled[1].1, poly((4, 5, 6)));
        assert_eq!(report.unmatched, vec![3, 4]);
        assert_eq!(report.classified_by.as_deref(), Some("Name"));
        assert_eq!(report.class_counts.get("Dam"), Some(&1));
        assert_eq!(report.class_counts.get("Lake"), Some(&1));
    }

    #[test]
    fn graduated_matches_numeric_ranges() {
        let renderer = Renderer::Graduated {
            attr: "Importance".into(),
            ranges: vec![
                (0.0, 10.0, marker(10)),
                (10.0, 20.0, marker(20)),
                (20.0, 30.0, marker(30)),
            ],
        };
        let features = vec![
            feature(1, &[("Importance", Value::Int(5))]),
            feature(2, &[("Importance", Value::Float(12.5))]),
            feature(3, &[("Importance", Value::Int(99))]),
            feature(4, &[("Importance", Value::Text("high".into()))]),
        ];
        let report = apply_style_report(&renderer, &features);
        assert_eq!(report.styled.len(), 2);
        assert_eq!(report.styled[0].1, marker(10));
        assert_eq!(report.styled[1].1, marker(20));
        assert_eq!(report.unmatched, vec![3, 4]);
    }

    #[test]
    fn integer_and_float_values_match_the_same_class() {
        let features = vec![
            feature(1, &[("Name", Value::Int(1))]),
            feature(2, &[("Name", Value::Float(1.0))]),
            feature(3, &[("Name", Value::Float(1.5))]),
            feature(4, &[("Name", Value::Null)]),
        ];
        // A class label of "1" must match both Int(1) and Float(1.0).
        let renderer = Renderer::Categorized {
            attr: "Name".into(),
            classes: vec![("1".into(), poly((7, 7, 7)))],
        };
        let styled = apply_style(&renderer, &features);
        assert_eq!(styled.len(), 2);
        assert_eq!(styled[0].0.id, 1);
        assert_eq!(styled[1].0.id, 2);
        assert_eq!(styled[0].1, poly((7, 7, 7)));
    }

    #[test]
    fn match_string_formatting_is_stable() {
        assert_eq!(value_to_match_string(&Value::Int(-3)), "-3");
        assert_eq!(value_to_match_string(&Value::Float(2.0)), "2");
        assert_eq!(value_to_match_string(&Value::Float(2.5)), "2.5");
        assert_eq!(value_to_match_string(&Value::Bool(true)), "true");
        assert_eq!(value_to_match_string(&Value::Null), "");
        assert_eq!(value_to_match_string(&Value::Text("a".into())), "a");
    }

    #[test]
    fn empty_input_produces_empty_output() {
        let report = apply_style_report(&categorized(), &[]);
        assert!(report.styled.is_empty());
        assert!(report.unmatched.is_empty());
        assert!(report.class_counts.is_empty());
    }

    #[test]
    fn applies_a_parsed_polys_qml_fixture() {
        let qml = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../styles/polys.qml"),
        )
        .expect("polys.qml fixture");
        let model = qgis_style::parse_qml(&qml).unwrap();
        let features = vec![
            feature(1, &[("Name", Value::Text("Dam".into()))]),
            feature(2, &[("Name", Value::Text("Lake".into()))]),
            feature(3, &[("Name", Value::Text("Forest".into()))]),
        ];
        let report = apply_style_report(&model.renderer, &features);
        assert_eq!(report.classified_by.as_deref(), Some("Name"));
        assert_eq!(report.styled.len(), 2);
        assert_eq!(report.class_counts.get("Dam"), Some(&1));
        assert_eq!(report.class_counts.get("Lake"), Some(&1));
        assert_eq!(report.unmatched, vec![3]);

        // The Dam class in polys.qml is blue (27,54,212).
        match &report.styled[0].1 {
            Symbol::Polygon { fill, .. } => assert_eq!(
                *fill,
                Color {
                    r: 27,
                    g: 54,
                    b: 212,
                    a: 255
                }
            ),
            other => panic!("expected polygon symbol, got {:?}", other),
        }

        // apply_model is the whole-model shorthand.
        assert_eq!(apply_model(&model, &features).len(), 2);
    }
}