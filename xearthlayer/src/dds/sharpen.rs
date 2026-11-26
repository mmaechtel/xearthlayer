//! Sharpening of a texture's full-resolution level (`texture.sharpen`).
//!
//! An unsharp mask on luma: Y' is Rec. 709 on the stored sRGB values,
//! blurred with a Gaussian (σ = 1 px, radius 3), and R, G and B each get
//! `strength × (Y' − blur(Y'))`, rounded and clamped. Alpha is unchanged.
//! Streamed imagery is soft at level 0, which X-Plane shows near the ground;
//! at 0.5, measured on Apple imagery, edges gain about a third in luma
//! gradient (mmaechtel/xearthlayer#6).
//!
//! Only level 0 is sharpened, and [`MipmapStream`](super::MipmapStream)
//! builds level 1 from the unsharpened image: the mask brightens in linear
//! light more than it darkens, so levels built from a sharpened level 0
//! came out brighter in the distance.
//!
//! Every CPU gives the same bytes (see [`Sharpen::apply`]).

use image::RgbaImage;

/// Pixels at the edge of a tile that are never changed: one BC1 block. A
/// tile is sharpened on its own, so near its edge the kernel would read
/// past it. With a 4 px border every changed pixel reads only real pixels,
/// and the edge blocks of two neighbouring tiles stay exactly as they were
/// encoded without sharpening, so no seam can appear. A 3 px border is not
/// enough: BC1 picks a 4×4 block's two colours from all 16 pixels, so a
/// changed fourth column changes how the edge pixels decode.
const BORDER: usize = 4;

/// Pixels over which the strength ramps up inside the border: 1/3, 2/3,
/// then full.
const RAMP: usize = 3;

/// The Gaussian of σ = 1 px, radius 3, normalised: `exp(-i² / 2)` over
/// their sum for `i` in -3..=3.
const KERNEL: [f32; 7] = [
    0.004_433_048,
    0.054_005_582,
    0.242_036_22,
    0.399_050_3,
    0.242_036_22,
    0.054_005_582,
    0.004_433_048,
];

/// Rec. 709 luma weights on the stored values.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// How strongly level 0 is sharpened, in tenths: 0 (off) to 10 (1.0).
///
/// Held in tenths so it compares exactly and names a cache key without
/// floating point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Sharpen(u8);

impl Sharpen {
    /// No sharpening: the image as delivered.
    pub const OFF: Self = Self(0);

    /// The strength `texture.sharpen` gives, 0.0 to 1.0, rounded to the
    /// nearest tenth; outside that range it is clamped (the config checks
    /// the range on read).
    pub fn from_strength(strength: f64) -> Self {
        if strength.is_nan() {
            return Self::OFF;
        }
        Self((strength.clamp(0.0, 1.0) * 10.0).round() as u8)
    }

    /// The strength in tenths, 0 to 10.
    pub fn tenths(self) -> u8 {
        self.0
    }

    /// Whether this changes nothing.
    pub fn is_off(self) -> bool {
        self.0 == 0
    }

    /// What a sharpened tile's cache key carries after its coordinates,
    /// e.g. `s5` at 0.5; nothing when off, so an unsharpened tile keeps the
    /// key it always had.
    pub fn key_suffix(self) -> Option<String> {
        (!self.is_off()).then(|| format!("s{}", self.0))
    }

    /// Sharpens `image` in place, with the widest vector instructions this
    /// CPU has (`CpuPath::detect`). Every path gives the same bytes (see
    /// `apply_on`).
    pub fn apply(self, image: &mut RgbaImage) {
        self.apply_on(CpuPath::detect(), image);
    }

    /// `apply` on one CPU path. The paths are one body compiled
    /// for different instruction sets: the operations and their order are
    /// the same, and Rust never fuses a multiply and an add into an FMA,
    /// whatever the instruction set offers, so the bytes are the same on
    /// every path.
    fn apply_on(self, path: CpuPath, image: &mut RgbaImage) {
        match path {
            CpuPath::Baseline => self.apply_body(image),
            // SAFETY: `CpuPath::available` is the only constructor of these
            // variants and gives each only when the CPU has its features.
            #[cfg(target_arch = "x86_64")]
            CpuPath::Avx2 => unsafe { apply_avx2(self, image) },
            #[cfg(target_arch = "x86_64")]
            CpuPath::Avx512 => unsafe { apply_avx512(self, image) },
        }
    }

