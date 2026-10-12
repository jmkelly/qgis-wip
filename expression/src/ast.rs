//! AST types for the QGIS expression grammar.

use std::fmt;

/// A literal value in an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// NULL literal.
    Null,
    /// Boolean literal.
    Bool(bool),
    /// Integer literal.
    Integer(i64),
    /// Float literal.
    Float(f64),
    /// String literal (without quotes).
    Str(String),
    /// Date/datetime as ISO 8601 string.
    Date(String),
    /// Array of literals.
    Array(Vec<Literal>),
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Null => write!(f, "NULL"),
            Literal::Bool(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Literal::Integer(i) => write!(f, "{}", i),
            Literal::Float(fl) => write!(f, "{}", fl),
            Literal::Str(s) => write!(f, "'{}'", s),
            Literal::Date(s) => write!(f, "date '{}'", s),
            Literal::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
        }
    }
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Pow => "^",
            BinaryOp::Concat => "||",
            BinaryOp::Eq => "=",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "AND",
            BinaryOp::Or => "OR",
        };
        write!(f, "{}", s)
    }
}

/// Unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Neg => write!(f, "-"),
            UnaryOp::Not => write!(f, "NOT"),
        }
    }
}

/// A parsed expression AST node.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal value.
    Literal(Literal),
    /// A field reference (e.g., `"field_name"` or `[field_name]`).
    FieldRef(String),
    /// A function call: name and arguments.
    FunctionCall(String, Vec<Expr>),
    /// Binary operation.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// Unary operation.
    Unary(UnaryOp, Box<Expr>),
    /// CASE WHEN ... THEN ... [ELSE ...] END.
    ///
    /// `operand` is the optional operand for `CASE expr WHEN ...`;
    /// for searched CASE it is `None`.
    CaseWhen {
        operand: Option<Box<Expr>>,
        whens: Vec<(Expr, Expr)>,
        else_branch: Option<Box<Expr>>,
    },
    /// `expr [NOT] IN (list)`.
    InList {
        expr: Box<Expr>,
        list: Vec<Expr>,
        negated: bool,
    },
    /// `expr [NOT] BETWEEN low AND high`.
    Between {
        expr: Box<Expr>,
        low: Box<Expr>,
        high: Box<Expr>,
        negated: bool,
    },
    /// `expr [NOT] LIKE pattern`.
    Like {
        expr: Box<Expr>,
        pattern: Box<Expr>,
        negated: bool,
    },
    /// `expr IS [NOT] value`.
    Is {
        left: Box<Expr>,
        negated: bool,
        right: IsTarget,
    },
}

/// Right-hand side of an IS comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum IsTarget {
    Null,
    True,
    False,
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal(l) => write!(f, "{}", l),
            Expr::FieldRef(name) => write!(f, "\"{}\"", name),
            Expr::FunctionCall(name, args) => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expr::Binary(op, left, right) => write!(f, "{} {} {}", left, op, right),
            Expr::Unary(op, expr) => {
                if *op == UnaryOp::Not {
                    write!(f, "NOT {}", expr)
                } else {
                    write!(f, "{}{}", op, expr)
                }
            }
            Expr::CaseWhen {
                operand,
                whens,
                else_branch,
            } => {
                write!(f, "CASE")?;
                if let Some(op) = operand {
                    write!(f, " {}", op)?;
                }
                for (cond, result) in whens {
                    write!(f, " WHEN {} THEN {}", cond, result)?;
                }
                if let Some(eb) = else_branch {
                    write!(f, " ELSE {}", eb)?;
                }
                write!(f, " END")
            }
            Expr::InList {
                expr,
                list,
                negated,
            } => {
                write!(f, "{} ", expr)?;
                if *negated {
                    write!(f, "NOT ")?;
                }
                write!(f, "IN (")?;
                for (i, item) in list.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            Expr::Between {
                expr,
                low,
                high,
                negated,
            } => {
                write!(f, "{} ", expr)?;
                if *negated {
                    write!(f, "NOT ")?;
                }
                write!(f, "BETWEEN {} AND {}", low, high)
            }
            Expr::Like {
                expr,
                pattern,
                negated,
            } => {
                write!(f, "{} ", expr)?;
                if *negated {
                    write!(f, "NOT ")?;
                }
                write!(f, "LIKE {}", pattern)
            }
            Expr::Is {
                left,
                negated,
                right,
            } => {
                write!(f, "{} IS ", left)?;
                if *negated {
                    write!(f, "NOT ")?;
                }
                let s = match right {
                    IsTarget::Null => "NULL",
                    IsTarget::True => "TRUE",
                    IsTarget::False => "FALSE",
                };
                write!(f, "{}", s)
            }
        }
    }
}

