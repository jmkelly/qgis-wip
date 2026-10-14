use qgis_geometry::Geometry;
use qgis_crs::SrsId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: i64,
    pub geometry: Option<Geometry>,
    pub attributes: BTreeMap<String, Value>,
}

impl Feature {
    pub fn new(id: i64, geometry: Option<Geometry>) -> Self {
        Self {
            id,
            geometry,
            attributes: BTreeMap::new(),
        }
    }
    pub fn attribute(&self, key: &str) -> Option<&Value> {
        self.attributes.get(key)
    }
    pub fn set_attribute(&mut self, key: impl Into<String>, value: Value) {
        self.attributes.insert(key.into(), value);
    }
    pub fn geometry_type(&self) -> &str {
        match &self.geometry {
            None => "None",
            Some(g) => match g {
                Geometry::Point(_) => "Point",
                Geometry::LineString(_) => "LineString",
                Geometry::Polygon(_) => "Polygon",
                Geometry::MultiPoint(_) => "MultiPoint",
                Geometry::MultiLineString(_) => "MultiLineString",
                Geometry::MultiPolygon(_) => "MultiPolygon",
                Geometry::GeometryCollection(_) => "GeometryCollection",
            },
        }
    }
    pub fn extent(&self) -> Option<(f64, f64, f64, f64)> {
        None
    }
}


#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    pub crs: SrsId,
    pub features: Vec<Feature>,
    pub visible: bool,
    pub opacity: f32,
    pub style_ref: Option<String>,
}

impl Layer {
    pub fn new(name: impl Into<String>, crs: SrsId) -> Self {
        Self {
            name: name.into(),
            crs,
            features: Vec::new(),
            visible: true,
            opacity: 1.0,
            style_ref: None,
        }
    }
    pub fn feature_count(&self) -> usize {
        self.features.len()
    }
    pub fn extent(&self) -> Option<(f64, f64, f64, f64)> {
        None
    }
    pub fn spatial_index(&self) -> Vec<usize> {
        (0..self.features.len()).collect()
    }
}

#[derive(Debug, Default)]
pub struct LayerRegistry {
    layers: Vec<Layer>,
}

impl LayerRegistry {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }
    pub fn add_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }
    pub fn remove_layer(&mut self, index: usize) -> Option<Layer> {
        if index < self.layers.len() {
            Some(self.layers.remove(index))
        } else {
            None
        }
    }
    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from >= self.layers.len() || to > self.layers.len() {
            return;
        }
        let l = self.layers.remove(from);
        let to = to.min(self.layers.len());
        self.layers.insert(to, l);
    }
    pub fn layer_at(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }
    pub fn layer_at_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }
    pub fn iter_visible(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().filter(|l| l.visible)
    }
    pub fn len(&self) -> usize {
        self.layers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
}
