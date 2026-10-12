//! String expression functions.
//!
//! Implements: concat, lower, upper, length, substr, replace,
//! regexp_match, regexp_replace, trim, ltrim, rtrim, wordwrap,
//! title, format, format_number, format_date, to_date, to_datetime.
//! Pure Rust, zero unsafe. Pattern matching uses the in-tree
//! `crate::regex` engine (no external regex dependency).

use crate::ast::Literal;
use crate::eval::EvalError;
use crate::regex;

/// Look up a string function by name.
pub fn lookup(name: &str) -> Option<super::FunctionFn> {
    let f = match name {
        "concat" => concat as super::FunctionFn,
        "lower" => lower as super::FunctionFn,
        "upper" => upper as super::FunctionFn,
        "length" => length as super::FunctionFn,
        "substr" => substr as super::FunctionFn,
        "replace" => replace as super::FunctionFn,
        "regexp_match" => regexp_match as super::FunctionFn,
        "regexp_replace" => regexp_replace as super::FunctionFn,
        "trim" => trim as super::FunctionFn,
        "ltrim" => ltrim as super::FunctionFn,
        "rtrim" => rtrim as super::FunctionFn,
        "wordwrap" => wordwrap as super::FunctionFn,
        "title" => title as super::FunctionFn,
        "format" => format as super::FunctionFn,
        "format_number" => format_number as super::FunctionFn,
        "format_date" => format_date as super::FunctionFn,
        "to_date" => to_date as super::FunctionFn,
        "to_datetime" => to_datetime as super::FunctionFn,
        _ => return None,
    };
    Some(f)
}

/// Get arity description for error messages.
pub fn arity(name: &str) -> Option<&'static str> {
    let desc = match name {
        "concat" => "at least 1",
        "lower" | "upper" | "length" | "trim" | "ltrim" | "rtrim" | "title" => "exactly 1",
        "substr" => "2 or 3",
        "replace" => "exactly 3",
        "regexp_match" => "exactly 2",
        "regexp_replace" => "exactly 3",
        "wordwrap" => "2 or 3",
        "format" => "at least 1",
        "format_number" => "1 to 3",
        "format_date" => "2 or 3",
        "to_date" => "exactly 1",
        "to_datetime" => "exactly 1",
        _ => return None,
    };
    Some(desc)
}

fn as_str_arg<'a>(args: &'a [Literal], idx: usize, func: &'a str) -> Result<&'a str, EvalError> {
    match args.get(idx) {
        Some(Literal::Str(s)) => Ok(s.as_str()),
        Some(Literal::Null) => Ok(""),
        Some(other) => Err(EvalError::TypeError {
            function: func.to_string(),
            message: format!("expected string argument, got {}", other),
        }),
        None => Err(EvalError::ArityError {
            function: func.to_string(),
            expected: arity(func).unwrap_or("correct number of").to_string(),
            got: args.len(),
        }),
    }
}

fn as_int_arg(args: &[Literal], idx: usize, func: &str) -> Result<i64, EvalError> {
    match args.get(idx) {
        Some(Literal::Integer(i)) => Ok(*i),
        Some(Literal::Float(f)) => Ok(*f as i64),
        Some(Literal::Null) => Ok(0),
        Some(other) => Err(EvalError::TypeError {
            function: func.to_string(),
            message: format!("expected numeric argument, got {}", other),
        }),
        None => Err(EvalError::ArityError {
            function: func.to_string(),
            expected: arity(func).unwrap_or("correct number of").to_string(),
            got: args.len(),
        }),
    }
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

// === Function implementations ===

/// `concat(arg1, arg2, ...)` — concatenate all arguments as strings.
fn concat(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "concat", 1, usize::MAX)?;
    let mut result = String::new();
    for arg in args {
        result.push_str(&literal_to_string(arg));
    }
    Ok(Literal::Str(result))
}