    /// The sharpening itself, inlined into each CPU path.
    ///
    /// The outer [`BORDER`] pixels are unchanged and the strength ramps up
    /// over the next [`RAMP`]. Works over a ring of seven rows of luma and
    /// their horizontal blur, each computed from its row before the row is
    /// changed, so no second full-size image is made. Every pass is a loop
    /// over a whole row, which the compiler vectorises on every target's
    /// baseline (f32: SSE2 has no 32-bit integer multiply) and wider where
    /// a path enables more. The order of
    /// every sum is fixed by the code and Rust never fuses or reorders
    /// float operations, so every CPU gives the same bytes.
    #[inline(always)]
    fn apply_body(self, image: &mut RgbaImage) {
        let (w, h) = (image.width() as usize, image.height() as usize);
        if self.is_off() || w <= 2 * BORDER || h <= 2 * BORDER {
            return;
        }
        let stride = 4 * w;
        let raw: &mut [u8] = image;
        let n = w - 2 * BORDER;
        let strength = f32::from(self.0) / 10.0;

        // Per ramp step of a row (1, 2, full), each changed column's
        // factor: strength × min(row ramp, column ramp) / RAMP.
        let factors: Vec<Vec<f32>> = (1..=RAMP)
            .map(|row_ramp| {
                (BORDER..w - BORDER)
                    .map(|x| strength * row_ramp.min(edge_ramp(x, w)) as f32 / RAMP as f32)
                    .collect()
            })
            .collect();

        // Per row, by row % 7: its luma (all columns) and its horizontal
        // blur over the changed columns.
        let mut lumas = vec![vec![0f32; w]; KERNEL.len()];
        let mut ring = vec![vec![0f32; n]; KERNEL.len()];
        let mut blur = vec![0f32; n];
        // Each changed pixel's change as [d, d, d, 0], added byte by byte.
        let mut change = vec![0i16; 4 * n];
        for r in BORDER - 3..BORDER + 3 {
            blur_row(
                &raw[r * stride..][..stride],
                &mut lumas[r % 7],
                &mut ring[r % 7],
            );
        }

        for y in BORDER..h - BORDER {
            let next = y + 3;
            blur_row(
                &raw[next * stride..][..stride],
                &mut lumas[next % 7],
                &mut ring[next % 7],
            );

            let rows: [&[f32]; 7] = std::array::from_fn(|k| &ring[(y - 3 + k) % 7][..n]);
            for (i, b) in blur.iter_mut().enumerate() {
                *b = convolve(|k| rows[k][i]);
            }

            let factor = &factors[edge_ramp(y, h) - 1];
            let luma = &lumas[y % 7][BORDER..];
            for (((d, &l), &b), &f) in change.chunks_exact_mut(4).zip(luma).zip(&blur).zip(factor) {
                let delta = round_to_i16(f * (l - b));
                d.copy_from_slice(&[delta, delta, delta, 0]);
            }

            let row = &mut raw[y * stride..][..stride][4 * BORDER..4 * (w - BORDER)];
            for (byte, &d) in row.iter_mut().zip(&change) {
                *byte = (i16::from(*byte) + d).clamp(0, 255) as u8;
            }
        }
    }
}

/// The instruction set [`Sharpen::apply`] is compiled for. x86-64 only
/// guarantees SSE2, four f32 at a time; with AVX2 a 4096 tile takes about
/// 14 ms instead of 24, with AVX-512 about 9 (one thread, Ryzen 7 9800X3D).
/// Most of the gain holds up to about six tiles sharpened at once (24
/// against 14 ms) and little is left at sixteen (54 against 48 ms), where
/// memory bandwidth binds. aarch64 has NEON as its baseline
/// and nothing wider to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuPath {
    Baseline,
    #[cfg(target_arch = "x86_64")]
    Avx2,
    #[cfg(target_arch = "x86_64")]
    Avx512,
}

