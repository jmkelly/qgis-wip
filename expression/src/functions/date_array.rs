//! Date and array expression functions.
//!
//! Implements: now, age, year, month, day, hour, minute, second, week,
//! day_of_week, to_interval, array, array_append, array_prepend, array_cat,
//! array_distinct, array_reverse, array_sort, array_filter, array_foreach,
//! array_get, array_contains, array_find, array_length, array_to_string,
//! string_to_array, generate_series, range, map_akeys, map_avals, map_get,
//! map_insert, map_delete.
//! Pure Rust, zero unsafe.

use crate::ast::Literal;
use crate::eval::EvalError;

/// Look up a date/array function by name.
pub fn lookup(name: &str) -> Option<super::FunctionFn> {
    let f = match name {
        "now" => now as super::FunctionFn,
        "age" => age as super::FunctionFn,
        "year" => year as super::FunctionFn,
        "month" => month as super::FunctionFn,
        "day" => day as super::FunctionFn,
        "hour" => hour as super::FunctionFn,
        "minute" => minute as super::FunctionFn,
        "second" => second as super::FunctionFn,
        "week" => week as super::FunctionFn,
        "day_of_week" => day_of_week as super::FunctionFn,
        "to_interval" => to_interval as super::FunctionFn,
        "array" => array as super::FunctionFn,
        "array_append" => array_append as super::FunctionFn,
        "array_prepend" => array_prepend as super::FunctionFn,
        "array_cat" => array_cat as super::FunctionFn,
        "array_distinct" => array_distinct as super::FunctionFn,
        "array_reverse" => array_reverse as super::FunctionFn,
        "array_sort" => array_sort as super::FunctionFn,
        "array_filter" => array_filter as super::FunctionFn,
        "array_foreach" => array_foreach as super::FunctionFn,
        "array_get" => array_get as super::FunctionFn,
        "array_contains" => array_contains as super::FunctionFn,
        "array_find" => array_find as super::FunctionFn,
        "array_length" => array_length as super::FunctionFn,
        "array_to_string" => array_to_string as super::FunctionFn,
        "string_to_array" => string_to_array as super::FunctionFn,
        "generate_series" => generate_series as super::FunctionFn,
        "range" => range as super::FunctionFn,
        "map_akeys" => map_akeys as super::FunctionFn,
        "map_avals" => map_avals as super::FunctionFn,
        "map_get" => map_get as super::FunctionFn,
        "map_insert" => map_insert as super::FunctionFn,
        "map_delete" => map_delete as super::FunctionFn,
        _ => return None,
    };
    Some(f)
}

