//! Unified error type for the expression engine.

use crate::{eval::EvalError, lexer::{LexError, Span}, parser::ParseError};

/// Unified error type for expression parsing and evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionError {
    ParseError { message: String, span: Span },
    TypeError(String),
    DivisionByZero,
    InvalidArgument(String),
    UndefinedVariable(String),
    RuntimeError(String),
}

impl std::fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpressionError::ParseError { message, span } => write!(
                f,
                "Parse Error at bytes {}..{}: {}",
                span.start, span.end, message
            ),
            ExpressionError::TypeError(msg) => write!(f, "Type Error: {}", msg),
            ExpressionError::DivisionByZero => write!(f, "Division by zero"),
            ExpressionError::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
            ExpressionError::UndefinedVariable(msg) => {
                write!(f, "Undefined variable: {}", msg)
            }
            ExpressionError::RuntimeError(msg) => write!(f, "Runtime Error: {}", msg),
        }
    }
}

impl std::error::Error for ExpressionError {}

impl From<ParseError> for ExpressionError {
    fn from(err: ParseError) -> Self {
        ExpressionError::ParseError {
            message: err.message,
            span: err.span,
        }
    }
}

impl From<LexError> for ExpressionError {
    fn from(err: LexError) -> Self {
        ExpressionError::ParseError {
            message: err.message,
            span: err.span,
        }
    }
}

impl From<EvalError> for ExpressionError {
    fn from(err: EvalError) -> Self {
        match err {
            EvalError::TypeError { function, message } => {
                ExpressionError::TypeError(format!("{}: {}", function, message))
            }
            EvalError::DivisionByZero => ExpressionError::DivisionByZero,
            EvalError::UnknownVariable(name) | EvalError::UnknownField(name) => {
                ExpressionError::UndefinedVariable(name)
            }
            EvalError::ArityError { function, .. } => {
                ExpressionError::InvalidArgument(function)
            }
            EvalError::UnknownFunction(name) => {
                ExpressionError::RuntimeError(format!("unknown function: {}", name))
            }
            EvalError::NoGeometry => ExpressionError::RuntimeError("no geometry available".into()),
            EvalError::Nested(msg) => ExpressionError::RuntimeError(msg),
        }
    }
}
