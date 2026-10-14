use qgis_providers::shapefile;
use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: load_shapefile <file.shp>");
    let features = shapefile::read_shapefile(&path).unwrap();
    println!("loaded {} features", features.len());
    for f in features.iter().take(5) {
        println!("{:?}", f.geometry);
        println!("{:?}", f.attributes);
    }
}
