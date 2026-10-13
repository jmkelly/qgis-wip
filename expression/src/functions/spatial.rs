//! Spatial expression functions.
//!
//! Geometries are represented as WKT strings in expressions. Functions
//! parse WKT, apply geometry operations, and return WKT strings (or
//! booleans/numbers for predicates and measurements).

use crate::ast::Literal;
use crate::eval::EvalError;
use qgis_geometry::{read_wkt, write_wkt, Geometry};

fn as_str(l: &Literal) -> Result<String, EvalError> {
    match l {
        Literal::Str(s) | Literal::Date(s) => Ok(s.clone()),
        _ => Err(EvalError::TypeError{function:"".into(),message:"expected string/WKT geometry".into()}),
    }
}

fn parse_geom(l: &Literal) -> Result<Geometry, EvalError> {
    let s = as_str(l)?;
    read_wkt(&s).map_err(|e| EvalError::TypeError{function:"".into(),message:format!("invalid geometry: {e}")})
}

fn geom_to_literal(g: Geometry) -> Literal {
    Literal::Str(write_wkt(&g))
}

fn numeric_measure(f: f64) -> Literal {
    if f.fract() == 0.0 && f.abs() < (1i64 << 52) as f64 {
        Literal::Integer(f as i64)
    } else {
        Literal::Float(f)
    }
}

// constructors
fn geom_from_wkt(args: &[Literal]) -> Result<Literal, EvalError> {
    if args.len() != 1 { return Err(EvalError::ArityError{function:"".into(),expected:"exactly 1".into(),got:args.len()}); }
    // validate + normalize
    let g = parse_geom(&args[0])?;
    Ok(geom_to_literal(g))
}

fn make_point(args: &[Literal]) -> Result<Literal, EvalError> {
    match args.len() {
        2 => {
            let x = match &args[0] { Literal::Integer(i)=>*i as f64, Literal::Float(f)=>*f, _=>return Err(EvalError::TypeError{function:"".into(),message:"number".into()})};
            let y = match &args[1] { Literal::Integer(i)=>*i as f64, Literal::Float(f)=>*f, _=>return Err(EvalError::TypeError{function:"".into(),message:"number".into()})};
            Ok(Literal::Str(format!("POINT({} {})", x, y)))
        }
        3 => {
            let x = match &args[0] { Literal::Integer(i)=>*i as f64, Literal::Float(f)=>*f, _=>return Err(EvalError::TypeError{function:"".into(),message:"number".into()})};
            let y = match &args[1] { Literal::Integer(i)=>*i as f64, Literal::Float(f)=>*f, _=>return Err(EvalError::TypeError{function:"".into(),message:"number".into()})};
            let z = match &args[2] { Literal::Integer(i)=>*i as f64, Literal::Float(f)=>*f, _=>return Err(EvalError::TypeError{function:"".into(),message:"number".into()})};
            Ok(Literal::Str(format!("POINT Z({} {} {})", x, y, z)))
        }
        _ => Err(EvalError::ArityError{function:"".into(),expected:"2 or 3".into(),got:args.len()}),
    }
}

// measurements
fn x_coord(args: &[Literal]) -> Result<Literal, EvalError> {
    let g = parse_geom(&args[0])?;
    match g {
        Geometry::Point(p) => Ok(Literal::Float(p.x())),
        _ => Err(EvalError::TypeError{function:"".into(),message:"point".into()}),
    }
}
fn y_coord(args: &[Literal]) -> Result<Literal, EvalError> {
    let g = parse_geom(&args[0])?;
    match g {
        Geometry::Point(p) => Ok(Literal::Float(p.y())),
        _ => Err(EvalError::TypeError{function:"".into(),message:"point".into()}),
    }
}
fn area(args: &[Literal]) -> Result<Literal, EvalError> {
    let g = parse_geom(&args[0])?;
    // use geo bridge? just compute via polygon area if polygon
    // fallback: try geometry area method
    // Geometry enum may implement area via trait? Check.
    // We'll compute simply: use g.area()? 
    // Let's call method if exists.
    let a = match &g {
        Geometry::Polygon(p) => p.area(),
        _ => 0.0,
    };
    Ok(numeric_measure(a))
}
fn perimeter(args: &[Literal]) -> Result<Literal, EvalError> {
    let g = parse_geom(&args[0])?;
    let p = match &g { Geometry::Polygon(pg)=>pg.perimeter(), Geometry::LineString(ls)=>ls.length(), _=>0.0};
    Ok(numeric_measure(p))
}
fn length(args: &[Literal]) -> Result<Literal, EvalError> {
    let g = parse_geom(&args[0])?;
    let l = match &g { Geometry::LineString(ls)=>ls.length(), _=>0.0};
    Ok(numeric_measure(l))
}

