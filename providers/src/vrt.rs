//! VRT (Virtual Raster) mosaic reader + resampling kernels, pure Rust.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResampleAlg { Nearest, Bilinear, Cubic, CubicSpline, Lanczos, Average, Mode, Sum }

impl ResampleAlg {
    pub fn parse_alg(s: &str) -> Self {
        match s.to_lowercase().replace(" ", "").as_str() {
            "nearest" | "nearestneighbour" | "nearestneighbor" | "nn" => Self::Nearest,
            "bilinear" => Self::Bilinear,
            "cubic" => Self::Cubic,
            "cubicspline" => Self::CubicSpline,
            "lanczos" => Self::Lanczos,
            "average" => Self::Average,
            "mode" => Self::Mode,
            "sum" => Self::Sum,
            _ => Self::Nearest,
        }
    }
}

// Resampling kernels (weight functions)
pub fn kernel_weight(alg: ResampleAlg, x: f64) -> f64 {
    let ax = x.abs();
    match alg {
        ResampleAlg::Nearest => if ax < 0.5 { 1.0 } else { 0.0 },
        ResampleAlg::Bilinear => if ax < 1.0 { 1.0 - ax } else { 0.0 },
        ResampleAlg::Cubic => cubic_convolution(ax, -0.5),
        ResampleAlg::CubicSpline => cubic_convolution(ax, -1.0),
        ResampleAlg::Lanczos => {
            if ax >= 3.0 { return 0.0; }
            if ax == 0.0 { return 1.0; }
            let pi_x = std::f64::consts::PI * ax;
            let pi_x_3 = pi_x / 3.0;
            (pi_x.sin() / pi_x) * (pi_x_3.sin() / pi_x_3)
        }
        _ => if ax < 0.5 { 1.0 } else { 0.0 },
    }
}

fn cubic_convolution(x: f64, a: f64) -> f64 {
    if x >= 2.0 { return 0.0; }
    if x < 1.0 {
        (a+2.0)*x*x*x - (a+3.0)*x*x + 1.0
    } else {
        a*x*x*x - 5.0*a*x*x + 8.0*a*x - 4.0*a
    }
}

#[derive(Debug, Clone)]
pub struct GeoTransform {
    pub origin_x: f64, pub pixel_w: f64, pub rot_x: f64,
    pub origin_y: f64, pub rot_y: f64, pub pixel_h: f64,
}

impl GeoTransform {
    pub fn identity() -> Self { Self { origin_x:0.0, pixel_w:1.0, rot_x:0.0, origin_y:0.0, rot_y:0.0, pixel_h:1.0 } }
}

#[derive(Debug, Clone)]
pub struct VrtSource {
    pub filename: String,
    pub band: usize,
}

#[derive(Debug, Clone)]
pub struct VrtDataset {
    pub raster_x_size: usize,
    pub raster_y_size: usize,
    pub geo_transform: Option<GeoTransform>,
    pub srs: Option<String>,
    pub sources: Vec<VrtSource>,
    pub complex: bool,
}

impl VrtDataset {
    pub fn parse(xml: &str) -> Self {
        let mut ds = Self {
            raster_x_size: attr_usize(xml, "rasterXSize").unwrap_or(0),
            raster_y_size: attr_usize(xml, "rasterYSize").unwrap_or(0),
            geo_transform: None,
            srs: None,
            sources: Vec::new(),
            complex: xml.contains("ComplexSource"),
        };
        // GeoTransform
        if let Some(start) = xml.find("<GeoTransform>") {
            if let Some(end) = xml.find("</GeoTransform>") {
                let inner = &xml[start+13..end];
                let vals: Vec<f64> = inner.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if vals.len() >= 6 {
                    ds.geo_transform = Some(GeoTransform {
                        origin_x: vals[0], pixel_w: vals[1], rot_x: vals[2],
                        origin_y: vals[3], rot_y: vals[4], pixel_h: vals[5],
                    });
                }
            }
        }
        // SRS
        if let Some(start) = xml.find("<SRS>") {
            if let Some(end) = xml.find("</SRS>") {
                ds.srs = Some(xml[start+5..end].to_string());
            }
        }
        // SourceFilename
        let mut search = 0;
        while let Some(i) = xml[search..].find("<SourceFilename") {
            let abs = search + i;
            if let Some(j) = xml[abs..].find("</SourceFilename>") {
                let seg = &xml[abs..abs+j+17];
                // filename text between > and <
                if let Some(gt) = seg.find('>') {
                    let marker = "</SourceFilename>";
                    let endpos = seg.find(marker).unwrap_or(seg.len());
                    let name = &seg[gt+1..endpos];
                    ds.sources.push(VrtSource { filename: name.trim().to_string(), band: 1 });
                }
                search = abs + j + 17;
            } else { break; }
        }
        ds
    }
}

