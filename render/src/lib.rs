//! Minimal 2D raster rendering primitives for the QGIS Rust port.
//!
//! Provides the pixel, image, coordinate-transform, and canvas types used by
//! the WMS GetMap renderer, plus a PNG encoder. This is a constrained
//! implementation covering only what the server's WMS rendering path needs;
//! a full renderer arrives with the E9 hardening epic.

use std::fmt;

/// Maximum image dimension (pixels) supported by the renderer.
pub const MAX_DIMENSION: u32 = 16384;

/// A single RGBA pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pixel {
    /// Red channel (0-255).
    pub r: u8,
    /// Green channel (0-255).
    pub g: u8,
    /// Blue channel (0-255).
    pub b: u8,
    /// Alpha channel (0-255, 0 = fully transparent).
    pub a: u8,
}

impl Pixel {
    /// Creates an opaque pixel from RGB channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates a pixel from RGBA channels.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Fully transparent pixel.
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
}

/// Errors produced by the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError(String);

impl RenderError {
    /// Creates a new render error with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "render error: {}", self.0)
    }
}

impl std::error::Error for RenderError {}

/// A raster image with RGBA pixels in row-major order.
#[derive(Debug, Clone)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<Pixel>,
}

impl Image {
    /// Creates a new image filled with the given background color.
    pub fn new(width: u32, height: u32, background: Pixel) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::new("image dimensions must be non-zero"));
        }
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(RenderError::new(format!(
                "image dimensions must not exceed {MAX_DIMENSION}"
            )));
        }
        let pixel_count = width as u64 * height as u64;
        Ok(Self {
            width,
            height,
            pixels: vec![background; pixel_count as usize],
        })
    }

    /// Returns the image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the pixel at the given coordinates, or `None` if out of bounds.
    pub fn get(&self, x: u32, y: u32) -> Option<Pixel> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels.get(y as usize * self.width as usize + x as usize).copied()
    }

    /// Sets the pixel at the given coordinates. Returns `false` if out of bounds.
    pub fn set(&mut self, x: u32, y: u32, pixel: Pixel) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.pixels[y as usize * self.width as usize + x as usize] = pixel;
        true
    }

    /// Returns the raw RGBA pixel bytes (4 bytes per pixel, row-major).
    pub fn rgba_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for p in &self.pixels {
            bytes.extend_from_slice(&[p.r, p.g, p.b, p.a]);
        }
        bytes
    }
}

/// An affine transform mapping map coordinates to pixel coordinates.
///
/// The transform is defined as:
/// - `px = sx * x + ox`
/// - `py = oy - sy * y` (y axis is flipped: pixel row 0 is the top)
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    sx: f64,
    ox: f64,
    sy: f64,
    oy: f64,
}

impl Transform {
    /// Creates a transform mapping the bounding box
    /// `(min_x, min_y) .. (max_x, max_y)` onto a `width` x `height` pixel grid.
    pub fn map_to_pixel(
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        width: u32,
        height: u32,
    ) -> Self {
        let sx = if max_x > min_x { width as f64 / (max_x - min_x) } else { 0.0 };
        let sy = if max_y > min_y { height as f64 / (max_y - min_y) } else { 0.0 };
        Self {
            sx,
            ox: -min_x * sx,
            sy,
            oy: height as f64 + min_y * sy,
        }
    }

    /// Transforms a map coordinate to a pixel coordinate.
    pub fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        (self.sx * x + self.ox, self.oy - self.sy * y)
    }

    /// Returns the inverse transform (pixel to map), or `None` if singular.
    pub fn inverse(&self) -> Option<Self> {
        if self.sx == 0.0 || self.sy == 0.0 {
            return None;
        }
        Some(Self {
            sx: 1.0 / self.sx,
            ox: -self.ox / self.sx,
            sy: 1.0 / self.sy,
            oy: self.oy / self.sy,
        })
    }
}

/// A drawing canvas backed by a mutable image.
///
/// All drawing operations alpha-blend the source color onto the image.
pub struct Canvas<'a> {
    image: &'a mut Image,
}

impl<'a> Canvas<'a> {
    /// Creates a new canvas drawing onto the given image.
    pub fn new(image: &'a mut Image) -> Self {
        Self { image }
    }