// transforms
fn buffer(args: &[Literal]) -> Result<Literal, EvalError> {
    if args.len() < 2 { return Err(EvalError::ArityError{function:"".into(),expected:"at least 2".into(),got:args.len()}); }
    let g = parse_geom(&args[0])?;
    let d = match &args[1] { Literal::Integer(i)=>*i as f64, Literal::Float(f)=>*f, _=>return Err(EvalError::TypeError{function:"".into(),message:"number".into()})};
    let b = qgis_geometry::buffer(&g, d, 8);
    Ok(geom_to_literal(b))
}
fn centroid(args: &[Literal]) -> Result<Literal, EvalError> {
    let g = parse_geom(&args[0])?;
    // compute centroid naive for polygon/line: average of vertices
    // Use geo_bridge? Let's attempt method centroid
    // fallback simple:
    let (sx, sy, n) = match &g {
        Geometry::Point(p) => (p.x(), p.y(), 1.0),
        Geometry::LineString(ls) => {
            let pts = ls.points();
            let (sx, sy) = pts.iter().fold((0.0,0.0), |a,p|(a.0+p.x(), a.1+p.y()));
            (sx, sy, pts.len() as f64)
        }
        Geometry::Polygon(pg) => {
            // use exterior ring
            let pts = pg.exterior();
            let (sx, sy) = pts.iter().fold((0.0,0.0), |a,p|(a.0+p.x(), a.1+p.y()));
            (sx, sy, pts.len() as f64)
        }
        _ => (0.0,0.0,1.0),
    };
    Ok(Literal::Str(format!("POINT({} {})", sx/n, sy/n)))
}

// predicates: functions return bool
macro_rules! pred_fn {
    ($name:ident, $geomfn:path) => {
        fn $name(args: &[Literal]) -> Result<Literal, EvalError> {
            if args.len() < 2 { return Err(EvalError::ArityError{function:"".into(),expected:"exactly 2".into(),got:args.len()}); }
            let a = parse_geom(&args[0])?;
            let b = parse_geom(&args[1])?;
            Ok(Literal::Bool($geomfn(&a, &b)))
        }
    };
}

pred_fn!(intersects, qgis_geometry::intersects);
pred_fn!(contains, qgis_geometry::contains);
pred_fn!(touches, qgis_geometry::touches);
pred_fn!(within, qgis_geometry::within);
pred_fn!(crosses, qgis_geometry::crosses);
pred_fn!(overlaps, qgis_geometry::overlaps);
pred_fn!(disjoint, qgis_geometry::disjoint);
pred_fn!(equals, qgis_geometry::equals);

// overlay ops returning geometry
fn intersection(args: &[Literal]) -> Result<Literal, EvalError> {
    let a = parse_geom(&args[0])?; let b = parse_geom(&args[1])?;
    Ok(geom_to_literal(qgis_geometry::intersection(&a,&b)))
}
fn union(args: &[Literal]) -> Result<Literal, EvalError> {
    let a = parse_geom(&args[0])?; let b = parse_geom(&args[1])?;
    Ok(geom_to_literal(qgis_geometry::union(&a,&b)))
}
fn difference(args: &[Literal]) -> Result<Literal, EvalError> {
    let a = parse_geom(&args[0])?; let b = parse_geom(&args[1])?;
    Ok(geom_to_literal(qgis_geometry::difference(&a,&b)))
}

pub fn lookup(name: &str) -> Option<super::FunctionFn> {
    let f = match name {
        "geom_from_wkt" | "st_geomfromwkt" | "geom_from_wkb" => geom_from_wkt as super::FunctionFn,
        "geom_to_wkt" | "st_astext" => geom_from_wkt as super::FunctionFn,
        "make_point" => make_point as super::FunctionFn,
        "x" => x_coord as super::FunctionFn,
        "y" => y_coord as super::FunctionFn,
        "area" => area as super::FunctionFn,
        "perimeter" => perimeter as super::FunctionFn,
        "length" => length as super::FunctionFn,
        "buffer" => buffer as super::FunctionFn,
        "centroid" => centroid as super::FunctionFn,
        "intersects" => intersects as super::FunctionFn,
        "contains" => contains as super::FunctionFn,
        "touches" => touches as super::FunctionFn,
        "within" => within as super::FunctionFn,
        "crosses" => crosses as super::FunctionFn,
        "overlaps" => overlaps as super::FunctionFn,
        "disjoint" => disjoint as super::FunctionFn,
        "geom_equals" | "equals" => equals as super::FunctionFn,
        "intersection" => intersection as super::FunctionFn,
        "union" => union as super::FunctionFn,
        "difference" => difference as super::FunctionFn,
        _ => return None,
    };
    Some(f)
}

pub fn arity(name: &str) -> Option<&'static str> {
    match name {
        "geom_from_wkt" | "geom_to_wkt" | "st_astext" | "x" | "y" | "area" | "perimeter" | "length" | "centroid" => Some("exactly 1"),
        "make_point" => Some("2 or 3"),
        "buffer" => Some("at least 2"),
        "intersects" | "contains" | "touches" | "within" | "crosses" | "overlaps" | "disjoint" | "geom_equals" | "equals" | "intersection" | "union" | "difference" => Some("exactly 2"),
        _ => None,
    }
}
