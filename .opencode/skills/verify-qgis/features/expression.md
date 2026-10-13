# Feature map: qgis-expression

## What it does

Expression engine replacing Qwt-expression. Hand-rolled recursive
descent parser (no lalrpop/chumsky), evaluator, and QGIS expression
function library.

## How to verify

```bash
./verify test --crate expression
./verify clippy --crate expression
```

## What proves each feature works

| Feature | Evidence |
|---------|----------|
| Lexer (numbers, strings, identifiers, operators) | `lexer.rs` tests |
| Parser (precedence, parens, function calls) | `parser.rs` tests |
| Double-quoted identifiers → ColumnRef (not Str) | `parser.rs` tests |
| Evaluation (arithmetic, comparison, logic) | `eval.rs` tests |
| QGIS function library (coalesce, if, etc.) | `functions.rs` tests |
| String escaping | `lexer.rs` tests |

## Oracle coverage

26 real QGIS expression test cases extracted from
`tests/src/python/test_qgsexpression.py` live in `fixtures/expressions/`.
These are the source of truth for expression parity.
