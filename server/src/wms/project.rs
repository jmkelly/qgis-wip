//! Minimal .qgs project file parser (pure Rust, no dependencies).
//!
//! Parses QGIS project XML files to extract:
//! - Project CRS
//! - Layer tree (ordered layer names and visibility)
//! - Project layers (data sources, geometry types, rendering styles)
//!
//! This is a constrained parser for the .qgs XML subset used by the QGIS
//! Rust port test suite. It extracts only the information needed for WMS
//! GetMap, GetFeatureInfo, and GetCapabilities.
//!
//! Reference: QGIS project file format (`QgsProject::read`)

use qgis_render::Pixel;
use std::collections::HashMap;

use super::get_map::LayerStyle;

/// A parsed QGIS project.
#[derive(Debug, Clone)]
pub struct Project {
    /// Project name.
    pub name: String,
    /// Project CRS identifier (e.g., "EPSG:4326").
    pub crs: Option<String>,
    /// Layers in tree order.
    pub layers: Vec<ProjectLayer>,
    /// Layer styles by layer name.
    pub styles: HashMap<String, LayerStyle>,
}

/// A layer in the project.
#[derive(Debug, Clone)]
pub struct ProjectLayer {
    /// Layer ID (unique within the project).
    pub id: String,
    /// Layer display name.
    pub name: String,
    /// Data source path.
    pub datasource: String,
    /// Provider type (e.g., "ogr", "geojson").
    pub provider: String,
    /// Geometry type ("vector", "raster").
    pub layer_type: String,
    /// Geometry type for vector layers (e.g., "Point", "Polygon").
    pub geometry_type: Option<String>,
    /// Whether the layer is visible (checked in the layer tree).
    pub visible: bool,
    /// Layer CRS identifier.
    pub crs: Option<String>,
}

/// Errors that can occur while parsing a project file.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectError {
    /// The file could not be read.
    Io(String),
    /// The XML is invalid.
    InvalidXml(String),
    /// The file is not a QGIS project.
    NotAProject,
    /// A required element is missing.
    MissingElement(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(s) => write!(f, "I/O error: {s}"),
            Self::InvalidXml(s) => write!(f, "invalid XML: {s}"),
            Self::NotAProject => write!(f, "not a QGIS project"),
            Self::MissingElement(s) => write!(f, "missing element: {s}"),
        }
    }
}

impl std::error::Error for ProjectError {}

/// Parses a QGIS project file from disk.
pub fn load_project(path: &str) -> Result<Project, ProjectError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| ProjectError::Io(e.to_string()))?;
    parse_project(&text)
}

/// Parses a QGIS project from XML text.
pub fn parse_project(text: &str) -> Result<Project, ProjectError> {
    let mut parser = XmlParser::new(text);
    parser.parse()?;

    let root = parser.root()?;
    if root.name != "qgis" {
        return Err(ProjectError::NotAProject);
    }

    let name = root
        .attribute("projectname")
        .unwrap_or("")
        .to_string();

    let mut project = Project {
        name,
        crs: None,
        layers: Vec::new(),
        styles: HashMap::new(),
    };

    // Parse project CRS from properties
    for child in &root.children {
            if child.name == "properties" {
                for prop in &child.children {
                    if prop.name == "ProjectCrs" {
                        project.crs = Some(prop.text.clone());
                    }
                    for subprop in &prop.children {
                        if subprop.name == "ProjectCrs" {
                            project.crs = Some(subprop.text.clone());
                        }
                    }
                }
            }
        }

    // Parse layer tree for visibility and order
    let mut layer_tree: Vec<(String, bool)> = Vec::new();
    for child in &root.children {
        if child.name == "layer-tree-group" {
            for tree_layer in &child.children {
                if tree_layer.name == "layer-tree-layer" {
                    let id = tree_layer.attribute("id").unwrap_or("").to_string();
                    let checked = tree_layer
                        .attribute("checked")
                        .unwrap_or("Qt::Checked");
                    let visible = checked == "Qt::Checked";
                    layer_tree.push((id, visible));
                }
            }
        }
    }

    // Parse project layers
    for child in &root.children {
        if child.name == "projectlayers" {
            for map_layer in &child.children {
                if map_layer.name != "maplayer" {
                    continue;
                }
                let layer = parse_map_layer(map_layer);
                project.layers.push(layer);
            }
        }
    }

    // Parse layer tree for visibility
    for (id, visible) in &layer_tree {
        if let Some(layer) = project.layers.iter_mut().find(|l| &l.id == id) {
            layer.visible = *visible;
        }
    }

    // Parse rendering styles from renderer-v2 elements
    for child in &root.children {
        if child.name == "projectlayers" {
            for map_layer in &child.children {
                if map_layer.name != "maplayer" {
                    continue;
                }
                let layer_name = get_child_text(map_layer, "layername");
                if let Some(layer_name) = layer_name {
                    if let Some(style) = parse_layer_style(map_layer) {
                        project.styles.insert(layer_name, style);
                    }
                }
            }
        }
    }

    Ok(project)
}

