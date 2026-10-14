use qgis_providers::gpkg;
use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: load_gpkg <file.gpkg>");
    let features = gpkg::read_gpkg(&path).unwrap();
    println!("loaded {} features", features.len());
    for f in features.iter().take(5) {
        println!("{:?}", f.geometry);
        println!("{:?}", f.attributes);
    }
}