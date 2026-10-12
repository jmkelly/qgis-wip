//! Conditional expression functions.
//!
//! Implements: if, coalesce, nullif, when.
//! Pure Rust, zero unsafe.

use crate::ast::Literal;
use crate::eval::EvalError;

/// Look up a conditional function by name.
pub fn lookup(name: &str) -> Option<super::FunctionFn> {
    let f = match name {
        "if" => if_then_else as super::FunctionFn,
        "coalesce" => coalesce as super::FunctionFn,
        "nullif" => nullif as super::FunctionFn,
        "when" => when_cond as super::FunctionFn,
        _ => return None,
    };
    Some(f)
}

/// Get arity description for error messages.
pub fn arity(name: &str) -> Option<&'static str> {
    let desc = match name {
        "if" => "exactly 3",
        "coalesce" => "at least 1",
        "nullif" => "exactly 2",
        "when" => "exactly 2",
        _ => return None,
    };
    Some(desc)
}

fn require_arity(
    args: &[Literal],
    func: &str,
    min: usize,
    max: usize,
) -> Result<(), EvalError> {
    if args.len() < min || args.len() > max {
        return Err(EvalError::ArityError {
            function: func.to_string(),
            expected: arity(func).unwrap_or("correct number of").to_string(),
            got: args.len(),
        });
    }
    Ok(())
}

fn is_truthy(lit: &Literal) -> bool {
    match lit {
        Literal::Bool(b) => *b,
        Literal::Integer(i) => *i != 0,
        Literal::Float(f) => *f != 0.0,
        Literal::Str(s) => !s.is_empty(),
        Literal::Null => false,
        Literal::Date(_) => false,
        Literal::Array(_) => false,
    }
}

/// `if(condition, true_value, false_value)` — conditional expression.
fn if_then_else(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "if", 3, 3)?;
    if is_truthy(&args[0]) {
        Ok(args[1].clone())
    } else {
        Ok(args[2].clone())
    }
}

/// `coalesce(arg1, arg2, ...)` — return the first non-NULL argument.
fn coalesce(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "coalesce", 1, usize::MAX)?;
    for arg in args {
        if !matches!(arg, Literal::Null) {
            return Ok(arg.clone());
        }
    }
    Ok(Literal::Null)
}

/// `nullif(a, b)` — return NULL if a == b, otherwise return a.
fn nullif(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "nullif", 2, 2)?;
    if literals_equal(&args[0], &args[1]) {
        Ok(Literal::Null)
    } else {
        Ok(args[0].clone())
    }
}

/// `when(condition, result)` — return result if condition is true, else NULL.
fn when_cond(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "when", 2, 2)?;
    if is_truthy(&args[0]) {
        Ok(args[1].clone())
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
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Literal>) -> Result<Literal, EvalError> {
        super::super::call(name, args)
    }

    #[test]
    fn test_if_true() {
        assert_eq!(
            call(
                "if",
                vec![
                    Literal::Bool(true),
                    Literal::Str("yes".to_string()),
                    Literal::Str("no".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("yes".to_string())
        );
    }

    #[test]
    fn test_if_false() {
        assert_eq!(
            call(
                "if",
                vec![
                    Literal::Bool(false),
                    Literal::Str("yes".to_string()),
                    Literal::Str("no".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("no".to_string())
        );
    }

    #[test]
    fn test_coalesce_first_non_null() {
        assert_eq!(
            call(
                "coalesce",
                vec![
                    Literal::Null,
                    Literal::Null,
                    Literal::Str("found".to_string()),
                    Literal::Str("other".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("found".to_string())
        );
    }

    #[test]
    fn test_coalesce_all_null() {
        assert_eq!(
            call("coalesce", vec![Literal::Null, Literal::Null]).unwrap(),
            Literal::Null
        );
    }

    #[test]
    fn test_nullif_equal() {
        assert_eq!(
            call(
                "nullif",
                vec![
                    Literal::Str("same".to_string()),
                    Literal::Str("same".to_string())
                ]
            )
            .unwrap(),
            Literal::Null
        );
    }

    #[test]
    fn test_nullif_not_equal() {
        assert_eq!(
            call(
                "nullif",
                vec![
                    Literal::Str("a".to_string()),
                    Literal::Str("b".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("a".to_string())
        );
    }

    #[test]
    fn test_when_true() {
        assert_eq!(
            call(
                "when",
                vec![Literal::Bool(true), Literal::Str("result".to_string())]
            )
            .unwrap(),
            Literal::Str("result".to_string())
        );
    }

    #[test]
    fn test_when_false() {
        assert_eq!(
            call(
                "when",
                vec![Literal::Bool(false), Literal::Str("result".to_string())]
            )
            .unwrap(),
            Literal::Null
        );
    }

    #[test]
    fn test_arity_errors() {
        assert!(matches!(
            call("if", vec![Literal::Bool(true)]),
            Err(EvalError::ArityError { .. })
        ));
        assert!(matches!(
            call("coalesce", vec![]),
            Err(EvalError::ArityError { .. })
        ));
        assert!(matches!(
            call("nullif", vec![Literal::Null]),
            Err(EvalError::ArityError { .. })
        ));
    }
}
