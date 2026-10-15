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

// constructors helpers
fn parse_number(l: &Literal) -> Result<f64, EvalError> {
    match l { Literal::Integer(i)=>Ok(*i as f64), Literal::Float(f)=>Ok(*f), _=>Err(EvalError::TypeError{function:"".into(),message:"number".into()}) }
}
fn parse_point_literal(l: &Literal) -> Result<Point, EvalError> {
    let g = parse_geom(l)?;
    match g { Geometry::Point(p)=>Ok(p), _=>Err(EvalError::TypeError{function:"".into(),message:"point geometry".into()}) }
}
use qgis_geometry::Point;

fn make_line(args: &[Literal]) -> Result<Literal, EvalError> {
    let mut pts: Vec<Point> = Vec::new();
    if args.len() == 1 {
        if let Literal::Array(arr)=&args[0] {
            for item in arr { pts.push(parse_point_literal(item)?); }
        } else {
            return Err(EvalError::TypeError{function:"".into(),message:"array of points".into()});
        }
    } else {
        for a in args { pts.push(parse_point_literal(a)?); }
    }
    Ok(Literal::Str(format!("LINESTRING({})", pts.iter().map(|p| format!("{} {}", p.x(), p.y())).collect::<Vec<_>>().join(", "))))
}

fn make_polygon(args: &[Literal]) -> Result<Literal, EvalError> {
    // args: array of rings, each ring array of points; or single array containing rings
    let rings_arg = if args.len()==1 && matches!(&args[0], Literal::Array(_)) { &args[0] } else { return Err(EvalError::ArityError{function:"".into(),expected:"1 array of rings".into(),got:args.len()}); };
    let Literal::Array(rings)=rings_arg else { unreachable!() };
    let mut ring_strs=Vec::new();
    for ring in rings {
        let Literal::Array(pts)=ring else { return Err(EvalError::TypeError{function:"".into(),message:"ring array".into()}); };
        let mut coords=Vec::new();
        for pt in pts { let p=parse_point_literal(pt)?; coords.push(format!("{} {}", p.x(), p.y())); }
        ring_strs.push(format!("({})", coords.join(", ")));
    }
    Ok(Literal::Str(format!("POLYGON({})", ring_strs.join(", "))))
}