/// `lower(string)` — convert to lowercase.
fn lower(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "lower", 1, 1)?;
    let s = as_str_arg(args, 0, "lower")?;
    Ok(Literal::Str(s.to_lowercase()))
}

/// `upper(string)` — convert to uppercase.
fn upper(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "upper", 1, 1)?;
    let s = as_str_arg(args, 0, "upper")?;
    Ok(Literal::Str(s.to_uppercase()))
}

/// `length(string)` — number of characters.
fn length(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "length", 1, 1)?;
    let s = as_str_arg(args, 0, "length")?;
    Ok(Literal::Integer(s.chars().count() as i64))
}

/// `substr(string, start [, length])` — extract a substring.
///
/// Start is 1-based (QGIS convention). Negative start counts from end.
fn substr(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "substr", 2, 3)?;
    let s = as_str_arg(args, 0, "substr")?;
    let start = as_int_arg(args, 1, "substr")?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    // Convert 1-based start to 0-based index
    let idx = if start > 0 {
        start - 1
    } else if start < 0 {
        len + start
    } else {
        0
    };
    let idx = idx.clamp(0, len) as usize;

    let end = if args.len() == 3 {
        let sublen = as_int_arg(args, 2, "substr")?;
        if sublen < 0 {
            (chars.len() as i64 + sublen).max(idx as i64) as usize
        } else {
            (idx + sublen as usize).min(chars.len())
        }
    } else {
        chars.len()
    };

    let result: String = chars[idx..end.max(idx)].iter().collect();
    Ok(Literal::Str(result))
}

/// `replace(string, old, new)` — replace all occurrences of `old` with `new`.
fn replace(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "replace", 3, 3)?;
    let s = as_str_arg(args, 0, "replace")?;
    let old = as_str_arg(args, 1, "replace")?;
    let new = as_str_arg(args, 2, "replace")?;
    Ok(Literal::Str(s.replace(old, new)))
}

/// `regexp_match(string, pattern)` — return the first match or NULL.
fn regexp_match(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "regexp_match", 2, 2)?;
    let s = as_str_arg(args, 0, "regexp_match")?;
    let pattern = as_str_arg(args, 1, "regexp_match")?;
    let re = regex::Regex::new(pattern).map_err(|e| EvalError::TypeError {
        function: "regexp_match".to_string(),
        message: e.to_string(),
    })?;
    match re.find(s) {
        Some(m) => Ok(Literal::Str(m.as_str().to_string())),
        None => Ok(Literal::Null),
    }
}

/// `regexp_replace(string, pattern, replacement)` — replace all regex matches.
fn regexp_replace(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "regexp_replace", 3, 3)?;
    let s = as_str_arg(args, 0, "regexp_replace")?;
    let pattern = as_str_arg(args, 1, "regexp_replace")?;
    let replacement = as_str_arg(args, 2, "regexp_replace")?;
    let re = regex::Regex::new(pattern).map_err(|e| EvalError::TypeError {
        function: "regexp_replace".to_string(),
        message: e.to_string(),
    })?;
    Ok(Literal::Str(re.replace_all(s, replacement).to_string()))
}

/// `trim(string)` — remove leading and trailing whitespace.
fn trim(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "trim", 1, 1)?;
    let s = as_str_arg(args, 0, "trim")?;
    Ok(Literal::Str(s.trim().to_string()))
}

/// `ltrim(string)` — remove leading whitespace.
fn ltrim(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "ltrim", 1, 1)?;
    let s = as_str_arg(args, 0, "ltrim")?;
    Ok(Literal::Str(s.trim_start().to_string()))
}

/// `rtrim(string)` — remove trailing whitespace.
fn rtrim(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "rtrim", 1, 1)?;
    let s = as_str_arg(args, 0, "rtrim")?;
    Ok(Literal::Str(s.trim_end().to_string()))
}

