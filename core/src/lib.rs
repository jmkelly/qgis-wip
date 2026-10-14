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
        let geom = self.geometry.as_ref()?;
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut any = false;
        fn visit_point(p: &qgis_geometry::Point, min_x: &mut f64, min_y: &mut f64, max_x: &mut f64, max_y: &mut f64, any: &mut bool) {
            *min_x = min_x.min(p.x);
            *min_y = min_y.min(p.y);
            *max_x = max_x.max(p.x);
            *max_y = max_y.max(p.y);
            *any = true;
        }
        fn visit_geom(g: &qgis_geometry::Geometry, min_x: &mut f64, min_y: &mut f64, max_x: &mut f64, max_y: &mut f64, any: &mut bool) {
            use qgis_geometry::Geometry;
            match g {
                Geometry::Point(p) => visit_point(p, min_x, min_y, max_x, max_y, any),
                Geometry::LineString(ls) => {
                    for p in ls.points() {
                        visit_point(p, min_x, min_y, max_x, max_y, any);
                    }
                }
                Geometry::Polygon(poly) => {
                    for p in &poly.exterior {
                        visit_point(p, min_x, min_y, max_x, max_y, any);
                    }
                    for ring in &poly.interiors {
                        for p in ring {
                            visit_point(p, min_x, min_y, max_x, max_y, any);
                        }
                    }
                }
                Geometry::MultiPoint(mp) => {
                    for p in mp.points() {
                        visit_point(p, min_x, min_y, max_x, max_y, any);
                    }
                }
                Geometry::MultiLineString(mls) => {
                    for ls in mls.lines() {
                        for p in ls.points() {
                            visit_point(p, min_x, min_y, max_x, max_y, any);
                        }
                    }
                }
                Geometry::MultiPolygon(mp) => {
                    for poly in mp.polygons() {
                        for p in &poly.exterior {
                            visit_point(p, min_x, min_y, max_x, max_y, any);
                        }
                        for ring in &poly.interiors {
                            for p in ring {
                                visit_point(p, min_x, min_y, max_x, max_y, any);
                            }
                        }
                    }
                }
                Geometry::GeometryCollection(gc) => {
                    for g in gc.geometries() {
                        visit_geom(g, min_x, min_y, max_x, max_y, any);
                    }
                }
            }
        }
        visit_geom(geom, &mut min_x, &mut min_y, &mut max_x, &mut max_y, &mut any);
        if any {
            Some((min_x, min_y, max_x, max_y))
        } else {
            None
        }
    }
    pub fn bounding_box(&self) -> Option<(f64, f64, f64, f64)> {
        self.extent()
    }
}


#[derive(Debug, Clone, PartialEq)]
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
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut any = false;
        for f in &self.features {
            if let Some((fx0, fy0, fx1, fy1)) = f.extent() {
                min_x = min_x.min(fx0);
                min_y = min_y.min(fy0);
                max_x = max_x.max(fx1);
                max_y = max_y.max(fy1);
                any = true;
            }
        }
        if any {
            Some((min_x, min_y, max_x, max_y))
        } else {
            None
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::Geometry;

    #[test]
    fn layer_basics() {
        let crs = SrsId::from_epsg(4326);
        let mut layer = Layer::new("roads", crs);
        assert_eq!(layer.name, "roads");
        assert_eq!(layer.crs, crs);
        assert!(layer.visible);
        assert_eq!(layer.opacity, 1.0);
        assert!(layer.style_ref.is_none());
        assert_eq!(layer.feature_count(), 0);
        assert!(layer.extent().is_none());

        let f = Feature::new(1, Some(Geometry::Point(qgis_geometry::Point::new(1.0, 2.0))));
        layer.features.push(f.clone());
        assert_eq!(layer.feature_count(), 1);
        assert_eq!(layer.extent(), Some((1.0, 2.0, 1.0, 2.0)));
        assert_eq!(layer.spatial_index(), vec![0]);
    }
}
