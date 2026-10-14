use qgis_bench::{raster::{raster_reproject, raster_render, Raster}, render::{render_labeling, render_layer, render_polygon_fill}, index::{rtree_insert, rtree_query}, expression::{expression_evaluate, expression_parse, EvalContext, Value}, Crs, Extent, Feature, Layer, MapSettings, Point, Polygon};
use std::time::Instant;


fn make_layers() -> Vec<Layer> { vec![Layer::new("l", vec![Feature::new(0, Polygon::new(vec![Point::new(0.0,0.0),Point::new(1.0,0.0),Point::new(1.0,1.0),Point::new(0.0,1.0)]))])] }

#[test]
fn no_regression_render_polygon_fill() {
    let layers=make_layers();
    let t=Instant::now(); let _=render_polygon_fill(&layers,800,600); assert!(t.elapsed().as_nanos() <= 500_000_000);
}
#[test]
fn no_regression_render_layer() {
    let layers=make_layers(); let s=MapSettings::default();
    let t=Instant::now(); let _=render_layer(&layers,&s); assert!(t.elapsed().as_nanos() <= 500_000_000);
}
#[test]
fn no_regression_render_labeling() {
    let layers=make_layers(); let s=MapSettings::default();
    let t=Instant::now(); let _=render_labeling(&layers,&s); assert!(t.elapsed().as_nanos() <= 500_000_000);
}
#[test]
fn no_regression_index_insert() {
    let feats: Vec<Feature> = (0..100).map(|i| { let x=i as f64; let ext=vec![Point::new(x,0.0),Point::new(x+1.0,0.0),Point::new(x+1.0,1.0),Point::new(x,1.0)]; Feature::new(i as u64,Polygon::new(ext)) }).collect();
    let t=Instant::now(); let _=rtree_insert(&feats); assert!(t.elapsed().as_nanos() <= 500_000_000);
}
#[test]
fn no_regression_index_query() {
    let feats: Vec<Feature> = (0..100).map(|i| { let x=i as f64; let ext=vec![Point::new(x,0.0),Point::new(x+1.0,0.0),Point::new(x+1.0,1.0),Point::new(x,1.0)]; Feature::new(i as u64,Polygon::new(ext)) }).collect();
    let tree=rtree_insert(&feats); let ext=Extent::new(-1.0,-1.0,50.0,1.0); let t=Instant::now(); let _=rtree_query(&tree,&ext); assert!(t.elapsed().as_nanos() <= 500_000_000);
}
#[test]
fn no_regression_expression_parse() {
    let t=Instant::now(); let _=expression_parse("\"population\" / \"area\" * 100 + \"id\"").unwrap(); assert!(t.elapsed().as_nanos() <= 100_000_000);
}
#[test]
fn no_regression_expression_evaluate() {
    let expr=expression_parse("\"population\" / \"area\" * 100 + \"id\"").unwrap();
    let ctx=EvalContext::new().with_field("population",Value::Integer(1_000_000)).with_field("area",Value::Float(25.0)).with_field("id",Value::Integer(42));
    let t=Instant::now(); let _=expression_evaluate(&expr,&ctx).unwrap(); assert!(t.elapsed().as_nanos() <= 100_000_000);
}
#[test]
fn no_regression_raster_reproject() {
    let r=Raster::new(256,256,Crs::new("EPSG:4326"),Extent::new(-180.0,-90.0,180.0,90.0)); let dc=Crs::new("EPSG:3857"); let de=Extent::new(-20037508.0,-20037508.0,20037508.0,20037508.0);
    let t=Instant::now(); let _=raster_reproject(&r,&dc,&de); assert!(t.elapsed().as_nanos() <= 500_000_000);
}
#[test]
fn no_regression_raster_render() {
    let r=Raster::new(256,256,Crs::new("EPSG:4326"),Extent::new(-180.0,-90.0,180.0,90.0)); let ms=MapSettings::default();
    let t=Instant::now(); let _=raster_render(&r,&ms); assert!(t.elapsed().as_nanos() <= 500_000_000);
}
