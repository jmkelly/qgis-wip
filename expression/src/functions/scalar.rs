//! Scalar and math expression functions.
//!
//! Implements: abs, ceil, floor, round, exp, ln, log10, log, pow, sqrt,
//! sin, cos, tan, asin, acos, atan, atan2, pi, min, max, clamp,
//! scale_linear, scale_exp, mod, to_int, to_real, to_string.
//! Pure Rust, zero unsafe.

use crate::ast::Literal;
use crate::eval::EvalError;

/// Look up a scalar/math function by name.
pub fn lookup(name: &str) -> Option<super::FunctionFn> {
    let f = match name {
        "abs" => abs as super::FunctionFn,
        "ceil" => ceil as super::FunctionFn,
        "ceiling" => ceil as super::FunctionFn,
        "floor" => floor as super::FunctionFn,
        "round" => round as super::FunctionFn,
        "exp" => exp as super::FunctionFn,
        "ln" => ln as super::FunctionFn,
        "log10" => log10 as super::FunctionFn,
        "log" => log as super::FunctionFn,
        "pow" => pow as super::FunctionFn,
        "sqrt" => sqrt as super::FunctionFn,
        "sin" => sin as super::FunctionFn,
        "cos" => cos as super::FunctionFn,
        "tan" => tan as super::FunctionFn,
        "asin" => asin as super::FunctionFn,
        "acos" => acos as super::FunctionFn,
        "atan" => atan as super::FunctionFn,
        "atan2" => atan2 as super::FunctionFn,
        "pi" => pi as super::FunctionFn,
        "min" => min as super::FunctionFn,
        "max" => max as super::FunctionFn,
        "clamp" => clamp as super::FunctionFn,
        "scale_linear" => scale_linear as super::FunctionFn,
        "scale_exp" => scale_exp as super::FunctionFn,
        "mod" => mod_fn as super::FunctionFn,
        "to_int" => to_int as super::FunctionFn,
        "to_real" => to_real as super::FunctionFn,
        "to_string" => to_string as super::FunctionFn,
        _ => return None,
    };
    Some(f)
}

/// Get arity description for error messages.
pub fn arity(name: &str) -> Option<&'static str> {
    let desc = match name {
        "abs" | "ceil" | "ceiling" | "floor" | "exp" | "ln" | "log10" | "sqrt"
        | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "to_int"
        | "to_real" | "to_string" => "exactly 1",
        "round" => "1 or 2",
        "log" => "1 or 2",
        "pow" | "atan2" | "mod" => "exactly 2",
        "pi" => "exactly 0",
        "min" | "max" => "at least 1",
        "clamp" => "exactly 3",
        "scale_linear" => "exactly 5",
        "scale_exp" => "exactly 5",
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

fn as_f64(arg: &Literal, func: &str) -> Result<f64, EvalError> {
    match arg {
        Literal::Integer(i) => Ok(*i as f64),
        Literal::Float(f) => Ok(*f),
        Literal::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Literal::Null => Ok(0.0),
        Literal::Str(s) => s.parse::<f64>().map_err(|_| EvalError::TypeError {
            function: func.to_string(),
            message: format!("cannot convert string '{}' to number", s),
        }),
    }
}

fn as_i64(arg: &Literal, func: &str) -> Result<i64, EvalError> {
    match arg {
        Literal::Integer(i) => Ok(*i),
        Literal::Float(f) => Ok(*f as i64),
        Literal::Bool(b) => Ok(if *b { 1 } else { 0 }),
        Literal::Null => Ok(0),
        Literal::Str(s) => s.parse::<i64>().map_err(|_| EvalError::TypeError {
            function: func.to_string(),
            message: format!("cannot convert string '{}' to integer", s),
        }),
    }
}

fn to_number_result(val: f64, original: &Literal) -> Literal {
    // If original was integer and result is a whole number, return integer
    match original {
        Literal::Integer(_) if val.fract() == 0.0 && val.abs() < i64::MAX as f64 => {
            Literal::Integer(val as i64)
        }
        _ => Literal::Float(val),
    }
}

// === Math functions ===

fn abs(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "abs", 1, 1)?;
    let val = as_f64(&args[0], "abs")?;
    Ok(to_number_result(val.abs(), &args[0]))
}