/// Attempts constant folding on an expression tree.
///
/// Returns a new expression where subexpressions consisting entirely of
/// literals have been folded. Non-foldable subexpressions are returned
/// unchanged (but their children are still folded).
pub fn simplify(expr: Expr) -> Expr {
    match expr {
        Expr::Unary(UnaryOp::Neg, inner) => {
            let folded = simplify(*inner);
            match folded {
                Expr::Literal(Literal::Integer(i)) => Expr::Literal(Literal::Integer(-i)),
                Expr::Literal(Literal::Float(fl)) => Expr::Literal(Literal::Float(-fl)),
                other => Expr::Unary(UnaryOp::Neg, Box::new(other)),
            }
        }
        Expr::Unary(UnaryOp::Not, inner) => {
            let folded = simplify(*inner);
            match folded {
                Expr::Literal(Literal::Bool(b)) => Expr::Literal(Literal::Bool(!b)),
                other => Expr::Unary(UnaryOp::Not, Box::new(other)),
            }
        }
        Expr::Binary(op, left, right) => {
            let l = simplify(*left);
            let r = simplify(*right);
            fold_binary(op, l, r)
        }
        Expr::FunctionCall(name, args) => {
            let folded_args: Vec<Expr> = args.into_iter().map(simplify).collect();
            Expr::FunctionCall(name, folded_args)
        }
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            let e = simplify(*expr);
            let l: Vec<Expr> = list.into_iter().map(simplify).collect();
            // Fold if all literals
            if let Expr::Literal(ref lit) = e {
                let mut found = false;
                for item in &l {
                    if let Expr::Literal(ilit) = item {
                        if literals_equal(lit, ilit) {
                            found = true;
                            break;
                        }
                    }
                }
                if found {
                    return Expr::Literal(Literal::Bool(!negated));
                } else if l.iter().all(|x| matches!(x, Expr::Literal(_))) {
                    return Expr::Literal(Literal::Bool(negated));
                }
            }
            Expr::InList {
                expr: Box::new(e),
                list: l,
                negated,
            }
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let e = simplify(*expr);
            let lo = simplify(*low);
            let hi = simplify(*high);
            // Fold if all literals
            if let (Expr::Literal(ref el), Expr::Literal(ref ll), Expr::Literal(ref hl)) =
                (&e, &lo, &hi)
            {
                use std::cmp::Ordering;
                if let Some(Ordering::Greater | Ordering::Equal) = compare_literals(el, ll) {
                    if let Some(Ordering::Less | Ordering::Equal) = compare_literals(el, hl) {
                        return Expr::Literal(Literal::Bool(!negated));
                    }
                }
            }
            Expr::Between {
                expr: Box::new(e),
                low: Box::new(lo),
                high: Box::new(hi),
                negated,
            }
        }
        Expr::Like {
            expr,
            pattern,
            negated,
        } => {
            let e = simplify(*expr);
            let p = simplify(*pattern);
            Expr::Like {
                expr: Box::new(e),
                pattern: Box::new(p),
                negated,
            }
        }
        Expr::Is {
            left,
            negated,
            right,
        } => {
            let l = simplify(*left);
            // Fold IS NULL / IS TRUE / IS FALSE when left is literal
            if let Expr::Literal(ref lit) = l {
                let matches = matches!(
                    (&right, lit),
                    (IsTarget::Null, Literal::Null)
                        | (IsTarget::True, Literal::Bool(true))
                        | (IsTarget::False, Literal::Bool(false))
                );
                if matches {
                    return Expr::Literal(Literal::Bool(!negated));
                }
                // If left is a Bool literal and right is IS TRUE/FALSE, fold to false
                if matches!(lit, Literal::Bool(_)) {
                    if let IsTarget::True | IsTarget::False = right {
                        return Expr::Literal(Literal::Bool(false));
                    }
                }
                // If left is a Bool literal and right is IS NULL, fold to false
                if matches!(lit, Literal::Bool(_)) {
                    if let IsTarget::Null = right {
                        return Expr::Literal(Literal::Bool(false));
                    }
                }
            }
            Expr::Is {
                left: Box::new(l),
                negated,
                right,
            }
        }
        Expr::CaseWhen {
            operand,
            whens,
            else_branch,
        } => {
            let op = operand.map(|o| Box::new(simplify(*o)));
            let mut folded_whens = Vec::new();
            for (cond, result) in whens {
                let c = simplify(cond);
                let r = simplify(result);
                folded_whens.push((c, r));
            }
            let eb = else_branch.map(|e| Box::new(simplify(*e)));
            // Check if all WHEN conditions are literal
            let all_literal = folded_whens
                .iter()
                .all(|(c, _)| matches!(c, Expr::Literal(_)));
            if all_literal {
                for (cond, result) in &folded_whens {
                    if let Expr::Literal(Literal::Bool(true)) = cond {
                        return result.clone();
                    }
                }
                if let Some(eb) = &eb {
                    return (**eb).clone();
                }
                return Expr::Literal(Literal::Null);
            }
            Expr::CaseWhen {
                operand: op,
                whens: folded_whens,
                else_branch: eb,
            }
        }
        other => other,
    }
}

