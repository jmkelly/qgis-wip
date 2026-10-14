use roxmltree::Document;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn from_qgis(s: &str) -> Self {
        let parts: Vec<&str> = s.split(',').collect();
        let get = |i: usize| -> u8 { parts.get(i).and_then(|x| x.trim().parse().ok()).unwrap_or(255) };
        Color { r: get(0), g: get(1), b: get(2), a: get(3) }
    }
}

#[derive(Debug, Clone)]
pub enum Symbol {
    Point { shape: String, size: f64, color: Color, outline: Color },
    Line { color: Color, width: f64 },
    Polygon { fill: Color, outline: Color, outline_width: f64, style: String },
}

#[derive(Debug, Clone)]
pub enum Renderer {
    Single(Symbol),
    Categorized { attr: String, classes: Vec<(String, Symbol)> },
    Graduated { attr: String, ranges: Vec<(f64, f64, Symbol)> },
}

#[derive(Debug, Clone)]
pub struct StyleModel {
    pub renderer: Renderer,
}

fn parse_color_prop(props: &std::collections::BTreeMap<String, String>, key: &str, default: Color) -> Color {
    match props.get(key) {
        Some(v) => Color::from_qgis(v),
        None => default,
    }
}

fn props_of(symbol: &roxmltree::Node) -> (String, std::collections::BTreeMap<String, String>) {
    let mut stype = String::new();
    if let Some(t) = symbol.attribute("type") { stype = t.to_string(); }
    let mut props = std::collections::BTreeMap::new();
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
            shape: props.get("shape").cloned().unwrap_or_else(|| "circle".into()),
            size: props.get("size").and_then(|v| v.parse().ok()).unwrap_or(4.0),
            color: parse_color_prop(&props, "color", Color { r: 0, g: 0, b: 0, a: 255 }),
            outline: parse_color_prop(&props, "outline_color", Color { r: 0, g: 0, b: 0, a: 255 }),
        },
        "line" => Symbol::Line {
            color: parse_color_prop(&props, "line_color", Color { r: 0, g: 0, b: 0, a: 255 }),
            width: props.get("line_width").and_then(|v| v.parse().ok()).unwrap_or(0.5),
        },
        _ => Symbol::Polygon {
            fill: parse_color_prop(&props, "color", Color { r: 200, g: 200, b: 200, a: 255 }),
            outline: parse_color_prop(&props, "outline_color", Color { r: 0, g: 0, b: 0, a: 255 }),
            outline_width: props.get("outline_width").and_then(|v| v.parse().ok()).unwrap_or(0.3),
            style: props.get("style").cloned().unwrap_or_else(|| "solid".into()),
        },
    }
}

pub fn parse_qml(input: &str) -> Result<StyleModel, String> {
    let start = input.find("<qgis").unwrap_or(0);
    let doc = Document::parse(&input[start..]).map_err(|e| e.to_string())?;
    let renderer_node = match doc.descendants().find(|n| n.has_tag_name("renderer-v2")) {
        Some(n) => n,
        None => return Ok(StyleModel { renderer: Renderer::Single(Symbol::Polygon { fill: Color { r:200,g:200,b:200,a:255}, outline: Color{r:0,g:0,b:0,a:255}, outline_width:0.3, style:"solid".into() }) }),
    };
    let rtype = renderer_node.attribute("type").unwrap_or("");
    let attr = renderer_node.attribute("attr").unwrap_or("").to_string();

    // symbols map name->Symbol
    let mut symbols: std::collections::BTreeMap<String, Symbol> = Default::default();
    if let Some(syms) = renderer_node.children().find(|n| n.has_tag_name("symbols")) {
        for sym in syms.children().filter(|n| n.has_tag_name("symbol")) {
            let name = sym.attribute("name").unwrap_or("").to_string();
            symbols.insert(name, parse_symbol(&sym));
        }
    }

    let renderer = match rtype {
        "singleSymbol" => {
            // find single symbol element
            let sym_node = renderer_node.children().find(|n| n.has_tag_name("symbol"))
                .or_else(|| symbols.keys().next().map(|_| renderer_node)); // fallback
            let symbol = if let Some(sn) = renderer_node.descendants().find(|n| n.has_tag_name("symbol")) {
                parse_symbol(&sn)
            } else {
                Symbol::Polygon { fill: Color { r:200,g:200,b:200,a:255}, outline: Color{r:0,g:0,b:0,a:255}, outline_width:0.3, style:"solid".into() }
            };
            Renderer::Single(symbol)
        }
        "categorizedSymbol" => {
            let mut classes = Vec::new();
            if let Some(cats) = renderer_node.children().find(|n| n.has_tag_name("categories")) {
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
            // categories may be ranges
            if let Some(cats) = renderer_node.children().find(|n| n.has_tag_name("categories")) {
                for cat in cats.children().filter(|n| n.has_tag_name("category")) {
                    let lower = cat.attribute("lower").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let upper = cat.attribute("upper").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let symname = cat.attribute("symbol").unwrap_or("").to_string();
                    if let Some(sym) = symbols.get(&symname) {
                        ranges.push((lower, upper, sym.clone()));
                    }
                }
            }
            Renderer::Graduated { attr, ranges }
        }
        _ => {
            let symbol = renderer_node.descendants().find(|n| n.has_tag_name("symbol"))
                .map(|sn| parse_symbol(&sn))
                .unwrap_or(Symbol::Polygon { fill: Color { r:200,g:200,b:200,a:255}, outline: Color{r:0,g:0,b:0,a:255}, outline_width:0.3, style:"solid".into() });
            Renderer::Single(symbol)
        }
    };

    Ok(StyleModel { renderer })
}

/// Classify a simple feature attribute map into a symbol.
pub fn classify(renderer: &Renderer, attrs: &std::collections::BTreeMap<String, String>) -> Option<Symbol> {
    match renderer {
        Renderer::Single(s) => Some(s.clone()),
        Renderer::Categorized { attr, classes } => {
            let val = attrs.get(attr)?;
            classes.iter().find(|(v, _)| v == val).map(|(_, s)| s.clone())
        }
        Renderer::Graduated { attr, ranges } => {
            let val: f64 = attrs.get(attr)?.parse().ok()?;
            ranges.iter().find(|(lo, hi, _)| val >= *lo && val < *hi).map(|(_, _, s)| s.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_all_fixtures() {
        let files = std::fs::read_dir("/home/james/.paseo/worktrees/1a7z1cik/qgis-1k3/styles").unwrap();
        for entry in files {
            let p = entry.unwrap().path();
            if p.extension().map(|e| e=="qml").unwrap_or(false) {
                let s = std::fs::read_to_string(&p).unwrap();
                let m = parse_qml(&s).expect(&format!("{:?}", p));
                // basic sanity
                let _ = m;
            }
        }
    }
}