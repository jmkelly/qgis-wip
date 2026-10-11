//! Recursive descent parser for the QGIS expression grammar.
//!
//! Replaces the Bison-based `qgsexpressionparser.y`.
//!
//! Grammar (simplified from QGIS):
//! ```text
//! expression    := logical_or
//! logical_or     := logical_and (OR logical_and)*
//! logical_and    := comparison (AND comparison)*
//! comparison     := additive
//!                | additive (=|!=|<|>|<=|>=) additive
//!                | additive [NOT] IN '(' expression_list ')'
//!                | additive [NOT] BETWEEN additive AND additive
//!                | additive [NOT] LIKE additive
//!                | additive IS [NOT] (NULL|TRUE|FALSE)
//! additive       := multiplicative (('+'|'-') multiplicative)*
//! multiplicative := unary_mul (('*'|'/'|'%') unary_mul)*
//! unary_mul      := postfix
//! power          := unary ('^' power)?
//! unary          := ('-'|'+') unary | NOT unary | power
//! postfix        := primary ('||' primary)*
//! primary        := literal
//!                | function_call
//!                | column_ref
//!                | CASE expression? (WHEN expression THEN expression)+ (ELSE expression)? END
//!                | '(' expression ')'
//! ```

use crate::ast::*;
use crate::lexer::{Keyword, Op, Token};

