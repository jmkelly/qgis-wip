//! Expression evaluator for the QGIS Rust port.
//!
//! Evaluates a parsed [`Expr`] AST against a provided context of field values.
//! Function calls are dispatched through a registry that maps names to
//! implementations.

use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, IsTarget, Literal, UnaryOp};
use crate::functions;

/// Context for expression evaluation: a map of field names to literal values.
pub type Context = HashMap<String, Literal>;

/// A reference to the current feature for feature/form scope.
#[derive(Debug, Clone)]
pub struct FeatureRef {
    /// The feature ID.
    pub id: i64,
    /// The feature's attributes.
    pub attributes: HashMap<String, Literal>,
}

/// Evaluation context with support for fields, variables, and scope.
///
/// Extends the simple [`Context`] (field-only) with:
/// - **Variables**: values for `@variable` references
/// - **Geometry**: the current feature's geometry for `$geometry` and related variables
/// - **Feature**: the current feature for feature/form scope
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    /// Field values (feature attributes).
    pub fields: HashMap<String, Literal>,
    /// Expression variables (`@variable`).
    pub variables: HashMap<String, Literal>,
    /// Current feature geometry (for `$geometry` and geometry variables).
    pub geometry: Option<Literal>,
    /// Current feature reference (for feature scope).
    pub feature: Option<FeatureRef>,
}

impl EvalContext {
    /// Create an empty evaluation context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field value.
    pub fn with_field(mut self, name: impl Into<String>, value: Literal) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    /// Add a variable value.
    pub fn with_variable(mut self, name: impl Into<String>, value: Literal) -> Self {
        self.variables.insert(name.into(), value);
        self
    }

    /// Set the current geometry.
    pub fn with_geometry(mut self, geometry: Literal) -> Self {
        self.geometry = Some(geometry);
        self
    }

    /// Set the current feature.
    pub fn with_feature(mut self, feature: FeatureRef) -> Self {
        self.feature = Some(feature);
        self
    }
}

impl From<HashMap<String, Literal>> for EvalContext {
    fn from(fields: HashMap<String, Literal>) -> Self {
        Self {
            fields,
            ..Default::default()
        }
    }
}