    /// Alpha-blends a pixel at integer coordinates.
    fn blend(&mut self, x: i32, y: i32, color: Pixel) {
        if color.a == 0 {
            return;
        }
        let w = self.image.width as i32;
        let h = self.image.height as i32;
        if x < 0 || y < 0 || x >= w || y >= h {
            return;
        }
        let dst = self.image.get(x as u32, y as u32).unwrap_or_default();
        let a = color.a as u32;
        let inv = 255 - a;
        let blended = Pixel {
            r: ((color.r as u32 * a + dst.r as u32 * inv) / 255) as u8,
            g: ((color.g as u32 * a + dst.g as u32 * inv) / 255) as u8,
            b: ((color.b as u32 * a + dst.b as u32 * inv) / 255) as u8,
            a: ((color.a as u32 * a + dst.a as u32 * inv) / 255) as u8,
        };
        self.image.set(x as u32, y as u32, blended);
    }

    /// Fills a circle centered at `(cx, cy)` with the given radius.
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Pixel) {
        if radius <= 0 {
            self.blend(cx, cy, color);
            return;
        }
        let r = radius as i64;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.blend(cx + dx as i32, cy + dy as i32, color);
                }
            }
        }
    }

    /// Draws the outline of a circle centered at `(cx, cy)`.
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Pixel) {
        if radius <= 0 {
            self.blend(cx, cy, color);
            return;
        }
        let r = radius as i64;
        let thickness = (r as f64).max(1.0);
        for dy in -r - 1..=r + 1 {
            for dx in -r - 1..=r + 1 {
                let dist = ((dx * dx + dy * dy) as f64).sqrt();
                if (dist - r as f64).abs() <= thickness / 2.0 {
                    self.blend(cx + dx as i32, cy + dy as i32, color);
                }
            }
        }
    }

    /// Draws a polyline through the given points using Bresenham's algorithm.
    pub fn draw_polyline(&mut self, points: &[(i32, i32)], color: Pixel) {
        for window in points.windows(2) {
            let (x0, y0) = window[0];
            let (x1, y1) = window[1];
            self.draw_line(x0, y0, x1, y1, color);
        }
    }

    /// Draws a line between two points using Bresenham's algorithm.
    fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Pixel) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;
        loop {
            self.blend(x, y, color);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Fills a polygon using the even-odd scanline rule.
    pub fn fill_polygon(&mut self, points: &[(i32, i32)], color: Pixel) {
        if points.len() < 3 {
            return;
        }
        let min_y = points.iter().map(|p| p.1).min().unwrap_or(0);
        let max_y = points.iter().map(|p| p.1).max().unwrap_or(0);
        let n = points.len();
        for y in min_y..=max_y {
            let mut crossings: Vec<f64> = Vec::new();
            for i in 0..n {
                let (x1, y1) = points[i];
                let (x2, y2) = points[(i + 1) % n];
                if (y1 <= y && y < y2) || (y2 <= y && y < y1) {
                    let t = (y - y1) as f64 / (y2 - y1) as f64;
                    crossings.push(x1 as f64 + t * (x2 - x1) as f64);
                }
            }
            crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for pair in crossings.chunks(2) {
                if pair.len() == 2 {
                    let start = pair[0].ceil() as i32;
                    let end = pair[1].floor() as i32;
                    for x in start..=end {
                        self.blend(x, y, color);
                    }
                }
            }
        }
    }
}