fn attr_usize(xml: &str, name: &str) -> Option<usize> {
    // search name="value"
    let key = format!("{}=\"", name);
    if let Some(i) = xml.find(&key) {
        let rest = &xml[i+key.len()..];
        if let Some(q) = rest.find('"') {
            return rest[..q].parse().ok();
        }
    }
    None
}

// Warp engine placeholder
pub struct WarpOptions {
    pub src_nodata: Option<f64>,
    pub dst_nodata: Option<f64>,
    pub resample: ResampleAlg,
}

impl Default for WarpOptions {
    fn default() -> Self { Self { src_nodata: None, dst_nodata: None, resample: ResampleAlg::Nearest } }
}

pub fn warp_reproject(_src: &VrtDataset, _opts: &WarpOptions) -> Result<VrtDataset, String> {
    // Minimal: return clone-like empty result
    Ok(VrtDataset { raster_x_size: 0, raster_y_size: 0, geo_transform: None, srs: None, sources: vec![], complex: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_xml() {
        let xml = r#"<VRTDataset rasterXSize="4" rasterYSize="3"><SRS>EPSG:4326</SRS><GeoTransform>0,1,0,0,0,-1</GeoTransform><VRTRasterBand><ComplexSource><SourceFilename>foo.tif</SourceFilename></ComplexSource></VRTRasterBand></VRTDataset>"#;
        let ds = VrtDataset::parse(xml);
        assert_eq!(ds.raster_x_size,4);
        assert_eq!(ds.raster_y_size,3);
        assert_eq!(ds.sources.len(),1);
        assert!(ds.complex);
    }
    #[test]
    fn kernels() {
        for alg in [ResampleAlg::Nearest, ResampleAlg::Bilinear, ResampleAlg::Cubic,
                    ResampleAlg::CubicSpline, ResampleAlg::Lanczos, ResampleAlg::Average,
                    ResampleAlg::Mode, ResampleAlg::Sum] {
            assert!(kernel_weight(alg,0.0) >= 0.0);
            assert!(kernel_weight(alg,10.0).abs() < 1e-12 || kernel_weight(alg,10.0)==0.0);
        }
        assert!(kernel_weight(ResampleAlg::Nearest,0.0) > 0.5);
    }
    #[test]
    fn parse_alg_all() {
        assert_eq!(ResampleAlg::parse_alg("bilinear"), ResampleAlg::Bilinear);
        assert_eq!(ResampleAlg::parse_alg("cubic"), ResampleAlg::Cubic);
        assert_eq!(ResampleAlg::parse_alg("cubicspline"), ResampleAlg::CubicSpline);
        assert_eq!(ResampleAlg::parse_alg("lanczos"), ResampleAlg::Lanczos);
        assert_eq!(ResampleAlg::parse_alg("average"), ResampleAlg::Average);
        assert_eq!(ResampleAlg::parse_alg("mode"), ResampleAlg::Mode);
        assert_eq!(ResampleAlg::parse_alg("sum"), ResampleAlg::Sum);
    }
}