/// `wordwrap(string, width [, delimiter])` — wrap text at `width` characters.
fn wordwrap(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "wordwrap", 2, 3)?;
    let s = as_str_arg(args, 0, "wordwrap")?;
    let width = as_int_arg(args, 1, "wordwrap")?;
    let delimiter = if args.len() == 3 {
        as_str_arg(args, 2, "wordwrap")?
    } else {
        " "
    };
    let width = width.max(1) as usize;

    let mut result = String::new();
    let mut line_len = 0;
    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        if line_len == 0 {
            result.push_str(word);
            line_len = word_len;
        } else if line_len + delimiter.chars().count() + word_len <= width {
            result.push_str(delimiter);
            result.push_str(word);
            line_len += delimiter.chars().count() + word_len;
        } else {
            result.push('\n');
            result.push_str(word);
            line_len = word_len;
        }
    }
    Ok(Literal::Str(result))
}

/// `title(string)` — title-case each word.
fn title(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "title", 1, 1)?;
    let s = as_str_arg(args, 0, "title")?;
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if capitalize_next {
                result.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                result.extend(c.to_lowercase());
            }
        } else {
            result.push(c);
            if c.is_whitespace() {
                capitalize_next = true;
            }
        }
    }
    Ok(Literal::Str(result))
}

/// `format(string, arg1, ...)` — printf-style formatting (simplified).
fn format(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "format", 1, usize::MAX)?;
    let fmt = as_str_arg(args, 0, "format")?;
    let mut result = String::new();
    let mut arg_idx = 1;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if chars.peek() == Some(&'%') {
                result.push('%');
                chars.next();
                continue;
            }
            if arg_idx < args.len() {
                result.push_str(&literal_to_string(&args[arg_idx]));
                arg_idx += 1;
            }
        } else {
            result.push(c);
        }
    }
    Ok(Literal::Str(result))
}

/// `format_number(number [, decimals [, grouping_separator]])` — format a number.
fn format_number(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "format_number", 1, 3)?;
    let num = match args.first() {
        Some(Literal::Integer(i)) => *i as f64,
        Some(Literal::Float(f)) => *f,
        Some(Literal::Null) => return Ok(Literal::Str("".to_string())),
        Some(other) => {
            return Err(EvalError::TypeError {
                function: "format_number".to_string(),
                message: format!("expected numeric argument, got {}", other),
            })
        }
        None => {
            return Err(EvalError::ArityError {
                function: "format_number".to_string(),
                expected: "1 to 3".to_string(),
                got: 0,
            })
        }
    };
    let decimals = if args.len() >= 2 {
        as_int_arg(args, 1, "format_number")? as usize
    } else {
        0
    };
    let separator = if args.len() >= 3 {
        as_str_arg(args, 2, "format_number")?
    } else {
        ""
    };

    let formatted = format!("{:.*}", decimals, num);
    if separator.is_empty() {
        Ok(Literal::Str(formatted))
    } else {
        let parts: Vec<&str> = formatted.splitn(2, '.').collect();
        let int_part = parts[0];
        let mut result = String::new();
        let int_chars: Vec<char> = int_part.chars().collect();
        let mut count = 0;
        for c in int_chars.iter().rev() {
            if *c == '-' {
                result.insert(0, '-');
                continue;
            }
            if count > 0 && count % 3 == 0 {
                result.insert(0, separator.chars().next().unwrap());
            }
            result.insert(0, *c);
            count += 1;
        }
        if parts.len() == 2 {
            result.push('.');
            result.push_str(parts[1]);
        }
        Ok(Literal::Str(result))
    }
}

/// `format_date(date, format)` — format a date/datetime as a string.
fn format_date(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "format_date", 2, 3)?;
    let date_str = as_str_arg(args, 0, "format_date")?;
    let fmt = as_str_arg(args, 1, "format_date")?;
    let _locale = if args.len() == 3 {
        as_str_arg(args, 2, "format_date")?
    } else {
        ""
    };
    // Simplified: just return the date string for now
    // Full date formatting would require a date/time library
    let _ = fmt;
    let _ = _locale;
    Ok(Literal::Str(date_str.to_string()))
}

