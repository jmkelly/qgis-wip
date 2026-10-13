//! Expression function registry.
//!
//! Maps function names to their implementations. Each function receives a
//! slice of evaluated argument literals and returns the result.

pub mod conditional;
pub mod date_array;
pub mod scalar;
pub mod spatial;
pub mod string;

use crate::ast::Literal;
use crate::eval::EvalError;

/// A function implementation: takes evaluated args, returns a literal.
pub type FunctionFn = fn(&[Literal]) -> Result<Literal, EvalError>;

/// Look up a function by name (case-insensitive).
pub fn lookup(name: &str) -> Option<FunctionFn> {
    let lower = name.to_ascii_lowercase();
    string::lookup(&lower)
        .or_else(|| conditional::lookup(&lower))
        .or_else(|| scalar::lookup(&lower))
        .or_else(|| date_array::lookup(&lower))
        .or_else(|| spatial::lookup(&lower))
}

/// Call a function by name with evaluated arguments.
pub fn call(name: &str, args: Vec<Literal>) -> Result<Literal, EvalError> {
    match lookup(name) {
        Some(f) => f(&args),
        None => Err(EvalError::UnknownFunction(name.to_string())),
    }
}

/// Get the expected arity description for a function (for error messages).
pub fn arity_description(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    string::arity(&lower)
        .or_else(|| conditional::arity(&lower))
        .or_else(|| scalar::arity(&lower))
        .or_else(|| date_array::arity(&lower))
        .or_else(|| spatial::arity(&lower))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_functions() {
        assert!(lookup("lower").is_some());
        assert!(lookup("upper").is_some());
        assert!(lookup("concat").is_some());
        assert!(lookup("coalesce").is_some());
        assert!(lookup("if").is_some());
        assert!(lookup("abs").is_some());
        assert!(lookup("sqrt").is_some());
        assert!(lookup("sin").is_some());
        assert!(lookup("pi").is_some());
        assert!(lookup("to_int").is_some());
        assert!(lookup("now").is_some());
        assert!(lookup("year").is_some());
        assert!(lookup("array").is_some());
        assert!(lookup("array_append").is_some());
        assert!(lookup("array_length").is_some());
        assert!(lookup("generate_series").is_some());
        assert!(lookup("map_get").is_some());
    }

    #[test]
    fn lookup_case_insensitive() {
        assert!(lookup("LOWER").is_some());
        assert!(lookup("Concat").is_some());
        assert!(lookup("COALESCE").is_some());
    }

    #[test]
    fn lookup_unknown_function() {
        assert!(lookup("totally_made_up").is_none());
    }

    #[test]
    fn call_string_function() {
        let result = call("lower", vec![Literal::Str("HELLO".to_string())]).unwrap();
        assert_eq!(result, Literal::Str("hello".to_string()));
    }

    #[test]
    fn call_unknown_function_error() {
        assert!(matches!(
            call("nonexistent", vec![]),
            Err(EvalError::UnknownFunction(_))
        ));
    }
}
