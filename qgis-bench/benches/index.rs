//! Criterion harness for the **index** (spatial R-tree) hot paths.
//!
//! Mirrors the `QgsSpatialIndex` build (insert) and query paths that the
//! render and provider layers drive.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use qgis_bench::index::{rtree_insert, rtree_query};
use qgis_bench::{Extent, Feature, Point, Polygon};

/// Builds `n` unit-square features laid out along the x axis.
fn make_features(n: usize) -> Vec<Feature> {
    (0..n)
        .map(|i| {
            let x = i as f64;
            let exterior = vec![
                Point::new(x, 0.0),
                Point::new(x + 1.0, 0.0),
                Point::new(x + 1.0, 1.0),
                Point::new(x, 1.0),
            ];
            Feature::new(i as u64, Polygon::new(exterior))
        })
        .collect()
}

fn bench_rtree_insert(c: &mut Criterion) {
    let features = make_features(1000);
    c.bench_function("index/rtree_insert", |b| {
        b.iter(|| rtree_insert(black_box(&features)))
    });
}

fn bench_rtree_query(c: &mut Criterion) {
    let features = make_features(1000);
    let tree = rtree_insert(&features);
    let extent = Extent::new(-1.0, -1.0, 500.0, 1.0);
    c.bench_function("index/rtree_query", |b| {
        b.iter(|| rtree_query(black_box(&tree), black_box(&extent)))
    });
}

criterion_group!(benches, bench_rtree_insert, bench_rtree_query);
criterion_main!(benches);