fn ceil(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "ceil", 1, 1)?;
    let val = as_f64(&args[0], "ceil")?;
    Ok(Literal::Integer(val.ceil() as i64))
}

fn floor(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "floor", 1, 1)?;
    let val = as_f64(&args[0], "floor")?;
    Ok(Literal::Integer(val.floor() as i64))
}

fn round(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "round", 1, 2)?;
    let val = as_f64(&args[0], "round")?;
    let result = if args.len() == 2 {
        let digits = as_i64(&args[1], "round")?;
        let factor = 10f64.powi(digits.clamp(-15, 15) as i32);
        (val * factor).round() / factor
    } else {
        val.round()
    };
    // QGIS round() returns integer when no decimal places specified
    if args.len() == 2 {
        Ok(Literal::Float(result))
    } else {
        Ok(Literal::Integer(result as i64))
    }
}

fn exp(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "exp", 1, 1)?;
    let val = as_f64(&args[0], "exp")?;
    Ok(Literal::Float(val.exp()))
}

fn ln(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "ln", 1, 1)?;
    let val = as_f64(&args[0], "ln")?;
    if val <= 0.0 {
        return Err(EvalError::TypeError {
            function: "ln".to_string(),
            message: "ln requires a positive number".to_string(),
        });
    }
    Ok(Literal::Float(val.ln()))
}

fn log10(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "log10", 1, 1)?;
    let val = as_f64(&args[0], "log10")?;
    if val <= 0.0 {
        return Err(EvalError::TypeError {
            function: "log10".to_string(),
            message: "log10 requires a positive number".to_string(),
        });
    }
    Ok(Literal::Float(val.log10()))
}

fn log(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "log", 1, 2)?;
    let val = as_f64(&args[0], "log")?;
    if val <= 0.0 {
        return Err(EvalError::TypeError {
            function: "log".to_string(),
            message: "log requires a positive number".to_string(),
        });
    }
    let result = if args.len() == 2 {
        let base = as_f64(&args[1], "log")?;
        if base <= 0.0 || base == 1.0 {
            return Err(EvalError::TypeError {
                function: "log".to_string(),
                message: "log base must be positive and not 1".to_string(),
            });
        }
        val.log(base)
    } else {
        val.ln()
    };
    Ok(Literal::Float(result))
}

fn pow(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "pow", 2, 2)?;
    let base = as_f64(&args[0], "pow")?;
    let exp = as_f64(&args[1], "pow")?;
    Ok(Literal::Float(base.powf(exp)))
}

fn sqrt(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "sqrt", 1, 1)?;
    let val = as_f64(&args[0], "sqrt")?;
    if val < 0.0 {
        return Err(EvalError::TypeError {
            function: "sqrt".to_string(),
            message: "sqrt requires a non-negative number".to_string(),
        });
    }
    Ok(Literal::Float(val.sqrt()))
}

// === Trig functions ===

fn sin(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "sin", 1, 1)?;
    let val = as_f64(&args[0], "sin")?;
    Ok(Literal::Float(val.sin()))
}

fn cos(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "cos", 1, 1)?;
    let val = as_f64(&args[0], "cos")?;
    Ok(Literal::Float(val.cos()))
}

fn tan(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "tan", 1, 1)?;
    let val = as_f64(&args[0], "tan")?;
    Ok(Literal::Float(val.tan()))
}

fn asin(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "asin", 1, 1)?;
    let val = as_f64(&args[0], "asin")?;
    if val < -1.0 || val > 1.0 {
        return Err(EvalError::TypeError {
            function: "asin".to_string(),
            message: "asin requires a value between -1 and 1".to_string(),
        });
    }
    Ok(Literal::Float(val.asin()))
}

