//! Lexer for the QGIS expression grammar.
//!
//! Tokenizes an input string into a stream of [`Token`]s, stripping whitespace
//! and comments. This replaces the Flex-based `qgsexpressionlexer.l`.

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// An integer literal (e.g., `42`, `-17` — note: unary minus is
    /// handled by the parser, so this is always non-negative).
    Integer(i64),
    /// A floating-point literal (e.g., `3.14`, `1e10`, `.5`).
    Float(f64),
    /// A string literal, single- or double-quoted (e.g., `'hello'`, `"world"`).
    Str(String),
    /// An identifier (function name or unquoted column name).
    Ident(String),
    /// A double-quoted column reference (e.g., `"field_name"`).
    ColumnRef(String),
    /// A bracketed column reference (e.g., `[field_name]`).
    BracketRef(String),
    /// A variable reference (e.g., `@my_var`).
    Variable(String),
    /// A dollar-prefixed variable (e.g., `$geometry`, `$area`).
    DollarVar(String),
    /// A keyword (AND, OR, NOT, etc.).
    Keyword(Keyword),
    /// An operator or punctuation.
    Op(Op),
}

/// A byte range in the original expression source (`start..end`).
///
/// Offsets are UTF-8 byte offsets so they can be used directly to slice the
/// source string or pass to editor diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A token paired with its range in the source expression.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// Keywords recognized by the expression lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    And,
    Or,
    Not,
    In,
    Like,
    ILike,
    Between,
    Is,
    Case,
    When,
    Then,
    Else,
    End,
    True,
    False,
    Null,
}

/// Operators and punctuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Concat,       // ||
    Eq,           // =
    Ne,           // != or <>
    Lt,
    Gt,
    Le,
    Ge,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

/// Lexing error.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    /// UTF-8 byte offset of the first character involved (kept for compatibility).
    pub position: usize,
    pub span: Span,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lex error at bytes {}..{}: {}", self.span.start, self.span.end, self.message)
    }
}

impl std::error::Error for LexError {}

/// Tokenize an expression string.
pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    Ok(lex_spanned(input)?.into_iter().map(|token| token.token).collect())
}

/// Tokenize an expression string while preserving each token's UTF-8 byte span.
pub fn lex_spanned(input: &str) -> Result<Vec<SpannedToken>, LexError> {
    let chars: Vec<char> = input.chars().collect();
    let byte_offsets = char_byte_offsets(&chars);
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let token_start = i;

        // Skip whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Skip line comments (# ...)
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // Numbers
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            let (tok, next) = lex_number(&chars, i)?;
            tokens.push(SpannedToken {
                token: tok,
                span: Span::new(byte_offsets[token_start], byte_offsets[next]),
            });
            i = next;
            continue;
        }

        // String literals (single-quoted) and column refs (double-quoted)
        if c == '\'' {
            let (tok, next) = lex_string(&chars, i, c)?;
            tokens.push(SpannedToken {
                token: tok,
                span: Span::new(byte_offsets[token_start], byte_offsets[next]),
            });
            i = next;
            continue;
        }
        if c == '"' {
            let (tok, next) = lex_string(&chars, i, c)?;
            // Double-quoted identifiers are column references in QGIS
            if let Token::Str(s) = tok {
                tokens.push(SpannedToken {
                    token: Token::ColumnRef(s),
                    span: Span::new(byte_offsets[token_start], byte_offsets[next]),
                });
            }
            i = next;
            continue;
        }

        // Bracketed column reference
        if c == '[' {
            let (tok, next) = lex_bracket_ref(&chars, i)?;
            tokens.push(SpannedToken {
                token: tok,
                span: Span::new(byte_offsets[token_start], byte_offsets[next]),
            });
            i = next;
            continue;
        }

        // Variable reference (@variable)
        if c == '@' {
            let (tok, next) = lex_variable(&chars, i, '@')?;
            tokens.push(SpannedToken {
                token: tok,
                span: Span::new(byte_offsets[token_start], byte_offsets[next]),
            });
            i = next;
            continue;
        }

        // Dollar-prefixed variable ($geometry, $area, etc.)
        if c == '$' {
            let (tok, next) = lex_variable(&chars, i, '$')?;
            tokens.push(SpannedToken {
                token: tok,
                span: Span::new(byte_offsets[token_start], byte_offsets[next]),
            });
            i = next;
            continue;
        }

        // Operators and punctuation
        if let Some((op, next)) = lex_operator(&chars, i) {
            tokens.push(SpannedToken {
                token: Token::Op(op),
                span: Span::new(byte_offsets[token_start], byte_offsets[next]),
            });
            i = next;
            continue;
        }

        // Identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            let (tok, next) = lex_ident(&chars, i);
            tokens.push(SpannedToken {
                token: tok,
                span: Span::new(byte_offsets[token_start], byte_offsets[next]),
            });
            i = next;
            continue;
        }

        return Err(LexError {
            message: format!("unexpected character '{}'", c),
            position: byte_offsets[i],
            span: Span::new(byte_offsets[i], byte_offsets[i + 1]),
        });
    }

    Ok(tokens)
}

