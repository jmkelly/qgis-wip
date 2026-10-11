//! Expression engine for the QGIS Rust port.
//!
//! Replaces Qwt-expression functionality with pure-Rust implementations.
//! See ADR-0001 for the no-FFI policy decision.

/// A parsed expression node.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal numeric value.
    Number(f64),
    /// A literal string value.
    String(String),
    /// A variable reference.
    Variable(String),
    /// Addition of two expressions.
    Add(Box<Expr>, Box<Expr>),
    /// Subtraction of two expressions.
    Sub(Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Evaluates the expression with no variables.
    pub fn eval(&self) -> Result<f64, String> {
        match self {
            Expr::Number(n) => Ok(*n),
            Expr::Variable(name) => Err(format!("Undefined variable: {}", name)),
            Expr::Add(a, b) => Ok(a.eval()? + b.eval()?),
            Expr::Sub(a, b) => Ok(a.eval()? - b.eval()?),
            Expr::String(_) => Err("Cannot evaluate string as number".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_number() {
        assert_eq!(Expr::Number(42.0).eval().unwrap(), 42.0);
    }

    #[test]
    fn eval_add() {
        let expr = Expr::Add(Box::new(Expr::Number(1.0)), Box::new(Expr::Number(2.0)));
        assert_eq!(expr.eval().unwrap(), 3.0);
    }

    #[test]
    fn eval_sub() {
        let expr = Expr::Sub(Box::new(Expr::Number(5.0)), Box::new(Expr::Number(3.0)));
        assert_eq!(expr.eval().unwrap(), 2.0);
    }
}