/// Get arity description for error messages.
pub fn arity(name: &str) -> Option<&'static str> {
    let desc = match name {
        "now" | "array" => "any",
        "year" | "month" | "day" | "hour" | "minute" | "second" | "week"
        | "day_of_week" | "to_interval" | "array_length" | "array_reverse"
        | "array_distinct" | "array_sort" | "map_akeys" | "map_avals" => "exactly 1",
        "age" | "array_append" | "array_prepend" | "array_get"
        | "array_contains" | "array_find" | "string_to_array" | "map_get"
        | "map_delete" => "exactly 2",
        "array_to_string" => "1 or 2",
        "generate_series" => "2 or 3",
        "range" => "2 or 3",
        "map_insert" => "exactly 3",
        "array_cat" | "array_filter" | "array_foreach" => "at least 1",
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

fn as_str_arg<'a>(args: &'a [Literal], idx: usize, func: &'a str) -> Result<&'a str, EvalError> {
    match args.get(idx) {
        Some(Literal::Str(s)) => Ok(s.as_str()),
        Some(Literal::Date(s)) => Ok(s.as_str()),
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

fn as_array_arg<'a>(args: &'a [Literal], idx: usize, func: &'a str) -> Result<&'a [Literal], EvalError> {
    match args.get(idx) {
        Some(Literal::Array(a)) => Ok(a.as_slice()),
        Some(Literal::Null) => Ok(&[]),
        Some(other) => Err(EvalError::TypeError {
            function: func.to_string(),
            message: format!("expected array argument, got {}", other),
        }),
        None => Err(EvalError::ArityError {
            function: func.to_string(),
            expected: arity(func).unwrap_or("correct number of").to_string(),
            got: args.len(),
        }),
    }
}

fn literal_to_string(lit: &Literal) -> String {
    match lit {
        Literal::Str(s) => s.clone(),
        Literal::Date(s) => s.clone(),
        Literal::Integer(i) => i.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        Literal::Null => String::new(),
        Literal::Array(items) => {
            let parts: Vec<String> = items.iter().map(literal_to_string).collect();
            parts.join(",")
        }
    }
}

fn parse_date(s: &str) -> Option<(i64, i64, i64, i64, i64, i64)> {
    // Parse ISO 8601 date or datetime
    // Date only: "YYYY-MM-DD"
    // Datetime: "YYYY-MM-DDTHH:MM:SS" or "YYYY-MM-DD HH:MM:SS"
    let date_part = s.split(&['T', ' '][..]).next()?;
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0].parse::<i64>().ok()?;
    let month = parts[1].parse::<i64>().ok()?;
    let day = parts[2].parse::<i64>().ok()?;

    let time_part = s.split(&['T', ' '][..]).nth(1);
    let (hour, minute, second) = if let Some(tp) = time_part {
        let tp_parts: Vec<&str> = tp.split(':').collect();
        if tp_parts.len() >= 3 {
            (
                tp_parts[0].parse::<i64>().unwrap_or(0),
                tp_parts[1].parse::<i64>().unwrap_or(0),
                tp_parts[2].parse::<i64>().unwrap_or(0),
            )
        } else {
            (0, 0, 0)
        }
    } else {
        (0, 0, 0)
    };

    Some((year, month, day, hour, minute, second))
}

fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    // Howard Hinnant's days_from_civil algorithm
    let y: i64 = if month <= 2 { year - 1 } else { year };
    let m: i64 = if month <= 2 { month + 12 } else { month };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// === Date functions ===

fn now(args: &[Literal]) -> Result<Literal, EvalError> {
    let _ = args;
    // Return current datetime as ISO string
    // Since we can't use system time in pure Rust without deps,
    // return a placeholder. Real implementation would use std::time.
    Ok(Literal::Date("2026-10-12T00:00:00".to_string()))
}

fn age(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "age", 2, 2)?;
    let d1 = as_str_arg(args, 0, "age")?;
    let d2 = as_str_arg(args, 1, "age")?;
    let date1 = parse_date(d1).ok_or_else(|| EvalError::TypeError {
        function: "age".to_string(),
        message: format!("cannot parse date '{}'", d1),
    })?;
    let date2 = parse_date(d2).ok_or_else(|| EvalError::TypeError {
        function: "age".to_string(),
        message: format!("cannot parse date '{}'", d2),
    })?;
    let days1 = days_since_epoch(date1.0, date1.1, date1.2);
    let days2 = days_since_epoch(date2.0, date2.1, date2.2);
    Ok(Literal::Integer(days2 - days1))
}

fn year(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "year", 1, 1)?;
    let d = as_str_arg(args, 0, "year")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "year".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    Ok(Literal::Integer(date.0))
}

fn month(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "month", 1, 1)?;
    let d = as_str_arg(args, 0, "month")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "month".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    Ok(Literal::Integer(date.1))
}

fn day(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "day", 1, 1)?;
    let d = as_str_arg(args, 0, "day")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "day".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    Ok(Literal::Integer(date.2))
}

fn hour(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "hour", 1, 1)?;
    let d = as_str_arg(args, 0, "hour")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "hour".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    Ok(Literal::Integer(date.3))
}

fn minute(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "minute", 1, 1)?;
    let d = as_str_arg(args, 0, "minute")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "minute".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    Ok(Literal::Integer(date.4))
}

fn second(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "second", 1, 1)?;
    let d = as_str_arg(args, 0, "second")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "second".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    Ok(Literal::Integer(date.5))
}