fn char_byte_offsets(chars: &[char]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut byte = 0;
    for c in chars {
        offsets.push(byte);
        byte += c.len_utf8();
    }
    offsets.push(byte);
    offsets
}

fn lex_error(chars: &[char], message: String, start: usize, end: usize) -> LexError {
    let offsets = char_byte_offsets(chars);
    let span = Span::new(offsets[start], offsets[end]);
    LexError {
        message,
        position: span.start,
        span,
    }
}

fn lex_number(chars: &[char], start: usize) -> Result<(Token, usize), LexError> {
    let mut i = start;
    let mut has_dot = false;
    let mut has_exp = false;

    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            i += 1;
        } else if c == '.' && !has_dot && !has_exp {
            has_dot = true;
            i += 1;
        } else if (c == 'e' || c == 'E') && !has_exp {
            has_exp = true;
            i += 1;
            if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                i += 1;
            }
        } else {
            break;
        }
    }

    let text: String = chars[start..i].iter().collect();
    if has_dot || has_exp {
        let val: f64 = text.parse().map_err(|_| {
            lex_error(chars, format!("invalid float literal '{}'", text), start, i)
        })?;
        Ok((Token::Float(val), i))
    } else {
        let val: i64 = text.parse().map_err(|_| {
            lex_error(chars, format!("invalid integer literal '{}'", text), start, i)
        })?;
        Ok((Token::Integer(val), i))
    }
}

fn lex_string(chars: &[char], start: usize, quote: char) -> Result<(Token, usize), LexError> {
    let mut i = start + 1; // skip opening quote
    let mut result = String::new();

    while i < chars.len() {
        let c = chars[i];
        if c == quote {
            // Check for escaped quote (doubled)
            if i + 1 < chars.len() && chars[i + 1] == quote {
                result.push(quote);
                i += 2;
                continue;
            }
            return Ok((Token::Str(result), i + 1));
        }
        if c == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            match next {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\\' => result.push('\\'),
                '\'' => result.push('\''),
                '"' => result.push('"'),
                _ => {
                    result.push('\\');
                    result.push(next);
                }
            }
            i += 2;
            continue;
        }
        result.push(c);
        i += 1;
    }

    Err(lex_error(
        chars,
        "unterminated string literal".to_string(),
        start,
        chars.len(),
    ))
}

fn lex_bracket_ref(chars: &[char], start: usize) -> Result<(Token, usize), LexError> {
    let mut i = start + 1; // skip '['
    let mut result = String::new();

    while i < chars.len() {
        let c = chars[i];
        if c == ']' {
            return Ok((Token::BracketRef(result), i + 1));
        }
        result.push(c);
        i += 1;
    }

    Err(lex_error(
        chars,
        "unterminated bracket reference".to_string(),
        start,
        chars.len(),
    ))
}

fn lex_variable(chars: &[char], start: usize, prefix: char) -> Result<(Token, usize), LexError> {
    let mut i = start + 1; // skip prefix character
    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
        i += 1;
    }
    let text: String = chars[start + 1..i].iter().collect();
    if text.is_empty() {
        return Err(lex_error(
            chars,
            format!("expected identifier after '{}'", prefix),
            start,
            i,
        ));
    }
    let tok = if prefix == '@' {
        Token::Variable(text)
    } else {
        Token::DollarVar(text)
    };
    Ok((tok, i))
}

