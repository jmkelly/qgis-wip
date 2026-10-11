//! Criterion harness for the **render** hot paths.
//!
//! Mirrors `QgsBench::render` in `tests/bench/qgsbench.cpp`: the map-render
//! loop drives polygon fill, per-layer render, and labeling passes.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use qgis_bench::render::{render_labeling, render_layer, render_polygon_fill};
use qgis_bench::{Crs, Extent, Feature, Layer, MapSettings, Point, Polygon};

/// Builds `n_layers` layers of `n_features` unit-square polygons each.
fn make_layers(n_layers: usize, n_features: usize) -> Vec<Layer> {
    (0..n_layers)
        .map(|l| {
            let features = (0..n_features)
                .map(|f| {
                    let exterior = vec![
                        Point::new(0.0, 0.0),
                        Point::new(1.0, 0.0),
                        Point::new(1.0, 1.0),
                        Point::new(0.0, 1.0),
                    ];
                    Feature::new((l * n_features + f) as u64, Polygon::new(exterior))
                })
                .collect();
            Layer::new(format!("layer_{l}"), features)
        })
        .collect()
}

fn bench_render_polygon_fill(c: &mut Criterion) {
    let layers = make_layers(10, 100);
    c.bench_function("render/polygon_fill", |b| {
        b.iter(|| render_polygon_fill(black_box(&layers), 800, 600))
    });
}

fn bench_render_layer(c: &mut Criterion) {
    let layers = make_layers(10, 100);
    let settings = MapSettings::new(Extent::new(-1.0, -1.0, 1.0, 1.0), Crs::default(), 800, 600);
    c.bench_function("render/layer_render", |b| {
        b.iter(|| render_layer(black_box(&layers), black_box(&settings)))
    });
}

fn bench_render_labeling(c: &mut Criterion) {
    let layers = make_layers(10, 100);
    let settings = MapSettings::default();
    c.bench_function("render/labeling", |b| {
        b.iter(|| render_labeling(black_box(&layers), black_box(&settings)))
    });
}

criterion_group!(
    benches,
    bench_render_polygon_fill,
    bench_render_layer,
    bench_render_labeling
);
criterion_main!(benches);