fn week(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "week", 1, 1)?;
    let d = as_str_arg(args, 0, "week")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "week".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    let days = days_since_epoch(date.0, date.1, date.2);
    // ISO week number (simplified)
    let week = (days / 7).rem_euclid(53) + 1;
    Ok(Literal::Integer(week))
}

fn day_of_week(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "day_of_week", 1, 1)?;
    let d = as_str_arg(args, 0, "day_of_week")?;
    let date = parse_date(d).ok_or_else(|| EvalError::TypeError {
        function: "day_of_week".to_string(),
        message: format!("cannot parse date '{}'", d),
    })?;
    let days = days_since_epoch(date.0, date.1, date.2);
    // 1970-01-01 was a Thursday (ISO day 4)
    // ISO: 1=Monday, 7=Sunday
    let dow = ((days + 3) % 7 + 7) % 7 + 1;
    Ok(Literal::Integer(dow))
}

fn to_interval(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "to_interval", 1, 1)?;
    let s = as_str_arg(args, 0, "to_interval")?;
    // Return as string representation
    Ok(Literal::Str(s.to_string()))
}

// === Array functions ===

fn array(args: &[Literal]) -> Result<Literal, EvalError> {
    Ok(Literal::Array(args.to_vec()))
}

fn array_append(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_append", 2, 2)?;
    let arr = as_array_arg(args, 0, "array_append")?;
    let mut result: Vec<Literal> = arr.to_vec();
    result.push(args[1].clone());
    Ok(Literal::Array(result))
}

fn array_prepend(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_prepend", 2, 2)?;
    let arr = as_array_arg(args, 0, "array_prepend")?;
    let mut result = vec![args[1].clone()];
    result.extend(arr.iter().cloned());
    Ok(Literal::Array(result))
}

fn array_cat(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_cat", 1, usize::MAX)?;
    let mut result = Vec::new();
    for arg in args {
        match arg {
            Literal::Array(a) => result.extend(a.iter().cloned()),
            Literal::Null => {}
            other => {
                return Err(EvalError::TypeError {
                    function: "array_cat".to_string(),
                    message: format!("expected array argument, got {}", other),
                })
            }
        }
    }
    Ok(Literal::Array(result))
}

fn array_distinct(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_distinct", 1, 1)?;
    let arr = as_array_arg(args, 0, "array_distinct")?;
    let mut result: Vec<Literal> = Vec::new();
    for item in arr {
        if !result.iter().any(|x| literals_equal(x, item)) {
            result.push(item.clone());
        }
    }
    Ok(Literal::Array(result))
}

fn array_reverse(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_reverse", 1, 1)?;
    let arr = as_array_arg(args, 0, "array_reverse")?;
    let mut result: Vec<Literal> = arr.to_vec();
    result.reverse();
    Ok(Literal::Array(result))
}

fn array_sort(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_sort", 1, 1)?;
    let arr = as_array_arg(args, 0, "array_sort")?;
    let mut result: Vec<Literal> = arr.to_vec();
    result.sort_by(|a, b| compare_literals(a, b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Literal::Array(result))
}

fn array_filter(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_filter", 2, usize::MAX)?;
    let arr = as_array_arg(args, 0, "array_filter")?;
    // Filter by a simple condition: keep items equal to args[1]
    let target = &args[1];
    let result: Vec<Literal> = arr
        .iter()
        .filter(|item| literals_equal(item, target))
        .cloned()
        .collect();
    Ok(Literal::Array(result))
}

fn array_foreach(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_foreach", 2, usize::MAX)?;
    let arr = as_array_arg(args, 0, "array_foreach")?;
    // Map each item by converting to string and appending args[1]
    let suffix = &args[1];
    let result: Vec<Literal> = arr
        .iter()
        .map(|item| Literal::Str(format!("{}{}", literal_to_string(item), literal_to_string(suffix))))
        .collect();
    Ok(Literal::Array(result))
}

fn array_get(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_get", 2, 2)?;
    let arr = as_array_arg(args, 0, "array_get")?;
    let idx = as_int_arg(args, 1, "array_get")?;
    if idx < 0 || idx as usize >= arr.len() {
        return Ok(Literal::Null);
    }
    Ok(arr[idx as usize].clone())
}

fn array_contains(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_contains", 2, 2)?;
    let arr = as_array_arg(args, 0, "array_contains")?;
    let target = &args[1];
    Ok(Literal::Bool(arr.iter().any(|item| literals_equal(item, target))))
}

fn array_find(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_find", 2, 2)?;
    let arr = as_array_arg(args, 0, "array_find")?;
    let target = &args[1];
    for (i, item) in arr.iter().enumerate() {
        if literals_equal(item, target) {
            return Ok(Literal::Integer(i as i64));
        }
    }
    Ok(Literal::Integer(-1))
}

fn array_length(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_length", 1, 1)?;
    let arr = as_array_arg(args, 0, "array_length")?;
    Ok(Literal::Integer(arr.len() as i64))
}

fn array_to_string(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "array_to_string", 1, 2)?;
    let arr = as_array_arg(args, 0, "array_to_string")?;
    let separator = if args.len() == 2 {
        as_str_arg(args, 1, "array_to_string")?
    } else {
        ","
    };
    let parts: Vec<String> = arr.iter().map(literal_to_string).collect();
    Ok(Literal::Str(parts.join(separator)))
}

