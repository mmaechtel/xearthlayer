//! Deterministic synthetic scenes, used when no real provider tiles are
//! available (the sandbox this was developed in blocks every tile server).
//!
//! A scene is rendered at ZL18 resolution with the five content types of
//! the benchmark. The "provider" then does what a tile pyramid does: it
//! JPEG-encodes the ZL18 scene chunk by chunk, and separately averages the
//! pristine scene 4x4 down to ZL16 and JPEG-encodes that. The benchmark
//! sees only those JPEGs, exactly as it would see a real provider's.
//!
//! This validates the pipeline and shows how the filters respond to JPEG
//! quality and content. It says nothing about any real provider.

use image::{Rgb, RgbImage};
use rayon::prelude::*;

/// The content types of the benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Content {
    Town,
    Forest,
    Farmland,
    Coast,
    Mountains,
}

impl Content {
    pub fn name(self) -> &'static str {
        match self {
            Self::Town => "town",
            Self::Forest => "forest",
            Self::Farmland => "farmland",
            Self::Coast => "coast",
            Self::Mountains => "mountains",
        }
    }
}

/// The synthetic locations: 12 scenes over the five content types.
pub const SCENES: [(&str, Content, u32); 12] = [
    ("town-a", Content::Town, 11),
    ("town-b", Content::Town, 12),
    ("town-c", Content::Town, 13),
    ("forest-a", Content::Forest, 21),
    ("forest-b", Content::Forest, 22),
    ("farmland-a", Content::Farmland, 31),
    ("farmland-b", Content::Farmland, 32),
    ("farmland-c", Content::Farmland, 33),
    ("coast-a", Content::Coast, 41),
    ("coast-b", Content::Coast, 42),
    ("mountains-a", Content::Mountains, 51),
    ("mountains-b", Content::Mountains, 52),
];

fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32)
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add((y as u32).wrapping_mul(0x85EB_CA77))
        .wrapping_add(seed.wrapping_mul(0xC2B2_AE3D));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// Uniform in 0..1 for an integer lattice point.