fn lex_operator(chars: &[char], start: usize) -> Option<(Op, usize)> {
    let c = chars[start];
    let next = chars.get(start + 1).copied();

    let op = match (c, next) {
        ('+', _) => (Op::Plus, start + 1),
        ('-', _) => (Op::Minus, start + 1),
        ('*', _) => (Op::Star, start + 1),
        ('/', _) => (Op::Slash, start + 1),
        ('%', _) => (Op::Percent, start + 1),
        ('^', _) => (Op::Caret, start + 1),
        ('|', Some('|')) => (Op::Concat, start + 2),
        ('=', _) => (Op::Eq, start + 1),
        ('!', Some('=')) => (Op::Ne, start + 2),
        ('<', Some('>')) => (Op::Ne, start + 2),
        ('<', Some('=')) => (Op::Le, start + 2),
        ('<', _) => (Op::Lt, start + 1),
        ('>', Some('=')) => (Op::Ge, start + 2),
        ('>', _) => (Op::Gt, start + 1),
        ('(', _) => (Op::LParen, start + 1),
        (')', _) => (Op::RParen, start + 1),
        ('[', _) => (Op::LBracket, start + 1),
        (']', _) => (Op::RBracket, start + 1),
        (',', _) => (Op::Comma, start + 1),
        _ => return None,
    };

    Some(op)
}