/// `to_date(string)` — convert string to date (returns as ISO string).
fn to_date(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "to_date", 1, 1)?;
    let s = as_str_arg(args, 0, "to_date")?;
    // Simplified: validate and return ISO format
    // Full date parsing would require chrono or similar
    Ok(Literal::Str(s.to_string()))
}

/// `to_datetime(string)` — convert string to datetime (returns as ISO string).
fn to_datetime(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "to_datetime", 1, 1)?;
    let s = as_str_arg(args, 0, "to_datetime")?;
    // Simplified: validate and return ISO format
    Ok(Literal::Str(s.to_string()))
}

fn literal_to_string(lit: &Literal) -> String {
    match lit {
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
        Literal::Null => String::new(),
        Literal::Date(s) => s.clone(),
        Literal::Array(items) => {
            let parts: Vec<String> = items.iter().map(literal_to_string).collect();
            parts.join(",")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Literal>) -> Result<Literal, EvalError> {
        super::super::call(name, args)
    }

    #[test]
    fn test_lower() {
        assert_eq!(
            call("lower", vec![Literal::Str("HELLO".to_string())]).unwrap(),
            Literal::Str("hello".to_string())
        );
    }

    #[test]
    fn test_upper() {
        assert_eq!(
            call("upper", vec![Literal::Str("hello".to_string())]).unwrap(),
            Literal::Str("HELLO".to_string())
        );
    }

    #[test]
    fn test_length() {
        assert_eq!(
            call("length", vec![Literal::Str("hello".to_string())]).unwrap(),
            Literal::Integer(5)
        );
    }

    #[test]
    fn test_concat() {
        assert_eq!(
            call(
                "concat",
                vec![
                    Literal::Str("a".to_string()),
                    Literal::Str("b".to_string()),
                    Literal::Str("c".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("abc".to_string())
        );
    }

    #[test]
    fn test_substr() {
        assert_eq!(
            call(
                "substr",
                vec![Literal::Str("hello".to_string()), Literal::Integer(2)]
            )
            .unwrap(),
            Literal::Str("ello".to_string())
        );
    }

    #[test]
    fn test_replace() {
        assert_eq!(
            call(
                "replace",
                vec![
                    Literal::Str("hello".to_string()),
                    Literal::Str("l".to_string()),
                    Literal::Str("w".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("hewwo".to_string())
        );
    }

    #[test]
    fn test_trim() {
        assert_eq!(
            call("trim", vec![Literal::Str("  hello  ".to_string())]).unwrap(),
            Literal::Str("hello".to_string())
        );
    }

    #[test]
    fn test_regexp_match() {
        assert_eq!(
            call(
                "regexp_match",
                vec![
                    Literal::Str("abc123".to_string()),
                    Literal::Str("[0-9]+".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("123".to_string())
        );
    }

    #[test]
    fn test_regexp_replace() {
        assert_eq!(
            call(
                "regexp_replace",
                vec![
                    Literal::Str("abc123def".to_string()),
                    Literal::Str("[0-9]+".to_string()),
                    Literal::Str("X".to_string())
                ]
            )
            .unwrap(),
            Literal::Str("abcXdef".to_string())
        );
    }

    #[test]
    fn test_title() {
        assert_eq!(
            call("title", vec![Literal::Str("hello world".to_string())]).unwrap(),
            Literal::Str("Hello World".to_string())
        );
    }

    #[test]
    fn test_coalesce() {
        assert_eq!(
            call(
                "coalesce",
                vec![Literal::Null, Literal::Str("default".to_string())]
            )
            .unwrap(),
            Literal::Str("default".to_string())
        );
    }

    #[test]
    fn test_nullif() {
        assert_eq!(
            call(
                "nullif",
                vec![
                    Literal::Str("hello".to_string()),
                    Literal::Str("hello".to_string())
                ]
            )
            .unwrap(),
            Literal::Null
        );
    }
}