fn acos(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "acos", 1, 1)?;
    let val = as_f64(&args[0], "acos")?;
    if val < -1.0 || val > 1.0 {
        return Err(EvalError::TypeError {
            function: "acos".to_string(),
            message: "acos requires a value between -1 and 1".to_string(),
        });
    }
    Ok(Literal::Float(val.acos()))
}

fn atan(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "atan", 1, 1)?;
    let val = as_f64(&args[0], "atan")?;
    Ok(Literal::Float(val.atan()))
}

fn atan2(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "atan2", 2, 2)?;
    let y = as_f64(&args[0], "atan2")?;
    let x = as_f64(&args[1], "atan2")?;
    Ok(Literal::Float(y.atan2(x)))
}

fn pi(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "pi", 0, 0)?;
    Ok(Literal::Float(std::f64::consts::PI))
}

// === Aggregate-like functions ===

fn min(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "min", 1, usize::MAX)?;
    let mut min_val = as_f64(&args[0], "min")?;
    for arg in &args[1..] {
        let val = as_f64(arg, "min")?;
        if val < min_val {
            min_val = val;
        }
    }
    Ok(to_number_result(min_val, &args[0]))
}

fn max(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "max", 1, usize::MAX)?;
    let mut max_val = as_f64(&args[0], "max")?;
    for arg in &args[1..] {
        let val = as_f64(arg, "max")?;
        if val > max_val {
            max_val = val;
        }
    }
    Ok(to_number_result(max_val, &args[0]))
}

fn clamp(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "clamp", 3, 3)?;
    let val = as_f64(&args[0], "clamp")?;
    let low = as_f64(&args[1], "clamp")?;
    let high = as_f64(&args[2], "clamp")?;
    Ok(to_number_result(val.clamp(low, high), &args[0]))
}

fn scale_linear(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "scale_linear", 5, 5)?;
    let val = as_f64(&args[0], "scale_linear")?;
    let domain_min = as_f64(&args[1], "scale_linear")?;
    let domain_max = as_f64(&args[2], "scale_linear")?;
    let range_min = as_f64(&args[3], "scale_linear")?;
    let range_max = as_f64(&args[4], "scale_linear")?;
    if domain_max == domain_min {
        return Ok(Literal::Float(range_min));
    }
    let t = (val - domain_min) / (domain_max - domain_min);
    Ok(Literal::Float(range_min + t * (range_max - range_min)))
}

fn scale_exp(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "scale_exp", 5, 5)?;
    let val = as_f64(&args[0], "scale_exp")?;
    let domain_min = as_f64(&args[1], "scale_exp")?;
    let domain_max = as_f64(&args[2], "scale_exp")?;
    let range_min = as_f64(&args[3], "scale_exp")?;
    let range_max = as_f64(&args[4], "scale_exp")?;
    if domain_max == domain_min {
        return Ok(Literal::Float(range_min));
    }
    let exponent = 1.0; // QGIS default
    let t = (val - domain_min) / (domain_max - domain_min);
    let scaled = t.powf(exponent);
    Ok(Literal::Float(range_min + scaled * (range_max - range_min)))
}

fn mod_fn(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "mod", 2, 2)?;
    let a = as_i64(&args[0], "mod")?;
    let b = as_i64(&args[1], "mod")?;
    if b == 0 {
        return Err(EvalError::DivisionByZero);
    }
    Ok(Literal::Integer(a % b))
}

// === Type conversion functions ===

fn to_int(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "to_int", 1, 1)?;
    match &args[0] {
        Literal::Integer(i) => Ok(Literal::Integer(*i)),
        Literal::Float(f) => Ok(Literal::Integer(*f as i64)),
        Literal::Bool(b) => Ok(Literal::Integer(if *b { 1 } else { 0 })),
        Literal::Str(s) => s.parse::<i64>().map(Literal::Integer).map_err(|_| {
            EvalError::TypeError {
                function: "to_int".to_string(),
                message: format!("cannot convert '{}' to integer", s),
            }
        }),
        Literal::Null => Ok(Literal::Null),
    }
}