fn fold_binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    if let (Expr::Literal(ref l), Expr::Literal(ref r)) = (&left, &right) {
        if let Some(result) = eval_binary_literal(op, l, r) {
            return Expr::Literal(result);
        }
    }
    Expr::Binary(op, Box::new(left), Box::new(right))
}

fn eval_binary_literal(op: BinaryOp, l: &Literal, r: &Literal) -> Option<Literal> {
    use Literal::*;
    match op {
        BinaryOp::Add => match (l, r) {
            (Integer(a), Integer(b)) => Some(Integer(a + b)),
            (Float(a), Float(b)) => Some(Float(a + b)),
            (Integer(a), Float(b)) => Some(Float(*a as f64 + b)),
            (Float(a), Integer(b)) => Some(Float(a + *b as f64)),
            _ => None,
        },
        BinaryOp::Sub => match (l, r) {
            (Integer(a), Integer(b)) => Some(Integer(a - b)),
            (Float(a), Float(b)) => Some(Float(a - b)),
            (Integer(a), Float(b)) => Some(Float(*a as f64 - b)),
            (Float(a), Integer(b)) => Some(Float(a - *b as f64)),
            _ => None,
        },
        BinaryOp::Mul => match (l, r) {
            (Integer(a), Integer(b)) => Some(Integer(a * b)),
            (Float(a), Float(b)) => Some(Float(a * b)),
            (Integer(a), Float(b)) => Some(Float(*a as f64 * b)),
            (Float(a), Integer(b)) => Some(Float(a * *b as f64)),
            _ => None,
        },
        BinaryOp::Div => match (l, r) {
            (Integer(a), Integer(b)) => {
                if *b != 0 {
                    Some(Float(*a as f64 / *b as f64))
                } else {
                    None
                }
            }
            (Float(a), Float(b)) => Some(Float(a / b)),
            (Integer(a), Float(b)) => Some(Float(*a as f64 / b)),
            (Float(a), Integer(b)) => Some(Float(a / *b as f64)),
            _ => None,
        },
        BinaryOp::Mod => match (l, r) {
            (Integer(a), Integer(b)) => {
                if *b != 0 {
                    Some(Integer(a % b))
                } else {
                    None
                }
            }
            (Float(a), Float(b)) => Some(Float(a % b)),
            (Integer(a), Float(b)) => Some(Float(*a as f64 % b)),
            (Float(a), Integer(b)) => Some(Float(a % *b as f64)),
            _ => None,
        },
        BinaryOp::Pow => match (l, r) {
            (Integer(a), Integer(b)) => {
                if *b >= 0 {
                    Some(Integer(a.pow(*b as u32)))
                } else {
                    Some(Float((*a as f64).powi(*b as i32)))
                }
            }
            (Float(a), Float(b)) => Some(Float(a.powf(*b))),
            (Integer(a), Float(b)) => Some(Float((*a as f64).powf(*b))),
            (Float(a), Integer(b)) => Some(Float(a.powi(*b as i32))),
            _ => None,
        },
        BinaryOp::Concat => match (l, r) {
            (Str(a), Str(b)) => Some(Str(format!("{}{}", a, b))),
            _ => None,
        },
        BinaryOp::Eq => {
            if literals_equal(l, r) {
                Some(Bool(true))
            } else if both_numeric(l, r) || both_string(l, r) {
                Some(Bool(false))
            } else {
                None
            }
        }
        BinaryOp::Ne => {
            if literals_equal(l, r) {
                Some(Bool(false))
            } else if both_numeric(l, r) || both_string(l, r) {
                Some(Bool(true))
            } else {
                None
            }
        }
        BinaryOp::Lt => compare_literals(l, r).map(|c| Bool(c == std::cmp::Ordering::Less)),
        BinaryOp::Gt => compare_literals(l, r).map(|c| Bool(c == std::cmp::Ordering::Greater)),
        BinaryOp::Le => compare_literals(l, r)
            .map(|c| Bool(matches!(c, std::cmp::Ordering::Less | std::cmp::Ordering::Equal))),
        BinaryOp::Ge => compare_literals(l, r)
            .map(|c| Bool(matches!(c, std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))),
        BinaryOp::And => match (l, r) {
            (Bool(a), Bool(b)) => Some(Bool(*a && *b)),
            _ => None,
        },
        BinaryOp::Or => match (l, r) {
            (Bool(a), Bool(b)) => Some(Bool(*a || *b)),
            _ => None,
        },
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

fn both_numeric(l: &Literal, r: &Literal) -> bool {
    matches!(
        (l, r),
        (Literal::Integer(_), Literal::Integer(_))
            | (Literal::Float(_), Literal::Float(_))
            | (Literal::Integer(_), Literal::Float(_))
            | (Literal::Float(_), Literal::Integer(_))
    )
}

fn both_string(l: &Literal, r: &Literal) -> bool {
    matches!((l, r), (Literal::Str(_), Literal::Str(_)))
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