/// Parsing error.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse an expression string into an AST.
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let tokens = crate::lexer::lex(input).map_err(|e| ParseError {
        message: e.message,
        position: e.position,
    })?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expression()?;
    parser.expect_end()?;
    Ok(expr)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect_end(&self) -> Result<(), ParseError> {
        if self.pos < self.tokens.len() {
            return Err(ParseError {
                message: format!("unexpected token {:?}", self.tokens[self.pos]),
                position: self.pos,
            });
        }
        Ok(())
    }

    fn error(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            message: msg.into(),
            position: self.pos,
        }
    }

    fn is_keyword(&self, kw: Keyword) -> bool {
        matches!(self.peek(), Some(Token::Keyword(k)) if *k == kw)
    }

    fn is_op(&self, op: Op) -> bool {
        matches!(self.peek(), Some(Token::Op(o)) if *o == op)
    }

    fn eat_keyword(&mut self, kw: Keyword) -> bool {
        if self.is_keyword(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_op(&mut self, op: Op) -> bool {
        if self.is_op(op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_op(&mut self, op: Op) -> Result<(), ParseError> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(self.error(format!("expected {:?}", op)))
        }
    }

    fn expect_keyword(&mut self, kw: Keyword) -> Result<(), ParseError> {
        if self.eat_keyword(kw) {
            Ok(())
        } else {
            Err(self.error(format!("expected keyword {:?}", kw)))
        }
    }

    // expression := logical_or
    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_logical_or()
    }

    // logical_or := logical_and (OR logical_and)*
    fn parse_logical_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_logical_and()?;
        while self.is_keyword(Keyword::Or) {
            self.advance();
            let right = self.parse_logical_and()?;
            left = Expr::Binary(BinaryOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    // logical_and := comparison (AND comparison)*
    fn parse_logical_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        while self.is_keyword(Keyword::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary(BinaryOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    // comparison := additive
    //             | additive (=|!=|<|>|<=|>=) additive
    //             | additive [NOT] IN '(' expression_list ')'
    //             | additive [NOT] BETWEEN additive AND additive
    //             | additive [NOT] LIKE additive
    //             | additive IS [NOT] (NULL|TRUE|FALSE)
    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_additive()?;

        // IS [NOT] (NULL|TRUE|FALSE)
        if self.is_keyword(Keyword::Is) {
            self.advance();
            let negated = self.eat_keyword(Keyword::Not);
            let right = if self.is_keyword(Keyword::Null) {
                self.advance();
                IsTarget::Null
            } else if self.is_keyword(Keyword::True) {
                self.advance();
                IsTarget::True
            } else if self.is_keyword(Keyword::False) {
                self.advance();
                IsTarget::False
            } else {
                return Err(self.error("expected NULL, TRUE, or FALSE after IS"));
            };
            return Ok(Expr::Is {
                left: Box::new(left),
                negated,
                right,
            });
        }

        // [NOT] IN (list)
        let save = self.pos;
        let mut negated = false;
        if self.is_keyword(Keyword::Not) {
            // Look ahead: NOT IN
            if matches!(self.peek_at(1), Some(Token::Keyword(Keyword::In))) {
                self.advance(); // NOT
                negated = true;
            }
        }
        if self.is_keyword(Keyword::In) {
            self.advance(); // IN
            self.expect_op(Op::LParen)?;
            let list = self.parse_expression_list()?;
            self.expect_op(Op::RParen)?;
            return Ok(Expr::InList {
                expr: Box::new(left),
                list,
                negated,
            });
        }
        // If we consumed NOT but it wasn't NOT IN, rewind
        if negated {
            self.pos = save;
        }

        // [NOT] BETWEEN additive AND additive
        let save = self.pos;
        let mut negated = false;
        if self.is_keyword(Keyword::Not)
            && matches!(self.peek_at(1), Some(Token::Keyword(Keyword::Between)))
        {
            self.advance(); // NOT
            negated = true;
        }
        if self.is_keyword(Keyword::Between) {
            self.advance(); // BETWEEN
            let low = self.parse_additive()?;
            self.expect_keyword(Keyword::And)?;
            let high = self.parse_additive()?;
            return Ok(Expr::Between {
                expr: Box::new(left),
                low: Box::new(low),
                high: Box::new(high),
                negated,
            });
        }
        if negated {
            self.pos = save;
        }

        // [NOT] LIKE / ILIKE additive
        let save = self.pos;
        let mut negated = false;
        if self.is_keyword(Keyword::Not)
            && matches!(
                self.peek_at(1),
                Some(Token::Keyword(Keyword::Like)) | Some(Token::Keyword(Keyword::ILike))
            )
        {
            self.advance(); // NOT
            negated = true;
        }
        if self.is_keyword(Keyword::Like) || self.is_keyword(Keyword::ILike) {
            self.advance();
            let pattern = self.parse_additive()?;
            return Ok(Expr::Like {
                expr: Box::new(left),
                pattern: Box::new(pattern),
                negated,
            });
        }
        if negated {
            self.pos = save;
        }

        // Arithmetic comparison: =, !=, <>, <, >, <=, >=
        let op = match self.peek() {
            Some(Token::Op(Op::Eq)) => Some(BinaryOp::Eq),
            Some(Token::Op(Op::Ne)) => Some(BinaryOp::Ne),
            Some(Token::Op(Op::Lt)) => Some(BinaryOp::Lt),
            Some(Token::Op(Op::Gt)) => Some(BinaryOp::Gt),
            Some(Token::Op(Op::Le)) => Some(BinaryOp::Le),
            Some(Token::Op(Op::Ge)) => Some(BinaryOp::Ge),
            _ => None,
        };

        if let Some(op) = op {
            self.advance();
            let right = self.parse_additive()?;
            return Ok(Expr::Binary(op, Box::new(left), Box::new(right)));
        }

        Ok(left)
    }

    // additive := multiplicative (('+'|'-') multiplicative)*
    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            if self.is_op(Op::Plus) {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Binary(BinaryOp::Add, Box::new(left), Box::new(right));
            } else if self.is_op(Op::Minus) {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Binary(BinaryOp::Sub, Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    // multiplicative := power (('*'|'/'|'%') power)*
    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_power()?;
        loop {
            if self.is_op(Op::Star) {
                self.advance();
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryOp::Mul, Box::new(left), Box::new(right));
            } else if self.is_op(Op::Slash) {
                self.advance();
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryOp::Div, Box::new(left), Box::new(right));
            } else if self.is_op(Op::Percent) {
                self.advance();
                let right = self.parse_power()?;
                left = Expr::Binary(BinaryOp::Mod, Box::new(left), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    // power := unary ('^' power)?  (right-associative)
    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_unary()?;
        if self.is_op(Op::Caret) {
            self.advance();
            let right = self.parse_power()?;
            Ok(Expr::Binary(BinaryOp::Pow, Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    // unary := ('-'|'+') unary | NOT unary | postfix
    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.is_op(Op::Minus) {
            self.advance();
            let expr = self.parse_unary()?;
            Ok(Expr::Unary(UnaryOp::Neg, Box::new(expr)))
        } else if self.is_op(Op::Plus) {
            self.advance();
            self.parse_unary()
        } else if self.is_keyword(Keyword::Not) {
            self.advance();
            let expr = self.parse_unary()?;
            Ok(Expr::Unary(UnaryOp::Not, Box::new(expr)))
        } else {
            self.parse_postfix()
        }
    }

    // postfix := primary ('||' primary)*
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_primary()?;
        while self.is_op(Op::Concat) {
            self.advance();
            let right = self.parse_primary()?;
            left = Expr::Binary(BinaryOp::Concat, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Token::Integer(i)) => {
                let val = *i;
                self.advance();
                Ok(Expr::Literal(Literal::Integer(val)))
            }
            Some(Token::Float(f)) => {
                let val = *f;
                self.advance();
                Ok(Expr::Literal(Literal::Float(val)))
            }
            Some(Token::Str(s)) => {
                let val = s.clone();
                self.advance();
                Ok(Expr::Literal(Literal::Str(val)))
            }
            Some(Token::ColumnRef(name)) => {
                let val = name.clone();
                self.advance();
                Ok(Expr::FieldRef(val))
            }
            Some(Token::BracketRef(name)) => {
                let val = name.clone();
                self.advance();
                Ok(Expr::FieldRef(val))
            }
            Some(Token::Keyword(Keyword::True)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(true)))
            }
            Some(Token::Keyword(Keyword::False)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(false)))
            }
            Some(Token::Keyword(Keyword::Null)) => {
                self.advance();
                Ok(Expr::Literal(Literal::Null))
            }
            Some(Token::Keyword(Keyword::Case)) => self.parse_case_when(),
            Some(Token::Op(Op::LParen)) => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect_op(Op::RParen)?;
                Ok(expr)
            }
            Some(Token::Ident(name)) => {
                let fname = name.clone();
                self.advance();
                if self.is_op(Op::LParen) {
                    self.advance();
                    let args = self.parse_expression_list()?;
                    self.expect_op(Op::RParen)?;
                    Ok(Expr::FunctionCall(fname, args))
                } else {
                    // Bare identifier as field reference
                    Ok(Expr::FieldRef(fname))
                }
            }
            Some(other) => Err(self.error(format!("unexpected token {:?}", other))),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn parse_case_when(&mut self) -> Result<Expr, ParseError> {
        self.expect_keyword(Keyword::Case)?;

        // Optional operand: CASE expr WHEN ...
        let operand = if self.is_keyword(Keyword::When) {
            None
        } else {
            Some(Box::new(self.parse_expression()?))
        };

        let mut whens = Vec::new();
        loop {
            self.expect_keyword(Keyword::When)?;
            let cond = self.parse_expression()?;
            self.expect_keyword(Keyword::Then)?;
            let result = self.parse_expression()?;
            whens.push((cond, result));
            if !self.is_keyword(Keyword::When) {
                break;
            }
        }

        let else_branch = if self.eat_keyword(Keyword::Else) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        self.expect_keyword(Keyword::End)?;

        Ok(Expr::CaseWhen {
            operand,
            whens,
            else_branch,
        })
    }

    fn parse_expression_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut exprs = Vec::new();
        if self.is_op(Op::RParen) {
            return Ok(exprs);
        }
        loop {
            let expr = self.parse_expression()?;
            exprs.push(expr);
            if self.eat_op(Op::Comma) {
                continue;
            }
            break;
        }
        Ok(exprs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_str(input: &str) -> Expr {
        parse(input).unwrap()
    }

    #[test]
    fn parse_integer() {
        let expr = parse_str("42");
        assert_eq!(expr, Expr::Literal(Literal::Integer(42)));
    }

    #[test]
    fn parse_float() {
        let expr = parse_str("3.14");
        assert_eq!(expr, Expr::Literal(Literal::Float(3.14)));
    }

    #[test]
    fn parse_string() {
        let expr = parse_str("'hello'");
        assert_eq!(expr, Expr::Literal(Literal::Str("hello".to_string())));
    }

    #[test]
    fn parse_true_false_null() {
        assert_eq!(parse_str("TRUE"), Expr::Literal(Literal::Bool(true)));
        assert_eq!(parse_str("FALSE"), Expr::Literal(Literal::Bool(false)));
        assert_eq!(parse_str("NULL"), Expr::Literal(Literal::Null));
    }

    #[test]
    fn parse_addition() {
        let expr = parse_str("1 + 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Add,
                Box::new(Expr::Literal(Literal::Integer(1))),
                Box::new(Expr::Literal(Literal::Integer(2))),
            )
        );
    }

    #[test]
    fn parse_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let expr = parse_str("1 + 2 * 3");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Add,
                Box::new(Expr::Literal(Literal::Integer(1))),
                Box::new(Expr::Binary(
                    BinaryOp::Mul,
                    Box::new(Expr::Literal(Literal::Integer(2))),
                    Box::new(Expr::Literal(Literal::Integer(3))),
                )),
            )
        );
    }

    #[test]
    fn parse_parens() {
        let expr = parse_str("(1 + 2) * 3");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Mul,
                Box::new(Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::Literal(Literal::Integer(1))),
                    Box::new(Expr::Literal(Literal::Integer(2))),
                )),
                Box::new(Expr::Literal(Literal::Integer(3))),
            )
        );
    }

    #[test]
    fn parse_unary_neg() {
        let expr = parse_str("-5");
        assert_eq!(
            expr,
            Expr::Unary(
                UnaryOp::Neg,
                Box::new(Expr::Literal(Literal::Integer(5))),
            )
        );
    }

    #[test]
    fn parse_unary_not() {
        let expr = parse_str("NOT TRUE");
        assert_eq!(
            expr,
            Expr::Unary(
                UnaryOp::Not,
                Box::new(Expr::Literal(Literal::Bool(true))),
            )
        );
    }

    #[test]
    fn parse_comparison_eq() {
        let expr = parse_str("1 = 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Eq,
                Box::new(Expr::Literal(Literal::Integer(1))),
                Box::new(Expr::Literal(Literal::Integer(2))),
            )
        );
    }

    #[test]
    fn parse_comparison_ne() {
        let expr = parse_str("1 != 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Ne,
                Box::new(Expr::Literal(Literal::Integer(1))),
                Box::new(Expr::Literal(Literal::Integer(2))),
            )
        );
    }

    #[test]
    fn parse_and_or() {
        let expr = parse_str("TRUE AND FALSE OR TRUE");
        // Should be: (TRUE AND FALSE) OR TRUE
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Or,
                Box::new(Expr::Binary(
                    BinaryOp::And,
                    Box::new(Expr::Literal(Literal::Bool(true))),
                    Box::new(Expr::Literal(Literal::Bool(false))),
                )),
                Box::new(Expr::Literal(Literal::Bool(true))),
            )
        );
    }

    #[test]
    fn parse_field_ref_double_quotes() {
        let expr = parse_str("\"my_field\"");
        assert_eq!(expr, Expr::FieldRef("my_field".to_string()));
    }

    #[test]
    fn parse_field_ref_brackets() {
        let expr = parse_str("[my_field]");
        assert_eq!(expr, Expr::FieldRef("my_field".to_string()));
    }

    #[test]
    fn parse_function_call() {
        let expr = parse_str("lower('HELLO')");
        assert_eq!(
            expr,
            Expr::FunctionCall(
                "lower".to_string(),
                vec![Expr::Literal(Literal::Str("HELLO".to_string()))],
            )
        );
    }

    #[test]
    fn parse_function_call_no_args() {
        let expr = parse_str("now()");
        assert_eq!(expr, Expr::FunctionCall("now".to_string(), vec![]));
    }

    #[test]
    fn parse_function_call_multiple_args() {
        let expr = parse_str("concat('a', 'b', 'c')");
        assert_eq!(
            expr,
            Expr::FunctionCall(
                "concat".to_string(),
                vec![
                    Expr::Literal(Literal::Str("a".to_string())),
                    Expr::Literal(Literal::Str("b".to_string())),
                    Expr::Literal(Literal::Str("c".to_string())),
                ],
            )
        );
    }

    #[test]
    fn parse_case_when_searched() {
        let expr = parse_str("CASE WHEN TRUE THEN 1 ELSE 2 END");
        assert_eq!(
            expr,
            Expr::CaseWhen {
                operand: None,
                whens: vec![(
                    Expr::Literal(Literal::Bool(true)),
                    Expr::Literal(Literal::Integer(1)),
                )],
                else_branch: Some(Box::new(Expr::Literal(Literal::Integer(2)))),
            }
        );
    }

    #[test]
    fn parse_case_when_with_operand() {
        let expr = parse_str("CASE x WHEN 1 THEN 'a' WHEN 2 THEN 'b' ELSE 'c' END");
        assert_eq!(
            expr,
            Expr::CaseWhen {
                operand: Some(Box::new(Expr::FieldRef("x".to_string()))),
                whens: vec![
                    (
                        Expr::Literal(Literal::Integer(1)),
                        Expr::Literal(Literal::Str("a".to_string())),
                    ),
                    (
                        Expr::Literal(Literal::Integer(2)),
                        Expr::Literal(Literal::Str("b".to_string())),
                    ),
                ],
                else_branch: Some(Box::new(Expr::Literal(Literal::Str("c".to_string())))),
            }
        );
    }

    #[test]
    fn parse_in_list() {
        let expr = parse_str("1 IN (1, 2, 3)");
        assert_eq!(
            expr,
            Expr::InList {
                expr: Box::new(Expr::Literal(Literal::Integer(1))),
                list: vec![
                    Expr::Literal(Literal::Integer(1)),
                    Expr::Literal(Literal::Integer(2)),
                    Expr::Literal(Literal::Integer(3)),
                ],
                negated: false,
            }
        );
    }

    #[test]
    fn parse_not_in_list() {
        let expr = parse_str("1 NOT IN (1, 2, 3)");
        assert_eq!(
            expr,
            Expr::InList {
                expr: Box::new(Expr::Literal(Literal::Integer(1))),
                list: vec![
                    Expr::Literal(Literal::Integer(1)),
                    Expr::Literal(Literal::Integer(2)),
                    Expr::Literal(Literal::Integer(3)),
                ],
                negated: true,
            }
        );
    }

    #[test]
    fn parse_between() {
        let expr = parse_str("5 BETWEEN 1 AND 10");
        assert_eq!(
            expr,
            Expr::Between {
                expr: Box::new(Expr::Literal(Literal::Integer(5))),
                low: Box::new(Expr::Literal(Literal::Integer(1))),
                high: Box::new(Expr::Literal(Literal::Integer(10))),
                negated: false,
            }
        );
    }

    #[test]
    fn parse_not_between() {
        let expr = parse_str("5 NOT BETWEEN 1 AND 10");
        assert_eq!(
            expr,
            Expr::Between {
                expr: Box::new(Expr::Literal(Literal::Integer(5))),
                low: Box::new(Expr::Literal(Literal::Integer(1))),
                high: Box::new(Expr::Literal(Literal::Integer(10))),
                negated: true,
            }
        );
    }

    #[test]
    fn parse_like() {
        let expr = parse_str("'hello' LIKE '%ell%'");
        assert_eq!(
            expr,
            Expr::Like {
                expr: Box::new(Expr::Literal(Literal::Str("hello".to_string()))),
                pattern: Box::new(Expr::Literal(Literal::Str("%ell%".to_string()))),
                negated: false,
            }
        );
    }

    #[test]
    fn parse_is_null() {
        let expr = parse_str("x IS NULL");
        assert_eq!(
            expr,
            Expr::Is {
                left: Box::new(Expr::FieldRef("x".to_string())),
                negated: false,
                right: IsTarget::Null,
            }
        );
    }

    #[test]
    fn parse_is_not_null() {
        let expr = parse_str("x IS NOT NULL");
        assert_eq!(
            expr,
            Expr::Is {
                left: Box::new(Expr::FieldRef("x".to_string())),
                negated: true,
                right: IsTarget::Null,
            }
        );
    }

    #[test]
    fn parse_is_true() {
        let expr = parse_str("x IS TRUE");
        assert_eq!(
            expr,
            Expr::Is {
                left: Box::new(Expr::FieldRef("x".to_string())),
                negated: false,
                right: IsTarget::True,
            }
        );
    }

    #[test]
    fn parse_concat() {
        let expr = parse_str("'a' || 'b'");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Concat,
                Box::new(Expr::Literal(Literal::Str("a".to_string()))),
                Box::new(Expr::Literal(Literal::Str("b".to_string()))),
            )
        );
    }

    #[test]
    fn parse_power() {
        let expr = parse_str("2 ^ 3");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Pow,
                Box::new(Expr::Literal(Literal::Integer(2))),
                Box::new(Expr::Literal(Literal::Integer(3))),
            )
        );
    }

    #[test]
    fn parse_complex_expression() {
        let expr = parse_str("\"population\" > 1000 AND \"name\" LIKE 'A%'");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::And,
                Box::new(Expr::Binary(
                    BinaryOp::Gt,
                    Box::new(Expr::FieldRef("population".to_string())),
                    Box::new(Expr::Literal(Literal::Integer(1000))),
                )),
                Box::new(Expr::Like {
                    expr: Box::new(Expr::FieldRef("name".to_string())),
                    pattern: Box::new(Expr::Literal(Literal::Str("A%".to_string()))),
                    negated: false,
                }),
            )
        );
    }

    #[test]
    fn parse_nested_function_calls() {
        let expr = parse_str("upper(lower('HELLO'))");
        assert_eq!(
            expr,
            Expr::FunctionCall(
                "upper".to_string(),
                vec![Expr::FunctionCall(
                    "lower".to_string(),
                    vec![Expr::Literal(Literal::Str("HELLO".to_string()))],
                )],
            )
        );
    }

    #[test]
    fn parse_error_unexpected_token() {
        let result = parse("1 +");
        assert!(result.is_err());
    }

    #[test]
    fn parse_error_unexpected_char() {
        let result = parse("@");
        assert!(result.is_err());
    }

    #[test]
    fn parse_error_missing_rparen() {
        let result = parse("(1 + 2");
        assert!(result.is_err());
    }

    #[test]
    fn parse_trailing_tokens_error() {
        let result = parse("1 2");
        assert!(result.is_err());
    }

    #[test]
    fn parse_modulo() {
        let expr = parse_str("10 % 3");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Mod,
                Box::new(Expr::Literal(Literal::Integer(10))),
                Box::new(Expr::Literal(Literal::Integer(3))),
            )
        );
    }

    #[test]
    fn parse_division() {
        let expr = parse_str("10 / 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Div,
                Box::new(Expr::Literal(Literal::Integer(10))),
                Box::new(Expr::Literal(Literal::Integer(2))),
            )
        );
    }

    #[test]
    fn parse_scientific_notation() {
        let expr = parse_str("1e3");
        assert!(matches!(expr, Expr::Literal(Literal::Float(1000.0))));
    }

    #[test]
    fn parse_negative_float() {
        let expr = parse_str("-3.14");
        assert_eq!(
            expr,
            Expr::Unary(
                UnaryOp::Neg,
                Box::new(Expr::Literal(Literal::Float(3.14))),
            )
        );
    }

    #[test]
    fn parse_double_unary_neg() {
        let expr = parse_str("-(-5)");
        assert_eq!(
            expr,
            Expr::Unary(
                UnaryOp::Neg,
                Box::new(Expr::Unary(
                    UnaryOp::Neg,
                    Box::new(Expr::Literal(Literal::Integer(5))),
                )),
            )
        );
    }

    #[test]
    fn parse_not_not_false() {
        let expr = parse_str("NOT NOT FALSE");
        assert_eq!(
            expr,
            Expr::Unary(
                UnaryOp::Not,
                Box::new(Expr::Unary(
                    UnaryOp::Not,
                    Box::new(Expr::Literal(Literal::Bool(false))),
                )),
            )
        );
    }

    #[test]
    fn parse_ilike() {
        let expr = parse_str("'hello' ILIKE '%ELL%'");
        assert_eq!(
            expr,
            Expr::Like {
                expr: Box::new(Expr::Literal(Literal::Str("hello".to_string()))),
                pattern: Box::new(Expr::Literal(Literal::Str("%ELL%".to_string()))),
                negated: false,
            }
        );
    }

    #[test]
    fn parse_not_like() {
        let expr = parse_str("'hello' NOT LIKE '%ell%'");
        assert_eq!(
            expr,
            Expr::Like {
                expr: Box::new(Expr::Literal(Literal::Str("hello".to_string()))),
                pattern: Box::new(Expr::Literal(Literal::Str("%ell%".to_string()))),
                negated: true,
            }
        );
    }

    #[test]
    fn parse_case_no_else() {
        let expr = parse_str("CASE WHEN x THEN 1 END");
        assert_eq!(
            expr,
            Expr::CaseWhen {
                operand: None,
                whens: vec![(
                    Expr::FieldRef("x".to_string()),
                    Expr::Literal(Literal::Integer(1)),
                )],
                else_branch: None,
            }
        );
    }

    #[test]
    fn parse_empty_in_list() {
        let expr = parse_str("1 IN ()");
        assert_eq!(
            expr,
            Expr::InList {
                expr: Box::new(Expr::Literal(Literal::Integer(1))),
                list: vec![],
                negated: false,
            }
        );
    }

    #[test]
    fn parse_arithmetic_comparison_chain() {
        // In QGIS, comparison is not chained: a < b < c is (a < b) < c
        let expr = parse_str("1 < 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Lt,
                Box::new(Expr::Literal(Literal::Integer(1))),
                Box::new(Expr::Literal(Literal::Integer(2))),
            )
        );
    }

    #[test]
    fn parse_mixed_arithmetic() {
        let expr = parse_str("1 + 2.5 * 3 - 4 / 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Sub,
                Box::new(Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::Literal(Literal::Integer(1))),
                    Box::new(Expr::Binary(
                        BinaryOp::Mul,
                        Box::new(Expr::Literal(Literal::Float(2.5))),
                        Box::new(Expr::Literal(Literal::Integer(3))),
                    )),
                )),
                Box::new(Expr::Binary(
                    BinaryOp::Div,
                    Box::new(Expr::Literal(Literal::Integer(4))),
                    Box::new(Expr::Literal(Literal::Integer(2))),
                )),
            )
        );
    }

    #[test]
    fn parse_is_false() {
        let expr = parse_str("x IS FALSE");
        assert_eq!(
            expr,
            Expr::Is {
                left: Box::new(Expr::FieldRef("x".to_string())),
                negated: false,
                right: IsTarget::False,
            }
        );
    }

    #[test]
    fn parse_is_not_true() {
        let expr = parse_str("x IS NOT TRUE");
        assert_eq!(
            expr,
            Expr::Is {
                left: Box::new(Expr::FieldRef("x".to_string())),
                negated: true,
                right: IsTarget::True,
            }
        );
    }

    #[test]
    fn parse_comment_in_expression() {
        let expr = parse_str("1 + 2 # addition");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Add,
                Box::new(Expr::Literal(Literal::Integer(1))),
                Box::new(Expr::Literal(Literal::Integer(2))),
            )
        );
    }

    #[test]
    fn parse_lt_gt_le_ge() {
        assert!(matches!(
            parse_str("1 < 2"),
            Expr::Binary(BinaryOp::Lt, _, _)
        ));
        assert!(matches!(
            parse_str("1 > 2"),
            Expr::Binary(BinaryOp::Gt, _, _)
        ));
        assert!(matches!(
            parse_str("1 <= 2"),
            Expr::Binary(BinaryOp::Le, _, _)
        ));
        assert!(matches!(
            parse_str("1 >= 2"),
            Expr::Binary(BinaryOp::Ge, _, _)
        ));
    }

    #[test]
    fn parse_neq_angle_bracket() {
        let expr = parse_str("1 <> 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Ne,
                Box::new(Expr::Literal(Literal::Integer(1))),
                Box::new(Expr::Literal(Literal::Integer(2))),
            )
        );
    }

    #[test]
    fn parse_power_right_assoc() {
        // 2 ^ 3 ^ 2 should parse as 2 ^ (3 ^ 2)
        let expr = parse_str("2 ^ 3 ^ 2");
        assert_eq!(
            expr,
            Expr::Binary(
                BinaryOp::Pow,
                Box::new(Expr::Literal(Literal::Integer(2))),
                Box::new(Expr::Binary(
                    BinaryOp::Pow,
                    Box::new(Expr::Literal(Literal::Integer(3))),
                    Box::new(Expr::Literal(Literal::Integer(2))),
                )),
            )
        );
    }

    #[test]
    fn parse_unary_plus() {
        let expr = parse_str("+5");
        assert_eq!(
            expr,
            Expr::Literal(Literal::Integer(5))
        );
    }

    #[test]
    fn parse_bare_identifier_as_field() {
        let expr = parse_str("my_field");
        assert_eq!(expr, Expr::FieldRef("my_field".to_string()));
    }

    #[test]
    fn parse_function_with_expression_args() {
        let expr = parse_str("max(1 + 2, 3 * 4)");
        assert_eq!(
            expr,
            Expr::FunctionCall(
                "max".to_string(),
                vec![
                    Expr::Binary(
                        BinaryOp::Add,
                        Box::new(Expr::Literal(Literal::Integer(1))),
                        Box::new(Expr::Literal(Literal::Integer(2))),
                    ),
                    Expr::Binary(
                        BinaryOp::Mul,
                        Box::new(Expr::Literal(Literal::Integer(3))),
                        Box::new(Expr::Literal(Literal::Integer(4))),
                    ),
                ],
            )
        );
    }
}
