//! Criterion harness for the **expression** hot paths.
//!
//! Mirrors the `QgsExpression` parse and evaluate paths: expressions are
//! parsed once and evaluated per feature.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use qgis_bench::expression::{expression_evaluate, expression_parse};
use qgis_bench::{Feature, Point, Polygon};

/// A representative QGIS expression string.
const EXPR: &str = "\"population\" / $area * 100 + $id";

fn make_feature() -> Feature {
    let exterior = vec![
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        Point::new(1.0, 1.0),
        Point::new(0.0, 1.0),
    ];
    Feature::new(42, Polygon::new(exterior))
}

fn bench_expression_parse(c: &mut Criterion) {
    c.bench_function("expression/parse", |b| {
        b.iter(|| expression_parse(black_box(EXPR)))
    });
}

fn bench_expression_evaluate(c: &mut Criterion) {
    let expr = expression_parse(EXPR);
    let feature = make_feature();
    c.bench_function("expression/evaluate", |b| {
        b.iter(|| expression_evaluate(black_box(&expr), black_box(&feature)))
    });
}

criterion_group!(benches, bench_expression_parse, bench_expression_evaluate);
criterion_main!(benches);
