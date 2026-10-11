//! `qgis-oracle` CLI binary.
//!
//! Thin wrapper over `qgis_oracle::cli`: parse args, run, exit with the
//! returned code. See `qgis_oracle::cli::USAGE` for the full surface.

fn main() {
    let cli = match qgis_oracle::cli::parse_args(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            eprint!("{}", qgis_oracle::cli::USAGE);
            std::process::exit(qgis_oracle::cli::EXIT_ERROR);
        }
    };
    match qgis_oracle::cli::run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(qgis_oracle::cli::EXIT_ERROR);
        }
    }
}
