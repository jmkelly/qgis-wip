//! Criterion harness for the **raster** hot paths.
//!
//! Mirrors the raster reprojection and render paths: a raster band is
//! reprojected/resampled onto the map canvas and rendered.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use qgis_bench::raster::{raster_reproject, raster_render, Raster};
use qgis_bench::{Crs, Extent, MapSettings};

fn make_raster() -> Raster {
    Raster::new(
        256,
        256,
        Crs::new("EPSG:4326"),
        Extent::new(-180.0, -90.0, 180.0, 90.0),
    )
}

fn bench_raster_reproject(c: &mut Criterion) {
    let raster = make_raster();
    let dst_crs = Crs::new("EPSG:3857");
    let extent = Extent::new(-20037508.0, -20037508.0, 20037508.0, 20037508.0);
    c.bench_function("raster/reproject", |b| {
        b.iter(|| raster_reproject(black_box(&raster), black_box(&dst_crs), black_box(&extent)))
    });
}

fn bench_raster_render(c: &mut Criterion) {
    let raster = make_raster();
    let settings = MapSettings::default();
    c.bench_function("raster/render", |b| {
        b.iter(|| raster_render(black_box(&raster), black_box(&settings)))
    });
}

criterion_group!(benches, bench_raster_reproject, bench_raster_render);
criterion_main!(benches);