fn to_real(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "to_real", 1, 1)?;
    match &args[0] {
        Literal::Integer(i) => Ok(Literal::Float(*i as f64)),
        Literal::Float(f) => Ok(Literal::Float(*f)),
        Literal::Bool(b) => Ok(Literal::Float(if *b { 1.0 } else { 0.0 })),
        Literal::Str(s) => s.parse::<f64>().map(Literal::Float).map_err(|_| {
            EvalError::TypeError {
                function: "to_real".to_string(),
                message: format!("cannot convert '{}' to real number", s),
            }
        }),
        Literal::Null => Ok(Literal::Null),
    }
}

fn to_string(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "to_string", 1, 1)?;
    Ok(Literal::Str(match &args[0] {
        Literal::Str(s) => s.clone(),
        Literal::Integer(i) => i.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Literal::Null => "".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Literal>) -> Result<Literal, EvalError> {
        super::super::call(name, args)
    }

    #[test]
    fn test_abs() {
        assert_eq!(call("abs", vec![Literal::Integer(-5)]).unwrap(), Literal::Integer(5));
        assert_eq!(call("abs", vec![Literal::Float(-3.14)]).unwrap(), Literal::Float(3.14));
    }

    #[test]
    fn test_ceil() {
        assert_eq!(call("ceil", vec![Literal::Float(3.2)]).unwrap(), Literal::Integer(4));
        assert_eq!(call("ceil", vec![Literal::Float(-3.2)]).unwrap(), Literal::Integer(-3));
    }

    #[test]
    fn test_floor() {
        assert_eq!(call("floor", vec![Literal::Float(3.8)]).unwrap(), Literal::Integer(3));
        assert_eq!(call("floor", vec![Literal::Float(-3.8)]).unwrap(), Literal::Integer(-4));
    }

    #[test]
    fn test_round() {
        assert_eq!(call("round", vec![Literal::Float(3.5)]).unwrap(), Literal::Integer(4));
        assert_eq!(call("round", vec![Literal::Float(3.4)]).unwrap(), Literal::Integer(3));
        assert_eq!(
            call("round", vec![Literal::Float(3.14159), Literal::Integer(2)]).unwrap(),
            Literal::Float(3.14)
        );
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(call("sqrt", vec![Literal::Integer(9)]).unwrap(), Literal::Float(3.0));
        assert_eq!(call("sqrt", vec![Literal::Float(2.0)]).unwrap(), Literal::Float(2.0f64.sqrt()));
    }

    #[test]
    fn test_pow() {
        assert_eq!(call("pow", vec![Literal::Integer(2), Literal::Integer(3)]).unwrap(), Literal::Float(8.0));
    }

    #[test]
    fn test_exp() {
        let result = call("exp", vec![Literal::Float(1.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if (f - std::f64::consts::E).abs() < 1e-10));
    }

    #[test]
    fn test_ln() {
        let result = call("ln", vec![Literal::Float(std::f64::consts::E)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if (f - 1.0).abs() < 1e-10));
    }

    #[test]
    fn test_log10() {
        assert_eq!(call("log10", vec![Literal::Integer(100)]).unwrap(), Literal::Float(2.0));
    }

    #[test]
    fn test_log() {
        assert_eq!(call("log", vec![Literal::Integer(8), Literal::Integer(2)]).unwrap(), Literal::Float(3.0));
    }

    #[test]
    fn test_sin() {
        let result = call("sin", vec![Literal::Float(0.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if f.abs() < 1e-10));
    }

    #[test]
    fn test_cos() {
        let result = call("cos", vec![Literal::Float(0.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if (f - 1.0).abs() < 1e-10));
    }

    #[test]
    fn test_tan() {
        let result = call("tan", vec![Literal::Float(0.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if f.abs() < 1e-10));
    }

    #[test]
    fn test_asin() {
        let result = call("asin", vec![Literal::Float(0.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if f.abs() < 1e-10));
    }

    #[test]
    fn test_acos() {
        let result = call("acos", vec![Literal::Float(1.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if f.abs() < 1e-10));
    }

    #[test]
    fn test_atan() {
        let result = call("atan", vec![Literal::Float(0.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if f.abs() < 1e-10));
    }

    #[test]
    fn test_atan2() {
        let result = call("atan2", vec![Literal::Float(1.0), Literal::Float(1.0)]).unwrap();
        assert!(matches!(result, Literal::Float(f) if (f - std::f64::consts::FRAC_PI_4).abs() < 1e-10));
    }

    #[test]
    fn test_pi() {
        let result = call("pi", vec![]).unwrap();
        assert!(matches!(result, Literal::Float(f) if (f - std::f64::consts::PI).abs() < 1e-10));
    }

    #[test]
    fn test_min() {
        assert_eq!(
            call("min", vec![Literal::Integer(3), Literal::Integer(1), Literal::Integer(2)]).unwrap(),
            Literal::Integer(1)
        );
    }

    #[test]
    fn test_max() {
        assert_eq!(
            call("max", vec![Literal::Integer(3), Literal::Integer(1), Literal::Integer(2)]).unwrap(),
            Literal::Integer(3)
        );
    }

    #[test]
    fn test_clamp() {
        assert_eq!(
            call("clamp", vec![Literal::Integer(5), Literal::Integer(1), Literal::Integer(3)]).unwrap(),
            Literal::Integer(3)
        );
        assert_eq!(
            call("clamp", vec![Literal::Integer(0), Literal::Integer(1), Literal::Integer(3)]).unwrap(),
            Literal::Integer(1)
        );
    }

    #[test]
    fn test_mod() {
        assert_eq!(call("mod", vec![Literal::Integer(10), Literal::Integer(3)]).unwrap(), Literal::Integer(1));
    }

    #[test]
    fn test_to_int() {
        assert_eq!(call("to_int", vec![Literal::Float(3.7)]).unwrap(), Literal::Integer(3));
        assert_eq!(call("to_int", vec![Literal::Str("42".to_string())]).unwrap(), Literal::Integer(42));
    }

    #[test]
    fn test_to_real() {
        assert_eq!(call("to_real", vec![Literal::Integer(5)]).unwrap(), Literal::Float(5.0));
        assert_eq!(call("to_real", vec![Literal::Str("3.14".to_string())]).unwrap(), Literal::Float(3.14));
    }

    #[test]
    fn test_to_string() {
        assert_eq!(call("to_string", vec![Literal::Integer(42)]).unwrap(), Literal::Str("42".to_string()));
        assert_eq!(call("to_string", vec![Literal::Float(3.14)]).unwrap(), Literal::Str("3.14".to_string()));
    }

    #[test]
    fn test_scale_linear() {
        let result = call(
            "scale_linear",
            vec![
                Literal::Float(5.0),
                Literal::Float(0.0),
                Literal::Float(10.0),
                Literal::Float(0.0),
                Literal::Float(100.0),
            ],
        )
        .unwrap();
        assert!(matches!(result, Literal::Float(f) if (f - 50.0).abs() < 1e-10));
    }

    #[test]
    fn test_domain_errors() {
        assert!(call("ln", vec![Literal::Float(-1.0)]).is_err());
        assert!(call("log10", vec![Literal::Float(0.0)]).is_err());
        assert!(call("sqrt", vec![Literal::Float(-1.0)]).is_err());
        assert!(call("asin", vec![Literal::Float(2.0)]).is_err());
        assert!(call("mod", vec![Literal::Integer(1), Literal::Integer(0)]).is_err());
    }

    #[test]
    fn test_arity_errors() {
        assert!(call("abs", vec![]).is_err());
        assert!(call("pi", vec![Literal::Integer(1)]).is_err());
        assert!(call("min", vec![]).is_err());
    }
}