/// Errors that can occur during expression evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// A field reference was not found in the evaluation context.
    UnknownField(String),
    /// A variable was not found in the evaluation context.
    UnknownVariable(String),
    /// No geometry is available in the evaluation context.
    NoGeometry,
    /// A function was called with the wrong number of arguments.
    ArityError {
        function: String,
        expected: String,
        got: usize,
    },
    /// A function was not found in the registry.
    UnknownFunction(String),
    /// A type error occurred during evaluation.
    TypeError {
        function: String,
        message: String,
    },
    /// Division by zero.
    DivisionByZero,
    /// An error from a nested expression evaluation.
    Nested(String),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::UnknownField(name) => write!(f, "unknown field: {}", name),
            EvalError::UnknownVariable(name) => write!(f, "unknown variable: {}", name),
            EvalError::NoGeometry => write!(f, "no geometry available"),
            EvalError::ArityError {
                function,
                expected,
                got,
            } => write!(
                f,
                "function {} expects {} arguments, got {}",
                function, expected, got
            ),
            EvalError::UnknownFunction(name) => write!(f, "unknown function: {}", name),
            EvalError::TypeError { function, message } => {
                write!(f, "type error in {}: {}", function, message)
            }
            EvalError::DivisionByZero => write!(f, "division by zero"),
            EvalError::Nested(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for EvalError {}

/// Evaluate an expression AST to a literal value.
pub fn evaluate(expr: &Expr, ctx: &EvalContext) -> Result<Literal, EvalError> {
    match expr {
        Expr::Literal(lit) => Ok(lit.clone()),
        Expr::FieldRef(name) => ctx
            .fields
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownField(name.clone())),
        Expr::Variable(name) => ctx
            .variables
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownVariable(name.clone())),
        Expr::DollarVar(name) => eval_dollar_var(name, ctx),
        Expr::Unary(op, inner) => {
            let val = evaluate(inner, ctx)?;
            eval_unary(*op, val)
        }
        Expr::Binary(op, left, right) => {
            let l = evaluate(left, ctx)?;
            let r = evaluate(right, ctx)?;
            eval_binary(*op, l, r)
        }
        Expr::FunctionCall(name, args) => {
            let evaluated: Result<Vec<Literal>, EvalError> =
                args.iter().map(|a| evaluate(a, ctx)).collect();
            functions::call(name, evaluated?)
        }
        Expr::CaseWhen {
            operand,
            whens,
            else_branch,
        } => eval_case_when(operand, whens, else_branch, ctx),
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            let needle = evaluate(expr, ctx)?;
            let mut found = false;
            for item in list {
                let val = evaluate(item, ctx)?;
                if literals_equal(&needle, &val) {
                    found = true;
                    break;
                }
            }
            Ok(Literal::Bool(if *negated { !found } else { found }))
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let val = evaluate(expr, ctx)?;
            let lo = evaluate(low, ctx)?;
            let hi = evaluate(high, ctx)?;
            let in_range = compare_literals(&val, &lo)
                .map(|o| o != std::cmp::Ordering::Less)
                .unwrap_or(false)
                && compare_literals(&val, &hi)
                    .map(|o| o != std::cmp::Ordering::Greater)
                    .unwrap_or(false);
            Ok(Literal::Bool(if *negated { !in_range } else { in_range }))
        }
        Expr::Like {
            expr,
            pattern,
            negated,
        } => {
            let val = evaluate(expr, ctx)?;
            let pat = evaluate(pattern, ctx)?;
            let matched = match (&val, &pat) {
                (Literal::Str(s), Literal::Str(p)) => like_match(s, p),
                _ => false,
            };
            Ok(Literal::Bool(if *negated { !matched } else { matched }))
        }
        Expr::Is {
            left,
            negated,
            right,
        } => {
            let val = evaluate(left, ctx)?;
            let is_match = match right {
                IsTarget::Null => matches!(val, Literal::Null),
                IsTarget::True => matches!(val, Literal::Bool(true)),
                IsTarget::False => matches!(val, Literal::Bool(false)),
            };
            Ok(Literal::Bool(if *negated { !is_match } else { is_match }))
        }
    }
}

/// Evaluate a dollar-prefixed variable (e.g., `$geometry`, `$area`).
fn eval_dollar_var(name: &str, ctx: &EvalContext) -> Result<Literal, EvalError> {
    match name {
        "geometry" | "currentgeometry" => ctx
            .geometry
            .clone()
            .ok_or(EvalError::NoGeometry),
        "area" | "length" | "x" | "y" | "x_at" | "y_at" => {
            // Geometry-derived variables require geometry support.
            // Return the geometry as a placeholder until geometry
            // computation is available.
            ctx.geometry
                .clone()
                .ok_or(EvalError::NoGeometry)
        }
        "currentfeature" => {
            // Return the feature's ID as a placeholder.
            ctx.feature
                .as_ref()
                .map(|f| Literal::Integer(f.id))
                .ok_or(EvalError::UnknownVariable("currentfeature".to_string()))
        }
        _ => Err(EvalError::UnknownVariable(name.to_string())),
    }
}

fn eval_unary(op: UnaryOp, val: Literal) -> Result<Literal, EvalError> {
    match op {
        UnaryOp::Neg => match val {
            Literal::Integer(i) => Ok(Literal::Integer(-i)),
            Literal::Float(f) => Ok(Literal::Float(-f)),
            other => Err(EvalError::TypeError {
                function: "-".to_string(),
                message: format!("cannot negate {}", other),
            }),
        },
        UnaryOp::Not => match val {
            Literal::Bool(b) => Ok(Literal::Bool(!b)),
            other => Err(EvalError::TypeError {
                function: "NOT".to_string(),
                message: format!("cannot apply NOT to {}", other),
            }),
        },
    }
}

