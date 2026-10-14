#![forbid(unsafe_code)]

use roxmltree::Document;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn from_qgis(s: &str) -> Self {
        let parts: Vec<&str> = s.split(',').collect();
        let get = |i: usize| -> u8 {
            parts
                .get(i)
                .and_then(|x| x.trim().parse().ok())
                .unwrap_or(255)
        };
        Color {
            r: get(0),
            g: get(1),
            b: get(2),
            a: get(3),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    Point {
        shape: String,
        size: f64,
        color: Color,
        outline: Color,
    },
    Line {
        color: Color,
        width: f64,
    },
    Polygon {
        fill: Color,
        outline: Color,
        outline_width: f64,
        style: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Renderer {
    Single(Symbol),
    Categorized {
        attr: String,
        classes: Vec<(String, Symbol)>,
    },
    Graduated {
        attr: String,
        ranges: Vec<(f64, f64, Symbol)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleModel {
    pub renderer: Renderer,
}

fn parse_color_prop(props: &BTreeMap<String, String>, keys: &[&str], default: Color) -> Color {
    for &key in keys {
        if let Some(v) = props.get(key) {
            return Color::from_qgis(v);
        }
    }
    default
}

fn parse_f64_prop(props: &BTreeMap<String, String>, keys: &[&str], default: f64) -> f64 {
    for &key in keys {
        if let Some(v) = props.get(key) {
            if let Ok(val) = v.trim().parse::<f64>() {
                return val;
            }
        }
    }
    default
}

fn parse_str_prop(props: &BTreeMap<String, String>, keys: &[&str], default: &str) -> String {
    for &key in keys {
        if let Some(v) = props.get(key) {
            return v.clone();
        }
    }
    default.to_string()
}

fn props_of(symbol: &roxmltree::Node) -> (String, BTreeMap<String, String>) {
    let stype = symbol.attribute("type").unwrap_or("").to_string();
    let mut props = BTreeMap::new();
    for layer in symbol.descendants().filter(|n| n.has_tag_name("layer")) {
        for prop in layer.children().filter(|n| n.has_tag_name("prop")) {
            if let (Some(k), Some(v)) = (prop.attribute("k"), prop.attribute("v")) {
                props.insert(k.to_string(), v.to_string());
            }
        }
    }
    (stype, props)
}

fn parse_symbol(symbol: &roxmltree::Node) -> Symbol {
    let (stype, props) = props_of(symbol);
    match stype.as_str() {
        "marker" => Symbol::Point {
            shape: parse_str_prop(&props, &["name", "shape"], "circle"),
            size: parse_f64_prop(&props, &["size"], 4.0),
            color: parse_color_prop(
                &props,
                &["color", "fill_color"],
                Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            ),
            outline: parse_color_prop(
                &props,
                &["outline_color", "color_border"],
                Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            ),
        },
        "line" => Symbol::Line {
            color: parse_color_prop(
                &props,
                &["line_color", "color"],
                Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            ),
            width: parse_f64_prop(&props, &["line_width", "width"], 0.5),
        },
        _ => Symbol::Polygon {
            fill: parse_color_prop(
                &props,
                &[
                    "color",
                    "color1",
                    "gradient_color",
                    "shapeburst_color",
                    "fill_color",
                ],
                Color {
                    r: 200,
                    g: 200,
                    b: 200,
                    a: 255,
                },
            ),
            outline: parse_color_prop(
                &props,
                &["outline_color", "color_border"],
                Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            ),
            outline_width: parse_f64_prop(&props, &["outline_width", "width_border"], 0.3),
            style: parse_str_prop(&props, &["style"], "solid"),
        },
    }
}

fn default_polygon_symbol() -> Symbol {
    Symbol::Polygon {
        fill: Color {
            r: 200,
            g: 200,
            b: 200,
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

pub fn parse_qml(input: &str) -> Result<StyleModel, String> {
    let start = input.find("<qgis").unwrap_or(0);
    let doc = Document::parse(&input[start..]).map_err(|e| e.to_string())?;
    let mut renderer_node = match doc.descendants().find(|n| n.has_tag_name("renderer-v2")) {
        Some(n) => n,
        None => {
            return Ok(StyleModel {
                renderer: Renderer::Single(default_polygon_symbol()),
            })
        }
    };

    // Unwrap nested renderer-v2 (e.g., invertedPolygonRenderer wrapping singleSymbol)
    while let Some(child_renderer) = renderer_node
        .children()
        .find(|n| n.has_tag_name("renderer-v2"))
    {
        renderer_node = child_renderer;
    }

    let rtype = renderer_node.attribute("type").unwrap_or("");
    let attr = renderer_node.attribute("attr").unwrap_or("").to_string();

    // symbols map name->Symbol
    let mut symbols: BTreeMap<String, Symbol> = Default::default();
    if let Some(syms) = renderer_node
        .children()
        .find(|n| n.has_tag_name("symbols"))
    {
        for sym in syms.children().filter(|n| n.has_tag_name("symbol")) {
            let name = sym.attribute("name").unwrap_or("").to_string();
            symbols.insert(name, parse_symbol(&sym));
        }
    }

    let renderer = match rtype {
        "singleSymbol" => {
            let symbol = symbols
                .values()
                .next()
                .cloned()
                .or_else(|| {
                    renderer_node
                        .descendants()
                        .find(|n| n.has_tag_name("symbol"))
                        .map(|sn| parse_symbol(&sn))
                })
                .unwrap_or_else(default_polygon_symbol);
            Renderer::Single(symbol)
        }
        "categorizedSymbol" => {
            let mut classes = Vec::new();
            if let Some(cats) = renderer_node
                .children()
                .find(|n| n.has_tag_name("categories"))
            {
                for cat in cats.children().filter(|n| n.has_tag_name("category")) {
                    let value = cat.attribute("value").unwrap_or("").to_string();
                    let symname = cat.attribute("symbol").unwrap_or("").to_string();
                    if let Some(sym) = symbols.get(&symname) {
                        classes.push((value, sym.clone()));
                    }
                }
            }
            Renderer::Categorized { attr, classes }
        }
        "graduatedSymbol" => {
            let mut ranges = Vec::new();
            if let Some(rngs) = renderer_node.children().find(|n| n.has_tag_name("ranges")) {
                for rng in rngs.children().filter(|n| n.has_tag_name("range")) {
                    let lower = rng
                        .attribute("lower")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.0);
                    let upper = rng
                        .attribute("upper")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.0);
                    let symname = rng.attribute("symbol").unwrap_or("").to_string();
                    if let Some(sym) = symbols.get(&symname) {
                        ranges.push((lower, upper, sym.clone()));
                    }
                }
            } else if let Some(cats) = renderer_node
                .children()
                .find(|n| n.has_tag_name("categories"))
            {
                for cat in cats.children().filter(|n| n.has_tag_name("category")) {
                    let lower = cat
                        .attribute("lower")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.0);
                    let upper = cat
                        .attribute("upper")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.0);
                    let symname = cat.attribute("symbol").unwrap_or("").to_string();
                    if let Some(sym) = symbols.get(&symname) {
                        ranges.push((lower, upper, sym.clone()));
                    }
                }
            }
            Renderer::Graduated { attr, ranges }
        }
        _ => {
            let symbol = symbols
                .values()
                .next()
                .cloned()
                .or_else(|| {
                    renderer_node
                        .descendants()
                        .find(|n| n.has_tag_name("symbol"))
                        .map(|sn| parse_symbol(&sn))
                })
                .unwrap_or_else(default_polygon_symbol);
            Renderer::Single(symbol)
        }
    };

    Ok(StyleModel { renderer })
}

pub fn parse_qml_file(path: impl AsRef<Path>) -> Result<StyleModel, String> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(|e| e.to_string())?;
    parse_qml(&content)
}

/// Classify a simple feature attribute map into a symbol.
pub fn classify(renderer: &Renderer, attrs: &BTreeMap<String, String>) -> Option<Symbol> {
    match renderer {
        Renderer::Single(s) => Some(s.clone()),
        Renderer::Categorized { attr, classes } => {
            let val = attrs.get(attr)?;
            classes
                .iter()
                .find(|(v, _)| v == val)
                .map(|(_, s)| s.clone())
        }
        Renderer::Graduated { attr, ranges } => {
            let val: f64 = attrs.get(attr)?.parse().ok()?;
            ranges
                .iter()
                .find(|(lo, hi, _)| val >= *lo && val <= *hi)
                .map(|(_, _, s)| s.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn styles_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../styles")
    }

    #[test]
    fn parse_all_fixtures() {
        let dir = styles_dir();
        let mut count = 0;
        let files = std::fs::read_dir(&dir).expect("read styles/ directory");
        for entry in files {
            let p = entry.unwrap().path();
            if p.extension().map(|e| e == "qml").unwrap_or(false) {
                let s = std::fs::read_to_string(&p).unwrap();
                let m = parse_qml(&s).unwrap_or_else(|e| panic!("failed to parse {:?}: {}", p, e));
                let _ = m;
                count += 1;
            }
        }
        assert_eq!(count, 11, "expected 11 .qml fixtures in styles/");
    }

    #[test]
    fn parse_polys_categorized() {
        let p = styles_dir().join("polys.qml");
        let model = parse_qml_file(&p).unwrap();
        match model.renderer {
            Renderer::Categorized { attr, classes } => {
                assert_eq!(attr, "Name");
                assert_eq!(classes.len(), 2);
                assert_eq!(classes[0].0, "Dam");
                assert_eq!(
                    classes[0].1,
                    Symbol::Polygon {
                        fill: Color {
                            r: 27,
                            g: 54,
                            b: 212,
                            a: 255
                        },
                        outline: Color {
                            r: 159,
                            g: 159,
                            b: 159,
                            a: 255
                        },
                        outline_width: 2.0,
                        style: "dense4".to_string(),
                    }
                );
                assert_eq!(classes[1].0, "Lake");
                assert_eq!(
                    classes[1].1,
                    Symbol::Polygon {
                        fill: Color {
                            r: 110,
                            g: 194,
                            b: 217,
                            a: 255
                        },
                        outline: Color {
                            r: 144,
                            g: 144,
                            b: 144,
                            a: 255
                        },
                        outline_width: 1.0,
                        style: "solid".to_string(),
                    }
                );
            }
            other => panic!("expected Categorized, got {:?}", other),
        }
    }

    #[test]
    fn parse_points_graduated() {
        let p = styles_dir().join("points_graduated_symbol.qml");
        let model = parse_qml_file(&p).unwrap();
        match model.renderer {
            Renderer::Graduated { attr, ranges } => {
                assert_eq!(attr, "Importance");
                assert_eq!(ranges.len(), 3);
                assert!((ranges[0].0 - 0.999).abs() < 1e-6);
                assert!((ranges[0].1 - 7.333).abs() < 1e-6);
                assert_eq!(
                    ranges[0].2,
                    Symbol::Point {
                        shape: "circle".to_string(),
                        size: 4.0,
                        color: Color {
                            r: 0,
                            g: 255,
                            b: 0,
                            a: 255
                        },
                        outline: Color {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255
                        },
                    }
                );
                assert_eq!(
                    ranges[1].2,
                    Symbol::Point {
                        shape: "square".to_string(),
                        size: 7.0,
                        color: Color {
                            r: 169,
                            g: 102,
                            b: 108,
                            a: 255
                        },
                        outline: Color {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255
                        },
                    }
                );
                assert_eq!(
                    ranges[2].2,
                    Symbol::Point {
                        shape: "star".to_string(),
                        size: 9.0,
                        color: Color {
                            r: 0,
                            g: 84,
                            b: 171,
                            a: 255
                        },
                        outline: Color {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255
                        },
                    }
                );
            }
            other => panic!("expected Graduated, got {:?}", other),
        }
    }

    #[test]
    fn parse_single_symbols() {
        let pt_model = parse_qml_file(styles_dir().join("points_single_symbol.qml")).unwrap();
        assert_eq!(
            pt_model.renderer,
            Renderer::Single(Symbol::Point {
                shape: "triangle".to_string(),
                size: 1.0,
                color: Color {
                    r: 193,
                    g: 145,
                    b: 122,
                    a: 255
                },
                outline: Color {
                    r: 140,
                    g: 82,
                    b: 60,
                    a: 255
                },
            })
        );

        let line_model = parse_qml_file(styles_dir().join("lines_single_symbol.qml")).unwrap();
        assert_eq!(
            line_model.renderer,
            Renderer::Single(Symbol::Line {
                color: Color {
                    r: 159,
                    g: 113,
                    b: 98,
                    a: 255
                },
                width: 0.8,
            })
        );

        let inv_poly = parse_qml_file(styles_dir().join("inverted_polys_single.qml")).unwrap();
        assert_eq!(
            inv_poly.renderer,
            Renderer::Single(Symbol::Polygon {
                fill: Color {
                    r: 76,
                    g: 143,
                    b: 160,
                    a: 255
                },
                outline: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255
                },
                outline_width: 0.26,
                style: "solid".to_string(),
            })
        );
    }
}