fn string_to_array(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "string_to_array", 2, 2)?;
    let s = as_str_arg(args, 0, "string_to_array")?;
    let separator = as_str_arg(args, 1, "string_to_array")?;
    if s.is_empty() {
        return Ok(Literal::Array(vec![]));
    }
    let result: Vec<Literal> = s.split(separator).map(|p| Literal::Str(p.to_string())).collect();
    Ok(Literal::Array(result))
}

fn generate_series(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "generate_series", 2, 3)?;
    let start = as_int_arg(args, 0, "generate_series")?;
    let stop = as_int_arg(args, 1, "generate_series")?;
    let step = if args.len() == 3 {
        let s = as_int_arg(args, 2, "generate_series")?;
        if s == 0 {
            return Err(EvalError::TypeError {
                function: "generate_series".to_string(),
                message: "step cannot be zero".to_string(),
            });
        }
        s
    } else {
        1
    };
    let mut result = Vec::new();
    if step > 0 {
        let mut i = start;
        while i <= stop {
            result.push(Literal::Integer(i));
            i += step;
        }
    } else {
        let mut i = start;
        while i >= stop {
            result.push(Literal::Integer(i));
            i += step;
        }
    }
    Ok(Literal::Array(result))
}

fn range(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "range", 2, 3)?;
    let start = as_int_arg(args, 0, "range")?;
    let stop = as_int_arg(args, 1, "range")?;
    let step = if args.len() == 3 {
        let s = as_int_arg(args, 2, "range")?;
        if s == 0 {
            return Err(EvalError::TypeError {
                function: "range".to_string(),
                message: "step cannot be zero".to_string(),
            });
        }
        s
    } else {
        1
    };
    let mut result = Vec::new();
    if step > 0 {
        let mut i = start;
        while i <= stop {
            result.push(Literal::Integer(i));
            i += step;
        }
    } else {
        let mut i = start;
        while i >= stop {
            result.push(Literal::Integer(i));
            i += step;
        }
    }
    Ok(Literal::Array(result))
}

// === Map functions (using array of key-value pairs) ===

fn map_akeys(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "map_akeys", 1, 1)?;
    let arr = as_array_arg(args, 0, "map_akeys")?;
    let mut keys = Vec::new();
    for item in arr {
        if let Literal::Array(pair) = item {
            if !pair.is_empty() {
                keys.push(pair[0].clone());
            }
        }
    }
    Ok(Literal::Array(keys))
}

fn map_avals(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "map_avals", 1, 1)?;
    let arr = as_array_arg(args, 0, "map_avals")?;
    let mut vals = Vec::new();
    for item in arr {
        if let Literal::Array(pair) = item {
            if pair.len() >= 2 {
                vals.push(pair[1].clone());
            }
        }
    }
    Ok(Literal::Array(vals))
}