fn eval_binary(op: BinaryOp, left: Literal, right: Literal) -> Result<Literal, EvalError> {
    use Literal::*;
    match op {
        BinaryOp::Add => match (&left, &right) {
            (Integer(a), Integer(b)) => Ok(Integer(a + b)),
            (Float(a), Float(b)) => Ok(Float(a + b)),
            (Integer(a), Float(b)) => Ok(Float(*a as f64 + b)),
            (Float(a), Integer(b)) => Ok(Float(a + *b as f64)),
            _ => Err(EvalError::TypeError {
                function: "+".to_string(),
                message: format!("cannot add {} and {}", left, right),
            }),
        },
        BinaryOp::Sub => match (&left, &right) {
            (Integer(a), Integer(b)) => Ok(Integer(a - b)),
            (Float(a), Float(b)) => Ok(Float(a - b)),
            (Integer(a), Float(b)) => Ok(Float(*a as f64 - b)),
            (Float(a), Integer(b)) => Ok(Float(a - *b as f64)),
            _ => Err(EvalError::TypeError {
                function: "-".to_string(),
                message: format!("cannot subtract {} from {}", right, left),
            }),
        },
        BinaryOp::Mul => match (&left, &right) {
            (Integer(a), Integer(b)) => Ok(Integer(a * b)),
            (Float(a), Float(b)) => Ok(Float(a * b)),
            (Integer(a), Float(b)) => Ok(Float(*a as f64 * b)),
            (Float(a), Integer(b)) => Ok(Float(a * *b as f64)),
            _ => Err(EvalError::TypeError {
                function: "*".to_string(),
                message: format!("cannot multiply {} and {}", left, right),
            }),
        },
        BinaryOp::Div => match (&left, &right) {
            (Integer(a), Integer(b)) => {
                if *b == 0 {
                    Err(EvalError::DivisionByZero)
                } else {
                    Ok(Float(*a as f64 / *b as f64))
                }
            }
            (Float(a), Float(b)) => {
                if *b == 0.0 {
                    Err(EvalError::DivisionByZero)
                } else {
                    Ok(Float(a / b))
                }
            }
            (Integer(a), Float(b)) => {
                if *b == 0.0 {
                    Err(EvalError::DivisionByZero)
                } else {
                    Ok(Float(*a as f64 / b))
                }
            }
            (Float(a), Integer(b)) => {
                if *b == 0 {
                    Err(EvalError::DivisionByZero)
                } else {
                    Ok(Float(a / *b as f64))
                }
            }
            _ => Err(EvalError::TypeError {
                function: "/".to_string(),
                message: format!("cannot divide {} by {}", left, right),
            }),
        },
        BinaryOp::Mod => match (&left, &right) {
            (Integer(a), Integer(b)) => {
                if *b == 0 {
                    Err(EvalError::DivisionByZero)
                } else {
                    Ok(Integer(a % b))
                }
            }
            (Float(a), Float(b)) => Ok(Float(a % b)),
            (Integer(a), Float(b)) => Ok(Float(*a as f64 % b)),
            (Float(a), Integer(b)) => Ok(Float(a % *b as f64)),
            _ => Err(EvalError::TypeError {
                function: "%".to_string(),
                message: format!("cannot mod {} by {}", left, right),
            }),
        },
        BinaryOp::Pow => match (&left, &right) {
            (Integer(a), Integer(b)) => {
                if *b >= 0 {
                    Ok(Integer(a.pow(*b as u32)))
                } else {
                    Ok(Float((*a as f64).powi(*b as i32)))
                }
            }
            (Float(a), Float(b)) => Ok(Float(a.powf(*b))),
            (Integer(a), Float(b)) => Ok(Float((*a as f64).powf(*b))),
            (Float(a), Integer(b)) => Ok(Float(a.powi(*b as i32))),
            _ => Err(EvalError::TypeError {
                function: "^".to_string(),
                message: format!("cannot raise {} to {}", left, right),
            }),
        },
        BinaryOp::Concat => match (&left, &right) {
            (Str(a), Str(b)) => Ok(Str(format!("{}{}", a, b))),
            (Str(a), Integer(b)) => Ok(Str(format!("{}{}", a, b))),
            (Str(a), Float(b)) => Ok(Str(format!("{}{}", a, b))),
            (Integer(a), Str(b)) => Ok(Str(format!("{}{}", a, b))),
            (Float(a), Str(b)) => Ok(Str(format!("{}{}", a, b))),
            (Null, Str(b)) => Ok(Str(b.clone())),
            (Str(a), Null) => Ok(Str(a.clone())),
            _ => Err(EvalError::TypeError {
                function: "||".to_string(),
                message: format!("cannot concatenate {} and {}", left, right),
            }),
        },
        BinaryOp::Eq => Ok(Literal::Bool(literals_equal(&left, &right))),
        BinaryOp::Ne => Ok(Literal::Bool(!literals_equal(&left, &right))),
        BinaryOp::Lt => compare_literals(&left, &right)
            .map(|o| Bool(o == std::cmp::Ordering::Less))
            .ok_or_else(|| EvalError::TypeError {
                function: "<".to_string(),
                message: format!("cannot compare {} < {}", left, right),
            }),
        BinaryOp::Gt => compare_literals(&left, &right)
            .map(|o| Bool(o == std::cmp::Ordering::Greater))
            .ok_or_else(|| EvalError::TypeError {
                function: ">".to_string(),
                message: format!("cannot compare {} > {}", left, right),
            }),
        BinaryOp::Le => compare_literals(&left, &right)
            .map(|o| Bool(o != std::cmp::Ordering::Greater))
            .ok_or_else(|| EvalError::TypeError {
                function: "<=".to_string(),
                message: format!("cannot compare {} <= {}", left, right),
            }),
        BinaryOp::Ge => compare_literals(&left, &right)
            .map(|o| Bool(o != std::cmp::Ordering::Less))
            .ok_or_else(|| EvalError::TypeError {
                function: ">=".to_string(),
                message: format!("cannot compare {} >= {}", left, right),
            }),
        BinaryOp::And => match (&left, &right) {
            (Bool(a), Bool(b)) => Ok(Bool(*a && *b)),
            _ => Err(EvalError::TypeError {
                function: "AND".to_string(),
                message: format!("cannot apply AND to {} and {}", left, right),
            }),
        },
        BinaryOp::Or => match (&left, &right) {
            (Bool(a), Bool(b)) => Ok(Bool(*a || *b)),
            _ => Err(EvalError::TypeError {
                function: "OR".to_string(),
                message: format!("cannot apply OR to {} and {}", left, right),
            }),
        },
    }
}