/// Parses a single `<maplayer>` element.
fn parse_map_layer(element: &XmlElement) -> ProjectLayer {
    ProjectLayer {
        id: get_child_text(element, "id").unwrap_or_default(),
        name: get_child_text(element, "layername").unwrap_or_default(),
        datasource: get_child_text(element, "datasource").unwrap_or_default(),
        provider: get_child_text(element, "provider").unwrap_or_default(),
        layer_type: get_child_text(element, "type").unwrap_or_default(),
        // QGIS writes geometry="Unknown" for a layer with no features and
        // geometry="NoGeometry" for an attribute-only layer; both mean "no
        // geometry type" and must parse back as None. Mirrors the gui reader
        // (gui/src/project.rs read_qgs).
        geometry_type: element
            .attribute("geometry")
            .map(|s| s.to_string())
            .filter(|g| g != "Unknown" && g != "NoGeometry"),
        visible: true,
        crs: get_layer_crs(element),
    }
}

/// Gets the CRS of a layer from its SRS element.
fn get_layer_crs(element: &XmlElement) -> Option<String> {
    for child in &element.children {
        if child.name == "srs" {
            for srs_child in &child.children {
                if srs_child.name == "spatialrefsys" {
                    return get_child_text(srs_child, "authid");
                }
            }
        }
    }
    None
}

/// Gets the text content of a child element.
fn get_child_text(element: &XmlElement, name: &str) -> Option<String> {
    for child in &element.children {
        if child.name == name {
            return Some(child.text.clone());
        }
    }
    None
}

