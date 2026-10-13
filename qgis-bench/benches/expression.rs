//! Criterion harness for the **expression** hot paths.
//!
//! Mirrors the `QgsExpression` parse and evaluate paths: expressions are
//! parsed once and evaluated per feature.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use qgis_bench::expression::{expression_evaluate, expression_parse, EvalContext, Value};

/// A representative QGIS expression string.
const EXPR: &str = "\"population\" / \"area\" * 100 + \"id\"";

fn make_context() -> EvalContext {
    EvalContext::new()
        .with_field("population", Value::Integer(1_000_000))
        .with_field("area", Value::Float(25.0))
        .with_field("id", Value::Integer(42))
}

fn bench_expression_parse(c: &mut Criterion) {
    c.bench_function("expression/parse", |b| {
        b.iter(|| expression_parse(black_box(EXPR)).expect("valid benchmark expression"))
    });
}

fn bench_expression_evaluate(c: &mut Criterion) {
    let expr = expression_parse(EXPR).expect("valid benchmark expression");
    let context = make_context();
    assert_eq!(
        expression_evaluate(&expr, &context).expect("benchmark expression evaluates"),
        Value::Float(4_000_042.0)
    );
    c.bench_function("expression/evaluate", |b| {
        b.iter(|| expression_evaluate(black_box(&expr), black_box(&context)))
    });
}

criterion_group!(benches, bench_expression_parse, bench_expression_evaluate);
criterion_main!(benches);