fn eval_case_when(
    operand: &Option<Box<Expr>>,
    whens: &[(Expr, Expr)],
    else_branch: &Option<Box<Expr>>,
    ctx: &EvalContext,
) -> Result<Literal, EvalError> {
    if let Some(op_expr) = operand {
        let op_val = evaluate(op_expr, ctx)?;
        for (cond, result) in whens {
            let cond_val = evaluate(cond, ctx)?;
            if literals_equal(&op_val, &cond_val) {
                return evaluate(result, ctx);
            }
        }
    } else {
        for (cond, result) in whens {
            let cond_val = evaluate(cond, ctx)?;
            if matches!(cond_val, Literal::Bool(true)) {
                return evaluate(result, ctx);
            }
        }
    }
    if let Some(eb) = else_branch {
        evaluate(eb, ctx)
    } else {
        Ok(Literal::Null)
    }
}

fn literals_equal(l: &Literal, r: &Literal) -> bool {
    use Literal::*;
    match (l, r) {
        (Null, Null) => true,
        (Bool(a), Bool(b)) => a == b,
        (Integer(a), Integer(b)) => a == b,
        (Float(a), Float(b)) => a == b,
        (Integer(a), Float(b)) | (Float(b), Integer(a)) => (*a as f64) == *b,
        (Str(a), Str(b)) => a == b,
        (Date(a), Date(b)) => a == b,
        (Array(a), Array(b)) => a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| literals_equal(x, y)),
        _ => false,
    }
}

