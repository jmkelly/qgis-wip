//! Expression-engine hot paths (replacing Qwt-expression / `QgsExpression`).
//!
//! These helpers use the real parser, compiler, and evaluator so the Criterion
//! harness measures expression-engine work rather than a synthetic stand-in.

pub use qgis_expression::{EvalContext, Expr as Expression, Literal as Value};
use qgis_expression::{compile, evaluate, ExpressionError};

/// Compile an expression using the production expression engine.
pub fn expression_parse(source: &str) -> Result<Expression, ExpressionError> {
    compile(source)
}

/// Evaluate a compiled expression against a production evaluation context.
pub fn expression_evaluate(
    expr: &Expression,
    context: &EvalContext,
) -> Result<Value, ExpressionError> {
    evaluate(expr, context).map_err(ExpressionError::from)
}