fn lex_ident(chars: &[char], start: usize) -> (Token, usize) {
    let mut i = start;
    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
        i += 1;
    }
    let text: String = chars[start..i].iter().collect();
    let lower = text.to_lowercase();

    let keyword = match lower.as_str() {
        "and" => Some(Keyword::And),
        "or" => Some(Keyword::Or),
        "not" => Some(Keyword::Not),
        "in" => Some(Keyword::In),
        "like" => Some(Keyword::Like),
        "ilike" => Some(Keyword::ILike),
        "between" => Some(Keyword::Between),
        "is" => Some(Keyword::Is),
        "case" => Some(Keyword::Case),
        "when" => Some(Keyword::When),
        "then" => Some(Keyword::Then),
        "else" => Some(Keyword::Else),
        "end" => Some(Keyword::End),
        "true" => Some(Keyword::True),
        "false" => Some(Keyword::False),
        "null" => Some(Keyword::Null),
        _ => None,
    };

    match keyword {
        Some(kw) => (Token::Keyword(kw), i),
        None => (Token::Ident(text), i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_integer() {
        let toks = lex("42").unwrap();
        assert_eq!(toks, vec![Token::Integer(42)]);
    }

    #[test]
    fn lex_float() {
        let toks = lex("2.5").unwrap();
        assert_eq!(toks, vec![Token::Float(2.5)]);
    }

    #[test]
    fn lex_scientific() {
        let toks = lex("1e10").unwrap();
        assert_eq!(toks, vec![Token::Float(1e10)]);
    }

    #[test]
    fn lex_string_single() {
        let toks = lex("'hello'").unwrap();
        assert_eq!(toks, vec![Token::Str("hello".to_string())]);
    }

    #[test]
    fn lex_string_double() {
        // Double-quoted identifiers are column references in QGIS
        let toks = lex("\"world\"").unwrap();
        assert_eq!(toks, vec![Token::ColumnRef("world".to_string())]);
    }

    #[test]
    fn lex_string_escape() {
        let toks = lex(r"'a\'b'").unwrap();
        assert_eq!(toks, vec![Token::Str("a'b".to_string())]);
    }

    #[test]
    fn lex_column_ref_double() {
        // In QGIS, double-quoted identifiers are column refs.
        let toks = lex("\"field_name\"").unwrap();
        assert_eq!(
            toks,
            vec![Token::ColumnRef("field_name".to_string())]
        );
    }

    #[test]
    fn lex_column_ref_bracket() {
        let toks = lex("[field_name]").unwrap();
        assert_eq!(
            toks,
            vec![Token::BracketRef("field_name".to_string())]
        );
    }

    #[test]
    fn lex_keywords() {
        let toks = lex("AND OR NOT IN LIKE").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Keyword(Keyword::And),
                Token::Keyword(Keyword::Or),
                Token::Keyword(Keyword::Not),
                Token::Keyword(Keyword::In),
                Token::Keyword(Keyword::Like),
            ]
        );
    }

    #[test]
    fn lex_case_keywords() {
        let toks = lex("CASE WHEN THEN ELSE END").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Keyword(Keyword::Case),
                Token::Keyword(Keyword::When),
                Token::Keyword(Keyword::Then),
                Token::Keyword(Keyword::Else),
                Token::Keyword(Keyword::End),
            ]
        );
    }

    #[test]
    fn lex_boolean_keywords() {
        let toks = lex("TRUE FALSE NULL").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Keyword(Keyword::True),
                Token::Keyword(Keyword::False),
                Token::Keyword(Keyword::Null),
            ]
        );
    }

    #[test]
    fn lex_operators() {
        let toks = lex("+ - * / % ^ || = != <> < > <= >=").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Op(Op::Plus),
                Token::Op(Op::Minus),
                Token::Op(Op::Star),
                Token::Op(Op::Slash),
                Token::Op(Op::Percent),
                Token::Op(Op::Caret),
                Token::Op(Op::Concat),
                Token::Op(Op::Eq),
                Token::Op(Op::Ne),
                Token::Op(Op::Ne),
                Token::Op(Op::Lt),
                Token::Op(Op::Gt),
                Token::Op(Op::Le),
                Token::Op(Op::Ge),
            ]
        );
    }

    #[test]
    fn lex_parens_comma() {
        let toks = lex("(a, b)").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Op(Op::LParen),
                Token::Ident("a".to_string()),
                Token::Op(Op::Comma),
                Token::Ident("b".to_string()),
                Token::Op(Op::RParen),
            ]
        );
    }

    #[test]
    fn lex_comment() {
        let toks = lex("42 # this is a comment\n+ 1").unwrap();
        assert_eq!(
            toks,
            vec![Token::Integer(42), Token::Op(Op::Plus), Token::Integer(1)]
        );
    }

    #[test]
    fn lex_function_call() {
        let toks = lex("lower('HELLO')").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Ident("lower".to_string()),
                Token::Op(Op::LParen),
                Token::Str("HELLO".to_string()),
                Token::Op(Op::RParen),
            ]
        );
    }

    #[test]
    fn lex_error_unexpected_char() {
        let result = lex("@");
        assert!(result.is_err());
    }

    #[test]
    fn lex_error_unterminated_string() {
        let result = lex("'hello");
        assert!(result.is_err());
    }

    #[test]
    fn lex_ident_with_underscore() {
        let toks = lex("_my_var").unwrap();
        assert_eq!(toks, vec![Token::Ident("_my_var".to_string())]);
    }

    #[test]
    fn lex_ident_with_digits() {
        let toks = lex("var123").unwrap();
        assert_eq!(toks, vec![Token::Ident("var123".to_string())]);
    }

    #[test]
    fn lex_variable_at() {
        let toks = lex("@my_var").unwrap();
        assert_eq!(toks, vec![Token::Variable("my_var".to_string())]);
    }

    #[test]
    fn lex_variable_dollar() {
        let toks = lex("$geometry").unwrap();
        assert_eq!(toks, vec![Token::DollarVar("geometry".to_string())]);
    }

    #[test]
    fn lex_variable_dollar_area() {
        let toks = lex("$area").unwrap();
        assert_eq!(toks, vec![Token::DollarVar("area".to_string())]);
    }

    #[test]
    fn lex_variable_at_with_underscore() {
        let toks = lex("@_private").unwrap();
        assert_eq!(toks, vec![Token::Variable("_private".to_string())]);
    }

    #[test]
    fn lex_variable_at_with_digits() {
        let toks = lex("@var123").unwrap();
        assert_eq!(toks, vec![Token::Variable("var123".to_string())]);
    }

    #[test]
    fn lex_error_at_no_ident() {
        let result = lex("@");
        assert!(result.is_err());
    }

    #[test]
    fn lex_error_dollar_no_ident() {
        let result = lex("$");
        assert!(result.is_err());
    }

    #[test]
    fn lex_variable_in_expression() {
        let toks = lex("@var + 1").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Variable("var".to_string()),
                Token::Op(Op::Plus),
                Token::Integer(1),
            ]
        );
    }

    #[test]
    fn lex_dollar_in_expression() {
        let toks = lex("$geometry IS NOT NULL").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::DollarVar("geometry".to_string()),
                Token::Keyword(Keyword::Is),
                Token::Keyword(Keyword::Not),
                Token::Keyword(Keyword::Null),
            ]
        );
    }

    #[test]
    fn lex_empty() {
        let toks = lex("").unwrap();
        assert!(toks.is_empty());
    }

    #[test]
    fn lex_whitespace_only() {
        let toks = lex("   \t\n  ").unwrap();
        assert!(toks.is_empty());
    }

    #[test]
    fn lex_spanned_tokens_use_utf8_byte_offsets() {
        let tokens = lex_spanned("é + 2").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 2));
        assert_eq!(tokens[1].span, Span::new(3, 4));
        assert_eq!(tokens[2].span, Span::new(5, 6));
    }

    #[test]
    fn lex_errors_include_utf8_byte_span() {
        let error = lex("é @").unwrap_err();
        assert_eq!(error.position, 3);
        assert_eq!(error.span, Span::new(3, 4));
        assert!(error.to_string().contains("bytes 3..4"));
    }

    #[test]
    fn unterminated_string_error_spans_to_end_of_source() {
        let error = lex("'abc").unwrap_err();
        assert_eq!(error.span, Span::new(0, 4));
    }
}