fn compare_literals(l: &Literal, r: &Literal) -> Option<std::cmp::Ordering> {
    use Literal::*;
    match (l, r) {
        (Integer(a), Integer(b)) => Some(a.cmp(b)),
        (Float(a), Float(b)) => a.partial_cmp(b),
        (Integer(a), Float(b)) => (*a as f64).partial_cmp(b),
        (Float(a), Integer(b)) => a.partial_cmp(&(*b as f64)),
        (Str(a), Str(b)) => Some(a.cmp(b)),
        (Date(a), Date(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Simple SQL LIKE pattern matching.
/// `%` matches any sequence of characters, `_` matches any single character.
fn like_match(value: &str, pattern: &str) -> bool {
    let val_chars: Vec<char> = value.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();
    like_inner(&val_chars, &pat_chars)
}

fn like_inner(val: &[char], pat: &[char]) -> bool {
    if pat.is_empty() {
        return val.is_empty();
    }
    match pat[0] {
        '%' => {
            for i in 0..=val.len() {
                if like_inner(&val[i..], &pat[1..]) {
                    return true;
                }
            }
            false
        }
        '_' => {
            if val.is_empty() {
                false
            } else {
                like_inner(&val[1..], &pat[1..])
            }
        }
        c => {
            if val.is_empty() || val[0] != c {
                false
            } else {
                like_inner(&val[1..], &pat[1..])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn eval_str(input: &str) -> Literal {
        let expr = parse(input).unwrap();
        let ctx = EvalContext::new();
        evaluate(&expr, &ctx).unwrap()
    }

    fn eval_with_ctx(input: &str, ctx: &EvalContext) -> Literal {
        let expr = parse(input).unwrap();
        evaluate(&expr, ctx).unwrap()
    }

    #[test]
    fn eval_integer_literal() {
        assert_eq!(eval_str("42"), Literal::Integer(42));
    }

    #[test]
    fn eval_string_literal() {
        assert_eq!(eval_str("'hello'"), Literal::Str("hello".to_string()));
    }

    #[test]
    fn eval_null_literal() {
        assert_eq!(eval_str("NULL"), Literal::Null);
    }

    #[test]
    fn eval_true_false() {
        assert_eq!(eval_str("TRUE"), Literal::Bool(true));
        assert_eq!(eval_str("FALSE"), Literal::Bool(false));
    }

    #[test]
    fn eval_addition() {
        assert_eq!(eval_str("1 + 2"), Literal::Integer(3));
    }

    #[test]
    fn eval_subtraction() {
        assert_eq!(eval_str("10 - 3"), Literal::Integer(7));
    }

    #[test]
    fn eval_multiplication() {
        assert_eq!(eval_str("3 * 4"), Literal::Integer(12));
    }

    #[test]
    fn eval_division() {
        assert_eq!(eval_str("10 / 4"), Literal::Float(2.5));
    }

    #[test]
    fn eval_division_by_zero() {
        let expr = parse("1 / 0").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::DivisionByZero)
        ));
    }

    #[test]
    fn eval_concat() {
        assert_eq!(
            eval_str("'foo' || 'bar'"),
            Literal::Str("foobar".to_string())
        );
    }

    #[test]
    fn eval_comparison() {
        assert_eq!(eval_str("1 < 2"), Literal::Bool(true));
        assert_eq!(eval_str("2 > 1"), Literal::Bool(true));
        assert_eq!(eval_str("1 = 1"), Literal::Bool(true));
        assert_eq!(eval_str("1 != 2"), Literal::Bool(true));
        assert_eq!(eval_str("1 >= 1"), Literal::Bool(true));
        assert_eq!(eval_str("1 <= 1"), Literal::Bool(true));
    }

    #[test]
    fn eval_and_or() {
        assert_eq!(eval_str("TRUE AND FALSE"), Literal::Bool(false));
        assert_eq!(eval_str("TRUE OR FALSE"), Literal::Bool(true));
    }

    #[test]
    fn eval_not() {
        assert_eq!(eval_str("NOT TRUE"), Literal::Bool(false));
        assert_eq!(eval_str("NOT FALSE"), Literal::Bool(true));
    }

    #[test]
    fn eval_field_ref() {
        let mut ctx = EvalContext::new();
        ctx.fields
            .insert("name".to_string(), Literal::Str("Alice".to_string()));
        assert_eq!(
            eval_with_ctx("\"name\"", &ctx),
            Literal::Str("Alice".to_string())
        );
    }

    #[test]
    fn eval_unknown_field_error() {
        let expr = parse("\"missing\"").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::UnknownField(_))
        ));
    }

    #[test]
    fn eval_is_null() {
        assert_eq!(eval_str("NULL IS NULL"), Literal::Bool(true));
        assert_eq!(eval_str("1 IS NULL"), Literal::Bool(false));
        assert_eq!(eval_str("NULL IS NOT NULL"), Literal::Bool(false));
    }

    #[test]
    fn eval_in_list() {
        assert_eq!(eval_str("1 IN (1, 2, 3)"), Literal::Bool(true));
        assert_eq!(eval_str("5 IN (1, 2, 3)"), Literal::Bool(false));
        assert_eq!(eval_str("1 NOT IN (1, 2, 3)"), Literal::Bool(false));
        assert_eq!(eval_str("5 NOT IN (1, 2, 3)"), Literal::Bool(true));
    }

    #[test]
    fn eval_between() {
        assert_eq!(eval_str("5 BETWEEN 1 AND 10"), Literal::Bool(true));
        assert_eq!(eval_str("15 BETWEEN 1 AND 10"), Literal::Bool(false));
        assert_eq!(eval_str("5 NOT BETWEEN 1 AND 10"), Literal::Bool(false));
    }

    #[test]
    fn eval_case_when() {
        assert_eq!(
            eval_str("CASE WHEN TRUE THEN 1 ELSE 2 END"),
            Literal::Integer(1)
        );
        assert_eq!(
            eval_str("CASE WHEN FALSE THEN 1 ELSE 2 END"),
            Literal::Integer(2)
        );
    }

    #[test]
    fn eval_like() {
        assert_eq!(
            eval_str("'hello' LIKE 'h%'"),
            Literal::Bool(true)
        );
        assert_eq!(
            eval_str("'hello' LIKE 'x%'"),
            Literal::Bool(false)
        );
        assert_eq!(
            eval_str("'hello' LIKE 'h_llo'"),
            Literal::Bool(true)
        );
    }

    #[test]
    fn eval_nested_function_calls() {
        assert_eq!(
            eval_str("upper(lower('HELLO'))"),
            Literal::Str("HELLO".to_string())
        );
    }

    #[test]
    fn eval_arity_error() {
        let expr = parse("length()").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::ArityError { .. })
        ));
    }

    #[test]
    fn eval_unknown_function_error() {
        let expr = parse("nonexistent_func(1)").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::UnknownFunction(_))
        ));
    }

    // === Variable tests ===

    #[test]
    fn eval_variable() {
        let mut ctx = EvalContext::new();
        ctx.variables
            .insert("my_var".to_string(), Literal::Integer(42));
        assert_eq!(eval_with_ctx("@my_var", &ctx), Literal::Integer(42));
    }

    #[test]
    fn eval_variable_string() {
        let mut ctx = EvalContext::new();
        ctx.variables.insert(
            "greeting".to_string(),
            Literal::Str("hello".to_string()),
        );
        assert_eq!(
            eval_with_ctx("@greeting", &ctx),
            Literal::Str("hello".to_string())
        );
    }

    #[test]
    fn eval_unknown_variable_error() {
        let expr = parse("@missing").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::UnknownVariable(name)) if name == "missing"
        ));
    }

    #[test]
    fn eval_variable_in_expression() {
        let mut ctx = EvalContext::new();
        ctx.variables
            .insert("x".to_string(), Literal::Integer(10));
        assert_eq!(eval_with_ctx("@x + 5", &ctx), Literal::Integer(15));
    }

    // === Dollar variable tests ===

    #[test]
    fn eval_dollar_geometry() {
        let ctx = EvalContext::new()
            .with_geometry(Literal::Str("POINT(1 2)".to_string()));
        assert_eq!(
            eval_with_ctx("$geometry", &ctx),
            Literal::Str("POINT(1 2)".to_string())
        );
    }

    #[test]
    fn eval_dollar_geometry_no_geometry_error() {
        let expr = parse("$geometry").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::NoGeometry)
        ));
    }

    #[test]
    fn eval_dollar_area() {
        let ctx = EvalContext::new()
            .with_geometry(Literal::Str("POLYGON((0 0, 1 0, 1 1, 0 1, 0 0))".to_string()));
        // For now, $area returns the geometry as a placeholder
        let result = eval_with_ctx("$area", &ctx);
        assert!(matches!(result, Literal::Str(_)));
    }

    #[test]
    fn eval_dollar_length() {
        let ctx = EvalContext::new()
            .with_geometry(Literal::Str("LINESTRING(0 0, 1 1)".to_string()));
        let result = eval_with_ctx("$length", &ctx);
        assert!(matches!(result, Literal::Str(_)));
    }

    #[test]
    fn eval_dollar_x() {
        let ctx = EvalContext::new()
            .with_geometry(Literal::Str("POINT(1 2)".to_string()));
        let result = eval_with_ctx("$x", &ctx);
        assert!(matches!(result, Literal::Str(_)));
    }

    #[test]
    fn eval_dollar_y() {
        let ctx = EvalContext::new()
            .with_geometry(Literal::Str("POINT(1 2)".to_string()));
        let result = eval_with_ctx("$y", &ctx);
        assert!(matches!(result, Literal::Str(_)));
    }

    #[test]
    fn eval_dollar_currentgeometry() {
        let ctx = EvalContext::new()
            .with_geometry(Literal::Str("POINT(5 10)".to_string()));
        assert_eq!(
            eval_with_ctx("$currentgeometry", &ctx),
            Literal::Str("POINT(5 10)".to_string())
        );
    }

    #[test]
    fn eval_dollar_currentfeature() {
        let mut attrs = HashMap::new();
        attrs.insert("name".to_string(), Literal::Str("test".to_string()));
        let feature = FeatureRef {
            id: 42,
            attributes: attrs,
        };
        let ctx = EvalContext::new().with_feature(feature);
        assert_eq!(
            eval_with_ctx("$currentfeature", &ctx),
            Literal::Integer(42)
        );
    }

    #[test]
    fn eval_dollar_unknown_variable() {
        let expr = parse("$nonexistent").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::UnknownVariable(name)) if name == "nonexistent"
        ));
    }

    #[test]
    fn eval_dollar_area_no_geometry_error() {
        let expr = parse("$area").unwrap();
        assert!(matches!(
            evaluate(&expr, &EvalContext::new()),
            Err(EvalError::NoGeometry)
        ));
    }

    // === Feature scope tests ===

    #[test]
    fn eval_feature_in_context() {
        let mut attrs = HashMap::new();
        attrs.insert("population".to_string(), Literal::Integer(1000));
        let feature = FeatureRef {
            id: 1,
            attributes: attrs,
        };
        let ctx = EvalContext::new()
            .with_feature(feature)
            .with_geometry(Literal::Str("POINT(0 0)".to_string()));
        // Feature scope: fields and geometry are accessible
        assert_eq!(
            eval_with_ctx("$geometry", &ctx),
            Literal::Str("POINT(0 0)".to_string())
        );
    }

    #[test]
    fn eval_variable_and_field_together() {
        let mut ctx = EvalContext::new();
        ctx.fields
            .insert("name".to_string(), Literal::Str("Alice".to_string()));
        ctx.variables
            .insert("greeting".to_string(), Literal::Str("Hello".to_string()));
        let result = eval_with_ctx("@greeting || ' ' || \"name\"", &ctx);
        assert_eq!(
            result,
            Literal::Str("Hello Alice".to_string())
        );
    }

    #[test]
    fn eval_context_from_hashmap() {
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), Literal::Integer(5));
        let ctx = EvalContext::from(fields);
        assert_eq!(eval_with_ctx("\"x\" + 1", &ctx), Literal::Integer(6));
    }
}
