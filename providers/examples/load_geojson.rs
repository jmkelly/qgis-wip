use qgis_providers::geojson::read_to_features;
use qgis_geometry::Geometry;

fn main() {
    let path = std::env::args().nth(1).expect("usage: load_geojson <path>");
    match read_to_features(&path) {
        Ok(fc) => {
            println!("FeatureCollection: {} features", fc.features.len());
            for (i, f) in fc.features.iter().enumerate() {
                let geom_kind = match &f.geometry {
                    Some(Geometry::Point(_)) => "Point",
                    Some(Geometry::LineString(_)) => "LineString",
                    Some(Geometry::Polygon(_)) => "Polygon",
                    Some(Geometry::MultiPoint(_)) => "MultiPoint",
                    Some(Geometry::MultiLineString(_)) => "MultiLineString",
                    Some(Geometry::MultiPolygon(_)) => "MultiPolygon",
                    Some(Geometry::GeometryCollection(_)) => "GeometryCollection",
                    None => "None",
                    _ => "Other",
                };
                println!("  feature {}: {} (props: {})", i, geom_kind, f.properties.len());
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}