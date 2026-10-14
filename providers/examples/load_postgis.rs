use qgis_geometry::Geometry;
use qgis_providers::postgis::{read_to_features, PostgisProvider};
use qgis_providers::Provider;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let conn_str = if args.is_empty() {
        "host=localhost dbname=gis".to_string()
    } else {
        args.join(" ")
    };

    let provider = PostgisProvider;
    println!("Provider '{}' (valid={}) connecting to: {}", provider.name(), provider.is_valid(), conn_str);

    match read_to_features(&conn_str) {
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
                };
                println!(
                    "  feature {}: {} (props: {:?})",
                    i, geom_kind, f.properties
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