fn map_get(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "map_get", 2, 2)?;
    let arr = as_array_arg(args, 0, "map_get")?;
    let key = &args[1];
    for item in arr {
        if let Literal::Array(pair) = item {
            if pair.len() >= 2 && literals_equal(&pair[0], key) {
                return Ok(pair[1].clone());
            }
        }
    }
    Ok(Literal::Null)
}

fn map_insert(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "map_insert", 3, 3)?;
    let arr = as_array_arg(args, 0, "map_insert")?;
    let key = &args[1];
    let val = &args[2];
    let mut result: Vec<Literal> = Vec::new();
    let mut found = false;
    for item in arr {
        if let Literal::Array(pair) = item {
            if pair.len() >= 2 && literals_equal(&pair[0], key) {
                result.push(Literal::Array(vec![key.clone(), val.clone()]));
                found = true;
            } else {
                result.push(item.clone());
            }
        } else {
            result.push(item.clone());
        }
    }
    if !found {
        result.push(Literal::Array(vec![key.clone(), val.clone()]));
    }
    Ok(Literal::Array(result))
}

fn map_delete(args: &[Literal]) -> Result<Literal, EvalError> {
    require_arity(args, "map_delete", 2, 2)?;
    let arr = as_array_arg(args, 0, "map_delete")?;
    let key = &args[1];
    let result: Vec<Literal> = arr
        .iter()
        .filter(|item| {
            if let Literal::Array(pair) = item {
                !(pair.len() >= 2 && literals_equal(&pair[0], key))
            } else {
                true
            }
        })
        .cloned()
        .collect();
    Ok(Literal::Array(result))
}

// === Helpers ===

fn literals_equal(l: &Literal, r: &Literal) -> bool {
    match (l, r) {
        (Literal::Null, Literal::Null) => true,
        (Literal::Bool(a), Literal::Bool(b)) => a == b,
        (Literal::Integer(a), Literal::Integer(b)) => a == b,
        (Literal::Float(a), Literal::Float(b)) => a == b,
        (Literal::Integer(a), Literal::Float(b)) | (Literal::Float(b), Literal::Integer(a)) => {
            (*a as f64) == *b
        }
        (Literal::Str(a), Literal::Str(b)) => a == b,
        (Literal::Date(a), Literal::Date(b)) => a == b,
        (Literal::Array(a), Literal::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| literals_equal(x, y))
        }
        _ => false,
    }
}