/// Encodes an image as PNG bytes.
pub fn encode_png(image: &Image) -> Result<Vec<u8>, RenderError> {
    let mut buffer = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buffer, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| RenderError::new(format!("failed to write PNG header: {e}")))?;
        writer
            .write_image_data(&image.rgba_bytes())
            .map_err(|e| RenderError::new(format!("failed to write PNG data: {e}")))?;
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rgb_is_opaque() {
        let p = Pixel::rgb(1, 2, 3);
        assert_eq!(p.r, 1);
        assert_eq!(p.g, 2);
        assert_eq!(p.b, 3);
        assert_eq!(p.a, 255);
    }

    #[test]
    fn pixel_default_is_transparent() {
        let p = Pixel::default();
        assert_eq!(p, Pixel::TRANSPARENT);
    }

    #[test]
    fn image_new_fills_background() {
        let img = Image::new(4, 3, Pixel::rgb(9, 8, 7)).unwrap();
        assert_eq!(img.width(), 4);
        assert_eq!(img.height(), 3);
        assert_eq!(img.get(0, 0), Some(Pixel::rgb(9, 8, 7)));
        assert_eq!(img.get(3, 2), Some(Pixel::rgb(9, 8, 7)));
        assert_eq!(img.get(4, 0), None);
        assert_eq!(img.get(0, 3), None);
    }

    #[test]
    fn image_new_rejects_zero_dimensions() {
        assert!(Image::new(0, 1, Pixel::default()).is_err());
        assert!(Image::new(1, 0, Pixel::default()).is_err());
    }

    #[test]
    fn image_set_and_get() {
        let mut img = Image::new(2, 2, Pixel::default()).unwrap();
        assert!(img.set(1, 1, Pixel::rgb(255, 0, 0)));
        assert_eq!(img.get(1, 1), Some(Pixel::rgb(255, 0, 0)));
        assert!(!img.set(2, 0, Pixel::default()));
    }

    #[test]
    fn transform_maps_bbox_to_pixels() {
        let t = Transform::map_to_pixel(0.0, 0.0, 100.0, 100.0, 200, 200);
        let (px, py) = t.transform_point(0.0, 0.0);
        assert!((px - 0.0).abs() < 1e-9);
        assert!((py - 200.0).abs() < 1e-9);
        let (px, py) = t.transform_point(100.0, 100.0);
        assert!((px - 200.0).abs() < 1e-9);
        assert!((py - 0.0).abs() < 1e-9);
        let (px, py) = t.transform_point(50.0, 50.0);
        assert!((px - 100.0).abs() < 1e-9);
        assert!((py - 100.0).abs() < 1e-9);
    }

    #[test]
    fn transform_inverse_round_trips() {
        let t = Transform::map_to_pixel(-180.0, -90.0, 180.0, 90.0, 800, 600);
        let inv = t.inverse().unwrap();
        let (px, py) = t.transform_point(12.5, -34.25);
        let (x, y) = inv.transform_point(px, py);
        assert!((x - 12.5).abs() < 1e-9);
        assert!((y - (-34.25)).abs() < 1e-9);
    }

    #[test]
    fn transform_inverse_none_when_singular() {
        let t = Transform::map_to_pixel(0.0, 0.0, 0.0, 0.0, 100, 100);
        assert!(t.inverse().is_none());
    }

    #[test]
    fn canvas_fill_circle_blends() {
        let mut img = Image::new(10, 10, Pixel::default()).unwrap();
        {
            let mut canvas = Canvas::new(&mut img);
            canvas.fill_circle(5, 5, 2, Pixel::new(255, 0, 0, 128));
        }
        let center = img.get(5, 5).unwrap();
        assert!(center.r > 0);
        assert!(center.a > 0);
    }

    #[test]
    fn canvas_draw_polyline() {
        let mut img = Image::new(10, 10, Pixel::default()).unwrap();
        {
            let mut canvas = Canvas::new(&mut img);
            canvas.draw_polyline(&[(0, 0), (9, 9)], Pixel::rgb(255, 255, 255));
        }
        assert_eq!(img.get(0, 0), Some(Pixel::rgb(255, 255, 255)));
        assert_eq!(img.get(9, 9), Some(Pixel::rgb(255, 255, 255)));
    }

    #[test]
    fn canvas_fill_polygon() {
        let mut img = Image::new(10, 10, Pixel::default()).unwrap();
        {
            let mut canvas = Canvas::new(&mut img);
            canvas.fill_polygon(&[(2, 2), (7, 2), (7, 7), (2, 7)], Pixel::rgb(0, 0, 255));
        }
        assert_eq!(img.get(4, 4), Some(Pixel::rgb(0, 0, 255)));
        assert_eq!(img.get(0, 0), Some(Pixel::default()));
    }

    #[test]
    fn encode_png_produces_valid_header() {
        let img = Image::new(8, 8, Pixel::rgb(255, 255, 255)).unwrap();
        let png = encode_png(&img).unwrap();
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }
}