/// Parses a layer's rendering style from its renderer-v2 element.
fn parse_layer_style(element: &XmlElement) -> Option<LayerStyle> {
    let mut style = LayerStyle::default();

    for child in &element.children {
        if child.name == "renderer-v2" {
            for symbols_child in &child.children {
                if symbols_child.name == "symbols" {
                    for symbol in &symbols_child.children {
                        if symbol.name == "symbol" {
                            for layer_elem in &symbol.children {
                                if layer_elem.name == "layer" {
                                    for prop in &layer_elem.children {
                                        if prop.name == "prop" {
                                            let key = prop.attribute("k").unwrap_or("");
                                            let value =
                                                prop.attribute("v").unwrap_or("");
                                            match key {
                                                "color" => {
                                                    style.fill_color = parse_qgis_color(value);
                                                }
                                                "outline_color" => {
                                                    style.stroke_color =
                                                        parse_qgis_color(value);
                                                }
                                                "outline_width" => {
                                                    style.stroke_width =
                                                        value.parse().unwrap_or(1);
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Some(style)
}

/// Parses a QGIS color string "r,g,b,a" into a Pixel.
fn parse_qgis_color(s: &str) -> Option<Pixel> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let r = parts[0].trim().parse::<u8>().ok()?;
    let g = parts[1].trim().parse::<u8>().ok()?;
    let b = parts[2].trim().parse::<u8>().ok()?;
    let a = if parts.len() >= 4 {
        parts[3].trim().parse::<u8>().ok()?
    } else {
        255
    };
    Some(Pixel::new(r, g, b, a))
}

// ── Minimal XML parser ─────────────────────────────────────────────────────

/// A parsed XML element.
#[derive(Debug, Clone)]
struct XmlElement {
    /// Element name.
    name: String,
    /// Attributes (key, value).
    attributes: Vec<(String, String)>,
    /// Child elements.
    children: Vec<XmlElement>,
    /// Text content.
    text: String,
}

impl XmlElement {
    /// Returns the value of an attribute.
    fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// A minimal XML parser for .qgs project files.
struct XmlParser<'a> {
    text: &'a [u8],
    pos: usize,
    root: Option<XmlElement>,
}

impl<'a> XmlParser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            pos: 0,
            root: None,
        }
    }

    fn parse(&mut self) -> Result<(), ProjectError> {
        self.skip_ws();
        // Skip XML declaration and doctype
        while self.peek() == Some(b'<') {
            if self.text[self.pos..].starts_with(b"<?") || self.text[self.pos..].starts_with(b"<!")
            {
                self.skip_until(b'>')?;
            } else {
                break;
            }
            self.skip_ws();
        }
        let root = self.parse_element()?;
        self.root = Some(root);
        Ok(())
    }

    fn root(&self) -> Result<&XmlElement, ProjectError> {
        self.root
            .as_ref()
            .ok_or_else(|| ProjectError::MissingElement("root".to_string()))
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn skip_until(&mut self, target: u8) -> Result<(), ProjectError> {
        while let Some(c) = self.peek() {
            if c == target {
                self.pos += 1;
                return Ok(());
            }
            self.pos += 1;
        }
        Err(ProjectError::InvalidXml(format!(
            "unterminated construct, expected '{}'",
            target as char
        )))
    }

    fn parse_element(&mut self) -> Result<XmlElement, ProjectError> {
        if self.peek() != Some(b'<') {
            return Err(ProjectError::InvalidXml(format!(
                "expected '<' at byte {}",
                self.pos
            )));
        }
        self.pos += 1; // skip '<'

        let name = self.parse_name()?;
        let mut attributes = Vec::new();

        // Parse attributes
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'/') => {
                    self.pos += 1; // skip '/'
                    if self.peek() != Some(b'>') {
                        return Err(ProjectError::InvalidXml(format!(
                            "expected '>' after '/' at byte {}",
                            self.pos
                        )));
                    }
                    self.pos += 1; // skip '>'
                    return Ok(XmlElement {
                        name,
                        attributes,
                        children: Vec::new(),
                        text: String::new(),
                    });
                }
                Some(b'>') => {
                    self.pos += 1; // skip '>'
                    break;
                }
                Some(_) => {
                    let (key, value) = self.parse_attribute()?;
                    attributes.push((key, value));
                }
                None => {
                    return Err(ProjectError::InvalidXml(format!(
                        "unterminated element '<{name}' at byte {}",
                        self.pos
                    )))
                }
            }
        }

        // Parse content (children and text)
        let mut children = Vec::new();
        let mut text = String::new();

        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'<') => {
                    if self.text[self.pos..].starts_with(b"</") {
                        // Closing tag
                        self.pos += 2; // skip '</'
                        let closing_name = self.parse_name()?;
                        if closing_name != name {
                            return Err(ProjectError::InvalidXml(format!(
                                "mismatched closing tag: expected '</{name}', got '</{closing_name}' at byte {}",
                                self.pos
                            )));
                        }
                        self.skip_ws();
                        if self.peek() != Some(b'>') {
                            return Err(ProjectError::InvalidXml(format!(
                                "expected '>' at byte {}",
                                self.pos
                            )));
                        }
                        self.pos += 1; // skip '>'
                        break;
                    } else {
                        // Child element
                        let child = self.parse_element()?;
                        children.push(child);
                    }
                }
                None => {
                    return Err(ProjectError::InvalidXml(format!(
                        "unterminated element '<{name}' at byte {}",
                        self.pos
                    )))
                }
                _ => {
                    // Text content
                    let start = self.pos;
                    while let Some(c) = self.peek() {
                        if c == b'<' {
                            break;
                        }
                        self.pos += 1;
                    }
                    let chunk = std::str::from_utf8(&self.text[start..self.pos])
                        .map_err(|e| ProjectError::InvalidXml(e.to_string()))?;
                    text.push_str(chunk);
                }
            }
        }

        Ok(XmlElement {
            name,
            attributes,
            children,
            text,
        })
    }

    fn parse_name(&mut self) -> Result<String, ProjectError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.' || c == b':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(ProjectError::InvalidXml(format!(
                "expected name at byte {}",
                self.pos
            )));
        }
        Ok(std::str::from_utf8(&self.text[start..self.pos])
            .map_err(|e| ProjectError::InvalidXml(e.to_string()))?
            .to_string())
    }

    fn parse_attribute(&mut self) -> Result<(String, String), ProjectError> {
        let key = self.parse_name()?;
        self.skip_ws();
        if self.peek() != Some(b'=') {
            return Err(ProjectError::InvalidXml(format!(
                "expected '=' at byte {}",
                self.pos
            )));
        }
        self.pos += 1; // skip '='
        self.skip_ws();
        let quote = self.peek().ok_or_else(|| {
            ProjectError::InvalidXml(format!("expected quote at byte {}", self.pos))
        })?;
        if quote != b'"' && quote != b'\'' {
            return Err(ProjectError::InvalidXml(format!(
                "expected quote at byte {}",
                self.pos
            )));
        }
        self.pos += 1; // skip opening quote
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == quote {
                break;
            }
            self.pos += 1;
        }
        let value = std::str::from_utf8(&self.text[start..self.pos])
            .map_err(|e| ProjectError::InvalidXml(e.to_string()))?
            .to_string();
        self.pos += 1; // skip closing quote
        Ok((key, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_project() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE qgis PUBLIC 'http://mrcc.com/qgis.dtd' 'SYSTEM'>
<qgis projectname="Test Project" version="3.28.0">
  <projectlayers>
    <maplayer id="layer1" geometry="Point" type="vector">
      <id>layer1</id>
      <layername>Points</layername>
      <datasource>./points.geojson</datasource>
      <provider>geojson</provider>
      <type>vector</type>
      <srs>
        <spatialrefsys>
          <authid>EPSG:4326</authid>
        </spatialrefsys>
      </srs>
    </maplayer>
  </projectlayers>
</qgis>"#;
        let project = parse_project(xml).unwrap();
        assert_eq!(project.name, "Test Project");
        assert_eq!(project.layers.len(), 1);
        assert_eq!(project.layers[0].name, "Points");
        assert_eq!(project.layers[0].datasource, "./points.geojson");
        assert_eq!(project.layers[0].provider, "geojson");
        assert_eq!(project.layers[0].geometry_type.as_deref(), Some("Point"));
        assert!(project.layers[0].visible);
        assert_eq!(project.layers[0].crs.as_deref(), Some("EPSG:4326"));
    }

    #[test]
    fn parse_layer_without_geometry_is_none() {
        // QGIS writes geometry="Unknown" for a layer with no features and
        // geometry="NoGeometry" for an attribute-only layer. Both mean "no
        // geometry type" and must read back as None, matching the gui reader
        // (gui/src/project.rs read_qgs).
        let xml = r#"<qgis projectname="Test" version="3.28.0">
  <projectlayers>
    <maplayer id="layer1" geometry="Unknown" type="vector">
      <id>layer1</id>
      <layername>Empty</layername>
      <datasource>./empty.geojson</datasource>
      <provider>geojson</provider>
      <type>vector</type>
    </maplayer>
    <maplayer id="layer2" geometry="NoGeometry" type="vector">
      <id>layer2</id>
      <layername>AttributesOnly</layername>
      <datasource>./attrs.csv</datasource>
      <provider>delimitedtext</provider>
      <type>vector</type>
    </maplayer>
  </projectlayers>
</qgis>"#;
        let project = parse_project(xml).unwrap();
        assert_eq!(project.layers.len(), 2);
        assert_eq!(project.layers[0].geometry_type, None);
        assert_eq!(project.layers[1].geometry_type, None);
    }

    #[test]
    fn parse_project_with_crs() {
        let xml = r#"<qgis projectname="Test" version="3.28.0">
  <properties>
    <SpatialRefSys>
      <ProjectCrs>EPSG:3857</ProjectCrs>
    </SpatialRefSys>
  </properties>
  <projectlayers>
  </projectlayers>
</qgis>"#;
        let project = parse_project(xml).unwrap();
        assert_eq!(project.crs.as_deref(), Some("EPSG:3857"));
    }

    #[test]
    fn parse_layer_tree_visibility() {
        let xml = r#"<qgis projectname="Test" version="3.28.0">
  <layer-tree-group>
    <layer-tree-layer id="layer1" name="Visible" checked="Qt::Checked"/>
    <layer-tree-layer id="layer2" name="Hidden" checked="Qt::Unchecked"/>
  </layer-tree-group>
  <projectlayers>
    <maplayer id="layer1" type="vector">
      <id>layer1</id>
      <layername>Visible</layername>
      <datasource>./a.geojson</datasource>
      <provider>geojson</provider>
      <type>vector</type>
    </maplayer>
    <maplayer id="layer2" type="vector">
      <id>layer2</id>
      <layername>Hidden</layername>
      <datasource>./b.geojson</datasource>
      <provider>geojson</provider>
      <type>vector</type>
    </maplayer>
  </projectlayers>
</qgis>"#;
        let project = parse_project(xml).unwrap();
        assert_eq!(project.layers.len(), 2);
        assert!(project.layers[0].visible);
        assert!(!project.layers[1].visible);
    }

    #[test]
    fn parse_layer_style() {
        let xml = r#"<qgis projectname="Test" version="3.28.0">
  <projectlayers>
    <maplayer id="layer1" type="vector">
      <id>layer1</id>
      <layername>Styled</layername>
      <datasource>./a.geojson</datasource>
      <provider>geojson</provider>
      <type>vector</type>
      <renderer-v2 type="singleSymbol">
        <symbols>
          <symbol type="fill">
            <layer class="SimpleFill">
              <prop k="color" v="255,0,0,255"/>
              <prop k="outline_color" v="0,0,0,255"/>
              <prop k="outline_width" v="2"/>
            </layer>
          </symbol>
        </symbols>
      </renderer-v2>
    </maplayer>
  </projectlayers>
</qgis>"#;
        let project = parse_project(xml).unwrap();
        let style = project.styles.get("Styled").unwrap();
        assert_eq!(style.fill_color, Some(Pixel::rgb(255, 0, 0)));
        assert_eq!(style.stroke_color, Some(Pixel::rgb(0, 0, 0)));
        assert_eq!(style.stroke_width, 2);
    }

    #[test]
    fn parse_not_a_project() {
        let xml = r#"<notqgis></notqgis>"#;
        let result = parse_project(xml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_xml() {
        let xml = r#"<qgis><unclosed></qgis>"#;
        let result = parse_project(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_qgis_color() {
        assert_eq!(parse_qgis_color("255,0,0,255"), Some(Pixel::rgb(255, 0, 0)));
        assert_eq!(parse_qgis_color("0,0,0,128"), Some(Pixel::new(0, 0, 0, 128)));
        assert_eq!(parse_qgis_color("255,0,0"), Some(Pixel::rgb(255, 0, 0)));
        assert_eq!(parse_qgis_color("invalid"), None);
        assert_eq!(parse_qgis_color(""), None);
    }

    #[test]
    fn parse_real_project_file() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../qgs-projects/joins.qgs"
        );
        if std::path::Path::new(path).exists() {
            let project = load_project(path).unwrap();
            assert_eq!(project.layers.len(), 2);
            assert!(project.crs.is_some());
            assert!(!project.layers.is_empty());
            assert!(project.styles.contains_key("polys_with_id"));
        }
    }
}
