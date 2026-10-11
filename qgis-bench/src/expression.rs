//! Expression-engine hot paths (replacing Qwt-expression / `QgsExpression`).
//!
//! QGIS expressions are parsed once and evaluated per feature. These stubs
//! stand in for the parse and evaluate hot paths.

use crate::Feature;

/// A parsed expression stub.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    /// The original expression source text.
    pub source: String,
}

/// The value produced by evaluating an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Str(String),
    Bool(bool),
    Null,
}

/// Stub for the expression parse hot path: "compile" `source` into an
/// expression. Eventually backed by the Rust expression engine.
pub fn expression_parse(source: &str) -> Expression {
    Expression {
        source: source.to_string(),
    }
}

/// Stub for the expression evaluation hot path: evaluate `expr` against
/// `feature`. Eventually backed by the Rust expression engine.
pub fn expression_evaluate(expr: &Expression, feature: &Feature) -> Value {
    if expr.source.is_empty() {
        return Value::Null;
    }
    // Deterministic stand-in so the harness measures real work.
    let n = feature.geometry.vertex_count() as f64;
    Value::Number(n + expr.source.len() as f64)
}