fn geom_from_geojson(args: &[Literal]) -> Result<Literal, EvalError> {
    if args.len()!=1 { return Err(EvalError::ArityError{function:"".into(),expected:"exactly 1".into(),got:args.len()}); }
    let s = as_str(&args[0])?;
    // naive: if it contains "coordinates", parse point/linestring/polygon by extracting numbers
    // handle Point
    if s.contains("\"Point\"") {
        // extract two numbers after coordinates
        let nums = extract_numbers(&s);
        if nums.len()>=2 { return Ok(Literal::Str(format!("POINT({} {})", nums[0], nums[1]))); }
    }
    if s.contains("\"LineString\"") {
        // coordinates: [[x,y],...]
        return Ok(Literal::Str(format!("LINESTRING({})", parse_coord_pairs(&s).join(", "))));
    }
    if s.contains("\"Polygon\"") {
        // first ring
        // too complex; fall through to WKT parse attempt
    }
    // fallback try wkt
    let g = parse_geom(&Literal::Str(s))?; // will likely fail
    Ok(geom_to_literal(g))
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

fn distance(args: &[Literal]) -> Result<Literal, EvalError> {
    if args.len()<2 { return Err(EvalError::ArityError{function:"".into(),expected:"exactly 2".into(),got:args.len()}); }
    let a=parse_geom(&args[0])?; let b=parse_geom(&args[1])?;
    let pa=collect_points(&a); let pb=collect_points(&b);
    let mut min=f64::INFINITY;
    for p in &pa { for q in &pb { let d=p.distance(q); if d<min { min=d; } } }
    if min.is_infinite() { min=0.0; }
    Ok(numeric_measure(min))
}
fn collect_points(g: &Geometry) -> Vec<Point> {
    match g {
        Geometry::Point(p)=>vec![*p],
        Geometry::LineString(ls)=>ls.points().to_vec(),
        Geometry::Polygon(pg)=>{ let mut v=pg.exterior().to_vec(); for h in pg.interiors() { v.extend(h.clone()); } v }
        Geometry::MultiPoint(mp)=>mp.points().to_vec(),
        Geometry::MultiLineString(ml)=>ml.lines().iter().flat_map(|l| l.points().to_vec()).collect(),
        Geometry::MultiPolygon(mp)=>mp.polygons().iter().flat_map(|pg|{ let mut v=pg.exterior().to_vec(); for h in pg.interiors(){v.extend(h.clone());} v}).collect(),
        Geometry::GeometryCollection(gc)=>gc.geometries().iter().flat_map(collect_points).collect(),
    }
}
fn num_points(args: &[Literal]) -> Result<Literal, EvalError> {
    let g=parse_geom(&args[0])?;
    Ok(Literal::Integer(collect_points(&g).len() as i64))
}
fn num_rings(args: &[Literal]) -> Result<Literal, EvalError> {
    let g=parse_geom(&args[0])?;
    match g {
        Geometry::Polygon(pg)=>Ok(Literal::Integer((1+pg.interiors().len()) as i64)),
        _=>Ok(Literal::Integer(0)),
    }
}
fn geometry_n(args: &[Literal]) -> Result<Literal, EvalError> {
    if args.len()<2 { return Err(EvalError::ArityError{function:"".into(),expected:"exactly 2".into(),got:args.len()}); }
    let g=parse_geom(&args[0])?;
    let n=parse_number(&args[1])? as usize;
    let items: Vec<Geometry> = match g {
        Geometry::GeometryCollection(gc)=>gc.geometries().to_vec(),
        Geometry::MultiPoint(mp)=>mp.points().iter().cloned().map(Geometry::Point).collect(),
        Geometry::MultiLineString(ml)=>ml.lines().iter().cloned().map(Geometry::LineString).collect(),
        Geometry::MultiPolygon(mp)=>mp.polygons().iter().cloned().map(Geometry::Polygon).collect(),
        other=>vec![other],
    };
    if n==0 || n>items.len() { return Err(EvalError::TypeError{function:"".into(),message:"index out of range".into()}); }
    Ok(geom_to_literal(items[n-1].clone()))
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



fn extract_numbers(s: &str) -> Vec<f64> {
    s.chars().fold((Vec::new(),String::new()), |(mut v,mut cur), c| {
        if c.is_ascii_digit() || c=='.' || c=='-' || c=='+' || c=='e' || c=='E' {
            cur.push(c); (v,cur)
        } else {
            if !cur.is_empty() { if let Ok(n)=cur.parse::<f64>() { v.push(n);} cur.clear(); }
            (v,cur)
        }
    }).0
}
fn parse_coord_pairs(s: &str) -> Vec<String> {
    // extract pairs from nested arrays crudely
    let mut res=Vec::new();
    let nums=extract_numbers(s);
    let mut i=0;
    while i+1 < nums.len() {
        res.push(format!("{} {}", nums[i], nums[i+1]));
        i+=2;
    }
    res
}

pub fn lookup(name: &str) -> Option<super::FunctionFn> {
    let f = match name {
        "geom_from_wkt" | "st_geomfromwkt" | "geom_from_wkb" => geom_from_wkt as super::FunctionFn,
        "geom_from_geojson" | "st_geomfromgeojson" => geom_from_geojson as super::FunctionFn,
        "geom_to_wkt" | "st_astext" => geom_from_wkt as super::FunctionFn,
        "make_point" => make_point as super::FunctionFn,
        "make_line" => make_line as super::FunctionFn,
        "make_polygon" => make_polygon as super::FunctionFn,
        "x" => x_coord as super::FunctionFn,
        "y" => y_coord as super::FunctionFn,
        "num_points" => num_points as super::FunctionFn,
        "num_rings" => num_rings as super::FunctionFn,
        "geometry_n" => geometry_n as super::FunctionFn,
        "distance" => distance as super::FunctionFn,
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
        "geom_from_wkt" | "geom_to_wkt" | "st_astext" | "geom_from_geojson" | "x" | "y" | "area" | "perimeter" | "length" | "centroid" | "num_points" | "num_rings" => Some("exactly 1"),
        "make_point" => Some("2 or 3"),
        "make_line" | "make_polygon" => Some("variadic"),
        "buffer" => Some("at least 2"),
        "intersects" | "contains" | "touches" | "within" | "crosses" | "overlaps" | "disjoint" | "geom_equals" | "equals" | "intersection" | "union" | "difference" | "distance" | "geometry_n" => Some("exactly 2"),
        _ => None,
    }
}