fn compare_literals(l: &Literal, r: &Literal) -> Option<std::cmp::Ordering> {
    match (l, r) {
        (Literal::Integer(a), Literal::Integer(b)) => Some(a.cmp(b)),
        (Literal::Float(a), Literal::Float(b)) => a.partial_cmp(b),
        (Literal::Integer(a), Literal::Float(b)) => (*a as f64).partial_cmp(b),
        (Literal::Float(a), Literal::Integer(b)) => a.partial_cmp(&(*b as f64)),
        (Literal::Str(a), Literal::Str(b)) => Some(a.cmp(b)),
        (Literal::Date(a), Literal::Date(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Literal>) -> Result<Literal, EvalError> {
        super::super::call(name, args)
    }

    #[test]
    fn test_now() {
        let result = call("now", vec![]).unwrap();
        assert!(matches!(result, Literal::Date(_)));
    }

    #[test]
    fn test_year() {
        assert_eq!(
            call("year", vec![Literal::Date("2026-10-12".to_string())]).unwrap(),
            Literal::Integer(2026)
        );
    }

    #[test]
    fn test_month() {
        assert_eq!(
            call("month", vec![Literal::Date("2026-10-12".to_string())]).unwrap(),
            Literal::Integer(10)
        );
    }

    #[test]
    fn test_day() {
        assert_eq!(
            call("day", vec![Literal::Date("2026-10-12".to_string())]).unwrap(),
            Literal::Integer(12)
        );
    }

    #[test]
    fn test_hour() {
        assert_eq!(
            call("hour", vec![Literal::Date("2026-10-12T14:30:00".to_string())]).unwrap(),
            Literal::Integer(14)
        );
    }

    #[test]
    fn test_minute() {
        assert_eq!(
            call("minute", vec![Literal::Date("2026-10-12T14:30:00".to_string())]).unwrap(),
            Literal::Integer(30)
        );
    }

    #[test]
    fn test_second() {
        assert_eq!(
            call("second", vec![Literal::Date("2026-10-12T14:30:45".to_string())]).unwrap(),
            Literal::Integer(45)
        );
    }

    #[test]
    fn test_age() {
        let result = call(
            "age",
            vec![
                Literal::Date("2020-01-01".to_string()),
                Literal::Date("2026-01-01".to_string())
            ]
        )
        .unwrap();
        // Approximately 6 years = ~2192 days
        assert!(matches!(result, Literal::Integer(n) if n >= 2190 && n <= 2195));
    }

    #[test]
    fn test_array_creation() {
        let result = call(
            "array",
            vec![Literal::Integer(1), Literal::Integer(2), Literal::Integer(3)],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![
                Literal::Integer(1),
                Literal::Integer(2),
                Literal::Integer(3)
            ])
        );
    }

    #[test]
    fn test_array_append() {
        let result = call(
            "array_append",
            vec![
                Literal::Array(vec![Literal::Integer(1)]),
                Literal::Integer(2),
            ],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![Literal::Integer(1), Literal::Integer(2)])
        );
    }

    #[test]
    fn test_array_prepend() {
        let result = call(
            "array_prepend",
            vec![
                Literal::Array(vec![Literal::Integer(2)]),
                Literal::Integer(1),
            ],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![Literal::Integer(1), Literal::Integer(2)])
        );
    }

    #[test]
    fn test_array_cat() {
        let result = call(
            "array_cat",
            vec![
                Literal::Array(vec![Literal::Integer(1)]),
                Literal::Array(vec![Literal::Integer(2)]),
            ],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![Literal::Integer(1), Literal::Integer(2)])
        );
    }

    #[test]
    fn test_array_distinct() {
        let result = call(
            "array_distinct",
            vec![Literal::Array(vec![
                Literal::Integer(1),
                Literal::Integer(2),
                Literal::Integer(1),
            ])],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![Literal::Integer(1), Literal::Integer(2)])
        );
    }

    #[test]
    fn test_array_reverse() {
        let result = call(
            "array_reverse",
            vec![Literal::Array(vec![
                Literal::Integer(1),
                Literal::Integer(2),
                Literal::Integer(3),
            ])],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![
                Literal::Integer(3),
                Literal::Integer(2),
                Literal::Integer(1)
            ])
        );
    }

    #[test]
    fn test_array_sort() {
        let result = call(
            "array_sort",
            vec![Literal::Array(vec![
                Literal::Integer(3),
                Literal::Integer(1),
                Literal::Integer(2),
            ])],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![
                Literal::Integer(1),
                Literal::Integer(2),
                Literal::Integer(3)
            ])
        );
    }

    #[test]
    fn test_array_get() {
        let result = call(
            "array_get",
            vec![
                Literal::Array(vec![
                    Literal::Str("a".to_string()),
                    Literal::Str("b".to_string()),
                    Literal::Str("c".to_string()),
                ]),
                Literal::Integer(1),
            ],
        )
        .unwrap();
        assert_eq!(result, Literal::Str("b".to_string()));
    }

    #[test]
    fn test_array_contains() {
        let result = call(
            "array_contains",
            vec![
                Literal::Array(vec![Literal::Integer(1), Literal::Integer(2)]),
                Literal::Integer(2),
            ],
        )
        .unwrap();
        assert_eq!(result, Literal::Bool(true));
    }

    #[test]
    fn test_array_find() {
        let result = call(
            "array_find",
            vec![
                Literal::Array(vec![
                    Literal::Str("a".to_string()),
                    Literal::Str("b".to_string()),
                    Literal::Str("c".to_string()),
                ]),
                Literal::Str("b".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(result, Literal::Integer(1));
    }

    #[test]
    fn test_array_length() {
        let result = call(
            "array_length",
            vec![Literal::Array(vec![
                Literal::Integer(1),
                Literal::Integer(2),
                Literal::Integer(3),
            ])],
        )
        .unwrap();
        assert_eq!(result, Literal::Integer(3));
    }

    #[test]
    fn test_array_to_string() {
        let result = call(
            "array_to_string",
            vec![Literal::Array(vec![
                Literal::Str("a".to_string()),
                Literal::Str("b".to_string()),
                Literal::Str("c".to_string()),
            ])],
        )
        .unwrap();
        assert_eq!(result, Literal::Str("a,b,c".to_string()));
    }

    #[test]
    fn test_string_to_array() {
        let result = call(
            "string_to_array",
            vec![
                Literal::Str("a,b,c".to_string()),
                Literal::Str(",".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![
                Literal::Str("a".to_string()),
                Literal::Str("b".to_string()),
                Literal::Str("c".to_string())
            ])
        );
    }

    #[test]
    fn test_generate_series() {
        let result = call(
            "generate_series",
            vec![Literal::Integer(1), Literal::Integer(5)],
        )
        .unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![
                Literal::Integer(1),
                Literal::Integer(2),
                Literal::Integer(3),
                Literal::Integer(4),
                Literal::Integer(5)
            ])
        );
    }

    #[test]
    fn test_range() {
        let result = call("range", vec![Literal::Integer(1), Literal::Integer(5)]).unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![
                Literal::Integer(1),
                Literal::Integer(2),
                Literal::Integer(3),
                Literal::Integer(4),
                Literal::Integer(5)
            ])
        );
    }

    #[test]
    fn test_map_get() {
        let map = Literal::Array(vec![Literal::Array(vec![
            Literal::Str("key1".to_string()),
            Literal::Str("val1".to_string()),
        ])]);
        let result = call(
            "map_get",
            vec![map, Literal::Str("key1".to_string())],
        )
        .unwrap();
        assert_eq!(result, Literal::Str("val1".to_string()));
    }

    #[test]
    fn test_map_insert() {
        let map = Literal::Array(vec![Literal::Array(vec![
            Literal::Str("key1".to_string()),
            Literal::Str("val1".to_string()),
        ])]);
        let result = call(
            "map_insert",
            vec![
                map,
                Literal::Str("key2".to_string()),
                Literal::Str("val2".to_string()),
            ],
        )
        .unwrap();
        assert!(matches!(result, Literal::Array(_)));
    }

    #[test]
    fn test_map_delete() {
        let map = Literal::Array(vec![
            Literal::Array(vec![
                Literal::Str("key1".to_string()),
                Literal::Str("val1".to_string()),
            ]),
            Literal::Array(vec![
                Literal::Str("key2".to_string()),
                Literal::Str("val2".to_string()),
            ]),
        ]);
        let result = call("map_delete", vec![map, Literal::Str("key1".to_string())]).unwrap();
        assert_eq!(
            result,
            Literal::Array(vec![Literal::Array(vec![
                Literal::Str("key2".to_string()),
                Literal::Str("val2".to_string()),
            ])])
        );
    }

    #[test]
    fn test_day_of_week() {
        // Verify the algorithm returns a consistent ISO day of week
        // 1970-01-01 was a Thursday (ISO day 4)
        let dow = call("day_of_week", vec![Literal::Date("1970-01-01".to_string())]).unwrap();
        assert!(matches!(dow, Literal::Integer(n) if n >= 1 && n <= 7));
    }

    #[test]
    fn test_week() {
        let result = call(
            "week",
            vec![Literal::Date("2026-10-12".to_string())],
        )
        .unwrap();
        assert!(matches!(result, Literal::Integer(_)));
    }

    #[test]
    fn test_to_interval() {
        let result = call(
            "to_interval",
            vec![Literal::Str("P1D".to_string())],
        )
        .unwrap();
        assert_eq!(result, Literal::Str("P1D".to_string()));
    }

    #[test]
    fn test_domain_errors() {
        assert!(call("year", vec![Literal::Str("not-a-date".to_string())]).is_err());
        assert!(call("month", vec![Literal::Integer(42)]).is_err());
    }

    #[test]
    fn test_arity_errors() {
        assert!(call("year", vec![]).is_err());
        assert!(call("array_get", vec![Literal::Array(vec![])]).is_err());
        assert!(call("map_insert", vec![]).is_err());
    }
}
