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

/// Ordered collection of [`Layer`]s backing the layer panel and map canvas.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LayerRegistry {
    layers: Vec<Layer>,
}

impl LayerRegistry {
    /// Creates an empty `LayerRegistry`.
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Appends a [`Layer`] to the end of the registry.
    pub fn add_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Removes and returns the [`Layer`] at `index`, or `None` if out of bounds.
    pub fn remove_layer(&mut self, index: usize) -> Option<Layer> {
        if index < self.layers.len() {
            Some(self.layers.remove(index))
        } else {
            None
        }
    }

    /// Moves the [`Layer`] at `from` to index `to` (clamped to the post-removal length).
    ///
    /// Does nothing if `from >= self.len()` or `to > self.len()`.
    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from >= self.layers.len() || to > self.layers.len() {
            return;
        }
        let l = self.layers.remove(from);
        let to = to.min(self.layers.len());
        self.layers.insert(to, l);
    }

    /// Returns a reference to the [`Layer`] at `index`, or `None` if out of bounds.
    pub fn layer_at(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    /// Returns a mutable reference to the [`Layer`] at `index`, or `None` if out of bounds.
    pub fn layer_at_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    /// Returns an iterator over all visible layers in registry order.
    pub fn iter_visible(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().filter(|l| l.visible)
    }

    /// Returns an iterator over all layers in registry order.
    pub fn iter(&self) -> std::slice::Iter<'_, Layer> {
        self.layers.iter()
    }

    /// Returns a mutable iterator over all layers in registry order.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Layer> {
        self.layers.iter_mut()
    }

    /// Returns the number of layers in the registry.
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Returns `true` if the registry contains no layers.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Returns a slice of all layers in registry order.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Returns a mutable slice of all layers in registry order.
    pub fn layers_mut(&mut self) -> &mut [Layer] {
        &mut self.layers
    }

    /// Removes all layers from the registry.
    pub fn clear(&mut self) {
        self.layers.clear();
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

    #[test]
    fn layer_registry_add_remove_access() {
        let crs = SrsId::from_epsg(4326);
        let mut reg = LayerRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.layer_at(0).is_none());
        assert!(reg.remove_layer(0).is_none());

        reg.add_layer(Layer::new("a", crs));
        reg.add_layer(Layer::new("b", crs));
        reg.add_layer(Layer::new("c", crs));

        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 3);
        assert_eq!(reg.layer_at(0).unwrap().name, "a");
        assert_eq!(reg.layer_at(1).unwrap().name, "b");
        assert_eq!(reg.layer_at(2).unwrap().name, "c");
        assert!(reg.layer_at(3).is_none());

        let removed = reg.remove_layer(1).unwrap();
        assert_eq!(removed.name, "b");
        assert_eq!(reg.len(), 2);
        assert_eq!(
            reg.layers().iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "c"]
        );
        assert!(reg.remove_layer(5).is_none());
    }

    #[test]
    fn layer_registry_move_and_reorder() {
        let crs = SrsId::from_epsg(4326);
        let mut reg = LayerRegistry::new();
        reg.add_layer(Layer::new("a", crs));
        reg.add_layer(Layer::new("b", crs));
        reg.add_layer(Layer::new("c", crs));

        // Move first to last
        reg.move_layer(0, 2);
        assert_eq!(
            reg.layers().iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            vec!["b", "c", "a"]
        );

        // Move last to first
        reg.move_layer(2, 0);
        assert_eq!(
            reg.layers().iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );

        // Out-of-bounds move is a no-op
        reg.move_layer(10, 0);
        reg.move_layer(0, 10);
        assert_eq!(
            reg.layers().iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn layer_registry_iter_visible() {
        let crs = SrsId::from_epsg(4326);
        let mut reg = LayerRegistry::new();
        reg.add_layer(Layer::new("a", crs));
        reg.add_layer(Layer::new("b", crs));
        reg.add_layer(Layer::new("c", crs));

        reg.layer_at_mut(1).unwrap().visible = false;

        let visible_names: Vec<&str> = reg.iter_visible().map(|l| l.name.as_str()).collect();
        assert_eq!(visible_names, vec!["a", "c"]);
    }

    #[test]
    fn feature_extent_and_bounding_box_all_geometry_variants() {
        use qgis_geometry::{
            GeometryCollection, LineString, MultiLineString, MultiPoint, MultiPolygon, Point,
            Polygon,
        };

        // None geometry
        let mut f_none = Feature::new(0, None);
        assert_eq!(f_none.geometry_type(), "None");
        assert_eq!(f_none.extent(), None);
        assert_eq!(f_none.bounding_box(), None);
        f_none.set_attribute("name", Value::Text("empty".into()));
        assert_eq!(f_none.attribute("name"), Some(&Value::Text("empty".into())));
        assert_eq!(f_none.attribute("missing"), None);

        // Point
        let f_pt = Feature::new(1, Some(Geometry::Point(Point::new(-3.5, 7.25))));
        assert_eq!(f_pt.geometry_type(), "Point");
        assert_eq!(f_pt.extent(), Some((-3.5, 7.25, -3.5, 7.25)));
        assert_eq!(f_pt.bounding_box(), Some((-3.5, 7.25, -3.5, 7.25)));

        // LineString (non-empty and empty)
        let ls = LineString::new(vec![
            Point::new(1.0, 5.0),
            Point::new(-2.0, 3.0),
            Point::new(4.0, -1.0),
        ]);
        let f_ls = Feature::new(2, Some(Geometry::LineString(ls)));
        assert_eq!(f_ls.geometry_type(), "LineString");
        assert_eq!(f_ls.extent(), Some((-2.0, -1.0, 4.0, 5.0)));
        assert_eq!(f_ls.bounding_box(), Some((-2.0, -1.0, 4.0, 5.0)));

        let f_ls_empty = Feature::new(3, Some(Geometry::LineString(LineString::new(vec![]))));
        assert_eq!(f_ls_empty.extent(), None);
        assert_eq!(f_ls_empty.bounding_box(), None);

        // Polygon (with exterior + interior ring, and empty)
        let poly = Polygon::with_holes(
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 8.0),
                Point::new(0.0, 8.0),
                Point::new(0.0, 0.0),
            ],
            vec![vec![
                Point::new(2.0, 2.0),
                Point::new(4.0, 2.0),
                Point::new(4.0, 4.0),
                Point::new(2.0, 2.0),
            ]],
        );
        let f_poly = Feature::new(4, Some(Geometry::Polygon(poly)));
        assert_eq!(f_poly.geometry_type(), "Polygon");
        assert_eq!(f_poly.extent(), Some((0.0, 0.0, 10.0, 8.0)));
        assert_eq!(f_poly.bounding_box(), Some((0.0, 0.0, 10.0, 8.0)));

        let f_poly_empty = Feature::new(5, Some(Geometry::Polygon(Polygon::new(vec![]))));
        assert_eq!(f_poly_empty.extent(), None);
        assert_eq!(f_poly_empty.bounding_box(), None);

        // MultiPoint (non-empty and empty)
        let mp = MultiPoint::new(vec![Point::new(-5.0, 2.0), Point::new(3.0, -4.0)]);
        let f_mp = Feature::new(6, Some(Geometry::MultiPoint(mp)));
        assert_eq!(f_mp.geometry_type(), "MultiPoint");
        assert_eq!(f_mp.extent(), Some((-5.0, -4.0, 3.0, 2.0)));
        assert_eq!(f_mp.bounding_box(), Some((-5.0, -4.0, 3.0, 2.0)));

        let f_mp_empty = Feature::new(7, Some(Geometry::MultiPoint(MultiPoint::new(vec![]))));
        assert_eq!(f_mp_empty.extent(), None);
        assert_eq!(f_mp_empty.bounding_box(), None);

        // MultiLineString (non-empty and empty)
        let mls = MultiLineString::new(vec![
            LineString::new(vec![Point::new(0.0, 1.0), Point::new(2.0, 3.0)]),
            LineString::new(vec![Point::new(-4.0, 6.0), Point::new(1.0, -2.0)]),
        ]);
        let f_mls = Feature::new(8, Some(Geometry::MultiLineString(mls)));
        assert_eq!(f_mls.geometry_type(), "MultiLineString");
        assert_eq!(f_mls.extent(), Some((-4.0, -2.0, 2.0, 6.0)));
        assert_eq!(f_mls.bounding_box(), Some((-4.0, -2.0, 2.0, 6.0)));

        let f_mls_empty = Feature::new(
            9,
            Some(Geometry::MultiLineString(MultiLineString::new(vec![]))),
        );
        assert_eq!(f_mls_empty.extent(), None);
        assert_eq!(f_mls_empty.bounding_box(), None);

        // MultiPolygon (non-empty and empty)
        let mpoly = MultiPolygon::new(vec![
            Polygon::new(vec![
                Point::new(1.0, 1.0),
                Point::new(3.0, 1.0),
                Point::new(3.0, 4.0),
                Point::new(1.0, 1.0),
            ]),
            Polygon::with_holes(
                vec![
                    Point::new(-6.0, -2.0),
                    Point::new(0.0, -2.0),
                    Point::new(0.0, 5.0),
                    Point::new(-6.0, -2.0),
                ],
                vec![vec![
                    Point::new(-3.0, -1.0),
                    Point::new(-1.0, -1.0),
                    Point::new(-1.0, 1.0),
                    Point::new(-3.0, -1.0),
                ]],
            ),
        ]);
        let f_mpoly = Feature::new(10, Some(Geometry::MultiPolygon(mpoly)));
        assert_eq!(f_mpoly.geometry_type(), "MultiPolygon");
        assert_eq!(f_mpoly.extent(), Some((-6.0, -2.0, 3.0, 5.0)));
        assert_eq!(f_mpoly.bounding_box(), Some((-6.0, -2.0, 3.0, 5.0)));

        let f_mpoly_empty =
            Feature::new(11, Some(Geometry::MultiPolygon(MultiPolygon::new(vec![]))));
        assert_eq!(f_mpoly_empty.extent(), None);
        assert_eq!(f_mpoly_empty.bounding_box(), None);

        // GeometryCollection (non-empty and empty)
        let gc = GeometryCollection::new(vec![
            Geometry::Point(Point::new(10.0, -10.0)),
            Geometry::LineString(LineString::new(vec![
                Point::new(-8.0, 2.0),
                Point::new(4.0, 12.0),
            ])),
        ]);
        let f_gc = Feature::new(12, Some(Geometry::GeometryCollection(gc)));
        assert_eq!(f_gc.geometry_type(), "GeometryCollection");
        assert_eq!(f_gc.extent(), Some((-8.0, -10.0, 10.0, 12.0)));
        assert_eq!(f_gc.bounding_box(), Some((-8.0, -10.0, 10.0, 12.0)));

        let f_gc_empty = Feature::new(
            13,
            Some(Geometry::GeometryCollection(GeometryCollection::new(
                vec![],
            ))),
        );
        assert_eq!(f_gc_empty.extent(), None);
        assert_eq!(f_gc_empty.bounding_box(), None);
    }
}
