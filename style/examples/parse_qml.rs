use qgis_style::{parse_qml_file, Renderer};
use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: parse_qml <file.qml>");
    match parse_qml_file(&path) {
        Ok(model) => {
            println!("Parsed QML style from: {path}");
            match &model.renderer {
                Renderer::Single(symbol) => {
                    println!("Renderer: singleSymbol");
                    println!("  Symbol: {symbol:?}");
                }
                Renderer::Categorized { attr, classes } => {
                    println!("Renderer: categorizedSymbol (attr: {attr}, {} classes)", classes.len());
                    for (val, sym) in classes {
                        println!("  [{val}] => {sym:?}");
                    }
                }
                Renderer::Graduated { attr, ranges } => {
                    println!("Renderer: graduatedSymbol (attr: {attr}, {} ranges)", ranges.len());
                    for (lo, hi, sym) in ranges {
                        println!("  [{lo} .. {hi}] => {sym:?}");
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error parsing {path}: {e}");
            std::process::exit(1);
        }
    }
}