impl CpuPath {
    /// Every path this CPU can run, the widest instruction set last.
    fn available() -> Vec<Self> {
        #[allow(unused_mut)]
        let mut paths = vec![Self::Baseline];
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                paths.push(Self::Avx2);
            }
            if is_x86_feature_detected!("avx512f")
                && is_x86_feature_detected!("avx512bw")
                && is_x86_feature_detected!("avx512dq")
                && is_x86_feature_detected!("avx512vl")
            {
                paths.push(Self::Avx512);
            }
        }
        paths
    }

    /// The path of the widest instruction set this CPU has, found once.
    fn detect() -> Self {
        static PATH: std::sync::OnceLock<CpuPath> = std::sync::OnceLock::new();
        *PATH.get_or_init(|| *Self::available().last().expect("baseline"))
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
fn apply_avx2(sharpen: Sharpen, image: &mut RgbaImage) {
    sharpen.apply_body(image);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw,avx512dq,avx512vl")]
fn apply_avx512(sharpen: Sharpen, image: &mut RgbaImage) {
    sharpen.apply_body(image);
}

/// How far up the ramp coordinate `i` of `len` is: 1, 2 or [`RAMP`] for
/// the first, second and every further pixel inside the border.
#[inline(always)]
fn edge_ramp(i: usize, len: usize) -> usize {
    (i.min(len - 1 - i) + 1 - BORDER).min(RAMP)
}

/// One row's luma into `luma`, and its horizontal blur over the changed
/// columns into `out`.
#[inline(always)]
fn blur_row(row: &[u8], luma: &mut [f32], out: &mut [f32]) {
    for (l, px) in luma.iter_mut().zip(row.chunks_exact(4)) {
        *l = LUMA[0] * f32::from(px[0]) + LUMA[1] * f32::from(px[1]) + LUMA[2] * f32::from(px[2]);
    }
    for (o, l) in out.iter_mut().zip(luma[BORDER - 3..].windows(KERNEL.len())) {
        *o = convolve(|k| l[k]);
    }
}

/// The kernel applied to seven values, summed in kernel order.
#[inline(always)]
fn convolve(value: impl Fn(usize) -> f32) -> f32 {
    let mut sum = KERNEL[0] * value(0);
    for (k, &weight) in KERNEL.iter().enumerate().skip(1) {
        sum += weight * value(k);
    }
    sum
}

/// `x` rounded to nearest, ties to even, for `|x|` well below 2^22: adding
/// 1.5 × 2^23 leaves the rounded integer in the float's low mantissa bits.
/// Unlike `round()` or an `as` cast this vectorises on SSE2.
#[inline(always)]
fn round_to_i16(x: f32) -> i16 {
    const MAGIC: f32 = 12_582_912.0;
    ((x + MAGIC).to_bits() as i32 - MAGIC.to_bits() as i32) as i16
}

impl std::fmt::Display for Sharpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.0 / 10, self.0 % 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dds::{compress_level, DdsFormat, IspcCompressor, MipmapStream};

    /// Deterministic noise from SplitMix64, opaque or with noisy alpha.
    fn noise(width: u32, height: u32, seed: u64) -> RgbaImage {
        let mut state = seed;
        let bytes = std::iter::repeat_with(move || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        })
        .flat_map(u64::to_le_bytes)
        .take(width as usize * height as usize * 4)
        .collect();
        RgbaImage::from_raw(width, height, bytes).unwrap()
    }

    /// Smooth imagery-like content: soft gradients and a few hard edges.
    fn scene(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            let edge = if (x / 9 + y / 13) % 2 == 0 { 60 } else { 0 };
            image::Rgba([
                (x * 3 % 180) as u8 + edge,
                (y * 2 % 170) as u8 + edge,
                ((x + y) % 150) as u8 + edge,
                255,
            ])
        })
    }

    /// The textbook unsharp mask in f64 with the same border ramp.
    fn reference(source: &RgbaImage, strength: f64) -> RgbaImage {
        let (w, h) = (source.width() as usize, source.height() as usize);
        let g: Vec<f64> = (-3..=3)
            .map(|i: i32| (-f64::from(i * i) / 2.0).exp())
            .collect();
        let norm: f64 = g.iter().sum();
        let luma = |x: usize, y: usize| {
            let p = source.get_pixel(x as u32, y as u32);
            0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
        };
        let mut out = source.clone();
        for y in BORDER..h - BORDER {
            for x in BORDER..w - BORDER {
                let mut blur = 0.0;
                for (j, gy) in g.iter().enumerate() {
                    for (i, gx) in g.iter().enumerate() {
                        blur += gy * gx * luma(x + i - 3, y + j - 3);
                    }
                }
                blur /= norm * norm;
                let d = y.min(h - 1 - y).min(x).min(w - 1 - x) + 1 - BORDER;
                let ramp = d.min(RAMP) as f64 / RAMP as f64;
                let delta = strength * ramp * (luma(x, y) - blur);
                let p = out.get_pixel_mut(x as u32, y as u32);
                for c in 0..3 {
                    p[c] = (f64::from(p[c]) + delta).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    }

    fn sharpened(source: &RgbaImage, tenths: u8) -> RgbaImage {
        let mut image = source.clone();
        Sharpen(tenths).apply(&mut image);
        image
    }

    #[test]
    fn strength_is_read_in_tenths_and_clamped() {
        assert_eq!(Sharpen::from_strength(0.5).tenths(), 5);
        assert_eq!(Sharpen::from_strength(0.54).tenths(), 5);
        assert_eq!(Sharpen::from_strength(0.0), Sharpen::OFF);
        assert_eq!(Sharpen::from_strength(1.0).tenths(), 10);
        assert_eq!(Sharpen::from_strength(3.0).tenths(), 10);
        assert_eq!(Sharpen::from_strength(-1.0), Sharpen::OFF);
        assert_eq!(Sharpen::from_strength(f64::NAN), Sharpen::OFF);
        assert_eq!(Sharpen(5).to_string(), "0.5");
        assert_eq!(Sharpen(10).to_string(), "1.0");
    }

    #[test]
    fn the_key_suffix_is_absent_when_off_and_names_the_tenths() {
        assert_eq!(Sharpen::OFF.key_suffix(), None);
        assert_eq!(Sharpen(5).key_suffix().as_deref(), Some("s5"));
        assert_eq!(Sharpen(10).key_suffix().as_deref(), Some("s10"));
    }

    #[test]
    fn off_leaves_the_image_byte_for_byte() {
        let source = noise(64, 64, 1);
        assert_eq!(sharpened(&source, 0), source);
    }

    #[test]
    fn a_uniform_colour_is_unchanged_at_every_strength() {
        let source = RgbaImage::from_pixel(32, 32, image::Rgba([90, 140, 60, 255]));
        for tenths in 0..=10 {
            assert_eq!(sharpened(&source, tenths), source, "strength {tenths}");
        }
    }

    #[test]
    fn a_step_edge_gets_steeper_with_overshoot_on_both_sides() {
        let source = RgbaImage::from_fn(32, 32, |x, _| {
            let v = if x < 16 { 60 } else { 180 };
            image::Rgba([v, v, v, 255])
        });
        let out = sharpened(&source, 10);
        let row = |x: u32| out.get_pixel(x, 16)[0];
        assert!(row(15) < 60, "dark side undershoots: {}", row(15));
        assert!(row(16) > 180, "bright side overshoots: {}", row(16));
        assert_eq!(row(8), 60, "far from the edge unchanged");
        assert_eq!(row(24), 180, "far from the edge unchanged");
    }

    #[test]
    fn the_result_stays_within_0_and_255() {
        let source = RgbaImage::from_fn(32, 32, |x, y| {
            let v = if (x + y) % 2 == 0 { 0 } else { 255 };
            image::Rgba([v, v, v, 255])
        });
        let out = sharpened(&source, 10);
        // Black stays black and white white: both overshoot is clamped.
        assert_eq!(out, source);
    }

    #[test]
    fn the_border_is_unchanged_and_its_bc1_blocks_too() {
        let source = noise(64, 48, 3);
        for tenths in [1, 5, 10] {
            let out = sharpened(&source, tenths);
            for y in 0..48 {
                for x in 0..64 {
                    let edge = x < 4 || y < 4 || x >= 60 || y >= 44;
                    if edge {
                        assert_eq!(out.get_pixel(x, y), source.get_pixel(x, y), "({x}, {y})");
                    }
                }
            }
            let blocks =
                |image: &RgbaImage| compress_level(&IspcCompressor, image, DdsFormat::BC1).unwrap();
            let (a, b) = (blocks(&source), blocks(&out));
            let (bw, bh) = (16, 12);
            for by in 0..bh {
                for bx in 0..bw {
                    if bx == 0 || by == 0 || bx == bw - 1 || by == bh - 1 {
                        let i = 8 * (by * bw + bx);
                        assert_eq!(a[i..i + 8], b[i..i + 8], "block ({bx}, {by})");
                    }
                }
            }
        }
    }

    #[test]
    fn alpha_is_unchanged() {
        let source = noise(40, 40, 4);
        let out = sharpened(&source, 10);
        for (a, b) in out.pixels().zip(source.pixels()) {
            assert_eq!(a[3], b[3]);
        }
    }

    #[test]
    fn a_tile_too_small_for_its_border_is_unchanged() {
        let source = noise(8, 8, 5);
        assert_eq!(sharpened(&source, 10), source);
    }

    #[test]
    fn matches_the_f64_mask_within_one_step() {
        for (source, what) in [(scene(96, 80), "scene"), (noise(48, 40, 6), "noise")] {
            for tenths in [1, 5, 10] {
                let got = sharpened(&source, tenths);
                let want = reference(&source, f64::from(tenths) / 10.0);
                for (i, (g, e)) in got.as_raw().iter().zip(want.as_raw()).enumerate() {
                    assert!(
                        g.abs_diff(*e) <= 1,
                        "{what} strength {tenths}: byte {i}: {g} against {e}"
                    );
                }
            }
        }
    }

    fn sha256(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn every_cpu_path_gives_the_baseline_bytes() {
        // Widths that leave every vector width a remainder, and one a
        // multiple of 64.
        let sources = [
            noise(96, 80, 11),
            noise(133, 57, 12),
            noise(256, 64, 13),
            scene(203, 131),
        ];
        for path in CpuPath::available() {
            for (n, source) in sources.iter().enumerate() {
                for tenths in [1, 2, 5, 10] {
                    let mut want = source.clone();
                    Sharpen(tenths).apply_on(CpuPath::Baseline, &mut want);
                    let mut got = source.clone();
                    Sharpen(tenths).apply_on(path, &mut got);
                    assert!(got == want, "{path:?}, source {n}, strength {tenths}");
                }
            }
        }
    }

    #[test]
    fn the_sharpened_bytes_are_pinned() {
        // Sharpened tiles are on disk under a key that names the strength
        // only: these bytes must not change without the key changing.
        // Pinned from the code before the CPU paths, the same on every CPU.
        for path in CpuPath::available() {
            let mut image = noise(256, 192, 21);
            Sharpen(2).apply_on(path, &mut image);
            assert_eq!(sha256(image.as_raw()), PINNED_SHA256, "{path:?}");
        }
    }

    const PINNED_SHA256: &str = "cc441e7678def541c68ec46d9c3df82b7eeaa7ffa9714d411bd5c5f00dd6b5bf";

    #[test]
    fn mipmap_stream_sharpens_level_0_only() {
        let source = scene(64, 64);
        let levels = |mut stream: MipmapStream| {
            let mut levels = Vec::new();
            while let Some(level) = stream.next_level() {
                levels.push(level.clone());
            }
            levels
        };
        let plain = levels(MipmapStream::new(source.clone(), 7));
        let sharp = levels(MipmapStream::new(source.clone(), 7).with_sharpen(Sharpen(5)));
        assert_eq!(sharp[0], sharpened(&source, 5));
        assert_ne!(sharp[0], plain[0]);
        assert_eq!(sharp[1..], plain[1..]);
    }

    /// Release-build timing of one 4096×4096 tile at 0.5, single thread.
    /// Run with `cargo test --release -p xearthlayer --lib
    /// time_sharpening_a_4096_tile -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn time_sharpening_a_4096_tile() {
        let source = scene(4096, 4096);
        let mut runs: Vec<_> = (0..11)
            .map(|_| {
                let mut image = source.clone();
                let started = std::time::Instant::now();
                Sharpen(5).apply(&mut image);
                let took = started.elapsed();
                std::hint::black_box(&image);
                took
            })
            .collect();
        runs.sort();
        println!(
            "sharpen 4096 at 0.5 ({:?}): median {:.2?}, min {:.2?}",
            CpuPath::detect(),
            runs[5],
            runs[0]
        );
    }
}