fn rand(x: i32, y: i32, seed: u32) -> f32 {
    (hash(x, y, seed) >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth value noise in 0..1.
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (xi, yi) = (xf as i32, yf as i32);
    let fade = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (fade(x - xf), fade(y - yf));
    let a = rand(xi, yi, seed);
    let b = rand(xi + 1, yi, seed);
    let c = rand(xi, yi + 1, seed);
    let d = rand(xi + 1, yi + 1, seed);
    (a + (b - a) * u) + ((c + (d - c) * u) - (a + (b - a) * u)) * v
}

/// Fractal noise in 0..1: `octaves` layers starting at wavelength `scale`.
fn fbm(x: f32, y: f32, scale: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut freq) = (0.0, 1.0, 0.0, 1.0 / scale);
    for o in 0..octaves {
        sum += amp * noise(x * freq, y * freq, seed.wrapping_add(o * 101));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Distance to the nearest jittered cell centre and that cell's random
/// value: tree crowns, boulders.
fn cells(x: f32, y: f32, size: f32, seed: u32) -> (f32, f32) {
    let (cx, cy) = ((x / size).floor() as i32, (y / size).floor() as i32);
    let mut best = (f32::MAX, 0.0);
    for j in -1..=1 {
        for i in -1..=1 {
            let (gx, gy) = (cx + i, cy + j);
            let px = (gx as f32 + rand(gx, gy, seed)) * size;
            let py = (gy as f32 + rand(gx, gy, seed + 1)) * size;
            let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt() / size;
            if d < best.0 {
                best = (d, rand(gx, gy, seed + 2));
            }
        }
    }
    best
}

type Colour = [f32; 3];

fn mix(a: Colour, b: Colour, t: f32) -> Colour {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn scale(c: Colour, k: f32) -> Colour {
    c.map(|v| v * k)
}

fn forest(x: f32, y: f32, seed: u32) -> Colour {
    let (d, id) = cells(x, y, 14.0, seed);
    let stand = fbm(x, y, 400.0, 3, seed + 7);
    let base = mix([34.0, 62.0, 30.0], [58.0, 92.0, 44.0], stand);
    // Crowns are lit from the north-west and dark in the gaps between.
    let light = 0.55 + 0.75 * (1.0 - d).powi(2) + 0.25 * (id - 0.5);
    let fine = 0.9 + 0.2 * noise(x / 2.5, y / 2.5, seed + 9);
    scale(base, light * fine)
}

fn farmland(x: f32, y: f32, seed: u32) -> Colour {
    let angle = 0.2 + 0.6 * rand(0, 0, seed);
    let (s, c) = angle.sin_cos();
    let (u, v) = (x * c + y * s, -x * s + y * c);
    let (fw, fh) = (520.0, 300.0);
    let (iu, iv) = ((u / fw).floor() as i32, (v / fh).floor() as i32);
    let palette: [Colour; 5] = [
        [96.0, 128.0, 60.0],
        [150.0, 132.0, 88.0],
        [70.0, 104.0, 52.0],
        [176.0, 164.0, 104.0],
        [118.0, 96.0, 70.0],
    ];
    let field = palette[(hash(iu, iv, seed) % 5) as usize];
    let tone = 0.92 + 0.16 * fbm(x, y, 260.0, 3, seed + 3);
    let rows = 1.0 + 0.035 * (v * std::f32::consts::TAU / 7.0).sin();
    let mut colour = scale(field, tone * rows);
    // Hedges between fields and one farm road.
    let (eu, ev) = (u - iu as f32 * fw, v - iv as f32 * fh);
    if eu < 4.0 || ev < 4.0 {
        colour = [44.0, 60.0, 36.0];
    }
    let road = (v - 180.0 - 0.3 * u).abs();
    if road < 9.0 {
        colour = [150.0, 146.0, 138.0];
    }
    colour
}

fn town(x: f32, y: f32, seed: u32) -> Colour {
    let block = 230.0;
    let (bx, by) = (x.rem_euclid(block), y.rem_euclid(block));
    if bx < 26.0 || by < 26.0 {
        // Asphalt with a centre line.
        let centre = (bx - 13.0).abs() < 1.0 || (by - 13.0).abs() < 1.0;
        let dashed = ((x + y) / 14.0).floor() as i32 % 2 == 0;
        return if centre && dashed {
            [210.0, 208.0, 200.0]
        } else {
            scale(
                [92.0, 92.0, 96.0],
                0.95 + 0.1 * noise(x / 3.0, y / 3.0, seed),
            )
        };
    }
    let lot = 51.0;
    let (lx, ly) = (
        ((x - 26.0) / lot).floor() as i32,
        ((y - 26.0) / lot).floor() as i32,
    );
    let (ox, oy) = ((x - 26.0).rem_euclid(lot), (y - 26.0).rem_euclid(lot));
    let inset = 5.0 + 8.0 * rand(lx, ly, seed + 1);
    let width = lot - inset - 4.0 - 8.0 * rand(lx, ly, seed + 2);
    let roofs: [Colour; 4] = [
        [150.0, 74.0, 58.0],
        [118.0, 116.0, 120.0],
        [196.0, 190.0, 180.0],
        [88.0, 64.0, 56.0],
    ];
    let inside = |px: f32, py: f32| px >= inset && px < width && py >= inset && py < width;
    if inside(ox, oy) {
        let roof = roofs[(hash(lx, ly, seed + 3) % 4) as usize];
        // Two roof faces meeting at a ridge.
        let face = if ox < (inset + width) / 2.0 {
            1.08
        } else {
            0.86
        };
        return scale(roof, face);
    }
    if inside(ox - 6.0, oy - 6.0) {
        return [30.0, 36.0, 30.0];
    }
    let garden = mix(
        [62.0, 96.0, 48.0],
        [96.0, 120.0, 66.0],
        fbm(x, y, 30.0, 3, seed + 5),
    );
    let (d, _) = cells(x, y, 22.0, seed + 6);
    scale(garden, 0.8 + 0.4 * (1.0 - d))
}

fn coast(x: f32, y: f32, seed: u32) -> Colour {
    let shore = 1800.0 + 900.0 * (fbm(x, y, 900.0, 5, seed) - 0.5) + 0.25 * (y - 2048.0);
    let depth = x - shore;
    if depth > 0.0 {
        let deep = mix([58.0, 110.0, 118.0], [20.0, 52.0, 84.0], depth / 700.0);
        let swell = 0.97 + 0.06 * fbm(x * 0.4 + y, y * 0.4, 60.0, 2, seed + 2);
        let foam = (1.0 - depth / 10.0).max(0.0) * noise(x / 6.0, y / 6.0, seed + 3);
        return mix(scale(deep, swell), [236.0, 240.0, 240.0], foam);
    }
    if depth > -45.0 {
        return scale(
            [206.0, 190.0, 150.0],
            0.92 + 0.16 * noise(x / 4.0, y / 4.0, seed + 4),
        );
    }
    let land = mix(
        [84.0, 104.0, 60.0],
        [124.0, 112.0, 80.0],
        fbm(x, y, 180.0, 4, seed + 5),
    );
    let (d, _) = cells(x, y, 18.0, seed + 6);
    scale(land, 0.78 + 0.4 * (1.0 - d))
}

fn mountains(x: f32, y: f32, seed: u32) -> Colour {
    let height = |px: f32, py: f32| {
        let ridged = 1.0 - (2.0 * fbm(px, py, 700.0, 6, seed) - 1.0).abs();
        ridged * ridged
    };
    let h = height(x, y);
    let (dx, dy) = (height(x + 2.0, y) - h, height(x, y + 2.0) - h);
    let shade = (0.75 - 60.0 * (dx + dy)).clamp(0.25, 1.35);
    let rock = mix(
        [96.0, 90.0, 84.0],
        [134.0, 126.0, 116.0],
        fbm(x, y, 40.0, 3, seed + 4),
    );
    let grit = 0.9 + 0.2 * noise(x / 2.0, y / 2.0, seed + 5);
    let snowline = 0.62 + 0.08 * (fbm(x, y, 90.0, 3, seed + 6) - 0.5);
    let colour = if h > snowline {
        [236.0, 240.0, 246.0]
    } else {
        scale(rock, grit)
    };
    scale(colour, shade)
}

/// Renders a `size` x `size` scene at ZL18 resolution.
///
/// A 1-2-1 blur in each direction stands in for the sensor's point spread,
/// so hard edges are about one pixel soft as in real imagery.
pub fn render(content: Content, seed: u32, size: u32) -> RgbImage {
    let n = size as usize;
    let mut raw = vec![[0.0f32; 3]; n * n];
    raw.par_chunks_mut(n).enumerate().for_each(|(y, row)| {
        for (x, pixel) in row.iter_mut().enumerate() {
            let (fx, fy) = (x as f32, y as f32);
            *pixel = match content {
                Content::Town => town(fx, fy, seed),
                Content::Forest => forest(fx, fy, seed),
                Content::Farmland => farmland(fx, fy, seed),
                Content::Coast => coast(fx, fy, seed),
                Content::Mountains => mountains(fx, fy, seed),
            };
        }
    });
    let at = |x: usize, y: usize| raw[y.min(n - 1) * n + x.min(n - 1)];
    RgbImage::from_fn(size, size, |x, y| {
        let (x, y) = (x as usize, y as usize);
        let mut acc = [0.0f32; 3];
        for (j, wy) in [1.0, 2.0, 1.0].iter().enumerate() {
            for (i, wx) in [1.0, 2.0, 1.0].iter().enumerate() {
                let p = at((x + i).saturating_sub(1), (y + j).saturating_sub(1));
                for c in 0..3 {
                    acc[c] += wx * wy * p[c];
                }
            }
        }
        Rgb(acc.map(|v| (v / 16.0).round().clamp(0.0, 255.0) as u8))
    })
}

/// The provider's own downsample of the pristine scene: a 4x4 average of
/// the stored sRGB values (GDAL's `average` resampling).
pub fn provider_downsample_4x(scene: &RgbImage) -> RgbImage {
    RgbImage::from_fn(scene.width() / 4, scene.height() / 4, |x, y| {
        let mut acc = [0u32; 3];
        for j in 0..4 {
            for i in 0..4 {
                let p = scene.get_pixel(x * 4 + i, y * 4 + j);
                for c in 0..3 {
                    acc[c] += u32::from(p[c]);
                }
            }
        }
        Rgb(acc.map(|v| ((v + 8) / 16) as u8))
    })
}

/// A 1-2-1 blur in each direction (sigma about 0.7 pixel): a provider whose
/// ZL16 is softer than a plain average of its ZL18, as resampling with a
/// wider kernel or imagery from a coarser capture would be.
pub fn soften(image: &RgbImage) -> RgbImage {
    let (w, h) = image.dimensions();
    RgbImage::from_fn(w, h, |x, y| {
        let mut acc = [0u32; 3];
        for (j, wy) in [1, 2, 1].iter().enumerate() {
            for (i, wx) in [1, 2, 1].iter().enumerate() {
                let sx = (x + i as u32).saturating_sub(1).min(w - 1);
                let sy = (y + j as u32).saturating_sub(1).min(h - 1);
                let p = image.get_pixel(sx, sy);
                for c in 0..3 {
                    acc[c] += wx * wy * u32::from(p[c]);
                }
            }
        }
        Rgb(acc.map(|v| ((v + 8) / 16) as u8))
    })
}

/// Baseline 4:2:0 JPEG of one 256 pixel chunk of `image`.
pub fn encode_chunk(image: &RgbImage, col: u32, row: u32, quality: u8) -> Vec<u8> {
    let chunk = image::imageops::crop_imm(image, col * 256, row * 256, 256, 256).to_image();
    let mut bytes = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, quality);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    encoder
        .encode(chunk.as_raw(), 256, 256, jpeg_encoder::ColorType::Rgb)
        .expect("in-memory JPEG encode");
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_is_deterministic() {
        for (_, content, seed) in SCENES.iter().step_by(3) {
            assert_eq!(render(*content, *seed, 64), render(*content, *seed, 64));
        }
    }

    #[test]
    fn seeds_give_different_scenes() {
        assert_ne!(
            render(Content::Forest, 1, 64),
            render(Content::Forest, 2, 64)
        );
    }

    #[test]
    fn a_chunk_encodes_at_the_requested_quality_in_420() {
        let scene = render(Content::Town, 11, 256);
        let info = crate::jpeg_info::parse(&encode_chunk(&scene, 0, 0, 75)).unwrap();
        assert_eq!(info.quality_estimate, 75);
        assert_eq!(info.subsampling, "4:2:0");
    }

    #[test]
    fn softening_keeps_flat_areas_and_spreads_an_edge() {
        let image = RgbImage::from_fn(8, 8, |x, _| Rgb([if x < 4 { 0 } else { 160 }, 9, 9]));
        let soft = soften(&image);
        assert_eq!(soft.get_pixel(1, 4).0, [0, 9, 9]);
        assert_eq!(soft.get_pixel(3, 4).0, [40, 9, 9]);
        assert_eq!(soft.get_pixel(4, 4).0, [120, 9, 9]);
    }

    #[test]
    fn provider_downsample_averages_blocks() {
        let scene = RgbImage::from_fn(8, 8, |x, _| Rgb([if x % 2 == 0 { 0 } else { 100 }, 7, 7]));
        let small = provider_downsample_4x(&scene);
        assert_eq!(small.dimensions(), (2, 2));
        assert_eq!(small.get_pixel(1, 1).0, [50, 7, 7]);
    }
}
