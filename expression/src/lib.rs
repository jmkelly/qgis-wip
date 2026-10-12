//! Expression engine for the QGIS Rust port.
//!
//! Pure-Rust replacement for QGIS's Flex/Bison expression parser.
//! See ADR-0001 for the no-FFI policy decision.

pub mod ast;
pub mod eval;
pub mod functions;
pub mod lexer;
pub mod parser;
pub mod regex;

pub use ast::{
    simplify, BinaryOp, Expr, IsTarget, Literal, UnaryOp,
};
pub use eval::{evaluate, Context, EvalError};
pub use lexer::{Keyword, LexError, Op, Token, lex};
pub use parser::{ParseError, parse};

/// Parse, simplify, and evaluate an expression in one step.
pub fn eval_str(input: &str, ctx: &Context) -> Result<Literal, EvalError> {
    let expr = parse_simplified(input).map_err(|e| EvalError::Nested(e.to_string()))?;
    evaluate(&expr, ctx)
}

/// Parse and simplify an expression string in one step.
pub fn parse_simplified(input: &str) -> Result<Expr, ParseError> {
    let expr = parse(input)?;
    Ok(simplify(expr))
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Simplification tests (from test_qgsexpression_cases.toml) ===

    #[test]
    fn simplify_neg_unary_double() {
        let expr = parse("-(-5)").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Integer(5)));
    }

    #[test]
    fn simplify_neg_unary_triple() {
        let expr = parse("-(-(-5))").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Integer(-5)));
    }

    #[test]
    fn simplify_not_false() {
        let expr = parse("NOT false").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(true)));
    }

    #[test]
    fn simplify_not_true() {
        let expr = parse("NOT true").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(false)));
    }

    #[test]
    fn simplify_not_not_false() {
        let expr = parse("NOT NOT false").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(false)));
    }

    #[test]
    fn simplify_addition_folds() {
        let expr = parse("1+2").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Integer(3)));
    }

    #[test]
    fn simplify_arith_parens_fold() {
        let expr = parse("(1 + 2) * 4").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Integer(12)));
    }

    #[test]
    fn simplify_mixed_type_mul() {
        let expr = parse("(1 + 2) * \"asd\"").unwrap();
        let simplified = simplify(expr);
        // (1+2) folds to 3, but 3 * "asd" doesn't fold
        // Note: "asd" is a double-quoted column reference, not a string literal
        assert_eq!(
            simplified,
            Expr::Binary(
                BinaryOp::Mul,
                Box::new(Expr::Literal(Literal::Integer(3))),
                Box::new(Expr::FieldRef("asd".to_string())),
            )
        );
    }

    #[test]
    fn simplify_mixed_type_add() {
        let expr = parse("(1 + \"asd\") * 4").unwrap();
        let simplified = simplify(expr);
        // (1 + "asd") doesn't fold, so it stays parenthesized under *
        // Note: "asd" is a double-quoted column reference, not a string literal
        assert_eq!(
            simplified,
            Expr::Binary(
                BinaryOp::Mul,
                Box::new(Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::Literal(Literal::Integer(1))),
                    Box::new(Expr::FieldRef("asd".to_string())),
                )),
                Box::new(Expr::Literal(Literal::Integer(4))),
            )
        );
    }

    #[test]
    fn simplify_sub_negative_folds() {
        let expr = parse("(1 - (-2)) * 4").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Integer(12)));
    }

    #[test]
    fn simplify_div_mixed_stays() {
        let expr = parse("(1 - (1/'a')) * 4").unwrap();
        let simplified = simplify(expr);
        // 1/'a' doesn't fold, so the whole thing stays
        assert_eq!(
            simplified,
            Expr::Binary(
                BinaryOp::Mul,
                Box::new(Expr::Binary(
                    BinaryOp::Sub,
                    Box::new(Expr::Literal(Literal::Integer(1))),
                    Box::new(Expr::Binary(
                        BinaryOp::Div,
                        Box::new(Expr::Literal(Literal::Integer(1))),
                        Box::new(Expr::Literal(Literal::Str("a".to_string()))),
                    )),
                )),
                Box::new(Expr::Literal(Literal::Integer(4))),
            )
        );
    }

    #[test]
    fn simplify_concat_folds() {
        let expr = parse("'abc' || 'def'").unwrap();
        let simplified = simplify(expr);
        assert_eq!(
            simplified,
            Expr::Literal(Literal::Str("abcdef".to_string()))
        );
    }

    #[test]
    fn simplify_lower_literal_no_fold() {
        let expr = parse("lower('HELLO')").unwrap();
        let simplified = simplify(expr);
        // Function calls are not folded
        assert_eq!(
            simplified,
            Expr::FunctionCall(
                "lower".to_string(),
                vec![Expr::Literal(Literal::Str("HELLO".to_string()))],
            )
        );
    }

    #[test]
    fn simplify_lower_concat_folds() {
        let expr = parse("lower('HEL' || 'LO')").unwrap();
        let simplified = simplify(expr);
        // Concat inside function arg folds first
        assert_eq!(
            simplified,
            Expr::FunctionCall(
                "lower".to_string(),
                vec![Expr::Literal(Literal::Str("HELLO".to_string()))],
            )
        );
    }

    #[test]
    fn simplify_case_when_true() {
        let expr = parse("CASE WHEN true THEN \"field_a\" ELSE \"field_b\" END").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::FieldRef("field_a".to_string()));
    }

    #[test]
    fn simplify_case_when_false_no_else() {
        let expr = parse("CASE WHEN false THEN \"field_a\" END").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Null));
    }

    #[test]
    fn simplify_case_when_multi_branch() {
        let expr =
            parse("CASE WHEN false THEN \"field_a\" WHEN true THEN \"field_c\" ELSE \"field_b\" END")
                .unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::FieldRef("field_c".to_string()));
    }

    #[test]
    fn simplify_in_list_hit() {
        let expr = parse("1 IN (1, 2, 3)").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(true)));
    }

    #[test]
    fn simplify_in_list_miss() {
        let expr = parse("5 IN (1, 2, 3)").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(false)));
    }

    #[test]
    fn simplify_not_in_list_hit() {
        let expr = parse("'a' NOT IN ('b', 'c')").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(true)));
    }

    #[test]
    fn simplify_not_in_list_miss() {
        let expr = parse("'a' NOT IN ('b', 'c', 'a')").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(false)));
    }

    #[test]
    fn simplify_in_list_arith() {
        let expr = parse("6 IN (1 + 1, 2 * 3, \"a\")").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(true)));
    }

    #[test]
    fn simplify_between_parses() {
        let expr = parse("'b' between 'a' AND 'c'").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(true)));
    }

    #[test]
    fn simplify_is_false_parses() {
        let expr = parse("True is False").unwrap();
        let simplified = simplify(expr);
        assert_eq!(simplified, Expr::Literal(Literal::Bool(false)));
    }
}
